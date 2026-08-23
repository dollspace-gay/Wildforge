//! The survival ruleset: which world-pressure mechanics are live in a mode.
//!
//! Capability E1 of the belt-quest port plan. A world's `mode` string names
//! a ruleset; the registry resolves it from mod-declared `[[mode]]` entries
//! (see `modes.toml`), or the built-in `survival` / `creative` defaults.
//! This is the "modding out" mechanism — belt-quest declares a mode that
//! turns hunger off, repoints ire, and disables hearts while keeping the
//! rest of the survival sim.

/// One mode's toggle state. All fields are "is this pressure live?" — a
/// `creative` mode turns every toggle off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ruleset {
    /// Creative superset: no item costs, no survival pressure. A mode that
    /// inherits from creative keeps its builders' conveniences.
    pub creative: bool,
    /// Hunger drains, food gates sprint, starvation weakens.
    pub hunger: bool,
    /// Falling damages the player.
    pub fall_damage: bool,
    /// Submerged, the player drowns.
    pub drowning: bool,
    /// Standing in lava burns.
    pub lava_burn: bool,
    /// Night-time / ire-driven hostile wardens spawn.
    pub hostile_spawns: bool,
    /// Extraction and kills accrue regional ire (the moral meter).
    pub ire: bool,
    /// The hearts / offerings loop is live.
    pub hearts: bool,
    /// Storms and seasonal extremes intensify world pressure.
    pub weather_extremes: bool,
    /// Players can hurt each other.
    pub pvp: bool,
    /// The skill-tree progression (E5) is live and XP accrues. Off by
    /// default so Survival/Creative worlds are unchanged; a mod mode opts
    /// a world in with `skills = true`.
    pub skills: bool,
    /// Modular equipment (E6): frames take slotted components, loadouts
    /// derive stats, and frames disable at 0 durability instead of being
    /// destroyed. Off by default so Survival/Creative worlds are unchanged;
    /// a mod mode opts a world in with `equipment = true`.
    pub equipment: bool,
    /// Industrial response gradient (capability E12): running machines and
    /// industrial buildings feed regional ire alongside extraction. Live
    /// wherever `ire` is live; a mod mode can repoint it off with
    /// `industrial_ire = false`.
    pub industrial_ire: bool,
    /// Nest/den spawns (E9): species bound to a nest block spawn near it.
    /// Split from `hostile_spawns` so a mode can silence the warden ring
    /// yet keep its dens (belt-quest's enemy model), and so the Deep's
    /// dungeons populate regardless of overworld pressure.
    pub nest_spawns: bool,
}

impl Ruleset {
    /// The canonical survival experience: every pressure is live.
    pub fn survival() -> Self {
        Self {
            creative: false,
            hunger: true,
            fall_damage: true,
            drowning: true,
            lava_burn: true,
            hostile_spawns: true,
            ire: true,
            industrial_ire: true,
            nest_spawns: true,
            hearts: true,
            weather_extremes: true,
            pvp: true,
            skills: false,
            equipment: false,
        }
    }

    /// The canonical creative experience: no survival pressure at all.
    pub fn creative() -> Self {
        Self {
            creative: true,
            hunger: false,
            fall_damage: false,
            drowning: false,
            lava_burn: false,
            hostile_spawns: false,
            ire: false,
            hearts: false,
            weather_extremes: false,
            pvp: false,
            skills: false,
            equipment: false,
            industrial_ire: false,
            nest_spawns: false,
        }
    }

    /// Overlay one mode's declared overrides onto a base ruleset.
    pub fn apply_overrides(&mut self, mode: &crate::registry::ModeDef) {
        if let Some(value) = mode.creative {
            self.creative = value;
        }
        if let Some(value) = mode.hunger {
            self.hunger = value;
        }
        if let Some(value) = mode.fall_damage {
            self.fall_damage = value;
        }
        if let Some(value) = mode.drowning {
            self.drowning = value;
        }
        if let Some(value) = mode.lava_burn {
            self.lava_burn = value;
        }
        if let Some(value) = mode.hostile_spawns {
            self.hostile_spawns = value;
        }
        if let Some(value) = mode.ire {
            self.ire = value;
        }
        if let Some(value) = mode.hearts {
            self.hearts = value;
        }
        if let Some(value) = mode.weather_extremes {
            self.weather_extremes = value;
        }
        if let Some(value) = mode.pvp {
            self.pvp = value;
        }
        if let Some(value) = mode.skills {
            self.skills = value;
        }
        if let Some(value) = mode.equipment {
            self.equipment = value;
        }
        if let Some(value) = mode.industrial_ire {
            self.industrial_ire = value;
        }
        if let Some(value) = mode.nest_spawns {
            self.nest_spawns = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::ModeDef;

    fn mode(
        id: &str,
        base: Option<&str>,
        hunger: Option<bool>,
        pvp: Option<bool>,
        skills: Option<bool>,
    ) -> ModeDef {
        ModeDef {
            id: id.into(),
            base: base.map(String::from),
            creative: None,
            hunger,
            fall_damage: None,
            drowning: None,
            lava_burn: None,
            hostile_spawns: None,
            ire: None,
            hearts: None,
            weather_extremes: None,
            pvp,
            skills,
            equipment: None,
            industrial_ire: None,
            nest_spawns: None,
        }
    }

    #[test]
    fn skills_toggle_is_opt_in_and_off_by_default() {
        assert!(!Ruleset::survival().skills);
        assert!(!Ruleset::creative().skills);
        let mut r = Ruleset::survival();
        let m = mode("progression", Some("survival"), None, None, Some(true));
        r.apply_overrides(&m);
        assert!(r.skills);
        assert!(r.hunger && r.pvp, "skills overlay leaves other toggles alone");
    }

    #[test]
    fn equipment_toggle_is_opt_in_and_off_by_default() {
        assert!(!Ruleset::survival().equipment);
        assert!(!Ruleset::creative().equipment);
        let mut r = Ruleset::survival();
        let m = ModeDef {
            id: "gear".into(),
            base: Some("survival".into()),
            creative: None,
            hunger: None,
            fall_damage: None,
            drowning: None,
            lava_burn: None,
            hostile_spawns: None,
            ire: None,
            hearts: None,
            weather_extremes: None,
            pvp: None,
            skills: Some(false),
            equipment: Some(true),
            industrial_ire: None,
            nest_spawns: None,
        };
        r.apply_overrides(&m);
        assert!(r.equipment, "mode opts into modular equipment");
        assert!(!r.skills, "equipment overlay leaves other toggles alone");
    }

    #[test]
    fn builtin_rulesets_are_opposites() {
        let s = Ruleset::survival();
        let c = Ruleset::creative();
        assert!(!s.creative && s.hunger && s.fall_damage && s.pvp);
        assert!(c.creative && !c.hunger && !c.fall_damage && !c.pvp);
    }

    #[test]
    fn overrides_layer_onto_survival() {
        let mut r = Ruleset::survival();
        let m = mode("belt_quest", Some("survival"), Some(false), Some(false), Some(true));
        r.apply_overrides(&m);
        assert!(!r.hunger && !r.pvp && r.skills);
        assert!(r.fall_damage && r.drowning && r.hostile_spawns);
    }

    #[test]
    fn overrides_layer_onto_creative() {
        let mut r = Ruleset::creative();
        let m = mode("calm", Some("creative"), Some(true), None, None);
        r.apply_overrides(&m);
        assert!(r.creative && r.hunger && !r.pvp && !r.skills);
    }

    #[test]
    fn builtin_resolution_from_registry() {
        let reg = crate::registry::load(std::path::Path::new("/nonexistent-mods-dir"));
        let s = reg.ruleset_for("survival");
        let c = reg.ruleset_for("creative");
        let unknown = reg.ruleset_for("no_such_mode");
        assert_eq!(s, Ruleset::survival());
        assert_eq!(c, Ruleset::creative());
        assert_eq!(unknown, Ruleset::survival());
    }

    #[test]
    fn declared_mode_resolves_through_base_chain() {
        let dir = std::env::temp_dir().join(format!(
            "wildforge-ruleset-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let mod_dir = dir.join("rules");
        std::fs::create_dir_all(&mod_dir).unwrap();
        std::fs::write(
            mod_dir.join("mod.toml"),
            "id = \"rules\"\nname = \"Rules\"\nversion = \"1.0.0\"\nworld_api = 2\ndepends = [\"base\"]\n",
        )
        .unwrap();
        std::fs::write(
            mod_dir.join("modes.toml"),
            r#"
[[mode]]
id = "belt_quest"
base = "survival"
hunger = false
pvp = false

[[mode]]
id = "belt_quest_hard"
base = "belt_quest"
hearts = false
"#,
        )
        .unwrap();
        let reg = crate::registry::load(&dir);
        assert!(
            reg.material_errors.is_empty(),
            "mode load errors: {:?}",
            reg.material_errors
        );
        let soft = reg.ruleset_for("rules:belt_quest");
        assert!(!soft.hunger && !soft.pvp && soft.hearts && soft.hostile_spawns);
        let hard = reg.ruleset_for("rules:belt_quest_hard");
        assert!(!hard.hunger && !hard.pvp && !hard.hearts);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A declared mode really switches world mechanics: a mode with
    /// `hostile_spawns = false` and `ire = false` spawns no wardens even on
    /// a wrathful night and accrues no ire for breaking blocks.
    #[test]
    fn declared_mode_disables_warden_spawns_and_ire() {
        let dir = std::env::temp_dir().join(format!(
            "wildforge-ruleset-sim-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let mod_dir = dir.join("rules");
        std::fs::create_dir_all(&mod_dir).unwrap();
        std::fs::write(
            mod_dir.join("mod.toml"),
            "id = \"rules\"\nname = \"Rules\"\nversion = \"1.0.0\"\nworld_api = 2\ndepends = [\"base\"]\n",
        )
        .unwrap();
        std::fs::write(
            mod_dir.join("modes.toml"),
            r#"
[[mode]]
id = "calm"
base = "survival"
hostile_spawns = false
ire = false
"#,
        )
        .unwrap();
        let reg = std::sync::Arc::new(crate::registry::load(&dir));
        assert!(reg.material_errors.is_empty(), "{:?}", reg.material_errors);
        let mut w = crate::world::World::new(42, std::path::PathBuf::from("saves/.ruleset-sim"), reg.clone());
        w.mode = "rules:calm".into();
        w.ire = 95.0; // wrathful
        for x in -2..=2 {
            for z in -2..=2 {
                w.ensure_chunk(
                    crate::chunk::ChunkPos::from_centered(crate::planet::Face::PosZ, x, z)
                        .expect("test chunk"),
                );
            }
        }
        let player = crate::planet::EntityPos::from_local(
            crate::planet::Face::PosZ,
            glam::Vec3::new(8.0, (w.surface_height(8, 8) + 1) as f32, 8.0),
        )
        .expect("player pos");
        let world_spawn = crate::planet::EntityPos::from_local(
            crate::planet::Face::PosZ,
            glam::Vec3::new(-500.0, 70.0, -500.0),
        )
        .expect("spawn");
        let mut rng = 77u32;
        for _ in 0..300 {
            w.tick_hostile_spawns(player, world_spawn, 0.12, 5.0, &mut rng);
        }
        let hostiles: Vec<_> = w
            .mobs()
            .iter()
            .filter(|m| reg.animals[m.species].hostile)
            .collect();
        assert!(
            hostiles.is_empty(),
            "calm mode spawned {} wardens: {:?}",
            hostiles.len(),
            hostiles.iter().map(|m| &reg.animals[m.species].name).collect::<Vec<_>>()
        );
        // Block breaking accrues no ire in a mode with ire off. Survival
        // would raise the regional ledger for breaking ore; calm leaves it.
        let surface = player.block().unwrap().surface();
        let h = w.surface_height_at(surface);
        let at = crate::planet::BlockPos::new(
            surface.face(),
            surface.u(),
            (h - 1) as u8,
            surface.v(),
        )
        .expect("block pos");
        w.set_block_at(at, reg.block_id("base:copper_ore").expect("ore"));
        let before = w.regional_ire_at_surface(surface);
        w.break_block_at(at, None, true, true);
        let after = w.regional_ire_at_surface(surface);
        assert_eq!(before, after, "calm mode broke no ire");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reserved_and_broken_modes_are_reported() {
        let dir = std::env::temp_dir().join(format!(
            "wildforge-ruleset-bad-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let mod_dir = dir.join("rules");
        std::fs::create_dir_all(&mod_dir).unwrap();
        std::fs::write(
            mod_dir.join("mod.toml"),
            "id = \"rules\"\nname = \"Rules\"\nversion = \"1.0.0\"\nworld_api = 2\ndepends = [\"base\"]\n",
        )
        .unwrap();
        std::fs::write(
            mod_dir.join("modes.toml"),
            r#"
[[mode]]
id = "survival"
base = "survival"
hunger = false

[[mode]]
id = "loop"
base = "loop"
pvp = false
"#,
        )
        .unwrap();
        let reg = crate::registry::load(&dir);
        let errors = reg.material_errors.clone();
        assert!(
            errors.iter().any(|e| e.contains("built-in mode id is reserved")),
            "reserved mode not reported: {errors:?}"
        );
        assert!(
            errors.iter().any(|e| e.contains("cyclic base chain")),
            "cyclic mode not reported: {errors:?}"
        );
        // The broken modes still resolve safely to survival.
        assert_eq!(reg.ruleset_for("rules:loop"), Ruleset::survival());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
