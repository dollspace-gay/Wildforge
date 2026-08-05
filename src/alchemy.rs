//! Authoritative alchemical definitions and exact durable batch state.
//!
//! Preparations are declarative shells around a closed set of native effect
//! handlers. Content may describe ingredients and bounded process controls,
//! but it never receives arbitrary inventory, player, block, Current, or
//! water mutation. Runtime batches retain every exact carrier and solute
//! remainder so decanting cannot duplicate or round matter away.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::arcane::Current;
use crate::planet::BlockPos;
use crate::planet_atlas::ReservoirMass;
use crate::registry::MaterialVector;
use crate::workings::WaterCarrier;

pub const PREPARATIONS_SCHEMA_VERSION: u32 = 1;
pub const PREPARATION_DEFINITION_VERSION: u32 = 1;
pub const ALCHEMY_STATE_SCHEMA_VERSION: u32 = 1;
pub const ALCHEMY_FILE: &str = "alchemy.wfa";
const ALCHEMY_BACKUP: &str = "alchemy.wfa.bak";
pub const MAX_PREPARATION_DEFINITIONS: usize = 65_536;
pub const MAX_ALCHEMY_APPARATUS: usize = 4_096;
pub const MAX_ALCHEMY_CONTAINERS: usize = 65_536;
pub const MAX_ALCHEMY_STATUSES: usize = 8_192;
pub const MAX_ALCHEMY_HISTORY: usize = 4_096;
pub const MAX_ALCHEMY_FILE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_PREPARATION_ID_BYTES: usize = 96;
pub const MAX_PREPARATION_TEXT_BYTES: usize = 512;
pub const MAX_PREPARATION_INGREDIENTS: usize = 8;
/// Per-record sparse-vector bound. Base content uses only a handful of
/// entries; this leaves ample mod headroom while preventing one nominal dose,
/// residue, or pollution cell from hiding an unbounded save allocation.
pub const MAX_ALCHEMY_VECTOR_ENTRIES: usize = 64;
pub const MAX_PROCESS_STEPS: usize = 12;
pub const MAX_BATCH_VOLUME_UNITS: u64 = 4_096;
pub const MAX_BATCH_DOSES: u16 = 16;
pub const MAX_PROCESS_TICKS: u64 = 20 * 60 * 60;
pub const MAX_EFFECT_TICKS: u64 = 20 * 60 * 60 * 24;
pub const MAX_PREPARATION_CHARGE: u64 = 65_536;
pub const DOSE_VOLUME_UNITS: u64 = 64;
const STATUS_OWNER_BIT: u64 = 1 << 63;

fn definition_version() -> u32 {
    PREPARATION_DEFINITION_VERSION
}

fn default_retention_permille() -> u16 {
    800
}

fn default_cleanliness() -> u16 {
    700
}

fn default_documentation() -> String {
    "Undocumented preparation.".into()
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessKind {
    Grind,
    Infuse,
    Distill,
    Filter,
}

impl ProcessKind {
    pub const fn apparatus(self) -> ApparatusKind {
        match self {
            Self::Grind => ApparatusKind::Mortar,
            Self::Infuse => ApparatusKind::InfusionBasin,
            Self::Distill => ApparatusKind::Alembic,
            Self::Filter => ApparatusKind::FilterStand,
        }
    }

    const fn required_step(self) -> ProcessStep {
        match self {
            Self::Grind => ProcessStep::Grind,
            Self::Infuse => ProcessStep::Charge,
            Self::Distill => ProcessStep::Distill,
            Self::Filter => ProcessStep::Filter,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApparatusKind {
    Mortar,
    InfusionBasin,
    Alembic,
    FilterStand,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CarrierKind {
    FreshWater,
    Brine,
    Alcohol,
    PlantOil,
}

impl CarrierKind {
    pub const fn is_water(self) -> bool {
        matches!(self, Self::FreshWater | Self::Brine)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationKind {
    Drink,
    Plot,
    Wash,
    Coat,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PreparationHandler {
    TraceSight,
    NaturalRecovery,
    RootUptake,
    StrainRelief,
    DrossWash,
    PreserveSpecimen,
    ThroughputSurge,
    DrossAntidote,
}

impl PreparationHandler {
    pub const ALL: [Self; 8] = [
        Self::TraceSight,
        Self::NaturalRecovery,
        Self::RootUptake,
        Self::StrainRelief,
        Self::DrossWash,
        Self::PreserveSpecimen,
        Self::ThroughputSurge,
        Self::DrossAntidote,
    ];

    pub const fn application(self) -> ApplicationKind {
        match self {
            Self::TraceSight
            | Self::NaturalRecovery
            | Self::StrainRelief
            | Self::ThroughputSurge
            | Self::DrossAntidote => ApplicationKind::Drink,
            Self::RootUptake => ApplicationKind::Plot,
            Self::DrossWash => ApplicationKind::Wash,
            Self::PreserveSpecimen => ApplicationKind::Coat,
        }
    }

    pub const fn allowed_processes(self) -> &'static [ProcessKind] {
        use ProcessKind::{Distill, Filter, Infuse};
        match self {
            Self::TraceSight => &[Distill],
            Self::NaturalRecovery | Self::RootUptake | Self::StrainRelief => &[Infuse],
            Self::DrossWash | Self::DrossAntidote => &[Filter],
            Self::PreserveSpecimen => &[Infuse, Filter],
            Self::ThroughputSurge => &[Distill, Infuse],
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessStep {
    Grind,
    Load,
    Heat,
    Agitate,
    Charge,
    Settle,
    Distill,
    Filter,
    Cool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgitationKind {
    Still,
    Stirred,
    Shaken,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchFailure {
    WeakExtraction,
    ScorchedMash,
    BrokenEmulsion,
    SpentLiquor,
    FouledBatch,
    OverchargedBatch,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DisposalRoute {
    Soil,
    Runoff,
    Air,
    SealedWaste,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AlchemyTarget {
    SelfActor,
    Plot(BlockPos),
    Surface(BlockPos),
    Item(u64),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ApparatusAction {
    Inspect,
    Begin {
        preparation_id: String,
    },
    Grind {
        inventory_slot: u8,
    },
    TransferMash {
        destination: BlockPos,
    },
    LoadCarrier {
        inventory_slot: u8,
    },
    LoadFilter {
        inventory_slot: u8,
    },
    SetHeat {
        temperature_millic: i32,
    },
    SetAgitation {
        agitation: AgitationKind,
    },
    Advance {
        step: ProcessStep,
    },
    Charge {
        inventory_slot: Option<u8>,
        units: u64,
    },
    Sample,
    Decant {
        vessel_slot: u8,
    },
    Clean {
        water_slot: u8,
        filter_slot: Option<u8>,
    },
    Repair {
        material_slot: u8,
    },
    Drain {
        route: DisposalRoute,
    },
    Dismantle {
        route: DisposalRoute,
    },
    FermentAlcohol {
        water_slot: u8,
        wheat_slot: u8,
        berry_slot: u8,
    },
    PressOil {
        seed_slot: u8,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AlchemyRequest {
    pub actor: [u8; 16],
    pub actor_label: String,
    pub expected_revision: Option<u64>,
    pub action: ApparatusAction,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AlchemyCueKind {
    Grind,
    Bubble,
    Drip,
    Filter,
    Pour,
    Drink,
    Apply,
    Clean,
    Leak,
    Overcharge,
    Spoil,
    Pulse,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AlchemyCue {
    pub pos: BlockPos,
    pub installation_id: u64,
    pub batch_id: u64,
    pub revision: u64,
    pub kind: AlchemyCueKind,
    pub intensity: u8,
    pub color: [u8; 3],
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AlchemyResult {
    pub installation_id: u64,
    pub batch_id: u64,
    pub revision: u64,
    pub preparation_id: Option<String>,
    pub outcome: Option<BatchOutcome>,
    pub volume_units: u64,
    pub doses_remaining: u16,
    pub temperature_millic: i32,
    pub cleanliness_permille: u16,
    pub next_step: Option<ProcessStep>,
    pub cue: AlchemyCue,
    pub produced: Option<ProducedStack>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProducedStack {
    pub item_name: String,
    pub count: u32,
    pub durability: u32,
    pub arcane_id: u64,
}

impl ProducedStack {
    pub fn into_stack(
        self,
        registry: &crate::registry::Registry,
    ) -> Result<crate::inventory::ItemStack, AlchemyError> {
        let item = registry.item_id(&self.item_name).ok_or_else(|| {
            AlchemyError::Corrupt(format!("alchemy produced missing item {}", self.item_name))
        })?;
        Ok(crate::inventory::ItemStack {
            item,
            count: self.count,
            durability: self.durability,
            arcane_id: self.arcane_id,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PreparationPhysiology {
    pub health: f32,
    pub max_health: f32,
    pub hunger: f32,
    pub nutrition: [f32; 5],
    pub strain: f32,
    pub bodily_dross: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PreparationModifiers {
    pub trace_sight: u16,
    pub strain_permille: u16,
    pub throughput_permille: u16,
    pub drain_permille: u16,
    pub overdraw_permille: u16,
    pub storm_warning: bool,
    /// Host-authoritative environmental dross presentation. Zero is clear;
    /// 1..=5 follows trace, strained, seep, scar, and breach risk.
    pub dross_band: u8,
    /// Non-color accessibility grammar: 0 clear, 1 haze, 2 pulse, 3 branch,
    /// 4 broken ring, 5 repeating shear.
    pub dross_pattern: u8,
    pub recovery_permille: u16,
    pub perception_permille: u16,
    pub stamina_permille: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PreparationUseResult {
    pub preparation_id: String,
    pub source_batch: u64,
    pub status_id: Option<u64>,
    /// Present only when an exact dose was actually applied. Inspecting an
    /// aged container can instead re-label it as spent liquor in place.
    pub returned_vessel: Option<ProducedStack>,
    pub byproduct: Option<ProducedStack>,
    pub cue: AlchemyCue,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreparationTickResult {
    pub physiology: PreparationPhysiology,
    pub modifiers: PreparationModifiers,
    pub cues: Vec<AlchemyCue>,
}

impl Default for PreparationModifiers {
    fn default() -> Self {
        Self {
            trace_sight: 0,
            strain_permille: 1_000,
            throughput_permille: 1_000,
            drain_permille: 1_000,
            overdraw_permille: 1_000,
            storm_warning: false,
            dross_band: 0,
            dross_pattern: 0,
            recovery_permille: 1_000,
            perception_permille: 1_000,
            stamina_permille: 1_000,
        }
    }
}

impl BatchFailure {
    pub const ALL: [Self; 6] = [
        Self::WeakExtraction,
        Self::ScorchedMash,
        Self::BrokenEmulsion,
        Self::SpentLiquor,
        Self::FouledBatch,
        Self::OverchargedBatch,
    ];

    pub const fn item_id(self) -> &'static str {
        match self {
            Self::WeakExtraction => "base:weak_extraction",
            Self::ScorchedMash => "base:scorched_mash",
            Self::BrokenEmulsion => "base:broken_emulsion",
            Self::SpentLiquor => "base:spent_liquor",
            Self::FouledBatch => "base:fouled_batch",
            Self::OverchargedBatch => "base:overcharged_batch",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawPreparationIngredient {
    pub item: String,
    pub count: u16,
    #[serde(default = "default_retention_permille")]
    pub retention_permille: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawPreparationEffect {
    pub duration_ticks: u64,
    pub recovery_ticks: u64,
    pub strength: u32,
    #[serde(default)]
    pub hunger_cost_milli: u32,
    #[serde(default)]
    pub nutrient_cost: u16,
    #[serde(default)]
    pub water_hu: u32,
    #[serde(default)]
    pub dross_capacity: u32,
    #[serde(default)]
    pub throughput_permille: u16,
    #[serde(default)]
    pub drain_permille: u16,
    #[serde(default)]
    pub overdraw_permille: u16,
    #[serde(default)]
    pub preservation_permille: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawPreparationDef {
    pub id: String,
    pub label: String,
    #[serde(default = "definition_version")]
    pub version: u32,
    pub process: ProcessKind,
    pub handler: PreparationHandler,
    pub application: ApplicationKind,
    pub carrier: CarrierKind,
    pub solvent_item: String,
    pub solvent_units: u64,
    #[serde(default)]
    pub dissolved_units: u64,
    pub ingredients: Vec<RawPreparationIngredient>,
    pub charge_units: u64,
    pub resonance: String,
    pub charge_rate: [u32; 2],
    pub dross_units: u64,
    pub steps: Vec<ProcessStep>,
    pub temperature_millic: [i32; 2],
    pub process_ticks: u64,
    pub agitation: AgitationKind,
    #[serde(default = "default_cleanliness")]
    pub cleanliness_min: u16,
    pub output_item: String,
    pub empty_vessel: String,
    pub doses: u16,
    pub dose_units: u64,
    pub residue_item: String,
    pub residue_count: u16,
    pub shelf_life_ticks: u64,
    pub storage_temperature_millic: [i32; 2],
    pub stack_group: String,
    pub effect: RawPreparationEffect,
    #[serde(default = "default_documentation")]
    pub description: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct PreparationsFile {
    pub schema_version: Option<u32>,
    #[serde(default)]
    pub preparation: Vec<RawPreparationDef>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PreparationIngredient {
    pub item: String,
    pub count: u16,
    pub retention_permille: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PreparationEffectDef {
    pub duration_ticks: u64,
    pub recovery_ticks: u64,
    pub strength: u32,
    pub hunger_cost_milli: u32,
    pub nutrient_cost: u16,
    pub water_hu: u32,
    pub dross_capacity: u32,
    pub throughput_permille: u16,
    pub drain_permille: u16,
    pub overdraw_permille: u16,
    pub preservation_permille: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PreparationDef {
    pub id: String,
    pub provider: String,
    pub label: String,
    pub version: u32,
    pub process: ProcessKind,
    pub handler: PreparationHandler,
    pub application: ApplicationKind,
    pub carrier: CarrierKind,
    pub solvent_item: String,
    pub solvent_units: u64,
    pub dissolved_units: u64,
    pub ingredients: Vec<PreparationIngredient>,
    pub charge_units: u64,
    pub resonance: String,
    pub charge_rate: [u32; 2],
    pub dross_units: u64,
    pub steps: Vec<ProcessStep>,
    pub temperature_millic: [i32; 2],
    pub process_ticks: u64,
    pub agitation: AgitationKind,
    pub cleanliness_min: u16,
    pub output_item: String,
    pub empty_vessel: String,
    pub doses: u16,
    pub dose_units: u64,
    pub residue_item: String,
    pub residue_count: u16,
    pub shelf_life_ticks: u64,
    pub storage_temperature_millic: [i32; 2],
    pub stack_group: String,
    pub effect: PreparationEffectDef,
    pub description: String,
}

impl PreparationDef {
    pub fn from_raw(provider: &str, raw: RawPreparationDef) -> Result<Self, AlchemyError> {
        let definition = Self {
            id: qualify(provider, &raw.id),
            provider: provider.into(),
            label: raw.label,
            version: raw.version,
            process: raw.process,
            handler: raw.handler,
            application: raw.application,
            carrier: raw.carrier,
            solvent_item: qualify(provider, &raw.solvent_item),
            solvent_units: raw.solvent_units,
            dissolved_units: raw.dissolved_units,
            ingredients: raw
                .ingredients
                .into_iter()
                .map(|ingredient| PreparationIngredient {
                    item: qualify(provider, &ingredient.item),
                    count: ingredient.count,
                    retention_permille: ingredient.retention_permille,
                })
                .collect(),
            charge_units: raw.charge_units,
            resonance: qualify(provider, &raw.resonance),
            charge_rate: raw.charge_rate,
            dross_units: raw.dross_units,
            steps: raw.steps,
            temperature_millic: raw.temperature_millic,
            process_ticks: raw.process_ticks,
            agitation: raw.agitation,
            cleanliness_min: raw.cleanliness_min,
            output_item: qualify(provider, &raw.output_item),
            empty_vessel: qualify(provider, &raw.empty_vessel),
            doses: raw.doses,
            dose_units: raw.dose_units,
            residue_item: qualify(provider, &raw.residue_item),
            residue_count: raw.residue_count,
            shelf_life_ticks: raw.shelf_life_ticks,
            storage_temperature_millic: raw.storage_temperature_millic,
            stack_group: qualify(provider, &raw.stack_group),
            effect: PreparationEffectDef {
                duration_ticks: raw.effect.duration_ticks,
                recovery_ticks: raw.effect.recovery_ticks,
                strength: raw.effect.strength,
                hunger_cost_milli: raw.effect.hunger_cost_milli,
                nutrient_cost: raw.effect.nutrient_cost,
                water_hu: raw.effect.water_hu,
                dross_capacity: raw.effect.dross_capacity,
                throughput_permille: raw.effect.throughput_permille,
                drain_permille: raw.effect.drain_permille,
                overdraw_permille: raw.effect.overdraw_permille,
                preservation_permille: raw.effect.preservation_permille,
            },
            description: raw.description,
        };
        definition.validate()?;
        Ok(definition)
    }

    pub fn validate(&self) -> Result<(), AlchemyError> {
        if !valid_content_id(&self.id)
            || !valid_content_id(&self.solvent_item)
            || !valid_content_id(&self.resonance)
            || !valid_content_id(&self.output_item)
            || !valid_content_id(&self.empty_vessel)
            || !valid_content_id(&self.residue_item)
            || !valid_content_id(&self.stack_group)
            || !bounded_text(&self.provider, MAX_PREPARATION_ID_BYTES)
        {
            return Err(AlchemyError::InvalidContent(format!(
                "{}: preparation identities must be bounded lowercase qualified ids",
                self.id
            )));
        }
        if self.version != PREPARATION_DEFINITION_VERSION {
            return Err(AlchemyError::InvalidContent(format!(
                "{}: unsupported preparation version {}",
                self.id, self.version
            )));
        }
        if self.application != self.handler.application()
            || !self.handler.allowed_processes().contains(&self.process)
        {
            return Err(AlchemyError::InvalidContent(format!(
                "{}: {:?} cannot use {:?}/{:?}",
                self.id, self.handler, self.process, self.application
            )));
        }
        if self.solvent_units == 0
            || self.solvent_units > MAX_BATCH_VOLUME_UNITS
            || self.dissolved_units > MAX_BATCH_VOLUME_UNITS
            || self
                .solvent_units
                .checked_add(self.dissolved_units)
                .is_none_or(|volume| volume > MAX_BATCH_VOLUME_UNITS)
            || self.dose_units == 0
            || self.dose_units > MAX_BATCH_VOLUME_UNITS
            || self.doses == 0
            || self.doses > MAX_BATCH_DOSES
            || u64::from(self.doses).saturating_mul(self.dose_units)
                > self.solvent_units.saturating_add(self.dissolved_units)
        {
            return Err(AlchemyError::InvalidContent(format!(
                "{}: batch output exceeds its finite carrier/displacement or dose bounds",
                self.id
            )));
        }
        if self.ingredients.is_empty()
            || self.ingredients.len() > MAX_PREPARATION_INGREDIENTS
            || self.ingredients.iter().any(|ingredient| {
                ingredient.count == 0
                    || ingredient.retention_permille > 1_000
                    || !valid_content_id(&ingredient.item)
            })
            || self
                .ingredients
                .iter()
                .map(|ingredient| &ingredient.item)
                .collect::<BTreeSet<_>>()
                .len()
                != self.ingredients.len()
        {
            return Err(AlchemyError::InvalidContent(format!(
                "{}: ingredients must be unique, physical, bounded quantities",
                self.id
            )));
        }
        if self.charge_units == 0
            || self.charge_units > MAX_PREPARATION_CHARGE
            || self.dross_units > self.charge_units
            || self.charge_rate[0] == 0
            || self.charge_rate[0] > self.charge_rate[1]
            || u64::from(self.charge_rate[1]) > MAX_PREPARATION_CHARGE
        {
            return Err(AlchemyError::InvalidContent(format!(
                "{}: charge, dross, or charge-rate bounds are invalid",
                self.id
            )));
        }
        if self.steps.is_empty()
            || self.steps.len() > MAX_PROCESS_STEPS
            || self.steps.iter().copied().collect::<BTreeSet<_>>().len() != self.steps.len()
            || !self.steps.contains(&ProcessStep::Load)
            || !self.steps.contains(&self.process.required_step())
        {
            return Err(AlchemyError::InvalidContent(format!(
                "{}: process order is empty, duplicated, or omits its native operation",
                self.id
            )));
        }
        let [minimum_temperature, maximum_temperature] = self.temperature_millic;
        let [storage_minimum, storage_maximum] = self.storage_temperature_millic;
        if minimum_temperature < -50_000
            || maximum_temperature > 250_000
            || minimum_temperature > maximum_temperature
            || storage_minimum < -50_000
            || storage_maximum > 100_000
            || storage_minimum > storage_maximum
            || self.process_ticks == 0
            || self.process_ticks > MAX_PROCESS_TICKS
            || self.cleanliness_min > 1_000
        {
            return Err(AlchemyError::InvalidContent(format!(
                "{}: heat, time, storage, or cleanliness controls are unbounded",
                self.id
            )));
        }
        if self.residue_count == 0
            || self.shelf_life_ticks == 0
            || self.shelf_life_ticks > MAX_EFFECT_TICKS.saturating_mul(365)
            || !bounded_text(&self.label, MAX_PREPARATION_ID_BYTES)
            || self.description == default_documentation()
            || !bounded_text(&self.description, MAX_PREPARATION_TEXT_BYTES)
        {
            return Err(AlchemyError::InvalidContent(format!(
                "{}: residue, shelf life, label, or documentation is incomplete",
                self.id
            )));
        }
        self.effect.validate(self.handler, &self.id)
    }

    pub fn validate_registry(
        &self,
        registry: &crate::registry::Registry,
    ) -> Result<(), AlchemyError> {
        let mut referenced = vec![
            self.solvent_item.as_str(),
            self.output_item.as_str(),
            self.empty_vessel.as_str(),
            self.residue_item.as_str(),
        ];
        referenced.extend(
            self.ingredients
                .iter()
                .map(|ingredient| ingredient.item.as_str()),
        );
        if let Some(missing) = referenced
            .into_iter()
            .find(|item| registry.item_id(item).is_none())
        {
            return Err(AlchemyError::InvalidContent(format!(
                "{}: references missing physical item {missing}",
                self.id
            )));
        }
        if !registry
            .arcane_registry
            .definitions
            .contains_key(&self.resonance)
        {
            return Err(AlchemyError::InvalidContent(format!(
                "{}: references unknown resonance {}",
                self.id, self.resonance
            )));
        }
        let output = registry.item(registry.item_id(&self.output_item).unwrap());
        if output.max_stack != 1 || output.arcane.is_none() {
            return Err(AlchemyError::InvalidContent(format!(
                "{}: a state-bearing filled preparation needs max_stack = 1 and a destruction disposition",
                self.id
            )));
        }
        if self.handler == PreparationHandler::DrossAntidote
            && !self
                .ingredients
                .iter()
                .any(|ingredient| ingredient.item.ends_with(":ashlace_tissue"))
        {
            return Err(AlchemyError::InvalidContent(format!(
                "{}: a dross antidote needs declared Ashlace-derived sequestration matter",
                self.id
            )));
        }
        Ok(())
    }
}

impl PreparationEffectDef {
    fn validate(&self, handler: PreparationHandler, id: &str) -> Result<(), AlchemyError> {
        if self.duration_ticks == 0
            || self.duration_ticks > MAX_EFFECT_TICKS
            || self.recovery_ticks > MAX_EFFECT_TICKS
            || self.strength == 0
            || self.strength > 10_000
        {
            return Err(AlchemyError::InvalidContent(format!(
                "{id}: effect duration, recovery, or magnitude is outside native bounds"
            )));
        }
        let valid = match handler {
            PreparationHandler::TraceSight => {
                self.strength <= 250 && self.hunger_cost_milli == 0 && self.dross_capacity == 0
            }
            PreparationHandler::NaturalRecovery => {
                self.strength <= 20 && self.hunger_cost_milli >= 500 && self.nutrient_cost > 0
            }
            PreparationHandler::RootUptake => {
                self.strength <= 1_000 && self.water_hu > 0 && self.nutrient_cost > 0
            }
            PreparationHandler::StrainRelief => {
                (250..1_000).contains(&self.throughput_permille) && self.recovery_ticks > 0
            }
            PreparationHandler::DrossWash | PreparationHandler::DrossAntidote => {
                self.dross_capacity > 0 && self.dross_capacity <= 256
            }
            PreparationHandler::PreserveSpecimen => {
                (100..1_000).contains(&self.preservation_permille)
            }
            PreparationHandler::ThroughputSurge => {
                (1_001..=1_500).contains(&self.throughput_permille)
                    && self.drain_permille >= self.throughput_permille
                    && self.overdraw_permille >= 1_100
                    && self.recovery_ticks > 0
            }
        };
        if valid {
            Ok(())
        } else {
            Err(AlchemyError::InvalidContent(format!(
                "{id}: effect parameters do not satisfy the closed {handler:?} capability"
            )))
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExactLiquid {
    pub carrier: Option<CarrierKind>,
    pub volume_units: u64,
    #[serde(with = "reservoir_mass_serde")]
    pub water: ReservoirMass,
    pub carrier_state: WaterCarrier,
    pub solutes: BTreeMap<String, u64>,
}

mod reservoir_mass_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use crate::planet_atlas::ReservoirMass;

    pub fn serialize<S>(mass: &ReservoirMass, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (mass.water_hu, mass.salt_mass).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ReservoirMass, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (water_hu, salt_mass) = <(u64, u64)>::deserialize(deserializer)?;
        Ok(ReservoirMass {
            water_hu,
            salt_mass,
        })
    }
}

impl ExactLiquid {
    pub fn validate(&self) -> Result<(), AlchemyError> {
        if self.volume_units > MAX_BATCH_VOLUME_UNITS
            || self.water.water_hu > self.volume_units
            || self.carrier.is_none() != (self.volume_units == 0)
            || self.solutes.len() > MAX_ALCHEMY_VECTOR_ENTRIES
            || self.solutes.iter().any(|(name, units)| {
                !valid_content_id(name) || *units == 0 || *units > u64::from(u32::MAX)
            })
            || self.carrier_state.thermal_millic_hu.unsigned_abs()
                > self.volume_units.saturating_mul(250_000)
            || self.carrier_state.dross_subunits
                > self.volume_units.saturating_mul(u64::from(u32::MAX))
        {
            return Err(AlchemyError::Corrupt(
                "alchemy liquid has impossible volume, carrier, heat, dross, or solute state"
                    .into(),
            ));
        }
        if self.carrier.is_some_and(CarrierKind::is_water)
            && (self.water.water_hu == 0 || self.water.water_hu > self.volume_units)
        {
            return Err(AlchemyError::Corrupt(
                "a water/brine solution needs nonzero exact planetary water custody no greater than its displaced volume".into(),
            ));
        }
        Ok(())
    }

    pub fn take(&mut self, requested_units: u64) -> Result<Self, AlchemyError> {
        self.validate()?;
        if requested_units == 0 || requested_units > self.volume_units {
            return Err(AlchemyError::InvalidOperation(
                "dose volume exceeds the exact liquid remaining".into(),
            ));
        }
        if requested_units == self.volume_units {
            return Ok(std::mem::take(self));
        }
        let before = self.volume_units;
        let water = self.water.take(
            u64::try_from(
                u128::from(self.water.water_hu) * u128::from(requested_units) / u128::from(before),
            )
            .map_err(|_| AlchemyError::Overflow)?,
        );
        let carrier_state = self
            .carrier_state
            .take(before, requested_units)
            .ok_or(AlchemyError::Overflow)?;
        let mut solutes = BTreeMap::new();
        for (name, remaining) in &mut self.solutes {
            let moved = u64::try_from(
                u128::from(*remaining) * u128::from(requested_units) / u128::from(before),
            )
            .map_err(|_| AlchemyError::Overflow)?;
            *remaining -= moved;
            if moved != 0 {
                solutes.insert(name.clone(), moved);
            }
        }
        self.solutes.retain(|_, units| *units != 0);
        self.volume_units -= requested_units;
        let parcel = Self {
            carrier: self.carrier,
            volume_units: requested_units,
            water,
            carrier_state,
            solutes,
        };
        self.validate()?;
        parcel.validate()?;
        Ok(parcel)
    }

    pub fn checked_add(&mut self, other: Self) -> Result<(), AlchemyError> {
        self.validate()?;
        other.validate()?;
        if self.volume_units == 0 {
            *self = other;
            return Ok(());
        }
        if other.volume_units == 0 || self.carrier != other.carrier {
            return Err(AlchemyError::InvalidOperation(
                "only matching finite carriers can be combined".into(),
            ));
        }
        self.volume_units = self
            .volume_units
            .checked_add(other.volume_units)
            .filter(|volume| *volume <= MAX_BATCH_VOLUME_UNITS)
            .ok_or(AlchemyError::Overflow)?;
        self.water = self
            .water
            .checked_add(other.water)
            .ok_or(AlchemyError::Overflow)?;
        self.carrier_state = self
            .carrier_state
            .checked_add(other.carrier_state)
            .ok_or(AlchemyError::Overflow)?;
        add_material_vector(&mut self.solutes, &other.solutes)?;
        self.validate()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BatchIngredientState {
    pub item: String,
    pub count: u16,
    /// Lowest measured condition among the physical units admitted to this
    /// ingredient lot. Organic freshness uses the ordinary item clock;
    /// non-perishables enter at full condition.
    #[serde(default = "full_condition_permille")]
    pub condition_permille: u16,
    pub retained_materials: MaterialVector,
    pub residue_materials: MaterialVector,
    pub source_arcane_ids: Vec<u64>,
}

const fn full_condition_permille() -> u16 {
    1_000
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BatchOutcome {
    Processing,
    Ready,
    Failed(BatchFailure),
    Spoiled,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcessObservation {
    pub step: ProcessStep,
    pub tick: u64,
    pub temperature_millic: i32,
    pub agitation: AgitationKind,
    pub cleanliness_permille: u16,
    pub charge_delta: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AlchemyBatch {
    pub id: u64,
    pub preparation_id: String,
    pub definition_version: u32,
    pub actor: [u8; 16],
    pub actor_label: String,
    pub installation_id: u64,
    pub liquid: ExactLiquid,
    /// Water physically carried by harvested wet ingredients (initially
    /// Rainbell Dew). It remains in mash/residue rather than increasing the
    /// declared bottled yield, and returns through cleaning or disposal.
    #[serde(default, with = "reservoir_mass_serde")]
    pub residue_water: ReservoirMass,
    pub ingredients: Vec<BatchIngredientState>,
    /// Total clean charge admitted before the process's declared conversion
    /// into dross. Kept separately so `current_units` always mirrors the live
    /// clean ledger account after that conversion.
    pub charge_input_units: u64,
    pub current_units: u64,
    pub dross_units: u64,
    pub step_index: u8,
    pub observations: Vec<ProcessObservation>,
    pub started_tick: u64,
    pub due_tick: u64,
    pub born_tick: u64,
    pub expires_tick: u64,
    pub outcome: BatchOutcome,
    pub revision: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AlchemyApparatusState {
    pub installation_id: u64,
    pub kind: ApparatusKind,
    pub pos: BlockPos,
    pub revision: u64,
    pub integrity_permille: u16,
    pub cleanliness_permille: u16,
    pub temperature_millic: i32,
    pub agitation: AgitationKind,
    pub batch: Option<AlchemyBatch>,
    pub residue_materials: MaterialVector,
    pub filter_burden: u64,
    /// One physical disposable medium mounted for the next filter step.
    pub filter_medium: Option<String>,
    /// Stable active-owner identity for dross captured by the mounted medium.
    /// Zero means no medium is installed.
    #[serde(default)]
    pub filter_owner_id: u64,
    pub filter_medium_materials: MaterialVector,
    pub last_operator: [u8; 16],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PreparationDose {
    pub container_id: u64,
    pub preparation_id: String,
    pub definition_version: u32,
    pub item_name: String,
    pub liquid: ExactLiquid,
    /// Exact ordinary matter carried by the reusable vessel itself. Filled
    /// dose item definitions intentionally do not invent a second bottle;
    /// this vector returns with the empty vessel or follows broken glass into
    /// local salvage.
    #[serde(default)]
    pub vessel_materials: MaterialVector,
    /// Exact retained ingredient matter suspended in this dose.
    pub materials: MaterialVector,
    pub current_units: u64,
    pub dross_units: u64,
    pub born_tick: u64,
    pub expires_tick: u64,
    /// Last authoritative carried-storage temperature assessment. Absolute
    /// time still ages every dose; this prevents repeated sweeps/relogs from
    /// applying the same hot-storage penalty twice.
    #[serde(default)]
    pub last_storage_tick: u64,
    pub outcome: BatchOutcome,
    pub source_installation: u64,
    pub source_batch: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActivePreparationStatus {
    pub status_id: u64,
    pub preparation_id: String,
    pub definition_version: u32,
    pub source_batch: u64,
    pub actor: [u8; 16],
    pub dose_volume_units: u64,
    pub active_current: Current,
    pub dross_current: Current,
    pub started_tick: u64,
    pub last_tick: u64,
    pub due_tick: u64,
    pub recovery_until_tick: u64,
    pub stack_group: String,
    pub completed_units: u64,
    pub refresh_count: u8,
    pub overdose_until_tick: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RootTreatment {
    pub source_batch: u64,
    pub actor: [u8; 16],
    pub applied_tick: u64,
    pub expires_tick: u64,
    pub water_hu: u64,
    pub nutrient_units: u64,
    pub uptake_permille: u16,
    pub concentration_permille: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpecimenCoating {
    pub status_id: u64,
    pub item_id: u64,
    pub source_batch: u64,
    pub actor: [u8; 16],
    pub applied_pos: BlockPos,
    pub applied_tick: u64,
    pub expires_tick: u64,
    pub preservation_permille: u16,
    pub maximum_temperature_millic: i32,
    pub age_paid: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AlchemyPollution {
    #[serde(with = "reservoir_mass_serde")]
    pub water: ReservoirMass,
    pub carrier_units: BTreeMap<CarrierKind, u64>,
    pub materials: MaterialVector,
    pub solutes: BTreeMap<String, u64>,
    pub dross: Current,
    pub last_actor: [u8; 16],
    pub last_operation_id: u64,
    pub updated_tick: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum OrdinaryProcessKind {
    FermentAlcohol,
    PressOil,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OrdinaryProcessJob {
    pub kind: OrdinaryProcessKind,
    pub installation_id: u64,
    pub actor: [u8; 16],
    pub started_tick: u64,
    pub due_tick: u64,
    pub output_item: String,
    pub output_count: u16,
    #[serde(with = "reservoir_mass_serde")]
    pub process_water: ReservoirMass,
    pub input_materials: MaterialVector,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AlchemyAuditEvent {
    pub operation_id: u64,
    pub installation_id: u64,
    pub batch_id: u64,
    pub actor: [u8; 16],
    pub action: String,
    pub preparation_id: String,
    pub volume_units: u64,
    pub current_units: u64,
    pub dross_units: u64,
    pub tick: u64,
    pub note: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AlchemyState {
    pub schema_version: u32,
    pub content_hash: u64,
    pub next_installation_id: u64,
    pub next_batch_id: u64,
    pub next_status_id: u64,
    pub next_operation_id: u64,
    pub apparatus: BTreeMap<BlockPos, AlchemyApparatusState>,
    pub containers: BTreeMap<u64, PreparationDose>,
    pub statuses: BTreeMap<[u8; 16], Vec<ActivePreparationStatus>>,
    pub root_treatments: BTreeMap<BlockPos, RootTreatment>,
    pub coatings: BTreeMap<u64, SpecimenCoating>,
    pub pollution: BTreeMap<BlockPos, AlchemyPollution>,
    pub ordinary_jobs: BTreeMap<BlockPos, OrdinaryProcessJob>,
    pub history: VecDeque<AlchemyAuditEvent>,
    /// Persistent round-robin positions for bounded background maintenance.
    /// Storing the last visited key avoids repeatedly favoring the first
    /// lexicographic laboratories and containers after every server tick.
    #[serde(default)]
    pub maintenance_phase: u8,
    #[serde(default)]
    pub maintenance_apparatus_cursor: Option<BlockPos>,
    #[serde(default)]
    pub maintenance_container_cursor: u64,
    #[serde(default)]
    pub maintenance_root_cursor: Option<BlockPos>,
    #[serde(default)]
    pub maintenance_coating_cursor: u64,
    #[serde(skip)]
    path: PathBuf,
}

/// Closed-world operator evidence for embodied preparations and every parent
/// ledger they touch.  The sidecar is descriptive custody: the Arcane,
/// planetary-water, and material ledgers remain authoritative, so a saved
/// preparation is qualified only when both views agree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AlchemyAudit {
    pub schema_version: u32,
    pub content_hash: u64,
    pub file_bytes: u64,
    pub apparatus: usize,
    pub batches: usize,
    pub containers: usize,
    pub statuses: usize,
    pub root_treatments: usize,
    pub coatings: usize,
    pub pollution_sites: usize,
    pub ordinary_jobs: usize,
    pub history: usize,
    pub water_hu: u64,
    pub salt_mass: u64,
    pub clean_current: u64,
    pub dross_current: u64,
    pub tracked_material_units: u64,
    pub missing_arcane_accounts: usize,
    pub mismatched_arcane_accounts: usize,
    pub water_custody_overdrawn: bool,
    pub arcane_balanced: bool,
    pub water_balanced: bool,
    pub material_balanced: bool,
}

impl AlchemyAudit {
    pub fn is_qualified(&self) -> bool {
        self.schema_version == ALCHEMY_STATE_SCHEMA_VERSION
            && self.file_bytes <= MAX_ALCHEMY_FILE_BYTES
            && self.apparatus <= MAX_ALCHEMY_APPARATUS
            && self.containers <= MAX_ALCHEMY_CONTAINERS
            && self.statuses <= MAX_ALCHEMY_STATUSES
            && self.history <= MAX_ALCHEMY_HISTORY
            && self.missing_arcane_accounts == 0
            && self.mismatched_arcane_accounts == 0
            && !self.water_custody_overdrawn
            && self.arcane_balanced
            && self.water_balanced
            && self.material_balanced
    }

    pub fn render(&self) -> String {
        format!(
            concat!(
                "Alchemy audit schema {}\n",
                "Content hash: {:016x}\n",
                "Sidecar: {} / {} bytes\n",
                "Embodied state: {} apparatus, {} batches, {} filled containers, {} statuses, {} root treatments, {} coatings, {} pollution sites, {} ordinary jobs\n",
                "History: {} / {}\n",
                "Custody: {} water HU, {} salt, {} clean Current, {} dross Current, {} tracked material units\n",
                "Integrity: {} missing arcane accounts, {} mismatched arcane accounts, industrial water overdrawn={}\n",
                "Parent ledgers: Current={}, water/salt={}, material={}\n",
                "Qualified: {}\n"
            ),
            self.schema_version,
            self.content_hash,
            self.file_bytes,
            MAX_ALCHEMY_FILE_BYTES,
            self.apparatus,
            self.batches,
            self.containers,
            self.statuses,
            self.root_treatments,
            self.coatings,
            self.pollution_sites,
            self.ordinary_jobs,
            self.history,
            MAX_ALCHEMY_HISTORY,
            self.water_hu,
            self.salt_mass,
            self.clean_current,
            self.dross_current,
            self.tracked_material_units,
            self.missing_arcane_accounts,
            self.mismatched_arcane_accounts,
            self.water_custody_overdrawn,
            self.arcane_balanced,
            self.water_balanced,
            self.material_balanced,
            self.is_qualified(),
        )
    }
}

impl AlchemyState {
    pub fn load_existing(world: &Path) -> Result<Option<Self>, AlchemyError> {
        let path = world.join(ALCHEMY_FILE);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let mut state = Self::decode(&bytes)?;
        state.path = path;
        state.validate()?;
        Ok(Some(state))
    }

    pub fn load_or_initialize(world: &Path, content_hash: u64) -> Result<Self, AlchemyError> {
        let path = world.join(ALCHEMY_FILE);
        if !path.exists() {
            return Ok(Self {
                schema_version: ALCHEMY_STATE_SCHEMA_VERSION,
                content_hash,
                next_installation_id: 1,
                next_batch_id: 1,
                next_status_id: 1,
                next_operation_id: 1,
                apparatus: BTreeMap::new(),
                containers: BTreeMap::new(),
                statuses: BTreeMap::new(),
                root_treatments: BTreeMap::new(),
                coatings: BTreeMap::new(),
                pollution: BTreeMap::new(),
                ordinary_jobs: BTreeMap::new(),
                history: VecDeque::new(),
                maintenance_phase: 0,
                maintenance_apparatus_cursor: None,
                maintenance_container_cursor: 0,
                maintenance_root_cursor: None,
                maintenance_coating_cursor: 0,
                path,
            });
        }
        let primary = std::fs::read(&path)?;
        let mut state = match Self::decode(&primary) {
            Ok(state) => state,
            Err(primary_error) => {
                let backup_path = world.join(ALCHEMY_BACKUP);
                let backup = std::fs::read(&backup_path).map_err(|backup_error| {
                    AlchemyError::Corrupt(format!(
                        "primary alchemy state failed ({primary_error}); backup could not be read: {backup_error}"
                    ))
                })?;
                let recovered = Self::decode(&backup).map_err(|backup_error| {
                    AlchemyError::Corrupt(format!(
                        "primary alchemy state failed ({primary_error}); backup failed ({backup_error})"
                    ))
                })?;
                crate::identity::atomic_write(&path, &backup, false)?;
                recovered
            }
        };
        state.path = path;
        state.validate()?;
        if state.content_hash != content_hash {
            // Saved batches retain their versioned definition identity. New
            // content is allowed alongside them; runtime lookup refuses an
            // incompatible active definition rather than rewriting a batch.
            state.content_hash = content_hash;
        }
        Ok(state)
    }

    fn decode(bytes: &[u8]) -> Result<Self, AlchemyError> {
        if bytes.len() as u64 > MAX_ALCHEMY_FILE_BYTES {
            return Err(AlchemyError::Corrupt(
                "alchemy sidecar exceeds its bounded file budget".into(),
            ));
        }
        postcard::from_bytes(bytes).map_err(|error| AlchemyError::Corrupt(error.to_string()))
    }

    pub fn validate(&self) -> Result<(), AlchemyError> {
        let status_count = self.statuses.values().map(Vec::len).sum::<usize>();
        if self.schema_version != ALCHEMY_STATE_SCHEMA_VERSION
            || self.next_installation_id == 0
            || self.next_batch_id == 0
            || self.next_status_id == 0
            || self.next_operation_id == 0
            || self.apparatus.len() > MAX_ALCHEMY_APPARATUS
            || self.containers.len() > MAX_ALCHEMY_CONTAINERS
            || status_count > MAX_ALCHEMY_STATUSES
            || self.history.len() > MAX_ALCHEMY_HISTORY
            || self.root_treatments.len() > MAX_ALCHEMY_CONTAINERS
            || self.coatings.len() > MAX_ALCHEMY_CONTAINERS
            || self.pollution.len() > MAX_ALCHEMY_APPARATUS
            || self.ordinary_jobs.len() > MAX_ALCHEMY_APPARATUS
            || self.maintenance_phase >= 4
        {
            return Err(AlchemyError::Corrupt(
                "alchemy state schema, allocator, or census is invalid".into(),
            ));
        }
        let mut batch_ids = BTreeSet::new();
        let mut installation_ids = BTreeSet::new();
        let mut maximum_installation_id = 0u64;
        let mut maximum_batch_id = 0u64;
        let mut maximum_status_id = 0u64;
        let mut maximum_operation_id = 0u64;
        for (pos, apparatus) in &self.apparatus {
            if *pos != apparatus.pos
                || apparatus.installation_id == 0
                || !installation_ids.insert(apparatus.installation_id)
                || apparatus.revision == 0
                || apparatus.integrity_permille > 1_000
                || apparatus.cleanliness_permille > 1_000
                || !(-50_000..=250_000).contains(&apparatus.temperature_millic)
                || apparatus.filter_burden > u64::from(u32::MAX)
                || !valid_material_vector(&apparatus.residue_materials)
                || !valid_material_vector(&apparatus.filter_medium_materials)
                || (apparatus.filter_medium.is_none()
                    && (!apparatus.filter_medium_materials.is_empty()
                        || apparatus.filter_owner_id != 0))
                || (apparatus.filter_medium.is_some() && apparatus.filter_owner_id == 0)
                || apparatus
                    .filter_medium
                    .as_ref()
                    .is_some_and(|medium| !valid_content_id(medium))
                || (apparatus.kind != ApparatusKind::FilterStand
                    && (apparatus.filter_medium.is_some() || apparatus.filter_burden != 0))
            {
                return Err(AlchemyError::Corrupt(
                    "alchemy apparatus identity or physical condition is invalid".into(),
                ));
            }
            maximum_installation_id = maximum_installation_id.max(apparatus.installation_id);
            if let Some(batch) = &apparatus.batch {
                validate_batch(batch, apparatus.installation_id)?;
                maximum_batch_id = maximum_batch_id.max(batch.id);
                if !batch_ids.insert(batch.id) {
                    return Err(AlchemyError::Corrupt(
                        "one stable batch appears in more than one apparatus".into(),
                    ));
                }
            }
        }
        for (id, dose) in &self.containers {
            if *id == 0
                || *id != dose.container_id
                || !valid_content_id(&dose.preparation_id)
                || !valid_content_id(&dose.item_name)
                || dose.definition_version == 0
                || dose.current_units > MAX_PREPARATION_CHARGE
                || dose.dross_units > MAX_PREPARATION_CHARGE
                || dose
                    .current_units
                    .checked_add(dose.dross_units)
                    .is_none_or(|units| units == 0 || units > MAX_PREPARATION_CHARGE)
                || dose
                    .vessel_materials
                    .values()
                    .chain(dose.materials.values())
                    .any(|units| *units > u64::from(u32::MAX))
                || !valid_material_vector(&dose.vessel_materials)
                || !valid_material_vector(&dose.materials)
                || dose.expires_tick <= dose.born_tick
                || (dose.last_storage_tick != 0 && dose.last_storage_tick < dose.born_tick)
                || dose.source_batch == 0
                || dose.source_installation == 0
            {
                return Err(AlchemyError::Corrupt(
                    "filled preparation container has invalid stable state".into(),
                ));
            }
            dose.liquid.validate()?;
            maximum_installation_id = maximum_installation_id.max(dose.source_installation);
            maximum_batch_id = maximum_batch_id.max(dose.source_batch);
        }
        let mut status_ids = BTreeSet::new();
        for (actor, statuses) in &self.statuses {
            if *actor == [0; 16] || statuses.len() > PreparationHandler::ALL.len() * 2 {
                return Err(AlchemyError::Corrupt(
                    "player preparation status census is invalid".into(),
                ));
            }
            let mut groups = BTreeSet::new();
            for status in statuses {
                if status.status_id == 0
                    || status.status_id >= STATUS_OWNER_BIT
                    || !status_ids.insert(status.status_id)
                    || status.actor != *actor
                    || status.source_batch == 0
                    || !valid_content_id(&status.preparation_id)
                    || !valid_content_id(&status.stack_group)
                    || !groups.insert(status.stack_group.clone())
                    || status.due_tick <= status.started_tick
                    || status.last_tick < status.started_tick
                    || status.last_tick > status.recovery_until_tick
                    || status.recovery_until_tick < status.due_tick
                    || status.dose_volume_units == 0
                    || status.dose_volume_units > MAX_BATCH_VOLUME_UNITS
                    || status.active_current.total() > MAX_PREPARATION_CHARGE
                    || status.dross_current.total() > MAX_PREPARATION_CHARGE
                    || !valid_current(&status.active_current)
                    || !valid_current(&status.dross_current)
                    || status.refresh_count > 3
                    || status.overdose_until_tick > status.recovery_until_tick
                {
                    return Err(AlchemyError::Corrupt(
                        "active preparation status is invalid or illegally stacked".into(),
                    ));
                }
                maximum_status_id = maximum_status_id.max(status.status_id);
                maximum_batch_id = maximum_batch_id.max(status.source_batch);
            }
        }
        if self.root_treatments.iter().any(|(_, treatment)| {
            treatment.source_batch == 0
                || treatment.actor == [0; 16]
                || treatment.expires_tick <= treatment.applied_tick
                || treatment.water_hu > MAX_BATCH_VOLUME_UNITS
                || treatment.nutrient_units > u64::from(u16::MAX)
                || treatment.uptake_permille > 1_000
                || treatment.concentration_permille == 0
                || treatment.concentration_permille > 2_000
        }) {
            return Err(AlchemyError::Corrupt(
                "alchemy treatment or specimen-coating state is invalid".into(),
            ));
        }
        for treatment in self.root_treatments.values() {
            maximum_batch_id = maximum_batch_id.max(treatment.source_batch);
        }
        for (id, coating) in &self.coatings {
            if *id == 0
                || *id != coating.item_id
                || coating.status_id == 0
                || coating.status_id >= STATUS_OWNER_BIT
                || !status_ids.insert(coating.status_id)
                || coating.source_batch == 0
                || coating.actor == [0; 16]
                || coating.expires_tick <= coating.applied_tick
                || coating.preservation_permille >= 1_000
            {
                return Err(AlchemyError::Corrupt(
                    "alchemy treatment or specimen-coating state is invalid".into(),
                ));
            }
            maximum_status_id = maximum_status_id.max(coating.status_id);
            maximum_batch_id = maximum_batch_id.max(coating.source_batch);
        }
        for pollution in self.pollution.values() {
            pollution
                .water
                .checked_add(ReservoirMass::default())
                .ok_or(AlchemyError::Overflow)?;
            pollution
                .dross
                .total_checked()
                .map_err(|error| AlchemyError::Corrupt(error.to_string()))?;
            if pollution.last_actor == [0; 16]
                || pollution.last_operation_id == 0
                || !valid_material_vector(&pollution.materials)
                || pollution.solutes.len() > MAX_ALCHEMY_VECTOR_ENTRIES
                || pollution.solutes.iter().any(|(name, units)| {
                    !valid_content_id(name) || *units == 0 || *units > u64::from(u32::MAX)
                })
                || !valid_current(&pollution.dross)
            {
                return Err(AlchemyError::Corrupt(
                    "alchemy pollution custody is invalid".into(),
                ));
            }
            maximum_operation_id = maximum_operation_id.max(pollution.last_operation_id);
        }
        if self.ordinary_jobs.iter().any(|(pos, job)| {
            job.installation_id == 0
                || job.actor == [0; 16]
                || job.due_tick <= job.started_tick
                || job.output_count == 0
                || job.output_count > MAX_BATCH_DOSES
                || !valid_content_id(&job.output_item)
                || !valid_material_vector(&job.input_materials)
                || self
                    .apparatus
                    .get(pos)
                    .is_none_or(|apparatus| apparatus.installation_id != job.installation_id)
        }) {
            return Err(AlchemyError::Corrupt(
                "ordinary alchemy carrier job is invalid or detached".into(),
            ));
        }
        for job in self.ordinary_jobs.values() {
            maximum_installation_id = maximum_installation_id.max(job.installation_id);
        }
        if self.history.iter().any(|event| {
            event.operation_id == 0
                || event.installation_id == 0
                || !bounded_text(&event.action, MAX_PREPARATION_ID_BYTES)
                || !valid_content_id(&event.preparation_id)
                || !bounded_text(&event.note, MAX_PREPARATION_TEXT_BYTES)
        }) {
            return Err(AlchemyError::Corrupt(
                "alchemy audit history contains invalid attribution".into(),
            ));
        }
        for event in &self.history {
            maximum_installation_id = maximum_installation_id.max(event.installation_id);
            maximum_batch_id = maximum_batch_id.max(event.batch_id);
            maximum_operation_id = maximum_operation_id.max(event.operation_id);
        }
        if self.next_installation_id <= maximum_installation_id
            || self.next_batch_id <= maximum_batch_id
            || self.next_status_id <= maximum_status_id
            || self.next_operation_id <= maximum_operation_id
        {
            return Err(AlchemyError::Corrupt(
                "alchemy stable-id allocator would collide with surviving state".into(),
            ));
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, AlchemyError> {
        self.validate()?;
        let bytes = postcard::to_allocvec(self)
            .map_err(|error| AlchemyError::Corrupt(error.to_string()))?;
        if bytes.len() as u64 > MAX_ALCHEMY_FILE_BYTES {
            return Err(AlchemyError::InvalidOperation(
                "alchemy state exceeds its bounded save budget".into(),
            ));
        }
        Ok(bytes)
    }

    pub fn save(&self) -> Result<(), AlchemyError> {
        let encoded = self.encode()?;
        if let Ok(previous) = std::fs::read(&self.path) {
            crate::identity::atomic_write(
                &self.path.with_file_name(ALCHEMY_BACKUP),
                &previous,
                false,
            )?;
        }
        crate::identity::atomic_write(&self.path, &encoded, false)?;
        Ok(())
    }

    pub fn allocate_installation_id(&mut self) -> Result<u64, AlchemyError> {
        allocate(&mut self.next_installation_id)
    }

    pub fn allocate_batch_id(&mut self) -> Result<u64, AlchemyError> {
        allocate(&mut self.next_batch_id)
    }

    pub fn allocate_status_id(&mut self) -> Result<u64, AlchemyError> {
        allocate(&mut self.next_status_id)
    }

    pub fn allocate_operation_id(&mut self) -> Result<u64, AlchemyError> {
        allocate(&mut self.next_operation_id)
    }

    pub fn record(&mut self, event: AlchemyAuditEvent) {
        self.history.push_back(event);
        while self.history.len() > MAX_ALCHEMY_HISTORY {
            self.history.pop_front();
        }
    }

    pub fn total_water_custody(&self) -> ReservoirMass {
        let mut total = ReservoirMass::default();
        for liquid in self
            .apparatus
            .values()
            .filter_map(|apparatus| apparatus.batch.as_ref().map(|batch| &batch.liquid))
            .chain(self.containers.values().map(|dose| &dose.liquid))
        {
            total.water_hu = total.water_hu.saturating_add(liquid.water.water_hu);
            total.salt_mass = total.salt_mass.saturating_add(liquid.water.salt_mass);
        }
        for batch in self
            .apparatus
            .values()
            .filter_map(|apparatus| apparatus.batch.as_ref())
        {
            total.water_hu = total.water_hu.saturating_add(batch.residue_water.water_hu);
            total.salt_mass = total
                .salt_mass
                .saturating_add(batch.residue_water.salt_mass);
        }
        for job in self.ordinary_jobs.values() {
            total.water_hu = total.water_hu.saturating_add(job.process_water.water_hu);
            total.salt_mass = total.salt_mass.saturating_add(job.process_water.salt_mass);
        }
        total
    }

    pub fn active_arcane_ids(&self) -> BTreeSet<u64> {
        self.apparatus
            .values()
            .filter_map(|apparatus| {
                apparatus
                    .batch
                    .as_ref()
                    .map(|batch| batch_owner_id(batch.id))
            })
            .chain(
                self.statuses
                    .values()
                    .flatten()
                    .map(|status| status_owner_id(status.status_id)),
            )
            .chain(
                self.coatings
                    .values()
                    .map(|coating| status_owner_id(coating.status_id)),
            )
            .chain(self.apparatus.values().filter_map(|apparatus| {
                (apparatus.filter_owner_id != 0).then_some(apparatus.filter_owner_id)
            }))
            .collect()
    }
}

pub fn audit_world(world: &Path) -> Result<AlchemyAudit, AlchemyError> {
    let state = AlchemyState::load_existing(world)?
        .ok_or_else(|| AlchemyError::Corrupt("world has no alchemy sidecar".into()))?;
    state.validate()?;
    let file_bytes = std::fs::metadata(world.join(ALCHEMY_FILE))?.len();
    let ledger = crate::arcane::ArcaneLedger::load(world)
        .map_err(|error| AlchemyError::Corrupt(error.to_string()))?;
    #[cfg(not(test))]
    let arcane_balanced = crate::arcane::audit_world(world)
        .map_err(|error| AlchemyError::Corrupt(error.to_string()))?
        .is_balanced();
    #[cfg(test)]
    let arcane_balanced = ledger
        .audit()
        .map_err(|error| AlchemyError::Corrupt(error.to_string()))?
        .unexplained_delta
        == 0;
    let material_audit = crate::materials::audit_world(world)?;
    #[cfg(not(test))]
    let atlas = crate::planet_atlas::PlanetAtlas::load(world)
        .map_err(|error| AlchemyError::Corrupt(error.to_string()))?;
    #[cfg(test)]
    let atlas = crate::planet_atlas::PlanetAtlas::load_fixture(world)
        .map_err(|error| AlchemyError::Corrupt(error.to_string()))?;
    let water_audit = atlas.water_audit();

    let mut clean_current = 0u64;
    let mut dross_current = 0u64;
    let mut missing_arcane_accounts = 0usize;
    let mut mismatched_arcane_accounts = 0usize;
    let mut inspect_total = |owner: crate::arcane::ArcaneOwner, expected: u64, dross: bool| {
        let actual = ledger
            .account(&owner)
            .map(|account| account.current.total());
        if expected == 0 {
            mismatched_arcane_accounts += usize::from(actual.is_some_and(|units| units != 0));
        } else if let Some(actual) = actual {
            mismatched_arcane_accounts += usize::from(actual != expected);
            if dross {
                dross_current = dross_current.saturating_add(actual);
            } else {
                clean_current = clean_current.saturating_add(actual);
            }
        } else {
            missing_arcane_accounts += 1;
        }
    };

    for apparatus in state.apparatus.values() {
        if let Some(batch) = &apparatus.batch {
            let owner_id = batch_owner_id(batch.id);
            inspect_total(
                crate::arcane::ArcaneOwner::Alchemy(owner_id),
                batch.current_units,
                false,
            );
            inspect_total(
                crate::arcane::ArcaneOwner::AlchemyDross(owner_id),
                batch.dross_units,
                true,
            );
        }
        if apparatus.filter_owner_id != 0 {
            inspect_total(
                crate::arcane::ArcaneOwner::AlchemyDross(apparatus.filter_owner_id),
                apparatus.filter_burden,
                true,
            );
        }
    }
    for dose in state.containers.values() {
        inspect_total(
            crate::arcane::ArcaneOwner::Item(dose.container_id),
            dose.current_units,
            false,
        );
        inspect_total(
            crate::arcane::ArcaneOwner::ItemDross(dose.container_id),
            dose.dross_units,
            true,
        );
    }
    for status in state.statuses.values().flatten() {
        let owner_id = status_owner_id(status.status_id);
        let clean = ledger.account(&crate::arcane::ArcaneOwner::Alchemy(owner_id));
        let dross = ledger.account(&crate::arcane::ArcaneOwner::AlchemyDross(owner_id));
        missing_arcane_accounts +=
            usize::from(!status.active_current.is_empty() && clean.is_none());
        missing_arcane_accounts += usize::from(!status.dross_current.is_empty() && dross.is_none());
        mismatched_arcane_accounts += usize::from(
            clean.map(|account| &account.current) != Some(&status.active_current)
                && (!status.active_current.is_empty() || clean.is_some()),
        );
        mismatched_arcane_accounts += usize::from(
            dross.map(|account| &account.current) != Some(&status.dross_current)
                && (!status.dross_current.is_empty() || dross.is_some()),
        );
        clean_current =
            clean_current.saturating_add(clean.map_or(0, |account| account.current.total()));
        dross_current =
            dross_current.saturating_add(dross.map_or(0, |account| account.current.total()));
    }
    // Coatings predate exact mixtures in their presentation record.  The
    // ledger remains the sole source of truth, but both owner classes must be
    // bounded and at least one must exist for every live coating.
    for coating in state.coatings.values() {
        let owner_id = status_owner_id(coating.status_id);
        let clean = ledger.account(&crate::arcane::ArcaneOwner::Alchemy(owner_id));
        let dross = ledger.account(&crate::arcane::ArcaneOwner::AlchemyDross(owner_id));
        missing_arcane_accounts += usize::from(clean.is_none() && dross.is_none());
        mismatched_arcane_accounts += usize::from(
            clean
                .into_iter()
                .chain(dross)
                .any(|account| account.current.total() > MAX_PREPARATION_CHARGE),
        );
        clean_current =
            clean_current.saturating_add(clean.map_or(0, |account| account.current.total()));
        dross_current =
            dross_current.saturating_add(dross.map_or(0, |account| account.current.total()));
    }
    dross_current = dross_current.saturating_add(
        state
            .pollution
            .values()
            .map(|pollution| pollution.dross.total())
            .sum::<u64>(),
    );

    let tracked_material_units = state
        .apparatus
        .values()
        .flat_map(|apparatus| {
            apparatus
                .residue_materials
                .values()
                .chain(apparatus.filter_medium_materials.values())
                .chain(
                    apparatus
                        .batch
                        .iter()
                        .flat_map(|batch| batch.ingredients.iter())
                        .flat_map(|ingredient| {
                            ingredient
                                .retained_materials
                                .values()
                                .chain(ingredient.residue_materials.values())
                        }),
                )
        })
        .chain(state.containers.values().flat_map(|dose| {
            dose.vessel_materials
                .values()
                .chain(dose.materials.values())
        }))
        .chain(
            state
                .pollution
                .values()
                .flat_map(|pollution| pollution.materials.values()),
        )
        .chain(
            state
                .ordinary_jobs
                .values()
                .flat_map(|job| job.input_materials.values()),
        )
        .copied()
        .fold(0u64, u64::saturating_add);
    let water = state.total_water_custody();
    let statuses = state.statuses.values().map(Vec::len).sum();
    let batches = state
        .apparatus
        .values()
        .filter(|apparatus| apparatus.batch.is_some())
        .count();

    Ok(AlchemyAudit {
        schema_version: state.schema_version,
        content_hash: state.content_hash,
        file_bytes,
        apparatus: state.apparatus.len(),
        batches,
        containers: state.containers.len(),
        statuses,
        root_treatments: state.root_treatments.len(),
        coatings: state.coatings.len(),
        pollution_sites: state.pollution.len(),
        ordinary_jobs: state.ordinary_jobs.len(),
        history: state.history.len(),
        water_hu: water.water_hu,
        salt_mass: water.salt_mass,
        clean_current,
        dross_current,
        tracked_material_units,
        missing_arcane_accounts,
        mismatched_arcane_accounts,
        water_custody_overdrawn: water.water_hu > water_audit.industrial.water_hu
            || water.salt_mass > water_audit.industrial.salt_mass,
        arcane_balanced,
        water_balanced: water_audit.unexplained_water_delta_hu == 0
            && water_audit.unexplained_salt_delta == 0,
        material_balanced: material_audit.is_balanced(),
    })
}

pub const fn batch_owner_id(batch_id: u64) -> u64 {
    batch_id
}

pub const fn status_owner_id(status_id: u64) -> u64 {
    STATUS_OWNER_BIT | status_id
}

fn validate_batch(batch: &AlchemyBatch, installation_id: u64) -> Result<(), AlchemyError> {
    if batch.id == 0
        || batch.id >= STATUS_OWNER_BIT
        || batch.installation_id != installation_id
        || !valid_content_id(&batch.preparation_id)
        || batch.definition_version == 0
        || !bounded_text(&batch.actor_label, MAX_PREPARATION_ID_BYTES)
        || batch.ingredients.len() > MAX_PREPARATION_INGREDIENTS
        || (!matches!(batch.outcome, BatchOutcome::Processing) && batch.ingredients.is_empty())
        || batch.current_units > MAX_PREPARATION_CHARGE
        || batch.charge_input_units > MAX_PREPARATION_CHARGE
        || batch.current_units > batch.charge_input_units
        || batch.dross_units > MAX_PREPARATION_CHARGE
        || batch.observations.len() > MAX_PROCESS_STEPS.saturating_mul(4)
        || batch
            .observations
            .windows(2)
            .any(|window| window[0].tick > window[1].tick)
        || batch.due_tick <= batch.started_tick
        || batch.expires_tick <= batch.born_tick
        || batch.revision == 0
    {
        return Err(AlchemyError::Corrupt(
            "alchemy batch identity, bounds, or schedule is invalid".into(),
        ));
    }
    batch.liquid.validate()?;
    batch
        .residue_water
        .checked_add(ReservoirMass::default())
        .ok_or(AlchemyError::Overflow)?;
    if batch.ingredients.iter().any(|ingredient| {
        !valid_content_id(&ingredient.item)
            || ingredient.count == 0
            || ingredient.condition_permille == 0
            || ingredient.condition_permille > 1_000
            || ingredient.source_arcane_ids.len() > usize::from(ingredient.count)
            || ingredient.source_arcane_ids.contains(&0)
            || !valid_material_vector(&ingredient.retained_materials)
            || !valid_material_vector(&ingredient.residue_materials)
    }) {
        return Err(AlchemyError::Corrupt(
            "alchemy batch ingredient custody is invalid".into(),
        ));
    }
    Ok(())
}

fn valid_material_vector(vector: &MaterialVector) -> bool {
    vector.len() <= MAX_ALCHEMY_VECTOR_ENTRIES
        && vector.iter().all(|(name, units)| {
            bounded_text(name, MAX_PREPARATION_ID_BYTES)
                && *units != 0
                && *units <= u64::from(u32::MAX)
        })
}

fn valid_current(current: &Current) -> bool {
    current.parts().len() <= MAX_ALCHEMY_VECTOR_ENTRIES
        && current
            .parts()
            .iter()
            .all(|(name, units)| valid_content_id(name) && *units != 0)
        && current.total_checked().is_ok()
}

fn allocate(next: &mut u64) -> Result<u64, AlchemyError> {
    let id = *next;
    *next = next.checked_add(1).ok_or(AlchemyError::Overflow)?;
    if id == 0 {
        return Err(AlchemyError::Overflow);
    }
    Ok(id)
}

fn add_material_vector(
    into: &mut BTreeMap<String, u64>,
    other: &BTreeMap<String, u64>,
) -> Result<(), AlchemyError> {
    for (name, units) in other {
        let next = into
            .get(name)
            .copied()
            .unwrap_or_default()
            .checked_add(*units)
            .ok_or(AlchemyError::Overflow)?;
        if next == 0 {
            into.remove(name);
        } else {
            into.insert(name.clone(), next);
        }
    }
    Ok(())
}

fn qualify(provider: &str, id: &str) -> String {
    if id.contains(':') {
        id.into()
    } else {
        format!("{provider}:{id}")
    }
}

fn valid_content_id(value: &str) -> bool {
    bounded_text(value, MAX_PREPARATION_ID_BYTES)
        && value.contains(':')
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b":_-/".contains(&byte)
        })
}

fn bounded_text(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}

#[derive(Debug)]
pub enum AlchemyError {
    Io(std::io::Error),
    InvalidContent(String),
    InvalidOperation(String),
    Corrupt(String),
    Overflow,
}

impl std::fmt::Display for AlchemyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::InvalidContent(message) => {
                write!(formatter, "invalid alchemy content: {message}")
            }
            Self::InvalidOperation(message) => {
                write!(formatter, "invalid alchemy operation: {message}")
            }
            Self::Corrupt(message) => write!(formatter, "corrupt alchemy state: {message}"),
            Self::Overflow => write!(formatter, "alchemy arithmetic overflow"),
        }
    }
}

impl std::error::Error for AlchemyError {}

impl From<std::io::Error> for AlchemyError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(handler: PreparationHandler) -> RawPreparationDef {
        let process = handler.allowed_processes()[0];
        let mut effect = RawPreparationEffect {
            duration_ticks: 200,
            recovery_ticks: 100,
            strength: 10,
            hunger_cost_milli: 0,
            nutrient_cost: 0,
            water_hu: 0,
            dross_capacity: 0,
            throughput_permille: 0,
            drain_permille: 0,
            overdraw_permille: 0,
            preservation_permille: 0,
        };
        match handler {
            PreparationHandler::NaturalRecovery => {
                effect.hunger_cost_milli = 500;
                effect.nutrient_cost = 1;
            }
            PreparationHandler::RootUptake => {
                effect.water_hu = 32;
                effect.nutrient_cost = 1;
            }
            PreparationHandler::StrainRelief => effect.throughput_permille = 750,
            PreparationHandler::DrossWash | PreparationHandler::DrossAntidote => {
                effect.dross_capacity = 16;
            }
            PreparationHandler::PreserveSpecimen => effect.preservation_permille = 500,
            PreparationHandler::ThroughputSurge => {
                effect.throughput_permille = 1_200;
                effect.drain_permille = 1_300;
                effect.overdraw_permille = 1_400;
            }
            PreparationHandler::TraceSight => {}
        }
        let mut steps = vec![ProcessStep::Load, process.required_step()];
        steps.dedup();
        RawPreparationDef {
            id: "fixture".into(),
            label: "Fixture Preparation".into(),
            version: 1,
            process,
            handler,
            application: handler.application(),
            carrier: CarrierKind::FreshWater,
            solvent_item: "base:bucket_water".into(),
            solvent_units: 256,
            dissolved_units: 0,
            ingredients: vec![RawPreparationIngredient {
                item: if handler == PreparationHandler::DrossAntidote {
                    "base:ashlace_tissue".into()
                } else {
                    "base:rainbell_dew".into()
                },
                count: 1,
                retention_permille: 800,
            }],
            charge_units: 16,
            resonance: "base:echo".into(),
            charge_rate: [1, 4],
            dross_units: 2,
            steps,
            temperature_millic: [10_000, 80_000],
            process_ticks: 200,
            agitation: AgitationKind::Still,
            cleanliness_min: 700,
            output_item: "base:fixture_dose".into(),
            empty_vessel: "base:glass_bottle".into(),
            doses: 4,
            dose_units: 64,
            residue_item: "base:spent_mash".into(),
            residue_count: 1,
            shelf_life_ticks: 20_000,
            storage_temperature_millic: [-10_000, 30_000],
            stack_group: "fixture".into(),
            effect,
            description: "A bounded fixture preparation with physical inputs and residue.".into(),
        }
    }

    #[test]
    fn every_native_effect_has_a_closed_valid_contract() {
        for handler in PreparationHandler::ALL {
            let definition = PreparationDef::from_raw("fixture", raw(handler)).unwrap();
            assert_eq!(definition.handler, handler);
            assert_eq!(definition.application, handler.application());
        }
    }

    #[test]
    fn free_volume_healing_hidden_state_and_raw_mutation_shapes_fail() {
        let mut free_volume = raw(PreparationHandler::TraceSight);
        free_volume.doses = 5;
        assert!(PreparationDef::from_raw("fixture", free_volume).is_err());

        let mut free_healing = raw(PreparationHandler::NaturalRecovery);
        free_healing.effect.hunger_cost_milli = 0;
        assert!(PreparationDef::from_raw("fixture", free_healing).is_err());

        let hidden_state = toml::from_str::<PreparationsFile>(
            "schema_version = 1\n[[preparation]]\nid = \"spy\"\nraw_mutation = \"reveal_inventory\"\n",
        );
        assert!(hidden_state.is_err());

        let mut oversized_solution = raw(PreparationHandler::TraceSight);
        oversized_solution.solvent_units = MAX_BATCH_VOLUME_UNITS;
        oversized_solution.dissolved_units = 1;
        assert!(PreparationDef::from_raw("fixture", oversized_solution).is_err());

        let registry =
            crate::registry::load(std::path::Path::new("/nonexistent-alchemy-validation-mods"));
        let mut no_state_shell = raw(PreparationHandler::TraceSight);
        no_state_shell.output_item = "base:potato".into();
        let no_state_shell = PreparationDef::from_raw("fixture", no_state_shell).unwrap();
        assert!(no_state_shell.validate_registry(&registry).is_err());
    }

    #[test]
    fn exact_liquid_repeated_splits_keep_every_remainder() {
        let original = ExactLiquid {
            carrier: Some(CarrierKind::Brine),
            volume_units: 257,
            water: ReservoirMass {
                water_hu: 257,
                salt_mass: 65_537,
            },
            carrier_state: WaterCarrier {
                thermal_millic_hu: 9_999_991,
                dross_subunits: 65_539,
            },
            solutes: BTreeMap::from([
                ("base:root_extract".into(), 1_003),
                ("base:ashlace_burden".into(), 257),
            ]),
        };
        original.validate().unwrap();
        let mut remaining = original.clone();
        let mut recombined = ExactLiquid::default();
        for volume in [64, 64, 64, 64, 1] {
            let portion = remaining.take(volume).unwrap();
            recombined.checked_add(portion).unwrap();
        }
        assert_eq!(remaining, ExactLiquid::default());
        assert_eq!(recombined, original);

        let displaced_solution = ExactLiquid {
            carrier: Some(CarrierKind::FreshWater),
            volume_units: 257,
            water: ReservoirMass::fresh(256),
            carrier_state: WaterCarrier {
                thermal_millic_hu: 5_140_000,
                dross_subunits: 17,
            },
            solutes: BTreeMap::from([("base:rainbell_dew".into(), 1)]),
        };
        displaced_solution.validate().unwrap();
        let mut remaining = displaced_solution.clone();
        let mut recombined = ExactLiquid::default();
        for volume in [64, 64, 64, 64, 1] {
            recombined
                .checked_add(remaining.take(volume).unwrap())
                .unwrap();
        }
        assert_eq!(remaining, ExactLiquid::default());
        assert_eq!(recombined, displaced_solution);
    }
}
