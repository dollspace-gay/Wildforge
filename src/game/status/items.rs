//! Items graphical status adapter.

use crate::audio;
use crate::audio::Sfx;
use crate::entity;
use crate::inventory::HOTBAR_SLOTS;
use crate::inventory::ItemStack;
use crate::world;
use glam::Vec3;
use crate::game::Game;
use crate::game::navigation::Screen;

impl Game {
    pub(in crate::game) fn update_items(&mut self, _dt: f32) {
        // Physics, lifetime, collision, and loss accounting are host-owned.
        // A guest only renders snapshots and receives authoritative Give
        // messages; it never predicts an inventory pickup.
        if self.multiplayer.remote.is_some() || self.ui_state.screen == Screen::Dead {
            return;
        }
        let mut items = self.runtime.local_mut().world.take_loose_items();
        // Pickup: magnetize into the inventory.
        let target = self
            .player
            .pos
            .translated(Vec3::new(0.0, 0.9, 0.0))
            .expect("pickup target stays beside the player")
            .pos;
        // Carry weight: an over-burdened survivor cannot lift another stack
        // off the ground. Creative ignores the ledger entirely.
        let capacity = self.carry_capacity();
        let mut weight = if self.creative {
            0.0
        } else {
            self.carried_weight()
        };
        let mut i = 0;
        while i < items.len() {
            let it = &items[i];
            let d = it.pos.distance_to(target);
            if it.age > entity::PICKUP_DELAY && d < 1.4 {
                let it_pos = it.pos;
                let (item, count, dur) = (items[i].item, items[i].count, items[i].durability);
                let reg = self.content.reg.clone();
                let unit = reg.item(item).carry_weight as f32;
                if !self.creative && weight + count as f32 * unit > capacity {
                    i += 1;
                    continue;
                }
                let left = if dur > 0 {
                    let mut stack = ItemStack::new(&reg, item, count);
                    stack.durability = dur;
                    self.inventory.add_stack(&reg, stack)
                } else {
                    self.inventory.add(&reg, item, count)
                };
                weight += (count - left) as f32 * unit;
                if left < count {
                    if !self.presentation.juice {
                        self.sfx(Sfx::Pickup);
                    } else {
                        // The collection ramp: each quick pickup chimes
                        // a step higher; the gap resets the melody.
                        self.presentation.pickup_streak.0 =
                            (self.presentation.pickup_streak.0 + 1).min(24);
                        self.presentation.pickup_streak.1 = 1.5;
                        let pitch = audio::pickup_pitch(self.presentation.pickup_streak.0 - 1);
                        self.sfx(Sfx::Pickup2(pitch));
                    }
                    if self.presentation.juice
                        && let Some(slot) = self
                            .inventory
                            .slots
                            .iter()
                            .position(|s| s.is_some_and(|s| s.item == item))
                        && slot < HOTBAR_SLOTS
                    {
                        // A ghost of the icon flies to its new home.
                        let clip = self.camera.view_proj() * it_pos.render_pos().extend(1.0);
                        if clip.w > 0.3 {
                            let w = self.renderer.config.width as f32;
                            let h = self.renderer.config.height as f32;
                            let sx = (clip.x / clip.w * 0.5 + 0.5) * w;
                            let sy = (0.5 - clip.y / clip.w * 0.5) * h;
                            let icon = self.content.reg.item(item).icon;
                            self.presentation.ui_flies.push((icon, (sx, sy), slot, 0.0));
                        }
                        self.presentation.slot_pulse[slot] = 0.18;
                    }
                }
                if left == 0 {
                    items.swap_remove(i);
                    continue;
                } else {
                    items[i].count = left;
                }
            }
            i += 1;
        }
        self.runtime.local_mut().world.replace_loose_items(items);
    }
}
