//! Block use in the ordered graphical action pipeline.

use super::ActionFrame;
use crate::audio::Sfx;
use crate::game::Game;
use crate::game::navigation::Screen;
use crate::inventory::ItemStack;
use crate::inventory::TOTAL_SLOTS;
use crate::net;
use crate::registry::AIR;
use crate::registry::ToolKind;
use crate::world;
use crate::world::TerrainRead;

impl Game {
    pub(in crate::game) fn interact_block_use(&mut self, frame: &ActionFrame) -> bool {
        let reg = &frame.reg;
        let hit = &frame.hit;
        let held = frame.held;
        let held_is_food = held.is_some_and(|i| reg.item(i).food.is_some());
        let targets_depot = hit.as_ref().is_some_and(|h| {
            reg.block(self.runtime.view().get_block_at(h.block))
                .interaction
                .as_deref()
                .is_some_and(|interaction| interaction.starts_with("depot:"))
        });
        if self.input.right_held
            && self.input.action_cooldown <= 0.0
            && (!held_is_food || targets_depot)
            && let Some(h) = &hit
        {
            let tb = self.runtime.view().get_block_at(h.block);
            // Harvestable blocks (berry bushes).
            if let Some((item, n, becomes)) = reg.block(tb).harvest {
                if self.reject_guest_action() {
                    return true;
                }
                self.runtime
                    .local_mut()
                    .world
                    .set_block_at(h.block, becomes);
                let left = self.inventory.add(reg, item, n);
                if left > 0 {
                    self.drop_stack(ItemStack::new(reg, item, left));
                }
                self.sfx(Sfx::Pickup);
                self.grant_xp("harvest");
                self.input.action_cooldown = 0.3;
                return true;
            }
            // Fertilizer feeds the field it lands on: dung from the
            // pen, guano from the cave, compost from the heap.
            if let Some(hi) = held {
                let v = world::soil::fertilizer_value(&reg.item(hi).name);
                if v > 0 && self.runtime.is_guest() && reg.block(tb).fert_tiles.is_some() {
                    self.reject_guest_action();
                    return true;
                }
                if v > 0
                    && !self.runtime.is_guest()
                    && self.runtime.local_mut().world.feed_soil_at(h.block, v)
                {
                    self.inventory.take_one(self.input.hotbar_sel);
                    self.sfx(Sfx::Place);
                    self.input.action_cooldown = 0.3;
                    return true;
                }
            }
            // The striker sets light to what you point it at. This is
            // the ONE place a fire is marked as a player's, and the
            // mark is inherited by everything it spreads to — so a
            // burn cannot change hands halfway down a hillside.
            if held.is_some_and(|i| reg.item(i).striker) {
                if self.reject_guest_action() {
                    return true;
                }
                let f = h.adjacent;
                if reg.block(tb).burns > 0 && self.runtime.local_mut().world.light_fire_at(f, true)
                {
                    self.inventory.wear_tool(reg, self.input.hotbar_sel);
                    self.toast("It catches. It is yours now.".to_string());
                    self.sfx(Sfx::Place);
                    self.input.action_cooldown = 0.4;
                    return true;
                }
                self.toast("Nothing here will take a light.".to_string());
                self.input.action_cooldown = 0.4;
                return true;
            }
            // Hoe tills grass/dirt into farmland.
            if let (Some((ToolKind::Hoe, _, _)), Some(farm)) = (
                held.and_then(|i| reg.item(i).tool),
                reg.block_id("base:farmland"),
            ) {
                let name = reg.block(tb).name.as_str();
                if name == "base:grass" || name == "base:dirt" {
                    if self.reject_guest_action() {
                        return true;
                    }
                    // The till reads the ground it came from: grass-fed
                    // loam starts richer than bare dirt (soil.rs).
                    let meta = self.runtime.local().world.till_meta_at(h.block);
                    self.runtime
                        .local_mut()
                        .world
                        .set_block_meta_at(h.block, farm, meta);
                    self.runtime
                        .local_mut()
                        .world
                        .initialize_tilled_soil_at(h.block);
                    self.inventory.wear_tool(reg, self.input.hotbar_sel);
                    self.sfx(Sfx::Place);
                    self.input.action_cooldown = 0.3;
                    return true;
                }
            }
            if self.content.scripts.wants("on_interact") {
                let name = reg.block(tb).name.clone();
                let allow = self.content.scripts.dispatch_view(
                    &self.runtime.view(),
                    "on_interact",
                    (
                        h.block.face().name().to_string(),
                        h.block.u() as i64,
                        h.block.y() as i64,
                        h.block.v() as i64,
                        name,
                    ),
                );
                self.apply_script_cmds();
                if !allow {
                    self.input.right_held = false;
                    self.input.action_cooldown = 0.22;
                    return true;
                }
            }
            // Flag-gated features (spec 2.5): right-clicking a sealed block
            // either opens it (player's KV flag met) or is refused with the
            // gate's message. Only the interact path opens a gate — mining
            // a sealed block is refused at the world level regardless.
            if let Some(gate) = self.runtime.view().gate_at(h.block) {
                let definition = self.content.reg.gates[gate].clone();
                let unlocked = self
                    .read_player_kv(&definition.flag)
                    .is_some_and(|v| v == definition.value);
                if !unlocked {
                    self.toast(definition.message.clone());
                    self.input.right_held = false;
                    self.input.action_cooldown = 0.35;
                    return true;
                }
                if let Some(unlocked_block) = definition.unlocked_block {
                    self.runtime
                        .local_mut()
                        .world
                        .set_block_at(h.block, unlocked_block);
                    self.runtime.local_mut().world.ungate_at(h.block);
                    self.sfx(Sfx::Place);
                    self.toast("The gate opens.".to_string());
                    self.input.right_held = false;
                    self.input.action_cooldown = 0.35;
                    return true;
                }
                // No unlocked_block: the gate stays but is now breakable.
                // Pass through to normal behavior below.
            }
            match reg.block(tb).interaction.as_deref() {
                Some("crafting") => {
                    if self.use_crafting_block() {
                        return true;
                    }
                }
                Some("furnace") => {
                    if self.use_furnace_block(h) {
                        return true;
                    }
                }
                Some("rail_switch") | Some("belt_switch") if self.input.action_cooldown <= 0.0 => {
                    if self.use_switch_block(h) {
                        return true;
                    }
                }
                // Capability E11: a mod screen block. Opening is pure
                // presentation, so it works identically solo and as a
                // guest; the screen's buttons carry the authority.
                // Capability E13: a settlement depot. Depositing held
                // goods the bound settlement needs pays reputation; the
                // pay-out lands in the game layer via SimEvent.
                Some(s) if s.starts_with("depot:") && self.input.action_cooldown <= 0.0 => {
                    if self.use_depot_block(frame, h) {
                        return true;
                    }
                }
                Some(s) if s.starts_with("screen:") => {
                    if self.use_mod_screen_block(frame, s) {
                        return true;
                    }
                }
                Some(s) if s.starts_with("dungeon_entry:") && self.input.action_cooldown <= 0.0 => {
                    if self.use_dungeon_entry_block(h, s) {
                        return true;
                    }
                }
                Some("dungeon_exit") if self.input.action_cooldown <= 0.0 => {
                    if self.use_dungeon_exit_block(h) {
                        return true;
                    }
                }
                Some("dungeon_checkpoint") if self.input.action_cooldown <= 0.0 => {
                    if self.use_dungeon_checkpoint_block(h) {
                        return true;
                    }
                }
                Some("chest") if self.input.action_cooldown <= 0.0 => {
                    if self.use_chest_block(h) {
                        return true;
                    }
                }
                Some("discovery_folio") if self.input.action_cooldown <= 0.0 => {
                    if self.use_folio_block(h) {
                        return true;
                    }
                }
                Some("discovery_writing") if self.input.action_cooldown <= 0.0 => {
                    if self.use_writing_block(h) {
                        return true;
                    }
                }
                Some("discovery_lab") if self.input.action_cooldown <= 0.0 => {
                    if self.use_laboratory_block(h) {
                        return true;
                    }
                }
                Some("lens_assembly") if self.input.action_cooldown <= 0.0 => {
                    if self.use_lens_block(h) {
                        return true;
                    }
                }
                Some("binding_frame") if self.input.action_cooldown <= 0.0 => {
                    if self.use_binding_frame_block(h) {
                        return true;
                    }
                }
                Some("alchemy_mortar" | "alchemy_basin" | "alchemy_alembic" | "alchemy_filter")
                    if self.input.action_cooldown <= 0.0 =>
                {
                    if self.use_alchemy_block(h) {
                        return true;
                    }
                }
                Some("heart") if self.input.action_cooldown <= 0.0 => {
                    if self.use_heart_block(frame, h) {
                        return true;
                    }
                }
                Some("compost") if self.input.action_cooldown <= 0.0 => {
                    if self.use_compost_block(frame, h) {
                        return true;
                    }
                }
                Some("offering") if self.input.action_cooldown <= 0.0 => {
                    if self.use_offering_block(h) {
                        return true;
                    }
                }
                Some("stall") if self.input.action_cooldown <= 0.0 => {
                    if self.use_stall_block(h) {
                        return true;
                    }
                }
                Some("smoker") if self.input.action_cooldown <= 0.0 => {
                    if self.use_smoker_block(frame, h) {
                        return true;
                    }
                }
                Some("sign") if self.input.action_cooldown <= 0.0 => {
                    if self.use_sign_block(h) {
                        return true;
                    }
                }
                Some("waystone") if self.input.action_cooldown <= 0.0 => {
                    if self.use_waystone_block(h) {
                        return true;
                    }
                }
                Some("survey") if self.input.action_cooldown <= 0.0 => {
                    if self.use_survey_block(h) {
                        return true;
                    }
                }
                Some(
                    st @ ("anvil" | "quern" | "millstone" | "sawmill" | "lathe" | "iron_lathe"
                    | "boring"),
                ) if self.input.action_cooldown <= 0.0 => {
                    if self.use_worked_station_block(frame, h, st) {
                        return true;
                    }
                }
                Some(interaction)
                    if self.input.action_cooldown <= 0.0
                        && reg.machine_by_interaction(interaction).is_some_and(|kind| {
                            reg.machine(kind).is_some_and(|def| def.handler.hand_fed())
                        }) =>
                {
                    if self.use_separator_block(frame, h, interaction) {
                        return true;
                    }
                }
                Some("firebox") if self.input.action_cooldown <= 0.0 => {
                    if self.use_firebox_block(frame, h) {
                        return true;
                    }
                }
                Some(interaction)
                    if self.input.action_cooldown <= 0.0
                        && reg.machine_by_interaction(interaction).is_some_and(|kind| {
                            reg.machine(kind).is_some_and(|def| def.handler.has_fire())
                        }) =>
                {
                    if self.use_fire_station_block(frame, h, interaction) {
                        return true;
                    }
                }
                Some(interaction)
                    if reg.machine_by_interaction(interaction).is_some_and(|kind| {
                        reg.machine(kind)
                            .is_some_and(|def| def.handler.is_station())
                    }) && self.use_recipe_station_block(frame, h, interaction) =>
                {
                    return true;
                }
                _ => {}
            }
            // Slot-module swap (spec Part 1.3): right-click an installed
            // module while holding a replacement from its category. The
            // host applies the swap (the world's 2c hook re-folds the
            // frame); guests see it through the host's echo.
            if self.multiplayer.remote.is_none()
                && let Some(category) = self.runtime.view().slot_category_at(h.block)
                && let Some(replacement) = held.and_then(|i| reg.item(i).places)
                && self.runtime.view().get_block_at(h.block) != replacement
                && crate::world::multiblock::modules_in_category(reg, category)
                    .contains(&replacement)
            {
                if let Ok(()) = self.runtime.local_mut().world.swap_slot_module_at(
                    h.block,
                    category,
                    replacement,
                ) {
                    if !self.creative {
                        self.inventory.take_one(self.input.hotbar_sel);
                    }
                    self.sfx(Sfx::Place);
                }
                self.input.action_cooldown = 0.3;
                return true;
            }
            let pos = h.adjacent;
            let place =
                self.inventory.slots[self.input.hotbar_sel].and_then(|s| reg.item(s.item).places);
            if let Some(block) = place {
                let bd = reg.block(block);
                // A survey cairn is raised with a prospector's strike:
                // the pick must be in the pack, and it wears.
                if Some(block) == reg.block_id("base:survey_cairn") && !self.creative {
                    let pick = reg.item_id("base:prospect_pick");
                    let slot = (0..TOTAL_SLOTS)
                        .find(|&i| self.inventory.slots[i].map(|s| Some(s.item)) == Some(pick));
                    let Some(slot) = slot else {
                        self.toast("Raising a cairn takes a prospector's pick.".to_string());
                        self.input.action_cooldown = 0.4;
                        return true;
                    };
                    self.inventory.wear_tool(reg, slot);
                }
                let needs_farmland = bd.crop_next.is_some() && !bd.crop_any_soil;
                let soil = pos
                    .offset(0, -1, 0)
                    .map_or(AIR, |below| self.runtime.view().get_block_at(below));
                if needs_farmland && Some(soil) != reg.block_id("base:farmland") {
                    return true;
                }
                if needs_farmland
                    && let Some(below) = pos.offset(0, -1, 0)
                    && let Some(reason) = self.runtime.view().soil_failure_at(below)
                {
                    self.toast(reason.to_string());
                }
                // Cross blocks (torches, plants) need solid ground.
                if bd.cross && !reg.is_solid(soil) {
                    return true;
                }
                // The cell must be one a block can take (air, fluid,
                // a thin layer) — checking merely "not solid" let a
                // click through into water or a crop, where the item
                // was spent and place_block then refused it.
                if reg.is_replaceable(self.runtime.view().get_block_at(pos))
                    && !self.player.overlaps_block_at(pos)
                {
                    let allow = if self.content.scripts.wants("on_block_place") {
                        let name = reg.block(block).name.clone();
                        let ok = self.content.scripts.dispatch_view(
                            &self.runtime.view(),
                            "on_block_place",
                            (
                                pos.face().name().to_string(),
                                pos.u() as i64,
                                pos.y() as i64,
                                pos.v() as i64,
                                name,
                            ),
                        );
                        self.apply_script_cmds();
                        ok
                    } else {
                        true
                    };
                    if !allow {
                        return true;
                    }
                    // Guests predict and let the host's echo correct
                    // them; the host places FIRST and only spends the
                    // item if the world actually took it.
                    if self.multiplayer.remote.is_some() {
                        if self.creative || self.inventory.take_one(self.input.hotbar_sel).is_some()
                        {
                            if let Some(r) = &self.multiplayer.remote {
                                r.session.send(&net::C2S::Place { pos });
                            }
                            self.input.action_cooldown = 0.22;
                            self.sfx(Sfx::Place);
                            self.grant_xp("build");
                        }
                        return true;
                    }
                    if self.inventory.slots[self.input.hotbar_sel].is_none() && !self.creative {
                        return true;
                    }
                    let placed = crate::player_ops::terrain::Placement::Block(block).apply(
                        &mut self.runtime.local_mut().world,
                        pos,
                        self.inventory.slots[self.input.hotbar_sel],
                        self.creative,
                    );
                    if placed {
                        if !self.creative {
                            self.inventory.take_one(self.input.hotbar_sel);
                        }
                        // A fresh sign or waystone wants its words.
                        if matches!(bd.interaction.as_deref(), Some("sign") | Some("waystone")) {
                            self.ui_state.sign_lines = Default::default();
                            self.ui_state.sign_line = 0;
                            self.set_screen(Screen::SignEdit(pos));
                        }
                        if bd.crop_next.is_some() {
                            // The wild notices things growing where you walk.
                            self.runtime
                                .local_mut()
                                .world
                                .plant_ire_at_surface(pos.surface(), 0.2);
                        }
                        self.input.action_cooldown = 0.22;
                        self.sfx(Sfx::Place);
                    }
                }
            }
        }

        false
    }
}
