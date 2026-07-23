//! Item browser layout, drawing, search, and navigation.

use super::*;

impl Game {
    pub(super) fn browser_origin(&self) -> (f32, f32) {
        let width = self.renderer.config.width as f32;
        let browser_width = Self::BCOLS as f32 * Self::BSLOT;
        let right_aligned = width - browser_width - 20.0;
        let x = if self.ui_state.screen == Screen::Inventory {
            let (panel_x, _, panel_width, _) = self.inventory_panel_rect();
            (panel_x + panel_width + 16.0).min(right_aligned)
        } else {
            right_aligned
        };
        (x.max(20.0), 96.0)
    }

    pub(super) fn browser_cell(&self, i: usize) -> (f32, f32, f32, f32) {
        let (x0, y0) = self.browser_origin();
        (
            x0 + (i % Self::BCOLS) as f32 * Self::BSLOT,
            y0 + (i / Self::BCOLS) as f32 * Self::BSLOT,
            Self::BSLOT,
            Self::BSLOT,
        )
    }

    pub(super) fn browser_search_rect(&self) -> (f32, f32, f32, f32) {
        let (x0, y0) = self.browser_origin();
        (x0, y0 - 34.0, Self::BCOLS as f32 * Self::BSLOT, 26.0)
    }

    pub(super) fn browser_nav_rect(&self, next: bool) -> (f32, f32, f32, f32) {
        let (x0, y0) = self.browser_origin();
        let y = y0 + Self::BROWS as f32 * Self::BSLOT + 8.0;
        if next {
            (x0 + Self::BCOLS as f32 * Self::BSLOT - 40.0, y, 40.0, 26.0)
        } else {
            (x0, y, 40.0, 26.0)
        }
    }

    pub(super) fn draw_browser(&self, ui: &mut UiBatch) {
        let reg = &self.content.reg;
        let (panel_x, panel_y) = self.browser_origin();
        ui.rect(
            panel_x - 10.0,
            panel_y - 44.0,
            Self::BCOLS as f32 * Self::BSLOT + 20.0,
            Self::BROWS as f32 * Self::BSLOT + 88.0,
            [0.04, 0.05, 0.06, 0.88],
        );
        let sr = self.browser_search_rect();
        ui.rect(sr.0, sr.1, sr.2, sr.3, [0.08, 0.08, 0.08, 0.95]);
        let caret = if self.ui_state.search_focus && (self.time_abs * 2.0) as i32 % 2 == 0 {
            "_"
        } else {
            ""
        };
        ui.text_shadow(
            sr.0 + 6.0,
            sr.1 + 6.0,
            2.0,
            &format!("{}{caret}", self.ui_state.search.to_uppercase()),
            [1.0; 4],
        );
        if self.ui_state.search.is_empty() && !self.ui_state.search_focus {
            ui.text_shadow(sr.0 + 6.0, sr.1 + 6.0, 2.0, "SEARCH", [0.5, 0.5, 0.5, 1.0]);
        }
        let items = browser_items(reg, &self.ui_state.search);
        let per = Self::BCOLS * Self::BROWS;
        let pages = items.len().div_ceil(per).max(1);
        let page = self.ui_state.browse_page.min(pages - 1);
        for (ci, item) in items.iter().skip(page * per).take(per).enumerate() {
            let r = self.browser_cell(ci);
            let hov = self.hit(r);
            ui.rect(
                r.0 + 1.0,
                r.1 + 1.0,
                r.2 - 2.0,
                r.3 - 2.0,
                if hov {
                    [0.4, 0.4, 0.4, 0.85]
                } else {
                    [0.16, 0.16, 0.16, 0.85]
                },
            );
            let icon = reg.item(*item).icon;
            ui.tile(r.0 + 5.0, r.1 + 5.0, 30.0, 30.0, icon, [1.0; 4]);
            if hov {
                ui.text_shadow(
                    r.0 - 120.0,
                    r.1 + 12.0,
                    1.5,
                    &reg.item(*item).label.to_uppercase(),
                    [1.0, 1.0, 0.7, 1.0],
                );
            }
        }
        for (next, lbl) in [(false, "<"), (true, ">")] {
            let r = self.browser_nav_rect(next);
            Self::draw_button(ui, r, lbl, self.hit(r));
        }
        let (x0, _) = self.browser_origin();
        let y = self.browser_nav_rect(false).1 + 5.0;
        ui.text_shadow(
            x0 + 90.0,
            y,
            2.0,
            &format!("{}/{pages}", page + 1),
            [1.0; 4],
        );

        // Recipe overlay.
        if let Some((item, uses)) = self.ui_state.browse_view {
            let (px, py) = (40.0, 96.0);
            let pw = 380.0;
            ui.rect(px - 10.0, py - 40.0, pw, 460.0, [0.05, 0.05, 0.08, 0.96]);
            ui.text_shadow(
                px,
                py - 30.0,
                2.0,
                &reg.item(item).label.to_uppercase(),
                [1.0, 1.0, 0.6, 1.0],
            );
            for (ti, lbl) in ["RECIPES", "USES"].iter().enumerate() {
                let r = (px + 150.0 + ti as f32 * 90.0, py - 34.0, 84.0, 24.0);
                Self::draw_button(ui, r, lbl, (ti == 1) == uses);
            }
            let cycle = (self.time_abs / 0.8) as usize;
            let mut y = py + 8.0;
            let (recipes, smelts) = if uses {
                let (r, s, fuel) = reg.uses_of(item);
                if fuel {
                    ui.text_shadow(px, y, 2.0, "USABLE AS FURNACE FUEL", [1.0, 0.8, 0.4, 1.0]);
                    y += 26.0;
                }
                (r, s)
            } else {
                (reg.recipes_for(item), reg.smelts_for(item))
            };
            for r in recipes.iter().take(3) {
                for cy in 0..r.h {
                    for cx in 0..r.w {
                        let cell = (px + cx as f32 * 38.0, y + cy as f32 * 38.0, 36.0, 36.0);
                        ui.rect(cell.0, cell.1, cell.2, cell.3, [0.18, 0.18, 0.18, 0.9]);
                        if let Some(ing) = &r.pattern[cy * r.w + cx] {
                            let show = match ing {
                                crate::registry::Ingredient::One(i) => *i,
                                crate::registry::Ingredient::Any(l) => l[cycle % l.len()],
                            };
                            let ic = reg.item(show).icon;
                            ui.tile(cell.0 + 3.0, cell.1 + 3.0, 30.0, 30.0, ic, [1.0; 4]);
                        }
                    }
                }
                let oy = y + (r.h as f32 * 38.0 - 36.0) / 2.0;
                ui.text_shadow(px + 126.0, oy + 12.0, 2.5, ">", [1.0; 4]);
                ui.rect(px + 150.0, oy, 36.0, 36.0, [0.22, 0.22, 0.22, 0.9]);
                let oc = reg.item(r.output).icon;
                ui.tile(px + 153.0, oy + 3.0, 30.0, 30.0, oc, [1.0; 4]);
                if r.count > 1 {
                    ui.text_shadow(
                        px + 172.0,
                        oy + 22.0,
                        2.0,
                        &format!("{}", r.count),
                        [1.0; 4],
                    );
                }
                y += r.h as f32 * 38.0 + 14.0;
            }
            for s in smelts.iter().take(2) {
                ui.text_shadow(px, y + 10.0, 2.0, "SMELT", [1.0, 0.6, 0.2, 1.0]);
                let show = match &s.input {
                    crate::registry::Ingredient::One(i) => *i,
                    crate::registry::Ingredient::Any(l) => l[cycle % l.len()],
                };
                for (sx, it2) in [(90.0, show), (170.0, s.output)] {
                    ui.rect(px + sx, y, 36.0, 36.0, [0.2, 0.2, 0.2, 0.9]);
                    let ic = reg.item(it2).icon;
                    ui.tile(px + sx + 3.0, y + 3.0, 30.0, 30.0, ic, [1.0; 4]);
                }
                ui.text_shadow(px + 140.0, y + 12.0, 2.5, ">", [1.0; 4]);
                y += 50.0;
            }
            if recipes.is_empty() && smelts.is_empty() {
                ui.text_shadow(px, y, 2.0, "NOTHING HERE", [0.6, 0.6, 0.6, 1.0]);
            }
            if !self.ui_state.browse_back.is_empty() {
                let r = (px, py + 370.0, 84.0, 24.0);
                Self::draw_button(ui, r, "BACK", self.hit(r));
            }
        }
    }

    /// Returns true if the click was handled by the browser.
    pub(super) fn browser_click(&mut self, right: bool) -> bool {
        if self.hit(self.browser_search_rect()) {
            self.ui_state.search_focus = true;
            return true;
        }
        self.ui_state.search_focus = false;
        for (next, _) in [(false, ()), (true, ())] {
            if self.hit(self.browser_nav_rect(next)) {
                let items = browser_items(&self.content.reg, &self.ui_state.search);
                let pages = items.len().div_ceil(Self::BCOLS * Self::BROWS).max(1);
                self.ui_state.browse_page = if next {
                    (self.ui_state.browse_page + 1).min(pages - 1)
                } else {
                    self.ui_state.browse_page.saturating_sub(1)
                };
                return true;
            }
        }
        if let Some((item, uses)) = self.ui_state.browse_view {
            let (px, py) = (40.0, 96.0);
            for ti in 0..2 {
                let r = (px + 150.0 + ti as f32 * 90.0, py - 34.0, 84.0, 24.0);
                if self.hit(r) {
                    self.ui_state.browse_view = Some((item, ti == 1));
                    return true;
                }
            }
            if !self.ui_state.browse_back.is_empty() {
                let r = (px, py + 370.0, 84.0, 24.0);
                if self.hit(r) {
                    self.ui_state.browse_view = self.ui_state.browse_back.pop();
                    return true;
                }
            }
            let _ = uses;
            if self.hit((px - 10.0, py - 40.0, 380.0, 460.0)) {
                return true; // swallow clicks inside the panel
            }
            self.ui_state.browse_view = None;
            return true;
        }
        let items = browser_items(&self.content.reg, &self.ui_state.search);
        let per = Self::BCOLS * Self::BROWS;
        let page = self
            .ui_state
            .browse_page
            .min(items.len().div_ceil(per).max(1) - 1);
        for (ci, item) in items.iter().skip(page * per).take(per).enumerate() {
            if self.hit(self.browser_cell(ci)) {
                if self.creative {
                    let reg = self.content.reg.clone();
                    let n = if right { 1 } else { reg.item(*item).max_stack };
                    self.ui_state.held_stack = Some(ItemStack::new(&reg, *item, n));
                } else {
                    self.ui_state.browse_back.clear();
                    self.ui_state.browse_view = Some((*item, false));
                }
                return true;
            }
        }
        false
    }
}
