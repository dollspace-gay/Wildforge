//! Fishing in the ordered graphical action pipeline.

use crate::game::Game;
use crate::world::TerrainRead;
use crate::audio::Sfx;
use crate::inventory::ItemStack;
use crate::raycast;
use super::ActionFrame;

impl Game {
    pub(in crate::game) fn interact_fishing(&mut self, frame: &ActionFrame) -> bool {
        let reg = &frame.reg;
        let hit = &frame.hit;
        let rod_held = frame.held.is_some_and(|item| frame.reg.item(item).name == "base:fishing_rod");
        // Rod clicks live outside the block-hit path: open water is
        // rarely a solid target. Strike on a bite, reel in early, or
        // cast at the first water the look-ray touches.
        if self.input.right_held && self.input.action_cooldown <= 0.0 && rod_held {
            self.input.action_cooldown = 0.45;
            self.input.right_held = false;
            match self.interaction.fishing.take() {
                Some((bobber, _, bite)) if bite > 0.0 => {
                    if self.reject_guest_action() { return true; }
                    // The strike: a real fish first, thin luck second.
                    let caught = self.runtime.local_mut().world.catch_fish_near_at(bobber, 6.0).is_some()
                        || self.rand01() < 0.25;
                    if caught {
                        if let Some(fish) = reg.item_id("base:raw_fish") {
                            let left = self.inventory.add(&reg, fish, 1);
                            if left > 0 {
                                self.drop_stack(ItemStack::new(&reg, fish, left));
                            }
                        }
                        self.inventory.wear_tool(&reg, self.input.hotbar_sel);
                        self.sfx(Sfx::Pickup);
                        self.grant_xp("fish");
                    } else {
                        self.sfx(Sfx::Splash);
                    }
                }
                Some(_) => {} // reeled in empty
                None => {
                    let cast = raycast::raycast_water_at(
                        &self.runtime.view(),
                        self.player.eye(),
                        self.camera.local_forward(),
                        14.0,
                    )
                    .filter(|hit| reg.is_water(self.runtime.view().get_block_at(hit.block)))
                    .map(|hit| hit.block.entity_at_height(0.9));
                    match cast {
                        Some(at) => {
                            self.interaction.fishing = Some((at, 3.0 + self.rand01() * 9.0, 0.0));
                            self.sfx(Sfx::Splash);
                        }
                        None => self.toast("Cast at water.".to_string()),
                    }
                }
            }
            return true;
        }

        false
    }
}
