//! Authoritative magical working definitions and durable transaction state.
//!
//! A working may select one of the native handlers in [`WorkingHandler`], but
//! content never receives a generic block, item, entity, or script mutation
//! capability.  Every durable effect is consequently represented by the
//! domain-specific [`WorkingEffect`] enum and can be audited before it is
//! committed by the world subsystem.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::arcane::{ArcaneOwner, Current};
use crate::planet::BlockPos;

pub const WORKINGS_SCHEMA_VERSION: u32 = 1;
pub const WORKING_DEFINITION_VERSION: u32 = 1;
pub const WORKINGS_FILE: &str = "workings.wfw";
pub const WATER_CARRIERS_FILE: &str = "water-carriers.wfw";
const WORKINGS_BACKUP: &str = "workings.wfw.bak";
pub const MAX_ACTIVE_WORKINGS: usize = 4_096;
pub const MAX_ACTIVE_RITUALS: usize = 1_024;
pub const MAX_WORKING_DEFINITIONS: usize = 65_536;
pub const MAX_WORKING_HISTORY: usize = 4_096;
pub const MAX_WORKINGS_FILE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_WORKING_ID_BYTES: usize = 96;
pub const MAX_WORKING_TEXT_BYTES: usize = 512;
pub const MAX_WORKING_TARGETS: usize = 256;
pub const MAX_WORKING_DEBITS: usize = 256;
pub const MAX_WARD_SEGMENTS: usize = 512;
pub const MAX_WORKING_RANGE: u16 = 64;
pub const MAX_WORKING_DURATION_TICKS: u64 = 20 * 60 * 60;
pub const MAX_WORKING_CHARGE: u64 = 65_536;
pub const MAX_WORKING_MAGNITUDE: u32 = 4_096;
pub const MAX_WATER_CARRIER_CELLS: usize = 1_048_576;
pub const MIN_WAND_SETTLE_TICKS: u64 = 7;
pub const MIN_WAND_SETTLE_SECONDS: f32 = MIN_WAND_SETTLE_TICKS as f32 / 20.0;
pub const WAND_RECOVERY_SECONDS: f32 = 0.2;

/// Exact non-hydrological carriers attached to a detailed water voxel. Heat
/// is stored as milli-Celsius times HU so mixing is additive. Dross uses 256
/// fixed subunits per Current unit; fractional transfers therefore remain in
/// the source/destination rather than disappearing into integer division.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WaterCarrier {
    pub thermal_millic_hu: i64,
    pub dross_subunits: u64,
}

impl WaterCarrier {
    pub fn temperature_millic(self, water_hu: u64) -> i32 {
        if water_hu == 0 {
            return 0;
        }
        (self.thermal_millic_hu / i64::try_from(water_hu).unwrap_or(i64::MAX))
            .clamp(-100_000, 100_000) as i32
    }

    pub fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            thermal_millic_hu: self
                .thermal_millic_hu
                .checked_add(other.thermal_millic_hu)?,
            dross_subunits: self.dross_subunits.checked_add(other.dross_subunits)?,
        })
    }

    pub fn take(&mut self, water_before_hu: u64, water_hu: u64) -> Option<Self> {
        if water_hu > water_before_hu || water_before_hu == 0 {
            return None;
        }
        if water_hu == water_before_hu {
            return Some(std::mem::take(self));
        }
        let thermal = i128::from(self.thermal_millic_hu).checked_mul(i128::from(water_hu))?
            / i128::from(water_before_hu);
        let dross = u128::from(self.dross_subunits).checked_mul(u128::from(water_hu))?
            / u128::from(water_before_hu);
        let parcel = Self {
            thermal_millic_hu: i64::try_from(thermal).ok()?,
            dross_subunits: u64::try_from(dross).ok()?,
        };
        self.thermal_millic_hu = self
            .thermal_millic_hu
            .checked_sub(parcel.thermal_millic_hu)?;
        self.dross_subunits = self.dross_subunits.checked_sub(parcel.dross_subunits)?;
        Some(parcel)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WaterCarrierState {
    pub schema_version: u32,
    pub cells: BTreeMap<BlockPos, WaterCarrier>,
    #[serde(skip)]
    path: PathBuf,
}

impl WaterCarrierState {
    pub fn load_or_initialize(world: &Path) -> Result<Self, WorkingError> {
        let path = world.join(WATER_CARRIERS_FILE);
        if !path.exists() {
            return Ok(Self {
                schema_version: 1,
                cells: BTreeMap::new(),
                path,
            });
        }
        let bytes = std::fs::read(&path)?;
        if bytes.len() > MAX_WORKINGS_FILE_BYTES as usize {
            return Err(WorkingError::Corrupt(
                "water carrier sidecar exceeds its bounded size".into(),
            ));
        }
        let mut state: Self = postcard::from_bytes(&bytes)
            .map_err(|error| WorkingError::Corrupt(error.to_string()))?;
        state.path = path;
        state.validate()?;
        Ok(state)
    }

    pub fn validate(&self) -> Result<(), WorkingError> {
        if self.schema_version != 1
            || self.cells.len() > MAX_WATER_CARRIER_CELLS
            || self.cells.values().any(|carrier| {
                carrier.thermal_millic_hu.unsigned_abs() > 100_000 * 256
                    || carrier.dross_subunits > u64::from(u32::MAX) * 256
            })
        {
            return Err(WorkingError::Corrupt(
                "water carrier state is unbounded or has an unsupported schema".into(),
            ));
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, WorkingError> {
        self.validate()?;
        postcard::to_allocvec(self).map_err(|error| WorkingError::Corrupt(error.to_string()))
    }

    pub fn save(&self) -> Result<(), WorkingError> {
        crate::identity::atomic_write(&self.path, &self.encode()?, false)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryMode {
    Wand,
    Ritual,
    Passive,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkingHandler {
    Trace,
    Gleam,
    Ignite,
    Nudge,
    Rootwake,
    Draw,
    Fieldmend,
    Holdfast,
    SettlingRite,
    RootingBed,
    WardBoundary,
    TransferCircle,
}

impl WorkingHandler {
    #[cfg_attr(not(test), allow(dead_code))]
    pub const ALL: [Self; 12] = [
        Self::Trace,
        Self::Gleam,
        Self::Ignite,
        Self::Nudge,
        Self::Rootwake,
        Self::Draw,
        Self::Fieldmend,
        Self::Holdfast,
        Self::SettlingRite,
        Self::RootingBed,
        Self::WardBoundary,
        Self::TransferCircle,
    ];

    pub const fn mode(self) -> DeliveryMode {
        match self {
            Self::Trace
            | Self::Gleam
            | Self::Ignite
            | Self::Nudge
            | Self::Rootwake
            | Self::Draw
            | Self::Fieldmend
            | Self::Holdfast => DeliveryMode::Wand,
            Self::SettlingRite | Self::RootingBed | Self::WardBoundary | Self::TransferCircle => {
                DeliveryMode::Ritual
            }
        }
    }

    pub const fn targets(self) -> &'static [&'static str] {
        match self {
            Self::Trace => &["visible_arcane", "self"],
            Self::Gleam => &["wand_tip", "visible_focus"],
            Self::Ignite => &["combustible"],
            Self::Nudge => &["dropped_item", "projectile", "mechanism"],
            Self::Rootwake => &["plant", "crop", "sapling"],
            Self::Draw => &["water_source", "water_destination"],
            Self::Fieldmend => &["damaged_portable", "matching_repair_material"],
            Self::Holdfast => &["fragile_carried", "fragile_mounted"],
            Self::SettlingRite => &["adjacent_magical_process", "dross_vessel"],
            Self::RootingBed => &["prepared_bed", "plant", "crop", "sapling"],
            Self::WardBoundary => &["closed_ward_boundary", "supernatural_pressure"],
            Self::TransferCircle => &["mounted_charge_source", "mounted_charge_destination"],
        }
    }

    pub const fn physical_requirements(self) -> &'static [&'static str] {
        match self {
            Self::Trace | Self::Gleam | Self::Nudge => &[],
            Self::Ignite => &["ordinary_fuel", "ordinary_ignition_rules"],
            Self::Rootwake => &["soil", "water", "nutrients", "habitat"],
            Self::Draw => &["water", "salt", "temperature", "dross_carrier"],
            Self::Fieldmend => &["matching_repair_material", "ordinary_residue"],
            Self::Holdfast => &["elapsed_age"],
            Self::SettlingRite => &["frame", "conductor", "containment", "dross_vessel"],
            Self::RootingBed => &["prepared_soil", "water", "nutrients", "focus_posts"],
            Self::WardBoundary => &["closed_boundary", "intact_segments", "charge_source"],
            Self::TransferCircle => &["adjacent_mounts", "conductor"],
        }
    }

    pub const fn effect_kind(self) -> WorkingEffectKind {
        match self {
            Self::Trace => WorkingEffectKind::Observe,
            Self::Gleam => WorkingEffectKind::PointLight,
            Self::Ignite => WorkingEffectKind::Ignite,
            Self::Nudge => WorkingEffectKind::Impulse,
            Self::Rootwake => WorkingEffectKind::AdvancePlant,
            Self::Draw => WorkingEffectKind::TransferWater,
            Self::Fieldmend => WorkingEffectKind::RepairItem,
            Self::Holdfast => WorkingEffectKind::Preserve,
            Self::SettlingRite => WorkingEffectKind::Settle,
            Self::RootingBed => WorkingEffectKind::AdvanceBed,
            Self::WardBoundary => WorkingEffectKind::Ward,
            Self::TransferCircle => WorkingEffectKind::TransferCurrent,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InterruptionPolicy {
    /// No physical effect landed: return usable Current and disorder the
    /// declared remainder into dross.
    RefundWithDross,
    /// A release is the normal commit edge; interruption before it refunds.
    ReleaseCommit,
    /// The host-owned process remains scheduled while its chunks are unloaded.
    ContinueUnloaded,
    /// A continuously supplied effect ends and settles at the interruption.
    EndContinuous,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CurrentDisposition {
    Return,
    Dross,
    Split,
}

fn default_definition_version() -> u32 {
    WORKING_DEFINITION_VERSION
}

fn default_max_targets() -> u16 {
    1
}

fn default_max_magnitude() -> u32 {
    1
}

fn default_safe_throughput() -> u64 {
    1
}

fn default_doc() -> String {
    "No browser documentation supplied.".into()
}

/// Strict TOML shell accepted from base content and mods. Unknown fields are
/// rejected so a misspelled physical debit cannot quietly become optional.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawWorkingDef {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default = "default_definition_version")]
    pub version: u32,
    pub handler: WorkingHandler,
    pub mode: DeliveryMode,
    pub focus: String,
    pub charge: u64,
    #[serde(default)]
    pub charge_per_magnitude: u64,
    #[serde(default)]
    pub charge_per_block: u64,
    #[serde(default)]
    pub charge_per_second: u64,
    pub dross: u64,
    #[serde(default)]
    pub dross_per_magnitude: u64,
    #[serde(default = "default_safe_throughput")]
    pub safe_throughput: u64,
    pub range: u16,
    #[serde(default = "default_max_magnitude")]
    pub max_magnitude: u32,
    #[serde(default)]
    pub max_volume: u32,
    #[serde(default = "default_max_targets")]
    pub max_targets: u16,
    #[serde(default)]
    pub max_duration_ticks: u64,
    pub target: Vec<String>,
    #[serde(default)]
    pub physical: Vec<String>,
    pub interruption: InterruptionPolicy,
    pub disposition: CurrentDisposition,
    #[serde(default)]
    pub wear: u16,
    #[serde(default)]
    pub ambient: bool,
    #[serde(default = "default_doc")]
    pub description: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkingsFile {
    pub schema_version: Option<u32>,
    #[serde(default)]
    pub working: Vec<RawWorkingDef>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkingDef {
    pub id: String,
    pub provider: String,
    pub label: String,
    pub version: u32,
    pub handler: WorkingHandler,
    pub mode: DeliveryMode,
    pub focus: String,
    pub charge: u64,
    pub charge_per_magnitude: u64,
    pub charge_per_block: u64,
    pub charge_per_second: u64,
    pub dross: u64,
    pub dross_per_magnitude: u64,
    pub safe_throughput: u64,
    pub range: u16,
    pub max_magnitude: u32,
    pub max_volume: u32,
    pub max_targets: u16,
    pub max_duration_ticks: u64,
    pub target: Vec<String>,
    pub physical: Vec<String>,
    pub interruption: InterruptionPolicy,
    pub disposition: CurrentDisposition,
    pub wear: u16,
    pub ambient: bool,
    pub description: String,
}

impl WorkingDef {
    pub fn from_raw(provider: &str, raw: RawWorkingDef) -> Result<Self, WorkingError> {
        let id = qualify(provider, &raw.id);
        let focus = qualify(provider, &raw.focus);
        let definition = Self {
            id,
            provider: provider.into(),
            label: raw.label.unwrap_or_else(|| humanize(&raw.id)),
            version: raw.version,
            handler: raw.handler,
            mode: raw.mode,
            focus,
            charge: raw.charge,
            charge_per_magnitude: raw.charge_per_magnitude,
            charge_per_block: raw.charge_per_block,
            charge_per_second: raw.charge_per_second,
            dross: raw.dross,
            dross_per_magnitude: raw.dross_per_magnitude,
            safe_throughput: raw.safe_throughput,
            range: raw.range,
            max_magnitude: raw.max_magnitude,
            max_volume: raw.max_volume,
            max_targets: raw.max_targets,
            max_duration_ticks: raw.max_duration_ticks,
            target: raw.target,
            physical: raw.physical,
            interruption: raw.interruption,
            disposition: raw.disposition,
            wear: raw.wear,
            ambient: raw.ambient,
            description: raw.description,
        };
        definition.validate()?;
        Ok(definition)
    }

    pub fn validate(&self) -> Result<(), WorkingError> {
        if !valid_content_id(&self.id)
            || !valid_content_id(&self.focus)
            || !bounded_text(&self.provider, MAX_WORKING_ID_BYTES)
        {
            return Err(WorkingError::InvalidContent(format!(
                "{}: working, provider, and focus need bounded lowercase qualified ids",
                self.id
            )));
        }
        if self.version != WORKING_DEFINITION_VERSION {
            return Err(WorkingError::InvalidContent(format!(
                "{}: unsupported working definition version {}",
                self.id, self.version
            )));
        }
        if self.mode != self.handler.mode() {
            return Err(WorkingError::InvalidContent(format!(
                "{}: {:?} is a {:?} handler, not {:?}",
                self.id,
                self.handler,
                self.handler.mode(),
                self.mode
            )));
        }
        if self.charge == 0
            || self.charge > MAX_WORKING_CHARGE
            || self.safe_throughput == 0
            || self.safe_throughput > MAX_WORKING_CHARGE
            || self.dross > self.charge
            || self.range > MAX_WORKING_RANGE
            || self.max_magnitude == 0
            || self.max_magnitude > MAX_WORKING_MAGNITUDE
            || self.max_targets == 0
            || usize::from(self.max_targets) > MAX_WORKING_TARGETS
            || self.max_duration_ticks > MAX_WORKING_DURATION_TICKS
        {
            return Err(WorkingError::InvalidContent(format!(
                "{}: working cost, throughput, range, duration, or magnitude is outside native bounds",
                self.id
            )));
        }
        if self.mode == DeliveryMode::Ritual && self.max_duration_ticks == 0 {
            return Err(WorkingError::InvalidContent(format!(
                "{}: a ritual must declare a bounded duration",
                self.id
            )));
        }
        if self.target.is_empty()
            || self.target.len() > MAX_WORKING_TARGETS
            || self.target.iter().any(|target| {
                !self.handler.targets().contains(&target.as_str())
                    || !bounded_token(target, MAX_WORKING_ID_BYTES)
            })
        {
            return Err(WorkingError::InvalidContent(format!(
                "{}: target list requests a capability not owned by {:?}",
                self.id, self.handler
            )));
        }
        let required = self.handler.physical_requirements();
        if required.iter().any(|need| {
            !self
                .physical
                .iter()
                .any(|declared| declared.as_str() == *need)
        }) || self.physical.iter().any(|value| {
            !required.contains(&value.as_str()) || !bounded_token(value, MAX_WORKING_ID_BYTES)
        }) {
            return Err(WorkingError::InvalidContent(format!(
                "{}: physical prerequisites must exactly use the {:?} capability contract",
                self.id, self.handler
            )));
        }
        if !bounded_text(&self.label, MAX_WORKING_ID_BYTES)
            || self.description == default_doc()
            || !bounded_text(&self.description, MAX_WORKING_TEXT_BYTES)
        {
            return Err(WorkingError::InvalidContent(format!(
                "{}: browser label/description is absent or exceeds its metadata budget",
                self.id
            )));
        }
        if matches!(self.handler, WorkingHandler::Draw) && self.max_volume == 0 {
            return Err(WorkingError::InvalidContent(format!(
                "{}: fluid transfer must declare a nonzero maximum volume",
                self.id
            )));
        }
        Ok(())
    }

    pub fn quote(
        &self,
        magnitude: u32,
        distance_blocks: u16,
        duration_ticks: u64,
    ) -> Result<WorkingQuote, WorkingError> {
        if magnitude == 0
            || magnitude > self.max_magnitude
            || distance_blocks > self.range
            || duration_ticks > self.max_duration_ticks
        {
            return Err(WorkingError::InvalidOperation(format!(
                "{} request exceeds its declared magnitude, range, or duration",
                self.id
            )));
        }
        let seconds = duration_ticks.div_ceil(20);
        let charge = self
            .charge
            .checked_add(
                self.charge_per_magnitude
                    .checked_mul(u64::from(magnitude))
                    .ok_or(WorkingError::Overflow)?,
            )
            .and_then(|value| {
                value.checked_add(
                    self.charge_per_block
                        .checked_mul(u64::from(distance_blocks))?,
                )
            })
            .and_then(|value| value.checked_add(self.charge_per_second.checked_mul(seconds)?))
            .ok_or(WorkingError::Overflow)?;
        let dross = self
            .dross
            .checked_add(
                self.dross_per_magnitude
                    .checked_mul(u64::from(magnitude))
                    .ok_or(WorkingError::Overflow)?,
            )
            .ok_or(WorkingError::Overflow)?;
        if charge > MAX_WORKING_CHARGE || dross > charge {
            return Err(WorkingError::InvalidOperation(format!(
                "{} quote exceeds its conserved charge envelope",
                self.id
            )));
        }
        Ok(WorkingQuote {
            charge,
            base_dross: dross,
            magnitude,
            distance_blocks,
            duration_ticks,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkingQuote {
    pub charge: u64,
    pub base_dross: u64,
    pub magnitude: u32,
    pub distance_blocks: u16,
    pub duration_ticks: u64,
}

/// Visible inputs to deterministic strain. All quantities are integer
/// permille or native unit counts; there is no hidden failure roll.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StrainInputs {
    pub resonance_mismatch_permille: u16,
    pub component_instability_permille: u16,
    pub throughput: u64,
    pub safe_throughput: u64,
    pub local_capacity_permille: u16,
    pub below_safe_floor_units: u64,
    pub apparatus_damage_permille: u16,
    pub contamination_permille: u16,
    pub interruption: bool,
    pub forced_overdraw_units: u64,
    /// Actor-bound preparation modifier. One thousand is ordinary strain;
    /// lower values reduce personal strain without revising fixed apparatus
    /// dross or physical damage.
    #[serde(default = "default_strain_permille")]
    pub personal_strain_permille: u16,
}

const fn default_strain_permille() -> u16 {
    1_000
}

impl Default for StrainInputs {
    fn default() -> Self {
        Self {
            resonance_mismatch_permille: 0,
            component_instability_permille: 0,
            throughput: 0,
            safe_throughput: 0,
            local_capacity_permille: 0,
            below_safe_floor_units: 0,
            apparatus_damage_permille: 0,
            contamination_permille: 0,
            interruption: false,
            forced_overdraw_units: 0,
            personal_strain_permille: 1_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct StrainOutcome {
    pub strain: u32,
    pub extra_dross: u64,
    pub refuses: bool,
    /// 0 settled, 1 tense, 2 discordant, 3 overload. Presentation uses the
    /// same value for sound and non-audio cues.
    pub warning_band: u8,
}

pub fn deterministic_strain(inputs: StrainInputs) -> Result<StrainOutcome, WorkingError> {
    if inputs.resonance_mismatch_permille > 1_000
        || inputs.component_instability_permille > 1_000
        || inputs.local_capacity_permille > 1_000
        || inputs.apparatus_damage_permille > 1_000
        || inputs.contamination_permille > 1_000
        || inputs.personal_strain_permille > 1_000
    {
        return Err(WorkingError::InvalidOperation(
            "strain inputs use permille values outside 0..=1000".into(),
        ));
    }
    let mut score = u64::from(inputs.resonance_mismatch_permille)
        .checked_mul(2)
        .and_then(|score| score.checked_add(u64::from(inputs.component_instability_permille) * 3))
        .and_then(|score| score.checked_add(u64::from(inputs.apparatus_damage_permille) * 4))
        .and_then(|score| score.checked_add(u64::from(inputs.contamination_permille) * 3))
        .ok_or(WorkingError::Overflow)?;
    score = score
        .checked_add(u64::from(
            1_000u16.saturating_sub(inputs.local_capacity_permille),
        ))
        .and_then(|score| score.checked_add(inputs.below_safe_floor_units.saturating_mul(20)))
        .and_then(|score| score.checked_add(inputs.forced_overdraw_units.saturating_mul(40)))
        .ok_or(WorkingError::Overflow)?;
    if inputs.safe_throughput == 0 {
        return Err(WorkingError::InvalidOperation(
            "safe throughput cannot be zero".into(),
        ));
    }
    if inputs.throughput > inputs.safe_throughput {
        score = score
            .checked_add(
                inputs
                    .throughput
                    .saturating_sub(inputs.safe_throughput)
                    .saturating_mul(30),
            )
            .ok_or(WorkingError::Overflow)?;
    }
    if inputs.interruption {
        score = score.checked_add(1_000).ok_or(WorkingError::Overflow)?;
    }
    score = score
        .checked_mul(u64::from(inputs.personal_strain_permille))
        .ok_or(WorkingError::Overflow)?
        .div_ceil(1_000);
    let strain = u32::try_from(score.min(u64::from(u32::MAX))).unwrap_or(u32::MAX);
    let extra_dross = score.div_ceil(1_000);
    let warning_band = match score {
        0..=999 => 0,
        1_000..=2_999 => 1,
        3_000..=5_999 => 2,
        _ => 3,
    };
    Ok(StrainOutcome {
        strain,
        extra_dross,
        refuses: inputs.forced_overdraw_units != 0 && score >= 10_000,
        warning_band,
    })
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorkingIntent {
    Start,
    /// Deliberately permits a wand to draw below the measured local safe
    /// floor. The host still computes the exact consequences and may refuse
    /// an apparatus that cannot survive them.
    StartForced,
    Hold,
    Release,
    Cancel,
}

/// A client names intent and visible targets only. Expected versions, costs,
/// physical inputs, and effects are always reconstructed by the host.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorkingTargetIntent {
    None,
    Block {
        pos: BlockPos,
        adjacent: Option<BlockPos>,
    },
    Water {
        from: BlockPos,
        to: BlockPos,
        water_hu: u64,
    },
    Entity {
        stable_id: u64,
    },
    Inventory {
        target_slot: u8,
        material_slot: Option<u8>,
        magnitude: u32,
    },
    Ritual {
        controller: BlockPos,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorkingCueKind {
    Settle,
    Active,
    Complete,
    Cancel,
    Refuse,
    Strain,
    Overload,
}

/// Guest-safe presentation data. It names the visible source/path and warning
/// band without exposing exact Current, private inventory, or hidden state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkingCue {
    pub stable_id: u64,
    pub working_id: String,
    pub handler: WorkingHandler,
    pub source: BlockPos,
    pub path: Vec<BlockPos>,
    pub kind: WorkingCueKind,
    pub warning_band: u8,
    pub completion_permille: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkingResult {
    pub success: bool,
    pub stable_id: u64,
    pub phase: Option<WorkingPhase>,
    pub cue: WorkingCueKind,
    pub warning_band: u8,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorkingPhase {
    Charging,
    Active,
    PendingApply,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorkingApparatus {
    Wand {
        instance_id: u64,
        expected_revision: u64,
    },
    Ritual {
        controller: BlockPos,
        expected_revision: u64,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorkingTargetSnapshot {
    Block {
        pos: BlockPos,
        block_name: String,
        metadata: u8,
        version: u64,
    },
    Item {
        stable_id: u64,
        item_name: String,
        durability: u32,
        age_ticks: u64,
        version: u64,
    },
    Entity {
        stable_id: u64,
        kind: String,
        version: u64,
    },
    Reservoir {
        pos: BlockPos,
        water_hu: u64,
        salt_mass: u64,
        temperature_millic: i32,
        thermal_millic_hu: i64,
        dross_units: u64,
        carrier_remainder: u64,
        version: u64,
    },
    Area {
        controller: BlockPos,
        revision: u64,
        cells: Vec<BlockPos>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PhysicalDebitKind {
    Material,
    Item,
    Fluid,
    SoilWater,
    SoilNutrients,
    Food,
    Heat,
    Durability,
    ElapsedAge,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicalDebit {
    pub kind: PhysicalDebitKind,
    pub source: String,
    pub content_id: String,
    pub units: u64,
    pub expected_version: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CurrentDebit {
    pub owner: ArcaneOwner,
    pub expected_version: u64,
    pub current: Current,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlantAdvance {
    pub pos: BlockPos,
    pub before_block: u16,
    pub after_block: u16,
    pub before_meta: u8,
    pub after_meta: u8,
    pub soil_pos: Option<BlockPos>,
    pub before_soil_meta: u8,
    pub after_soil_meta: u8,
    pub water_source: Option<BlockPos>,
    pub water_before_hu: u64,
    pub water_after_hu: u64,
    pub salt_before: u64,
    pub salt_after: u64,
    pub water_hu: u64,
    pub nutrient_units: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WardSegment {
    pub pos: BlockPos,
    pub expected_block: u16,
    pub expected_damage: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorkingEffectKind {
    Observe,
    PointLight,
    Ignite,
    Impulse,
    AdvancePlant,
    TransferWater,
    RepairItem,
    Preserve,
    Settle,
    AdvanceBed,
    Ward,
    TransferCurrent,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum NudgeEntityKind {
    #[default]
    Projectile,
    DroppedItem,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum PreservationKind {
    #[default]
    ElapsedAge,
    ChargeLeakage,
}

/// Domain effects are intentionally closed. Adding another semantic mutation
/// requires adding and reviewing a native variant and handler together.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorkingEffect {
    Observe {
        origin: BlockPos,
        expires_tick: u64,
    },
    PointLight {
        source: BlockPos,
        target: BlockPos,
        intensity: u8,
        expires_tick: u64,
    },
    Ignite {
        fuel: BlockPos,
        fire_cell: BlockPos,
        expected_fuel: u16,
        expected_air: u16,
        player_caused: bool,
    },
    Impulse {
        entity_id: u64,
        #[serde(default)]
        entity_kind: NudgeEntityKind,
        source: BlockPos,
        target: BlockPos,
        before_velocity_milli: [i32; 3],
        impulse_milli: [i32; 3],
        expected_version: u64,
    },
    /// A bounded ordinary mechanism operation. Nudge may toggle only the
    /// named native mechanism state; it receives no generic block mutation
    /// capability and may not change material identity.
    OperateMechanism {
        source: BlockPos,
        target: BlockPos,
        block_name: String,
        before_state: u8,
        after_state: u8,
    },
    AdvancePlant(PlantAdvance),
    TransferWater {
        from: BlockPos,
        to: BlockPos,
        water_hu: u64,
        salt_mass: u64,
        temperature_millic: i32,
        thermal_millic_hu: i64,
        dross_units: u64,
        dross_subunits: u64,
        carrier_remainder_before: u64,
        carrier_remainder_after: u64,
    },
    RepairItem {
        item_id: u64,
        item_name: String,
        before_durability: u32,
        after_durability: u32,
        repair_material: String,
        material_units: u64,
        residue_item: String,
        residue_units: u32,
    },
    Preserve {
        item_id: u64,
        #[serde(default)]
        preservation_kind: PreservationKind,
        before_age_ticks: u64,
        elapsed_ticks: u64,
        age_advance_ticks: u64,
        #[serde(default)]
        charge_spent_units: u64,
    },
    Settle {
        controller: BlockPos,
        process_id: u64,
        dross_vessel_id: u64,
        dross_routed: u64,
        stabilizer_wear: u16,
    },
    AdvanceBed {
        controller: BlockPos,
        plants: Vec<PlantAdvance>,
        scheduled_tick: u64,
    },
    Ward {
        controller: BlockPos,
        segments: Vec<WardSegment>,
        pressure_kind: String,
        pressure_units: u64,
        ire_before_millipoints: i64,
        ire_after_millipoints: i64,
    },
    TransferCurrent {
        from: ArcaneOwner,
        to: ArcaneOwner,
        current: Current,
    },
}

impl WorkingEffect {
    pub const fn kind(&self) -> WorkingEffectKind {
        match self {
            Self::Observe { .. } => WorkingEffectKind::Observe,
            Self::PointLight { .. } => WorkingEffectKind::PointLight,
            Self::Ignite { .. } => WorkingEffectKind::Ignite,
            Self::Impulse { .. } | Self::OperateMechanism { .. } => WorkingEffectKind::Impulse,
            Self::AdvancePlant(_) => WorkingEffectKind::AdvancePlant,
            Self::TransferWater { .. } => WorkingEffectKind::TransferWater,
            Self::RepairItem { .. } => WorkingEffectKind::RepairItem,
            Self::Preserve { .. } => WorkingEffectKind::Preserve,
            Self::Settle { .. } => WorkingEffectKind::Settle,
            Self::AdvanceBed { .. } => WorkingEffectKind::AdvanceBed,
            Self::Ward { .. } => WorkingEffectKind::Ward,
            Self::TransferCurrent { .. } => WorkingEffectKind::TransferCurrent,
        }
    }

    fn validate(&self) -> Result<(), WorkingError> {
        match self {
            Self::PointLight { intensity, .. } if *intensity == 0 || *intensity > 15 => {
                Err(WorkingError::InvalidOperation(
                    "Gleam intensity must be in 1..=15".into(),
                ))
            }
            Self::Impulse {
                entity_id,
                source,
                target,
                before_velocity_milli,
                impulse_milli,
                expected_version,
                ..
            } if *entity_id == 0
                || source == target
                || *impulse_milli == [0; 3]
                || *expected_version == 0
                || before_velocity_milli
                    .iter()
                    .chain(impulse_milli)
                    .any(|component| component.unsigned_abs() > 80_000) =>
            {
                Err(WorkingError::InvalidOperation(
                    "Nudge needs one stable visible entity and a bounded ordinary impulse".into(),
                ))
            }
            Self::OperateMechanism {
                source,
                target,
                block_name,
                before_state,
                after_state,
            } if source == target
                || !valid_content_id(block_name)
                || *before_state > 1
                || *after_state > 1
                || (*before_state ^ *after_state) != 1 =>
            {
                Err(WorkingError::InvalidOperation(
                    "Nudge may only toggle one declared mechanism latch without changing its material"
                        .into(),
                ))
            }
            Self::AdvancePlant(advance)
                if (advance.before_block == advance.after_block
                    && advance.before_meta == advance.after_meta)
                    || (advance.water_hu == 0 && advance.nutrient_units == 0)
                    || advance.water_before_hu.saturating_sub(advance.water_after_hu)
                        != advance.water_hu
                    || advance.salt_after > advance.salt_before =>
            {
                Err(WorkingError::InvalidOperation(
                    "Rootwake must advance one stage and debit biological inputs".into(),
                ))
            }
            Self::TransferWater {
                from,
                to,
                water_hu,
                salt_mass,
                temperature_millic,
                thermal_millic_hu,
                dross_units,
                dross_subunits,
                carrier_remainder_before,
                carrier_remainder_after,
            } if from == to
                || *water_hu == 0
                || *salt_mass > water_hu.saturating_mul(255)
                || !(-100_000..=100_000).contains(temperature_millic)
                || i64::from(*temperature_millic)
                    != *thermal_millic_hu / i64::try_from(*water_hu).unwrap_or(i64::MAX)
                || *dross_units != *dross_subunits / 256
                || *carrier_remainder_before >= 256
                || *carrier_remainder_after >= 256 =>
            {
                Err(WorkingError::InvalidOperation(
                    "Draw needs distinct reservoirs and a valid exact water/salt/heat/dross parcel"
                        .into(),
                ))
            }
            Self::RepairItem {
                item_id,
                item_name,
                before_durability,
                after_durability,
                repair_material,
                material_units,
                residue_item,
                ..
            } if *item_id == 0
                || after_durability <= before_durability
                || *material_units == 0
                || !valid_content_id(item_name)
                || !valid_content_id(repair_material)
                || !valid_content_id(residue_item) =>
            {
                Err(WorkingError::InvalidOperation(
                    "Fieldmend needs one damaged item, matching matter, residue, and a bounded repair"
                        .into(),
                ))
            }
            Self::Preserve {
                item_id,
                elapsed_ticks,
                age_advance_ticks,
                charge_spent_units,
                ..
            } if *item_id == 0
                || age_advance_ticks > elapsed_ticks
                || *charge_spent_units > MAX_WORKING_CHARGE =>
            {
                Err(WorkingError::InvalidOperation(
                    "Holdfast may slow elapsed age but cannot reverse it".into(),
                ))
            }
            Self::AdvanceBed { plants, .. }
                if plants.is_empty() || plants.len() > MAX_WORKING_TARGETS =>
            {
                Err(WorkingError::InvalidOperation(
                    "rooting bed has an empty or unbounded crop census".into(),
                ))
            }
            Self::Ward {
                segments,
                pressure_kind,
                ire_before_millipoints,
                ire_after_millipoints,
                ..
            } if segments.len() < 4
                || segments.len() > MAX_WARD_SEGMENTS
                || pressure_kind.is_empty()
                || ire_before_millipoints != ire_after_millipoints =>
            {
                Err(WorkingError::InvalidOperation(
                    "ward needs a bounded closed boundary and may not change Ire".into(),
                ))
            }
            Self::TransferCurrent { from, to, current }
                if from == to || current.is_empty() =>
            {
                Err(WorkingError::InvalidOperation(
                    "transfer circle requires distinct adjacent owners and nonempty Current".into(),
                ))
            }
            _ => Ok(()),
        }
    }
}

fn legacy_working_source() -> BlockPos {
    BlockPos::new(crate::planet::Face::PosX, 0, 1, 0)
        .expect("legacy workings fallback is inside the finite planet")
}

/// Ordered physical route shared by restored transactions and live execution.
pub(crate) fn working_effect_positions(effect: &WorkingEffect) -> Vec<BlockPos> {
    match effect {
        WorkingEffect::Observe { origin, .. } => vec![*origin],
        WorkingEffect::PointLight { source, target, .. } => vec![*source, *target],
        WorkingEffect::Ignite {
            fuel, fire_cell, ..
        } => vec![*fuel, *fire_cell],
        WorkingEffect::Impulse { source, target, .. }
        | WorkingEffect::OperateMechanism { source, target, .. } => vec![*source, *target],
        WorkingEffect::AdvancePlant(advance) => std::iter::once(advance.pos)
            .chain(advance.soil_pos)
            .chain(advance.water_source)
            .collect(),
        WorkingEffect::TransferWater { from, to, .. } => vec![*from, *to],
        WorkingEffect::Settle { controller, .. } => vec![*controller],
        WorkingEffect::AdvanceBed {
            controller, plants, ..
        } => std::iter::once(*controller)
            .chain(plants.iter().map(|plant| plant.pos))
            .collect(),
        WorkingEffect::Ward {
            controller,
            segments,
            ..
        } => std::iter::once(*controller)
            .chain(segments.iter().map(|segment| segment.pos))
            .collect(),
        WorkingEffect::RepairItem { .. }
        | WorkingEffect::Preserve { .. }
        | WorkingEffect::TransferCurrent { .. } => Vec::new(),
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkingTransaction {
    pub id: u64,
    pub definition: WorkingDef,
    pub actor: [u8; 16],
    pub actor_label: String,
    /// Real planetary origin used for attribution, regional settlement, and
    /// guest-safe presentation. Inventory effects may have no block target,
    /// so deriving this from the effect would fabricate a dummy location.
    #[serde(default = "legacy_working_source")]
    pub source: BlockPos,
    /// Bounded visible route only; never contains hidden server state.
    #[serde(default)]
    pub path: Vec<BlockPos>,
    pub apparatus: WorkingApparatus,
    pub targets: Vec<WorkingTargetSnapshot>,
    pub current_debits: Vec<CurrentDebit>,
    pub reserved_current: Current,
    pub physical_debits: Vec<PhysicalDebit>,
    pub effect: WorkingEffect,
    pub return_current: Current,
    pub dross_current: Current,
    pub phase: WorkingPhase,
    pub started_tick: u64,
    pub due_tick: u64,
    pub interruption: InterruptionPolicy,
    pub strain: StrainOutcome,
    /// Player-authorized unsafe draw, persisted so replay and audit never
    /// have to infer intent from a derived strain number.
    #[serde(default)]
    pub forced: bool,
    pub trace: String,
    pub completion_nonce: u64,
}

impl WorkingTransaction {
    pub fn validate(&self) -> Result<(), WorkingError> {
        self.definition.validate()?;
        if self.id == 0
            || !bounded_text(&self.actor_label, MAX_WORKING_ID_BYTES)
            || !bounded_text(&self.trace, MAX_WORKING_TEXT_BYTES)
            || self.path.is_empty()
            || self.path.len() > MAX_WORKING_TARGETS
            || self.targets.is_empty()
            || self.targets.len()
                > usize::from(self.definition.max_targets)
                    .saturating_add(self.physical_debits.len())
                    .min(MAX_WORKING_TARGETS)
            || self.current_debits.is_empty()
            || self.current_debits.len() > MAX_WORKING_DEBITS
            || self.physical_debits.len() > MAX_WORKING_DEBITS
            || self.due_tick < self.started_tick
            || self.due_tick.saturating_sub(self.started_tick) > self.definition.max_duration_ticks
            || self.interruption != self.definition.interruption
            || self.effect.kind() != self.definition.handler.effect_kind()
        {
            return Err(WorkingError::InvalidOperation(
                "working transaction identity, bounds, timing, or handler capability is invalid"
                    .into(),
            ));
        }
        self.effect.validate()?;
        let mut debit_current = Current::default();
        for debit in &self.current_debits {
            debit_current
                .checked_add(&debit.current)
                .map_err(|error| WorkingError::InvalidOperation(error.to_string()))?;
        }
        let mut settled_current = self.return_current.clone();
        settled_current
            .checked_add(&self.dross_current)
            .map_err(|error| WorkingError::InvalidOperation(error.to_string()))?;
        let reserved_total = self
            .reserved_current
            .total_checked()
            .map_err(|error| WorkingError::InvalidOperation(error.to_string()))?;
        if debit_current != self.reserved_current || self.reserved_current != settled_current {
            return Err(WorkingError::InvalidOperation(format!(
                "working {} does not conserve its exact resonance mixture across debit, reserve, and settlement",
                self.id
            )));
        }
        if let WorkingEffect::TransferCurrent { current, .. } = &self.effect {
            let mut deliverable = self.return_current.clone();
            deliverable.checked_sub(current).map_err(|_| {
                WorkingError::InvalidOperation(format!(
                    "working {} cannot deliver charge that was not reserved in its clean settlement",
                    self.id
                ))
            })?;
        }
        if reserved_total < self.definition.charge {
            return Err(WorkingError::InvalidOperation(format!(
                "working {} reserved less than its minimum charge",
                self.id
            )));
        }
        for debit in &self.physical_debits {
            if debit.units == 0
                || !bounded_text(&debit.source, MAX_WORKING_TEXT_BYTES)
                || !bounded_token(&debit.content_id, MAX_WORKING_ID_BYTES)
            {
                return Err(WorkingError::InvalidOperation(
                    "working physical debit is missing its source, identity, or quantity".into(),
                ));
            }
        }
        self.validate_physical_contract()?;
        Ok(())
    }

    fn validate_physical_contract(&self) -> Result<(), WorkingError> {
        let sum = |kind: PhysicalDebitKind, content: Option<&str>| {
            self.physical_debits
                .iter()
                .filter(|debit| {
                    debit.kind == kind
                        && content.is_none_or(|expected| debit.content_id == expected)
                })
                .fold(0u64, |total, debit| total.saturating_add(debit.units))
        };
        let kinds_are = |allowed: &[PhysicalDebitKind]| {
            self.physical_debits
                .iter()
                .all(|debit| allowed.contains(&debit.kind))
        };
        let valid = match (&self.definition.handler, &self.effect) {
            (WorkingHandler::Trace | WorkingHandler::Gleam | WorkingHandler::Nudge, _) => {
                self.physical_debits.is_empty()
            }
            (WorkingHandler::Ignite, WorkingEffect::Ignite { .. }) => {
                kinds_are(&[PhysicalDebitKind::Heat]) && sum(PhysicalDebitKind::Heat, None) == 1
            }
            (WorkingHandler::Rootwake, WorkingEffect::AdvancePlant(advance)) => {
                kinds_are(&[
                    PhysicalDebitKind::SoilWater,
                    PhysicalDebitKind::SoilNutrients,
                ]) && sum(PhysicalDebitKind::SoilWater, Some("water")) == advance.water_hu
                    && sum(PhysicalDebitKind::SoilNutrients, Some("soil_nutrients"))
                        == advance.nutrient_units
            }
            (
                WorkingHandler::Draw,
                WorkingEffect::TransferWater {
                    water_hu,
                    salt_mass,
                    dross_subunits,
                    ..
                },
            ) => {
                kinds_are(&[PhysicalDebitKind::Fluid])
                    && sum(PhysicalDebitKind::Fluid, Some("water")) == *water_hu
                    && sum(PhysicalDebitKind::Fluid, Some("salt")) == *salt_mass
                    && sum(PhysicalDebitKind::Fluid, Some("waterborne_dross_subunit"))
                        == *dross_subunits
            }
            (
                WorkingHandler::Fieldmend,
                WorkingEffect::RepairItem {
                    before_durability,
                    after_durability,
                    material_units,
                    ..
                },
            ) => {
                kinds_are(&[PhysicalDebitKind::Material, PhysicalDebitKind::Item])
                    && sum(PhysicalDebitKind::Material, None) == *material_units
                    && sum(PhysicalDebitKind::Item, None)
                        == u64::from(after_durability.saturating_sub(*before_durability))
            }
            (WorkingHandler::Holdfast, WorkingEffect::Preserve { .. }) => {
                kinds_are(&[PhysicalDebitKind::ElapsedAge])
                    && sum(PhysicalDebitKind::ElapsedAge, None) != 0
            }
            (WorkingHandler::SettlingRite, WorkingEffect::Settle { .. }) => {
                kinds_are(&[PhysicalDebitKind::Durability, PhysicalDebitKind::Item])
                    && sum(PhysicalDebitKind::Durability, None) != 0
                    && sum(PhysicalDebitKind::Item, None) == 1
            }
            (WorkingHandler::RootingBed, WorkingEffect::AdvanceBed { plants, .. }) => {
                let water = plants
                    .iter()
                    .fold(0u64, |total, plant| total.saturating_add(plant.water_hu));
                let nutrients = plants.iter().fold(0u64, |total, plant| {
                    total.saturating_add(plant.nutrient_units)
                });
                kinds_are(&[
                    PhysicalDebitKind::SoilWater,
                    PhysicalDebitKind::SoilNutrients,
                    PhysicalDebitKind::Durability,
                ]) && sum(PhysicalDebitKind::SoilWater, Some("water")) == water
                    && sum(PhysicalDebitKind::SoilNutrients, Some("soil_nutrients")) == nutrients
                    && sum(PhysicalDebitKind::Durability, Some("focus_posts")) != 0
            }
            (WorkingHandler::WardBoundary, WorkingEffect::Ward { segments, .. }) => {
                kinds_are(&[PhysicalDebitKind::Durability, PhysicalDebitKind::Item])
                    && sum(PhysicalDebitKind::Durability, Some("closed_boundary"))
                        == segments.len() as u64
                    && sum(PhysicalDebitKind::Item, None) == 1
            }
            (WorkingHandler::TransferCircle, WorkingEffect::TransferCurrent { current, .. }) => {
                kinds_are(&[PhysicalDebitKind::Item])
                    && self.physical_debits.len() == 2
                    && self
                        .physical_debits
                        .iter()
                        .all(|debit| debit.units == current.total())
            }
            _ => false,
        };
        if valid {
            Ok(())
        } else {
            Err(WorkingError::InvalidOperation(format!(
                "working {} omits, invents, or mismatches its handler's physical debit contract",
                self.id
            )))
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkingAuditEvent {
    pub id: u64,
    pub working_id: String,
    pub actor: [u8; 16],
    pub outcome: String,
    pub current_units: u64,
    pub dross_units: u64,
    pub completed_tick: u64,
    #[serde(default = "legacy_working_source")]
    pub source: BlockPos,
    #[serde(default)]
    pub path: Vec<BlockPos>,
    #[serde(default)]
    pub warning_band: u8,
    #[serde(default)]
    pub forced: bool,
    pub trace: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkingsState {
    pub schema_version: u32,
    pub content_hash: u64,
    pub manifests: BTreeMap<String, WorkingDef>,
    pub active: BTreeMap<u64, WorkingTransaction>,
    pub history: VecDeque<WorkingAuditEvent>,
    #[serde(skip)]
    path: PathBuf,
}

impl WorkingsState {
    pub fn load(world: &Path) -> Result<Option<Self>, WorkingError> {
        let path = world.join(WORKINGS_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let metadata = std::fs::metadata(&path)?;
        if metadata.len() > MAX_WORKINGS_FILE_BYTES {
            return Err(WorkingError::Corrupt(format!(
                "workings sidecar is {} bytes; limit is {MAX_WORKINGS_FILE_BYTES}",
                metadata.len()
            )));
        }
        let bytes = std::fs::read(&path)?;
        let mut state: Self = postcard::from_bytes(&bytes)
            .map_err(|error| WorkingError::Corrupt(error.to_string()))?;
        state.path = path;
        state.migrate_legacy_locations();
        state.validate()?;
        Ok(Some(state))
    }

    pub fn load_or_initialize(
        world: &Path,
        content_hash: u64,
        definitions: &BTreeMap<String, WorkingDef>,
    ) -> Result<Self, WorkingError> {
        let path = world.join(WORKINGS_FILE);
        if !path.exists() {
            let state = Self {
                schema_version: WORKINGS_SCHEMA_VERSION,
                content_hash,
                manifests: definitions.clone(),
                active: BTreeMap::new(),
                history: VecDeque::new(),
                path,
            };
            state.validate()?;
            return Ok(state);
        }
        let mut state = Self::load(world)?.expect("existing workings path was checked above");
        // Preserve definitions used by active transactions after provider
        // removal, while adding newly installed declarations for future use.
        for (id, definition) in definitions {
            if state
                .active
                .values()
                .any(|active| &active.definition.id == id)
            {
                if state.manifests.get(id) != Some(definition) {
                    return Err(WorkingError::InvalidContent(format!(
                        "{id}: provider changed an active working definition without a versioned migration"
                    )));
                }
            } else {
                state.manifests.insert(id.clone(), definition.clone());
            }
        }
        state.content_hash = content_hash;
        state.validate()?;
        Ok(state)
    }

    pub fn validate(&self) -> Result<(), WorkingError> {
        if self.schema_version != WORKINGS_SCHEMA_VERSION {
            return Err(WorkingError::Corrupt(format!(
                "unsupported workings schema {}",
                self.schema_version
            )));
        }
        if self.manifests.len() > MAX_WORKING_DEFINITIONS
            || self.active.len() > MAX_ACTIVE_WORKINGS
            || self.history.len() > MAX_WORKING_HISTORY
            || self
                .active
                .values()
                .filter(|transaction| transaction.definition.mode == DeliveryMode::Ritual)
                .count()
                > MAX_ACTIVE_RITUALS
        {
            return Err(WorkingError::Corrupt(
                "workings state exceeds its bounded census".into(),
            ));
        }
        for (id, definition) in &self.manifests {
            if id != &definition.id {
                return Err(WorkingError::Corrupt(
                    "working manifest key does not match its identity".into(),
                ));
            }
            definition
                .validate()
                .map_err(|error| WorkingError::Corrupt(error.to_string()))?;
        }
        for (id, transaction) in &self.active {
            if *id != transaction.id
                || self
                    .manifests
                    .get(&transaction.definition.id)
                    .is_none_or(|definition| {
                        definition.version != transaction.definition.version
                            || definition.handler != transaction.definition.handler
                    })
            {
                return Err(WorkingError::Corrupt(
                    "active working has an unknown or mismatched saved definition".into(),
                ));
            }
            transaction
                .validate()
                .map_err(|error| WorkingError::Corrupt(error.to_string()))?;
        }
        for event in &self.history {
            if event.id == 0
                || !valid_content_id(&event.working_id)
                || !bounded_text(&event.outcome, MAX_WORKING_ID_BYTES)
                || !bounded_text(&event.trace, MAX_WORKING_TEXT_BYTES)
                || event.path.is_empty()
                || event.path.len() > MAX_WORKING_TARGETS
                || event.warning_band > 3
            {
                return Err(WorkingError::Corrupt(
                    "working audit history contains invalid metadata".into(),
                ));
            }
        }
        Ok(())
    }

    fn migrate_legacy_locations(&mut self) {
        for transaction in self.active.values_mut() {
            if transaction.path.is_empty() {
                let path = working_effect_positions(&transaction.effect);
                if let Some(source) = path.first().copied() {
                    transaction.source = source;
                    transaction.path = path;
                } else {
                    transaction.path.push(transaction.source);
                }
            }
        }
        for event in &mut self.history {
            if event.path.is_empty() {
                event.path.push(event.source);
            }
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, WorkingError> {
        self.validate()?;
        let bytes = postcard::to_allocvec(self)
            .map_err(|error| WorkingError::Corrupt(error.to_string()))?;
        if bytes.len() as u64 > MAX_WORKINGS_FILE_BYTES {
            return Err(WorkingError::InvalidOperation(format!(
                "workings sidecar would be {} bytes; limit is {MAX_WORKINGS_FILE_BYTES}",
                bytes.len()
            )));
        }
        Ok(bytes)
    }

    pub fn save(&self) -> Result<(), WorkingError> {
        let bytes = self.encode()?;
        if let Ok(old) = std::fs::read(&self.path) {
            crate::identity::atomic_write(&self.path.with_file_name(WORKINGS_BACKUP), &old, false)?;
        }
        crate::identity::atomic_write(&self.path, &bytes, false)?;
        Ok(())
    }

    pub fn active_ids(&self) -> BTreeSet<u64> {
        self.active.keys().copied().collect()
    }

    pub fn max_working_id(&self) -> u64 {
        self.active
            .keys()
            .copied()
            .chain(self.history.iter().map(|event| event.id))
            .max()
            .unwrap_or_default()
    }

    pub fn start(&mut self, transaction: WorkingTransaction) -> Result<(), WorkingError> {
        transaction.validate()?;
        if self.active.contains_key(&transaction.id) {
            return Err(WorkingError::InvalidOperation(format!(
                "working {} is already active",
                transaction.id
            )));
        }
        if self.active.len() >= MAX_ACTIVE_WORKINGS
            || (transaction.definition.mode == DeliveryMode::Ritual
                && self
                    .active
                    .values()
                    .filter(|active| active.definition.mode == DeliveryMode::Ritual)
                    .count()
                    >= MAX_ACTIVE_RITUALS)
        {
            return Err(WorkingError::InvalidOperation(
                "active working or ritual census is full".into(),
            ));
        }
        if let Some(conflict) = self.active.values().find(|active| {
            apparatuses_conflict(&active.apparatus, &transaction.apparatus)
                || active.targets.iter().any(|left| {
                    transaction
                        .targets
                        .iter()
                        .any(|right| targets_conflict(left, right))
                })
        }) {
            return Err(WorkingError::InvalidOperation(format!(
                "apparatus or target is already reserved by active working {}",
                conflict.id
            )));
        }
        self.manifests
            .entry(transaction.definition.id.clone())
            .or_insert_with(|| transaction.definition.clone());
        self.active.insert(transaction.id, transaction);
        self.validate()
    }

    pub fn phase(&mut self, id: u64, phase: WorkingPhase) -> Result<(), WorkingError> {
        let transaction = self
            .active
            .get_mut(&id)
            .ok_or_else(|| WorkingError::InvalidOperation(format!("working {id} is not active")))?;
        let allowed = matches!(
            (transaction.phase, phase),
            (WorkingPhase::Charging, WorkingPhase::Active)
                | (WorkingPhase::Charging, WorkingPhase::PendingApply)
                | (WorkingPhase::Active, WorkingPhase::PendingApply)
        );
        if !allowed {
            return Err(WorkingError::InvalidOperation(format!(
                "working {id} cannot move from {:?} to {phase:?}",
                transaction.phase
            )));
        }
        transaction.phase = phase;
        Ok(())
    }

    pub fn settle(
        &mut self,
        id: u64,
        outcome: &str,
        completed_tick: u64,
    ) -> Result<WorkingTransaction, WorkingError> {
        if !bounded_text(outcome, MAX_WORKING_ID_BYTES) {
            return Err(WorkingError::InvalidOperation(
                "working outcome text exceeds its metadata budget".into(),
            ));
        }
        let transaction = self
            .active
            .remove(&id)
            .ok_or_else(|| WorkingError::InvalidOperation(format!("working {id} is not active")))?;
        self.history.push_back(WorkingAuditEvent {
            id,
            working_id: transaction.definition.id.clone(),
            actor: transaction.actor,
            outcome: outcome.into(),
            current_units: transaction.reserved_current.total(),
            dross_units: transaction.dross_current.total(),
            completed_tick,
            source: transaction.source,
            path: transaction.path.clone(),
            warning_band: transaction.strain.warning_band,
            forced: transaction.forced,
            trace: transaction.trace.clone(),
        });
        while self.history.len() > MAX_WORKING_HISTORY {
            self.history.pop_front();
        }
        self.validate()?;
        Ok(transaction)
    }
}

/// A fitted implement is a physical source, not a reusable account handle.
/// Its Current account may have enough charge for several effects, but the
/// same wand cannot be in two hands/channels at once and one ritual controller
/// cannot run two overlapping arrangements. Revisions protect against later
/// mutation; stable identity is what makes concurrent reservations conflict.
fn apparatuses_conflict(left: &WorkingApparatus, right: &WorkingApparatus) -> bool {
    match (left, right) {
        (
            WorkingApparatus::Wand { instance_id: a, .. },
            WorkingApparatus::Wand { instance_id: b, .. },
        ) => a == b,
        (
            WorkingApparatus::Ritual { controller: a, .. },
            WorkingApparatus::Ritual { controller: b, .. },
        ) => a == b,
        _ => false,
    }
}

/// Transactions lock their exact physical targets while Current is active.
/// This is intentionally conservative: an area reservation conflicts with
/// any named cell inside it, and a reservoir conflicts with the voxel that
/// physically carries it. Domain adapters may add ordinary non-working locks
/// (inventory slots, pumps, and machine bays), but two workings can never
/// double-spend the same saved target.
fn targets_conflict(left: &WorkingTargetSnapshot, right: &WorkingTargetSnapshot) -> bool {
    use WorkingTargetSnapshot::{Area, Block, Entity, Item, Reservoir};

    match (left, right) {
        (Block { pos: a, .. }, Block { pos: b, .. })
        | (Block { pos: a, .. }, Reservoir { pos: b, .. })
        | (Reservoir { pos: a, .. }, Block { pos: b, .. })
        | (Reservoir { pos: a, .. }, Reservoir { pos: b, .. }) => a == b,
        (Item { stable_id: a, .. }, Item { stable_id: b, .. })
        | (Entity { stable_id: a, .. }, Entity { stable_id: b, .. }) => a == b,
        (
            Area {
                controller: a,
                cells,
                ..
            },
            Area {
                controller: b,
                cells: other,
                ..
            },
        ) => a == b || cells.iter().any(|cell| other.contains(cell)),
        (
            Area {
                controller, cells, ..
            },
            Block { pos, .. },
        )
        | (
            Area {
                controller, cells, ..
            },
            Reservoir { pos, .. },
        )
        | (
            Block { pos, .. },
            Area {
                controller, cells, ..
            },
        )
        | (
            Reservoir { pos, .. },
            Area {
                controller, cells, ..
            },
        ) => controller == pos || cells.contains(pos),
        _ => false,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingsAudit {
    pub schema_version: u32,
    pub content_hash: u64,
    pub file_bytes: u64,
    pub definitions: usize,
    pub active: usize,
    pub rituals: usize,
    pub pending_apply: usize,
    pub history: usize,
    pub missing_active_accounts: usize,
    pub mismatched_active_current: usize,
    pub orphan_working_accounts: usize,
    pub invalid_transactions: usize,
    pub arcane_conserved: bool,
}

impl WorkingsAudit {
    pub fn is_qualified(&self) -> bool {
        self.schema_version == WORKINGS_SCHEMA_VERSION
            && self.file_bytes <= MAX_WORKINGS_FILE_BYTES
            && self.definitions <= MAX_WORKING_DEFINITIONS
            && self.active <= MAX_ACTIVE_WORKINGS
            && self.rituals <= MAX_ACTIVE_RITUALS
            && self.history <= MAX_WORKING_HISTORY
            && self.missing_active_accounts == 0
            && self.mismatched_active_current == 0
            && self.orphan_working_accounts == 0
            && self.invalid_transactions == 0
            && self.arcane_conserved
    }

    pub fn render(&self) -> String {
        format!(
            concat!(
                "Workings audit schema {}\n",
                "Content hash: {:016x}\n",
                "Sidecar: {} / {} bytes\n",
                "Definitions: {} / {}\n",
                "Active: {} / {} ({} rituals / {}; {} pending apply)\n",
                "History: {} / {}\n",
                "Integrity: {} missing active accounts, {} mismatched reservations, {} orphan working accounts, {} invalid transactions\n",
                "Parent Current conserved: {}\n",
                "Qualified: {}\n"
            ),
            self.schema_version,
            self.content_hash,
            self.file_bytes,
            MAX_WORKINGS_FILE_BYTES,
            self.definitions,
            MAX_WORKING_DEFINITIONS,
            self.active,
            MAX_ACTIVE_WORKINGS,
            self.rituals,
            MAX_ACTIVE_RITUALS,
            self.pending_apply,
            self.history,
            MAX_WORKING_HISTORY,
            self.missing_active_accounts,
            self.mismatched_active_current,
            self.orphan_working_accounts,
            self.invalid_transactions,
            self.arcane_conserved,
            self.is_qualified(),
        )
    }
}

pub fn audit_world(world: &Path) -> Result<WorkingsAudit, WorkingError> {
    let state = WorkingsState::load(world)?
        .ok_or_else(|| WorkingError::Corrupt("world has no workings sidecar".into()))?;
    let ledger = crate::arcane::ArcaneLedger::load(world)
        .map_err(|error| WorkingError::Corrupt(error.to_string()))?;
    let arcane = ledger
        .audit()
        .map_err(|error| WorkingError::Corrupt(error.to_string()))?;
    let active_ids = state.active_ids();
    let mut missing_active_accounts = 0usize;
    let mut mismatched_active_current = 0usize;
    let mut invalid_transactions = 0usize;
    for transaction in state.active.values() {
        invalid_transactions += usize::from(transaction.validate().is_err());
        let account = ledger.account(&ArcaneOwner::Working(transaction.id));
        if transaction.phase == WorkingPhase::PendingApply {
            mismatched_active_current += usize::from(account.is_some());
        } else if let Some(account) = account {
            mismatched_active_current +=
                usize::from(account.current != transaction.reserved_current);
        } else {
            missing_active_accounts += 1;
        }
    }
    let orphan_working_accounts = ledger
        .accounts
        .keys()
        .filter(|owner| matches!(owner, ArcaneOwner::Working(id) if !active_ids.contains(id)))
        .count();
    Ok(WorkingsAudit {
        schema_version: state.schema_version,
        content_hash: state.content_hash,
        file_bytes: std::fs::metadata(world.join(WORKINGS_FILE))?.len(),
        definitions: state.manifests.len(),
        active: state.active.len(),
        rituals: state
            .active
            .values()
            .filter(|transaction| transaction.definition.mode == DeliveryMode::Ritual)
            .count(),
        pending_apply: state
            .active
            .values()
            .filter(|transaction| transaction.phase == WorkingPhase::PendingApply)
            .count(),
        history: state.history.len(),
        missing_active_accounts,
        mismatched_active_current,
        orphan_working_accounts,
        invalid_transactions,
        arcane_conserved: arcane.unexplained_delta == 0,
    })
}

#[derive(Debug)]
pub enum WorkingError {
    InvalidContent(String),
    InvalidOperation(String),
    Corrupt(String),
    Overflow,
    Io(std::io::Error),
}

impl std::fmt::Display for WorkingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidContent(message)
            | Self::InvalidOperation(message)
            | Self::Corrupt(message) => f.write_str(message),
            Self::Overflow => f.write_str("working integer overflow"),
            Self::Io(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for WorkingError {}

impl From<std::io::Error> for WorkingError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

fn qualify(provider: &str, id: &str) -> String {
    if id.contains(':') {
        id.into()
    } else {
        format!("{provider}:{id}")
    }
}

fn humanize(id: &str) -> String {
    let short = id.rsplit(':').next().unwrap_or(id);
    short
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + chars.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn valid_content_id(value: &str) -> bool {
    bounded_text(value, MAX_WORKING_ID_BYTES)
        && value.contains(':')
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b":_-/".contains(&byte)
        })
}

fn bounded_token(value: &str, max: usize) -> bool {
    bounded_text(value, max)
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_:-/".contains(&byte)
        })
}

fn bounded_text(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(handler: WorkingHandler) -> RawWorkingDef {
        RawWorkingDef {
            id: "test".into(),
            label: None,
            version: 1,
            handler,
            mode: handler.mode(),
            focus: "base:echo".into(),
            charge: 10,
            charge_per_magnitude: 1,
            charge_per_block: 1,
            charge_per_second: 0,
            dross: 1,
            dross_per_magnitude: 0,
            safe_throughput: 10,
            range: 5,
            max_magnitude: 1,
            max_volume: u32::from(handler == WorkingHandler::Draw),
            max_targets: 2,
            max_duration_ticks: if handler.mode() == DeliveryMode::Ritual {
                20
            } else {
                0
            },
            target: handler
                .targets()
                .iter()
                .map(|value| (*value).into())
                .collect(),
            physical: handler
                .physical_requirements()
                .iter()
                .map(|value| (*value).into())
                .collect(),
            interruption: InterruptionPolicy::RefundWithDross,
            disposition: CurrentDisposition::Split,
            wear: 1,
            ambient: false,
            description: "Bounded fixture working.".into(),
        }
    }

    #[test]
    fn every_native_handler_has_a_valid_declarative_contract() {
        for handler in WorkingHandler::ALL {
            WorkingDef::from_raw("fixture", raw(handler)).unwrap();
        }
    }

    #[test]
    fn handlers_cannot_request_another_domains_target_or_physical_capability() {
        let mut definition = raw(WorkingHandler::Trace);
        definition.target = vec!["combustible".into()];
        assert!(WorkingDef::from_raw("fixture", definition).is_err());

        let mut definition = raw(WorkingHandler::Trace);
        definition.physical = vec!["water".into()];
        assert!(WorkingDef::from_raw("fixture", definition).is_err());
    }

    #[test]
    fn unknown_or_raw_mutation_handlers_fail_toml_registration() {
        for forbidden in ["set_block", "conjure", "transmute", "teleport", "script"] {
            let source = format!(
                r#"[[working]]
id = "bad"
handler = "{forbidden}"
mode = "wand"
focus = "base:echo"
charge = 10
dross = 1
safe_throughput = 10
range = 1
target = ["self"]
interruption = "refund_with_dross"
disposition = "split"
description = "Forbidden fixture."
"#
            );
            assert!(
                toml::from_str::<WorkingsFile>(&source).is_err(),
                "{forbidden}"
            );
        }
    }

    #[test]
    fn strain_is_deterministic_monotonic_and_visibly_banded() {
        let calm = deterministic_strain(StrainInputs {
            safe_throughput: 10,
            local_capacity_permille: 1_000,
            ..StrainInputs::default()
        })
        .unwrap();
        let forced = deterministic_strain(StrainInputs {
            resonance_mismatch_permille: 500,
            component_instability_permille: 500,
            throughput: 20,
            safe_throughput: 10,
            local_capacity_permille: 200,
            apparatus_damage_permille: 500,
            contamination_permille: 500,
            forced_overdraw_units: 100,
            ..StrainInputs::default()
        })
        .unwrap();
        assert!(forced.strain > calm.strain);
        assert!(forced.extra_dross > calm.extra_dross);
        assert_eq!(forced.warning_band, 3);
        assert!(forced.refuses);
        assert_eq!(
            forced,
            deterministic_strain(StrainInputs {
                resonance_mismatch_permille: 500,
                component_instability_permille: 500,
                throughput: 20,
                safe_throughput: 10,
                local_capacity_permille: 200,
                apparatus_damage_permille: 500,
                contamination_permille: 500,
                forced_overdraw_units: 100,
                ..StrainInputs::default()
            })
            .unwrap()
        );
    }

    fn census_transaction(id: u64, definition: &WorkingDef, ritual: bool) -> WorkingTransaction {
        let source = legacy_working_source();
        let reserved = Current::single(definition.focus.clone(), 10);
        let return_current = Current::single(definition.focus.clone(), 9);
        let dross_current = Current::single(definition.focus.clone(), 1);
        let (targets, physical_debits, effect, apparatus) = if ritual {
            (
                vec![
                    WorkingTargetSnapshot::Item {
                        stable_id: id * 2,
                        item_name: "base:charge_vessel".into(),
                        durability: 1,
                        age_ticks: 0,
                        version: 1,
                    },
                    WorkingTargetSnapshot::Item {
                        stable_id: id * 2 + 1,
                        item_name: "base:charge_vessel".into(),
                        durability: 1,
                        age_ticks: 0,
                        version: 1,
                    },
                ],
                vec![
                    PhysicalDebit {
                        kind: PhysicalDebitKind::Item,
                        source: "fixture_source".into(),
                        content_id: "base:charge_vessel".into(),
                        units: 1,
                        expected_version: 1,
                    },
                    PhysicalDebit {
                        kind: PhysicalDebitKind::Item,
                        source: "fixture_destination".into(),
                        content_id: "base:charge_vessel".into(),
                        units: 1,
                        expected_version: 1,
                    },
                ],
                WorkingEffect::TransferCurrent {
                    from: ArcaneOwner::Item(id * 2),
                    to: ArcaneOwner::Item(id * 2 + 1),
                    current: Current::single(definition.focus.clone(), 1),
                },
                WorkingApparatus::Ritual {
                    controller: source,
                    expected_revision: 1,
                },
            )
        } else {
            (
                vec![WorkingTargetSnapshot::Block {
                    pos: source,
                    block_name: "base:air".into(),
                    metadata: 0,
                    version: id,
                }],
                Vec::new(),
                WorkingEffect::Observe {
                    origin: source,
                    expires_tick: 0,
                },
                WorkingApparatus::Wand {
                    instance_id: id,
                    expected_revision: 1,
                },
            )
        };
        WorkingTransaction {
            id,
            definition: definition.clone(),
            actor: id.to_le_bytes().repeat(2).try_into().unwrap(),
            actor_label: format!("census-{id}"),
            source,
            path: vec![source],
            apparatus,
            targets,
            current_debits: vec![CurrentDebit {
                owner: ArcaneOwner::Item(1_000_000 + id),
                expected_version: 1,
                current: reserved.clone(),
            }],
            reserved_current: reserved,
            physical_debits,
            effect,
            return_current,
            dross_current,
            phase: WorkingPhase::Charging,
            started_tick: 0,
            due_tick: if ritual { 20 } else { 0 },
            interruption: definition.interruption,
            strain: StrainOutcome::default(),
            forced: false,
            trace: "bounded maximum-active performance census".into(),
            completion_nonce: 1,
        }
    }

    #[test]
    fn maximum_legal_active_workings_and_rituals_stay_bounded() {
        let mut trace_raw = raw(WorkingHandler::Trace);
        trace_raw.id = "performance_trace".into();
        let trace = WorkingDef::from_raw("fixture", trace_raw).unwrap();
        let mut circle_raw = raw(WorkingHandler::TransferCircle);
        circle_raw.id = "performance_circle".into();
        let circle = WorkingDef::from_raw("fixture", circle_raw).unwrap();
        let mut manifests = BTreeMap::new();
        manifests.insert(trace.id.clone(), trace.clone());
        manifests.insert(circle.id.clone(), circle.clone());
        let mut active = BTreeMap::new();
        for id in 1..=MAX_ACTIVE_RITUALS as u64 {
            active.insert(id, census_transaction(id, &circle, true));
        }
        for id in MAX_ACTIVE_RITUALS as u64 + 1..=MAX_ACTIVE_WORKINGS as u64 {
            active.insert(id, census_transaction(id, &trace, false));
        }
        let mut state = WorkingsState {
            schema_version: WORKINGS_SCHEMA_VERSION,
            content_hash: 1,
            manifests,
            active,
            history: VecDeque::new(),
            path: std::env::temp_dir().join("workings-maximum-census.wfw"),
        };

        let started = std::time::Instant::now();
        state.validate().unwrap();
        let bytes = state.encode().unwrap();
        let elapsed = started.elapsed();
        assert!(bytes.len() as u64 <= MAX_WORKINGS_FILE_BYTES);
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "maximum census validation/serialization took {elapsed:?}"
        );

        let mut extra = census_transaction(MAX_ACTIVE_WORKINGS as u64 + 1, &trace, false);
        assert!(state.start(extra.clone()).is_err());
        let replaced = MAX_ACTIVE_RITUALS as u64 + 1;
        extra = census_transaction(replaced, &circle, true);
        state.active.insert(replaced, extra);
        assert!(
            state.validate().is_err(),
            "the ritual census must be enforced"
        );
    }
}
