//! Inventory menu actions.

use crate::audio::Sfx;
use crate::inventory::TOTAL_SLOTS;
use crate::game::Game;

impl Game {
    pub(in crate::game) fn click_inventory_menu(&mut self , right: bool) {
                if self.hit(self.inventory_layout().tab_rect(0)) {
                    self.sfx(Sfx::Click);
                    self.ui_state.inventory_status_open = false;
                    self.ui_state.inventory_browser_open = false;
                    self.ui_state.inventory_discovery_open = false;
                    self.ui_state.discovery_label_focus = false;
                    self.ui_state.search_focus = false;
                    self.ui_state.browse_view = None;
                    return;
                }
                if self.hit(self.inventory_layout().tab_rect(1)) {
                    self.sfx(Sfx::Click);
                    self.ui_state.inventory_status_open = true;
                    self.ui_state.inventory_browser_open = false;
                    self.ui_state.inventory_discovery_open = false;
                    self.ui_state.discovery_label_focus = false;
                    self.ui_state.search_focus = false;
                    self.ui_state.browse_view = None;
                    return;
                }
                if self.hit(self.inventory_layout().tab_rect(2)) {
                    self.sfx(Sfx::Click);
                    self.ui_state.inventory_browser_open = !self.ui_state.inventory_browser_open;
                    self.ui_state.inventory_status_open = false;
                    self.ui_state.inventory_discovery_open = false;
                    self.ui_state.discovery_label_focus = false;
                    if !self.ui_state.inventory_browser_open {
                        self.ui_state.search_focus = false;
                        self.ui_state.browse_view = None;
                    }
                    return;
                }
                if self.hit(self.inventory_layout().tab_rect(3)) {
                    self.sfx(Sfx::Click);
                    self.ui_state.inventory_discovery_open = true;
                    self.ui_state.inventory_status_open = false;
                    self.ui_state.inventory_browser_open = false;
                    self.ui_state.search_focus = false;
                    self.ui_state.browse_view = None;
                    return;
                }
                if self.ui_state.inventory_browser_open && self.browser_click(right) {
                    return;
                }
                if self.ui_state.inventory_discovery_open {
                    if self.hit(self.discovery_label_rect()) {
                        self.ui_state.discovery_label_focus = true;
                        self.sfx(Sfx::Click);
                        return;
                    }
                    self.ui_state.discovery_label_focus = false;
                    if self.hit(self.discovery_button_rect(0)) {
                        self.ui_state.discovery_sort = (self.ui_state.discovery_sort + 1) % 3;
                        self.ui_state.discovery_page = 0;
                        self.sfx(Sfx::Click);
                        return;
                    }
                    if self.hit(self.discovery_button_rect(1)) {
                        self.ui_state.discovery_page =
                            self.ui_state.discovery_page.saturating_sub(1);
                        self.sfx(Sfx::Click);
                        return;
                    }
                    if self.hit(self.discovery_button_rect(2)) {
                        let pages = self
                            .ui_state
                            .discovery_records
                            .len()
                            .div_ceil(Self::DISCOVERY_ROWS)
                            .max(1);
                        self.ui_state.discovery_page =
                            (self.ui_state.discovery_page + 1).min(pages - 1);
                        self.sfx(Sfx::Click);
                        return;
                    }
                    if self.hit(self.discovery_button_rect(3)) {
                        self.swap_discovery_copy_direction();
                        self.sfx(Sfx::Click);
                        return;
                    }
                    if self.hit(self.discovery_button_rect(4)) {
                        self.ui_state.discovery_include_location =
                            !self.ui_state.discovery_include_location;
                        self.sfx(Sfx::Click);
                        return;
                    }
                    if self.hit(self.discovery_button_rect(5)) {
                        self.copy_selected_discovery_record();
                        self.sfx(Sfx::Click);
                        return;
                    }
                    for row in 0..Self::DISCOVERY_ROWS {
                        if !self.hit(self.discovery_record_rect(row)) {
                            continue;
                        }
                        let index = self.ui_state.discovery_page * Self::DISCOVERY_ROWS + row;
                        let record_id = self
                            .sorted_discovery_records()
                            .get(index)
                            .map(|record| record.record_id);
                        if let Some(record_id) = record_id {
                            if self.ui_state.discovery_selected[0] == Some(record_id) {
                                self.ui_state.discovery_selected[0] = None;
                            } else if self.ui_state.discovery_selected[1] == Some(record_id) {
                                self.ui_state.discovery_selected[1] = None;
                            } else if self.ui_state.discovery_selected[0].is_none() {
                                self.ui_state.discovery_selected[0] = Some(record_id);
                            } else if self.ui_state.discovery_selected[1].is_none() {
                                self.ui_state.discovery_selected[1] = Some(record_id);
                            } else {
                                self.ui_state.discovery_selected[0] =
                                    self.ui_state.discovery_selected[1];
                                self.ui_state.discovery_selected[1] = Some(record_id);
                            }
                            self.sfx(Sfx::Click);
                        }
                        return;
                    }
                    return;
                }
                if !self.ui_state.inventory_status_open {
                    for i in 0..5 {
                        if self.hit(self.armor_slot_rect(i)) {
                            self.armor_click(i);
                            return;
                        }
                    }
                }
                for i in 0..TOTAL_SLOTS {
                    if self.hit(self.inventory_layout().slot_rect(i)) {
                        self.inventory_click(false, i, right);
                        return;
                    }
                }
                if !self.ui_state.inventory_status_open {
                    for i in 0..self.interaction.craft_size * self.interaction.craft_size {
                        if self.hit(self.inventory_layout().craft_slot_rect(i)) {
                            self.inventory_click(true, i, right);
                            return;
                        }
                    }
                    if self.hit(self.inventory_layout().result_slot_rect()) {
                        self.result_click();
                    }
                }
                }
}
