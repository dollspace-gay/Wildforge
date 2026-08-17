//! Inventory-screen composition.
//!
//! The inventory is one focused workspace: identity and equipment, crafting,
//! then storage. Detailed survival readouts and recipe discovery are secondary
//! views, opened deliberately instead of competing with the core task.

use super::ui::wrap_ui_status;
use super::*;

impl Game {
    pub(super) const DISCOVERY_ROWS: usize = 7;

    pub(super) fn discovery_record_rect(&self, row: usize) -> (f32, f32, f32, f32) {
        let panel = self.inventory_panel_rect();
        (
            panel.0 + 16.0,
            panel.1 + 82.0 + row as f32 * 42.0,
            292.0,
            38.0,
        )
    }

    pub(super) fn discovery_label_rect(&self) -> (f32, f32, f32, f32) {
        let panel = self.inventory_panel_rect();
        (panel.0 + 16.0, panel.1 + 47.0, 292.0, 28.0)
    }

    pub(super) fn discovery_button_rect(&self, button: usize) -> (f32, f32, f32, f32) {
        let panel = self.inventory_panel_rect();
        let widths = [126.0, 58.0, 58.0, 84.0, 126.0, 126.0];
        let mut x = panel.0 + 16.0;
        for width in widths.iter().take(button) {
            x += *width + 7.0;
        }
        (x, panel.1 + panel.3 - 40.0, widths[button], 28.0)
    }

    pub(super) fn sorted_discovery_records(&self) -> Vec<&crate::discovery::ObservationSummary> {
        let mut records = self.ui_state.discovery_records.iter().collect::<Vec<_>>();
        match self.ui_state.discovery_sort % 3 {
            0 => records.sort_by(|a, b| {
                a.phenomenon_id
                    .cmp(&b.phenomenon_id)
                    .then_with(|| b.day.cmp(&a.day))
                    .then_with(|| a.record_id.cmp(&b.record_id))
            }),
            1 => records.sort_by(|a, b| {
                a.category
                    .cmp(&b.category)
                    .then_with(|| a.phenomenon_id.cmp(&b.phenomenon_id))
                    .then_with(|| a.record_id.cmp(&b.record_id))
            }),
            _ => records.sort_by(|a, b| {
                b.day
                    .cmp(&a.day)
                    .then_with(|| b.record_id.cmp(&a.record_id))
            }),
        }
        records
    }

    fn draw_discovery_card(
        &self,
        ui: &mut UiBatch,
        record: Option<&crate::discovery::ObservationSummary>,
        rect: (f32, f32, f32, f32),
        heading: &str,
    ) {
        ui.rect(rect.0, rect.1, rect.2, rect.3, [0.025, 0.035, 0.04, 0.96]);
        ui.text_shadow(
            rect.0 + 10.0,
            rect.1 + 8.0,
            1.25,
            heading,
            [0.62, 0.86, 0.72, 1.0],
        );
        let Some(record) = record else {
            ui.text_shadow(
                rect.0 + 10.0,
                rect.1 + 34.0,
                1.2,
                "SELECT A RECORD",
                [0.58, 0.62, 0.65, 1.0],
            );
            return;
        };
        let title = record
            .label
            .as_deref()
            .unwrap_or(&record.phenomenon_id)
            .to_uppercase();
        ui.text_shadow(rect.0 + 10.0, rect.1 + 28.0, 1.35, &title, [1.0; 4]);
        let mut y = rect.1 + 49.0;
        for line in wrap_ui_status(&record.reading.to_uppercase(), rect.2 - 20.0, 1.05, 3) {
            ui.text_shadow(rect.0 + 10.0, y, 1.05, &line, [0.83, 0.88, 0.9, 1.0]);
            y += 15.0;
        }
        if let Some((property, value)) = record.properties.first() {
            let detail = format!("{}: {}", property, value).to_uppercase();
            let detail = wrap_ui_status(&detail, rect.2 - 20.0, 0.9, 1)
                .into_iter()
                .next()
                .unwrap_or_default();
            ui.text_shadow(
                rect.0 + 10.0,
                rect.1 + rect.3 - 55.0,
                0.9,
                &detail,
                [0.72, 0.86, 0.74, 1.0],
            );
        }
        let place = record
            .provenance
            .place
            .as_deref()
            .unwrap_or(&record.provenance.biome)
            .to_uppercase();
        ui.text_shadow(
            rect.0 + 10.0,
            rect.1 + rect.3 - 38.0,
            1.0,
            &format!(
                "{} · DAY {} {}",
                record.observer_name.to_uppercase(),
                record.day,
                record.season.to_uppercase()
            ),
            [0.67, 0.74, 0.78, 1.0],
        );
        ui.text_shadow(
            rect.0 + 10.0,
            rect.1 + rect.3 - 21.0,
            1.0,
            &format!(
                "{} · {}{}",
                place,
                if record.location.is_some() {
                    "LOCATED"
                } else {
                    "LOCATION WITHHELD"
                },
                if record.obsolete_content {
                    " · OBSOLETE"
                } else {
                    ""
                }
            ),
            [0.67, 0.74, 0.78, 1.0],
        );
    }

    fn draw_discovery_catalogue(&self, ui: &mut UiBatch) {
        let panel = self.inventory_panel_rect();
        let label_rect = self.discovery_label_rect();
        ui.rect(
            label_rect.0,
            label_rect.1,
            label_rect.2,
            label_rect.3,
            [0.025, 0.035, 0.04, 0.96],
        );
        let caret = if self.ui_state.discovery_label_focus && (self.time_abs * 2.0) as i32 % 2 == 0
        {
            "_"
        } else {
            ""
        };
        let label =
            if self.ui_state.discovery_label.is_empty() && !self.ui_state.discovery_label_focus {
                "NEXT READING LABEL (OPTIONAL)".to_string()
            } else {
                format!(
                    "LABEL: {}{caret}",
                    self.ui_state.discovery_label.to_uppercase()
                )
            };
        ui.text_shadow(
            label_rect.0 + 8.0,
            label_rect.1 + 8.0,
            1.05,
            &label,
            [0.76, 0.82, 0.85, 1.0],
        );

        let records = self.sorted_discovery_records();
        let start = self.ui_state.discovery_page * Self::DISCOVERY_ROWS;
        for (row, record) in records
            .iter()
            .skip(start)
            .take(Self::DISCOVERY_ROWS)
            .enumerate()
        {
            let rect = self.discovery_record_rect(row);
            let selected = self
                .ui_state
                .discovery_selected
                .contains(&Some(record.record_id));
            ui.rect(
                rect.0,
                rect.1,
                rect.2,
                rect.3,
                if selected {
                    [0.20, 0.38, 0.28, 0.98]
                } else if self.hit(rect) {
                    [0.18, 0.22, 0.24, 0.98]
                } else {
                    [0.055, 0.07, 0.08, 0.96]
                },
            );
            let label = record.label.as_deref().unwrap_or(&record.phenomenon_id);
            let count = records
                .iter()
                .filter(|candidate| candidate.phenomenon_id == record.phenomenon_id)
                .count();
            ui.text_shadow(
                rect.0 + 8.0,
                rect.1 + 6.0,
                1.05,
                &label.to_uppercase(),
                [1.0; 4],
            );
            ui.text_shadow(
                rect.0 + 8.0,
                rect.1 + 22.0,
                0.9,
                &format!(
                    "{} · {} READING{} · DAY {} · #{}",
                    record.category.to_uppercase(),
                    count,
                    if count == 1 { "" } else { "S" },
                    record.day,
                    record.record_id
                ),
                [0.65, 0.72, 0.76, 1.0],
            );
        }

        let selected = self.ui_state.discovery_selected.map(|id| {
            id.and_then(|id| {
                records
                    .iter()
                    .copied()
                    .find(|record| record.record_id == id)
            })
        });
        self.draw_discovery_card(
            ui,
            selected[0],
            (panel.0 + 326.0, panel.1 + 82.0, panel.2 - 342.0, 145.0),
            "COMPARISON A",
        );
        self.draw_discovery_card(
            ui,
            selected[1],
            (panel.0 + 326.0, panel.1 + 235.0, panel.2 - 342.0, 145.0),
            "COMPARISON B",
        );
        let comparison = match selected {
            [Some(a), Some(b)] if a.phenomenon_id != b.phenomenon_id => {
                "DIFFERENT PHENOMENA".to_string()
            }
            [Some(a), Some(b)] if a.reading != b.reading || a.properties != b.properties => {
                "DISAGREEMENT: RETAIN BOTH SIGNED READINGS".to_string()
            }
            [Some(_), Some(_)] => "READINGS AGREE WITHIN RECORDED PRECISION".to_string(),
            _ => "SELECT TWO RECORDS TO COMPARE".to_string(),
        };
        ui.text_shadow(
            panel.0 + 326.0,
            panel.1 + 391.0,
            0.9,
            &comparison,
            [0.72, 0.86, 0.74, 1.0],
        );

        let sort =
            ["SORT: ID", "SORT: TYPE", "SORT: DAY"][self.ui_state.discovery_sort as usize % 3];
        let page_count = records.len().div_ceil(Self::DISCOVERY_ROWS).max(1);
        let labels = [
            sort.to_string(),
            "<".into(),
            ">".into(),
            "SWAP".into(),
            if self.ui_state.discovery_include_location {
                "COPY: LOCATED".into()
            } else {
                "COPY: PRIVATE".into()
            },
            "COPY SELECTED".into(),
        ];
        for (button, label) in labels.iter().enumerate() {
            let rect = self.discovery_button_rect(button);
            Self::draw_button(ui, rect, label, self.hit(rect));
        }
        ui.text_shadow(
            panel.0 + panel.2 - 126.0,
            panel.1 + 57.0,
            1.0,
            &format!(
                "{}/{} · {}/{}",
                self.ui_state.discovery_page + 1,
                page_count,
                records.len(),
                self.ui_state.discovery_capacity
            ),
            [0.74, 0.8, 0.83, 1.0],
        );
    }

    fn draw_inventory_tab(
        &self,
        ui: &mut UiBatch,
        rect: (f32, f32, f32, f32),
        label: &str,
        active: bool,
    ) {
        let color = if active {
            [0.28, 0.42, 0.34, 0.98]
        } else if self.hit(rect) {
            [0.28, 0.30, 0.32, 0.98]
        } else {
            [0.16, 0.18, 0.20, 0.96]
        };
        ui.rect(rect.0, rect.1, rect.2, rect.3, color);
        if active {
            ui.rect(
                rect.0,
                rect.1 + rect.3 - 3.0,
                rect.2,
                3.0,
                [0.58, 0.9, 0.65, 1.0],
            );
        }
        let text_width = UiBatch::text_width(1.5, label);
        ui.text_shadow(
            rect.0 + (rect.2 - text_width) * 0.5,
            rect.1 + 8.0,
            1.5,
            label,
            [1.0; 4],
        );
    }

    fn draw_inventory_status(&self, ui: &mut UiBatch, rect: (f32, f32, f32, f32)) {
        ui.rect(rect.0, rect.1, rect.2, rect.3, [0.02, 0.03, 0.04, 0.94]);
        ui.text_shadow(rect.0 + 14.0, rect.1 + 12.0, 1.75, "NUTRITION", [1.0; 4]);

        let names = ["GRAIN", "VEG", "FRUIT", "FUNGI", "PROT"];
        let colors = [
            [0.85, 0.7, 0.25, 1.0],
            [0.35, 0.75, 0.3, 1.0],
            [0.85, 0.3, 0.3, 1.0],
            [0.6, 0.45, 0.3, 1.0],
            [0.8, 0.4, 0.35, 1.0],
        ];
        let bar_x = rect.0 + 90.0;
        let bar_width = 150.0;
        for i in 0..5 {
            let y = rect.1 + 43.0 + i as f32 * 26.0;
            ui.text_shadow(rect.0 + 14.0, y, 1.35, names[i], [0.92, 0.94, 0.96, 1.0]);
            ui.rect(bar_x, y + 1.0, bar_width, 9.0, [0.10, 0.11, 0.12, 0.95]);
            ui.rect(
                bar_x,
                y + 1.0,
                bar_width * (self.survival.nutrition[i] / 100.0),
                9.0,
                colors[i],
            );
        }

        let split_x = rect.0 + rect.2 * 0.52;
        ui.rect(
            split_x,
            rect.1 + 12.0,
            2.0,
            rect.3 - 24.0,
            [0.4, 0.43, 0.46, 0.45],
        );
        let info_x = split_x + 18.0;
        let bonus = (self.max_health() - MAX_HEALTH) as i32 / 2;
        ui.text_shadow(info_x, rect.1 + 17.0, 1.5, "VITALS", [1.0; 4]);
        ui.text_shadow(
            info_x,
            rect.1 + 47.0,
            1.4,
            &format!("MAX HEALTH +{bonus}"),
            [0.9, 0.92, 0.95, 1.0],
        );

        let tier = self.server.world.ire_tier();
        let tier_color = [
            [0.45, 0.75, 0.4, 1.0],
            [0.8, 0.75, 0.35, 1.0],
            [0.9, 0.55, 0.25, 1.0],
            [0.9, 0.3, 0.25, 1.0],
        ][tier];
        ui.text_shadow(
            info_x,
            rect.1 + 78.0,
            1.4,
            "THE WILD",
            [0.9, 0.92, 0.95, 1.0],
        );
        ui.rect(
            info_x + 96.0,
            rect.1 + 80.0,
            112.0,
            9.0,
            [0.10, 0.11, 0.12, 0.95],
        );
        ui.rect(
            info_x + 96.0,
            rect.1 + 80.0,
            self.server.world.ire * 1.12,
            9.0,
            tier_color,
        );
        ui.text_shadow(
            info_x,
            rect.1 + 105.0,
            1.4,
            world::IRE_TIERS[tier],
            tier_color,
        );

        let dross_text = match self.survival.preparation_modifiers.dross_band {
            1 => "TRACE / GLASS HAZE",
            2 => "STRAINED / TWO-PULSE",
            3 => "SEEP / BRANCHING",
            4 => "SCAR / BROKEN RING",
            5 => "BREACH / SHEAR",
            _ => "CLEAR / EVEN",
        };
        ui.text_shadow(
            info_x,
            rect.1 + 124.0,
            1.1,
            &format!("DROSS {dross_text}"),
            [0.78, 0.86, 0.92, 1.0],
        );

        let world = &self.server.world;
        let season = world.season_at_surface(self.player.pos.surface());
        let third = ["EARLY", "MID", "LATE"][((world.season_progress() * 3.0) as usize).min(2)];
        ui.text_shadow(
            info_x,
            rect.1 + 146.0,
            1.4,
            &format!("DAY {} - {third} {}", world.day + 1, world::SEASONS[season]),
            [0.78, 0.86, 1.0, 1.0],
        );
        let weather = world.weather_at_surface(self.player.pos.surface());
        ui.text_shadow(
            info_x,
            rect.1 + 164.0,
            1.0,
            &format!(
                "{}  {:+.0}C  WIND {:.1}",
                weather.kind.name().to_uppercase(),
                weather.temperature_c,
                weather.wind[0].hypot(weather.wind[1]),
            ),
            [0.68, 0.76, 0.86, 1.0],
        );
    }

    fn draw_inventory_gear(&self, ui: &mut UiBatch) {
        let panel = self.inventory_panel_rect();
        let avatar = self.inventory_avatar_rect();
        let first = self.craft_slot_rect(0);
        ui.rect(
            panel.0 + 8.0,
            panel.1 + 42.0,
            58.0,
            198.0,
            [0.02, 0.025, 0.03, 0.94],
        );
        ui.rect(
            first.0 - 16.0,
            panel.1 + 48.0,
            panel.0 + panel.2 - first.0,
            184.0,
            [0.02, 0.025, 0.03, 0.94],
        );
        ui.rect(
            avatar.0,
            avatar.1,
            avatar.2,
            avatar.3,
            [0.01, 0.02, 0.03, 0.55],
        );
        ui.rect(avatar.0, avatar.1, avatar.2, 2.0, [0.48, 0.52, 0.55, 0.72]);
        ui.rect(
            avatar.0,
            avatar.1 + avatar.3 - 2.0,
            avatar.2,
            2.0,
            [0.48, 0.52, 0.55, 0.72],
        );
        ui.rect(avatar.0, avatar.1, 2.0, avatar.3, [0.48, 0.52, 0.55, 0.72]);
        ui.rect(
            avatar.0 + avatar.2 - 2.0,
            avatar.1,
            2.0,
            avatar.3,
            [0.48, 0.52, 0.55, 0.72],
        );

        let (active_name, social_name) = self.selected_multiplayer_name();
        let mut active_name = active_name.to_uppercase();
        if active_name.chars().count() > 20 {
            active_name = format!("{}...", active_name.chars().take(17).collect::<String>());
        }
        let name_width = UiBatch::text_width(1.55, &active_name);
        ui.rect(
            avatar.0 + 3.0,
            avatar.1 + 3.0,
            avatar.2 - 6.0,
            if social_name { 34.0 } else { 22.0 },
            [0.01, 0.02, 0.03, 0.76],
        );
        ui.text_shadow(
            avatar.0 + (avatar.2 - name_width) * 0.5,
            avatar.1 + 7.0,
            1.55,
            &active_name,
            [0.72, 1.0, 0.78, 1.0],
        );
        if social_name
            && let Some(handle) = self
                .atproto_account
                .as_ref()
                .and_then(|account| account.handle.as_deref())
        {
            let mut handle = format!("@{handle}");
            if handle.chars().count() > 28 {
                handle = format!("{}...", handle.chars().take(25).collect::<String>());
            }
            let handle_width = UiBatch::text_width(1.05, &handle);
            ui.text_shadow(
                avatar.0 + (avatar.2 - handle_width) * 0.5,
                avatar.1 + 23.0,
                1.05,
                &handle,
                [0.65, 0.78, 1.0, 1.0],
            );
        }

        for (i, label) in ["H", "C", "L", "B", "*"].iter().enumerate() {
            let slot = self.armor_slot_rect(i);
            if i == 4 {
                ui.text_shadow(
                    slot.0 - 1.0,
                    slot.1 - 15.0,
                    1.0,
                    "CHARM",
                    [0.72, 0.75, 0.78, 1.0],
                );
            }
            Self::draw_slot(
                &self.content.reg,
                ui,
                slot,
                self.survival.armor[i],
                false,
                self.hit(slot),
            );
            if self.survival.armor[i].is_none() {
                ui.text_shadow(
                    slot.0 + slot.2 * 0.5 - 5.0,
                    slot.1 + slot.3 * 0.5 - 7.0,
                    2.0,
                    label,
                    [0.55, 0.55, 0.55, 0.8],
                );
            }
        }

        let count = self.interaction.craft_size * self.interaction.craft_size;
        ui.text_shadow(
            first.0,
            first.1 - 22.0,
            1.35,
            "CRAFT",
            [0.72, 0.75, 0.78, 1.0],
        );
        for i in 0..count {
            let slot = self.craft_slot_rect(i);
            Self::draw_slot(
                &self.content.reg,
                ui,
                slot,
                self.interaction.craft_grid[i],
                false,
                self.hit(slot),
            );
        }
        let result_slot = self.result_slot_rect();
        ui.text_shadow(
            result_slot.0 - 34.0,
            result_slot.1 + 16.0,
            2.5,
            "-",
            [1.0; 4],
        );
        ui.text_shadow(
            result_slot.0 - 24.0,
            result_slot.1 + 14.0,
            2.5,
            ">",
            [1.0; 4],
        );
        let result =
            crafting::match_repair(&self.content.reg, &self.interaction.craft_grid[..count])
                .map(|repair| repair.output)
                .or_else(|| {
                    crafting::match_recipe(
                        &self.content.reg,
                        &self.interaction.craft_grid[..count],
                        self.interaction.craft_size,
                    )
                    .map(|recipe| ItemStack::new(&self.content.reg, recipe.output, recipe.count))
                });
        Self::draw_slot(
            &self.content.reg,
            ui,
            result_slot,
            result,
            false,
            self.hit(result_slot),
        );
        // Spec 3.5: a locked recipe's preview shows a dimmed lock badge.
        let locked = crafting::match_recipe(
            &self.content.reg,
            &self.interaction.craft_grid[..count],
            self.interaction.craft_size,
        )
        .is_some_and(|r| self.recipe_locked(r));
        if locked {
            ui.rect(
                result_slot.0,
                result_slot.1,
                result_slot.2,
                result_slot.3,
                [0.0, 0.0, 0.0, 0.55],
            );
            ui.text_shadow(
                result_slot.0 + result_slot.2 / 2.0 - 24.0,
                result_slot.1 + result_slot.3 / 2.0 - 8.0,
                1.5,
                "LOCKED",
                [1.0, 0.8, 0.3, 1.0],
            );
        }
    }

    pub(super) fn draw_inventory_screen(&self, ui: &mut UiBatch) {
        let width = self.renderer.config.width as f32;
        let height = self.renderer.config.height as f32;
        ui.rect(0.0, 0.0, width, height, [0.0, 0.0, 0.0, 0.48]);

        let panel = self.inventory_panel_rect();
        ui.rect(
            panel.0,
            panel.1,
            panel.2,
            panel.3,
            [0.055, 0.065, 0.075, 0.38],
        );
        ui.rect(panel.0, panel.1, panel.2, 42.0, [0.06, 0.07, 0.08, 0.94]);
        ui.rect(panel.0, panel.1, panel.2, 2.0, [0.55, 0.58, 0.60, 0.78]);
        ui.rect(
            panel.0,
            panel.1 + panel.3 - 2.0,
            panel.2,
            2.0,
            [0.18, 0.20, 0.22, 0.9],
        );
        ui.text_shadow(panel.0 + 14.0, panel.1 + 13.0, 1.8, "INVENTORY", [1.0; 4]);

        let gear_tab = self.inventory_tab_rect(0);
        let status_tab = self.inventory_tab_rect(1);
        let recipe_tab = self.inventory_tab_rect(2);
        let discovery_tab = self.inventory_tab_rect(3);
        self.draw_inventory_tab(
            ui,
            gear_tab,
            "GEAR",
            !self.ui_state.inventory_status_open
                && !self.ui_state.inventory_browser_open
                && !self.ui_state.inventory_discovery_open,
        );
        self.draw_inventory_tab(
            ui,
            status_tab,
            "STATUS",
            self.ui_state.inventory_status_open,
        );
        self.draw_inventory_tab(
            ui,
            recipe_tab,
            "RECIPES",
            self.ui_state.inventory_browser_open,
        );
        self.draw_inventory_tab(
            ui,
            discovery_tab,
            "RECORDS",
            self.ui_state.inventory_discovery_open,
        );

        if self.ui_state.inventory_discovery_open {
            self.draw_discovery_catalogue(ui);
            return;
        }

        if self.ui_state.inventory_status_open {
            self.draw_inventory_status(ui, (panel.0 + 16.0, panel.1 + 48.0, panel.2 - 32.0, 184.0));
        } else {
            self.draw_inventory_gear(ui);
        }

        let (_, grid_y, _, _) = self.inv_slot_rect(HOTBAR_SLOTS);
        ui.rect(
            panel.0 + 8.0,
            grid_y - 8.0,
            panel.2 - 16.0,
            panel.1 + panel.3 - grid_y,
            [0.02, 0.025, 0.03, 0.94],
        );
        for i in 0..TOTAL_SLOTS {
            let slot = self.inv_slot_rect(i);
            Self::draw_slot(
                &self.content.reg,
                ui,
                slot,
                self.inventory.slots[i],
                i == self.input.hotbar_sel,
                self.hit(slot),
            );
        }

        if self.ui_state.inventory_browser_open {
            self.draw_browser(ui);
        }

        if let Some(stack) = self.ui_state.held_stack {
            let (cursor_x, cursor_y) = self.input.ui_cursor;
            let icon = self.content.reg.item(stack.item).icon;
            ui.tile(cursor_x - 16.0, cursor_y - 16.0, 32.0, 32.0, icon, [1.0; 4]);
            if stack.count > 1 {
                ui.text_shadow(
                    cursor_x + 6.0,
                    cursor_y + 4.0,
                    2.0,
                    &format!("{}", stack.count),
                    [1.0; 4],
                );
            }
        }
    }
}
