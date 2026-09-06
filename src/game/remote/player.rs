//! Player graphical guest adapter.

use super::RemoteFlow;
use crate::audio;
use crate::audio::Sfx;
use crate::inventory::ItemStack;
use crate::net;
use crate::world;
use crate::game::Remote;

impl Game {
    pub(in crate::game) fn remote_player_message(&mut self, r: &mut Remote, message: net::S2C) -> RemoteFlow {
        match message {
                net::S2C::Hit { dmg, from } => self.hurt_player_from_wild(dmg, from, None),
                net::S2C::MobHit { id, dmg, crit } => {
                    // The host's authoritative damage for the guest's swing;
                    // float the number over the mob the snapshot still shows.
                    let at = self.runtime.view().mob_by_id(id).map(|m| m.pos);
                    if let Some(at) = at {
                        self.spawn_damage_number(at, dmg, crit);
                    }
                }
                net::S2C::Give {
                    item,
                    count,
                    durability,
                    arcane_id,
                    current_units,
                } => {
                    if let Some(local) = r.session.content().item(item) {
                        let reg = self.content.reg.clone();
                        let mut stack = ItemStack::new(&reg, local, count.max(1));
                        if durability > 0 {
                            stack.durability = durability;
                        }
                        stack.arcane_id = arcane_id;
                        if let Some((world, _)) = self.runtime.guest_mut() {
                            world.set_remote_arcane_item(arcane_id, current_units);
                        }
                        let left = self.inventory.add_stack(&reg, stack);
                        if left == 0 {
                            // Guests harvest over the wire; the ramp
                            // climbs for them too.
                            if self.presentation.juice {
                                self.presentation.pickup_streak.0 =
                                    (self.presentation.pickup_streak.0 + 1).min(24);
                                self.presentation.pickup_streak.1 = 1.5;
                                let p = audio::pickup_pitch(self.presentation.pickup_streak.0 - 1);
                                self.sfx(Sfx::Pickup2(p));
                            } else {
                                self.sfx(Sfx::Pickup);
                            }
                        }
                    }
                }
                net::S2C::PlayerState(state) => {
                    self.apply_remote_player_state(r.session.content(), state, false);
                }
                net::S2C::SettlementDelivery {
                    settlement,
                    item,
                    units,
                    rep_per_unit,
                } => {
                    // Capability E13: the host's depot accepted the goods;
                    // standing pays locally, exactly like quest rewards.
                    let rep = units * rep_per_unit;
                    let ns = self.player_namespace();
                    let rep_key = self
                        .content
                        .reg
                        .settlements
                        .iter()
                        .find(|s| s.id == settlement)
                        .map(|s| s.rep_key.clone())
                        .unwrap_or_else(|| format!("rep_{settlement}"));
                    self.content
                        .scripts
                        .kv
                        .borrow_mut()
                        .entry(ns)
                        .or_default()
                        .entry(rep_key)
                        .and_modify(|current: &mut String| {
                            *current = (current.parse::<u32>().unwrap_or(0) + rep).to_string();
                        })
                        .or_insert_with(|| rep.to_string());
                    self.toast(format!(
                        "{settlement} appreciates the {item} (+{rep} standing)."
                    ));
                }
            _ => {}
        }
        RemoteFlow::Continue
    }
}
