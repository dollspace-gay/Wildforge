//! Alchemy capture scene construction.

use super::DemoChart;
use crate::game::Game;
use crate::identity;
use crate::inventory::Inventory;
use crate::planet::EntityPos;
use crate::registry::AIR;
use glam::Vec3;

impl Game {
    /// A compact physical apothecary used by the GPU capture gate. Every
    /// station is a real authored block with an authoritative installation
    /// record, adjacent conductor, heat source, cooling stock, and reusable
    /// vessels/inputs in the ordinary player inventory.
    pub(super) fn stage_alchemy_demo(&mut self, spawn: EntityPos) -> Result<(), String> {
        let chart = DemoChart::new(spawn.face());
        let reg = self.content.reg.clone();
        let bx = spawn.x.round() as i32;
        let bz = spawn.z.round() as i32 - 8;
        let y = (-3..=3)
            .map(|dx| demo_height!(self.runtime.local().world, chart, bx + dx, bz))
            .max()
            .unwrap_or_else(|| demo_height!(self.runtime.local().world, chart, bx, bz))
            + 1;
        let stone = reg
            .block_id("base:stone")
            .ok_or("capture registry lacks stone")?;
        for dx in -6..=6 {
            for dz in -2..=6 {
                if dz <= 2 {
                    demo_set!(
                        self.runtime.local_mut().world,
                        chart,
                        bx + dx,
                        y - 1,
                        bz + dz,
                        stone
                    );
                }
                for dy in 0..=6 {
                    demo_set!(
                        self.runtime.local_mut().world,
                        chart,
                        bx + dx,
                        y + dy,
                        bz + dz,
                        AIR
                    );
                }
            }
        }
        let mut stations = Vec::new();
        for (dx, name) in [
            (-3, "base:alchemy_mortar"),
            (-1, "base:infusion_basin"),
            (1, "base:alembic"),
            (3, "base:filter_stand"),
        ] {
            let pos = chart.block(bx + dx, y, bz);
            let block = reg
                .block_id(name)
                .ok_or_else(|| format!("capture registry lacks {name}"))?;
            self.runtime.local_mut().world.set_block_authored_at(
                pos,
                block,
                "development alchemy qualification scene",
            );
            let conductor = reg
                .block_id("base:arcane_conductor")
                .ok_or("capture registry lacks arcane conductor")?;
            self.runtime.local_mut().world.set_block_authored_at(
                chart.block(bx + dx, y, bz - 1),
                conductor,
                "development alchemy qualification scene",
            );
            stations.push(pos);
        }
        let fire = reg
            .block_id("base:fire")
            .ok_or("capture registry lacks fire")?;
        let ice = reg
            .block_id("base:ice")
            .ok_or("capture registry lacks ice")?;
        self.runtime.local_mut().world.set_block_authored_at(
            chart.block(bx, y, bz + 1),
            fire,
            "development alchemy heat source",
        );
        self.runtime.local_mut().world.set_block_authored_at(
            chart.block(bx + 2, y, bz + 1),
            ice,
            "development alchemy cooling stock",
        );

        let actor = identity::local_player_id(
            &self.runtime.local().world.save_dir_for_saving(),
            self.identity.device_id(),
        )
        .map_err(|error| error.to_string())?;
        let mut work = Inventory::new();
        let mut last = None;
        for pos in stations {
            last = Some(self.runtime.local_mut().world.operate_alchemy(
                pos,
                &mut work,
                crate::alchemy::AlchemyRequest {
                    actor: actor.0,
                    actor_label: "development visual qualification".into(),
                    expected_revision: None,
                    action: crate::alchemy::ApparatusAction::Inspect,
                },
            )?);
        }
        for (name, count) in [
            ("base:glass_bottle", 4),
            ("base:glass_jar", 2),
            ("base:filter_cloth", 2),
            ("base:bucket_water", 1),
            ("base:pilgrim_root_cutting", 1),
        ] {
            let item = reg
                .item_id(name)
                .ok_or_else(|| format!("capture registry lacks {name}"))?;
            self.give_dev_item(&reg, item, count);
        }
        if let Some(result) = last {
            self.present_alchemy_cue(result.cue);
        }
        if std::env::var("WILDFORGE_POS").is_err() {
            self.player.pos = self
                .player
                .pos
                .relocated_local(Vec3::new(bx as f32, y as f32 + 3.2, bz as f32 + 5.5))
                .map_err(|error| format!("alchemy capture position failed: {error}"))?;
        }
        self.player.vel = Vec3::ZERO;
        self.flying = true;
        self.camera.follow_planet(self.player.eye());
        self.camera.yaw = -std::f32::consts::FRAC_PI_2;
        self.camera.pitch = -0.28;
        Ok(())
    }
}
