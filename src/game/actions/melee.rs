//! Melee in the ordered graphical action pipeline.

use crate::game::Game;
use crate::world::TerrainRead;
use crate::audio::Sfx;
use crate::net;
use crate::raycast;
use glam::Vec3;
use crate::game::combat;
use super::ActionFrame;

impl Game {
    pub(in crate::game) fn interact_melee(&mut self, frame: &ActionFrame) -> bool {
        let reg = &frame.reg;
        let hit = &frame.hit;
        let aim = &frame.aim;
        let held = frame.held;
        // Attacking: a mob in the crosshair takes the swing before the
        // block behind it. Held tools/swords set the damage. Every third
        // press inside the combo window is a heavy finisher; a hit from
        // behind the mob's facing backstabs for double.
        if self.input.left_held
            && let Some(mi) = self.mob_in_crosshair(&hit)
            && !matches!(aim, Some(raycast::TargetHit::Structure { .. }))
        {
            self.interaction.breaking = None;
            if self.input.attack_cooldown <= 0.0 {
                let heavy = self.combat.combo >= combat::HEAVY_PRESS;
                if !self.can_swing(heavy) {
                    return true;
                }
                self.combat.swing(heavy, self.swing_cost(heavy));
                self.input.attack_cooldown = if heavy {
                    combat::HEAVY_SWING_INTERVAL
                } else {
                    combat::SWING_INTERVAL
                };
                self.presentation.swing = 1.0;
                let Some(mob) = self.runtime.view().mob(mi) else {
                    return true;
                };
                let (sp, mob_id, mob_pos) = (mob.species, mob.id, mob.pos);
                let pitch = reg.animals[sp].sound_pitch;
                if let Some(r) = &self.multiplayer.remote {
                    // The host is the damage authority; it applies heavy and
                    // backstab and reports the true number back.
                    r.session.send(&net::C2S::AttackMob { id: mob_id, heavy });
                    self.runtime.present_mob_hit(mob_id);
                    if self.presentation.juice {
                        self.presentation.hitch = 0.06;
                    }
                    let at = mob_pos
                        .translated(Vec3::new(0.0, 0.5, 0.0))
                        .expect("mob hit effect stays beside the mob")
                        .pos
                        .render_pos();
                    self.presentation.burst(at, reg.animals[sp].tile, 5, 1.6);
                    self.sfx(Sfx::MobHurt(pitch));
                    self.survival.hunger = (self.survival.hunger - 0.01).max(0.0);
                    if !self.creative {
                        self.inventory.wear_tool(&reg, self.input.hotbar_sel);
                    }
                    return true;
                }
                let def = reg.animals[sp].clone();
                if let Some(mob) = self.runtime.local_mut().world.mob_mut(mi) {
                    let base = held.map(|i| reg.item(i).damage).unwrap_or(1.0);
                    let backstab = !self.creative
                        && crate::player_ops::combat::mob_facing_away(mob.yaw, mob.pos, self.player.pos);
                    let damage = crate::player_ops::combat::melee_damage(base, heavy, backstab);
                    let (dmg, crit) = (damage.amount, damage.critical);
                    let dmg_type = held.and_then(|i| reg.item(i).damage_type.clone());
                    mob.hurt(&def, dmg, dmg_type.as_deref(), self.player.eye());
                    // Heavy finishers shove: mob.hurt already knocked back
                    // along the attack line; an extra impulse sells the hit.
                    if heavy {
                        let mut dir = self.player.pos.local_delta_to(mob.pos);
                        dir.y = 0.0;
                        if dir.length_squared() > 0.001 {
                            mob.vel += dir.normalize() * 3.0;
                        }
                    }
                    let hit_pos = mob.pos;
                    self.spawn_damage_number(hit_pos, dmg, crit);
                    if self.content.scripts.wants("on_hurt") {
                        self.content.scripts.dispatch_view(
                            &self.runtime.view(),
                            "on_hurt",
                            (
                                def.name.clone(),
                                dmg as f64,
                                dmg_type.clone().unwrap_or_default(),
                            ),
                        );
                        self.apply_script_cmds();
                    }
                }
                if self.presentation.juice {
                    self.presentation.hitch = 0.06;
                }
                let at = mob_pos
                    .translated(Vec3::new(0.0, 0.5, 0.0))
                    .expect("mob hit effect stays beside the mob")
                    .pos
                    .render_pos();
                self.presentation.burst(at, def.tile, 5, 1.6);
                self.sfx(Sfx::MobHurt(pitch));
                self.survival.hunger = (self.survival.hunger - 0.01).max(0.0);
                if !self.creative {
                    self.inventory.wear_tool(&reg, self.input.hotbar_sel);
                }
            }
            return true;
        }


        false
    }
}
