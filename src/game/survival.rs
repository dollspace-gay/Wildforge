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
        base
    }

    /// Is a charm of this kind worn?
    pub(super) fn charm(&self, kind: &str) -> bool {
        self.survival.armor[4].as_ref().is_some_and(|s| {
            self.content.reg.item(s.item).charm.as_deref() == Some(kind)
                && s.arcane_id != 0
                && self.server.world.charm_can_pay(*s, kind)
        })
    }

    /// Damage from a warden: knockback away from the attacker, and the
    /// death screen knows who to blame. Armor blocks 4% per point (cap
    /// 60%) and wears; it does nothing against falls or hunger.
    pub(super) fn hurt_player_from_wild(&mut self, amount: f32, from: crate::planet::EntityPos) {
        if self.creative || self.ui_state.screen == Screen::Dead {
            return;
        }
        let mut pts = self.armor_points();
        if let Some(mut charm) = self.survival.armor[4]
            && let Some(pos) = self.player.pos.block()
            && self.server.world.debit_charm_at(
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
            for a in self.survival.armor.iter_mut() {
                if let Some(st) = a {
                    if reg.item(st.item).durability == 0 {
                        continue; // charms don't wear
                    }
                    st.durability = st.durability.saturating_sub(1);
                    if st.durability == 0 {
                        *a = reg.item(st.item).broken_into.map(|broken| ItemStack {
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
            if let Some(channel) = self.interaction.working.take() {
                if let Some(remote) = &self.multiplayer.remote {
                    remote.client.send(&net::C2S::OperateWorking {
                        working_id: channel.working_id,
                        held_instance: channel.wand_id,
                        target: channel.target,
                        intent: crate::workings::WorkingIntent::Cancel,
                    });
                } else if channel.stable_id != 0 {
                    let prior = self
                        .server
                        .world
                        .working_cues()
                        .into_iter()
                        .find(|cue| cue.stable_id == channel.stable_id);
                    if let Ok(result) = self.server.world.interrupt_working(channel.stable_id)
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
                    &self.server.world.save_dir_for_saving(),
                    self.identity.device_id(),
                )
                .unwrap_or(crate::identity::PlayerId([0; 16]));
                if let Err(error) = self
                    .server
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
            // finite-material sink.
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
        self.server.world.spawn_loose_item(entity);
    }

    pub(super) fn respawn(&mut self) {
        if let Some(remote) = &self.multiplayer.remote {
            remote.client.send(&net::C2S::Respawn);
        }
        // The stored spawn can be stale in both directions — built
        // over (you'd wake inside a hill) or dug out (you'd wake in
        // free fall). Settle it into a real standing spot first.
        let spawn = self.server.world.settle_spawn_at(self.survival.spawn_point);
        self.player = Player::new_at(spawn);
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
