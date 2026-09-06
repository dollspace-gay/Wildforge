//! Hover tooltips for item slots.
//!
//! The lines are *derived* from what an item actually does rather than
//! written out in TOML beside it. A charm's line comes from its effect,
//! a tool's from its tier and speed, a food's from its hunger and the
//! nutrient groups it fills. That way a modded item explains itself for
//! free and a shipped one can never drift out of date with its own
//! numbers — the failure mode of a hand-written `desc` field is a lie,
//! and a lie about a mechanic is worse than silence.

use super::Game;
use super::navigation::Screen;
use crate::crafting;
use crate::inventory::ItemStack;
use crate::inventory::TOTAL_SLOTS;
use crate::registry::{ArmorSlot, NUTRIENTS, Registry, ToolKind};
use crate::ui::UiBatch;
use crate::world;

const TITLE: [f32; 4] = [1.0, 0.98, 0.92, 1.0];
const BODY: [f32; 4] = [0.72, 0.76, 0.80, 1.0];
/// What the thing does for you, as opposed to what it is.
const EFFECT: [f32; 4] = [0.66, 0.88, 0.70, 1.0];
const WEAR: [f32; 4] = [0.80, 0.72, 0.56, 1.0];

fn tool_name(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::Pickaxe => "PICKAXE",
        ToolKind::Axe => "AXE",
        ToolKind::Shovel => "SHOVEL",
        ToolKind::Hoe => "HOE",
    }
}

fn armor_name(slot: ArmorSlot) -> &'static str {
    match slot {
        ArmorSlot::Head => "HEAD",
        ArmorSlot::Chest => "CHEST",
        ArmorSlot::Legs => "LEGS",
        ArmorSlot::Feet => "FEET",
    }
}

/// A charm is the one thing whose effect is invisible in its numbers:
/// it has no damage, no durability, no block it places. Left to the
/// derived lines it would say nothing at all, which is exactly what the
/// game said about it before.
fn charm_line(effect: &str) -> Option<&'static str> {
    Some(match effect {
        "quiet" => "WORN: THE WILD MINDS YOU LESS",
        "bark" => "WORN: +1 ARMOUR AGAINST THE WILD",
        "hunger" => "WORN: HUNGER FALLS 15% SLOWER",
        _ => return None,
    })
}

/// Trim a float to at most one decimal, without a trailing ".0".
fn num(v: f32) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.1}")
    }
}

/// Every line of an item's tooltip, top to bottom, with its color.
/// Pure so the wording can be tested without a window.
#[cfg(test)]
pub fn item_tooltip_lines(reg: &Registry, stack: ItemStack) -> Vec<(String, [f32; 4])> {
    item_tooltip_lines_with_current(reg, stack, None)
}

fn item_tooltip_lines_with_current(
    reg: &Registry,
    stack: ItemStack,
    current_units: Option<u64>,
) -> Vec<(String, [f32; 4])> {
    let d = reg.item(stack.item);
    let mut lines = vec![(d.label.to_uppercase(), TITLE)];

    if let Some(block) = d.places {
        lines.push((
            format!("PLACES {}", reg.block(block).label.to_uppercase()),
            BODY,
        ));
    }
    if let Some((kind, speed, tier)) = d.tool {
        lines.push((
            format!("{} - TIER {} - {}X", tool_name(kind), tier, num(speed)),
            BODY,
        ));
    }
    if let Some(bow) = &d.bow {
        lines.push((
            format!(
                "DRAWN: {} DAMAGE, SPEED {}",
                num(bow.damage),
                num(bow.speed)
            ),
            EFFECT,
        ));
    } else if d.damage > 1.0 {
        lines.push((format!("{} DAMAGE", num(d.damage)), EFFECT));
    }
    if let Some(ammo) = &d.ammo {
        lines.push((format!("AMMO: {}", ammo.to_uppercase()), BODY));
    }
    if let Some((slot, points)) = d.armor {
        // The same 4%-per-point the damage path applies, said out loud.
        lines.push((
            format!(
                "{} - {} ARMOUR ({}% LESS)",
                armor_name(slot),
                points,
                (points * 4).min(60)
            ),
            EFFECT,
        ));
    }
    if let Some(food) = &d.food {
        lines.push((format!("EATS: {} HUNGER", num(food.hunger)), EFFECT));
        let groups: Vec<&str> = NUTRIENTS
            .iter()
            .zip(food.nutrition.iter())
            .filter(|(_, v)| **v > 0.0)
            .map(|(n, _)| *n)
            .collect();
        if !groups.is_empty() {
            lines.push((format!("FEEDS: {}", groups.join(", ").to_uppercase()), BODY));
        }
    }
    if let Some(effect) = d.charm.as_deref().and_then(charm_line) {
        lines.push((effect.to_string(), EFFECT));
    }
    if d.implement
        .as_ref()
        .is_some_and(|implement| implement.kind == crate::implements::ImplementItemKind::Wand)
    {
        lines.push(("WAND WORKINGS (TARGET + HOLD USE)".into(), EFFECT));
        let labels = reg
            .workings
            .values()
            .filter(|working| working.mode == crate::workings::DeliveryMode::Wand)
            .map(|working| working.label.to_uppercase())
            .collect::<Vec<_>>();
        for group in labels.chunks(4) {
            lines.push((group.join(" / "), BODY));
        }
        lines.push(("RELEASE COMMITS / CTRL + USE FORCES OVERDRAW".into(), WEAR));
    }
    if d.places
        .is_some_and(|block| reg.block(block).interaction.as_deref() == Some("binding_frame"))
    {
        lines.push(("CONSTRUCTED RITUALS".into(), EFFECT));
        let labels = reg
            .workings
            .values()
            .filter(|working| working.mode == crate::workings::DeliveryMode::Ritual)
            .map(|working| working.label.to_uppercase())
            .collect::<Vec<_>>();
        for group in labels.chunks(3) {
            lines.push((group.join(" / "), BODY));
        }
    }
    if d.bedroll {
        lines.push(("USE: SLEEP TO DAWN, SET SPAWN".into(), EFFECT));
    }
    if d.shears {
        lines.push(("CUTS LEAVES WHOLE".into(), EFFECT));
    }
    if d.brush_tool {
        lines.push(("USE: SWEEP REMNANTS".into(), EFFECT));
    }
    if d.hammer {
        lines.push(("WORKS BLOOMS ON AN ANVIL".into(), EFFECT));
    }
    if d.tablet {
        lines.push(("USE: READ".into(), EFFECT));
    }
    if d.throw_speed.is_some() {
        lines.push(("USE: THROW".into(), EFFECT));
    }
    if d.glow.is_some() {
        lines.push(("GIVES LIGHT IN HAND".into(), EFFECT));
    }
    if d.name == "base:ashlace_tissue" {
        lines.push(("BINDS DROSS. DOES NOT CLEAN IT".into(), BODY));
    }
    if let Some(preparation) = reg
        .preparations
        .values()
        .find(|preparation| preparation.output_item == d.name)
    {
        let verb = match preparation.application {
            crate::alchemy::ApplicationKind::Drink => "DRINK ONE EXACT DOSE",
            crate::alchemy::ApplicationKind::Plot => "APPLY TO ONE VIABLE PLOT",
            crate::alchemy::ApplicationKind::Wash => "WASH ONE SMALL TARGET",
            crate::alchemy::ApplicationKind::Coat => "COAT ONE STABLE SPECIMEN",
        };
        lines.push((format!("USE: {verb}"), EFFECT));
        let effect = match preparation.handler {
            crate::alchemy::PreparationHandler::TraceSight => "BOUNDED LOW-LIGHT TRACE SIGHT",
            crate::alchemy::PreparationHandler::NaturalRecovery => {
                "RECOVERY PAID BY HUNGER + NUTRITION"
            }
            crate::alchemy::PreparationHandler::RootUptake => {
                "SUPPLIES WATER + NUTRIENTS. DOES NOT CREATE GROWTH"
            }
            crate::alchemy::PreparationHandler::StrainRelief => {
                "LESS PERSONAL STRAIN. LOWER THROUGHPUT"
            }
            crate::alchemy::PreparationHandler::DrossWash => {
                "MOVES BOUNDED DROSS INTO PHYSICAL WASTE"
            }
            crate::alchemy::PreparationHandler::PreserveSpecimen => {
                "SLOWS AGE + LEAKAGE. NEVER RESETS AGE"
            }
            crate::alchemy::PreparationHandler::ThroughputSurge => {
                "MORE THROUGHPUT + DRAIN + OVERDRAW"
            }
            crate::alchemy::PreparationHandler::DrossAntidote => {
                "REDUCES BODILY HARM. DOES NOT CLEAN THE REGION"
            }
        };
        lines.push((effect.into(), BODY));
    }
    if let Some(discovery) = &d.discovery {
        let line = match discovery.kind.as_str() {
            "tuning_lens" => Some("HOLD USE: SETTLE A QUALITATIVE READING"),
            "lens_frame" => Some("FIT AT A LENS ASSEMBLY BENCH"),
            "field_ledger" => Some("USE: OPEN SIGNED FIELD RECORDS"),
            "survey_folio" => Some("MOUNT: INDEX A SETTLEMENT LIBRARY"),
            "calibration_plate" => Some("CARRIED: NARROWS READING UNCERTAINTY"),
            "artifact" => Some("USE: READ THIS MAKER'S SURVIVING CLAIM"),
            "reference_object" => Some("PHYSICAL REFERENCE FOR CONTROLLED TRIALS"),
            _ => None,
        };
        if let Some(line) = line {
            lines.push((line.into(), EFFECT));
        }
        if let Some(class) = discovery.evidence_class.as_deref() {
            lines.push((
                format!("EVIDENCE: {}", class.replace('_', " ").to_uppercase()),
                BODY,
            ));
        }
    }
    if let (Some(arcane), Some(units)) = (&d.arcane, current_units) {
        lines.push((
            format!(
                "CURRENT: {}",
                crate::arcane::qualitative_current(units, arcane.capacity).to_uppercase()
            ),
            EFFECT,
        ));
        let behavior = if arcane.stability_permille >= 750 {
            "HOLDS CHARGE STEADILY"
        } else if arcane.conductivity_permille >= 700 {
            "CONDUCTS CHARGE READILY"
        } else {
            "CHARGE FEELS RESTLESS"
        };
        lines.push((behavior.into(), BODY));
    }
    if d.durability > 0 {
        // The same field means two different things. On a tool it is
        // wear; on food it is freshness, burning down at
        // FRESHNESS_PER_SEC until the stack turns to mush. Calling a
        // carrot's clock "durability" told the player nothing about
        // the only thing it actually governs — how long they have to
        // eat it.
        if d.food.is_some() {
            // Said in days, because the question a larder answers is
            // "will this last the winter", and winter is a count of
            // days. Minutes were the honest unit when a season was
            // two hours; they are noise now that it is twelve.
            let days =
                stack.durability as f32 / world::FRESHNESS_PER_SEC / crate::server::DAY_LENGTH;
            lines.push((
                match days {
                    d if d < 1.0 => "FRESH: UNDER A DAY - EAT IT".to_string(),
                    d if d < 2.0 => "FRESH: ABOUT A DAY LEFT".to_string(),
                    d => format!("FRESH: {} DAYS LEFT", d as u32),
                },
                WEAR,
            ));
        } else {
            lines.push((
                format!("DURABILITY {} / {}", stack.durability, d.durability),
                WEAR,
            ));
        }
    }
    lines
}

impl Game {
    /// The stack under the cursor, whichever slot family it belongs to.
    /// Mirrors the click router's geometry — same rects, no mutation.
    fn hovered_item(&self) -> Option<ItemStack> {
        let carried = self.ui_state.held_stack.is_some();
        // While dragging a stack the cursor already says what it holds;
        // a tooltip under it would describe the slot it is about to
        // cover, which is the opposite of helpful.
        if carried {
            return None;
        }
        let inv = || {
            (0..TOTAL_SLOTS)
                .find(|&i| self.hit(self.inventory_layout().slot_rect(i)))
                .and_then(|i| self.inventory.slots[i])
        };
        match self.ui_state.screen {
            Screen::Inventory => {
                if self.ui_state.inventory_discovery_open {
                    return None;
                }
                if self.ui_state.inventory_browser_open || self.ui_state.inventory_status_open {
                    return inv();
                }
                for i in 0..5 {
                    if self.hit(self.armor_slot_rect(i)) {
                        return self.survival.armor[i];
                    }
                }
                let n = self.interaction.craft_size * self.interaction.craft_size;
                for i in 0..n {
                    if self.hit(self.inventory_layout().craft_slot_rect(i)) {
                        return self.interaction.craft_grid[i];
                    }
                }
                if self.hit(self.inventory_layout().result_slot_rect()) {
                    return crafting::match_repair(
                        &self.content.reg,
                        &self.interaction.craft_grid[..n],
                    )
                    .map(|repair| repair.output)
                    .or_else(|| {
                        crafting::match_recipe(
                            &self.content.reg,
                            &self.interaction.craft_grid[..n],
                            self.interaction.craft_size,
                        )
                        .map(|r| ItemStack::new(&self.content.reg, r.output, r.count))
                    });
                }
                inv()
            }
            Screen::Chest(pos) => (0..27)
                .find(|&i| self.hit(self.chest_slot_rect(i)))
                .and_then(|i| match self.runtime.view().block_entity_at(&pos) {
                    Some(world::BlockEntity::Chest(c)) => c.slots[i],
                    _ => None,
                })
                .or_else(inv),
            Screen::Furnace(pos) => (0..3)
                .find(|&i| self.hit(self.furnace_slot_rect(i)))
                .and_then(|i| match self.runtime.view().block_entity_at(&pos) {
                    Some(world::BlockEntity::Furnace(f)) => [f.input, f.fuel, f.output][i],
                    _ => None,
                })
                .or_else(inv),
            Screen::Offering(pos) => (0..3)
                .find(|&i| self.hit(self.offering_slot_rect(i)))
                .and_then(|i| match self.runtime.view().block_entity_at(&pos) {
                    Some(world::BlockEntity::Offering(o)) => o.slots[i],
                    _ => None,
                })
                .or_else(inv),
            Screen::MobCargo(id) => (0..9)
                .find(|&i| self.hit(self.mob_cargo_slot_rect(i)))
                .and_then(|i| {
                    self.runtime
                        .view()
                        .mob_by_id(id)
                        .and_then(|m| m.cargo.as_ref().and_then(|c| c[i]))
                })
                .or_else(inv),
            Screen::Bloomery(_)
            | Screen::Kiln(_)
            | Screen::Workbench(_)
            | Screen::Stall(_)
            | Screen::Mod(_) => inv(),
            _ => None,
        }
    }

    /// Draw the hovered item's card beside the cursor, flipped to stay
    /// on screen. Appended after every screen has drawn, so it is never
    /// painted over by a slot grid.
    pub(super) fn draw_item_tooltip(&mut self) {
        let Some(stack) = self.hovered_item() else {
            return;
        };
        let current = self
            .runtime
            .view()
            .inspectable_item_current(stack.arcane_id);
        let mut lines = item_tooltip_lines_with_current(&self.content.reg, stack, current);
        let has_lens = self
            .inventory
            .slots
            .iter()
            .chain(self.survival.armor.iter())
            .flatten()
            .any(|held| {
                self.content
                    .reg
                    .item(held.item)
                    .discovery
                    .as_ref()
                    .is_some_and(|definition| definition.kind == "tuning_lens")
            });
        lines.extend(
            self.runtime
                .view()
                .preparation_tooltip(stack, has_lens)
                .into_iter()
                .map(|line| (line, EFFECT)),
        );
        let implement = self.runtime.view().implement_tooltip(stack, has_lens);
        if !implement.is_empty() {
            // The implement resolver knows its actual component-derived
            // capacity; remove the generic content-manifest reading so the
            // card never shows two contradictory charge bands.
            lines.retain(|(line, _)| {
                !line.starts_with("CURRENT:")
                    && line != "HOLDS CHARGE STEADILY"
                    && line != "CONDUCTS CHARGE READILY"
                    && line != "CHARGE FEELS RESTLESS"
            });
            lines.extend(
                implement
                    .into_iter()
                    .map(|line| (line.to_uppercase(), EFFECT)),
            );
        }
        const S: f32 = 1.3;
        const PAD: f32 = 8.0;
        const LINE: f32 = 15.0;
        let width = lines
            .iter()
            .map(|(t, _)| UiBatch::text_width(S, t))
            .fold(0.0f32, f32::max)
            + PAD * 2.0;
        let height = lines.len() as f32 * LINE + PAD * 2.0 - 4.0;
        let (cx, cy) = self.input.ui_cursor;
        let sw = self.renderer.config.width as f32;
        let sh = self.renderer.config.height as f32;
        // Right and below by default; flip on whichever edge it would
        // otherwise run off, so a slot in the far corner still reads.
        let x = if cx + 18.0 + width > sw {
            (cx - 18.0 - width).max(0.0)
        } else {
            cx + 18.0
        };
        let y = if cy + 14.0 + height > sh {
            (cy - 14.0 - height).max(0.0)
        } else {
            cy + 14.0
        };
        let mut ui = std::mem::replace(&mut self.ui, UiBatch::new());
        ui.rect(x, y, width, height, [0.03, 0.04, 0.05, 0.96]);
        ui.rect(x, y, width, 2.0, [0.55, 0.58, 0.60, 0.85]);
        ui.rect(x, y + height - 2.0, width, 2.0, [0.20, 0.22, 0.24, 0.9]);
        ui.rect(x, y, 2.0, height, [0.40, 0.43, 0.46, 0.8]);
        ui.rect(x + width - 2.0, y, 2.0, height, [0.20, 0.22, 0.24, 0.9]);
        for (i, (text, color)) in lines.iter().enumerate() {
            ui.text_shadow(x + PAD, y + PAD + i as f32 * LINE, S, text, *color);
        }
        self.ui = ui;
    }
}

#[cfg(test)]
#[path = "tooltip_tests.rs"]
mod tests;
