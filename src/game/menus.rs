//! Ordered menu dispatch; screen handlers own their transitions.

use crate::game::{Game, navigation::Screen};
use winit::event_loop::ActiveEventLoop;

impl Game {
    pub(super) fn menu_click(&mut self, event_loop: &ActiveEventLoop, right: bool) {
        if std::env::var("WILDFORGE_DEBUG").is_ok() {
            eprintln!("menu_click at {:?} right={right}", self.input.ui_cursor);
        }
        match self.ui_state.screen {
            Screen::Inventory => self.click_inventory_menu(right),
            Screen::Paused => self.click_paused_menu(),
            Screen::Moderation(id) => self.click_moderation_menu(id),
            Screen::Dead => self.click_dead_menu(),
            Screen::Title => self.click_title_menu(event_loop),
            Screen::NewWorld => self.click_new_world_menu(),
            Screen::CreatingWorld => self.click_creating_world_menu(),
            Screen::Accounts => self.click_accounts_menu(),
            Screen::Mods => self.click_mods_menu(),
            Screen::Packs => self.click_packs_menu(),
            Screen::Furnace(pos) => self.click_furnace_menu(right, pos),
            Screen::Bloomery(pos) => self.click_bloomery_menu(right, pos),
            Screen::Kiln(pos) => self.click_kiln_menu(right, pos),
            Screen::Workbench(pos) => self.click_workbench_menu(right, pos),
            Screen::Mod(_) => self.click_mod_menu(right),
            Screen::SignEdit(_) => {}
            Screen::Stall(pos) => self.click_stall_menu(right, pos),
            Screen::MobCargo(id) => self.click_mob_cargo_menu(right, id),
            Screen::Chest(pos) => self.click_chest_menu(right, pos),
            Screen::Offering(pos) => self.click_offering_menu(right, pos),
            Screen::Join => self.click_join_menu(),
            Screen::Settings => self.click_settings_menu(),
            Screen::Appearance => self.click_appearance_menu(right),
            Screen::ConfirmDelete => self.click_confirm_delete_menu(),
            Screen::Dialog { .. } => self.click_dialog_menu(),
            Screen::Journal => {}
            Screen::Skills => self.click_skills_menu(),
            Screen::Loadout => self.click_loadout_menu(),
            Screen::Playing => {}
        }
    }
}

mod inventory;
mod pause;
mod moderation;
mod worlds;
mod accounts;
mod preferences;
mod stations;
mod storage;
mod join;
mod character;
mod account_tasks;
