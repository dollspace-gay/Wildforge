//! Block machines in the ordered graphical action pipeline.

use super::ActionFrame;
use crate::audio::Sfx;
use crate::game::Game;
use crate::game::navigation::Screen;
use crate::inventory::ItemStack;
use crate::net;
use crate::raycast;
use crate::world;

impl Game {
    pub(in crate::game) fn use_sign_block(&mut self, h: &raycast::PlanetHit) -> bool {
        // Reopen the editor with what's written.
        self.input.action_cooldown = 0.3;
        self.input.right_held = false;
        let cur = match self.runtime.view().block_entity_at(&h.block) {
            Some(world::BlockEntity::Sign(sg)) => sg.lines.clone(),
            _ => Default::default(),
        };
        self.ui_state.sign_lines = cur;
        self.ui_state.sign_line = 0;
        self.set_screen(Screen::SignEdit(h.block));
        true
    }
    pub(in crate::game) fn use_waystone_block(&mut self, h: &raycast::PlanetHit) -> bool {
        self.input.action_cooldown = 0.4;
        self.read_waystone(h.block);
        true
    }
    pub(in crate::game) fn use_survey_block(&mut self, h: &raycast::PlanetHit) -> bool {
        // A raised cairn is bought knowledge: anyone reads
        // the surveyor's ground, no pick required — and a
        // country's heart is the first thing worth knowing.
        let report = self.runtime.view().heart_report_at(h.block.surface());
        self.toast(report);
        self.toast_prospect(h.block.surface());
        self.sfx(Sfx::Click);
        self.input.action_cooldown = 0.6;
        true
    }
    pub(in crate::game) fn use_worked_station_block(
        &mut self,
        frame: &ActionFrame,
        h: &raycast::PlanetHit,
        st: &str,
    ) -> bool {
        let reg = &frame.reg;
        let held = frame.held;

        // Rest work with a click, take it back bare-handed.
        // The held channels (hammer strikes, quern turns)
        // run earlier and return before reaching this arm.
        let table = world::worked_table_for(st);
        let workable = held.is_some_and(|i| {
            reg.worked
                .iter()
                .any(|w| w.input == i && w.station == table)
        });
        if workable {
            self.input.action_cooldown = 0.25;
            let stack = self.inventory.slots[self.input.hotbar_sel].unwrap();
            if let Some(rc) = &self.multiplayer.remote {
                rc.session.send(&net::C2S::AnvilPut { pos: h.block });
                return true;
            }
            let one = ItemStack { count: 1, ..stack };
            if self.runtime.local_mut().world.anvil_put_at(h.block, one) {
                if !self.creative {
                    self.inventory.take_one(self.input.hotbar_sel);
                }
                self.sfx(Sfx::Place);
            } else {
                self.toast("It holds all it can.".to_string());
            }
            return true;
        }
        if held.is_none() {
            self.input.action_cooldown = 0.3;
            if let Some(rc) = &self.multiplayer.remote {
                rc.session.send(&net::C2S::AnvilTake { pos: h.block });
                return true;
            }
            if let Some(st) = self.runtime.local_mut().world.anvil_take_at(h.block) {
                let left = self.inventory.add_stack(reg, st);
                if left > 0 {
                    self.drop_stack(ItemStack { count: left, ..st });
                }
                self.sfx(Sfx::Pickup);
            }
            return true;
        }
        true
    }
    pub(in crate::game) fn use_separator_block(
        &mut self,
        frame: &ActionFrame,
        h: &raycast::PlanetHit,
        interaction: &str,
    ) -> bool {
        let reg = &frame.reg;
        let held = frame.held;

        if self.reject_guest_action() {
            return true;
        }
        // Powder and fuel in by hand; bare hands take the
        // split back out (smoker rules, no screen).
        self.input.action_cooldown = 0.3;
        let kind = reg
            .machine_by_interaction(interaction)
            .expect("resolved above");
        let powder = reg.item_id("base:rare_earth_powder");
        // Separator persistence stores this bed as a count and
        // returns charcoal on dismantling, so admitting arbitrary
        // finite coal here would destroy its identity.
        let is_fuel = held == reg.item_id("base:charcoal");
        self.runtime.local_mut().world.ensure_block_entity_at(
            h.block,
            world::BlockEntity::Multiblock(world::MachineInstance {
                kind,
                ..Default::default()
            }),
        );
        let valid = kind.validate(&self.runtime.view(), h.block).is_some();
        let Some(world::BlockEntity::Multiblock(sp)) =
            self.runtime.local_mut().world.block_entity_mut_at(&h.block)
        else {
            return true;
        };
        if held.is_some() && held == powder {
            if sp.powder >= 8 {
                self.toast("The hopper is full.".to_string());
                return true;
            }
            if self.creative || self.inventory.take_one(self.input.hotbar_sel).is_some() {
                if let Some(world::BlockEntity::Multiblock(sp)) =
                    self.runtime.local_mut().world.block_entity_mut_at(&h.block)
                {
                    sp.powder += 1;
                }
                self.sfx(Sfx::Place);
                if !valid {
                    self.toast("The separator wants its firebrick stack.".to_string());
                }
            }
            return true;
        }
        if is_fuel {
            if sp.separator_fuel >= 8 {
                self.toast("The firebed is full.".to_string());
                return true;
            }
            if self.creative || self.inventory.take_one(self.input.hotbar_sel).is_some() {
                if let Some(world::BlockEntity::Multiblock(sp)) =
                    self.runtime.local_mut().world.block_entity_mut_at(&h.block)
                {
                    sp.separator_fuel += 1;
                }
                self.sfx(Sfx::Place);
            }
            return true;
        }
        if held.is_none() {
            let (nd, ce) = (sp.neodymium, sp.cerium);
            if nd == 0 && ce == 0 {
                let (p, f) = (sp.powder, sp.separator_fuel);
                self.toast(format!("Powder {p}, fuel {f}, nothing split yet."));
                return true;
            }
            if let Some(world::BlockEntity::Multiblock(sp)) =
                self.runtime.local_mut().world.block_entity_mut_at(&h.block)
            {
                sp.neodymium = 0;
                sp.cerium = 0;
            }
            for (name, n) in [("base:neodymium", nd), ("base:cerium", ce)] {
                if n > 0
                    && let Some(item) = reg.item_id(name)
                {
                    let mut st = ItemStack::new(reg, item, 1);
                    st.count = n;
                    let left = self.inventory.add_stack(reg, st);
                    if left > 0 {
                        self.drop_stack(ItemStack { count: left, ..st });
                    }
                }
            }
            self.sfx(Sfx::Pickup);
        }
        true
    }
    pub(in crate::game) fn use_firebox_block(
        &mut self,
        frame: &ActionFrame,
        h: &raycast::PlanetHit,
    ) -> bool {
        let reg = &frame.reg;
        let held = frame.held;

        if self.reject_guest_action() {
            return true;
        }
        // Coal in at the door; bare hands read the gauges.
        self.input.action_cooldown = 0.3;
        let fuel = held.and_then(|i| reg.fuel_value(i));
        let e = self
            .runtime
            .local_mut()
            .world
            .ensure_block_entity_at(h.block, world::BlockEntity::Steam(Default::default()));
        let world::BlockEntity::Steam(s) = e else {
            return true;
        };
        if let Some((burn, _)) = fuel {
            if s.fuel >= world::STEAM_FUEL_CAP {
                self.toast("The firebox is banked full.".to_string());
                return true;
            }
            if self.creative || self.inventory.take_one(self.input.hotbar_sel).is_some() {
                if let Some(item) = held
                    && let Some(ledger) = &mut self.runtime.local_mut().world.material_ledger
                {
                    let materials =
                        crate::materials::stack_materials(reg, ItemStack::new(reg, item, 1));
                    if let Err(error) = ledger.record_consumption(&materials) {
                        eprintln!("materials: firebox fuel accounting failed: {error}");
                    }
                }
                let e = self.runtime.local_mut().world.block_entity_mut_at(&h.block);
                if let Some(world::BlockEntity::Steam(s)) = e {
                    s.fuel = (s.fuel + burn * 4.0).min(world::STEAM_FUEL_CAP);
                }
                self.sfx(Sfx::Place);
            }
            return true;
        }
        let f = s.fuel as u32;
        let blocks = s.water.water_hu as f64 / crate::planet_atlas::HYDRO_UNITS_PER_BLOCK as f64;
        let salinity = s.water.salinity();
        self.toast(format!(
            "Fire banked {f}s; boiler water {blocks:.2} blocks (salinity {}).",
            salinity
        ));
        true
    }
    pub(in crate::game) fn use_fire_station_block(
        &mut self,
        frame: &ActionFrame,
        h: &raycast::PlanetHit,
        interaction: &str,
    ) -> bool {
        let reg = &frame.reg;

        self.input.action_cooldown = 0.3;
        if let Some(rc) = &self.multiplayer.remote {
            rc.session.send(&net::C2S::OpenContainer { pos: h.block });
            return true;
        }
        let kind = reg
            .machine_by_interaction(interaction)
            .expect("resolved above");
        let screen = match reg.machine(kind).map(|def| def.handler) {
            Some(crate::machines::MachineHandler::Kiln) => Screen::Kiln(h.block),
            Some(crate::machines::MachineHandler::Bloomery)
            | Some(crate::machines::MachineHandler::Forge) => Screen::Bloomery(h.block),
            _ => return true,
        };
        let default = world::BlockEntity::Multiblock(world::MachineInstance {
            kind,
            ..Default::default()
        });
        self.runtime
            .local_mut()
            .world
            .ensure_block_entity_at(h.block, default);
        self.set_screen(screen);
        true
    }
    pub(in crate::game) fn use_recipe_station_block(
        &mut self,
        frame: &ActionFrame,
        h: &raycast::PlanetHit,
        interaction: &str,
    ) -> bool {
        let reg = &frame.reg;

        // A recipe-list station (the workbench pattern): the
        // screen lists the machine's `station` recipes and the
        // player crafts them from the inventory.
        self.input.right_held = false;
        if let Some(rc) = &self.multiplayer.remote {
            rc.session.send(&net::C2S::OpenContainer { pos: h.block });
            return true;
        }
        let kind = reg
            .machine_by_interaction(interaction)
            .expect("resolved above");
        let default = world::BlockEntity::Multiblock(world::MachineInstance {
            kind,
            ..Default::default()
        });
        self.runtime
            .local_mut()
            .world
            .ensure_block_entity_at(h.block, default);
        self.set_screen(Screen::Workbench(h.block));
        true
    }
}
