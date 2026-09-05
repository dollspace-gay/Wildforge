//! Shared UI paint operations over explicit read-only inputs.

use crate::inventory::ItemStack;
use crate::registry::Registry;
use crate::ui::UiBatch;

pub(super) type Rect = (f32, f32, f32, f32);

pub(super) fn hit(cursor: (f32, f32), r: Rect) -> bool {
    let (x, y) = cursor;
    x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3
}

pub(super) fn button(ui: &mut UiBatch, r: (f32, f32, f32, f32), label: &str, hover: bool) {
    // Hover grows the plate 4% and brightens; a press dips it 2% —
    // the down-up is what makes a click feel mechanical.
    let mut r = r;
    if hover {
        let sc = if ui.press_dip { 0.98 } else { 1.04 };
        let (cx, cy) = (r.0 + r.2 / 2.0, r.1 + r.3 / 2.0);
        r = (cx - r.2 / 2.0 * sc, cy - r.3 / 2.0 * sc, r.2 * sc, r.3 * sc);
    }
    let bg = if hover {
        [0.55, 0.55, 0.55, 0.95]
    } else {
        [0.25, 0.25, 0.25, 0.95]
    };
    ui.rect(r.0, r.1, r.2, r.3, [0.1, 0.1, 0.1, 0.95]);
    ui.rect(r.0 + 2.0, r.1 + 2.0, r.2 - 4.0, r.3 - 4.0, bg);
    let lw = UiBatch::text_width(2.0, label);
    ui.text_shadow(
        r.0 + (r.2 - lw) / 2.0,
        r.1 + (r.3 - 14.0) / 2.0,
        2.0,
        label,
        [1.0; 4],
    );
}

pub(super) fn slot(
    reg: &Registry,
    ui: &mut UiBatch,
    r: (f32, f32, f32, f32),
    stack: Option<ItemStack>,
    selected: bool,
    hover: bool,
) {
    let (x, y, w, h) = r;
    let border = if selected {
        [1.0, 1.0, 1.0, 0.9]
    } else {
        [0.35, 0.35, 0.35, 0.9]
    };
    ui.rect(x + 1.0, y + 1.0, w - 2.0, h - 2.0, border);
    let bg = if hover {
        [0.45, 0.45, 0.45, 0.92]
    } else {
        [0.18, 0.18, 0.18, 0.92]
    };
    ui.rect(x + 3.0, y + 3.0, w - 6.0, h - 6.0, bg);
    if let Some(s) = stack {
        let pad = 8.0;
        let icon = reg.item(s.item).icon;
        let tile = icon;
        ui.tile(
            x + pad,
            y + pad,
            w - 2.0 * pad,
            h - 2.0 * pad,
            tile,
            [1.0; 4],
        );
        if s.count > 1 {
            let txt = format!("{}", s.count);
            let tw = UiBatch::text_width(2.0, &txt);
            ui.text_shadow(x + w - tw - 4.0, y + h - 18.0, 2.0, &txt, [1.0; 4]);
        }
        // Durability bar for worn tools.
        let max = reg.item(s.item).durability;
        if max > 0 && s.durability < max {
            let frac = s.durability as f32 / max as f32;
            ui.rect(x + 6.0, y + h - 9.0, w - 12.0, 4.0, [0.05, 0.05, 0.05, 0.9]);
            ui.rect(
                x + 6.0,
                y + h - 9.0,
                (w - 12.0) * frac,
                4.0,
                [1.0 - frac, frac, 0.1, 1.0],
            );
        }
    }
}

pub(super) fn held_stack(reg: &Registry, ui: &mut UiBatch, cursor: (f32, f32), stack: Option<ItemStack>) {
    if let Some(s) = stack {
        let (cx, cy) = cursor;
        let icon = reg.item(s.item).icon;
        ui.tile(cx - 16.0, cy - 16.0, 32.0, 32.0, icon, [1.0; 4]);
        if s.count > 1 {
            ui.text_shadow(cx + 6.0, cy + 4.0, 2.0, &format!("{}", s.count), [1.0; 4]);
        }
    }
}
