//! Player survival, damage, death, respawn, and inventory drops.

use super::*;

impl Game {
    pub(super) fn armor_points(&self) -> u32 {
        let base: u32 = self
            .survival
            .armor
            .iter()
            .flatten()
            .filter_map(|s| self.content.reg.item(s.item).armor.map(|(_, p)| p))
            .sum();
        base + if self.charm("bark") { 1 } else { 0 }
    }

    /// Is a charm of this kind worn?
    pub(super) fn charm(&self, kind: &str) -> bool {
        self.survival.armor[4]
            .as_ref()
            .is_some_and(|s| self.content.reg.item(s.item).charm.as_deref() == Some(kind))
    }

    /// Damage from a warden: knockback away from the attacker, and the
    /// death screen knows who to blame. Armor blocks 4% per point (cap
    /// 60%) and wears; it does nothing against falls or hunger.
    pub(super) fn hurt_player_from_wild(&mut self, amount: f32, from: Vec3) {
        if self.creative || self.ui_state.screen == Screen::Dead {
            return;
        }
        let pts = self.armor_points();
        let amount = reduced_damage(amount, pts);
        if pts > 0 {
            let reg = self.content.reg.clone();
            for a in self.survival.armor.iter_mut() {
                if let Some(st) = a {
                    if reg.item(st.item).durability == 0 {
                        continue; // charms don't wear
                    }
                    st.durability = st.durability.saturating_sub(1);
                    if st.durability == 0 {
                        *a = None; // worn through
                    }
                }
            }
        }
        let mut away = self.player.pos - from;
        away.y = 0.0;
        if away.length_squared() > 0.001 {
            let dir = away.normalize();
            self.player.vel += dir * 6.0 + Vec3::new(0.0, 3.5, 0.0);
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
            // Death: scatter the inventory and worn armor as item drops.
            let stacks = self.inventory.drain();
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
            self.ui_state.held_stack = None;
            self.set_screen(Screen::Dead);
        }
    }

    pub(super) fn drop_stack(&mut self, stack: ItemStack) {
        let a = self.rand01() * std::f32::consts::TAU;
        let v = Vec3::new(a.cos() * 2.0, 3.0 + self.rand01() * 1.5, a.sin() * 2.0);
        let pos = self.player.pos + Vec3::new(0.0, 1.0, 0.0);
        self.interaction
            .items
            .push(ItemEntity::new(pos, v, stack.item, stack.count));
    }

    pub(super) fn respawn(&mut self) {
        if let Some(remote) = &self.multiplayer.remote {
            remote.client.send(&net::C2S::Respawn);
        }
        // The stored spawn can be stale in both directions — built
        // over (you'd wake inside a hill) or dug out (you'd wake in
        // free fall). Settle it into a real standing spot first.
        let spawn = self.server.world.settle_spawn(self.survival.spawn_point);
        self.player = Player::new(spawn);
        self.survival.health = self.max_health();
        self.survival.hunger = 20.0;
        self.survival.air = MAX_AIR;
        self.survival.fall_start = None;
        self.survival.drown_timer = 0.0;
        self.survival.since_damage = 100.0;
        self.set_screen(Screen::Playing);
        if self.content.scripts.wants("on_player_respawn") {
            self.content
                .scripts
                .dispatch(&self.server.world, "on_player_respawn", ());
            self.apply_script_cmds();
        }
    }
}
