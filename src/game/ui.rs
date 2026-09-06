//! UI layout, drawing, and screen composition.

use super::{Game, navigation::Screen, widgets};
use crate::ui::UiBatch;
use glam::{Mat4, Vec3};

fn project_world_label(
    view_proj: Mat4,
    point: Vec3,
    width: f32,
    height: f32,
) -> Option<(f32, f32)> {
    let clip = view_proj * point.extend(1.0);
    if clip.w <= 0.05 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    if ndc.x.abs() > 1.0 || ndc.y.abs() > 1.0 || !ndc.is_finite() {
        return None;
    }
    Some(((ndc.x * 0.5 + 0.5) * width, (0.5 - ndc.y * 0.5) * height))
}

pub(super) fn wrap_ui_status(
    text: &str,
    max_width: f32,
    scale: f32,
    max_lines: usize,
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_owned()
        } else {
            format!("{current} {word}")
        };
        if !current.is_empty() && UiBatch::text_width(scale, &candidate) > max_width {
            lines.push(std::mem::take(&mut current));
            if lines.len() == max_lines {
                return lines;
            }
            current.push_str(word);
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() && lines.len() < max_lines {
        lines.push(current);
    }
    lines
}

impl Game {
    pub(super) fn hit(&self, r: (f32, f32, f32, f32)) -> bool {
        widgets::hit(self.input.ui_cursor, r)
    }

    pub(super) fn build_ui(&mut self) {
        self.build_ui_inner();
        // Last, so it lands over every slot grid and every early return
        // the screens above take.
        self.draw_item_tooltip();
    }

    fn build_ui_inner(&mut self) {
        self.poll_account_task();
        self.poll_world_creation();
        let mut ui = std::mem::replace(&mut self.ui, UiBatch::new());
        ui.clear();
        ui.press_dip = self.presentation.juice && self.presentation.press_dip > 0.0;
        let w = self.renderer.config.width as f32;
        let h = self.renderer.config.height as f32;

        // Menu-only screens draw over the sky and skip the HUD entirely.
        let menu_only = match self.ui_state.screen {
            Screen::Title => {
                self.draw_title_screen(&mut ui, w, h);
                true
            }
            Screen::NewWorld => {
                self.draw_new_world_screen(&mut ui, w, h);
                true
            }
            Screen::CreatingWorld => {
                self.draw_creating_world_screen(&mut ui, w, h);
                true
            }
            Screen::Accounts => {
                self.draw_accounts_screen(&mut ui, w, h);
                true
            }
            Screen::Moderation(id) => {
                self.draw_moderation_screen(&mut ui, w, h, id);
                true
            }
            Screen::Mods => {
                self.draw_mods_screen(&mut ui, w, h);
                true
            }
            Screen::Packs => {
                self.draw_packs_screen(&mut ui, w, h);
                true
            }
            Screen::Join => {
                self.draw_join_screen(&mut ui, w, h);
                true
            }
            Screen::Settings => {
                self.draw_settings_screen(&mut ui, w, h);
                true
            }
            Screen::Appearance => {
                self.draw_appearance_screen(&mut ui, w, h);
                true
            }
            Screen::ConfirmDelete => {
                self.draw_confirm_delete_screen(&mut ui, w, h);
                true
            }
            _ => false,
        };
        if menu_only {
            self.ui = ui;
            return;
        }
        self.draw_status_overlays(&mut ui, w, h);
        self.draw_gameplay_hud(&mut ui, w, h);
        match self.ui_state.screen.clone() {
            Screen::Playing
            | Screen::Title
            | Screen::NewWorld
            | Screen::CreatingWorld
            | Screen::Accounts
            | Screen::Moderation(_)
            | Screen::Mods
            | Screen::Packs
            | Screen::Join
            | Screen::Settings
            | Screen::Appearance
            | Screen::ConfirmDelete => {}
            Screen::Furnace(pos) => self.draw_furnace_screen(&mut ui, w, h, pos),
            Screen::Bloomery(pos) => self.draw_bloomery_screen(&mut ui, w, h, pos),
            Screen::Kiln(pos) => self.draw_kiln_screen(&mut ui, w, h, pos),
            Screen::Workbench(pos) => self.draw_workbench_screen(&mut ui, w, h, pos),
            Screen::Mod(idx) => self.draw_mod_screen(&mut ui, w, h, idx),
            Screen::SignEdit(_) => self.draw_sign_edit_screen(&mut ui, w, h),
            Screen::Stall(pos) => self.draw_stall_screen(&mut ui, w, h, pos),
            Screen::MobCargo(id) => self.draw_mob_cargo_screen(&mut ui, w, h, id),
            Screen::Chest(pos) => self.draw_chest_screen(&mut ui, w, h, pos),
            Screen::Offering(pos) => self.draw_offering_screen(&mut ui, w, h, pos),
            Screen::Inventory => self.draw_inventory_screen(&mut ui),
            Screen::Paused => self.draw_paused_screen(&mut ui, w, h),
            Screen::Dialog { npc, node_id, .. } => {
                self.draw_dialog_screen(&mut ui, w, h, npc, node_id)
            }
            Screen::Journal => self.draw_journal_screen(&mut ui, w, h),
            Screen::Loadout => self.draw_loadout_screen(&mut ui, w, h),
            Screen::Skills => self.draw_skills_screen(&mut ui, w, h),
            Screen::Dead => self.draw_dead_screen(&mut ui, w, h),
        }
        self.ui = ui;
    }
}

#[cfg(test)]
mod characterization {
    use super::{project_world_label, wrap_ui_status};
    use crate::ui::UiBatch;
    use glam::{Mat4, Vec3};

    #[test]
    fn world_labels_project_to_pixels_and_clip_offscreen_points() {
        assert_eq!(
            project_world_label(Mat4::IDENTITY, Vec3::ZERO, 800.0, 600.0),
            Some((400.0, 300.0))
        );
        assert_eq!(
            project_world_label(Mat4::IDENTITY, Vec3::new(2.0, 0.0, 0.0), 800.0, 600.0),
            None
        );
    }

    #[test]
    fn long_system_toasts_wrap_inside_the_viewport() {
        let lines = wrap_ui_status(
            "A FAINT STATIC CATCHES ON STONE; THE SIGNS HOLD A STEADY RHYTHM; LOOSE MINERAL GRAINS RING WHEN DISTURBED.",
            600.0,
            2.0,
            3,
        );
        assert_eq!(lines.len(), 3);
        assert!(
            lines
                .iter()
                .all(|line| UiBatch::text_width(2.0, line) <= 600.0)
        );
    }
}

mod dialogue_journal;
mod gameplay_hud;
mod loadout;
mod menu_layout;
mod menus_accounts;
mod menus_preferences;
mod menus_world;
mod pause_death;
mod script_panel;
mod settings_layout;
mod skills;
mod station_panels;
mod status_overlays;
mod storage_panels;
mod world_labels;
mod writing_panel;
