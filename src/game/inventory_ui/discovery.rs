//! Discovery graphical inventory ui adapter.

use crate::game::widgets;
use crate::game::ui::wrap_ui_status;
use crate::ui::UiBatch;
use crate::game::Game;

impl Game {
    pub(in crate::game) fn discovery_record_rect(&self, row: usize) -> (f32, f32, f32, f32) {
        let panel = self.inventory_layout().panel_rect();
        (
            panel.0 + 16.0,
            panel.1 + 82.0 + row as f32 * 42.0,
            292.0,
            38.0,
        )
    }

    pub(in crate::game) fn discovery_label_rect(&self) -> (f32, f32, f32, f32) {
        let panel = self.inventory_layout().panel_rect();
        (panel.0 + 16.0, panel.1 + 47.0, 292.0, 28.0)
    }

    pub(in crate::game) fn discovery_button_rect(&self, button: usize) -> (f32, f32, f32, f32) {
        let panel = self.inventory_layout().panel_rect();
        let widths = [126.0, 58.0, 58.0, 84.0, 126.0, 126.0];
        let mut x = panel.0 + 16.0;
        for width in widths.iter().take(button) {
            x += *width + 7.0;
        }
        (x, panel.1 + panel.3 - 40.0, widths[button], 28.0)
    }

    pub(in crate::game) fn sorted_discovery_records(&self) -> Vec<&crate::discovery::ObservationSummary> {
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

    pub(super) fn draw_discovery_card(
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

    pub(super) fn draw_discovery_catalogue(&self, ui: &mut UiBatch) {
        let panel = self.inventory_layout().panel_rect();
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
            widgets::button(ui, rect, label, self.hit(rect));
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
}
