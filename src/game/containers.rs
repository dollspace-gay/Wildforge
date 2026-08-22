//! Inventory, crafting, armor, and machine-container interactions.

use super::*;
use crate::registry::RecipeDef;

impl Game {
    pub(super) fn slot_get(&self, craft: bool, i: usize) -> Option<ItemStack> {
        if craft {
            self.interaction.craft_grid[i]
        } else {
            self.inventory.slots[i]
        }
    }

    pub(super) fn slot_set(&mut self, craft: bool, i: usize, v: Option<ItemStack>) {
        if craft {
            self.interaction.craft_grid[i] = v;
        } else {
            self.inventory.slots[i] = v;
        }
    }

    pub(super) fn inventory_click(&mut self, craft: bool, slot: usize, right: bool) {
        if let Some(remote) = &self.multiplayer.remote {
            remote.client.send(&net::C2S::InventoryClick {
                area: if craft {
                    net::InventoryArea::Craft
                } else {
                    net::InventoryArea::Inventory
                },
                slot: slot as u8,
                right,
            });
        }
        let cur = self.slot_get(craft, slot);
        let (new_slot, new_held) =
            inventory::click_stack(&self.content.reg, cur, self.ui_state.held_stack, right);
        self.slot_set(craft, slot, new_slot);
        self.ui_state.held_stack = new_held;
    }

    /// Click the craft result slot: take the output, consume ingredients.
    pub(super) fn result_click(&mut self) {
        let reg = self.content.reg.clone();
        let n2 = self.interaction.craft_size * self.interaction.craft_size;
        let recipe = crafting::match_recipe(
            &reg,
            &self.interaction.craft_grid[..n2],
            self.interaction.craft_size,
        );
        // Spec 3.5 gate: a recipe locked by its tech flag or missing blueprint
        // item is refused before the multiplayer request leaves the client (the
        // KV lives client-side; the host re-enforces the blueprint gate).
        if let Some(r) = recipe
            && self.recipe_locked(r)
        {
            return;
        }
        if let Some(remote) = &self.multiplayer.remote {
            remote.client.send(&net::C2S::CraftResult {
                size: self.interaction.craft_size as u8,
            });
        }
        let reg = self.content.reg.clone();
        let n2 = self.interaction.craft_size * self.interaction.craft_size;
        if let Some(repair) = crafting::match_repair(&reg, &self.interaction.craft_grid[..n2]) {
            if self.ui_state.held_stack.is_some() {
                return;
            }
            let consumed_part = self.interaction.craft_grid[repair.part_slot];
            self.ui_state.held_stack = Some(repair.output);
            crafting::consume_repair(&mut self.interaction.craft_grid[..n2], &repair);
            if self.multiplayer.remote.is_none()
                && let Some(stack) = consumed_part
                && stack.arcane_id != 0
                && let Some(pos) = self.player.pos.block()
            {
                self.server.world.retire_arcane_stack_at(
                    pos,
                    ItemStack { count: 1, ..stack },
                    "charged repair part consumed",
                );
            }
            if let Some(ledger) = &mut self.server.world.material_ledger
                && let Err(error) = ledger.record_recipe_loss(&repair.scale_loss)
            {
                eprintln!("materials: repair scale accounting failed: {error}");
            }
            self.sfx(Sfx::Craft);
            return;
        }
        let Some(recipe) = crafting::match_recipe(
            &reg,
            &self.interaction.craft_grid[..n2],
            self.interaction.craft_size,
        ) else {
            return;
        };
        let out = ItemStack::new(&reg, recipe.output, recipe.count);
        let recipe_loss = recipe.loss.clone();
        let recipe_byproducts = recipe.byproducts.clone();
        let charged_inputs = self.interaction.craft_grid[..n2]
            .iter()
            .flatten()
            .filter(|stack| stack.arcane_id != 0)
            .map(|stack| ItemStack { count: 1, ..*stack })
            .collect::<Vec<_>>();
        match self.ui_state.held_stack {
            None => {
                self.ui_state.held_stack = Some(out);
            }
            Some(h)
                if h.can_merge(&reg, &out) && h.count + out.count <= reg.item(h.item).max_stack =>
            {
                self.ui_state.held_stack = Some(ItemStack {
                    count: h.count + out.count,
                    ..h
                });
            }
            _ => return, // held stack can't take the output
        }
        crafting::consume(&mut self.interaction.craft_grid[..n2]);
        // Spec 3.5: a blueprint item is consumed from the inventory, not the grid.
        if let Some(blueprint) = recipe.blueprint {
            self.inventory.try_consume(&[(blueprint, 1)]);
        }
        if self.multiplayer.remote.is_none()
            && let Some(pos) = self.player.pos.block()
        {
            for stack in charged_inputs {
                self.server.world.retire_arcane_stack_at(
                    pos,
                    stack,
                    "charged crafting ingredient consumed",
                );
            }
        }
        if let Some(ledger) = &mut self.server.world.material_ledger
            && let Err(error) = ledger.record_recipe_loss(&recipe_loss)
        {
            eprintln!("materials: crafting loss accounting failed: {error}");
        }
        for (item, count) in recipe_byproducts {
            if crate::materials::is_secondary_item(&reg, item)
                && let Some(ledger) = &mut self.server.world.material_ledger
            {
                let materials =
                    crate::materials::stack_materials(&reg, ItemStack::new(&reg, item, count));
                if let Err(error) = ledger.record_secondary_output(&materials) {
                    eprintln!("materials: crafting secondary output failed: {error}");
                }
            }
            let remainder = self.inventory.add(&reg, item, count);
            if remainder != 0 {
                let pos = self.player.pos.block();
                if let (Some(pos), Some(ledger)) = (pos, &mut self.server.world.material_ledger)
                    && let Err(error) = ledger.bury_stack(
                        &reg,
                        pos,
                        ItemStack::new(&reg, item, remainder),
                        "full inventory after crafting",
                    )
                {
                    eprintln!("materials: crafting byproduct salvage failed: {error}");
                }
            }
        }
        self.sfx(Sfx::Craft);
        self.grant_xp("craft");
        if self.content.scripts.wants("on_craft") {
            let name = reg.item(recipe.output).name.clone();
            self.content
                .scripts
                .dispatch(&self.server.world, "on_craft", (name,));
            self.apply_script_cmds();
        }
    }

    /// Spec 3.5 gate: a recipe is locked when its tech KV key reads falsy or
    /// when the player lacks the required blueprint item.
    pub(super) fn recipe_locked(&self, recipe: &RecipeDef) -> bool {
        let tech_value = recipe
            .tech
            .as_deref()
            .and_then(|key| self.read_player_kv(key));
        recipe_gates_met(tech_value.as_deref(), &self.inventory, recipe).is_some()
    }

    /// Furnace slot rects: 0 input, 1 fuel, 2 output (centered panel).
    /// Bloomery slots: 0-3 charge (top row), 4-7 fuel (bottom row).
    pub(super) fn bloomery_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        let (col, row) = (i % 4, i / 4);
        (
            w / 2.0 - 2.0 * (Self::SLOT + 10.0) + col as f32 * (Self::SLOT + 10.0) + 5.0,
            h / 2.0 - 250.0 + row as f32 * (Self::SLOT + 34.0),
            Self::SLOT,
            Self::SLOT,
        )
    }

    /// Mob pack slots: 12 in two rows of six.
    pub(super) fn mob_cargo_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        let (col, row) = (i % 6, i / 6);
        (
            w / 2.0 - 3.0 * (Self::SLOT + 10.0) + col as f32 * (Self::SLOT + 10.0) + 5.0,
            h / 2.0 - 250.0 + row as f32 * (Self::SLOT + 10.0),
            Self::SLOT,
            Self::SLOT,
        )
    }

    /// One click in a mob's pack: local worlds mutate directly;
    /// guests send the click and predict nothing (the host echoes).
    pub(super) fn mob_cargo_click(&mut self, mob_id: u32, slot: usize, right: bool) {
        if slot >= 12 {
            return;
        }
        let reg = self.content.reg.clone();
        if let Some(rc) = &self.multiplayer.remote {
            rc.client.send(&net::C2S::MobCargoClick {
                id: mob_id,
                slot: slot as u8,
                right,
            });
            return;
        }
        let held = self.ui_state.held_stack;
        let Some(mob) = self.server.world.mob_by_id_mut(mob_id) else {
            return;
        };
        let Some(cargo) = mob.cargo.as_mut() else {
            return;
        };
        let (ns, nh) = inventory::click_stack(&reg, cargo[slot], held, right);
        cargo[slot] = ns;
        self.ui_state.held_stack = nh;
    }

    /// Stall layout: goods 0-5 (top row), price 6 (left mid), till
    /// 7-12 (bottom row); the BUY button sits mid-right.
    pub(super) fn stall_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        let (col, row) = match i {
            0..=5 => (i, 0),
            6 => (0, 1),
            _ => (i - 7, 2),
        };
        (
            w / 2.0 - 3.0 * (Self::SLOT + 10.0) + col as f32 * (Self::SLOT + 10.0) + 5.0,
            h / 2.0 - 260.0 + row as f32 * (Self::SLOT + 34.0),
            Self::SLOT,
            Self::SLOT,
        )
    }

    pub(super) fn stall_buy_rect(&self) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (
            w / 2.0 + (Self::SLOT + 10.0) + 20.0,
            h / 2.0 - 260.0 + (Self::SLOT + 34.0),
            110.0,
            36.0,
        )
    }

    /// Whether the local player owns this stall (local worlds/hosts).
    pub(super) fn stall_is_mine(&self, pos: crate::planet::BlockPos) -> bool {
        let my_id = identity::local_player_id(
            &self.server.world.save_dir_for_saving(),
            self.identity.device_id(),
        )
        .map(|p| p.0)
        .unwrap_or([0; 16]);
        match self.server.world.block_entity_at(&pos) {
            Some(world::BlockEntity::Stall(st)) => st.owner == my_id,
            _ => false,
        }
    }

    pub(super) fn stall_click(&mut self, pos: crate::planet::BlockPos, slot: usize, right: bool) {
        if slot > 12 {
            return;
        }
        let reg = self.content.reg.clone();
        if let Some(rc) = &self.multiplayer.remote {
            rc.client.send(&net::C2S::ContainerClick {
                pos,
                slot: slot as u8,
                right,
            });
            return;
        }
        if !self.stall_is_mine(pos) {
            return; // visitors browse; the BUY button is theirs
        }
        let held = self.ui_state.held_stack;
        let Some(world::BlockEntity::Stall(st)) = self.server.world.block_entity_mut_at(&pos)
        else {
            return;
        };
        let sref = match slot {
            0..=5 => &mut st.goods[slot],
            6 => &mut st.price,
            _ => &mut st.till[slot - 7],
        };
        let (ns, nh) = inventory::click_stack(&reg, *sref, held, right);
        *sref = ns;
        self.ui_state.held_stack = nh;
    }

    /// Local purchase: the singleplayer/host mirror of C2S::StallBuy.
    pub(super) fn stall_buy_local(&mut self, pos: crate::planet::BlockPos) {
        let reg = self.content.reg.clone();
        if !self.server.world.check_stall_at(pos) {
            self.toast("The stall wants its posts and awning.".to_string());
            return;
        }
        let result: Option<ItemStack>;
        let mut price_used: Option<ItemStack> = None;
        {
            let have = |inv: &crate::inventory::Inventory, item, n| {
                inv.slots
                    .iter()
                    .flatten()
                    .filter(|s| s.item == item)
                    .map(|s| s.count)
                    .sum::<u32>()
                    >= n
            };
            let Some(world::BlockEntity::Stall(st)) = self.server.world.block_entity_mut_at(&pos)
            else {
                return;
            };
            let Some(price) = st.price else {
                return;
            };
            if !have(&self.inventory, price.item, price.count) {
                return;
            }
            let Some(gs) = st.goods.iter_mut().find(|s| s.is_some()) else {
                return;
            };
            let fits = st.till.iter().any(|t| match t {
                None => true,
                Some(t) => {
                    t.item == price.item && t.count + price.count <= reg.item(t.item).max_stack
                }
            });
            if !fits {
                return;
            }
            let mut g = gs.take().unwrap();
            let sold = ItemStack { count: 1, ..g };
            g.count -= 1;
            if g.count > 0 {
                *gs = Some(g);
            }
            for t in st.till.iter_mut() {
                match t {
                    Some(ts)
                        if ts.item == price.item
                            && ts.count + price.count <= reg.item(ts.item).max_stack =>
                    {
                        ts.count += price.count;
                        price_used = Some(price);
                        break;
                    }
                    None => {
                        *t = Some(price);
                        price_used = Some(price);
                        break;
                    }
                    _ => {}
                }
            }
            result = Some(sold);
        }
        if let (Some(sold), Some(price)) = (result, price_used) {
            let mut need = price.count;
            for s in self.inventory.slots.iter_mut() {
                if need == 0 {
                    break;
                }
                if let Some(st2) = s
                    && st2.item == price.item
                {
                    let take = st2.count.min(need);
                    need -= take;
                    st2.count -= take;
                    if st2.count == 0 {
                        *s = None;
                    }
                }
            }
            let left = self.inventory.add_stack(&reg, sold);
            if left > 0 {
                self.drop_stack(ItemStack {
                    count: left,
                    ..sold
                });
            }
            self.sfx(Sfx::Pickup);
        }
    }

    pub(super) fn bloomery_light_rect(&self) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (
            w / 2.0 + 2.0 * (Self::SLOT + 10.0) + 20.0,
            h / 2.0 - 230.0,
            110.0,
            36.0,
        )
    }

    pub(super) fn bloomery_click(
        &mut self,
        pos: crate::planet::BlockPos,
        slot: usize,
        right: bool,
    ) {
        self.remote_container_notify(pos, slot, right);
        let reg = self.content.reg.clone();
        // Mirror of the host rule: sealed while firing, charge takes
        // what the station smelts, the bank takes its fuel; taking is
        // free. The bloomery wants its chain; the forge takes any
        // smeltable and any fuel.
        let held = self.ui_state.held_stack;
        let (b, ok) = match self.server.world.block_entity_mut_at(&pos) {
            Some(world::BlockEntity::Multiblock(b))
                if b.kind.handler(&reg) == Some(crate::machines::MachineHandler::Bloomery) =>
            {
                let chain = reg.bloomery.first().cloned();
                let want = chain.map(|c| if slot < 4 { c.charge } else { c.fuel });
                let ok = held.is_none() || held.map(|h| Some(h.item)) == Some(want);
                (b, ok)
            }
            Some(world::BlockEntity::Multiblock(b))
                if b.kind.handler(&reg) == Some(crate::machines::MachineHandler::Forge) =>
            {
                let ok = match held {
                    None => true,
                    Some(h) if slot < 4 => {
                        reg.smelts.iter().any(|sm| sm.input.matches(h.item))
                            || reg
                                .forge_salvage
                                .iter()
                                .any(|salvage| salvage.input == h.item)
                            || crate::materials::is_reclaimable_stock(&reg, h.item)
                    }
                    Some(h) => reg.fuel_value(h.item).is_some(),
                };
                (b, ok)
            }
            _ => return,
        };
        if b.lit || slot >= 8 {
            return;
        }
        let s = if slot < 4 {
            &mut b.charge[slot]
        } else {
            &mut b.fuel[slot - 4]
        };
        if ok {
            let (ns, nh) = inventory::click_stack(&reg, *s, self.ui_state.held_stack, right);
            *s = ns;
            self.ui_state.held_stack = nh;
        }
    }

    /// The LIGHT action: needs an ember in hand or inventory, a valid
    /// shell, and a charge. Guests request; the host answers.
    pub(super) fn light_bloomery_action(&mut self, pos: crate::planet::BlockPos) {
        let reg = self.content.reg.clone();
        let Some(ember) = reg.item_id("base:ember") else {
            return;
        };
        let slot =
            (0..TOTAL_SLOTS).find(|&i| self.inventory.slots[i].is_some_and(|s| s.item == ember));
        let Some(slot) = slot else {
            self.toast("Lighting the stack takes a warden's ember.".to_string());
            return;
        };
        if let Some(rc) = &self.multiplayer.remote {
            self.inventory.take_one(slot);
            rc.client.send(&net::C2S::LightBloomery { pos });
            return;
        }
        let block = self.server.world.get_block_at(pos);
        let station = self.content.reg.block(block).interaction.as_deref();
        // Capability E7: light any fire handler by its interaction; the
        // kind's shell and charge rules come from the machine def.
        let res = match station
            .and_then(|interaction| reg.machine_by_interaction(interaction))
            .filter(|kind| {
                reg.machine(*kind).is_some_and(|def| def.handler.has_fire())
            }) {
            Some(kind) => {
                let matched = match kind.validate(&self.server.world, pos) {
                    Some(matched) => matched,
                    None => {
                        self.toast("The stack is breached.".to_string());
                        return;
                    }
                };
                crate::world::machines::light_machine_at(
                    &mut self.server.world,
                    pos,
                    kind,
                    matched,
                )
            }
            None => self.server.world.light_bloomery_at(pos),
        };
        match res {
            Ok(()) => {
                let consumed = self.inventory.slots[slot];
                self.inventory.take_one(slot);
                if !self.creative
                    && let Some(stack) = consumed
                {
                    self.server.world.retire_arcane_stack_at(
                        pos,
                        ItemStack { count: 1, ..stack },
                        "high-heat station ignition",
                    );
                }
                self.sfx(Sfx::Bolt(0.8));
                let kilnish = self.server.world.block_entity_at(&pos).is_some_and(|e| {
                    matches!(
                        e,
                        world::BlockEntity::Multiblock(m)
                            if m.kind.handler(&reg)
                                == Some(crate::machines::MachineHandler::Kiln)
                    )
                });
                self.toast(if kilnish {
                    "The kiln takes the ember. White heat.".to_string()
                } else {
                    "The stack takes the ember. Half a day of fire.".to_string()
                });
            }
            Err(e) => self.toast(e.to_string()),
        }
    }

    /// Kiln slots: 0-3 sand (top), 4 powder (middle), 5-8 fuel (bottom).
    pub(super) fn kiln_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        let (col, row) = if i == 4 {
            (1.5, 1.0)
        } else if i < 4 {
            (i as f32, 0.0)
        } else {
            ((i - 5) as f32, 2.0)
        };
        (
            w / 2.0 - 2.0 * (Self::SLOT + 10.0) + col * (Self::SLOT + 10.0) + 5.0,
            h / 2.0 - 270.0 + row * (Self::SLOT + 26.0),
            Self::SLOT,
            Self::SLOT,
        )
    }

    pub(super) fn kiln_click(&mut self, pos: crate::planet::BlockPos, slot: usize, right: bool) {
        self.remote_container_notify(pos, slot, right);
        let reg = self.content.reg.clone();
        let Some(world::BlockEntity::Multiblock(k)) = self.server.world.block_entity_mut_at(&pos)
        else {
            return;
        };
        if k.lit || slot >= 9 {
            return;
        }
        let base = reg.kiln_base;
        let powders: Vec<ItemId> = reg.kiln.iter().map(|recipe| recipe.powder).collect();
        let ok_put = |it: ItemId| match slot {
            0..=3 => base.map(|(sa, _, _)| sa) == Some(it),
            4 => powders.contains(&it),
            _ => base.map(|(_, fu, _)| fu) == Some(it),
        };
        let s = match slot {
            0..=3 => &mut k.charge[slot],
            4 => &mut k.reagent,
            _ => &mut k.fuel[slot - 5],
        };
        if self.ui_state.held_stack.is_none()
            || self.ui_state.held_stack.map(|h| ok_put(h.item)) == Some(true)
        {
            let (ns, nh) = inventory::click_stack(&reg, *s, self.ui_state.held_stack, right);
            *s = ns;
            self.ui_state.held_stack = nh;
        }
    }

    pub(super) fn furnace_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        let (cx, cy) = (w / 2.0, h / 2.0 - 190.0);
        match i {
            0 => (cx - 120.0, cy - 46.0, Self::SLOT, Self::SLOT),
            1 => (cx - 120.0, cy + 34.0, Self::SLOT, Self::SLOT),
            _ => (cx + 50.0, cy - 6.0, Self::SLOT, Self::SLOT),
        }
    }

    #[allow(clippy::type_complexity)]
    pub(super) fn furnace_view(
        &self,
        pos: crate::planet::BlockPos,
    ) -> (
        Option<ItemStack>,
        Option<ItemStack>,
        Option<ItemStack>,
        f32,
        f32,
    ) {
        match self.server.world.block_entity_at(&pos) {
            Some(world::BlockEntity::Furnace(f)) => {
                let time = f
                    .input
                    .and_then(|s| self.content.reg.smelt_for(s.item))
                    .map(|s| s.time)
                    .unwrap_or(8.0);
                let burn = if f.burn_total > 0.0 {
                    f.burn_left / f.burn_total
                } else {
                    0.0
                };
                (
                    f.input,
                    f.fuel,
                    f.output,
                    (f.progress / time).min(1.0),
                    burn,
                )
            }
            _ => (None, None, None, 0.0, 0.0),
        }
    }

    /// Chest slot rects: 9x3 grid centered above the inventory panel.
    pub(super) fn chest_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        let x0 = w / 2.0 - 4.5 * Self::SLOT;
        let y0 = h / 2.0 - 300.0;
        (
            x0 + (i % 9) as f32 * Self::SLOT,
            y0 + (i / 9) as f32 * Self::SLOT,
            Self::SLOT,
            Self::SLOT,
        )
    }

    /// Armor column beside the paper doll — head, chest, legs, feet, then charm.
    pub(super) fn armor_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let (panel_x, panel_y, _, _) = self.inventory_panel_rect();
        if i == 4 {
            let (avatar_x, avatar_y, avatar_w, avatar_h) = self.inventory_avatar_rect();
            (
                avatar_x + avatar_w + 8.0,
                avatar_y + avatar_h - Self::SLOT,
                Self::SLOT,
                Self::SLOT,
            )
        } else {
            (
                panel_x + 16.0,
                panel_y + 48.0 + i as f32 * Self::SLOT,
                Self::SLOT,
                Self::SLOT,
            )
        }
    }

    pub(super) fn armor_click(&mut self, i: usize) {
        if let Some(remote) = &self.multiplayer.remote {
            remote.client.send(&net::C2S::InventoryClick {
                area: net::InventoryArea::Armor,
                slot: i as u8,
                right: false,
            });
        }
        // Swapping or removing a frame must not strand its components.
        if i < 4 {
            self.return_loadout_components(i);
        }
        let reg = self.content.reg.clone();
        match (self.ui_state.held_stack, self.survival.armor[i]) {
            (Some(h), cur) => {
                // Matching piece in its slot; charms in the charm slot.
                let fits = if i == 4 {
                    reg.item(h.item).charm.is_some()
                } else {
                    reg.item(h.item).armor.map(|(s, _)| s as usize) == Some(i)
                };
                if fits {
                    self.survival.armor[i] = Some(h);
                    self.ui_state.held_stack = cur;
                }
            }
            (None, Some(_)) => {
                self.ui_state.held_stack = self.survival.armor[i].take();
            }
            (None, None) => {}
        }
    }

    /// Offering stone: three slots, centered.
    pub(super) fn offering_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (
            w / 2.0 + (i as f32 - 1.5) * (Self::SLOT + 10.0) + 5.0,
            h / 2.0 - 200.0,
            Self::SLOT,
            Self::SLOT,
        )
    }

    pub(super) fn offering_click(
        &mut self,
        pos: crate::planet::BlockPos,
        slot: usize,
        right: bool,
    ) {
        self.remote_container_notify(pos, slot, right);
        let reg = self.content.reg.clone();
        let Some(world::BlockEntity::Offering(o)) = self.server.world.block_entity_mut_at(&pos)
        else {
            return;
        };
        let (new_slot, new_held) =
            inventory::click_stack(&reg, o.slots[slot], self.ui_state.held_stack, right);
        o.slots[slot] = new_slot;
        self.ui_state.held_stack = new_held;
    }

    /// Guests mirror container clicks to the host with the cursor stack
    /// riding along; the local mutation that follows is a prediction
    /// (same click_stack, same synced content) and the Container +
    /// HeldResult echo is the truth that reconciles it.
    pub(super) fn remote_container_notify(
        &mut self,
        pos: crate::planet::BlockPos,
        slot: usize,
        right: bool,
    ) {
        let Some(r) = &self.multiplayer.remote else {
            return;
        };
        r.client.send(&net::C2S::ContainerClick {
            pos,
            slot: slot as u8,
            right,
        });
    }

    pub(super) fn chest_click(&mut self, pos: crate::planet::BlockPos, slot: usize, right: bool) {
        self.remote_container_notify(pos, slot, right);
        let reg = self.content.reg.clone();
        let Some(world::BlockEntity::Chest(c)) = self.server.world.block_entity_mut_at(&pos) else {
            return;
        };
        let (new_slot, new_held) =
            inventory::click_stack(&reg, c.slots[slot], self.ui_state.held_stack, right);
        c.slots[slot] = new_slot;
        self.ui_state.held_stack = new_held;
    }

    pub(super) fn furnace_click(&mut self, pos: crate::planet::BlockPos, slot: usize, right: bool) {
        self.remote_container_notify(pos, slot, right);
        let reg = self.content.reg.clone();
        let Some(world::BlockEntity::Furnace(f)) = self.server.world.block_entity_mut_at(&pos)
        else {
            return;
        };
        match slot {
            0 | 1 => {
                let cur = if slot == 0 { f.input } else { f.fuel };
                let (new_slot, new_held) =
                    inventory::click_stack(&reg, cur, self.ui_state.held_stack, right);
                if slot == 0 {
                    if f.input.map(|s| s.item) != new_slot.map(|s| s.item) {
                        f.progress = 0.0;
                    }
                    f.input = new_slot;
                } else {
                    f.fuel = new_slot;
                }
                self.ui_state.held_stack = new_held;
            }
            _ => {
                // Output: take-only, merging into the held stack.
                let Some(out) = f.output else { return };
                match self.ui_state.held_stack {
                    None => {
                        self.ui_state.held_stack = Some(out);
                        f.output = None;
                        self.grant_xp("smelt");
                    }
                    Some(h)
                        if h.can_merge(&reg, &out)
                            && h.count + out.count <= reg.item(h.item).max_stack =>
                    {
                        self.ui_state.held_stack = Some(ItemStack {
                            count: h.count + out.count,
                            ..h
                        });
                        f.output = None;
                        self.grant_xp("smelt");
                    }
                    _ => {}
                }
            }
        }
    }

    pub(super) const BCOLS: usize = 6;
    pub(super) const BROWS: usize = 8;
    pub(super) const BSLOT: f32 = 40.0;
}

impl Game {
    /// One recipe row of the workbench list (capability E7). The list
    /// starts under the title and drops one row per recipe.
    pub(super) fn workbench_recipe_rect(&self, index: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (w / 2.0 - 330.0, h / 2.0 - 250.0 + index as f32 * 62.0, 660.0, 56.0)
    }

    /// One widget row on a mod screen (capability E11).
    pub(super) fn mod_screen_row_rect(&self, index: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (w / 2.0 - 330.0, h / 2.0 - 240.0 + index as f32 * 56.0, 660.0, 46.0)
    }

    /// The def behind the open mod screen, if any.
    pub(super) fn mod_screen_def(&self) -> Option<&crate::screens::ScreenDef> {
        match self.ui_state.screen {
            Screen::Mod(i) => self.content.reg.screens.get(i),
            _ => None,
        }
    }

    /// Click handling for a mod screen (capability E11): toggles flip
    /// their player-KV key client-side; buttons dispatch the mod's
    /// `on_screen_click` hook — directly solo, via the host for a guest.
    pub(super) fn mod_screen_click(&mut self) {
        // Scope the registry borrow: everything needed from the def is
        // cloned before any `&mut self` work below.
        let open = self
            .mod_screen_def()
            .map(|d| (d.id.clone(), d.rows().to_vec()));
        let Some((id, rows)) = open else {
            return;
        };
        for (i, row) in rows.iter().enumerate() {
            if !self.hit(self.mod_screen_row_rect(i)) {
                continue;
            }
            match row {
                crate::screens::ScreenWidget::Toggle { key, .. } => {
                    let next = if self.read_player_kv(key).as_deref() == Some("1") {
                        "0"
                    } else {
                        "1"
                    };
                    self.write_player_kv(key, next.to_string());
                    self.sfx(Sfx::Click);
                }
                crate::screens::ScreenWidget::Button { action, .. } => {
                    self.sfx(Sfx::Click);
                    if let Some(rc) = &self.multiplayer.remote {
                        // The host validates both ids against its own
                        // registry before dispatching anything.
                        rc.client.send(&net::C2S::ScreenClick {
                            screen: id.clone(),
                            action: action.clone(),
                        });
                    } else if self.content.scripts.wants("on_screen_click") {
                        // Rhai's FuncArgs wants 'static: hand over owned
                        // clones rather than borrows.
                        let sid = id.clone();
                        let act = action.clone();
                        self.content.scripts.dispatch(
                            &self.server.world,
                            "on_screen_click",
                            (sid, act),
                        );
                        self.apply_script_cmds();
                    }
                }
                _ => {}
            }
            return;
        }
        for i in 0..TOTAL_SLOTS {
            if self.hit(self.inv_slot_rect(i)) {
                self.inventory_click(false, i, false);
                return;
            }
        }
    }

    /// Craft a workbench recipe from the inventory (capability E7). The
    /// screen lists the machine's `station` recipes; clicking one consumes
    /// one of each ingredient and adds the output, exactly like the free
    /// grid but bound to the machine rather than the player's hands.
    pub(super) fn workbench_craft(
        &mut self,
        pos: crate::planet::BlockPos,
        recipe_index: usize,
    ) {
        let reg = self.content.reg.clone();
        let Some(world::BlockEntity::Multiblock(m)) =
            self.server.world.block_entity_at(&pos)
        else {
            return;
        };
        let recipes = reg.machine_recipes_for(m.kind);
        let Some(recipe) = recipes.get(recipe_index) else {
            return;
        };
        let tech_value = recipe
            .tech
            .as_deref()
            .and_then(|key| self.read_player_kv(key));
        if let Some(gate) = recipe_gates_met(tech_value.as_deref(), &self.inventory, recipe) {
            let label = if gate == "blueprint" {
                recipe
                    .blueprint
                    .map(|b| format!("requires {}", reg.item(b).label))
                    .unwrap_or_else(|| "requires a blueprint".to_string())
            } else {
                "is locked".to_string()
            };
            self.toast(format!("This recipe {label}."));
            return;
        }
        let mut found: Vec<usize> = Vec::new();
        'ingredients: for cell in recipe.pattern.iter().flatten() {
            for (index, slot) in self.inventory.slots.iter().enumerate() {
                if !found.contains(&index)
                    && slot.is_some_and(|stack| cell.matches(stack.item))
                {
                    found.push(index);
                    continue 'ingredients;
                }
            }
            self.toast("You're missing an ingredient.".to_string());
            return;
        }
        for index in found {
            let stack = self.inventory.slots[index].as_mut().expect("just located");
            stack.count -= 1;
            if stack.count == 0 {
                self.inventory.slots[index] = None;
            }
        }
        let output = ItemStack::new(&reg, recipe.output, recipe.count);
        let left = self.inventory.add_stack(&reg, output);
        if left > 0 {
            self.drop_stack(ItemStack { count: left, ..output });
        }
        self.sfx(Sfx::Craft);
        self.grant_xp("craft");
    }
}

/// Spec 3.5 gate check as a pure seam: `tech_value` is the per-player KV
/// value for `recipe.tech` (None = key absent). Returns `Some` with the
/// unmet gate's label when the recipe is locked. Standalone so both craft
/// sites and the tests share one implementation.
pub(crate) fn recipe_gates_met(
    tech_value: Option<&str>,
    inventory: &crate::inventory::Inventory,
    recipe: &RecipeDef,
) -> Option<&'static str> {
    if let Some(_tech) = &recipe.tech {
        match tech_value {
            Some(value) if !value.is_empty() && value != "false" && value != "0" => {}
            _ => return Some("locked"),
        }
    }
    if let Some(blueprint) = recipe.blueprint
        && !inventory.can_afford(&[(blueprint, 1)])
    {
        return Some("blueprint");
    }
    None
}
