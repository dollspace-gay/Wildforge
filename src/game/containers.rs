//! Inventory, crafting, armor, and machine-container interactions.

use crate::world::TerrainRead;

use crate::audio::Sfx;
use crate::crafting;
use crate::identity;
use crate::inventory;
use crate::inventory::ItemStack;
use crate::inventory::TOTAL_SLOTS;
use crate::net;
use crate::registry::ItemId;
use crate::world;
use super::Game;
use super::navigation::Screen;
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
            remote.session.send(&net::C2S::InventoryClick {
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
            remote.session.send(&net::C2S::CraftResult {
                size: self.interaction.craft_size as u8,
            });
        }
        let Ok(effects) = crate::player_ops::craft::take_result(
            &reg,
            self.interaction.craft_size,
            &mut self.interaction.craft_grid,
            &mut self.inventory,
            &mut self.ui_state.held_stack,
        ) else {
            return;
        };
        let kind = self.runtime.finish_craft(effects, self.player.pos.block(), &mut self.inventory);
        self.sfx(Sfx::Craft);
        if let crate::player_ops::craft::CraftKind::Recipe(output) = kind {
            self.grant_xp("craft");
            if self.content.scripts.wants("on_craft") {
                let name = reg.item(output).name.clone();
                self.content.scripts.dispatch_view(&self.runtime.view(), "on_craft", (name,));
                self.apply_script_cmds();
            }
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
            rc.session.send(&net::C2S::MobCargoClick {
                id: mob_id,
                slot: slot as u8,
                right,
            });
            return;
        }
        let held = self.ui_state.held_stack;
        let Some(mob) = self.runtime.local_mut().world.mob_by_id_mut(mob_id) else {
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
            &self.runtime.local().world.save_dir_for_saving(),
            self.identity.device_id(),
        )
        .map(|p| p.0)
        .unwrap_or([0; 16]);
        match self.runtime.view().block_entity_at(&pos) {
            Some(world::BlockEntity::Stall(st)) => st.owner == my_id,
            _ => false,
        }
    }

    pub(super) fn stall_click(&mut self, pos: crate::planet::BlockPos, slot: usize, right: bool) {
        if slot > 12 {
            return;
        }
        if let Some(rc) = &self.multiplayer.remote {
            rc.session.send(&net::C2S::ContainerClick {
                pos,
                slot: slot as u8,
                right,
            });
            return;
        }
        if !self.stall_is_mine(pos) {
            return; // visitors browse; the BUY button is theirs
        }
        let owner = match self.runtime.view().block_entity_at(&pos) {
            Some(world::BlockEntity::Stall(stall)) => stall.owner,
            _ => return,
        };
        let _ = self.runtime.click_container(
            pos, &mut self.ui_state.held_stack,
            crate::player_ops::container::Click { slot, right, actor: Some(owner) },
        );
    }

    /// Local purchase: the singleplayer/host mirror of C2S::StallBuy.
    pub(super) fn stall_buy_local(&mut self, pos: crate::planet::BlockPos) {
        let reg = self.content.reg.clone();
        if !self.runtime.view().check_stall_at(pos) {
            self.toast("The stall wants its posts and awning.".to_string());
            return;
        }
        let Some(world::BlockEntity::Stall(stall)) = self.runtime.local_mut().world.block_entity_mut_at(&pos) else {
            return;
        };
        let Ok(purchase) = crate::player_ops::trade::purchase(&reg, stall, &mut self.inventory) else {
            return;
        };
        if let Some(stack) = purchase.overflow {
            self.drop_stack(stack);
        }
        self.sfx(Sfx::Pickup);
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
        self.exchange_container_slot(pos, slot, right);
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
            rc.session.send(&net::C2S::LightBloomery { pos });
            return;
        }
        let block = self.runtime.view().get_block_at(pos);
        let station = self.content.reg.block(block).interaction.as_deref();
        // Capability E7: light any fire handler by its interaction; the
        // kind's shell and charge rules come from the machine def.
        let res = match station
            .and_then(|interaction| reg.machine_by_interaction(interaction))
            .filter(|kind| reg.machine(*kind).is_some_and(|def| def.handler.has_fire()))
        {
            Some(kind) => {
                let matched = match kind.validate(&self.runtime.local().world, pos) {
                    Some(matched) => matched,
                    None => {
                        self.toast("The stack is breached.".to_string());
                        return;
                    }
                };
                crate::world::machines::light_machine_at(&mut self.runtime.local_mut().world, pos, kind, matched)
            }
            None => self.runtime.local_mut().world.light_bloomery_at(pos),
        };
        match res {
            Ok(()) => {
                let consumed = self.inventory.slots[slot];
                self.inventory.take_one(slot);
                if !self.creative
                    && let Some(stack) = consumed
                {
                    self.runtime.local_mut().world.retire_arcane_stack_at(
                        pos,
                        ItemStack { count: 1, ..stack },
                        "high-heat station ignition",
                    );
                }
                self.sfx(Sfx::Bolt(0.8));
                let kilnish = self.runtime.view().block_entity_at(&pos).is_some_and(|e| {
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
        self.exchange_container_slot(pos, slot, right);
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
        match self.runtime.view().block_entity_at(&pos) {
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
        let (panel_x, panel_y, _, _) = self.inventory_layout().panel_rect();
        if i == 4 {
            let (avatar_x, avatar_y, avatar_w, avatar_h) = self.inventory_layout().avatar_rect();
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
            remote.session.send(&net::C2S::InventoryClick {
                area: net::InventoryArea::Armor,
                slot: i as u8,
                right: false,
            });
        }
        // Swapping or removing a frame must not strand its components.
        if i < 4 {
            self.return_loadout_components(i);
        }
        crate::player_ops::equipment::exchange(
            &self.content.reg,
            &mut self.survival.armor,
            &mut self.ui_state.held_stack,
            i,
        );
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
        self.exchange_container_slot(pos, slot, right);
    }

    /// The same exchange updates a local container or predicts the received
    /// snapshot. The host echo remains the truth for a graphical guest.
    fn exchange_container_slot(&mut self, pos: crate::planet::BlockPos, slot: usize, right: bool) {
        self.remote_container_notify(pos, slot, right);
        let result = self.runtime.click_container(
            pos, &mut self.ui_state.held_stack,
            crate::player_ops::container::Click { slot, right, actor: None },
        );
        if result.is_ok_and(|effect| effect.took_furnace_output) { self.grant_xp("smelt"); }
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
        r.session.send(&net::C2S::ContainerClick {
            pos,
            slot: slot as u8,
            right,
        });
    }

    pub(super) fn chest_click(&mut self, pos: crate::planet::BlockPos, slot: usize, right: bool) {
        self.exchange_container_slot(pos, slot, right);
    }

    pub(super) fn furnace_click(&mut self, pos: crate::planet::BlockPos, slot: usize, right: bool) {
        self.exchange_container_slot(pos, slot, right);
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
        (
            w / 2.0 - 330.0,
            h / 2.0 - 250.0 + index as f32 * 62.0,
            660.0,
            56.0,
        )
    }

    /// One widget row on a mod screen (capability E11).
    pub(super) fn mod_screen_row_rect(&self, index: usize) -> (f32, f32, f32, f32) {
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;
        (
            w / 2.0 - 330.0,
            h / 2.0 - 240.0 + index as f32 * 56.0,
            660.0,
            46.0,
        )
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
                        rc.session.send(&net::C2S::ScreenClick {
                            screen: id.clone(),
                            action: action.clone(),
                        });
                    } else if self.content.scripts.wants("on_screen_click") {
                        // Rhai's FuncArgs wants 'static: hand over owned
                        // clones rather than borrows.
                        let sid = id.clone();
                        let act = action.clone();
                        self.content.scripts.dispatch_view(
                            &self.runtime.view(),
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
            if self.hit(self.inventory_layout().slot_rect(i)) {
                self.inventory_click(false, i, false);
                return;
            }
        }
    }

    /// Craft a workbench recipe from the inventory (capability E7). The
    /// screen lists the machine's `station` recipes; clicking one consumes
    /// one of each ingredient and adds the output, exactly like the free
    /// grid but bound to the machine rather than the player's hands.
    pub(super) fn workbench_craft(&mut self, pos: crate::planet::BlockPos, recipe_index: usize) {
        let reg = self.content.reg.clone();
        let Some(world::BlockEntity::Multiblock(m)) = self.runtime.view().block_entity_at(&pos)
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
                if !found.contains(&index) && slot.is_some_and(|stack| cell.matches(stack.item)) {
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
            self.drop_stack(ItemStack {
                count: left,
                ..output
            });
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
