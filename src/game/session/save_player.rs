//! Save player graphical session adapter.

use crate::identity;
use crate::world;
use crate::game::Game;

impl Game {
    pub(in crate::game) fn save_player(&self) -> std::io::Result<()> {
        if !self.in_world || self.runtime.is_guest() || self.multiplayer.remote.is_some() {
            return Ok(());
        }
        use std::fmt::Write as _;
        let mut out = String::new();
        let p = self.player.pos;
        let _ = writeln!(out, "version = 2");
        let _ = writeln!(
            out,
            "face = {}\nu = {}\ny = {}\nv = {}",
            p.face() as u8,
            p.u(),
            p.y(),
            p.v()
        );
        let _ = writeln!(
            out,
            "yaw = {}\npitch = {}",
            self.camera.yaw, self.camera.pitch
        );
        let _ = writeln!(
            out,
            "health = {}\nhunger = {}",
            self.survival.health, self.survival.hunger
        );
        let _ = writeln!(out, "nutrition = {:?}", self.survival.nutrition);
        let _ = writeln!(out, "hotbar = {}", self.input.hotbar_sel);
        let _ = writeln!(
            out,
            "level = {}\nxp = {}\nskill_points = {}\nallocated = {:?}\nrespecs = {}",
            self.skills.level,
            self.skills.xp,
            self.skills.points,
            self.skills.allocated,
            self.skills.respecs
        );
        for (source, count) in &self.skills.source_counts {
            let _ = writeln!(out, "[[skill_xp]]\nsource = \"{source}\"\ncount = {count}");
        }
        let sp = self.survival.spawn_point;
        let _ = writeln!(
            out,
            "spawn_face = {}\nspawn_u = {}\nspawn_y = {}\nspawn_v = {}",
            sp.face() as u8,
            sp.u(),
            sp.y(),
            sp.v()
        );
        for (i, s) in self.inventory.slots.iter().enumerate() {
            if let Some(s) = s {
                let _ = writeln!(
                    out,
                    "[[slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}\narcane_id = {}",
                    self.content.reg.item(s.item).name,
                    s.count,
                    s.durability,
                    s.arcane_id
                );
            }
        }
        for (i, s) in self.survival.armor.iter().enumerate() {
            if let Some(s) = s {
                let _ = writeln!(
                    out,
                    "[[armor]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}\narcane_id = {}",
                    self.content.reg.item(s.item).name,
                    s.count,
                    s.durability,
                    s.arcane_id
                );
            }
        }
        for (i, loadout) in self.survival.loadouts.iter().enumerate() {
            if loadout.is_empty() {
                continue;
            }
            let _ = writeln!(out, "[[loadout]]\nindex = {i}");
            for component in &loadout.components {
                let _ = writeln!(
                    out,
                    "[[loadout.component]]\nitem = \"{}\"\ncount = {}\ndurability = {}\narcane_id = {}",
                    self.content.reg.item(component.stack.item).name,
                    component.stack.count,
                    component.stack.durability,
                    component.stack.arcane_id
                );
            }
        }
        for preset in &self.survival.loadout_presets {
            if preset.slots.iter().all(Option::is_none) {
                continue;
            }
            let _ = writeln!(out, "[[loadout_preset]]\nname = \"{}\"", preset.name);
            for (i, slot) in preset.slots.iter().enumerate() {
                let Some(slot) = slot else { continue };
                let _ = writeln!(
                    out,
                    "[[loadout_preset.slot]]\nindex = {i}\nframe = \"{}\"",
                    slot.frame
                );
                for component in &slot.components {
                    let _ = writeln!(
                        out,
                        "[[loadout_preset.slot.component]]\nitem = \"{component}\""
                    );
                }
            }
        }
        let world = self.runtime.local().world.save_dir_for_saving();
        let path = identity::local_profile_path(&world, self.identity.device_id())?;
        identity::atomic_write(&path, out.as_bytes(), false)?;
        identity::finish_local_profile_migration(&world);
        Ok(())
    }

    /// Persist every local-session component and keep enough context for a
    /// player-facing error. A remote guest owns none of this state.
    pub(in crate::game) fn save_session(&mut self) -> Result<String, String> {
        if self.runtime.is_guest() || self.multiplayer.remote.is_some() {
            return Ok("remote session has no local world state".into());
        }
        let mut failures = Vec::new();
        if let Err(error) = self.save_player() {
            failures.push(format!("player profile: {error}"));
        }
        let world_dir = self.runtime.local().world.save_dir_for_saving();
        if let Err(error) = self.save_loose_items(&world_dir) {
            failures.push(format!("loose items: {error}"));
        }
        self.runtime.local_mut().world.settle_falling();
        let world_report = self.runtime.local_mut().world.save_modified();
        if !world_report.is_ok() {
            failures.push(format!("world: {}", world_report.summary()));
        }
        if let Err(error) = self.content.scripts.save_kv(&world_dir) {
            failures.push(format!(
                "mod storage ({}): {error}",
                world_dir.join("modstore.toml").display()
            ));
        }
        if failures.is_empty() {
            Ok(world_report.summary())
        } else {
            Err(failures.join("; "))
        }
    }
}
