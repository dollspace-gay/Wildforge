//! Authenticated terrain request adapter.

use super::{C2S, EDITS_PER_SEC, HostSession, ItemStack, REACH, Server, refresh_held};

impl HostSession {
    pub(super) fn request_terrain(&mut self, server: &mut Server, id: u32, msg: C2S) {
        let Some(guest) = self.guests.get_mut(&id) else {
            return;
        };
        match msg {
            C2S::Break { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH
                    || guest.edits >= EDITS_PER_SEC
                {
                    return;
                }
                guest.edits += 1;
                let creative = server.world.mode == "creative";
                let held = guest.inventory.slots[guest.hotbar].map(|stack| stack.item);
                let Some(mined) =
                    crate::player_ops::terrain::mine(&mut server.world, pos, held, creative)
                else {
                    return;
                };
                let result = mined.result;
                if !creative {
                    guest.hunger = (guest.hunger - 0.008).max(0.0);
                    guest.inventory.wear_tool(&server.world.reg, guest.hotbar);
                }
                refresh_held(guest);
                if let Some(stack) = result.drop {
                    server.world.queue_give(id, stack);
                }
                if !creative
                    && let Some(stack) =
                        server
                            .world
                            .roll_bonus_drop_at(pos, result.block, &mut server.rng)
                {
                    server.world.queue_give(id, stack);
                }
                self.send_player_state(id);
            }
            C2S::Scoop { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH
                    || guest.edits >= EDITS_PER_SEC
                {
                    return;
                }
                // Only a full cell fills a bucket — partials would let
                // a guest mint fluid out of films. Either fluid dips.
                let b = server.world.get_block_at(pos);
                if server.world.reg.fluid_volume(b) != Some(8) {
                    return;
                }
                let water_class = server
                    .world
                    .water_mass_at(pos)
                    .map(|mass| mass.water_class());
                let full_name = if server.world.reg.is_lava(b) {
                    "base:bucket_lava"
                } else {
                    match water_class {
                        Some(crate::planet_atlas::WaterClass::Brackish) => "base:bucket_brackish",
                        Some(crate::planet_atlas::WaterClass::Salt) => "base:bucket_salt",
                        _ => "base:bucket_water",
                    }
                };
                let Some(empty) = server.world.reg.item_id("base:bucket") else {
                    return;
                };
                if server.world.mode != "creative"
                    && guest.inventory.slots[guest.hotbar].map(|stack| stack.item) != Some(empty)
                {
                    return;
                }
                guest.edits += 1;
                if server.world.reg.is_lava(b) {
                    server.world.set_block_at(pos, crate::registry::AIR);
                } else if server.world.scoop_water_at(pos).is_none() {
                    return;
                }
                if server.world.mode != "creative"
                    && let Some(full) = server.world.reg.item_id(full_name)
                {
                    guest.inventory.slots[guest.hotbar] =
                        Some(ItemStack::new(&server.world.reg, full, 1));
                    refresh_held(guest);
                    self.send_player_state(id);
                }
            }
            C2S::Place { pos } => {
                if guest.pos.distance_to(pos.entity_center()) > REACH
                    || guest.edits >= EDITS_PER_SEC
                {
                    return;
                }
                let selected = guest.inventory.slots[guest.hotbar];
                let creative = server.world.mode == "creative";
                let Some(placement) =
                    crate::player_ops::terrain::Placement::from_stack(&server.world.reg, selected)
                else {
                    return;
                };
                let overlaps = {
                    let player = crate::physics::Player::new_at(guest.pos);
                    player.overlaps_block_at(pos)
                };
                if overlaps {
                    return;
                }
                let placed = placement.apply(&mut server.world, pos, selected, creative);
                if !placed {
                    return;
                }
                guest.edits += 1;
                if !creative {
                    if placement.is_bucket() {
                        if let Some(empty) = server.world.reg.item_id("base:bucket") {
                            guest.inventory.slots[guest.hotbar] =
                                Some(ItemStack::new(&server.world.reg, empty, 1));
                        }
                    } else {
                        guest.inventory.take_one(guest.hotbar);
                    }
                    refresh_held(guest);
                    self.send_player_state(id);
                }
            }

            _ => {}
        }
    }
}
