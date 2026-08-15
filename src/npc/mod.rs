//! Friendly NPC runtime (spec Part 3.1).
//!
//! Definitions live in `npcs.toml` and load as [`crate::registry::NpcDef`];
//! each def synthesizes a companion `AnimalDef` so the existing mob pipeline
//! (render/persist/network/raycast) treats the NPC as an ordinary species.
//! This module holds the *instance* state the mob has no field for: the
//! patrol walker (waypoint index + pause), the fixed-vs-patrol choice, and
//! which dialogue tree talking opens.
//!
//! NPCs are not fauna. They never flee, hunt, graze, tame, or drop; the
//! companion species' empty `biomes` keep them out of the wildlife pass.
//! The walker drives the mob's existing `Wander`/`Idle` states so the mob's
//! physics (gravity, collision, auto-step) is reused unchanged.

use crate::mobs::Mob;
use crate::planet::EntityPos;
use crate::registry::NpcDef;

/// One live NPC: a companion mob species plus the authoring fields that
/// make it an NPC rather than an animal. `mob_id` links back to the Mob
/// entity in the world (the companion species alone can't, since several
/// instances of one def may exist).
#[derive(Clone, Debug)]
pub struct NpcInstance {
    /// Index into `reg.npcs` (the authoring def).
    pub def: usize,
    /// Stable id of the companion Mob this NPC drives.
    pub mob_id: u32,
    /// The NPC's anchor: where it stands (empty patrol) or the patrol
    /// origin its waypoints offset from.
    pub anchor: EntityPos,
    /// Patrol waypoints as face-local offsets from `anchor`; empty = fixed.
    pub patrol: Vec<[f32; 3]>,
    /// Seconds to stand at each waypoint before the next leg.
    pub pause: f32,
    /// Index of the waypoint we're currently heading to.
    pub waypoint: usize,
    /// Seconds left standing at a waypoint.
    pub pause_timer: f32,
    /// The dialogue tree id this NPC enters when talked to.
    pub dialogue: Option<String>,
    /// The companion species index (`reg.npcs[def].species`).
    pub species: usize,
}

impl NpcInstance {
    /// Build an instance from a def at a spawn anchor. `mob_id` is the
    /// companion Mob's stable id (already spawned by the caller).
    pub fn new(def: &NpcDef, anchor: EntityPos, mob_id: u32) -> NpcInstance {
        NpcInstance {
            def: 0,
            mob_id,
            anchor,
            patrol: def.patrol.clone(),
            pause: def.pause,
            waypoint: 0,
            pause_timer: 0.0,
            dialogue: def.dialogue.clone(),
            species: def.species,
        }
    }

    /// Attach the def index once the caller knows it.
    pub fn with_def(mut self, def: usize) -> NpcInstance {
        self.def = def;
        self
    }

    /// Drive the companion mob's walker for one tick: fixed NPCs stand,
    /// patrol NPCs walk their loop. Reuses the mob's `Wander`/`Idle` states
    /// so all physics/collision handling is unchanged.
    pub fn tick(&mut self, mob: &mut Mob, dt: f32) {
        if self.patrol.is_empty() {
            // Fixed: idle in place, facing the anchor.
            mob.state = crate::mobs::MobState::Idle;
            mob.state_timer = mob.state_timer.max(1.0);
            return;
        }
        if self.pause_timer > 0.0 {
            // Standing at a waypoint for the authored pause.
            self.pause_timer -= dt;
            mob.state = crate::mobs::MobState::Idle;
            mob.state_timer = mob.state_timer.max(0.1);
            if self.pause_timer <= 0.0 {
                self.waypoint = (self.waypoint + 1) % self.patrol.len();
            }
            return;
        }
        let Some(waypoint) = self.waypoint_pos() else {
            mob.state = crate::mobs::MobState::Idle;
            return;
        };
        let delta = mob.pos.local_delta_to(waypoint);
        let dist = glam::Vec3::new(delta.x, 0.0, delta.z).length();
        if dist < 0.6 {
            // Arrived: start the pause.
            self.pause_timer = self.pause.max(0.0);
            mob.state = crate::mobs::MobState::Idle;
            mob.state_timer = mob.state_timer.max(0.1);
            return;
        }
        // Walk the leg.
        mob.target = waypoint;
        mob.state = crate::mobs::MobState::Wander;
        mob.state_timer = mob.state_timer.max(6.0);
    }

    fn waypoint_pos(&self) -> Option<EntityPos> {
        let wp = self.patrol.get(self.waypoint % self.patrol.len())?;
        let delta = glam::Vec3::new(wp[0], wp[1], wp[2]);
        self.anchor.translated(delta).ok().map(|c| c.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::NpcDef;

    fn def_with_patrol() -> NpcDef {
        NpcDef {
            name: "base:elder".into(),
            label: "Elder Rowan".into(),
            dialogue: Some("base:elder".into()),
            species: 0,
            talk_radius: 3.0,
            patrol: vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 0.0, 2.0]],
            pause: 1.0,
            sound_pitch: 0.85,
        }
    }

    fn anchor() -> EntityPos {
        EntityPos::new(crate::planet::Face::PosX, 10.0, 4.0, 10.0).unwrap()
    }

    fn mob_at(pos: EntityPos) -> Mob {
        let mut m = Mob::new_at(0, pos, 0.0);
        m.id = 7;
        m
    }

    #[test]
    fn fixed_npc_stays_idle() {
        let def = NpcDef {
            patrol: vec![],
            ..def_with_patrol()
        };
        let mut npc = NpcInstance::new(&def, anchor(), 7);
        let mut m = mob_at(anchor());
        npc.tick(&mut m, 0.05);
        assert_eq!(m.state, crate::mobs::MobState::Idle);
    }

    #[test]
    fn patrol_npc_walks_toward_first_waypoint() {
        let def = def_with_patrol();
        let mut npc = NpcInstance::new(&def, anchor(), 7);
        // Start offset from the anchor (whose first waypoint is the anchor
        // itself) so the walker is mid-leg and should be walking.
        let start = anchor().translated(glam::Vec3::new(1.0, 0.0, 0.0)).unwrap().pos;
        let mut m = mob_at(start);
        npc.tick(&mut m, 0.05);
        assert_eq!(m.state, crate::mobs::MobState::Wander);
        let expected = anchor()
            .translated(glam::Vec3::ZERO)
            .unwrap()
            .pos;
        assert_eq!(m.target, expected);
    }

    #[test]
    fn walker_advances_and_loops_after_pausing() {
        let def = def_with_patrol();
        let mut npc = NpcInstance::new(&def, anchor(), 7);
        let mut m = mob_at(anchor());
        // Advance through the whole patrol loop: teleport to each waypoint
        // as the walker reaches it, letting pause elapse each time.
        let mut legs = 0u32;
        let last_waypoint = def.patrol.len() - 1;
        for _ in 0..600 {
            npc.tick(&mut m, 0.05);
            if npc.waypoint == 0 && legs > 0 {
                // Back at the start after a full lap.
                break;
            }
            if m.state == crate::mobs::MobState::Wander {
                // Walked a full leg: land on the waypoint; the pause then
                // elapses naturally before the walker advances.
                m.pos = npc.waypoint_pos().unwrap();
                legs += 1;
            }
        }
        assert!(legs >= last_waypoint as u32, "walked the whole loop, got {legs} legs");
    }
}
