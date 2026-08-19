//! Data-driven machine kinds (capability E7).
//!
//! The closed `MachineKind` enum becomes an index into `Registry::machines`:
//! a `machines.toml` entry declares a kind, its label, the closed native
//! handler that drives it, and the data knobs that handler reads. The
//! engine dispatches on `MachineDef::handler` instead of matching enum
//! variants, so a mod can add a machine (with a shell, recipes, and a
//! screen) without touching the engine.

use serde::Deserialize;

pub const MACHINES_SCHEMA_VERSION: u32 = 1;

/// The closed native machine handlers. Each handler drives one engine
/// behavior set: shell shape, lighting, ticking, click rules, and screen.
/// A `[[machine]]` entry binds a kind to exactly one handler; the knobs on
/// `MachineDef` tune it, the handler is never data.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum MachineHandler {
    Bloomery,
    Forge,
    Kiln,
    Separator,
    Workbench,
}

impl MachineHandler {
    pub const ALL: [MachineHandler; 5] = [
        MachineHandler::Bloomery,
        MachineHandler::Forge,
        MachineHandler::Kiln,
        MachineHandler::Separator,
        MachineHandler::Workbench,
    ];

    pub fn name(self) -> &'static str {
        match self {
            MachineHandler::Bloomery => "bloomery",
            MachineHandler::Forge => "forge",
            MachineHandler::Kiln => "kiln",
            MachineHandler::Separator => "separator",
            MachineHandler::Workbench => "workbench",
        }
    }

    pub fn from_name(name: &str) -> Option<MachineHandler> {
        Self::ALL.into_iter().find(|handler| handler.name() == name)
    }

    /// Whether this handler fires a lit face and must be lit with an ember.
    pub fn has_fire(self) -> bool {
        matches!(
            self,
            MachineHandler::Bloomery | MachineHandler::Forge | MachineHandler::Kiln
        )
    }

    /// Whether this handler is a hand-fed machine (the separator pattern):
    /// inputs go in and outputs come out by hand, with no container screen.
    pub fn hand_fed(self) -> bool {
        self == MachineHandler::Separator
    }

    /// Whether this handler is a recipe-list station (the workbench
    /// pattern): the screen lists the machine's station recipes and the
    /// player crafts from the inventory.
    pub fn is_station(self) -> bool {
        self == MachineHandler::Workbench
    }

    /// Whether a chimney over the core upgrades this kind (the kiln
    /// becomes a glassworks).
    pub fn reads_chimney(self) -> bool {
        self == MachineHandler::Kiln
    }

    /// Whether a chimney and an anvil must sit within the shell for this
    /// kind to validate (the forge's full workshop).
    pub fn requires_anvil(self) -> bool {
        self == MachineHandler::Forge
    }
}

/// One declared machine kind. Everything the engine needs about a machine
/// lives here; `MachineKind` (in `world::multiblock`) is just this list's
/// index.
#[derive(Clone, Debug)]
pub struct MachineDef {
    /// Qualified id, e.g. `base:bloomery` or `gems:jewel_bench`. Persisted
    /// as the save/UI name.
    pub id: String,
    pub label: String,
    pub handler: MachineHandler,
    /// Qualified block id of the mouth block (the placed station).
    pub mouth: String,
    /// Qualified block id of the lit face, if the handler has fire.
    pub mouth_lit: Option<String>,
    /// Seconds of fire before a batch completes (handlers with fire).
    pub fire_secs: f32,
    /// Charge slots (0-4).
    pub charge_slots: u8,
    /// Fuel slots (0-4).
    pub fuel_slots: u8,
    /// Reagent slots (0 or 1 — the kiln's pigment).
    pub reagent_slots: u8,
    /// Items produced per unit of fuel (the forge's thrifty 2).
    pub items_per_fuel: u32,
    /// Charge units the fire handler needs before it will light
    /// (bloomery and kiln want 2, the forge 1).
    pub min_charge: u8,
    /// Fuel units the fire handler needs before it will light.
    pub min_fuel: u8,
}

#[derive(Deserialize, Clone, Debug)]
pub struct RawMachineToml {
    #[serde(default)]
    pub schema_version: Option<u32>,
    #[serde(default)]
    pub machine: Vec<RawMachine>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct RawMachine {
    pub id: String,
    pub label: String,
    pub handler: String,
    pub mouth: String,
    #[serde(default)]
    pub mouth_lit: Option<String>,
    #[serde(default = "one_f32")]
    pub fire_secs: f32,
    #[serde(default)]
    pub charge_slots: u8,
    #[serde(default)]
    pub fuel_slots: u8,
    #[serde(default)]
    pub reagent_slots: u8,
    #[serde(default = "one_u32")]
    pub items_per_fuel: u32,
    #[serde(default)]
    pub min_charge: u8,
    #[serde(default)]
    pub min_fuel: u8,
}

fn one_f32() -> f32 {
    1.0
}

fn one_u32() -> u32 {
    1
}

fn qualify(modid: &str, name: &str) -> String {
    if name.contains(':') {
        name.to_string()
    } else {
        format!("{modid}:{name}")
    }
}

/// Qualify and validate every raw machine across the pack, in declaration
/// order. The returned index is the stable `MachineKind` id, so base
/// should declare its machines first (kind 0 is the default).
pub fn resolve(raws: &[(String, RawMachineToml)]) -> Result<Vec<MachineDef>, Vec<String>> {
    let mut errors = Vec::new();
    let mut out: Vec<MachineDef> = Vec::new();
    for (modid, raw) in raws {
        for machine in &raw.machine {
            match build(modid, machine) {
                Ok(definition) => {
                    if out.iter().any(|existing| existing.id == definition.id) {
                        errors.push(format!("{}: duplicate machine identity", definition.id));
                    } else {
                        out.push(definition);
                    }
                }
                Err(error) => errors.push(error),
            }
        }
    }
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors)
    }
}

fn build(modid: &str, machine: &RawMachine) -> Result<MachineDef, String> {
    let id = qualify(modid, &machine.id);
    if machine.id.is_empty() {
        return Err(format!("{modid}: machine id must not be empty"));
    }
    if machine.label.is_empty() {
        return Err(format!("{id}: machine label must not be empty"));
    }
    let Some(handler) = MachineHandler::from_name(&machine.handler) else {
        return Err(format!(
            "{id}: unknown machine handler \"{}\" (expected one of {})",
            machine.handler,
            MachineHandler::ALL
                .iter()
                .map(|h| h.name())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    };
    let mouth = qualify(modid, &machine.mouth);
    if machine.mouth.is_empty() {
        return Err(format!("{id}: machine mouth block must not be empty"));
    }
    let mouth_lit = machine.mouth_lit.as_deref().map(|lit| qualify(modid, lit));
    if machine.mouth_lit.as_deref().is_some_and(|lit| lit.is_empty()) {
        return Err(format!("{id}: mouth_lit block must not be empty"));
    }
    if machine.fire_secs < 0.0 {
        return Err(format!("{id}: fire_secs must not be negative"));
    }
    if machine.charge_slots > 4 || machine.fuel_slots > 4 {
        return Err(format!(
            "{id}: charge_slots and fuel_slots are capped at 4"
        ));
    }
    if machine.reagent_slots > 1 {
        return Err(format!("{id}: reagent_slots must be 0 or 1"));
    }
    if machine.items_per_fuel == 0 {
        return Err(format!("{id}: items_per_fuel must be at least 1"));
    }
    Ok(MachineDef {
        id,
        label: machine.label.clone(),
        handler,
        mouth,
        mouth_lit,
        fire_secs: machine.fire_secs,
        charge_slots: machine.charge_slots,
        fuel_slots: machine.fuel_slots,
        reagent_slots: machine.reagent_slots,
        items_per_fuel: machine.items_per_fuel,
        min_charge: machine.min_charge,
        min_fuel: machine.min_fuel,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(id: &str, handler: &str, mouth: &str) -> RawMachine {
        RawMachine {
            id: id.into(),
            label: id.to_uppercase(),
            handler: handler.into(),
            mouth: mouth.into(),
            mouth_lit: None,
            fire_secs: 1.0,
            charge_slots: 0,
            fuel_slots: 0,
            reagent_slots: 0,
            items_per_fuel: 1,
            min_charge: 0,
            min_fuel: 0,
        }
    }

    #[test]
    fn handler_names_round_trip() {
        for handler in MachineHandler::ALL {
            assert_eq!(MachineHandler::from_name(handler.name()), Some(handler));
        }
        assert_eq!(MachineHandler::from_name("nope"), None);
    }

    #[test]
    fn resolve_qualifies_and_orders() {
        let raws = vec![
            (
                "base".to_string(),
                RawMachineToml {
                    schema_version: None,
                    machine: vec![raw("bloomery", "bloomery", "base:bloomery")],
                },
            ),
            (
                "gems".to_string(),
                RawMachineToml {
                    schema_version: None,
                    machine: vec![raw("jewel_bench", "workbench", "gems:jewel_bench")],
                },
            ),
        ];
        let machines = resolve(&raws).expect("valid pack resolves");
        assert_eq!(machines.len(), 2);
        assert_eq!(machines[0].id, "base:bloomery");
        assert_eq!(machines[0].handler, MachineHandler::Bloomery);
        assert_eq!(machines[1].id, "gems:jewel_bench");
        assert_eq!(machines[1].handler, MachineHandler::Workbench);
    }

    #[test]
    fn unknown_handler_fails() {
        let raws = vec![(
            "base".to_string(),
            RawMachineToml {
                schema_version: None,
                machine: vec![raw("thing", "not_a_handler", "base:thing")],
            },
        )];
        let errors = resolve(&raws).expect_err("bad handler must fail");
        assert!(errors.iter().any(|e| e.contains("unknown machine handler")));
    }

    #[test]
    fn base_registry_ships_the_four_machines() {
        let reg = crate::registry::load(std::path::Path::new("/nonexistent-mods-dir"));
        let ids: Vec<String> = reg.machines.iter().map(|m| m.id.clone()).collect();
        assert!(
            reg.machine_kind("base:bloomery").is_some(),
            "base ships its machines: {ids:?}"
        );
        assert!(reg.machine_kind("base:forge").is_some());
        assert!(reg.machine_kind("base:kiln").is_some());
        assert!(reg.machine_kind("base:separator").is_some());
    }

    #[test]
    fn duplicate_ids_fail() {
        let raws = vec![
            ("base".to_string(), RawMachineToml {
                schema_version: None,
                machine: vec![raw("forge", "forge", "base:forge")],
            }),
            ("gems".to_string(), RawMachineToml {
                schema_version: None,
                machine: vec![raw("base:forge", "forge", "base:forge")],
            }),
        ];
        let errors = resolve(&raws).expect_err("duplicate id must fail");
        assert!(errors.iter().any(|e| e.contains("duplicate machine identity")));
    }
}
