//! Authenticated movement request adapter.

use super::{
    C2S, Guest, HOTBAR_SLOTS, HostSession, PlayerRuntime, S2C, Server, Vec3, World, refresh_held,
};
use std::sync::Arc;

use crate::planet::EntityPos;
use crate::registry::{ItemId, Registry};
use crate::stats::{StatBlock, StatKind, StatModifier};

const MOVEMENT_WINDOW: f32 = 0.3;
const ENTRY_ALLOWANCE: f32 = 3.0;
const HORIZONTAL_SPEED: f32 = 8.0;
const RISE_SPEED: f32 = 14.0;

/// A guest can spend host elapsed time and jitter tolerance only once, no
/// matter how many movement packets arrive together. Direction changes still
/// spend distance; zero-distance stance updates are free.
#[derive(Clone, Copy, Debug)]
pub(super) struct MovementBudget {
    pending_seconds: f32,
    horizontal: f32,
    upward: f32,
    downward: f32,
}

impl Default for MovementBudget {
    fn default() -> Self {
        Self {
            pending_seconds: 0.0,
            horizontal: ENTRY_ALLOWANCE,
            upward: ENTRY_ALLOWANCE,
            downward: ENTRY_ALLOWANCE,
        }
    }
}

impl MovementBudget {
    pub(super) fn advance(&mut self, dt: f32) {
        if dt.is_finite() && dt > 0.0 {
            self.pending_seconds = (self.pending_seconds + dt).min(MOVEMENT_WINDOW);
        }
    }

    /// Preview a refill. The caller adopts it only after every physical check
    /// succeeds, so refused moves do not consume or refresh any credit.
    fn refilled(mut self, horizontal_speed: f32) -> Self {
        self.horizontal = (self.horizontal + horizontal_speed * self.pending_seconds)
            .min((horizontal_speed * MOVEMENT_WINDOW + 0.35).max(ENTRY_ALLOWANCE));
        self.upward = (self.upward + RISE_SPEED * self.pending_seconds)
            .min(RISE_SPEED * MOVEMENT_WINDOW + 0.75);
        self.downward = (self.downward + crate::physics::TERMINAL * self.pending_seconds)
            .min(crate::physics::TERMINAL * MOVEMENT_WINDOW + 0.75);
        self.pending_seconds = 0.0;
        self
    }

    fn spent(mut self, delta: Vec3) -> Option<Self> {
        let horizontal = Vec3::new(delta.x, 0.0, delta.z).length();
        let upward = delta.y.max(0.0);
        let downward = (-delta.y).max(0.0);
        if horizontal > self.horizontal || upward > self.upward || downward > self.downward {
            return None;
        }
        self.horizontal -= horizontal;
        self.upward -= upward;
        self.downward -= downward;
        Some(self)
    }
}

fn add_positive_speed(block: &mut StatBlock, modifiers: impl Iterator<Item = StatModifier>) {
    block.add_all(
        modifiers
            .filter(|modifier| modifier.kind == StatKind::MoveSpeed)
            .map(|modifier| {
                StatModifier::new(
                    StatKind::MoveSpeed,
                    modifier.flat.max(0.0),
                    modifier.mult_permille.max(1_000),
                )
            }),
    );
}

pub(super) struct MovementLimits {
    registry: Arc<Registry>,
    mode: String,
    armor: [Option<(ItemId, bool)>; 5],
    stats: StatBlock,
}

impl std::fmt::Debug for MovementLimits {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MovementLimits")
            .field("content_hash", &self.registry.content_hash)
            .field("mode", &self.mode)
            .field("armor", &self.armor)
            .field("stats", &self.stats)
            .finish()
    }
}

fn horizontal_speed_limit(world: &World, guest: &mut Guest) -> f32 {
    if world.mode == "creative" {
        return crate::physics::FLY_SPEED;
    }
    let armor = guest
        .armor
        .map(|slot| slot.map(|stack| (stack.item, stack.durability == 0)));
    if guest.movement_limits.as_ref().is_none_or(|limits| {
        !Arc::ptr_eq(&limits.registry, &world.reg)
            || limits.mode != world.mode
            || limits.armor != armor
    }) {
        guest.movement_limits = Some(MovementLimits {
            registry: Arc::clone(&world.reg),
            mode: world.mode.clone(),
            armor,
            stats: content_speed_stats(world, guest),
        });
    }
    let mut stats = guest
        .movement_limits
        .as_ref()
        .map_or_else(StatBlock::default, |limits| limits.stats);
    stats.add(StatModifier::new(
        StatKind::MoveSpeed,
        0.0,
        world
            .preparation_modifiers(guest.player_id.0)
            .move_speed_permille,
    ));
    let speed = HORIZONTAL_SPEED * stats.effective(StatKind::MoveSpeed, 1.0);
    if speed.is_finite() {
        speed.max(HORIZONTAL_SPEED)
    } else {
        HORIZONTAL_SPEED
    }
}

fn content_speed_stats(world: &World, guest: &Guest) -> StatBlock {
    let ruleset = world.ruleset();
    let mut stats = StatBlock::default();
    for armor in guest.armor.iter().flatten() {
        let definition = world.reg.item(armor.item);
        if ruleset.equipment && definition.frame.is_some() && armor.durability == 0 {
            continue;
        }
        stats.add_all(definition.stats.iter().copied());
        // The current protocol carries worn frames but not their component
        // allocations. Bound the enabled content's possible positive effects
        // rather than rejecting existing client-side loadouts.
        if ruleset.equipment
            && let Some(frame) = &definition.frame
        {
            for slot in &frame.slots {
                let mut ceiling = StatBlock::default();
                for component in world
                    .reg
                    .items
                    .iter()
                    .filter(|item| item.component.as_deref() == Some(slot.slot_type.as_str()))
                {
                    add_positive_speed(&mut ceiling, component.stats.iter().copied());
                }
                for _ in 0..slot.max {
                    stats.merge(&ceiling);
                }
            }
        }
    }
    // Skill allocations likewise are not present in legacy guest profiles.
    // This content ceiling preserves enabled skill speed boosts without
    // inventing client-supplied authority or changing the wire format.
    if ruleset.skills {
        add_positive_speed(
            &mut stats,
            world
                .reg
                .skills
                .nodes
                .iter()
                .flat_map(|node| node.stats.iter().copied()),
        );
    }
    stats
}

fn validate_move(
    world: &World,
    guest: &mut Guest,
    pos: EntityPos,
    yaw: f32,
    hotbar: u8,
) -> Option<(MovementBudget, f32)> {
    if !pos.is_canonical() || !yaw.is_finite() || hotbar >= HOTBAR_SLOTS as u8 {
        return None;
    }
    let delta = guest.pos.local_delta_to(pos);
    if !guest.has_moved && delta.length() > ENTRY_ALLOWANCE {
        return None;
    }
    let horizontal_speed = horizontal_speed_limit(world, guest);
    let budget = guest
        .movement_budget
        .refilled(horizontal_speed)
        .spent(delta)?;
    // Refuse excessive travel before consulting terrain or collision. The
    // budget is still only a preview until these physical checks pass.
    let probe = crate::physics::Player::new_at(pos);
    if probe.collides(world, probe.pos) {
        return None;
    }
    let grounded = pos
        .translated(Vec3::new(0.0, -0.05, 0.0))
        .ok()
        .and_then(|p| p.pos.block())
        .is_some_and(|p| world.reg.is_solid(world.get_block_at(p)));
    let in_water = pos
        .translated(Vec3::new(0.0, 0.6, 0.0))
        .ok()
        .and_then(|p| p.pos.block())
        .is_some_and(|p| world.reg.is_water(world.get_block_at(p)));
    let airborne_rise = if grounded || in_water || world.mode == "creative" {
        0.0
    } else {
        guest.airborne_rise + delta.y.max(0.0)
    };
    (airborne_rise <= 2.4).then_some((budget, airborne_rise))
}

impl HostSession {
    pub(super) fn request_movement(&mut self, server: &mut Server, id: u32, msg: C2S) {
        let Some(guest) = self.guests.get_mut(&id) else {
            return;
        };
        match msg {
            C2S::Move {
                pos,
                yaw,
                hotbar,
                sprint,
            } => {
                let Some((budget, airborne_rise)) =
                    validate_move(&server.world, guest, pos, yaw, hotbar)
                else {
                    self.net.send(
                        id,
                        &S2C::PlayerState(PlayerRuntime::from_guest(guest).to_snap()),
                    );
                    return;
                };
                guest.movement_budget = budget;
                guest.render_from = guest.render_pos();
                guest.net_interval = guest.net_age.clamp(0.03, 0.3);
                guest.net_age = 0.0;
                guest.pos = pos;
                guest.yaw = yaw;
                guest.hotbar = hotbar as usize;
                guest.sprinting = sprint && guest.hunger >= 6.0;
                guest.airborne_rise = airborne_rise;
                guest.has_moved = true;
                refresh_held(guest);
                // Guests leave footprints too; the edit echoes to all.
                if let Some(at) = pos.block() {
                    server.world.tread_at(at);
                }
            }
            C2S::SleepRequest => {
                guest.sleeping = true;
            }
            C2S::SleepCancel => {
                guest.sleeping = false;
            }
            C2S::Respawn => {
                if guest.health > 0.0 {
                    return;
                }
                // A dungeon death (capability E10) wakes at the party's
                // checkpoint with belongings intact; the host never
                // scattered them.
                if let Some(cp) = server.world.dungeon_checkpoint_for(guest.pos) {
                    guest.pos = cp;
                    guest.health = 14.0;
                    guest.hunger = 20.0;
                    guest.since_damage = 100.0;
                    self.send_player_state(id);
                    return;
                }
                // The saved spawn may be buried or dug out by now.
                guest.pos = server.world.settle_spawn_at(guest.spawn);
                guest.health = 14.0;
                guest.hunger = 20.0;
                guest.since_damage = 100.0;
                self.send_player_state(id);
            }

            _ => {}
        }
    }
}
