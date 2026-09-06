//! Portable items in the ordered graphical action pipeline.

use super::ActionFrame;
use crate::audio::Sfx;
use crate::game::Game;
use crate::inventory::ItemStack;
use crate::mobs;
use crate::net;
use crate::raycast;
use crate::registry::AIR;
use crate::world::TerrainRead;

impl Game {
    pub(in crate::game) fn interact_portable_items(&mut self, frame: &ActionFrame) -> bool {
        let reg = &frame.reg;
        let reach = frame.reach;
        let hit = &frame.hit;
        let held = frame.held;
        // The bucket: scoop a full water cell or pour it back — the
        // A boat in hand launches onto struck water.
        if held.is_some()
            && held == reg.item_id("base:boat")
            && self.input.right_held
            && self.input.action_cooldown <= 0.0
            && let Some(w) = raycast::raycast_water_at(
                &self.runtime.view(),
                self.player.eye(),
                self.camera.local_forward(),
                self.reach(),
            )
        {
            if self.reject_guest_action() {
                return true;
            }
            let pos = w.block;
            if reg.is_water(self.runtime.view().get_block_at(pos))
                && let Some(bi) = reg.animal_id("base:boat")
                && (self.creative || self.inventory.take_one(self.input.hotbar_sel).is_some())
            {
                let mut boat = mobs::Mob::new_at(
                    bi,
                    crate::planet::EntityPos::new(
                        pos.face(),
                        f32::from(pos.u()) + 0.5,
                        f32::from(pos.y()) + 0.8,
                        f32::from(pos.v()) + 0.5,
                    )
                    .expect("a launched boat is inside its water cell"),
                    self.camera.yaw,
                );
                boat.health = reg.animals[bi].health;
                boat.tamed = true; // vehicles are born ours
                self.runtime.local_mut().world.spawn_mob(boat);
                self.sfx(Sfx::Place);
                self.input.action_cooldown = 0.5;
                return true;
            }
        }
        // cell moves with you, it never multiplies. Guests request and
        // the host's echo applies the world side; the bucket swap is
        // local (inventories are player-owned).
        if held.is_some() && held == reg.item_id("base:bucket") {
            if self.input.right_held
                && self.input.action_cooldown <= 0.0
                && let Some(w) = raycast::raycast_water_at(
                    &self.runtime.view(),
                    self.player.eye(),
                    self.camera.local_forward(),
                    reach,
                )
            {
                let pos = w.block;
                let b = self.runtime.view().get_block_at(pos);
                // Either fluid fills the bucket — a full cell only.
                if reg.fluid_volume(b) == Some(8) {
                    let water_class = self
                        .runtime
                        .view()
                        .water_mass_at(pos)
                        .map(|mass| mass.water_class());
                    let full_item = if reg.is_lava(b) {
                        reg.item_id("base:bucket_lava")
                    } else {
                        reg.item_id(match water_class {
                            Some(crate::planet_atlas::WaterClass::Brackish) => {
                                "base:bucket_brackish"
                            }
                            Some(crate::planet_atlas::WaterClass::Salt) => "base:bucket_salt",
                            _ => "base:bucket_water",
                        })
                    };
                    let moved = if let Some(r) = &self.multiplayer.remote {
                        r.session.send(&net::C2S::Scoop { pos });
                        true
                    } else if reg.is_lava(b) {
                        self.runtime.local_mut().world.set_block_at(pos, AIR);
                        true
                    } else {
                        self.runtime.local_mut().world.scoop_water_at(pos).is_some()
                    };
                    if moved && let Some(full) = full_item {
                        self.inventory.slots[self.input.hotbar_sel] =
                            Some(ItemStack::new(reg, full, 1));
                    }
                    self.input.action_cooldown = 0.25;
                    self.sfx(Sfx::Splash);
                }
            }
            return true;
        }
        let held_water_class = if held == reg.item_id("base:bucket_water") {
            Some(crate::planet_atlas::WaterClass::Fresh)
        } else if held == reg.item_id("base:bucket_brackish") {
            Some(crate::planet_atlas::WaterClass::Brackish)
        } else if held == reg.item_id("base:bucket_salt") {
            Some(crate::planet_atlas::WaterClass::Salt)
        } else {
            None
        };
        if let Some(water_class) = held_water_class {
            if self.input.right_held
                && self.input.action_cooldown <= 0.0
                && let Some(h) = &hit
            {
                let pos = h.adjacent;
                if self.runtime.view().get_block_at(pos) == AIR
                    && !self.player.overlaps_block_at(pos)
                {
                    if let Some(r) = &self.multiplayer.remote {
                        r.session.send(&net::C2S::Place { pos });
                    } else {
                        crate::player_ops::terrain::Placement::Water(water_class).apply(
                            &mut self.runtime.local_mut().world,
                            pos,
                            self.inventory.slots[self.input.hotbar_sel],
                            self.creative,
                        );
                    }
                    if let Some(empty) = reg.item_id("base:bucket") {
                        self.inventory.slots[self.input.hotbar_sel] =
                            Some(ItemStack::new(reg, empty, 1));
                    }
                    self.input.action_cooldown = 0.25;
                    self.sfx(Sfx::Splash);
                }
            }
            return true;
        }
        if held.is_some() && held == reg.item_id("base:bucket_lava") {
            if self.input.right_held
                && self.input.action_cooldown <= 0.0
                && let Some(h) = &hit
            {
                let pos = h.adjacent;
                if self.runtime.view().get_block_at(pos) == AIR
                    && !self.player.overlaps_block_at(pos)
                {
                    if let Some(r) = &self.multiplayer.remote {
                        r.session.send(&net::C2S::Place { pos });
                    } else {
                        crate::player_ops::terrain::Placement::Lava(reg.lava_for_volume(8)).apply(
                            &mut self.runtime.local_mut().world,
                            pos,
                            self.inventory.slots[self.input.hotbar_sel],
                            self.creative,
                        );
                    }
                    if let Some(empty) = reg.item_id("base:bucket") {
                        self.inventory.slots[self.input.hotbar_sel] =
                            Some(ItemStack::new(reg, empty, 1));
                    }
                    self.input.action_cooldown = 0.25;
                    self.sfx(Sfx::Splash);
                }
            }
            return true;
        }

        false
    }
}
