//! Block menus in the ordered graphical action pipeline.

use crate::game::Game;
use crate::world::TerrainRead;
use crate::audio::Sfx;
use crate::net;
use crate::raycast;
use crate::world;
use crate::game::navigation::Screen;
use super::ActionFrame;

impl Game {
    pub(in crate::game) fn use_crafting_block(&mut self) -> bool {

        self.input.right_held = false;
        self.interaction.craft_size = 3;
        self.set_screen(Screen::Inventory);
        true
    }
    pub(in crate::game) fn use_furnace_block(&mut self, h: &raycast::PlanetHit) -> bool {

        self.input.right_held = false;
        if let Some(remote) = &self.multiplayer.remote {
            remote.session.send(&net::C2S::OpenContainer { pos: h.block });
            return true;
        }
        self.runtime.local_mut().world.ensure_block_entity_at(
            h.block,
            world::BlockEntity::Furnace(Default::default()),
        );
        self.set_screen(Screen::Furnace(h.block));
        true
    }
    pub(in crate::game) fn use_switch_block(&mut self, h: &raycast::PlanetHit) -> bool {

        self.input.action_cooldown = 0.25;
        self.input.right_held = false;
        if let Some(rc) = &self.multiplayer.remote {
            // The host owns the switch; it echoes the selection.
            rc.session.send(&net::C2S::ToggleSwitch { pos: h.block });
            return true;
        }
        self.runtime.local_mut().world.toggle_switch(h.block);
        self.toast("The switch points differently now.".to_string());
        true
    }
    pub(in crate::game) fn use_depot_block(&mut self, frame: &ActionFrame, h: &raycast::PlanetHit) -> bool {
        let reg = &frame.reg;
        let held = frame.held;

        self.input.action_cooldown = 0.3;
        self.input.right_held = false;
        let held = self.inventory.slots[self.input.hotbar_sel];
        let Some(held) = held else {
            self.toast("Nothing in hand to deliver.".to_string());
            return true;
        };
        if let Some(rc) = &self.multiplayer.remote {
            rc.session.send(&net::C2S::DepotDeposit { pos: h.block });
            return true;
        }
        let item_name = reg.item(held.item).name.clone();
        let delivery = self.runtime.local_mut().world.deliver_to_depot(
            h.block,
            &mut self.inventory,
            self.input.hotbar_sel,
        );
        match delivery {
            Some((settlement, _, units, rep_per_unit)) => {
                self.sfx(Sfx::Click);
                // Solo: the player KV namespace lives on this
                // Game, so pay the standing directly.
                let rep = units * rep_per_unit;
                let rep_key = self
                    .content
                    .reg
                    .settlements
                    .iter()
                    .find(|sd| sd.id == settlement)
                    .map(|sd| sd.rep_key.clone())
                    .unwrap_or_else(|| format!("rep_{settlement}"));
                let ns = self.player_namespace();
                self.content
                    .scripts
                    .kv
                    .borrow_mut()
                    .entry(ns)
                    .or_default()
                    .entry(rep_key)
                    .and_modify(|current: &mut String| {
                        *current =
                            (current.parse::<u32>().unwrap_or(0) + rep).to_string();
                    })
                    .or_insert_with(|| rep.to_string());
                self.toast(format!(
                    "{settlement} appreciates the {item_name} (+{rep} standing)."
                ));
            }
            _ => {
                self.toast(format!(
                    "The depot has no appetite for {} right now.",
                    reg.item(held.item).label
                ));
            }
        }
        true
    }
    pub(in crate::game) fn use_mod_screen_block(&mut self, frame: &ActionFrame, s: &str) -> bool {
        let reg = &frame.reg;

        self.input.right_held = false;
        if self.input.action_cooldown > 0.0 {
            return true;
        }
        self.input.action_cooldown = 0.3;
        match reg.screen_by_interaction(s) {
            Some(idx) => {
                self.sfx(Sfx::Click);
                self.set_screen(Screen::Mod(idx));
            }
            None => self.toast("The panel is blank.".to_string()),
        }
        true
    }
}
