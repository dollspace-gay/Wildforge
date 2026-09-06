//! Player survival, damage, death, respawn, and inventory drops.

use super::Game;
use super::MAX_AIR;
use super::combat;
use super::navigation::Screen;
use crate::audio::Sfx;
use crate::entity::ItemEntity;
use crate::inventory::ItemStack;
use crate::net;
use crate::physics::Player;
use glam::Vec3;

impl Game {
    pub(super) fn armor_points(&self) -> u32 {
        let enabled = super::equipment::equipment_enabled(self);
        let base: u32 = self
            .survival
            .armor
            .iter()
            .filter_map(|s| {
                let s = s.as_ref()?;
                let def = self.content.reg.item(s.item);
                // A modular frame disabled at 0 durability protects nothing.
                if enabled && def.frame.is_some() && s.durability == 0 {
                    return None;
                }
                def.armor.map(|(_, p)| p)
            })
            .sum();
        base
    }

    /// Is a charm of this kind worn?
    pub(super) fn charm(&self, kind: &str) -> bool {
        self.survival.armor[4].as_ref().is_some_and(|s| {
            self.content.reg.item(s.item).charm.as_deref() == Some(kind)
                && s.arcane_id != 0
                && self.runtime.view().charm_can_pay(*s, kind)
        })
    }

    /// Damage from a warden: knockback away from the attacker, and the
    /// death screen knows who to blame. Armor blocks 4% per point (cap
    /// 60%) and wears; it does nothing against falls or hunger. Per-type
    /// player armor is out of scope; the damage class is logged only.
    pub(super) fn hurt_player_from_wild(
        &mut self,
        amount: f32,
        from: crate::planet::EntityPos,
        dmg_type: Option<&str>,
    ) {
        if self.creative || self.ui_state.screen == Screen::Dead {
            return;
        }
        if std::env::var("WILDFORGE_DEBUG").is_ok() {
            eprintln!("wild hit {amount} dmg_type={dmg_type:?}");
        }
        // A raised guard takes most of the sting out of wild hits. The
        // block costs stamina; running the guard dry staggers it.
        let mut blocked = false;
        let mut amount = amount;
        if self.combat.blocking {
            blocked = true;
            amount *= 1.0 - combat::BLOCK_REDUCTION;
            self.combat.landed_block();
            self.sfx(Sfx::Block);
        }
        let mut pts = self.armor_points();
        if !self.runtime.is_guest()
            && let Some(mut charm) = self.survival.armor[4]
            && let Some(pos) = self.player.pos.block()
            && self.runtime.local_mut().world.debit_charm_at(
                pos,
                &mut charm,
                "bark",
                "bark charm prevented warden damage",
            )
        {
            self.survival.armor[4] = Some(charm);
            pts = pts.saturating_add(crate::implements::BARK_CHARM_ARMOR_POINTS);
        }
        let amount = reduced_damage(amount, pts);
        if pts > 0 {
            let reg = self.content.reg.clone();
            let equipment_enabled = super::equipment::equipment_enabled(self);
            for a in self.survival.armor.iter_mut() {
                if let Some(st) = a {
                    let def = reg.item(st.item);
                    if def.durability == 0 {
                        continue; // charms don't wear
                    }
                    let frame = equipment_enabled && def.frame.is_some();
                    // A modular frame disabled at 0 stays whole in its slot
                    // (repairable); only legacy armor changes identity.
                    if frame && st.durability == 0 {
                        continue;
                    }
                    st.durability = st.durability.saturating_sub(1);
                    if st.durability == 0 && !frame {
                        *a = def.broken_into.map(|broken| ItemStack {
                            item: broken,
                            count: 1,
                            durability: 0,
                            arcane_id: st.arcane_id,
                        });
                    }
                }
            }
        }
        let mut away = from.local_delta_to(self.player.pos);
        away.y = 0.0;
        if away.length_squared() > 0.001 {
            let dir = away.normalize();
            let (shove, lift) = if blocked {
                (
                    6.0 * combat::BLOCK_KNOCKBACK_MULT,
                    3.5 * combat::BLOCK_KNOCKBACK_MULT,
                )
            } else {
                (6.0, 3.5)
            };
            self.player.vel += dir * shove + Vec3::new(0.0, lift, 0.0);
            // The plan's one camera shake: a 2px nudge away from the
            // attacker, so the flinch points at the threat.
            if self.presentation.juice {
                self.presentation.nudge = (dir, 0.08);
            }
        }
        self.survival.killed_by_wild = true;
        self.damage(amount);
        self.survival.killed_by_wild = self.survival.health <= 0.0;
    }

    pub(super) fn damage(&mut self, amount: f32) {
        if amount <= 0.0 || self.ui_state.screen == Screen::Dead || self.creative {
            return;
        }
        // Dodge i-frames: the window makes the player untouchable, so a
        // dodge cleanly avoids a warden's lunge, a fall, or a splash.
        if self.combat.iframes > 0.0 {
            return;
        }
        if std::env::var("WILDFORGE_DEBUG").is_ok() {
            eprintln!(
                "damage {amount} at pos {:?} vel {:?} fall_start {:?} frame {}",
                self.player.pos, self.player.vel, self.survival.fall_start, self.total_frames
            );
        }
        self.survival.health -= amount;
        self.survival.damage_flash = 0.45;
        self.survival.since_damage = 0.0;
        self.sfx(Sfx::Hurt);
        if self.survival.health <= 0.0 {
            self.survival.health = 0.0;
            if let Some(channel) = self.interaction.working.take() {
                if let Some(remote) = &self.multiplayer.remote {
                    remote.session.send(&net::C2S::OperateWorking {
                        working_id: channel.working_id,
                        held_instance: channel.wand_id,
                        target: channel.target,
                        intent: crate::workings::WorkingIntent::Cancel,
                    });
                } else if channel.stable_id != 0 {
                    let prior = self
                        .runtime
                        .local()
                        .world
                        .working_cues()
                        .into_iter()
                        .find(|cue| cue.stable_id == channel.stable_id);
                    if let Ok(result) = self
                        .runtime
                        .local_mut()
                        .world
                        .interrupt_working(channel.stable_id)
                        && let Some(mut cue) = prior
                    {
                        cue.kind = result.cue;
                        cue.warning_band = result.warning_band;
                        cue.completion_permille = 1_000;
                        self.present_working_cue(cue);
                    }
                }
            }
            if self.multiplayer.remote.is_none()
                && let Some(actor_pos) = self.player.pos.block()
            {
                let actor = crate::identity::local_player_id(
                    &self.runtime.local().world.save_dir_for_saving(),
                    self.identity.device_id(),
                )
                .unwrap_or(crate::identity::PlayerId([0; 16]));
                if let Err(error) = self
                    .runtime
                    .local_mut()
                    .world
                    .settle_preparations_on_death(actor.0, actor_pos)
                {
                    eprintln!("alchemy: local death settlement failed: {error}");
                }
                self.survival.preparation_modifiers =
                    crate::alchemy::PreparationModifiers::default();
            }
            // Death: scatter every player-owned stack. The cursor and craft
            // grid are inventories too; clearing either would be an invisible
            // finite-material sink. A dungeon death (capability E10) keeps
            // everything: you respawn at the party checkpoint instead.
            if self.player.pos.face().is_deep() {
                self.set_screen(Screen::Dead);
                return;
            }
            let mut stacks = self.inventory.drain();
            stacks.extend(self.ui_state.held_stack.take());
            stacks.extend(
                self.interaction
                    .craft_grid
                    .iter_mut()
                    .filter_map(Option::take),
            );
            for s in stacks {
                self.drop_stack(s);
            }
            let worn: Vec<ItemStack> = self
                .survival
                .armor
                .iter_mut()
                .filter_map(|a| a.take())
                .collect();
            for s in worn {
                self.drop_stack(s);
            }
            let loadout_drops: Vec<ItemStack> = {
                let mut out = Vec::new();
                for loadout in self.survival.loadouts.iter_mut() {
                    while let Some(stack) = loadout.unslot(0) {
                        out.push(stack);
                    }
                }
                out
            };
            for stack in loadout_drops {
                self.drop_stack(stack);
            }
            self.set_screen(Screen::Dead);
        }
    }

    pub(super) fn drop_stack(&mut self, stack: ItemStack) {
        let a = self.rand01() * std::f32::consts::TAU;
        let v = Vec3::new(a.cos() * 2.0, 3.0 + self.rand01() * 1.5, a.sin() * 2.0);
        let pos = self
            .player
            .pos
            .translated(Vec3::new(0.0, 1.0, 0.0))
            .expect("dropped item begins beside the player")
            .pos;
        let mut entity = ItemEntity::new(pos, v, stack.item, stack.count);
        entity.durability = stack.durability;
        entity.arcane_id = stack.arcane_id;
        self.runtime.present_loose_item(entity);
    }

    pub(super) fn respawn(&mut self) {
        if let Some(remote) = &self.multiplayer.remote {
            remote.session.send(&net::C2S::Respawn);
        }
        // A dungeon death (capability E10) wakes at the party's checkpoint
        // with belongings intact. The host owns run state; a guest falls
        // through to the ordinary spawn and is corrected by the host snap.
        if let Some(cp) = self.runtime.view().dungeon_checkpoint_for(self.player.pos) {
            self.player = Player::new_at(cp);
            self.survival.health = self.max_health();
            self.survival.hunger = 20.0;
            self.survival.air = MAX_AIR;
            self.survival.fall_start = None;
            self.survival.drown_timer = 0.0;
            self.survival.since_damage = 100.0;
            self.combat = combat::CombatState::new();
            self.combat.stamina = self.stamina_max();
            self.set_screen(Screen::Playing);
            return;
        }
        // The stored spawn can be stale in both directions — built
        // over (you'd wake inside a hill) or dug out (you'd wake in
        // free fall). Settle it into a real standing spot first.
        let spawn = self.runtime.settle_spawn_at(self.survival.spawn_point);
        self.player = Player::new_at(spawn);
        self.survival.health = self.max_health();
        self.survival.hunger = 20.0;
        self.survival.air = MAX_AIR;
        self.survival.fall_start = None;
        self.survival.drown_timer = 0.0;
        self.survival.since_damage = 100.0;
        self.combat = combat::CombatState::new();
        self.combat.stamina = self.stamina_max();
        self.set_screen(Screen::Playing);
        if self.content.scripts.wants("on_player_respawn") {
            self.content
                .scripts
                .dispatch_view(&self.runtime.view(), "on_player_respawn", ());
            self.apply_script_cmds();
        }
    }
}

/// Armor: each point blocks 4% of the wild's damage, capped at 60%.
pub(crate) fn reduced_damage(amount: f32, points: u32) -> f32 {
    amount * (1.0 - (points as f32 * 0.04).min(0.6))
}
