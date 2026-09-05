//! Block household in the ordered graphical action pipeline.

use crate::game::Game;
use crate::world::TerrainRead;
use crate::audio::Sfx;
use crate::identity;
use crate::inventory::ItemStack;
use crate::net;
use crate::raycast;
use crate::world;
use crate::game::navigation::Screen;
use super::ActionFrame;

impl Game {
    pub(in crate::game) fn use_heart_block(&mut self, frame: &ActionFrame, h: &raycast::PlanetHit) -> bool {
        let reg = &frame.reg;
        let held = frame.held;

        if self.reject_guest_action() { return true; }
        self.input.action_cooldown = 0.5;
        self.input.right_held = false;
        let carried = held.and_then(|i| world::seed_nature(&reg.item(i).name));
        let holding_seed = carried.is_some();
        // A cutting from a living heart: the thing you
        // carry across the world to wake a dead country.
        if !holding_seed
            && let Some(seed) = reg.item_id(world::seed_of_form(world::heart_form(
                self.runtime.local().world.generator.biome_at(h.block.surface()),
            )))
            && self.runtime.local_mut().world.take_heart_cutting_at(h.block.surface())
        {
            let left = self.inventory.add(&reg, seed, 1);
            if left > 0 {
                self.drop_stack(ItemStack::new(&reg, seed, left));
            }
            // Taking from the wild is taking, even gently.
            self.runtime.local_mut().world.add_ire_at_surface(h.block.surface(), 1.0);
            self.toast(
                "A cutting comes away in your hand. This country will \
                 remember that you took it."
                    .to_string(),
            );
            self.sfx(Sfx::Pickup);
            return true;
        }
        if holding_seed {
            if self.inventory.slots[self.input.hotbar_sel]
                .is_some_and(|stack| stack.durability == 0)
            {
                self.toast(
                    "The cutting is still matter, but its living interval has spent itself."
                        .to_string(),
                );
                return true;
            }
            // What you carry decides what wakes: its own
            // kind reawakens, a stranger's replaces.
            let seed_stack = self.inventory.slots[self.input.hotbar_sel]
                .expect("holding_seed was derived from this authoritative slot");
            match self.runtime.local_mut().world.plant_heart_seed_stack_at(h.block, seed_stack)
            {
                Some(refusal) => self.toast(refusal),
                None => {
                    self.inventory.take_one(self.input.hotbar_sel);
                    self.toast(
                        "You plant it in the ruin of the old heart.".to_string(),
                    );
                    self.sfx(Sfx::Place);
                }
            }
            return true;
        }
        // A dead site answers with the state of its ground
        // and the work left on it. Bare-handed, at the one
        // place the player is standing when they want to
        // know: "this country is alone" alone taught
        // nothing, and a scar you cannot read is a scar you
        // walk away from.
        let world = self.runtime.view();
        if world
            .heart_at_surface(h.block.surface())
            .is_some_and(|hh| hh.stage == 0)
        {
            let hp = world.heart_at_surface(h.block.surface()).unwrap().pos;
            let (ready, total) = self.runtime.local().world.root_ground_ready_at(hp);
            let want = (total as f32 * crate::world::ROOT_READY_FRAC).ceil() as u32;
            self.toast(if ready >= want {
                "Nothing answers. The ground is living again, though. Bring it a cutting from a heart still awake."
                    .to_string()
            } else {
                format!(
                    "Nothing answers. Around it, {ready} of {want} plots are living."
                )
            });
            self.sfx(Sfx::Click);
            return true;
        }
        let line = match world.heart_at_surface(h.block.surface()) {
            // It gave already. Saying so plainly is the
            // point: the old silence read as a broken
            // button rather than a spirit with nothing left
            // to give this season.
            Some(hh) if hh.stage == 2 && hh.regrow > 0.0 => {
                "It has nothing more to give yet. Come back in a season."
            }
            Some(hh) if hh.stage == 2 && hh.strain > 4.0 => {
                "Warm to the touch, and it flinches from your hand."
            }
            Some(hh) if hh.stage == 2 => "Warm to the touch. Something here is awake.",
            Some(hh) if hh.stage == 1 => "It is cold, and it is going out.",
            Some(_) => "Nothing answers. This country is alone.",
            None => "Something stood here once.",
        };
        self.toast(line.to_string());
        self.sfx(Sfx::Click);
        true
    }
    pub(in crate::game) fn use_compost_block(&mut self, frame: &ActionFrame, h: &raycast::PlanetHit) -> bool {
        let reg = &frame.reg;
        let held = frame.held;

        if self.reject_guest_action() { return true; }
        self.input.action_cooldown = 0.3;
        // A ripened heap hands over its compost bare-handed;
        // a fresh one eats greens item by item.
        if self.runtime.local_mut().world.compost_take_at(h.block) {
            if let Some(c) = reg.item_id("base:compost") {
                let left = self.inventory.add(&reg, c, 2);
                if left > 0 {
                    self.drop_stack(ItemStack::new(&reg, c, left));
                }
            }
            self.sfx(Sfx::Pickup);
            return true;
        }
        if let Some(hi) = held {
            let name = reg.item(hi).name.clone();
            if self.runtime.local_mut().world.compost_fill_at(h.block, &name) {
                if let Some(consumed) =
                    self.inventory.take_one_stack(self.input.hotbar_sel)
                    && self.multiplayer.remote.is_none()
                {
                    if let Err(error) =
                        self.runtime.local_mut().world.record_consumed_stacks([consumed])
                    {
                        eprintln!("materials: compost feed accounting failed: {error}");
                    }
                    self.runtime.local_mut().world.retire_arcane_stack_at(
                        h.block,
                        consumed,
                        "magical biomass composted",
                    );
                }
                self.sfx(Sfx::Place);
                return true;
            }
        }
        let fill = self.runtime.view().get_meta_at(h.block);
        self.toast(if fill >= world::soil::COMPOST_FULL {
            "The heap is cooking.".to_string()
        } else {
            format!(
                "The heap wants greens ({fill}/{}).",
                world::soil::COMPOST_FULL
            )
        });
        true
    }
    pub(in crate::game) fn use_offering_block(&mut self, h: &raycast::PlanetHit) -> bool {

        self.input.action_cooldown = 0.3;
        if let Some(rc) = &self.multiplayer.remote {
            rc.session.send(&net::C2S::OpenContainer { pos: h.block });
            return true;
        }
        self.runtime.local_mut().world.ensure_block_entity_at(
            h.block,
            world::BlockEntity::Offering(Default::default()),
        );
        self.set_screen(Screen::Offering(h.block));
        true
    }
    pub(in crate::game) fn use_stall_block(&mut self, h: &raycast::PlanetHit) -> bool {

        self.input.action_cooldown = 0.3;
        self.input.right_held = false;
        if let Some(rc) = &self.multiplayer.remote {
            rc.session.send(&net::C2S::OpenContainer { pos: h.block });
            return true;
        }
        // First open claims an unowned counter for the
        // local player (the host's stall, by identity).
        let my_id = identity::local_player_id(
            &self.runtime.local().world.save_dir_for_saving(),
            self.identity.device_id(),
        )
        .map(|p| p.0)
        .unwrap_or([0; 16]);
        let my_name = self.config.display_name.clone();
        let e = self.runtime.local_mut().world.ensure_block_entity_at(
            h.block,
            world::BlockEntity::Stall(Default::default()),
        );
        if let world::BlockEntity::Stall(st) = e
            && st.owner == [0; 16]
        {
            st.owner = my_id;
            st.owner_name = my_name;
        }
        self.set_screen(Screen::Stall(h.block));
        true
    }
    pub(in crate::game) fn use_smoker_block(&mut self, frame: &ActionFrame, h: &raycast::PlanetHit) -> bool {
        let reg = &frame.reg;
        let held = frame.held;

        if self.reject_guest_action() { return true; }
        self.input.action_cooldown = 0.35;
        let raws = reg.tags.get("base:raw_meats").cloned().unwrap_or_default();
        let holding_raw = held.is_some_and(|h| raws.contains(&h));
        let e = self.runtime.local_mut().world.ensure_block_entity_at(
            h.block,
            world::BlockEntity::Smoker(Default::default()),
        );
        let world::BlockEntity::Smoker(sm) = e else {
            return true;
        };
        if holding_raw {
            if let Some(slot) = sm.meat.iter_mut().find(|s| s.is_none()) {
                let item = held.unwrap();
                if self.creative
                    || self.inventory.take_one(self.input.hotbar_sel).is_some()
                {
                    *slot = Some(ItemStack::new(&reg, item, 1));
                    self.sfx(Sfx::Place);
                    let torch_below = h.block.offset(0, -1, 0).is_some_and(|below| {
                        Some(self.runtime.view().get_block_at(below))
                            == reg.block_id("base:torch")
                    });
                    if !torch_below {
                        self.toast(
                            "The rack wants a torch burning beneath.".to_string(),
                        );
                    }
                }
            } else {
                self.toast("The rack is full.".to_string());
            }
            return true;
        }
        // Empty-handed (or otherwise): take the cuts back.
        let mut took: Option<ItemStack> = None;
        if let world::BlockEntity::Smoker(sm) =
            self.runtime.local_mut().world.block_entity_mut_at(&h.block).unwrap()
            && let Some(slot) = sm.meat.iter_mut().rev().find(|s| s.is_some())
        {
            took = slot.take();
        }
        if let Some(st) = took {
            let left = self.inventory.add_stack(&reg, st);
            if left > 0 {
                self.drop_stack(ItemStack { count: left, ..st });
            }
            self.sfx(Sfx::Pickup);
        }
        true
    }
}
