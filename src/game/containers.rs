//! Inventory, crafting, armor, and machine-container interactions.

use super::*;

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
        if let Some(remote) = &self.multiplayer.remote {
            remote.client.send(&net::C2S::CraftResult {
                size: self.interaction.craft_size as u8,
            });
        }
        let reg = self.content.reg.clone();
        let n2 = self.interaction.craft_size * self.interaction.craft_size;
        let Some(recipe) = crafting::match_recipe(
            &reg,
            &self.interaction.craft_grid[..n2],
            self.interaction.craft_size,
        ) else {
            return;
        };
        let out = ItemStack::new(&reg, recipe.output, recipe.count);
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
        self.sfx(Sfx::Craft);
        if self.content.scripts.wants("on_craft") {
            let name = reg.item(recipe.output).name.clone();
            self.content
                .scripts
                .dispatch(&self.server.world, "on_craft", (name,));
            self.apply_script_cmds();
        }
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

    pub(super) fn bloomery_click(&mut self, pos: (i32, i32, i32), slot: usize, right: bool) {
        self.remote_container_notify(pos, slot, right);
        let reg = self.content.reg.clone();
        let Some(world::BlockEntity::Bloomery(b)) = self.server.world.block_entity_mut(&pos) else {
            return;
        };
        // Mirror of the host rule: sealed while firing, charge takes
        // the chain's ore, the bank takes its fuel; taking is free.
        if b.lit || slot >= 8 {
            return;
        }
        let chain = reg.bloomery.first().cloned();
        let want = chain.map(|c| if slot < 4 { c.charge } else { c.fuel });
        let s = if slot < 4 {
            &mut b.charge[slot]
        } else {
            &mut b.fuel[slot - 4]
        };
        if self.ui_state.held_stack.is_none()
            || self.ui_state.held_stack.map(|h| Some(h.item)) == Some(want)
        {
            let (ns, nh) = inventory::click_stack(&reg, *s, self.ui_state.held_stack, right);
            *s = ns;
            self.ui_state.held_stack = nh;
        }
    }

    /// The LIGHT action: needs an ember in hand or inventory, a valid
    /// shell, and a charge. Guests request; the host answers.
    pub(super) fn light_bloomery_action(&mut self, pos: (i32, i32, i32)) {
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
            rc.client.send(&net::C2S::LightBloomery {
                x: pos.0,
                y: pos.1,
                z: pos.2,
            });
            return;
        }
        let kilnish = self
            .content
            .reg
            .block(self.server.world.get_block(pos.0, pos.1, pos.2))
            .interaction
            .as_deref()
            == Some("kiln");
        let res = if kilnish {
            self.server.world.light_kiln(pos.0, pos.1, pos.2)
        } else {
            self.server.world.light_bloomery(pos.0, pos.1, pos.2)
        };
        match res {
            Ok(()) => {
                self.inventory.take_one(slot);
                self.sfx(Sfx::Bolt(0.8));
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

    pub(super) fn kiln_click(&mut self, pos: (i32, i32, i32), slot: usize, right: bool) {
        self.remote_container_notify(pos, slot, right);
        let reg = self.content.reg.clone();
        let Some(world::BlockEntity::Kiln(k)) = self.server.world.block_entity_mut(&pos) else {
            return;
        };
        if k.lit || slot >= 9 {
            return;
        }
        let base = reg.kiln_base;
        let powders: Vec<ItemId> = reg.kiln.iter().map(|(p, _)| *p).collect();
        let ok_put = |it: ItemId| match slot {
            0..=3 => base.map(|(sa, _, _)| sa) == Some(it),
            4 => powders.contains(&it),
            _ => base.map(|(_, fu, _)| fu) == Some(it),
        };
        let s = match slot {
            0..=3 => &mut k.sand[slot],
            4 => &mut k.powder,
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
        pos: (i32, i32, i32),
    ) -> (
        Option<ItemStack>,
        Option<ItemStack>,
        Option<ItemStack>,
        f32,
        f32,
    ) {
        match self.server.world.block_entity(&pos) {
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

    /// Armor column: right of the storage grid — head, chest, legs, feet.
    pub(super) fn armor_slot_rect(&self, i: usize) -> (f32, f32, f32, f32) {
        let (x0, y0, _, _) = self.inv_slot_rect(HOTBAR_SLOTS);
        (
            x0 + 9.0 * Self::SLOT + 14.0,
            y0 + i as f32 * Self::SLOT,
            Self::SLOT,
            Self::SLOT,
        )
    }

    pub(super) fn armor_click(&mut self, i: usize) {
        if let Some(remote) = &self.multiplayer.remote {
            remote.client.send(&net::C2S::InventoryClick {
                area: net::InventoryArea::Armor,
                slot: i as u8,
                right: false,
            });
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

    pub(super) fn offering_click(&mut self, pos: (i32, i32, i32), slot: usize, right: bool) {
        self.remote_container_notify(pos, slot, right);
        let reg = self.content.reg.clone();
        let Some(world::BlockEntity::Offering(o)) = self.server.world.block_entity_mut(&pos) else {
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
        pos: (i32, i32, i32),
        slot: usize,
        right: bool,
    ) {
        let Some(r) = &self.multiplayer.remote else {
            return;
        };
        r.client.send(&net::C2S::ContainerClick {
            x: pos.0,
            y: pos.1,
            z: pos.2,
            slot: slot as u8,
            right,
        });
    }

    pub(super) fn chest_click(&mut self, pos: (i32, i32, i32), slot: usize, right: bool) {
        self.remote_container_notify(pos, slot, right);
        let reg = self.content.reg.clone();
        let Some(world::BlockEntity::Chest(c)) = self.server.world.block_entity_mut(&pos) else {
            return;
        };
        let (new_slot, new_held) =
            inventory::click_stack(&reg, c.slots[slot], self.ui_state.held_stack, right);
        c.slots[slot] = new_slot;
        self.ui_state.held_stack = new_held;
    }

    pub(super) fn furnace_click(&mut self, pos: (i32, i32, i32), slot: usize, right: bool) {
        self.remote_container_notify(pos, slot, right);
        let reg = self.content.reg.clone();
        let Some(world::BlockEntity::Furnace(f)) = self.server.world.block_entity_mut(&pos) else {
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
