//! Overrides capture scene construction.

use super::DemoChart;
use crate::game::Game;
use crate::game::navigation::Screen;
use crate::inventory::ItemStack;
use crate::planet::{EntityPos, Face};
use crate::world;
use glam::Vec3;

impl Game {
    pub(super) fn stage_capture_presentation(&mut self) {
        // Dev/headless: the in-world screens can only be reached by
        // playing, which a shot run cannot do. This makes the paper
        // doll and the slot grid verifiable from a capture.
        // Dev/headless: park the UI cursor, so a capture can show a
        // hover state (tooltips) that otherwise needs a real mouse.
        if let Ok(spec) = std::env::var("WILDFORGE_CURSOR")
            && let Some((x, z)) = spec.split_once(',')
            && let (Ok(x), Ok(y)) = (x.trim().parse::<f32>(), z.trim().parse::<f32>())
        {
            self.input.ui_cursor = (x, y);
            self.input.cursor_locked = true;
        }
        match std::env::var("WILDFORGE_SCREEN").as_deref() {
            Ok("inventory") => self.set_screen(Screen::Inventory),
            Ok("status") => {
                self.set_screen(Screen::Inventory);
                self.ui_state.inventory_status_open = true;
                self.ui_state.inventory_browser_open = false;
            }
            _ => {}
        }
        // Dev: force time of day (0..1; 0.75 = midnight).
        if let Ok(t) = std::env::var("WILDFORGE_TIME")
            && let Ok(t) = t.parse::<f32>()
        {
            *self.runtime.time_of_day_mut() = t.fract();
        }
        // Dev: force the calendar day, to land on a specific moon phase
        // (day % LUNAR_DAYS; 0 = new, 4 = full at the default cycle length).
        if let Ok(d) = std::env::var("WILDFORGE_DAY")
            && let Ok(d) = d.parse::<u32>()
        {
            self.runtime.local_mut().world.set_calendar_day(d);
        }
        // Planetary visual qualification needs to show a whole valley,
        // shoreline, or treeline rather than whatever happens to occupy the
        // player's eye-height foreground. Lift only automated captures into a
        // stationary creative flyover; ordinary starts and interactive play
        // are untouched.
        if self.auto_shot.is_some()
            && let Ok(height) = std::env::var("WILDFORGE_SHOT_ALTITUDE")
            && let Ok(height) = height.parse::<f32>()
        {
            self.player.pos.y = (self.player.pos.y + height.clamp(0.0, 96.0))
                .min(crate::chunk::CHUNK_Y as f32 - 3.0);
            self.player.vel = Vec3::ZERO;
            self.flying = true;
            self.camera.follow_planet(self.player.eye());
        }
        if self.auto_shot.is_some() {
            self.runtime.local_mut().freeze_clock = true;
        }
        // Dev: WILDFORGE_HELD=<item> puts an item in the selected hotbar slot,
        // so a headless run can hold a torch — the held-light path is otherwise
        // only reachable by playing.
        if let Ok(name) = std::env::var("WILDFORGE_HELD") {
            let reg = self.content.reg.clone();
            match reg
                .item_id(&name)
                .or_else(|| reg.item_id(&format!("base:{name}")))
            {
                Some(item) => {
                    if let Some(previous) = self.inventory.slots[self.input.hotbar_sel]
                        && let Err(error) = self
                            .runtime
                            .local_mut()
                            .world
                            .record_admin_stack_deletion(previous)
                    {
                        eprintln!("materials: held-item override deletion failed: {error}");
                    }
                    let mut stack = ItemStack::new(&reg, item, 1);
                    let accepted = self.player.pos.block().is_none_or(|at| {
                        self.runtime
                            .local_mut()
                            .world
                            .bind_arcane_stack_at(at, &mut stack, "development held-item override")
                            .map_err(|error| {
                                eprintln!("arcane: held-item override rejected: {error}");
                                error
                            })
                            .is_ok()
                    });
                    if accepted {
                        if let Err(error) = self
                            .runtime
                            .local_mut()
                            .world
                            .record_external_stack(stack, "development held-item override")
                        {
                            eprintln!("materials: held-item override source failed: {error}");
                        }
                        self.inventory.slots[self.input.hotbar_sel] = Some(stack);
                    }
                }
                None => eprintln!("WILDFORGE_HELD: no item named {name:?}"),
            }
        }
        self.load_attunements();
        // Dev: WILDFORGE_POS="x,y,z" teleports to an exact chart-local spot
        // (reproducing reported coordinates). WILDFORGE_FACE selects another
        // cube chart for qualification sites away from the saved spawn.
        // Runs after load so it wins.
        if let Ok(s) = std::env::var("WILDFORGE_POS") {
            let p: Vec<f32> = s.split(',').filter_map(|v| v.trim().parse().ok()).collect();
            if p.len() == 3 {
                let face = std::env::var("WILDFORGE_FACE")
                    .ok()
                    .as_deref()
                    .and_then(Face::from_name)
                    .unwrap_or(self.player.pos.face());
                let target_chart = DemoChart::new(face);
                let cp = target_chart.chunk(p[0] as i32, p[2] as i32);
                for dx in -2..=2 {
                    for dz in -2..=2 {
                        self.runtime
                            .local_mut()
                            .world
                            .ensure_chunk(cp.offset(dx, dz));
                    }
                }
                self.player.pos =
                    crate::planet::EntityPos::from_local(face, Vec3::new(p[0], p[1], p[2]))
                        .unwrap();
                self.player.vel = Vec3::ZERO;
                self.camera.follow_planet(self.player.eye());
            }
        }
        // Dev: force camera look ("yaw,pitch" in radians) for framed captures.
        self.apply_look_env();
        // Dev: WILDFORGE_SCREEN=inventory opens the pack in-world for
        // layout screenshots (menu screens are handled at startup).
        if std::env::var("WILDFORGE_SCREEN").as_deref() == Ok("inventory") {
            self.set_screen(Screen::Inventory);
        }
    }

    pub(super) fn stage_capture_juice(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: a glassworks yard - kiln stack, quern, minerals, sand.
        // Dev: stage the juice layer for screenshots — a trodden snow
        // trail, low health (heart wobble + vignette), and a debris
        // burst frozen mid-flight.
        if std::env::var("WILDFORGE_DEMO_JUICE").is_ok() {
            let b = |n: &str| self.content.reg.block_id(n);
            if let (Some(layer), Some(dirt)) = (b("base:snow_layer"), b("base:dirt")) {
                let (sx, sz) = (spawn.x as i32 + 4, spawn.z as i32 - 2);
                let sy = demo_height!(self.runtime.local().world, chart, sx, sz);
                for rx in 0..6i32 {
                    for rz in -2..=2i32 {
                        demo_set!(
                            self.runtime.local_mut().world,
                            chart,
                            sx + rx,
                            sy,
                            sz + rz,
                            dirt
                        );
                        demo_set!(
                            self.runtime.local_mut().world,
                            chart,
                            sx + rx,
                            sy + 1,
                            sz + rz,
                            layer
                        );
                    }
                }
                // A walker crossed the field on the diagonal.
                for i in 0..5i32 {
                    self.runtime.local_mut().world.tread_at(chart.block(
                        sx + i,
                        sy + 1,
                        sz - 2 + i,
                    ));
                }
                // A break mid-burst, sparks and all; the tick re-stamps
                // the moment so any capture frame lands mid-effect.
                let center = chart
                    .entity(Vec3::new(sx as f32 + 2.5, sy as f32 + 2.5, sz as f32 + 0.5))
                    .render_pos();
                self.presentation.demo_burst =
                    Some((center, self.content.reg.block(dirt).tiles[0]));
                self.presentation
                    .burst(center, self.content.reg.block(dirt).tiles[0], 10, 2.2);
            }
            self.survival.health = 5.0;
            self.survival.damage_flash = 0.35;
        }
    }

    pub(super) fn stage_capture_environment(&mut self) {
        // Dev: WILDFORGE_IRE=N forces the wild's ire (spawn testing).
        if let Ok(v) = std::env::var("WILDFORGE_IRE")
            && let Ok(v) = v.parse::<f32>()
        {
            self.runtime.local_mut().world.ire = v.clamp(0.0, 100.0);
            self.runtime.local_mut().sync_tier();
        }
        // Dev: force the calendar and the sky.
        if let Ok(v) = std::env::var("WILDFORGE_DAY")
            && let Ok(v) = v.parse::<u32>()
        {
            self.runtime.local_mut().world.set_calendar_day(v);
        }
        if let Ok(v) = std::env::var("WILDFORGE_SEASON")
            && let Ok(v) = v.parse::<u32>()
        {
            self.runtime
                .local_mut()
                .world
                .set_calendar_day((v % 4) * world::SEASON_DAYS);
        }
        // Calendar overrides must move the authoritative simulation clock too.
        // Local astronomy and climate sample `World::clock`; leaving it at the
        // pre-override value makes a capture's sky disagree with its weather.
        let clock = (f64::from(self.runtime.view().day())
            + f64::from(self.runtime.time_of_day().rem_euclid(1.0)))
            * f64::from(crate::server::DAY_LENGTH);
        self.runtime.local_mut().world.set_simulation_clock(clock);
        if let Ok(v) = std::env::var("WILDFORGE_WEATHER") {
            self.runtime.local_mut().world.force_local_weather(&v);
            self.presentation.weather_vis = match v.as_str() {
                "overcast" => 0.4,
                "precip" | "rain" | "snow" => 0.55,
                "storm" => 0.7,
                _ => 0.0,
            };
        }
    }

    pub(super) fn stage_capture_inventory(&mut self) {
        // Dev/headless: open the inventory for UI verification.
        if std::env::var("WILDFORGE_SCREEN").as_deref() == Ok("inventory") {
            self.interaction.craft_size = 2;
            self.set_screen(Screen::Inventory);
        }
    }
}
