//! Station panels layout and UI composition.

use crate::game::Game;
use crate::game::widgets;
use crate::ui::UiBatch;
use crate::world;

impl Game {
    pub(in crate::game) fn draw_furnace_screen(
        &mut self,
        ui: &mut UiBatch,
        w: f32,
        h: f32,
        pos: crate::planet::BlockPos,
    ) {
        ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
        let title = "FURNACE";
        let tw = UiBatch::text_width(3.0, title);
        ui.text_shadow((w - tw) / 2.0, h / 2.0 - 285.0, 3.0, title, [1.0; 4]);
        let (inp, fuel, out, prog, burn) = self.furnace_view(pos);
        let ir = self.furnace_slot_rect(0);
        let fr = self.furnace_slot_rect(1);
        let orr = self.furnace_slot_rect(2);
        widgets::slot(&self.content.reg, &mut *ui, ir, inp, false, self.hit(ir));
        widgets::slot(&self.content.reg, &mut *ui, fr, fuel, false, self.hit(fr));
        widgets::slot(&self.content.reg, &mut *ui, orr, out, false, self.hit(orr));
        // Flame between input and fuel, arrow toward the output.
        let flame_h = 24.0 * burn;
        ui.rect(
            ir.0 + 12.0,
            fr.1 - 4.0 - flame_h,
            22.0,
            flame_h,
            [1.0, 0.55, 0.1, 0.95],
        );
        let ay = ir.1 + Self::SLOT + 14.0;
        ui.rect(ir.0 + 64.0, ay, 100.0, 8.0, [0.15, 0.15, 0.15, 0.9]);
        ui.rect(ir.0 + 64.0, ay, 100.0 * prog, 8.0, [1.0, 1.0, 1.0, 0.95]);
        // Player inventory below for restocking.
        self.draw_player_inventory(&mut *ui);
        self.draw_browser(&mut *ui);
        widgets::held_stack(
            &self.content.reg,
            &mut *ui,
            self.input.ui_cursor,
            self.ui_state.held_stack,
        );
    }
    pub(in crate::game) fn draw_bloomery_screen(
        &mut self,
        ui: &mut UiBatch,
        w: f32,
        h: f32,
        pos: crate::planet::BlockPos,
    ) {
        ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
        // The forge rides the bloomery screen: same slots,
        // its own shell check and firing clock.
        let forge = matches!(
            self.runtime.view().block_entity_at(&pos),
            Some(world::BlockEntity::Multiblock(b))
                if b.kind.handler(&self.content.reg)
                    == Some(crate::machines::MachineHandler::Forge)
        );
        let title = if forge { "FORGE" } else { "BLOOMERY" };
        let tw = UiBatch::text_width(3.0, title);
        ui.text_shadow((w - tw) / 2.0, h / 2.0 - 300.0, 3.0, title, [1.0; 4]);
        let (slots, lit, progress, breached) = {
            let breached = if forge {
                self.runtime.view().check_forge_at(pos).is_none()
            } else {
                self.runtime.view().check_bloomery_at(pos).is_none()
            };
            match self.runtime.view().block_entity_at(&pos) {
                Some(world::BlockEntity::Multiblock(b)) => {
                    let mut v = [None; 8];
                    v[..4].copy_from_slice(&b.charge);
                    v[4..].copy_from_slice(&b.fuel);
                    let secs = if forge {
                        world::FORGE_FIRE_SECS
                    } else {
                        world::BLOOMERY_FIRE_SECS
                    };
                    (v, b.lit, b.progress / secs, breached)
                }
                _ => ([None; 8], false, 0.0, breached),
            }
        };
        ui.text_shadow(w / 2.0 - 150.0, h / 2.0 - 268.0, 1.5, "CHARGE", [1.0; 4]);
        let fuel_label = if forge { "FUEL" } else { "CHARCOAL" };
        ui.text_shadow(w / 2.0 - 150.0, h / 2.0 - 186.0, 1.5, fuel_label, [1.0; 4]);
        for (i, s) in slots.iter().enumerate() {
            let r = self.bloomery_slot_rect(i);
            widgets::slot(&self.content.reg, &mut *ui, r, *s, false, self.hit(r));
        }
        let lr = self.bloomery_light_rect();
        if lit {
            let br = (
                w / 2.0 - 2.0 * (Self::SLOT + 10.0) + 5.0,
                h / 2.0 - 120.0,
                4.0 * (Self::SLOT + 10.0) - 10.0,
                10.0,
            );
            ui.rect(br.0, br.1, br.2, br.3, [0.15, 0.15, 0.15, 0.9]);
            ui.rect(br.0, br.1, br.2 * progress, br.3, [1.0, 0.55, 0.1, 0.95]);
            ui.text_shadow(
                br.0,
                br.1 + 16.0,
                1.5,
                "FIRING - SEALED",
                [1.0, 0.8, 0.5, 1.0],
            );
        } else if breached {
            ui.text_shadow(
                lr.0,
                lr.1 + 44.0,
                1.5,
                if forge {
                    "WANTS STACK, CHIMNEY, ANVIL"
                } else {
                    "THE STACK IS BREACHED"
                },
                [1.0, 0.5, 0.4, 1.0],
            );
            widgets::button(&mut *ui, lr, "LIGHT", false);
        } else {
            widgets::button(&mut *ui, lr, "LIGHT", self.hit(lr));
        }
        self.draw_player_inventory(&mut *ui);
        self.draw_browser(&mut *ui);
        widgets::held_stack(
            &self.content.reg,
            &mut *ui,
            self.input.ui_cursor,
            self.ui_state.held_stack,
        );
    }
    pub(in crate::game) fn draw_kiln_screen(
        &mut self,
        ui: &mut UiBatch,
        w: f32,
        h: f32,
        pos: crate::planet::BlockPos,
    ) {
        ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
        let title = if self.runtime.view().check_glassworks_at(pos).is_some() {
            "GLASSWORKS"
        } else {
            "GLASS KILN"
        };
        let tw = UiBatch::text_width(3.0, title);
        ui.text_shadow((w - tw) / 2.0, h / 2.0 - 310.0, 3.0, title, [1.0; 4]);
        let (slots, lit, progress, breached) = {
            let breached = self.runtime.view().check_kiln_at(pos).is_none();
            match self.runtime.view().block_entity_at(&pos) {
                Some(world::BlockEntity::Multiblock(k)) => {
                    let mut v = [None; 9];
                    v[..4].copy_from_slice(&k.charge);
                    v[4] = k.reagent;
                    v[5..].copy_from_slice(&k.fuel);
                    (v, k.lit, k.progress / world::KILN_FIRE_SECS, breached)
                }
                _ => ([None; 9], false, 0.0, breached),
            }
        };
        ui.text_shadow(w / 2.0 - 150.0, h / 2.0 - 288.0, 1.5, "SAND", [1.0; 4]);
        ui.text_shadow(w / 2.0 - 150.0, h / 2.0 - 210.0, 1.5, "PIGMENT", [1.0; 4]);
        ui.text_shadow(w / 2.0 - 150.0, h / 2.0 - 132.0, 1.5, "CHARCOAL", [1.0; 4]);
        for (i, sl) in slots.iter().enumerate() {
            let r = self.kiln_slot_rect(i);
            widgets::slot(&self.content.reg, &mut *ui, r, *sl, false, self.hit(r));
        }
        let lr = self.bloomery_light_rect();
        if lit {
            let br = (
                w / 2.0 - 2.0 * (Self::SLOT + 10.0) + 5.0,
                h / 2.0 - 60.0,
                4.0 * (Self::SLOT + 10.0) - 10.0,
                10.0,
            );
            ui.rect(br.0, br.1, br.2, br.3, [0.15, 0.15, 0.15, 0.9]);
            ui.rect(br.0, br.1, br.2 * progress, br.3, [1.0, 0.9, 0.5, 0.95]);
            ui.text_shadow(
                br.0,
                br.1 + 16.0,
                1.5,
                "FIRING - SEALED",
                [1.0, 0.9, 0.6, 1.0],
            );
        } else if breached {
            ui.text_shadow(
                lr.0,
                lr.1 + 44.0,
                1.5,
                "THE STACK IS BREACHED",
                [1.0, 0.5, 0.4, 1.0],
            );
            widgets::button(&mut *ui, lr, "LIGHT", false);
        } else {
            widgets::button(&mut *ui, lr, "LIGHT", self.hit(lr));
        }
        self.draw_player_inventory(&mut *ui);
        self.draw_browser(&mut *ui);
        widgets::held_stack(
            &self.content.reg,
            &mut *ui,
            self.input.ui_cursor,
            self.ui_state.held_stack,
        );
    }
    pub(in crate::game) fn draw_workbench_screen(
        &mut self,
        ui: &mut UiBatch,
        w: f32,
        h: f32,
        pos: crate::planet::BlockPos,
    ) {
        ui.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.55]);
        let reg = &self.content.reg;
        let machine = self
            .runtime
            .view()
            .block_entity_at(&pos)
            .and_then(|e| match e {
                world::BlockEntity::Multiblock(m) => {
                    reg.machine(m.kind).map(|def| def.label.clone())
                }
                _ => None,
            });
        let title = machine
            .unwrap_or_else(|| "WORKBENCH".to_string())
            .to_uppercase();
        let tw = UiBatch::text_width(3.0, &title);
        ui.text_shadow((w - tw) / 2.0, h / 2.0 - 310.0, 3.0, &title, [1.0; 4]);
        let recipes = self
            .runtime
            .view()
            .block_entity_at(&pos)
            .and_then(|e| match e {
                world::BlockEntity::Multiblock(m) => Some(reg.machine_recipes_for(m.kind)),
                _ => None,
            });
        let recipes: Vec<&crate::registry::RecipeDef> = recipes.unwrap_or_default();
        if recipes.is_empty() {
            ui.text_shadow(
                w / 2.0 - 120.0,
                h / 2.0 - 230.0,
                1.8,
                "NO RECIPES HERE",
                [0.6, 0.6, 0.6, 1.0],
            );
        }
        let cycle = (self.time_abs / 0.8) as usize;
        for (i, r) in recipes.iter().enumerate().take(20) {
            let rr = self.workbench_recipe_rect(i);
            let tech_value = r.tech.as_deref().and_then(|key| self.read_player_kv(key));
            let locked = crate::game::containers::recipe_gates_met(
                tech_value.as_deref(),
                &self.inventory,
                r,
            )
            .is_some();
            let craftable = !locked
                && (0..r.h).all(|y| {
                    (0..r.w).all(|x| {
                        let Some(ing) = &r.pattern[y * r.w + x] else {
                            return true;
                        };
                        self.inventory
                            .slots
                            .iter()
                            .any(|slot| slot.is_some_and(|stack| ing.matches(stack.item)))
                    })
                });
            let border = if self.hit(rr) {
                [1.0, 1.0, 1.0, 0.6]
            } else {
                [0.35, 0.35, 0.35, 0.9]
            };
            ui.rect(rr.0, rr.1, rr.2, rr.3, border);
            ui.rect(
                rr.0 + 2.0,
                rr.1 + 2.0,
                rr.2 - 4.0,
                rr.3 - 4.0,
                [0.15, 0.15, 0.15, 0.95],
            );
            let mut x = rr.0 + 12.0;
            for cell in r.pattern.iter().flatten() {
                let show = match cell {
                    crate::registry::Ingredient::One(item) => *item,
                    crate::registry::Ingredient::Any(items) => items[cycle % items.len()],
                };
                let icon = reg.item(show).icon;
                ui.tile(x, rr.1 + 10.0, 34.0, 34.0, icon, [1.0; 4]);
                x += 40.0;
            }
            ui.text_shadow(x + 4.0, rr.1 + 18.0, 2.4, ">", [1.0; 4]);
            let oc = reg.item(r.output).icon;
            ui.tile(x + 24.0, rr.1 + 10.0, 34.0, 34.0, oc, [1.0; 4]);
            if r.count > 1 {
                ui.text_shadow(
                    x + 44.0,
                    rr.1 + 32.0,
                    2.0,
                    &format!("{}", r.count),
                    [1.0; 4],
                );
            }
            ui.text_shadow(
                x + 70.0,
                rr.1 + 20.0,
                1.6,
                &reg.item(r.output).label,
                [1.0; 4],
            );
            if locked {
                ui.text_shadow(
                    rr.0 + rr.2 - 90.0,
                    rr.1 + 20.0,
                    1.5,
                    "LOCKED",
                    [1.0, 0.35, 0.35, 1.0],
                );
            } else if !craftable {
                ui.text_shadow(
                    rr.0 + rr.2 - 90.0,
                    rr.1 + 20.0,
                    1.5,
                    "MISSING",
                    [0.8, 0.6, 0.3, 1.0],
                );
            }
        }
        self.draw_player_inventory(&mut *ui);
        self.draw_browser(&mut *ui);
        widgets::held_stack(
            &self.content.reg,
            &mut *ui,
            self.input.ui_cursor,
            self.ui_state.held_stack,
        );
    }
}
