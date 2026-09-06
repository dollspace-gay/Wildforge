//! Ecology capture scene construction.

use crate::world::TerrainRead;
use crate::inventory::ItemStack;
use crate::registry::AIR;
use crate::world;
use glam::Vec3;
use crate::game::Game;
use crate::planet::{BlockPos, EntityPos, Face, SurfacePos};
use super::DemoChart;

impl Game {
    pub(in crate::game) fn stage_capture_magic_ecology(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: plant the base magical ecology through the real cultivation
        // path around the prepared doorstep. This is deliberately not a row
        // of authored decorative blocks: each placement registers a distinct
        // player-owned persistent site, begins empty, and must establish from
        // the local water, nutrients, climate, and Current if the capture is
        // allowed to run on.
        if std::env::var("WILDFORGE_DEMO_MAGIC_ECOLOGY").is_ok() {
            let bx = spawn.x as i32;
            let bz = spawn.z as i32;
            let mut planted = 0usize;
            for (index, item_name) in [
                "base:rainbell_dew",
                "base:hushwood_switch",
                "base:stormvine_tendril",
                "base:cairnbloom_flower",
                "base:ashlace_tissue",
                "base:pilgrim_root_cutting",
                "base:lantern_reed_pith",
                "base:nightglass_pod",
                "base:frostlace_frond",
                "base:tidekelp_blade",
                "base:echo_cap_ring",
                "base:ember_petal",
            ]
            .into_iter()
            .enumerate()
            {
                let x = bx - 6 + (index % 4) as i32 * 4;
                let z = bz + 5 + (index / 4) as i32 * 4;
                let y = demo_height!(self.runtime.local().world, chart, x, z);
                let pos = chart.block(x, y + 1, z);
                self.runtime.local_mut().world.set_block_at(pos, AIR);
                let Some(item) = self.content.reg.item_id(item_name) else {
                    continue;
                };
                if self.runtime.local_mut().world.place_item_block_at(pos, ItemStack::new(&self.content.reg, item, 1))
                {
                    planted += 1;
                }
            }
            eprintln!("magical ecology demo registered {planted} cultivated sites");
        }
    }

    pub(in crate::game) fn stage_capture_wildlife(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: the wild arc in one frame — a smoking rack curing cuts
        // over a torch, and a watcher warden at the treeline.
        if std::env::var("WILDFORGE_DEMO_WILD").is_ok() {
            let b = |n: &str| self.content.reg.block_id(n);
            let reg2 = self.content.reg.clone();
            let bx = spawn.x as i32;
            let bz = spawn.z as i32;
            let y = demo_height!(self.runtime.local().world, chart, bx, bz);
            if let Some(grass) = b("base:grass") {
                for dx in -8..=8i32 {
                    for dz in -2..=16i32 {
                        let (x, z) = (bx + dx, bz + dz);
                        demo_set!(self.runtime.local_mut().world, chart, x, y, z, grass);
                        for hh in 1..=6 {
                            if demo_get!(self.runtime.local().world, chart, x, y + hh, z) != AIR {
                                demo_set!(self.runtime.local_mut().world, chart, x, y + hh, z, AIR);
                            }
                        }
                    }
                }
            }
            if let (Some(rack), Some(torch)) = (b("base:smoking_rack"), b("base:torch")) {
                demo_set!(self.runtime.local_mut().world, chart, bx - 2, y + 1, bz + 4, torch);
                demo_set!(self.runtime.local_mut().world, chart, bx - 2, y + 2, bz + 4, rack);
                let mut sm = crate::world::SmokerState::default();
                if let (Some(raw), Some(smoked)) = (
                    reg2.item_id("base:raw_venison"),
                    reg2.item_id("base:smoked_meat"),
                ) {
                    sm.meat[0] = Some(ItemStack::new(&reg2, raw, 1));
                    sm.meat[1] = Some(ItemStack::new(&reg2, smoked, 1));
                }
                demo_insert!(
                    self.runtime.local_mut().world,
                    chart,
                    (bx - 2, y + 2, bz + 4),
                    crate::world::BlockEntity::Smoker(sm),
                );
            }
            // Aggrieved country and its watcher, mid-vigil. A torch
            // line marks the settlement's edge; the wild stands just
            // beyond it.
            for _ in 0..12 {
                demo_ire!(self.runtime.local_mut().world, chart, bx, bz, 1.0);
            }
            if let Some(torch) = b("base:torch") {
                for dx in [0i32, 3, 6] {
                    demo_set!(self.runtime.local_mut().world, chart, bx + dx, y + 1, bz + 11, torch);
                }
            }
            if let Some(ti) = reg2.animals.iter().position(|a| a.hostile) {
                let mut w = demo_mob!(
                    chart,
                    ti,
                    glam::Vec3::new(bx as f32 + 3.5, y as f32 + 1.0, bz as f32 + 13.5),
                    3.4,
                );
                w.health = reg2.animals[ti].health;
                w.watcher = true;
                w.watch_baseline = demo_standing!(self.runtime.local().world, chart, bx, bz);
                self.runtime.local_mut().world.spawn_mob(w);
            }
        }
    }

    pub(in crate::game) fn stage_capture_heart(&mut self, spawn: EntityPos, chart: DemoChart) {
        if std::env::var("WILDFORGE_DEMO_HEART").is_ok() {
            // The three forms, alive/failing/dead, in a row — and one
            // ruined site with its ground raised ready for a seed.
            let b = |n: &str| self.content.reg.block_id(n);
            let bx = spawn.x as i32;
            let bz = spawn.z as i32;
            let y = demo_height!(self.runtime.local().world, chart, bx, bz);
            eprintln!("heart demo anchored at ({bx},{y},{bz})");
            if let Some(grass) = b("base:grass") {
                let w = &mut self.runtime.local_mut().world;
                for dx in -18..=18i32 {
                    for dz in -20..=14i32 {
                        let (x, z) = (bx + dx, bz + dz);
                        demo_set!(w, chart, x, y, z, grass);
                        for hh in 1..=10 {
                            if demo_get!(w, chart, x, y + hh, z) != AIR {
                                demo_set!(w, chart, x, y + hh, z, AIR);
                            }
                        }
                    }
                }
            }
            let w = &mut self.runtime.local_mut().world;
            // One of each shape, each from a different country, so a
            // capture shows the bole, the spring and the stone at all
            // three stages.
            for (col, biome) in [
                (-10i32, crate::worldgen::Biome::Jungle),
                (0, crate::worldgen::Biome::Savanna),
                (10, crate::worldgen::Biome::Arctic),
            ] {
                let form = crate::world::heart_form(biome);
                for (row, stage) in [(-4i32, 2u8), (2, 1), (8, 0)] {
                    let name = crate::world::heart_block_name(form, stage);
                    let Some(block) = b(&name) else { continue };
                    let tall = crate::world::heart_height(form);
                    for dy in 1..=tall {
                        demo_set!(w, chart, bx + col, y + dy, bz + row, block);
                    }
                }
            }
            // Ground made ready around the dead stone: the long walk's
            // last step, waiting on a seed.
            if let Some(farm) = b("base:farmland") {
                for dx in -4..=4i32 {
                    for dz in -4..=4i32 {
                        if dx * dx + dz * dz > 16 {
                            continue;
                        }
                        demo_meta!(
                            w,
                            chart,
                            bx + 10 + dx,
                            y,
                            bz + 8 + dz,
                            farm,
                            crate::world::soil::soil_meta(48, 0),
                        );
                    }
                }
            }
        }
    }

    pub(in crate::game) fn stage_capture_steward(&mut self, spawn: EntityPos, chart: DemoChart) {
        // Dev: stewardship showcase — offering stone with gifts, a planted
        // sapling, and a grown oak (verification).
        if std::env::var("WILDFORGE_DEMO_STEWARD").is_ok() {
            let reg = self.content.reg.clone();
            let (sx, sz) = (spawn.x as i32, spawn.z as i32);
            if let Some(os) = reg.block_id("base:offering_stone") {
                let y = demo_height!(self.runtime.local().world, chart, sx - 3, sz - 5) + 1;
                demo_set!(self.runtime.local_mut().world, chart, sx - 3, y, sz - 5, os);
                let mut st = world::OfferingState::default();
                if let Some(hw) = reg.item_id("base:heartwood") {
                    st.slots[0] = Some(ItemStack::new(&reg, hw, 2));
                }
                demo_insert!(
                    self.runtime.local_mut().world,
                    chart,
                    (sx - 3, y, sz - 5),
                    world::BlockEntity::Offering(st)
                );
            }
            if let Some(sap) = reg.block_id("base:oak_sapling") {
                let y = demo_height!(self.runtime.local().world, chart, sx + 2, sz - 6) + 1;
                demo_set!(self.runtime.local_mut().world, chart, sx + 2, y, sz - 6, sap);
            }
            let ty = demo_height!(self.runtime.local().world, chart, sx + 6, sz - 8) + 1;
            self.runtime.local_mut().world.grow_tree_at(chart.block(sx + 6, ty, sz - 8), "oak", 3);
            for name in ["base:bedroll", "base:oak_sapling"] {
                if let Some(item) = reg.item_id(name) {
                    self.give_dev_item(&reg, item, 1);
                }
            }
        }
    }
}
