//! Conserved environmental dross, warning bands, manifestations, and evidence.
//!
//! Exact Current remains owned by the arcane ledger.  Dense environmental
//! custody is the `Geography` subledger: `ArcaneDynamicCell::dross` is its
//! soil/sediment compartment, while [`DrossCellState`] holds the airborne and
//! waterborne compartments.  Scar sites exported to `ArcaneOwner::Scar` keep
//! only identity and presentation here; their charge is never copied into the
//! site record.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::planet::BlockPos;
use crate::planet::Direction4;
use crate::planet_atlas::{
    AtlasPos, PlanetAtlas, RunoffTransport, seasonal_scalar, seasonal_vector,
};

pub const DROSS_STATE_VERSION: u32 = 1;
pub const MAX_DROSS_EVENTS: usize = 4_096;
pub const MAX_PROVENANCE_CELLS: usize = 32_768;
pub const MAX_CONTAINED_PROVENANCE: usize = 32_768;
pub const MAX_DROSS_RUNOFF_ROUTES: usize = 2_000_000;
pub const MAX_PROVENANCE_ENTRIES: usize = 4;
pub const MAX_SCAR_SITES: usize = 16_384;
pub const MAX_MATERIALIZED_SCARS: usize = 16_384;
pub const MAX_ACTIVE_SCARS_PER_REGION: usize = 8;
pub const MAX_SOURCE_LABEL_BYTES: usize = 96;
pub const MAX_DROSS_PROCESS_COUNTERS: usize = 256;
pub const DROSS_WARNING_STEPS: u64 = 3;
pub const BREACH_FORECAST_STEPS: u64 = 6;
pub const BREACH_RECOVERY_STEPS: u64 = 12;
pub const PROVENANCE_HOP_CONFIDENCE_COST: u16 = 35;
pub const PROVENANCE_MIN_IDENTIFIABLE_CONFIDENCE: u16 = 200;
#[cfg(test)]
pub const DROSS_MAX_PROJECTED_SAVE_BYTES: u64 = 96 * 1024 * 1024;
#[cfg(test)]
pub const DROSS_MAX_DENSE_ROUTE_RESIDENT_BYTES: u64 = 64 * 1024 * 1024;
#[cfg(test)]
pub const DROSS_MAX_CLIENT_CUE_BYTES: usize = 256;
pub const DROSS_SERVER_SLICE_CELLS: usize = 4_096;

pub(crate) fn add_bounded_process_units(
    counters: &mut BTreeMap<String, u64>,
    process: &str,
    units: u64,
) {
    if units == 0 {
        return;
    }
    let mut process = process.to_owned();
    process.truncate(MAX_SOURCE_LABEL_BYTES);
    if process.is_empty() {
        process = "unknown".into();
    }
    const COMPACTED: &str = "unknown/compacted";
    if !counters.contains_key(&process) && counters.len() >= MAX_DROSS_PROCESS_COUNTERS {
        if !counters.contains_key(COMPACTED)
            && let Some(evicted) = counters.keys().next_back().cloned()
        {
            let evicted_units = counters.remove(&evicted).unwrap_or_default();
            counters.insert(COMPACTED.into(), evicted_units);
        }
        process = COMPACTED.into();
    }
    let previous = counters.get(&process).copied().unwrap_or_default();
    counters.insert(process, previous.saturating_add(units));
}

/// Relative burden boundaries from `docs/magic-dross-plan.md`, expressed in
/// integer permille. The upper interval is deliberately open ended.
pub const TRACE_BURDEN: u32 = 100;
pub const STRAINED_BURDEN: u32 = 250;
pub const SEEP_BURDEN: u32 = 500;
pub const SCAR_BURDEN: u32 = 750;
pub const BREACH_BURDEN: u32 = 1_000;
const HYSTERESIS_PERMILLE: u32 = 40;

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum DrossCarrier {
    Air,
    Water,
    #[default]
    Soil,
    Organism,
    Contained,
    Apparatus,
    Scar,
}

impl DrossCarrier {
    pub const ALL: [Self; 7] = [
        Self::Air,
        Self::Water,
        Self::Soil,
        Self::Organism,
        Self::Contained,
        Self::Apparatus,
        Self::Scar,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Air => "airborne",
            Self::Water => "waterborne",
            Self::Soil => "soil/sediment",
            Self::Organism => "organism",
            Self::Contained => "contained",
            Self::Apparatus => "apparatus",
            Self::Scar => "manifested scar",
        }
    }
}

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum DrossBand {
    #[default]
    Clear,
    Trace,
    Strained,
    Seep,
    Scar,
    BreachRisk,
}

impl DrossBand {
    pub const ALL: [Self; 6] = [
        Self::Clear,
        Self::Trace,
        Self::Strained,
        Self::Seep,
        Self::Scar,
        Self::BreachRisk,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Trace => "trace",
            Self::Strained => "strained",
            Self::Seep => "seep",
            Self::Scar => "scar",
            Self::BreachRisk => "breach risk",
        }
    }

    pub const fn damaging(self) -> bool {
        matches!(self, Self::Seep | Self::Scar | Self::BreachRisk)
    }

    pub const fn ordinal(self) -> u8 {
        match self {
            Self::Clear => 0,
            Self::Trace => 1,
            Self::Strained => 2,
            Self::Seep => 3,
            Self::Scar => 4,
            Self::BreachRisk => 5,
        }
    }

    /// Environmental instability contributed to a real magical operation.
    /// Trace remains observational only; from Strained onward the same work
    /// measurably produces more dross instead of merely changing a UI meter.
    pub const fn stability_penalty_permille(self) -> u16 {
        match self {
            Self::Clear | Self::Trace => 0,
            Self::Strained => 80,
            Self::Seep => 160,
            Self::Scar => 280,
            Self::BreachRisk => 400,
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Clear => Self::Trace,
            Self::Trace => Self::Strained,
            Self::Strained => Self::Seep,
            Self::Seep => Self::Scar,
            Self::Scar | Self::BreachRisk => Self::BreachRisk,
        }
    }

    pub const fn previous(self) -> Self {
        match self {
            Self::Clear | Self::Trace => Self::Clear,
            Self::Strained => Self::Trace,
            Self::Seep => Self::Strained,
            Self::Scar => Self::Seep,
            Self::BreachRisk => Self::Scar,
        }
    }

    pub const fn entry_threshold(self) -> u32 {
        match self {
            Self::Clear => 0,
            Self::Trace => TRACE_BURDEN,
            Self::Strained => STRAINED_BURDEN,
            Self::Seep => SEEP_BURDEN,
            Self::Scar => SCAR_BURDEN,
            Self::BreachRisk => BREACH_BURDEN,
        }
    }

    pub fn from_burden(burden_permille: u32) -> Self {
        match burden_permille {
            0..TRACE_BURDEN => Self::Clear,
            TRACE_BURDEN..STRAINED_BURDEN => Self::Trace,
            STRAINED_BURDEN..SEEP_BURDEN => Self::Strained,
            SEEP_BURDEN..SCAR_BURDEN => Self::Seep,
            SCAR_BURDEN..BREACH_BURDEN => Self::Scar,
            _ => Self::BreachRisk,
        }
    }
}

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ScarKind {
    WetFilm,
    DryNeedles,
    ForestThreads,
    FrostCraze,
    CaveEcho,
    IndustrialScale,
    #[default]
    BrokenSymmetry,
}

impl ScarKind {
    pub const ALL: [Self; 7] = [
        Self::WetFilm,
        Self::DryNeedles,
        Self::ForestThreads,
        Self::FrostCraze,
        Self::CaveEcho,
        Self::IndustrialScale,
        Self::BrokenSymmetry,
    ];

    pub const fn block_id(self) -> &'static str {
        match self {
            Self::WetFilm => "base:scar_wet_film",
            Self::DryNeedles => "base:scar_dry_needles",
            Self::ForestThreads => "base:scar_forest_threads",
            Self::FrostCraze => "base:scar_frost_craze",
            Self::CaveEcho => "base:scar_cave_echo",
            Self::IndustrialScale => "base:scar_industrial_scale",
            Self::BrokenSymmetry => "base:scar_broken_symmetry",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::WetFilm => "oily refracting film",
            Self::DryNeedles => "branching glass-salt needles",
            Self::ForestThreads => "bleached thread mat",
            Self::FrostCraze => "singing frost craze",
            Self::CaveEcho => "false-shadow crust",
            Self::IndustrialScale => "discordant industrial scale",
            Self::BrokenSymmetry => "broken-symmetry filament",
        }
    }
}

/// Closed native placement vocabulary available to data packs.  A scar
/// definition chooses presentation and eligibility; it never receives a raw
/// block-mutation callback.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScarHandler {
    SurfaceOverlay,
    WaterMarginFilm,
    FilamentGrowth,
    MineralCrust,
}

/// Closed, bounded body-effect vocabulary for a manifested scar.  These are
/// modifiers of the normal band-driven exposure path, not permanent stats or
/// arbitrary scripted status effects.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScarStatusHandler {
    RecoveryDrag,
    PerceptionWarp,
    StaminaDrag,
    WorkingInstability,
}

/// Closed local activity vocabulary.  Goal 8 uses these values to select
/// deterministic breach presentation; content cannot spawn arbitrary actors,
/// teleport, or issue world edits.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScarActivityHandler {
    Shear,
    AnimatedCastoff,
    DustWake,
    ArcaneSquall,
}

/// Qualified, validated shell around one native scar lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DrossScarDef {
    pub content_id: String,
    pub provider: String,
    pub block: crate::registry::BlockId,
    pub kind: ScarKind,
    pub handler: ScarHandler,
    pub carriers: Vec<DrossCarrier>,
    pub min_band: DrossBand,
    pub status: Option<ScarStatusHandler>,
    pub activity: Option<ScarActivityHandler>,
    pub max_sites_per_region: u8,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct DrossCellState {
    /// Exact dense airborne dross by base resonance. Soil/sediment remains in
    /// the corresponding `ArcaneDynamicCell::dross` array.
    pub airborne: [u16; 6],
    /// Exact dense dissolved/suspended dross by base resonance.
    pub waterborne: [u16; 6],
    pub band: DrossBand,
    /// Step at which the currently displayed band became stable.
    pub band_since_step: u64,
    /// First breach-risk warning. Zero means no live forecast.
    pub breach_forecast_step: u64,
    pub last_breach_step: u64,
    /// Fixed-point remainders for air, water, sorption, and reordering.
    pub remainders: [u32; 4],
}

impl DrossCellState {
    pub fn airborne_total(self) -> u64 {
        self.airborne.into_iter().map(u64::from).sum()
    }

    pub fn waterborne_total(self) -> u64 {
        self.waterborne.into_iter().map(u64::from).sum()
    }

    pub fn mobile_total(self) -> u64 {
        self.airborne_total()
            .saturating_add(self.waterborne_total())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DrossContribution {
    pub actor: Option<[u8; 16]>,
    pub installation_id: Option<u64>,
    pub source_class: String,
    pub units: u64,
    pub first_step: u64,
    pub last_step: u64,
    pub confidence_permille: u16,
}

impl DrossContribution {
    fn same_source(&self, other: &Self) -> bool {
        self.actor == other.actor
            && self.installation_id == other.installation_id
            && self.source_class == other.source_class
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct DrossProvenance {
    pub entries: Vec<DrossContribution>,
    pub unknown_units: u64,
}

impl DrossProvenance {
    pub fn total_units(&self) -> u64 {
        self.entries.iter().fold(self.unknown_units, |sum, entry| {
            sum.saturating_add(entry.units)
        })
    }

    pub fn known_units(&self) -> u64 {
        self.entries.iter().map(|entry| entry.units).sum()
    }

    /// Player-facing forensic evidence. This intentionally reveals neither
    /// actor nor installation ids and never promises certainty after mixing.
    pub fn qualitative_signature(&self) -> Option<String> {
        let total = self.total_units();
        if total == 0 {
            return None;
        }
        let Some(top) = self.entries.first() else {
            return Some("inconclusive after mixing or transport".into());
        };
        let share_permille = top.units.saturating_mul(1_000) / total;
        let source = if top.source_class.is_empty() {
            "recorded process"
        } else {
            top.source_class.as_str()
        };
        Some(if share_permille >= 700 && top.confidence_permille >= 700 {
            format!("likely dominated by one {source} signature")
        } else if share_permille >= 350 && top.confidence_permille >= 400 {
            format!("consistent with {source} among mixed signatures")
        } else {
            "inconclusive after mixing or transport".into()
        })
    }

    pub fn add_known(&mut self, mut contribution: DrossContribution) {
        contribution.source_class.truncate(MAX_SOURCE_LABEL_BYTES);
        if contribution.units == 0 {
            return;
        }
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|entry| entry.same_source(&contribution))
        {
            let old_units = existing.units;
            let total = old_units.saturating_add(contribution.units);
            let weighted = u128::from(existing.confidence_permille)
                .saturating_mul(u128::from(old_units))
                .saturating_add(
                    u128::from(contribution.confidence_permille)
                        .saturating_mul(u128::from(contribution.units)),
                );
            existing.units = total;
            existing.first_step = existing.first_step.min(contribution.first_step);
            existing.last_step = existing.last_step.max(contribution.last_step);
            existing.confidence_permille = if total == 0 {
                0
            } else {
                u16::try_from(weighted / u128::from(total))
                    .unwrap_or(1_000)
                    .min(1_000)
            };
        } else {
            contribution.confidence_permille = contribution.confidence_permille.min(1_000);
            self.entries.push(contribution);
        }
        self.compact();
    }

    pub fn add_unknown(&mut self, units: u64) {
        self.unknown_units = self.unknown_units.saturating_add(units);
    }

    fn compact(&mut self) {
        self.entries.sort_by(|a, b| {
            b.units
                .cmp(&a.units)
                .then_with(|| b.confidence_permille.cmp(&a.confidence_permille))
                .then_with(|| a.source_class.cmp(&b.source_class))
        });
        while self.entries.len() > MAX_PROVENANCE_ENTRIES {
            if let Some(removed) = self.entries.pop() {
                self.unknown_units = self.unknown_units.saturating_add(removed.units);
            }
        }
    }

    /// Move the same proportional share as its physical carrier and reduce
    /// confidence one hop. Fixed integer allocation is deterministic and the
    /// final entry receives the remainder, so evidence units close exactly.
    pub fn take_for_transport(&mut self, units: u64) -> Self {
        let units = units.min(self.total_units());
        if units == 0 {
            return Self::default();
        }
        let total_before = self.total_units();
        if units == total_before {
            let mut all = std::mem::take(self);
            all.degrade_one_hop();
            return all;
        }
        let mut moved = Self::default();
        let mut remaining = units;
        let original = self.entries.clone();
        for (index, old) in original.iter().enumerate() {
            if remaining == 0 {
                break;
            }
            let share = if index + 1 == original.len() && self.unknown_units == 0 {
                remaining.min(old.units)
            } else {
                (u128::from(old.units) * u128::from(units) / u128::from(total_before))
                    .try_into()
                    .unwrap_or(u64::MAX)
                    .min(old.units)
                    .min(remaining)
            };
            if share == 0 {
                continue;
            }
            if let Some(entry) = self.entries.iter_mut().find(|entry| entry.same_source(old)) {
                entry.units -= share;
            }
            let mut part = old.clone();
            part.units = share;
            moved.entries.push(part);
            remaining -= share;
        }
        self.entries.retain(|entry| entry.units != 0);
        let unknown_share = remaining.min(self.unknown_units);
        self.unknown_units -= unknown_share;
        moved.unknown_units = unknown_share;
        remaining -= unknown_share;
        if remaining != 0 {
            // Rounding can leave at most a bounded tail. Take it from the
            // largest remaining known contribution and retain its identity.
            if let Some(source) = self.entries.first_mut() {
                let tail = remaining.min(source.units);
                let mut part = source.clone();
                part.units = tail;
                source.units -= tail;
                moved.entries.push(part);
                remaining -= tail;
            }
        }
        if remaining != 0 {
            let tail = remaining.min(self.unknown_units);
            self.unknown_units -= tail;
            moved.unknown_units += tail;
        }
        self.entries.retain(|entry| entry.units != 0);
        moved.degrade_one_hop();
        moved.compact();
        moved
    }

    pub fn merge(&mut self, other: Self) {
        self.unknown_units = self.unknown_units.saturating_add(other.unknown_units);
        for entry in other.entries {
            self.add_known(entry);
        }
    }

    pub fn degrade_one_hop(&mut self) {
        let mut still_known = Vec::with_capacity(self.entries.len());
        for mut entry in self.entries.drain(..) {
            entry.confidence_permille = entry
                .confidence_permille
                .saturating_sub(PROVENANCE_HOP_CONFIDENCE_COST);
            if entry.confidence_permille < PROVENANCE_MIN_IDENTIFIABLE_CONFIDENCE {
                self.unknown_units = self.unknown_units.saturating_add(entry.units);
            } else {
                still_known.push(entry);
            }
        }
        self.entries = still_known;
        self.compact();
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScarSite {
    pub id: u64,
    pub region: AtlasPos,
    pub kind: ScarKind,
    /// Stable registry identity.  Empty means a pre-schema site and falls
    /// back to the base definition for `kind`; missing mod providers likewise
    /// retain this identity while materializing the safe base fallback.
    #[serde(default)]
    pub content_id: String,
    /// Stable non-overlapping presentation slot inside the region's canonical
    /// chunk. This lets bounded mod variants coexist without load-order
    /// placement races or two Scar owners claiming one block.
    #[serde(default)]
    pub site_slot: u8,
    pub created_step: u64,
    pub last_changed_step: u64,
    pub breach_count: u16,
    pub materialized_at: Option<BlockPos>,
    #[serde(default)]
    pub resolved_step: Option<u64>,
    pub actor_hint: Option<[u8; 16]>,
    pub installation_hint: Option<u64>,
    /// Bounded evidentiary mixture for the exact charge held by
    /// `ArcaneOwner::Scar(id)`. This is attribution, not a copied reservoir.
    #[serde(default)]
    pub provenance: DrossProvenance,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DrossEventKind {
    BandChanged {
        from: DrossBand,
        to: DrossBand,
    },
    BreachForecast,
    Breach,
    ScarActivity {
        scar_id: u64,
        activity: ScarActivityHandler,
    },
    ScarManifested {
        scar_id: u64,
        kind: ScarKind,
    },
    ScarExcavated {
        scar_id: u64,
    },
    ContainmentFailed {
        carrier: DrossCarrier,
    },
    Reordered {
        units: u64,
        heart_aided: bool,
    },
}

/// Interest-managed public consequence of the warning ladder. This contains
/// presentation categories only: no exact units, actor, installation, or
/// provenance crosses the ordinary client boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DrossCue {
    pub region: AtlasPos,
    pub kind: DrossCueKind,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DrossCueKind {
    BreachForecast,
    Breach {
        activity: Option<ScarActivityHandler>,
    },
}

impl DrossCue {
    pub const fn accessible_text(self) -> &'static str {
        match self.kind {
            DrossCueKind::BreachForecast => {
                "DROSS BREACH FORECAST — repeating shear; evacuate or establish emergency containment"
            }
            DrossCueKind::Breach {
                activity: Some(ScarActivityHandler::Shear),
            } => "DROSS BREACH — field shear redistributed the existing burden",
            DrossCueKind::Breach {
                activity: Some(ScarActivityHandler::AnimatedCastoff),
            } => "DROSS BREACH — animated castoff carried the existing burden outward",
            DrossCueKind::Breach {
                activity: Some(ScarActivityHandler::DustWake),
            } => "DROSS BREACH — a dust wake carried the existing burden outward",
            DrossCueKind::Breach {
                activity: Some(ScarActivityHandler::ArcaneSquall),
            } => "DROSS BREACH — an arcane squall redistributed the existing burden",
            DrossCueKind::Breach { activity: None } => {
                "DROSS BREACH — the field redistributed its existing burden"
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DrossEvent {
    pub sequence: u64,
    pub step: u64,
    pub region: AtlasPos,
    pub kind: DrossEventKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DrossPlanetState {
    pub version: u32,
    pub cells: Vec<DrossCellState>,
    pub scars: BTreeMap<u64, ScarSite>,
    pub materialized: BTreeMap<BlockPos, u64>,
    pub provenance: BTreeMap<AtlasPos, DrossProvenance>,
    /// Bounded evidence for contained item owners. Exact charge remains only
    /// in `ArcaneOwner::ItemDross(id)`.
    pub contained_provenance: BTreeMap<u64, DrossProvenance>,
    pub events: VecDeque<DrossEvent>,
    pub event_sequence: u64,
    pub completed_steps: u64,
    pub reordered_units: u64,
    pub generated_units: u64,
    pub imported_units: u64,
    pub transported_units: u64,
    pub breach_count: u64,
    /// Net Current imported across the Geography boundary after cancelling
    /// outstanding exports, by base resonance. Geography began with a fixed
    /// genesis allotment but may later receive Deep-origin waste; this keeps
    /// that legitimate inbound transfer distinct from unexplained creation.
    #[serde(default)]
    pub external_imported: [u64; 6],
    #[serde(default)]
    pub generated_by_process: BTreeMap<String, u64>,
    #[serde(default)]
    pub reordered_by_process: BTreeMap<String, u64>,
    /// The exact accepted water routes for an in-progress dross hour. The
    /// sliced delta map is intentionally recomputed after reload, but its
    /// physical inputs must survive or a save boundary could substitute the
    /// following weather hour's river movement.
    #[serde(default)]
    pub pending_transport_hour: u64,
    #[serde(default)]
    pub pending_runoff_routes: Vec<RunoffTransport>,
}

#[derive(Clone, Copy)]
pub struct DrossConditions<'a> {
    pub runoff_routes: &'a [RunoffTransport],
    pub living_hearts: &'a BTreeSet<u16>,
    pub long_winter: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DrossAdvance {
    pub processed_cells: usize,
    pub completed_hour: Option<u64>,
    pub reordered_units: u64,
    pub transported_units: u64,
    pub band_changes: usize,
    pub breaches: usize,
    pub cues: Vec<DrossCue>,
}

#[derive(Clone, Debug)]
pub struct DrossAudit {
    pub carrier_totals: BTreeMap<DrossCarrier, u64>,
    pub resonance_totals: BTreeMap<String, u64>,
    pub country_totals: BTreeMap<u16, u64>,
    pub watershed_totals: BTreeMap<u32, u64>,
    pub generated_by_process: BTreeMap<String, u64>,
    pub generated_by_installation: BTreeMap<u64, u64>,
    pub reordered_by_process: BTreeMap<String, u64>,
    pub band_cells: BTreeMap<DrossBand, u64>,
    pub band_area: BTreeMap<DrossBand, f64>,
    pub population_exposure: BTreeMap<DrossBand, u64>,
    pub active_scars: usize,
    pub resolved_scars: usize,
    pub materialized_scars: usize,
    pub contained_owners: usize,
    pub contained_units: u64,
    pub known_provenance_units: u64,
    pub unknown_provenance_units: u64,
    pub current_total: u64,
    pub unexplained_arcane_delta: i128,
    pub completed_hours: u64,
    pub projected_environmental_recovery_hours: Option<u64>,
}

impl DrossAudit {
    pub fn is_balanced(&self) -> bool {
        self.unexplained_arcane_delta == 0
    }

    pub fn render(&self) -> String {
        let mut out = format!(
            "Dross audit\nCurrent dross: {}\nKnown/unknown provenance: {} / {}\nCompleted transport hours: {}\nUnexplained arcane delta: {}\nScar sites: {} active, {} resolved, {} materialized\nContained: {} units in {} owners\nProjected environmental recovery: {}\nCarriers:\n",
            self.current_total,
            self.known_provenance_units,
            self.unknown_provenance_units,
            self.completed_hours,
            self.unexplained_arcane_delta,
            self.active_scars,
            self.resolved_scars,
            self.materialized_scars,
            self.contained_units,
            self.contained_owners,
            self.projected_environmental_recovery_hours.map_or_else(
                || "no observed bounded rate (contained burden requires treatment)".into(),
                |hours| format!("about {hours} hours at the observed rate"),
            ),
        );
        for carrier in DrossCarrier::ALL {
            out.push_str(&format!(
                "  {:<20} {}\n",
                carrier.label(),
                self.carrier_totals
                    .get(&carrier)
                    .copied()
                    .unwrap_or_default()
            ));
        }
        out.push_str("Resonance:\n");
        for (name, units) in &self.resonance_totals {
            out.push_str(&format!("  {name:<20} {units}\n"));
        }
        out.push_str("Burden bands (cells / physical area):\n");
        for band in DrossBand::ALL {
            out.push_str(&format!(
                "  {:<12} {} / {:.3}\n",
                band.label(),
                self.band_cells.get(&band).copied().unwrap_or_default(),
                self.band_area.get(&band).copied().unwrap_or_default(),
            ));
        }
        let damaging_area = DrossBand::ALL
            .into_iter()
            .filter(|band| band.damaging())
            .map(|band| self.band_area.get(&band).copied().unwrap_or_default())
            .sum::<f64>();
        out.push_str(&format!(
            "Damaging-band physical area: {damaging_area:.3}\n"
        ));
        out.push_str("Saved population exposure:\n");
        for band in DrossBand::ALL {
            out.push_str(&format!(
                "  {:<12} {}\n",
                band.label(),
                self.population_exposure
                    .get(&band)
                    .copied()
                    .unwrap_or_default()
            ));
        }
        out.push_str("Generation by attributed process:\n");
        for (process, units) in &self.generated_by_process {
            let per_thousand_hours = u128::from(*units)
                .saturating_mul(1_000)
                .checked_div(u128::from(self.completed_hours.max(1)))
                .unwrap_or_default();
            out.push_str(&format!(
                "  {process:<32} {units} ({per_thousand_hours}/1000h)\n"
            ));
        }
        out.push_str("Reordering by process:\n");
        for (process, units) in &self.reordered_by_process {
            let per_thousand_hours = u128::from(*units)
                .saturating_mul(1_000)
                .checked_div(u128::from(self.completed_hours.max(1)))
                .unwrap_or_default();
            out.push_str(&format!(
                "  {process:<32} {units} ({per_thousand_hours}/1000h)\n"
            ));
        }
        out.push_str("Top generating installations:\n");
        let mut installations = self
            .generated_by_installation
            .iter()
            .map(|(installation, units)| (*units, *installation))
            .collect::<Vec<_>>();
        installations.sort_by(|left, right| right.cmp(left));
        for (units, installation) in installations.into_iter().take(16) {
            out.push_str(&format!("  {installation:<20} {units}\n"));
        }
        out.push_str("Countries with burden:\n");
        for (country, units) in &self.country_totals {
            out.push_str(&format!("  {country:<8} {units}\n"));
        }
        out.push_str("Watersheds with burden:\n");
        for (watershed, units) in &self.watershed_totals {
            out.push_str(&format!("  {watershed:<8} {units}\n"));
        }
        out
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DenseDrossCaptureBefore {
    cell: crate::arcane_geography::ArcaneDynamicCell,
    carrier: DrossCellState,
    provenance: Option<DrossProvenance>,
    contained: Option<DrossProvenance>,
    exported: [u64; 6],
    external_imported: [u64; 6],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct DrossDelta {
    air: [i32; 6],
    water: [i32; 6],
    soil: [i32; 6],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DrossTransportPass {
    target_hour: u64,
    cursor: usize,
    delta: BTreeMap<usize, DrossDelta>,
    movements: BTreeMap<(usize, usize), u64>,
    routes: BTreeMap<usize, RunoffTransport>,
}

impl Default for DrossPlanetState {
    fn default() -> Self {
        Self {
            version: DROSS_STATE_VERSION,
            cells: Vec::new(),
            scars: BTreeMap::new(),
            materialized: BTreeMap::new(),
            provenance: BTreeMap::new(),
            contained_provenance: BTreeMap::new(),
            events: VecDeque::new(),
            event_sequence: 0,
            completed_steps: 0,
            reordered_units: 0,
            generated_units: 0,
            imported_units: 0,
            transported_units: 0,
            breach_count: 0,
            external_imported: [0; 6],
            generated_by_process: BTreeMap::new(),
            reordered_by_process: BTreeMap::new(),
            pending_transport_hour: 0,
            pending_runoff_routes: Vec::new(),
        }
    }
}

impl DrossPlanetState {
    pub fn initialized(cell_count: usize) -> Self {
        Self {
            cells: vec![DrossCellState::default(); cell_count],
            ..Self::default()
        }
    }

    pub fn ensure_cells(&mut self, cell_count: usize) {
        if self.cells.is_empty() {
            self.cells = vec![DrossCellState::default(); cell_count];
        }
    }

    pub fn has_transport_work(
        &self,
        cells: &[crate::arcane_geography::ArcaneDynamicCell],
        ecology: &crate::arcane_ecology::ArcaneEcologyState,
    ) -> bool {
        self.cells.iter().any(|cell| cell.mobile_total() != 0)
            || cells.iter().any(|cell| cell.dross_total() != 0)
            || ecology.sites.iter().any(|site| site.dross_total() != 0)
            || self.scars.values().any(|site| site.resolved_step.is_none())
    }

    pub fn record(&mut self, step: u64, region: AtlasPos, kind: DrossEventKind) {
        self.event_sequence = self.event_sequence.saturating_add(1);
        self.events.push_back(DrossEvent {
            sequence: self.event_sequence,
            step,
            region,
            kind,
        });
        while self.events.len() > MAX_DROSS_EVENTS {
            self.events.pop_front();
        }
    }

    fn add_process_units(counters: &mut BTreeMap<String, u64>, process: &str, units: u64) {
        add_bounded_process_units(counters, process, units);
    }

    pub fn add_provenance(&mut self, region: AtlasPos, contribution: DrossContribution) {
        if !self.provenance.contains_key(&region) && self.provenance.len() >= MAX_PROVENANCE_CELLS {
            return;
        }
        self.provenance
            .entry(region)
            .or_default()
            .add_known(contribution);
    }

    pub fn add_unknown_provenance(&mut self, region: AtlasPos, units: u64) {
        if units == 0 {
            return;
        }
        if !self.provenance.contains_key(&region) && self.provenance.len() >= MAX_PROVENANCE_CELLS {
            return;
        }
        self.provenance
            .entry(region)
            .or_default()
            .add_unknown(units);
    }

    pub fn move_provenance(&mut self, from: AtlasPos, to: AtlasPos, units: u64) {
        if from == to || units == 0 {
            return;
        }
        let moved = self
            .provenance
            .get_mut(&from)
            .map(|mixture| mixture.take_for_transport(units))
            .unwrap_or_default();
        if moved.total_units() == 0 {
            return;
        }
        if !self.provenance.contains_key(&to) && self.provenance.len() >= MAX_PROVENANCE_CELLS {
            return;
        }
        self.provenance.entry(to).or_default().merge(moved);
        if self
            .provenance
            .get(&from)
            .is_some_and(|mixture| mixture.total_units() == 0)
        {
            self.provenance.remove(&from);
        }
    }

    pub fn validate(&self, cell_count: usize, side: u16) -> Result<(), String> {
        if self.version != DROSS_STATE_VERSION
            || self.cells.len() != cell_count
            || self.scars.len() > MAX_SCAR_SITES
            || self.materialized.len() > MAX_MATERIALIZED_SCARS
            || self.provenance.len() > MAX_PROVENANCE_CELLS
            || self.contained_provenance.len() > MAX_CONTAINED_PROVENANCE
            || self.events.len() > MAX_DROSS_EVENTS
            || self.pending_runoff_routes.len() > MAX_DROSS_RUNOFF_ROUTES.min(cell_count)
            || self.generated_by_process.len() > MAX_DROSS_PROCESS_COUNTERS
            || self.reordered_by_process.len() > MAX_DROSS_PROCESS_COUNTERS
        {
            return Err("dross state exceeds its version or cardinality bounds".into());
        }
        if self
            .provenance
            .values()
            .chain(self.contained_provenance.values())
            .any(|mixture| mixture.entries.len() > MAX_PROVENANCE_ENTRIES)
        {
            return Err("dross provenance exceeds its bounded top-contributor mixture".into());
        }
        if self.scars.iter().any(|(id, site)| {
            *id != site.id
                || site.region.u >= side
                || site.region.v >= side
                || site.created_step > site.last_changed_step
                || site.content_id.len() > MAX_SOURCE_LABEL_BYTES
                || usize::from(site.site_slot) >= MAX_ACTIVE_SCARS_PER_REGION
                || (!site.content_id.is_empty()
                    && (!site.content_id.contains(':')
                        || !site.content_id.bytes().all(|byte| {
                            byte.is_ascii_lowercase()
                                || byte.is_ascii_digit()
                                || matches!(byte, b':' | b'_' | b'-')
                        })))
                || site.provenance.entries.len() > MAX_PROVENANCE_ENTRIES
                || (site.resolved_step.is_some() && site.materialized_at.is_some())
        }) {
            return Err("persistent dross scar identity or lifecycle is invalid".into());
        }
        if self.events.iter().any(|event| {
            event.sequence == 0
                || event.sequence > self.event_sequence
                || event.region.u >= side
                || event.region.v >= side
        }) || self
            .events
            .iter()
            .zip(self.events.iter().skip(1))
            .any(|(left, right)| left.sequence >= right.sequence)
        {
            return Err("dross event history is invalid or out of sequence".into());
        }
        if self
            .generated_by_process
            .keys()
            .chain(self.reordered_by_process.keys())
            .any(|process| process.is_empty() || process.len() > MAX_SOURCE_LABEL_BYTES)
        {
            return Err("dross process counters contain an invalid bounded identity".into());
        }
        if self.materialized.iter().any(|(pos, id)| {
            self.scars
                .get(id)
                .is_none_or(|site| site.materialized_at != Some(*pos))
        }) {
            return Err("materialized scar index disagrees with its persistent site".into());
        }
        let mut active_slots = BTreeSet::new();
        if self
            .scars
            .values()
            .filter(|site| site.resolved_step.is_none())
            .any(|site| !active_slots.insert((site.region, site.site_slot)))
        {
            return Err("active dross scars share one canonical regional slot".into());
        }
        if self.pending_transport_hour == 0 && !self.pending_runoff_routes.is_empty()
            || self.pending_runoff_routes.iter().any(|route| {
                route.from.u >= side
                    || route.from.v >= side
                    || route.to.u >= side
                    || route.to.v >= side
                    || route.from == route.to
                    || route.water_hu == 0
                    || route.source_water_before_hu == 0
                    || route.water_hu > route.source_water_before_hu
            })
        {
            return Err("dross pending transport checkpoint is invalid".into());
        }
        Ok(())
    }

    /// Make room for a new site by discarding only the oldest resolved
    /// presentation records. Exact Current and live provenance have already
    /// left those owners during excavation; the bounded event journal retains
    /// the social evidence. Active sites are never pruned.
    pub fn make_room_for_scar(&mut self) -> bool {
        if self.scars.len() < MAX_SCAR_SITES {
            return true;
        }
        let resolved = self
            .scars
            .values()
            .filter_map(|site| {
                site.resolved_step
                    .map(|step| (step, site.last_changed_step, site.id))
            })
            .min();
        if let Some((_, _, id)) = resolved {
            self.scars.remove(&id);
        }
        self.scars.len() < MAX_SCAR_SITES
    }
}

/// Documented burden function. Mobile carriers count fully; retained soil is
/// weighted by local retention. Low stability and low capacity make the same
/// physical load more consequential. The result is a relative permille ratio
/// against the cell's Current capacity and deliberately allows values >1000.
pub fn burden_permille(
    soil_units: u64,
    airborne_units: u64,
    waterborne_units: u64,
    capacity: u16,
    retention: u8,
    stability: u16,
) -> u32 {
    let soil_weight = 600u128 + u128::from(retention) * 2;
    let air_weight = 850u128;
    let water_weight = 1_100u128;
    let instability = 1_000u128 + u128::from(1_000u16.saturating_sub(stability)) / 2;
    let weighted = (u128::from(soil_units) * soil_weight
        + u128::from(airborne_units) * air_weight
        + u128::from(waterborne_units) * water_weight)
        .saturating_mul(instability);
    let denominator = u128::from(capacity.max(1)).saturating_mul(1_000);
    u32::try_from(weighted / denominator).unwrap_or(u32::MAX)
}

/// Hysteretic, rate-limited warning ladder. Even a very large release can
/// advance only one band after the previous warning has remained visible for
/// `DROSS_WARNING_STEPS`; falling burden likewise retreats one stage at a
/// time only below the band's lowered exit threshold.
pub fn advance_band(
    current: DrossBand,
    band_since_step: u64,
    now_step: u64,
    burden: u32,
) -> DrossBand {
    let target = DrossBand::from_burden(burden);
    if target > current {
        if now_step.saturating_sub(band_since_step) >= DROSS_WARNING_STEPS {
            current.next()
        } else {
            current
        }
    } else if target < current {
        let exit = current
            .entry_threshold()
            .saturating_sub(HYSTERESIS_PERMILLE);
        if burden < exit && now_step.saturating_sub(band_since_step) >= DROSS_WARNING_STEPS {
            current.previous()
        } else {
            current
        }
    } else {
        current
    }
}

fn carrier_array(
    state: &crate::arcane_geography::ArcaneGeography,
    index: usize,
    carrier: DrossCarrier,
) -> [u16; 6] {
    match carrier {
        DrossCarrier::Air => state.dynamic.dross_state.cells[index].airborne,
        DrossCarrier::Water => state.dynamic.dross_state.cells[index].waterborne,
        DrossCarrier::Soil => state.dynamic.cells[index].dross,
        _ => [0; 6],
    }
}

fn carrier_delta(delta: &mut DrossDelta, carrier: DrossCarrier) -> &mut [i32; 6] {
    match carrier {
        DrossCarrier::Air => &mut delta.air,
        DrossCarrier::Water => &mut delta.water,
        DrossCarrier::Soil => &mut delta.soil,
        _ => unreachable!("only dense environmental carriers enter the transport pass"),
    }
}

#[allow(clippy::too_many_arguments)]
fn add_carrier_transfer(
    geography: &crate::arcane_geography::ArcaneGeography,
    pass: &mut DrossTransportPass,
    from: usize,
    to: usize,
    from_carrier: DrossCarrier,
    to_carrier: DrossCarrier,
    slot: usize,
    requested: u16,
) -> u16 {
    if requested == 0 || from == to && from_carrier == to_carrier {
        return 0;
    }
    let from_value = i64::from(carrier_array(geography, from, from_carrier)[slot]);
    let to_value = i64::from(carrier_array(geography, to, to_carrier)[slot]);
    let from_pending = pass.delta.get(&from).map_or(0, |delta| {
        i64::from(match from_carrier {
            DrossCarrier::Air => delta.air[slot],
            DrossCarrier::Water => delta.water[slot],
            DrossCarrier::Soil => delta.soil[slot],
            _ => 0,
        })
    });
    let to_pending = pass.delta.get(&to).map_or(0, |delta| {
        i64::from(match to_carrier {
            DrossCarrier::Air => delta.air[slot],
            DrossCarrier::Water => delta.water[slot],
            DrossCarrier::Soil => delta.soil[slot],
            _ => 0,
        })
    });
    let available = (from_value + from_pending).max(0) as u64;
    let room = (i64::from(u16::MAX) - to_value - to_pending).max(0) as u64;
    let moved = u64::from(requested).min(available).min(room) as u16;
    if moved == 0 {
        return 0;
    }
    carrier_delta(pass.delta.entry(from).or_default(), from_carrier)[slot] -= i32::from(moved);
    carrier_delta(pass.delta.entry(to).or_default(), to_carrier)[slot] += i32::from(moved);
    if from != to {
        *pass.movements.entry((from, to)).or_default() += u64::from(moved);
    }
    moved
}

fn apply_signed_u16(value: u16, delta: i32) -> Result<u16, String> {
    u16::try_from(i64::from(value) + i64::from(delta))
        .map_err(|_| format!("dross carrier overflow: {value} + {delta}"))
}

fn wind_direction(wind: [f32; 2]) -> Direction4 {
    if wind[0].abs() >= wind[1].abs() {
        if wind[0] >= 0.0 {
            Direction4::East
        } else {
            Direction4::West
        }
    } else if wind[1] >= 0.0 {
        Direction4::North
    } else {
        Direction4::South
    }
}

fn mix_dross(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn base_current_from_array(values: [u16; 6]) -> crate::arcane::Current {
    crate::arcane::Current::from_parts(
        crate::arcane::BASE_RESONANCES
            .into_iter()
            .zip(values)
            .filter(|(_, units)| *units != 0)
            .map(|(resonance, units)| (resonance.to_string(), u64::from(units))),
    )
    .expect("base resonance array is valid Current")
}

fn take_array_units(values: &mut [u16; 6], requested: u64, rotation: usize) -> [u16; 6] {
    let mut moved = [0u16; 6];
    let mut remaining = requested;
    for offset in 0..6 {
        let slot = (rotation + offset) % 6;
        let take = u64::from(values[slot]).min(remaining) as u16;
        values[slot] -= take;
        moved[slot] = take;
        remaining -= u64::from(take);
        if remaining == 0 {
            break;
        }
    }
    moved
}

fn add_array_checked(target: &mut [u16; 6], moved: [u16; 6]) -> Result<(), String> {
    for slot in 0..6 {
        target[slot] = target[slot]
            .checked_add(moved[slot])
            .ok_or("dross carrier exceeds its bounded cell band")?;
    }
    Ok(())
}

impl crate::arcane_geography::ArcaneGeography {
    pub(crate) fn snapshot_dense_dross_capture(
        &self,
        pos: AtlasPos,
        contained_item: u64,
    ) -> DenseDrossCaptureBefore {
        let index = pos.index(self.manifest.side);
        DenseDrossCaptureBefore {
            cell: self.dynamic.cells[index],
            carrier: self.dynamic.dross_state.cells[index],
            provenance: self.dynamic.dross_state.provenance.get(&pos).cloned(),
            contained: self
                .dynamic
                .dross_state
                .contained_provenance
                .get(&contained_item)
                .cloned(),
            exported: self.dynamic.ecology.exported,
            external_imported: self.dynamic.dross_state.external_imported,
        }
    }

    pub(crate) fn restore_dense_dross_capture(
        &mut self,
        pos: AtlasPos,
        contained_item: u64,
        before: DenseDrossCaptureBefore,
    ) {
        let index = pos.index(self.manifest.side);
        self.dynamic.cells[index] = before.cell;
        self.dynamic.dross_state.cells[index] = before.carrier;
        self.dynamic.ecology.exported = before.exported;
        self.dynamic.dross_state.external_imported = before.external_imported;
        if let Some(provenance) = before.provenance {
            self.dynamic.dross_state.provenance.insert(pos, provenance);
        } else {
            self.dynamic.dross_state.provenance.remove(&pos);
        }
        if let Some(provenance) = before.contained {
            self.dynamic
                .dross_state
                .contained_provenance
                .insert(contained_item, provenance);
        } else {
            self.dynamic
                .dross_state
                .contained_provenance
                .remove(&contained_item);
        }
    }

    pub(crate) fn record_contained_dross_provenance(
        &mut self,
        item_id: u64,
        provenance: DrossProvenance,
    ) -> Result<(), String> {
        if provenance.total_units() == 0 {
            return Ok(());
        }
        if !self
            .dynamic
            .dross_state
            .contained_provenance
            .contains_key(&item_id)
            && self.dynamic.dross_state.contained_provenance.len() >= MAX_CONTAINED_PROVENANCE
        {
            return Err("contained dross provenance bound is full".into());
        }
        self.dynamic
            .dross_state
            .contained_provenance
            .entry(item_id)
            .or_default()
            .merge(provenance);
        Ok(())
    }

    pub fn dense_dross_current_at(
        &self,
        pos: AtlasPos,
        carrier: DrossCarrier,
    ) -> crate::arcane::Current {
        base_current_from_array(carrier_array(self, pos.index(self.manifest.side), carrier))
    }

    pub fn dense_dross_total_at(&self, pos: AtlasPos) -> u64 {
        let index = pos.index(self.manifest.side);
        self.dynamic.cells[index]
            .dross_total()
            .saturating_add(self.dynamic.dross_state.cells[index].mobile_total())
    }

    pub fn dross_band_at(&self, pos: AtlasPos) -> DrossBand {
        self.dynamic.dross_state.cells[pos.index(self.manifest.side)].band
    }

    /// Move sparse ledger custody into the compact Geography owner. Unknown or
    /// non-base resonance remains in the sparse source; the returned Current
    /// is exactly what fit and therefore exactly what the linked ledger debit
    /// must move.
    pub fn import_environmental_dross(
        &mut self,
        pos: AtlasPos,
        medium: crate::arcane::DrossMedium,
        offered: &crate::arcane::Current,
        contributions: &[DrossContribution],
    ) -> Result<crate::arcane::Current, String> {
        if pos.u >= self.manifest.side || pos.v >= self.manifest.side {
            return Err("environmental dross names an invalid atlas cell".into());
        }
        let index = pos.index(self.manifest.side);
        let carrier = match medium {
            crate::arcane::DrossMedium::Air => DrossCarrier::Air,
            crate::arcane::DrossMedium::Water => DrossCarrier::Water,
            crate::arcane::DrossMedium::Soil => DrossCarrier::Soil,
        };
        let mut moved = [0u16; 6];
        for (slot, resonance) in crate::arcane::BASE_RESONANCES.into_iter().enumerate() {
            let offered = offered.units_of(resonance);
            let present = u64::from(carrier_array(self, index, carrier)[slot]);
            moved[slot] = offered
                .min(u64::from(u16::MAX).saturating_sub(present))
                .min(u64::from(u16::MAX)) as u16;
        }
        match carrier {
            DrossCarrier::Air => {
                add_array_checked(&mut self.dynamic.dross_state.cells[index].airborne, moved)?
            }
            DrossCarrier::Water => {
                add_array_checked(&mut self.dynamic.dross_state.cells[index].waterborne, moved)?
            }
            DrossCarrier::Soil => add_array_checked(&mut self.dynamic.cells[index].dross, moved)?,
            _ => unreachable!(),
        }
        for (slot, units) in moved.into_iter().enumerate() {
            crate::arcane_geography::record_geography_import(
                &mut self.dynamic.ecology.exported,
                &mut self.dynamic.dross_state.external_imported,
                slot,
                u64::from(units),
            )
            .map_err(|error| error.to_string())?;
        }
        let moved_current = base_current_from_array(moved);
        let units = moved_current.total();
        self.dynamic.dross_state.imported_units = self
            .dynamic
            .dross_state
            .imported_units
            .saturating_add(units);
        self.dynamic.dross_state.generated_units = self
            .dynamic
            .dross_state
            .generated_units
            .saturating_add(units);
        let mut attributed = 0u64;
        for contribution in contributions {
            let mut contribution = contribution.clone();
            contribution.units = contribution.units.min(units.saturating_sub(attributed));
            attributed = attributed.saturating_add(contribution.units);
            if contribution.units != 0 {
                DrossPlanetState::add_process_units(
                    &mut self.dynamic.dross_state.generated_by_process,
                    &contribution.source_class,
                    contribution.units,
                );
                self.dynamic.dross_state.add_provenance(pos, contribution);
            }
        }
        if attributed < units {
            DrossPlanetState::add_process_units(
                &mut self.dynamic.dross_state.generated_by_process,
                "unknown",
                units - attributed,
            );
            self.dynamic
                .dross_state
                .add_unknown_provenance(pos, units - attributed);
        }
        Ok(moved_current)
    }

    /// Export exact dense dross into a container/scar ledger owner. Callers
    /// commit the returned debit and geography replacement in one linked
    /// transaction. Failure can restore the copied cell and dross state.
    pub fn export_environmental_dross(
        &mut self,
        pos: AtlasPos,
        carrier: DrossCarrier,
        requested: u64,
    ) -> Result<(crate::arcane::Current, DrossProvenance), String> {
        if pos.u >= self.manifest.side
            || pos.v >= self.manifest.side
            || !matches!(
                carrier,
                DrossCarrier::Air | DrossCarrier::Water | DrossCarrier::Soil
            )
        {
            return Err("dross export requires one valid dense environmental carrier".into());
        }
        let index = pos.index(self.manifest.side);
        let rotation = index ^ self.dynamic.dross_state.completed_steps as usize;
        let moved = match carrier {
            DrossCarrier::Air => take_array_units(
                &mut self.dynamic.dross_state.cells[index].airborne,
                requested,
                rotation,
            ),
            DrossCarrier::Water => take_array_units(
                &mut self.dynamic.dross_state.cells[index].waterborne,
                requested,
                rotation,
            ),
            DrossCarrier::Soil => {
                take_array_units(&mut self.dynamic.cells[index].dross, requested, rotation)
            }
            _ => unreachable!(),
        };
        let current = base_current_from_array(moved);
        for (slot, units) in moved.into_iter().enumerate() {
            crate::arcane_geography::record_geography_export(
                &mut self.dynamic.ecology.exported,
                &mut self.dynamic.dross_state.external_imported,
                slot,
                u64::from(units),
            )
            .map_err(|error| error.to_string())?;
        }
        let mut moved_provenance = DrossProvenance::default();
        if let Some(provenance) = self.dynamic.dross_state.provenance.get_mut(&pos) {
            moved_provenance = provenance.take_for_transport(current.total());
            if provenance.total_units() == 0 {
                self.dynamic.dross_state.provenance.remove(&pos);
            }
        }
        let accounted = moved_provenance.total_units();
        if accounted < current.total() {
            moved_provenance.add_unknown(current.total() - accounted);
        }
        Ok((current, moved_provenance))
    }

    pub fn advance_dross_toward(
        &mut self,
        atlas: &PlanetAtlas,
        registry: &crate::registry::Registry,
        target_hour: u64,
        budget: usize,
        conditions: DrossConditions<'_>,
    ) -> Result<DrossAdvance, String> {
        self.dynamic
            .dross_state
            .ensure_cells(self.dynamic.cells.len());
        if self.dynamic.dross_state.completed_steps >= target_hour {
            return Ok(DrossAdvance::default());
        }
        if self.dross_transport.is_none()
            && !self
                .dynamic
                .dross_state
                .has_transport_work(&self.dynamic.cells, &self.dynamic.ecology)
        {
            self.dynamic.dross_state.completed_steps = target_hour;
            self.dynamic.dross_state.pending_transport_hour = 0;
            self.dynamic.dross_state.pending_runoff_routes.clear();
            return Ok(DrossAdvance {
                completed_hour: Some(target_hour),
                ..DrossAdvance::default()
            });
        }
        if self.dross_transport.is_none() {
            let next = self.dynamic.dross_state.completed_steps.saturating_add(1);
            if self.dynamic.dross_state.pending_transport_hour == 0 {
                self.dynamic.dross_state.pending_transport_hour = next;
                self.dynamic.dross_state.pending_runoff_routes = conditions
                    .runoff_routes
                    .iter()
                    .copied()
                    .filter(|route| {
                        route.from.u < self.manifest.side
                            && route.from.v < self.manifest.side
                            && route.to.u < self.manifest.side
                            && route.to.v < self.manifest.side
                    })
                    .take(MAX_DROSS_RUNOFF_ROUTES.min(self.dynamic.cells.len()))
                    .collect();
            } else if self.dynamic.dross_state.pending_transport_hour != next {
                return Err("dross pending route checkpoint does not name the next hour".into());
            }
            let routes = self
                .dynamic
                .dross_state
                .pending_runoff_routes
                .iter()
                .copied()
                .map(|route| (route.from.index(self.manifest.side), route))
                .collect();
            self.dross_transport = Some(DrossTransportPass {
                target_hour: next,
                cursor: 0,
                delta: BTreeMap::new(),
                movements: BTreeMap::new(),
                routes,
            });
        }
        let mut pass = self.dross_transport.take().expect("dross pass initialized");
        let start = pass.cursor;
        let end = start
            .saturating_add(budget.max(1))
            .min(self.dynamic.cells.len());
        let day = pass.target_hour as f64 / 24.0;
        for index in start..end {
            self.compute_dross_cell_flux(atlas, day, conditions.long_winter, &mut pass, index);
        }
        pass.cursor = end;
        let mut report = DrossAdvance {
            processed_cells: end - start,
            ..DrossAdvance::default()
        };
        if end != self.dynamic.cells.len() {
            self.dross_transport = Some(pass);
            return Ok(report);
        }
        for (index, delta) in &pass.delta {
            for slot in 0..6 {
                self.dynamic.dross_state.cells[*index].airborne[slot] = apply_signed_u16(
                    self.dynamic.dross_state.cells[*index].airborne[slot],
                    delta.air[slot],
                )?;
                self.dynamic.dross_state.cells[*index].waterborne[slot] = apply_signed_u16(
                    self.dynamic.dross_state.cells[*index].waterborne[slot],
                    delta.water[slot],
                )?;
                self.dynamic.cells[*index].dross[slot] =
                    apply_signed_u16(self.dynamic.cells[*index].dross[slot], delta.soil[slot])?;
            }
        }
        for ((from, to), units) in pass.movements {
            let from = AtlasPos::from_index(from, self.manifest.side)
                .ok_or("dross movement source index is invalid")?;
            let to = AtlasPos::from_index(to, self.manifest.side)
                .ok_or("dross movement destination index is invalid")?;
            self.dynamic.dross_state.move_provenance(from, to, units);
            report.transported_units = report.transported_units.saturating_add(units);
        }
        self.dynamic.dross_state.transported_units = self
            .dynamic
            .dross_state
            .transported_units
            .saturating_add(report.transported_units);
        let (reordered, band_changes, breaches, cues) =
            self.finish_dross_hour(atlas, registry, pass.target_hour, conditions.living_hearts)?;
        self.dynamic.dross_state.completed_steps = pass.target_hour;
        self.dynamic.dross_state.pending_transport_hour = 0;
        self.dynamic.dross_state.pending_runoff_routes.clear();
        report.completed_hour = Some(pass.target_hour);
        report.reordered_units = reordered;
        report.band_changes = band_changes;
        report.breaches = breaches;
        report.cues = cues;
        Ok(report)
    }

    fn compute_dross_cell_flux(
        &self,
        atlas: &PlanetAtlas,
        day: f64,
        long_winter: bool,
        pass: &mut DrossTransportPass,
        index: usize,
    ) {
        let pos = AtlasPos::from_index(index, self.manifest.side).expect("dross cell index");
        let cell = self.dynamic.dross_state.cells[index];
        let soil = self.dynamic.cells[index].dross;
        let climate = atlas.genesis.climate.values()[index];
        let wind = seasonal_vector(climate.seasonal_wind, day);
        let downwind = pos.step(wind_direction(wind), self.manifest.side).pos;
        let downwind_index = downwind.index(self.manifest.side);
        for (slot, soil_units) in soil.into_iter().enumerate() {
            let advected = cell.airborne[slot] / 8;
            let _ = add_carrier_transfer(
                self,
                pass,
                index,
                downwind_index,
                DrossCarrier::Air,
                DrossCarrier::Air,
                slot,
                advected,
            );
            let diffuse = cell.airborne[slot].saturating_sub(advected) / 96;
            for direction in [
                Direction4::East,
                Direction4::North,
                Direction4::West,
                Direction4::South,
            ] {
                let neighbor = pos
                    .step(direction, self.manifest.side)
                    .pos
                    .index(self.manifest.side);
                let _ = add_carrier_transfer(
                    self,
                    pass,
                    index,
                    neighbor,
                    DrossCarrier::Air,
                    DrossCarrier::Air,
                    slot,
                    diffuse,
                );
            }

            if let Some(route) = pass.routes.get(&index).copied()
                && route.source_water_before_hu != 0
            {
                let moved = (u128::from(cell.waterborne[slot]) * u128::from(route.water_hu)
                    / u128::from(route.source_water_before_hu))
                .min(u128::from(u16::MAX)) as u16;
                let _ = add_carrier_transfer(
                    self,
                    pass,
                    index,
                    route.to.index(self.manifest.side),
                    DrossCarrier::Water,
                    DrossCarrier::Water,
                    slot,
                    moved,
                );
            }

            // Retentive soils pull a small declared fraction out of runoff;
            // high-energy runoff remobilizes an even smaller sediment share.
            let sorbed = (u32::from(cell.waterborne[slot])
                * u32::from(self.controls[index].dross_retention)
                / (255 * 48)) as u16;
            let _ = add_carrier_transfer(
                self,
                pass,
                index,
                index,
                DrossCarrier::Water,
                DrossCarrier::Soil,
                slot,
                sorbed,
            );
            if pass.routes.contains_key(&index) {
                let erosion = (u32::from(soil_units)
                    * u32::from(255u8.saturating_sub(self.controls[index].dross_retention))
                    / (255 * 96)) as u16;
                let _ = add_carrier_transfer(
                    self,
                    pass,
                    index,
                    index,
                    DrossCarrier::Soil,
                    DrossCarrier::Water,
                    slot,
                    erosion,
                );
            }

            let temperature = seasonal_scalar(climate.seasonal_temperature, day)
                - if long_winter { 18.0 } else { 0.0 };
            if temperature <= 0.0 {
                let frozen_partition = cell.waterborne[slot] / 16;
                let _ = add_carrier_transfer(
                    self,
                    pass,
                    index,
                    index,
                    DrossCarrier::Water,
                    DrossCarrier::Soil,
                    slot,
                    frozen_partition,
                );
            } else if temperature >= 3.0 && pass.routes.contains_key(&index) {
                let thawed_partition = soil[slot] / 128;
                let _ = add_carrier_transfer(
                    self,
                    pass,
                    index,
                    index,
                    DrossCarrier::Soil,
                    DrossCarrier::Water,
                    slot,
                    thawed_partition,
                );
            }
        }
    }

    fn finish_dross_hour(
        &mut self,
        atlas: &PlanetAtlas,
        registry: &crate::registry::Registry,
        hour: u64,
        living_hearts: &BTreeSet<u16>,
    ) -> Result<(u64, usize, usize, Vec<DrossCue>), String> {
        let healthy_ecosystems = self
            .dynamic
            .ecology
            .sites
            .iter()
            .filter(|site| {
                matches!(
                    site.stage,
                    crate::arcane_ecology::EcologyStage::Establishing
                        | crate::arcane_ecology::EcologyStage::Mature
                        | crate::arcane_ecology::EcologyStage::Recovering
                ) && registry
                    .arcane_ecology
                    .get(&site.content_id)
                    .is_some_and(|definition| {
                        definition.roles.iter().any(|role| {
                            matches!(
                                role,
                                crate::registry::EcologyRole::Transformer
                                    | crate::registry::EcologyRole::Stabilizer
                            )
                        })
                    })
            })
            .map(|site| site.atlas_pos)
            .collect::<BTreeSet<_>>();
        let scar_regions = self
            .dynamic
            .dross_state
            .scars
            .values()
            .filter(|site| site.resolved_step.is_none())
            .map(|site| site.region)
            .collect::<BTreeSet<_>>();
        let mut reordered_total = 0u64;
        let mut events = Vec::new();
        let mut cues = Vec::new();
        let mut breach_indices = Vec::new();
        for index in 0..self.dynamic.cells.len() {
            let pos = AtlasPos::from_index(index, self.manifest.side).expect("dross finish index");
            let heart_assignment = atlas.genesis.biomes.values()[index].heart_assignment;
            let heart_alive = heart_assignment == 0 || living_hearts.contains(&heart_assignment);
            let ecosystem = healthy_ecosystems.contains(&pos);
            let raw_rate = 1
                + u64::from(self.controls[index].recovery_potential) / 384
                + u64::from(heart_alive)
                + u64::from(ecosystem) * 2;
            let rate = raw_rate.min(16);
            let burden = burden_permille(
                self.dynamic.cells[index].dross_total(),
                self.dynamic.dross_state.cells[index].airborne_total(),
                self.dynamic.dross_state.cells[index].waterborne_total(),
                self.controls[index].capacity,
                self.controls[index].dross_retention,
                self.controls[index].stability,
            );
            let rate = if burden >= BREACH_BURDEN {
                rate / 2
            } else {
                rate
            }
            .max(1);
            let ambient_room = (0..6)
                .map(|slot| u64::from(u16::MAX - self.dynamic.cells[index].ambient[slot]))
                .sum::<u64>();
            let requested = rate.min(ambient_room).min(self.dense_dross_total_at(pos));
            let mut remaining = requested;
            let rotation = index ^ hour as usize;
            let from_air = take_array_units(
                &mut self.dynamic.dross_state.cells[index].airborne,
                remaining,
                rotation,
            );
            remaining -= from_air.into_iter().map(u64::from).sum::<u64>();
            let from_water = take_array_units(
                &mut self.dynamic.dross_state.cells[index].waterborne,
                remaining,
                rotation + 1,
            );
            remaining -= from_water.into_iter().map(u64::from).sum::<u64>();
            let from_soil = take_array_units(
                &mut self.dynamic.cells[index].dross,
                remaining,
                rotation + 2,
            );
            let mut reordered = [0u16; 6];
            for slot in 0..6 {
                reordered[slot] = from_air[slot]
                    .checked_add(from_water[slot])
                    .and_then(|value| value.checked_add(from_soil[slot]))
                    .ok_or("dross reordering mixture overflowed")?;
                self.dynamic.cells[index].ambient[slot] = self.dynamic.cells[index].ambient[slot]
                    .checked_add(reordered[slot])
                    .ok_or("reordered Current exceeds the cell ambient band")?;
            }
            let reordered_units = reordered.into_iter().map(u64::from).sum::<u64>();
            if reordered_units != 0 {
                if let Some(provenance) = self.dynamic.dross_state.provenance.get_mut(&pos) {
                    let _ = provenance.take_for_transport(reordered_units);
                }
                events.push((
                    pos,
                    DrossEventKind::Reordered {
                        units: reordered_units,
                        heart_aided: heart_alive,
                    },
                ));
                reordered_total = reordered_total.saturating_add(reordered_units);
                let process = match (heart_alive, ecosystem) {
                    (true, true) => "heart_and_ecology",
                    (true, false) => "heart_aided",
                    (false, true) => "ecology_aided",
                    (false, false) => "baseline_recovery",
                };
                DrossPlanetState::add_process_units(
                    &mut self.dynamic.dross_state.reordered_by_process,
                    process,
                    reordered_units,
                );
            }

            let scar_bonus = if scar_regions.contains(&pos) {
                u64::from(self.controls[index].capacity) / 4
            } else {
                0
            };
            let burden = burden_permille(
                self.dynamic.cells[index]
                    .dross_total()
                    .saturating_add(scar_bonus),
                self.dynamic.dross_state.cells[index].airborne_total(),
                self.dynamic.dross_state.cells[index].waterborne_total(),
                self.controls[index].capacity,
                self.controls[index].dross_retention,
                self.controls[index].stability,
            );
            let old = self.dynamic.dross_state.cells[index].band;
            let next = advance_band(
                old,
                self.dynamic.dross_state.cells[index].band_since_step,
                hour,
                burden,
            );
            if next != old {
                self.dynamic.dross_state.cells[index].band = next;
                self.dynamic.dross_state.cells[index].band_since_step = hour;
                events.push((
                    pos,
                    DrossEventKind::BandChanged {
                        from: old,
                        to: next,
                    },
                ));
            }
            if next == DrossBand::BreachRisk {
                let forecast = self.dynamic.dross_state.cells[index].breach_forecast_step;
                if forecast == 0 {
                    self.dynamic.dross_state.cells[index].breach_forecast_step = hour;
                    events.push((pos, DrossEventKind::BreachForecast));
                    cues.push(DrossCue {
                        region: pos,
                        kind: DrossCueKind::BreachForecast,
                    });
                } else if hour.saturating_sub(forecast) >= BREACH_FORECAST_STEPS
                    && hour.saturating_sub(self.dynamic.dross_state.cells[index].last_breach_step)
                        >= BREACH_RECOVERY_STEPS
                {
                    breach_indices.push(index);
                }
            } else {
                self.dynamic.dross_state.cells[index].breach_forecast_step = 0;
            }
        }
        let mut breaches = 0;
        for index in breach_indices {
            if self.redistribute_breach(index, hour, atlas)? != 0 {
                let pos = AtlasPos::from_index(index, self.manifest.side)
                    .expect("breach index remains valid");
                self.dynamic.dross_state.cells[index].last_breach_step = hour;
                self.dynamic.dross_state.breach_count =
                    self.dynamic.dross_state.breach_count.saturating_add(1);
                events.push((pos, DrossEventKind::Breach));
                let mut public_activity = None;
                if let Some(scar_id) = self
                    .dynamic
                    .dross_state
                    .scars
                    .values()
                    .filter(|site| site.region == pos && site.resolved_step.is_none())
                    .min_by_key(|site| site.id)
                {
                    let scar_id = scar_id.id;
                    let site = self
                        .dynamic
                        .dross_state
                        .scars
                        .get_mut(&scar_id)
                        .expect("selected active scar remains present");
                    site.breach_count = site.breach_count.saturating_add(1);
                    site.last_changed_step = hour;
                    if let Some(activity) = registry
                        .resolve_dross_scar(&site.content_id, site.kind)
                        .and_then(|definition| definition.activity)
                    {
                        public_activity = Some(activity);
                        events.push((pos, DrossEventKind::ScarActivity { scar_id, activity }));
                    }
                }
                cues.push(DrossCue {
                    region: pos,
                    kind: DrossCueKind::Breach {
                        activity: public_activity,
                    },
                });
                breaches += 1;
            }
        }
        self.dynamic.dross_state.reordered_units = self
            .dynamic
            .dross_state
            .reordered_units
            .saturating_add(reordered_total);
        let band_changes = events
            .iter()
            .filter(|(_, event)| matches!(event, DrossEventKind::BandChanged { .. }))
            .count();
        for (pos, event) in events {
            self.dynamic.dross_state.record(hour, pos, event);
        }
        Ok((reordered_total, band_changes, breaches, cues))
    }

    fn redistribute_breach(
        &mut self,
        index: usize,
        hour: u64,
        atlas: &PlanetAtlas,
    ) -> Result<u64, String> {
        let pos = AtlasPos::from_index(index, self.manifest.side)
            .ok_or("breach source index is invalid")?;
        let direction = match mix_dross(
            u64::from(self.manifest.seed) ^ index as u64 ^ hour.rotate_left(17),
        ) % 4
        {
            0 => Direction4::East,
            1 => Direction4::North,
            2 => Direction4::West,
            _ => Direction4::South,
        };
        let target = pos.step(direction, self.manifest.side).pos;
        let target_index = target.index(self.manifest.side);
        let source_hydrology = atlas.genesis.hydrology.values()[index];
        let target_hydrology = atlas.genesis.hydrology.values()[target_index];
        let same_watershed = source_hydrology.watershed_id != 0
            && source_hydrology.watershed_id == target_hydrology.watershed_id;
        let mut moved_total = 0u64;
        for slot in 0..6 {
            let air = self.dynamic.dross_state.cells[index].airborne[slot] / 4;
            let water = self.dynamic.dross_state.cells[index].waterborne[slot] / 4;
            let soil = self.dynamic.cells[index].dross[slot] / 8;
            let target_value = if same_watershed {
                self.dynamic.dross_state.cells[target_index].waterborne[slot]
            } else {
                self.dynamic.dross_state.cells[target_index].airborne[slot]
            };
            let requested = air.saturating_add(water).saturating_add(soil);
            let moved = requested.min(u16::MAX - target_value);
            let mut remaining = moved;
            let from_air = air.min(remaining);
            self.dynamic.dross_state.cells[index].airborne[slot] -= from_air;
            remaining -= from_air;
            let from_water = water.min(remaining);
            self.dynamic.dross_state.cells[index].waterborne[slot] -= from_water;
            remaining -= from_water;
            let from_soil = soil.min(remaining);
            self.dynamic.cells[index].dross[slot] -= from_soil;
            let target_carrier = if same_watershed {
                &mut self.dynamic.dross_state.cells[target_index].waterborne[slot]
            } else {
                &mut self.dynamic.dross_state.cells[target_index].airborne[slot]
            };
            *target_carrier += moved;
            moved_total += u64::from(moved);
        }
        self.dynamic
            .dross_state
            .move_provenance(pos, target, moved_total);
        Ok(moved_total)
    }

    pub fn scar_kind_for_region(
        &self,
        atlas: &PlanetAtlas,
        pos: AtlasPos,
        industrial: bool,
        cave: bool,
    ) -> ScarKind {
        if industrial {
            return ScarKind::IndustrialScale;
        }
        if cave {
            return ScarKind::CaveEcho;
        }
        let index = pos.index(self.manifest.side);
        let climate = atlas.genesis.climate.values()[index];
        let hydro = atlas.genesis.hydrology.values()[index];
        let biome = atlas.genesis.biomes.values()[index].baseline_biome;
        if climate.mean_temperature <= 1.0 || climate.snow_persistence >= 0.45 {
            ScarKind::FrostCraze
        } else if hydro.ocean_basin_id != 0
            || hydro.lake_basin_id != 0
            || hydro.mean_runoff >= 500.0
        {
            ScarKind::WetFilm
        } else if matches!(
            biome,
            crate::planet_atlas::BIOME_FOREST
                | crate::planet_atlas::BIOME_JUNGLE
                | crate::planet_atlas::BIOME_TAIGA
        ) {
            ScarKind::ForestThreads
        } else if matches!(
            biome,
            crate::planet_atlas::BIOME_DESERT
                | crate::planet_atlas::BIOME_BADLANDS
                | crate::planet_atlas::BIOME_SAVANNA
        ) || climate.aridity >= 1.2
        {
            ScarKind::DryNeedles
        } else {
            ScarKind::BrokenSymmetry
        }
    }

    pub fn scar_candidates(&self) -> Vec<AtlasPos> {
        let active_counts = self
            .dynamic
            .dross_state
            .scars
            .values()
            .filter(|site| site.resolved_step.is_none())
            .fold(BTreeMap::<AtlasPos, usize>::new(), |mut counts, site| {
                *counts.entry(site.region).or_default() += 1;
                counts
            });
        self.dynamic
            .dross_state
            .cells
            .iter()
            .enumerate()
            .filter(|(_, cell)| cell.band >= DrossBand::Seep)
            .filter_map(|(index, _)| AtlasPos::from_index(index, self.manifest.side))
            .filter(|pos| {
                active_counts.get(pos).copied().unwrap_or_default() < MAX_ACTIVE_SCARS_PER_REGION
            })
            .take(MAX_SCAR_SITES.saturating_sub(self.dynamic.dross_state.scars.len()))
            .collect()
    }
}

fn add_audit_units(
    carriers: &mut BTreeMap<DrossCarrier, u64>,
    resonance: &mut BTreeMap<String, u64>,
    carrier: DrossCarrier,
    current: &crate::arcane::Current,
) {
    let units = current.total();
    *carriers.entry(carrier).or_default() = carriers
        .get(&carrier)
        .copied()
        .unwrap_or_default()
        .saturating_add(units);
    for (name, units) in current.parts() {
        *resonance.entry(name.clone()).or_default() = resonance
            .get(name)
            .copied()
            .unwrap_or_default()
            .saturating_add(*units);
    }
}

fn add_audit_bands(
    carriers: &mut BTreeMap<DrossCarrier, u64>,
    resonance: &mut BTreeMap<String, u64>,
    carrier: DrossCarrier,
    bands: impl IntoIterator<Item = u64>,
) -> u64 {
    let mut total = 0u64;
    for (slot, units) in bands.into_iter().enumerate() {
        total = total.saturating_add(units);
        if units != 0 {
            let name = crate::arcane::BASE_RESONANCES
                .get(slot)
                .copied()
                .unwrap_or("unknown")
                .to_owned();
            *resonance.entry(name).or_default() = resonance
                .get(
                    crate::arcane::BASE_RESONANCES
                        .get(slot)
                        .copied()
                        .unwrap_or("unknown"),
                )
                .copied()
                .unwrap_or_default()
                .saturating_add(units);
        }
    }
    *carriers.entry(carrier).or_default() = carriers
        .get(&carrier)
        .copied()
        .unwrap_or_default()
        .saturating_add(total);
    total
}

fn add_regional_audit(
    atlas: &PlanetAtlas,
    region: AtlasPos,
    units: u64,
    countries: &mut BTreeMap<u16, u64>,
    watersheds: &mut BTreeMap<u32, u64>,
) {
    if units == 0 {
        return;
    }
    let index = region.index(atlas.side());
    let country = atlas.genesis.biomes.values()[index].country_id;
    let watershed = atlas.genesis.hydrology.values()[index].watershed_id;
    *countries.entry(country).or_default() = countries
        .get(&country)
        .copied()
        .unwrap_or_default()
        .saturating_add(units);
    *watersheds.entry(watershed).or_default() = watersheds
        .get(&watershed)
        .copied()
        .unwrap_or_default()
        .saturating_add(units);
}

/// Offline operator view of every persisted dross carrier. It deliberately
/// exposes aggregates and bounded evidence, never a survival-only omniscient
/// player list.
pub fn audit_world(world: &Path) -> Result<DrossAudit, String> {
    #[cfg(not(test))]
    let atlas = PlanetAtlas::load(world).map_err(|error| error.to_string())?;
    #[cfg(test)]
    let atlas = PlanetAtlas::load_fixture(world).map_err(|error| error.to_string())?;
    let geography = crate::arcane_geography::ArcaneGeography::load(world, &atlas)
        .map_err(|error| error.to_string())?;
    let ledger = crate::arcane::ArcaneLedger::load(world).map_err(|error| error.to_string())?;
    let arcane_audit = ledger.audit().map_err(|error| error.to_string())?;
    let state = &geography.dynamic.dross_state;
    let mut carrier_totals = DrossCarrier::ALL
        .into_iter()
        .map(|carrier| (carrier, 0))
        .collect::<BTreeMap<_, _>>();
    let mut resonance_totals = BTreeMap::new();
    let mut country_totals = BTreeMap::new();
    let mut watershed_totals = BTreeMap::new();
    let mut regional_units = vec![0u64; geography.dynamic.cells.len()];

    for (index, cell) in geography.dynamic.cells.iter().enumerate() {
        let air = add_audit_bands(
            &mut carrier_totals,
            &mut resonance_totals,
            DrossCarrier::Air,
            state.cells[index].airborne.into_iter().map(u64::from),
        );
        let water = add_audit_bands(
            &mut carrier_totals,
            &mut resonance_totals,
            DrossCarrier::Water,
            state.cells[index].waterborne.into_iter().map(u64::from),
        );
        let soil = add_audit_bands(
            &mut carrier_totals,
            &mut resonance_totals,
            DrossCarrier::Soil,
            cell.dross.into_iter().map(u64::from),
        );
        regional_units[index] = regional_units[index]
            .saturating_add(air)
            .saturating_add(water)
            .saturating_add(soil);
    }
    for wake in &geography.dynamic.wakes {
        let units = add_audit_bands(
            &mut carrier_totals,
            &mut resonance_totals,
            DrossCarrier::Air,
            wake.dross.into_iter().map(u64::from),
        );
        if let Some(region) = wake.path.get(wake.cursor as usize).copied() {
            regional_units[region.index(atlas.side())] =
                regional_units[region.index(atlas.side())].saturating_add(units);
        }
    }
    for site in &geography.dynamic.ecology.sites {
        let units = add_audit_bands(
            &mut carrier_totals,
            &mut resonance_totals,
            DrossCarrier::Organism,
            site.dross.into_iter().map(u64::from),
        );
        regional_units[site.atlas_pos.index(atlas.side())] =
            regional_units[site.atlas_pos.index(atlas.side())].saturating_add(units);
    }
    for (owner, account) in &ledger.accounts {
        let (carrier, region) = match owner {
            crate::arcane::ArcaneOwner::Dross { region, medium } => (
                match medium {
                    crate::arcane::DrossMedium::Air => DrossCarrier::Air,
                    crate::arcane::DrossMedium::Water => DrossCarrier::Water,
                    crate::arcane::DrossMedium::Soil => DrossCarrier::Soil,
                },
                Some(*region),
            ),
            crate::arcane::ArcaneOwner::ItemDross(_) => (DrossCarrier::Contained, None),
            crate::arcane::ArcaneOwner::AlchemyDross(_) => (DrossCarrier::Apparatus, None),
            crate::arcane::ArcaneOwner::Scar(id) => (
                DrossCarrier::Scar,
                state.scars.get(id).map(|site| site.region),
            ),
            _ => continue,
        };
        add_audit_units(
            &mut carrier_totals,
            &mut resonance_totals,
            carrier,
            &account.current,
        );
        if let Some(region) = region {
            regional_units[region.index(atlas.side())] =
                regional_units[region.index(atlas.side())].saturating_add(account.current.total());
        }
    }
    for (index, units) in regional_units.into_iter().enumerate() {
        let region = AtlasPos::from_index(index, atlas.side())
            .ok_or("dross regional audit index is invalid")?;
        add_regional_audit(
            &atlas,
            region,
            units,
            &mut country_totals,
            &mut watershed_totals,
        );
    }

    let mut band_cells = DrossBand::ALL
        .into_iter()
        .map(|band| (band, 0))
        .collect::<BTreeMap<_, _>>();
    let mut band_area = DrossBand::ALL
        .into_iter()
        .map(|band| (band, 0.0))
        .collect::<BTreeMap<_, _>>();
    for (index, cell) in state.cells.iter().enumerate() {
        *band_cells.entry(cell.band).or_default() += 1;
        *band_area.entry(cell.band).or_default() +=
            f64::from(atlas.genesis.geometry.values()[index].physical_area);
    }

    #[derive(Deserialize)]
    struct ExposureProfile {
        pos: crate::planet::EntityPos,
    }
    let mut population_exposure = DrossBand::ALL
        .into_iter()
        .map(|band| (band, 0))
        .collect::<BTreeMap<_, _>>();
    let profiles = world.join("players");
    if let Ok(entries) = std::fs::read_dir(profiles) {
        for entry in entries.flatten() {
            if entry
                .path()
                .extension()
                .and_then(|extension| extension.to_str())
                != Some("toml")
                || entry.file_name() == "index.toml"
            {
                continue;
            }
            let Some(profile) = std::fs::read_to_string(entry.path())
                .ok()
                .and_then(|text| toml::from_str::<ExposureProfile>(&text).ok())
            else {
                continue;
            };
            let region = atlas.atlas_pos(profile.pos.surface());
            *population_exposure
                .entry(geography.dross_band_at(region))
                .or_default() += 1;
        }
    }

    let current_total = carrier_totals.values().copied().sum::<u64>();
    let known_provenance_units = state
        .provenance
        .values()
        .chain(state.contained_provenance.values())
        .chain(
            state
                .scars
                .values()
                .filter(|site| site.resolved_step.is_none())
                .map(|site| &site.provenance),
        )
        .map(DrossProvenance::known_units)
        .sum::<u64>()
        .min(current_total);
    let environmental = [DrossCarrier::Air, DrossCarrier::Water, DrossCarrier::Soil]
        .into_iter()
        .map(|carrier| carrier_totals.get(&carrier).copied().unwrap_or_default())
        .sum::<u64>();
    let projected_environmental_recovery_hours = (state.reordered_units != 0 && environmental != 0)
        .then(|| {
            let hours = state.completed_steps.max(1);
            let numerator = u128::from(environmental).saturating_mul(u128::from(hours));
            u64::try_from(numerator.div_ceil(u128::from(state.reordered_units))).unwrap_or(u64::MAX)
        });
    let contained_units = carrier_totals
        .get(&DrossCarrier::Contained)
        .copied()
        .unwrap_or_default();
    let contained_owners = ledger
        .accounts
        .keys()
        .filter(|owner| matches!(owner, crate::arcane::ArcaneOwner::ItemDross(_)))
        .count();
    Ok(DrossAudit {
        carrier_totals,
        resonance_totals,
        country_totals,
        watershed_totals,
        generated_by_process: ledger.dross_generated_by_process.clone(),
        generated_by_installation: ledger.dross_generated_by_installation.clone(),
        reordered_by_process: state.reordered_by_process.clone(),
        band_cells,
        band_area,
        population_exposure,
        active_scars: state
            .scars
            .values()
            .filter(|site| site.resolved_step.is_none())
            .count(),
        resolved_scars: state
            .scars
            .values()
            .filter(|site| site.resolved_step.is_some())
            .count(),
        materialized_scars: state.materialized.len(),
        contained_owners,
        contained_units,
        known_provenance_units,
        unknown_provenance_units: current_total.saturating_sub(known_provenance_units),
        current_total,
        unexplained_arcane_delta: arcane_audit.unexplained_delta,
        completed_hours: state.completed_steps,
        projected_environmental_recovery_hours,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(
        seed: u32,
    ) -> (
        PlanetAtlas,
        crate::registry::Registry,
        crate::arcane_geography::ArcaneGeography,
    ) {
        fixture_with_side(seed, 8)
    }

    fn fixture_with_side(
        seed: u32,
        side: u16,
    ) -> (
        PlanetAtlas,
        crate::registry::Registry,
        crate::arcane_geography::ArcaneGeography,
    ) {
        let atlas = PlanetAtlas::fixture(seed, side).unwrap();
        let registry = crate::registry::load(Path::new("__no_dross_fixture_mods__"));
        let mut geography = crate::arcane_geography::ArcaneGeography::generate(
            &atlas,
            &registry,
            &crate::planet_atlas::CancellationToken::default(),
            |_| {},
        )
        .unwrap();
        geography
            .dynamic
            .dross_state
            .ensure_cells(geography.dynamic.cells.len());
        (atlas, registry, geography)
    }

    fn advance_fully(
        geography: &mut crate::arcane_geography::ArcaneGeography,
        atlas: &PlanetAtlas,
        registry: &crate::registry::Registry,
        target: u64,
        budget: usize,
        routes: &[RunoffTransport],
        living_hearts: &BTreeSet<u16>,
    ) {
        while geography.dynamic.dross_state.completed_steps < target {
            geography
                .advance_dross_toward(
                    atlas,
                    registry,
                    target,
                    budget,
                    DrossConditions {
                        runoff_routes: routes,
                        living_hearts,
                        long_winter: false,
                    },
                )
                .unwrap();
        }
    }

    fn environmental_and_ambient_total(
        geography: &crate::arcane_geography::ArcaneGeography,
    ) -> u64 {
        geography
            .dynamic
            .cells
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                cell.ambient_total()
                    .saturating_add(cell.dross_total())
                    .saturating_add(geography.dynamic.dross_state.cells[index].mobile_total())
            })
            .sum()
    }

    #[test]
    fn documented_bands_and_hysteresis_do_not_flicker() {
        assert_eq!(DrossBand::from_burden(99), DrossBand::Clear);
        assert_eq!(DrossBand::from_burden(100), DrossBand::Trace);
        assert_eq!(DrossBand::from_burden(249), DrossBand::Trace);
        assert_eq!(DrossBand::from_burden(250), DrossBand::Strained);
        assert_eq!(DrossBand::from_burden(500), DrossBand::Seep);
        assert_eq!(DrossBand::from_burden(750), DrossBand::Scar);
        assert_eq!(DrossBand::from_burden(1_000), DrossBand::BreachRisk);

        assert_eq!(
            advance_band(DrossBand::Trace, 10, 12, 260),
            DrossBand::Trace
        );
        assert_eq!(
            advance_band(DrossBand::Trace, 10, 13, 260),
            DrossBand::Strained
        );
        // Trace does not retreat until burden is below 100 - 40.
        assert_eq!(advance_band(DrossBand::Trace, 13, 20, 75), DrossBand::Trace);
        assert_eq!(advance_band(DrossBand::Trace, 13, 20, 59), DrossBand::Clear);
    }

    #[test]
    fn provenance_transport_conserves_units_and_degrades_evidence() {
        let mut mixture = DrossProvenance::default();
        mixture.add_known(DrossContribution {
            actor: Some([7; 16]),
            installation_id: Some(42),
            source_class: "alchemy wastewater".into(),
            units: 70,
            first_step: 1,
            last_step: 2,
            confidence_permille: 900,
        });
        mixture.add_unknown(30);
        let moved = mixture.take_for_transport(33);
        assert_eq!(mixture.total_units(), 67);
        assert_eq!(moved.total_units(), 33);
        assert!(moved.entries[0].confidence_permille < 900);
        let mut reunited = mixture;
        reunited.merge(moved);
        assert_eq!(reunited.total_units(), 100);
    }

    #[test]
    fn player_source_signatures_are_bounded_qualitative_and_private() {
        let actor = [0xab; 16];
        let mut likely = DrossProvenance::default();
        likely.add_known(DrossContribution {
            actor: Some(actor),
            installation_id: Some(0xdead_beef),
            source_class: "alchemy wastewater".into(),
            units: 90,
            first_step: 4,
            last_step: 7,
            confidence_permille: 900,
        });
        likely.add_unknown(10);
        let reading = likely.qualitative_signature().unwrap();
        assert!(reading.contains("likely"));
        assert!(reading.contains("alchemy wastewater"));
        assert!(!reading.contains("dead"));
        assert!(!reading.contains("abab"));
        assert!(reading.len() <= 96);

        let mut mixed = DrossProvenance::default();
        mixed.add_known(DrossContribution {
            actor: Some(actor),
            installation_id: Some(1),
            source_class: "wand strain".into(),
            units: 45,
            first_step: 1,
            last_step: 2,
            confidence_permille: 650,
        });
        mixed.add_unknown(55);
        assert!(
            mixed
                .qualitative_signature()
                .unwrap()
                .contains("consistent")
        );

        let mut moved = mixed.take_for_transport(100);
        for _ in 0..8 {
            moved.degrade_one_hop();
        }
        assert!(
            moved
                .qualitative_signature()
                .unwrap()
                .contains("inconclusive")
        );
    }

    #[test]
    fn process_counter_compaction_is_exact_and_never_exceeds_its_hard_bound() {
        let mut counters = BTreeMap::new();
        for index in 0..(MAX_DROSS_PROCESS_COUNTERS + 73) {
            add_bounded_process_units(&mut counters, &format!("fixture:{index:04}"), 1);
        }
        assert_eq!(counters.len(), MAX_DROSS_PROCESS_COUNTERS);
        assert_eq!(counters.values().sum::<u64>(), 329);
        assert!(counters.contains_key("unknown/compacted"));
    }

    #[test]
    fn water_routes_move_the_exact_proportion_without_moving_water_mass() {
        let (atlas, registry, mut geography) = fixture(0xd205_5001);
        for cell in &mut geography.dynamic.cells {
            cell.ambient = [u16::MAX; 6];
        }
        let from = AtlasPos {
            face: crate::planet::Face::PosZ,
            u: 3,
            v: 3,
        };
        let to = from.step(Direction4::East, atlas.side()).pos;
        geography.dynamic.dross_state.cells[from.index(atlas.side())].waterborne[0] = 1_001;
        let route = RunoffTransport {
            from,
            to,
            water_hu: 333,
            source_water_before_hu: 1_000,
        };
        let total_before = environmental_and_ambient_total(&geography);
        advance_fully(
            &mut geography,
            &atlas,
            &registry,
            1,
            usize::MAX,
            &[route],
            &BTreeSet::new(),
        );
        assert_eq!(
            geography.dynamic.dross_state.cells[to.index(atlas.side())].waterborne[0],
            333,
            "the carrier uses the accepted water fraction and leaves its integer remainder upstream"
        );
        assert_eq!(environmental_and_ambient_total(&geography), total_before);
        assert_eq!(
            route.water_hu, 333,
            "dross transport never edits water mass"
        );
    }

    #[test]
    fn evaporation_freezing_thawing_sorption_and_erosion_obey_exact_partitions() {
        let (mut atlas, registry, geography) = fixture(0xd205_5004);
        for climate in atlas.genesis.climate.values_mut() {
            climate.seasonal_temperature = [20.0; 4];
        }
        let pos = AtlasPos {
            face: crate::planet::Face::PosZ,
            u: 3,
            v: 3,
        };
        let to = pos.step(Direction4::East, atlas.side()).pos;
        let index = pos.index(atlas.side());

        // A hot, route-free hour represents evaporative water loss: dross is
        // explicitly nonvolatile, so no fraction follows vapor implicitly.
        let mut evaporating = geography.clone();
        for cell in &mut evaporating.dynamic.cells {
            cell.ambient = [u16::MAX; 6];
        }
        evaporating.controls[index].dross_retention = 0;
        evaporating.dynamic.dross_state.cells[index].waterborne[0] = 1_024;
        let before = environmental_and_ambient_total(&evaporating);
        advance_fully(
            &mut evaporating,
            &atlas,
            &registry,
            1,
            usize::MAX,
            &[],
            &BTreeSet::new(),
        );
        assert_eq!(
            evaporating.dynamic.dross_state.cells[index].waterborne[0],
            1_024
        );
        assert_eq!(environmental_and_ambient_total(&evaporating), before);

        let mut sorbing = geography.clone();
        for cell in &mut sorbing.dynamic.cells {
            cell.ambient = [u16::MAX; 6];
        }
        sorbing.controls[index].dross_retention = 255;
        sorbing.dynamic.dross_state.cells[index].waterborne[1] = 12_288;
        let before = environmental_and_ambient_total(&sorbing);
        advance_fully(
            &mut sorbing,
            &atlas,
            &registry,
            1,
            usize::MAX,
            &[],
            &BTreeSet::new(),
        );
        assert_eq!(
            sorbing.dynamic.dross_state.cells[index].waterborne[1],
            12_032
        );
        assert_eq!(sorbing.dynamic.cells[index].dross[1], 256);
        assert_eq!(environmental_and_ambient_total(&sorbing), before);

        let mut frozen_atlas = atlas.clone();
        for climate in frozen_atlas.genesis.climate.values_mut() {
            climate.seasonal_temperature = [-10.0; 4];
        }
        let mut freezing = geography.clone();
        for cell in &mut freezing.dynamic.cells {
            cell.ambient = [u16::MAX; 6];
        }
        freezing.controls[index].dross_retention = 0;
        freezing.dynamic.dross_state.cells[index].waterborne[2] = 1_600;
        let before = environmental_and_ambient_total(&freezing);
        advance_fully(
            &mut freezing,
            &frozen_atlas,
            &registry,
            1,
            usize::MAX,
            &[],
            &BTreeSet::new(),
        );
        assert_eq!(
            freezing.dynamic.dross_state.cells[index].waterborne[2],
            1_500
        );
        assert_eq!(freezing.dynamic.cells[index].dross[2], 100);
        assert_eq!(environmental_and_ambient_total(&freezing), before);

        let route = RunoffTransport {
            from: pos,
            to,
            water_hu: 1,
            source_water_before_hu: 1,
        };
        let mut thawing = geography.clone();
        for cell in &mut thawing.dynamic.cells {
            cell.ambient = [u16::MAX; 6];
        }
        thawing.controls[index].dross_retention = 255;
        thawing.dynamic.cells[index].dross[3] = 1_280;
        let before = environmental_and_ambient_total(&thawing);
        advance_fully(
            &mut thawing,
            &atlas,
            &registry,
            1,
            usize::MAX,
            &[route],
            &BTreeSet::new(),
        );
        assert_eq!(thawing.dynamic.cells[index].dross[3], 1_270);
        assert_eq!(thawing.dynamic.dross_state.cells[index].waterborne[3], 10);
        assert_eq!(environmental_and_ambient_total(&thawing), before);

        let mut mild_atlas = atlas.clone();
        for climate in mild_atlas.genesis.climate.values_mut() {
            climate.seasonal_temperature = [1.0; 4];
        }
        let mut eroding = geography;
        for cell in &mut eroding.dynamic.cells {
            cell.ambient = [u16::MAX; 6];
        }
        eroding.controls[index].dross_retention = 0;
        eroding.dynamic.cells[index].dross[4] = 9_600;
        let before = environmental_and_ambient_total(&eroding);
        advance_fully(
            &mut eroding,
            &mild_atlas,
            &registry,
            1,
            usize::MAX,
            &[route],
            &BTreeSet::new(),
        );
        assert_eq!(eroding.dynamic.cells[index].dross[4], 9_500);
        assert_eq!(eroding.dynamic.dross_state.cells[index].waterborne[4], 100);
        assert_eq!(environmental_and_ambient_total(&eroding), before);
    }

    #[test]
    fn air_and_water_cross_every_directed_cube_seam_without_privilege_or_loss() {
        let (atlas, registry, mut geography) = fixture(0xd205_5002);
        for cell in &mut geography.dynamic.cells {
            cell.ambient = [u16::MAX; 6];
        }
        let mut target_hour = 0;
        for face in crate::planet::Face::ALL {
            for direction in Direction4::ALL {
                for cell in &mut geography.dynamic.cells {
                    cell.dross = [0; 6];
                }
                geography
                    .dynamic
                    .dross_state
                    .cells
                    .fill(DrossCellState::default());
                geography.dynamic.dross_state.provenance.clear();
                let middle = atlas.side() / 2;
                let (u, v) = match direction {
                    Direction4::East => (atlas.side() - 1, middle),
                    Direction4::North => (middle, atlas.side() - 1),
                    Direction4::West => (0, middle),
                    Direction4::South => (middle, 0),
                };
                let from = AtlasPos { face, u, v };
                let to = from.step(direction, atlas.side()).pos;
                assert_ne!(from.face, to.face);
                let from_index = from.index(atlas.side());
                geography.dynamic.dross_state.cells[from_index].airborne[0] = 960;
                geography.dynamic.dross_state.cells[from_index].waterborne[1] = 200;
                let route = RunoffTransport {
                    from,
                    to,
                    water_hu: 1,
                    source_water_before_hu: 2,
                };
                let before = environmental_and_ambient_total(&geography);
                target_hour += 1;
                advance_fully(
                    &mut geography,
                    &atlas,
                    &registry,
                    target_hour,
                    5,
                    &[route],
                    &BTreeSet::new(),
                );
                let destination = geography.dynamic.dross_state.cells[to.index(atlas.side())];
                assert!(
                    destination.airborne[0] >= 8,
                    "air failed to diffuse across {face:?}/{direction:?}"
                );
                assert_eq!(
                    destination.waterborne[1], 100,
                    "water carrier was privileged at {face:?}/{direction:?}"
                );
                assert_eq!(environmental_and_ambient_total(&geography), before);
            }
        }
    }

    #[test]
    fn sliced_catchup_and_reload_recompute_identical_transport_checkpoints() {
        let (atlas, registry, mut seed) = fixture(0xd205_5003);
        let from = AtlasPos {
            face: crate::planet::Face::NegX,
            u: atlas.side() - 1,
            v: atlas.side() / 2,
        };
        let to = from.step(Direction4::East, atlas.side()).pos;
        let index = from.index(atlas.side());
        seed.dynamic.dross_state.cells[index].airborne = [960, 720, 0, 0, 0, 0];
        seed.dynamic.dross_state.cells[index].waterborne = [0, 0, 600, 0, 0, 0];
        seed.dynamic.cells[index].dross = [0, 0, 0, 480, 0, 0];
        seed.dynamic.dross_state.add_provenance(
            from,
            DrossContribution {
                actor: Some([3; 16]),
                installation_id: Some(91),
                source_class: "fixture laboratory".into(),
                units: 2_760,
                first_step: 0,
                last_step: 0,
                confidence_permille: 900,
            },
        );
        let route = RunoffTransport {
            from,
            to,
            water_hu: 1,
            source_water_before_hu: 3,
        };
        let mut continuous = seed.clone();
        let mut sliced = seed.clone();
        let mut reloaded = seed;
        advance_fully(
            &mut continuous,
            &atlas,
            &registry,
            4,
            usize::MAX,
            &[route],
            &BTreeSet::new(),
        );
        advance_fully(
            &mut sliced,
            &atlas,
            &registry,
            4,
            3,
            &[route],
            &BTreeSet::new(),
        );
        reloaded
            .advance_dross_toward(
                &atlas,
                &registry,
                4,
                5,
                DrossConditions {
                    runoff_routes: &[route],
                    living_hearts: &BTreeSet::new(),
                    long_winter: false,
                },
            )
            .unwrap();
        assert!(reloaded.dross_transport.is_some());
        reloaded.dross_transport = None;
        advance_fully(
            &mut reloaded,
            &atlas,
            &registry,
            1,
            7,
            &[],
            &BTreeSet::new(),
        );
        advance_fully(
            &mut reloaded,
            &atlas,
            &registry,
            4,
            7,
            &[route],
            &BTreeSet::new(),
        );
        assert_eq!(continuous.dynamic, sliced.dynamic);
        assert_eq!(continuous.catalog, sliced.catalog);
        assert_eq!(continuous.dynamic, reloaded.dynamic);
        assert_eq!(continuous.catalog, reloaded.catalog);
    }

    #[test]
    fn regional_scar_slots_and_all_bounded_populations_fail_closed() {
        let (atlas, _, mut geography) = fixture(0xd205_5005);
        let region = AtlasPos {
            face: crate::planet::Face::PosZ,
            u: 3,
            v: 3,
        };
        geography.dynamic.dross_state.cells[region.index(atlas.side())].band = DrossBand::Seep;
        for slot in 0..MAX_ACTIVE_SCARS_PER_REGION {
            assert!(geography.scar_candidates().contains(&region));
            let id = slot as u64 + 1;
            geography.dynamic.dross_state.scars.insert(
                id,
                ScarSite {
                    id,
                    region,
                    kind: ScarKind::BrokenSymmetry,
                    content_id: "base:scar_broken_symmetry".into(),
                    site_slot: slot as u8,
                    created_step: 1,
                    last_changed_step: 1,
                    breach_count: 0,
                    materialized_at: None,
                    resolved_step: None,
                    actor_hint: None,
                    installation_hint: None,
                    provenance: DrossProvenance::default(),
                },
            );
        }
        assert!(!geography.scar_candidates().contains(&region));
        geography
            .dynamic
            .dross_state
            .validate(geography.dynamic.cells.len(), atlas.side())
            .unwrap();
        let last_id = MAX_ACTIVE_SCARS_PER_REGION as u64;
        geography
            .dynamic
            .dross_state
            .scars
            .get_mut(&last_id)
            .unwrap()
            .site_slot = 0;
        assert!(
            geography
                .dynamic
                .dross_state
                .validate(geography.dynamic.cells.len(), atlas.side())
                .is_err()
        );
    }

    #[test]
    fn dross_save_memory_wire_and_server_slice_budgets_are_measured() {
        const SAMPLE: usize = 6 * 8 * 8;
        fn evidence(seed: u64) -> DrossProvenance {
            let mut evidence = DrossProvenance::default();
            for source in 0..MAX_PROVENANCE_ENTRIES {
                evidence.add_known(DrossContribution {
                    actor: Some([(seed as u8).wrapping_add(source as u8); 16]),
                    installation_id: Some(u64::MAX - seed - source as u64),
                    source_class: format!("fixture:{source}:{}", "x".repeat(80)),
                    units: u64::MAX / 16,
                    first_step: u64::MAX - 1,
                    last_step: u64::MAX,
                    confidence_permille: 1_000,
                });
            }
            evidence
        }
        fn scaled_delta(base: usize, variant: usize, population: usize) -> u128 {
            (variant.saturating_sub(base) as u128)
                .saturating_mul(population as u128)
                .div_ceil(SAMPLE as u128)
        }

        let mut dense = DrossPlanetState::initialized(SAMPLE);
        for (index, cell) in dense.cells.iter_mut().enumerate() {
            cell.airborne = [u16::MAX; 6];
            cell.waterborne = [u16::MAX; 6];
            cell.band = DrossBand::BreachRisk;
            cell.band_since_step = u64::MAX - index as u64;
            cell.breach_forecast_step = u64::MAX - 1;
            cell.last_breach_step = u64::MAX;
        }
        let started = std::time::Instant::now();
        let dense_bytes = postcard::to_allocvec(&dense).unwrap().len();

        let mut with_provenance = dense.clone();
        let mut with_contained = dense.clone();
        let mut with_scars = dense.clone();
        let mut with_routes = dense.clone();
        let mut with_events = dense.clone();
        for index in 0..SAMPLE {
            let pos = AtlasPos::from_index(index, 8).unwrap();
            with_provenance
                .provenance
                .insert(pos, evidence(index as u64));
            with_contained
                .contained_provenance
                .insert(u64::MAX - index as u64, evidence(index as u64));
            with_scars.scars.insert(
                index as u64 + 1,
                ScarSite {
                    id: index as u64 + 1,
                    region: pos,
                    kind: ScarKind::BrokenSymmetry,
                    content_id: "fixture:maximal_scar_identity".into(),
                    site_slot: 0,
                    created_step: u64::MAX - 1,
                    last_changed_step: u64::MAX,
                    breach_count: u16::MAX,
                    materialized_at: None,
                    resolved_step: None,
                    actor_hint: Some([index as u8; 16]),
                    installation_hint: Some(u64::MAX - index as u64),
                    provenance: evidence(index as u64),
                },
            );
            with_routes.pending_runoff_routes.push(RunoffTransport {
                from: pos,
                to: pos.step(Direction4::East, 8).pos,
                water_hu: u64::from(u32::MAX),
                source_water_before_hu: u64::from(u32::MAX),
            });
            with_events.record(
                u64::MAX,
                pos,
                DrossEventKind::ContainmentFailed {
                    carrier: DrossCarrier::Contained,
                },
            );
        }
        with_routes.pending_transport_hour = u64::MAX;
        let provenance_bytes = postcard::to_allocvec(&with_provenance).unwrap().len();
        let contained_bytes = postcard::to_allocvec(&with_contained).unwrap().len();
        let scar_bytes = postcard::to_allocvec(&with_scars).unwrap().len();
        let route_bytes = postcard::to_allocvec(&with_routes).unwrap().len();
        let event_bytes = postcard::to_allocvec(&with_events).unwrap().len();
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        let production = crate::planet_atlas::ATLAS_CELL_COUNT;
        let projected = (dense_bytes as u128)
            .saturating_mul(production as u128)
            .div_ceil(SAMPLE as u128)
            .saturating_add(scaled_delta(
                dense_bytes,
                provenance_bytes,
                MAX_PROVENANCE_CELLS.min(production),
            ))
            .saturating_add(scaled_delta(
                dense_bytes,
                contained_bytes,
                MAX_CONTAINED_PROVENANCE,
            ))
            .saturating_add(scaled_delta(dense_bytes, scar_bytes, MAX_SCAR_SITES))
            .saturating_add(scaled_delta(
                dense_bytes,
                route_bytes,
                MAX_DROSS_RUNOFF_ROUTES.min(production),
            ))
            .saturating_add(scaled_delta(dense_bytes, event_bytes, MAX_DROSS_EVENTS));
        assert!(
            projected <= u128::from(DROSS_MAX_PROJECTED_SAVE_BYTES),
            "projected maximal dross state is {} MiB",
            projected / (1024 * 1024)
        );
        let dense_route_resident = (production
            * (std::mem::size_of::<DrossCellState>() + std::mem::size_of::<RunoffTransport>()))
            as u64;
        assert!(dense_route_resident <= DROSS_MAX_DENSE_ROUTE_RESIDENT_BYTES);

        let cue = crate::net::S2C::PreparationState {
            modifiers: crate::alchemy::PreparationModifiers {
                dross_band: DrossBand::BreachRisk.ordinal(),
                dross_pattern: DrossBand::BreachRisk.ordinal(),
                ..crate::alchemy::PreparationModifiers::default()
            },
            bodily_dross: 4_096,
        };
        assert!(crate::net::encode(&cue).len() <= DROSS_MAX_CLIENT_CUE_BYTES);
        let field = crate::net::S2C::ArcaneCue {
            bands: [4, DrossBand::BreachRisk.ordinal()],
            dominant: 6,
            ecology: Some((
                "visible indicator response without exact provenance".into(),
                false,
            )),
        };
        assert!(crate::net::encode(&field).len() <= DROSS_MAX_CLIENT_CUE_BYTES);
        let breach = crate::net::S2C::DrossEvent(DrossCue {
            region: AtlasPos {
                face: crate::planet::Face::PosZ,
                u: 7,
                v: 7,
            },
            kind: DrossCueKind::Breach {
                activity: Some(ScarActivityHandler::Shear),
            },
        });
        assert!(crate::net::encode(&breach).len() <= DROSS_MAX_CLIENT_CUE_BYTES);

        let (atlas, registry, mut geography) = fixture(0xd205_5006);
        for cell in &mut geography.dynamic.cells {
            cell.ambient = [u16::MAX; 6];
        }
        for cell in &mut geography.dynamic.dross_state.cells {
            cell.airborne = [1_000; 6];
        }
        let slice = std::time::Instant::now();
        let report = geography
            .advance_dross_toward(
                &atlas,
                &registry,
                1,
                DROSS_SERVER_SLICE_CELLS.min(31),
                DrossConditions {
                    runoff_routes: &[],
                    living_hearts: &BTreeSet::new(),
                    long_winter: false,
                },
            )
            .unwrap();
        assert!(report.processed_cells <= 31);
        assert!(slice.elapsed() < std::time::Duration::from_secs(1));
        eprintln!(
            "dross budget: projected={} MiB dense+routes={} MiB cue={} bytes slice={:?}; sample dense/provenance/contained/scar/route/event={dense_bytes}/{provenance_bytes}/{contained_bytes}/{scar_bytes}/{route_bytes}/{event_bytes}",
            projected / (1024 * 1024),
            dense_route_resident / (1024 * 1024),
            crate::net::encode(&cue).len(),
            slice.elapsed()
        );
    }

    #[test]
    fn long_recovery_retains_exact_total_and_bounded_cardinality() {
        let (atlas, registry, mut geography) = fixture(0xd205_5007);
        let region = AtlasPos {
            face: crate::planet::Face::NegY,
            u: 4,
            v: 4,
        };
        let index = region.index(atlas.side());
        geography.dynamic.dross_state.cells[index].airborne = [2_000; 6];
        geography.dynamic.dross_state.cells[index].waterborne = [2_000; 6];
        geography.dynamic.cells[index].dross = [2_000; 6];
        for source in 0..32 {
            geography.dynamic.dross_state.add_provenance(
                region,
                DrossContribution {
                    actor: Some([source; 16]),
                    installation_id: Some(u64::from(source)),
                    source_class: format!("long-recovery-source-{source}"),
                    units: 1_125,
                    first_step: 0,
                    last_step: 0,
                    confidence_permille: 1_000,
                },
            );
        }
        let total = environmental_and_ambient_total(&geography);
        advance_fully(
            &mut geography,
            &atlas,
            &registry,
            10 * 365 * 24,
            17,
            &[],
            &BTreeSet::new(),
        );
        assert_eq!(environmental_and_ambient_total(&geography), total);
        assert_eq!(geography.dense_dross_total_at(region), 0);
        assert!(geography.dynamic.dross_state.provenance.len() <= MAX_PROVENANCE_CELLS);
        assert!(
            geography
                .dynamic
                .dross_state
                .provenance
                .values()
                .all(|mixture| mixture.entries.len() <= MAX_PROVENANCE_ENTRIES)
        );
        assert!(geography.dynamic.dross_state.events.len() <= MAX_DROSS_EVENTS);
        geography
            .dynamic
            .dross_state
            .validate(geography.dynamic.cells.len(), atlas.side())
            .unwrap();
    }

    #[test]
    fn century_equivalent_magic_use_and_stopped_abuse_are_exact_and_bounded() {
        const CENTURY_HOURS: u64 = 100 * 365 * 24;
        let (atlas, registry, mut baseline) = fixture_with_side(0xd205_5008, 2);
        for cell in &mut baseline.dynamic.cells {
            // The fixture needs enough movable Current to fund both practice
            // and cleanup without relying on a production-world distribution.
            cell.ambient = [30_000; 6];
            cell.dross = [0; 6];
        }
        baseline
            .dynamic
            .dross_state
            .cells
            .fill(DrossCellState::default());
        let exact_total = environmental_and_ambient_total(&baseline);

        let mut untouched = baseline.clone();
        advance_fully(
            &mut untouched,
            &atlas,
            &registry,
            CENTURY_HOURS,
            usize::MAX,
            &[],
            &BTreeSet::new(),
        );
        assert_eq!(environmental_and_ambient_total(&untouched), exact_total);
        assert!(untouched.dynamic.dross_state.scars.is_empty());

        let home = AtlasPos {
            face: crate::planet::Face::PosZ,
            u: 0,
            v: 0,
        };
        let home_index = home.index(atlas.side());
        let mut careful = baseline.clone();
        for month in 0..1_200u64 {
            // One small, explicitly funded practice session per month. The
            // Current leaves Ambient before it enters Dross.
            careful.dynamic.cells[home_index].ambient[0] -= 2;
            careful.dynamic.cells[home_index].dross[0] += 2;
            advance_fully(
                &mut careful,
                &atlas,
                &registry,
                (month + 1) * CENTURY_HOURS / 1_200,
                usize::MAX,
                &[],
                &BTreeSet::new(),
            );
        }
        assert_eq!(environmental_and_ambient_total(&careful), exact_total);
        assert!(
            careful
                .dynamic
                .dross_state
                .cells
                .iter()
                .all(|cell| cell.band < DrossBand::Seep)
        );
        assert!(careful.dynamic.dross_state.scars.is_empty());

        let mut abuse = baseline;
        let sources = [
            home,
            home.step(Direction4::East, atlas.side()).pos,
            home.step(Direction4::North, atlas.side()).pos,
        ];
        for source in sources {
            let index = source.index(atlas.side());
            abuse.controls[index].capacity = 64;
            abuse.controls[index].stability = 0;
            for slot in 0..3 {
                abuse.dynamic.cells[index].ambient[slot] -= 6_000;
            }
            abuse.dynamic.dross_state.cells[index].airborne[0] = 6_000;
            abuse.dynamic.dross_state.cells[index].waterborne[1] = 6_000;
            abuse.dynamic.cells[index].dross[2] = 6_000;
        }
        advance_fully(&mut abuse, &atlas, &registry, 22, 3, &[], &BTreeSet::new());
        let damaged = abuse
            .dynamic
            .dross_state
            .cells
            .iter()
            .filter(|cell| cell.band >= DrossBand::Scar)
            .count();
        assert!(damaged >= sources.len());
        assert!(damaged < abuse.dynamic.cells.len());
        assert_eq!(environmental_and_ambient_total(&abuse), exact_total);

        for control in &mut abuse.controls {
            control.recovery_potential = 6_144;
        }
        advance_fully(
            &mut abuse,
            &atlas,
            &registry,
            CENTURY_HOURS,
            usize::MAX,
            &[],
            &BTreeSet::new(),
        );
        assert_eq!(environmental_and_ambient_total(&abuse), exact_total);
        assert!(
            abuse
                .dynamic
                .dross_state
                .cells
                .iter()
                .all(|cell| cell.band < DrossBand::Scar)
        );
        let breaches = abuse.dynamic.dross_state.breach_count;
        advance_fully(
            &mut abuse,
            &atlas,
            &registry,
            CENTURY_HOURS + 100,
            1,
            &[],
            &BTreeSet::new(),
        );
        assert_eq!(abuse.dynamic.dross_state.breach_count, breaches);
        assert_eq!(environmental_and_ambient_total(&abuse), exact_total);
        eprintln!(
            "century magic profile: hours={CENTURY_HOURS} exact_current={exact_total} careful_sessions=1200 abuse_sources={} damaged_cells={damaged} recovery_breaches={breaches}",
            sources.len(),
        );
    }

    #[test]
    fn scenario_careful_homestead_stays_below_seep_for_two_years() {
        let (atlas, registry, mut geography) = fixture(0xd205_5101);
        let home = AtlasPos {
            face: crate::planet::Face::PosZ,
            u: atlas.side() / 2,
            v: atlas.side() / 2,
        };
        let index = home.index(atlas.side());
        let mut highest = DrossBand::Clear;
        let hours = 2 * 365 * 24;
        for hour in 1..=hours {
            // Occasional, stable small practice: two accounted units per day.
            // The ordinary recovery interval is deliberately much longer than
            // the production event.
            if hour % 24 == 1 {
                geography.dynamic.cells[index].dross[0] = geography.dynamic.cells[index].dross[0]
                    .checked_add(2)
                    .unwrap();
            }
            advance_fully(
                &mut geography,
                &atlas,
                &registry,
                hour,
                usize::MAX,
                &[],
                &BTreeSet::new(),
            );
            highest = highest.max(geography.dynamic.dross_state.cells[index].band);
        }
        assert!(
            highest < DrossBand::Seep,
            "careful practice reached {highest:?}"
        );
        assert!(geography.dynamic.dross_state.scars.is_empty());
    }

    #[test]
    fn scenario_careless_laboratory_warns_through_every_band_before_breach() {
        let (atlas, registry, mut geography) = fixture(0xd205_5102);
        let laboratory = AtlasPos {
            face: crate::planet::Face::PosZ,
            u: atlas.side() / 2,
            v: atlas.side() / 2,
        };
        let index = laboratory.index(atlas.side());
        geography.controls[index].capacity = 64;
        geography.controls[index].stability = 0;
        geography.dynamic.dross_state.cells[index].airborne[0] = 8_000;
        geography.dynamic.dross_state.cells[index].waterborne[1] = 8_000;
        geography.dynamic.cells[index].dross[2] = 8_000;
        geography.dynamic.dross_state.scars.insert(
            42,
            ScarSite {
                id: 42,
                region: laboratory,
                kind: ScarKind::BrokenSymmetry,
                content_id: "base:scar_broken_symmetry".into(),
                site_slot: 0,
                created_step: 0,
                last_changed_step: 0,
                breach_count: 0,
                materialized_at: None,
                resolved_step: None,
                actor_hint: Some([0x42; 16]),
                installation_hint: Some(0x4242),
                provenance: DrossProvenance::default(),
            },
        );
        for cell in &mut geography.dynamic.cells {
            cell.ambient = [u16::MAX; 6];
        }
        let mut public_cues = Vec::new();
        while geography.dynamic.dross_state.completed_steps < 22 {
            let report = geography
                .advance_dross_toward(
                    &atlas,
                    &registry,
                    22,
                    7,
                    DrossConditions {
                        runoff_routes: &[],
                        living_hearts: &BTreeSet::new(),
                        long_winter: false,
                    },
                )
                .unwrap();
            public_cues.extend(report.cues);
        }
        let warnings = geography
            .dynamic
            .dross_state
            .events
            .iter()
            .filter(|event| event.region == laboratory)
            .filter_map(|event| match event.kind {
                DrossEventKind::BandChanged { to, .. } => Some(to),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            &warnings[..5],
            &[
                DrossBand::Trace,
                DrossBand::Strained,
                DrossBand::Seep,
                DrossBand::Scar,
                DrossBand::BreachRisk,
            ]
        );
        let forecast = geography
            .dynamic
            .dross_state
            .events
            .iter()
            .position(|event| {
                event.region == laboratory && matches!(event.kind, DrossEventKind::BreachForecast)
            })
            .unwrap();
        let breach = geography
            .dynamic
            .dross_state
            .events
            .iter()
            .position(|event| {
                event.region == laboratory && matches!(event.kind, DrossEventKind::Breach)
            })
            .unwrap();
        assert!(forecast < breach);
        assert!(
            public_cues
                .iter()
                .any(|cue| matches!(cue.kind, DrossCueKind::BreachForecast))
        );
        assert!(public_cues.iter().any(|cue| matches!(
            cue.kind,
            DrossCueKind::Breach {
                activity: Some(ScarActivityHandler::Shear)
            }
        )));
        for cue in &public_cues {
            assert_eq!(cue.region, laboratory);
            assert!(
                crate::net::encode(&crate::net::S2C::DrossEvent(*cue)).len()
                    <= DROSS_MAX_CLIENT_CUE_BYTES
            );
            let text = cue.accessible_text();
            assert!(!text.contains("4242"));
            assert!(!text.contains("42"));
        }
        assert!(geography.scar_candidates().contains(&laboratory));
    }

    #[test]
    fn scenario_watershed_dispute_moves_pollution_and_degrades_a_useful_signature() {
        let (atlas, registry, mut geography) = fixture(0xd205_5103);
        for cell in &mut geography.dynamic.cells {
            cell.ambient = [u16::MAX; 6];
        }
        let upstream = AtlasPos {
            face: crate::planet::Face::PosY,
            u: 3,
            v: 3,
        };
        let downstream = upstream.step(Direction4::South, atlas.side()).pos;
        let upstream_index = upstream.index(atlas.side());
        geography.dynamic.dross_state.cells[upstream_index].waterborne[0] = 1_200;
        geography.dynamic.dross_state.add_provenance(
            upstream,
            DrossContribution {
                actor: Some([9; 16]),
                installation_id: Some(0x51a7e),
                source_class: "upstream alchemy wastewater".into(),
                units: 1_200,
                first_step: 0,
                last_step: 0,
                confidence_permille: 940,
            },
        );
        let route = RunoffTransport {
            from: upstream,
            to: downstream,
            water_hu: 1,
            source_water_before_hu: 2,
        };
        let total = environmental_and_ambient_total(&geography);
        advance_fully(
            &mut geography,
            &atlas,
            &registry,
            1,
            4,
            &[route],
            &BTreeSet::new(),
        );
        assert_eq!(
            geography.dynamic.dross_state.cells[downstream.index(atlas.side())].waterborne[0],
            600
        );
        let sample = geography
            .dynamic
            .dross_state
            .provenance
            .get(&downstream)
            .unwrap();
        assert_eq!(sample.known_units(), 600);
        assert_eq!(sample.entries[0].installation_id, Some(0x51a7e));
        assert!(sample.entries[0].confidence_permille < 940);
        assert!(
            sample.entries[0].confidence_permille >= PROVENANCE_MIN_IDENTIFIABLE_CONFIDENCE,
            "one watershed hop should remain useful evidence"
        );
        assert_eq!(environmental_and_ambient_total(&geography), total);
    }

    #[test]
    fn scenario_dead_heart_recovery_remains_possible_and_reawakening_accelerates_it() {
        let (atlas, registry, mut seed) = fixture(0xd205_5104);
        let (index, assignment) = atlas
            .genesis
            .biomes
            .values()
            .iter()
            .enumerate()
            .find_map(|(index, cell)| {
                (cell.heart_assignment != 0).then_some((index, cell.heart_assignment))
            })
            .expect("fixture has a heart-assigned country");
        seed.dynamic.cells[index].dross[0] = 1_000;
        seed.controls[index].recovery_potential = 0;
        let before = environmental_and_ambient_total(&seed);
        let mut dead = seed.clone();
        let mut reawakened = seed;
        advance_fully(
            &mut dead,
            &atlas,
            &registry,
            1,
            usize::MAX,
            &[],
            &BTreeSet::new(),
        );
        advance_fully(
            &mut reawakened,
            &atlas,
            &registry,
            1,
            usize::MAX,
            &[],
            &BTreeSet::from([assignment]),
        );
        assert!(dead.dynamic.dross_state.reordered_units > 0);
        assert!(
            reawakened.dynamic.dross_state.reordered_units
                > dead.dynamic.dross_state.reordered_units
        );
        assert_eq!(environmental_and_ambient_total(&dead), before);
        assert_eq!(environmental_and_ambient_total(&reawakened), before);
    }

    #[test]
    fn scenario_planetary_abuse_is_regional_not_self_replicating_and_recovers_when_stopped() {
        let (atlas, registry, mut geography) = fixture(0xd205_5105);
        let center = AtlasPos {
            face: crate::planet::Face::NegZ,
            u: atlas.side() / 2,
            v: atlas.side() / 2,
        };
        let sources = [
            center,
            center.step(Direction4::East, atlas.side()).pos,
            center.step(Direction4::North, atlas.side()).pos,
        ];
        for source in sources {
            let index = source.index(atlas.side());
            geography.controls[index].capacity = 64;
            geography.controls[index].stability = 0;
            geography.dynamic.dross_state.cells[index].airborne[0] = 6_000;
            geography.dynamic.dross_state.cells[index].waterborne[1] = 6_000;
            geography.dynamic.cells[index].dross[2] = 6_000;
        }
        for cell in &mut geography.dynamic.cells {
            cell.ambient = [u16::MAX; 6];
        }
        advance_fully(
            &mut geography,
            &atlas,
            &registry,
            22,
            11,
            &[],
            &BTreeSet::new(),
        );
        let regional = geography
            .dynamic
            .dross_state
            .cells
            .iter()
            .filter(|cell| cell.band >= DrossBand::Scar)
            .count();
        assert!(regional >= sources.len());
        assert!(regional < geography.dynamic.cells.len() / 4);
        assert!(geography.dynamic.dross_state.breach_count > 0);

        for (control, cell) in geography
            .controls
            .iter_mut()
            .zip(&mut geography.dynamic.cells)
        {
            control.recovery_potential = 6_144;
            cell.ambient = [0; 6];
        }
        let recovery_start = environmental_and_ambient_total(&geography);
        advance_fully(
            &mut geography,
            &atlas,
            &registry,
            5_000,
            usize::MAX,
            &[],
            &BTreeSet::new(),
        );
        assert_eq!(environmental_and_ambient_total(&geography), recovery_start);
        assert!(
            geography
                .dynamic
                .dross_state
                .cells
                .iter()
                .all(|cell| cell.band < DrossBand::Scar)
        );
        assert_eq!(geography.dynamic.dross_state.generated_units, 0);
        let recovered_breaches = geography.dynamic.dross_state.breach_count;
        advance_fully(
            &mut geography,
            &atlas,
            &registry,
            5_100,
            usize::MAX,
            &[],
            &BTreeSet::new(),
        );
        assert_eq!(
            geography.dynamic.dross_state.breach_count, recovered_breaches,
            "a recovered region must not keep a self-replicating breach clock"
        );

        let (accident_atlas, accident_registry, mut accident) = fixture(0xd205_5106);
        let accident_index = center.index(accident_atlas.side());
        accident.dynamic.dross_state.cells[accident_index].airborne[0] = 12;
        advance_fully(
            &mut accident,
            &accident_atlas,
            &accident_registry,
            48,
            usize::MAX,
            &[],
            &BTreeSet::new(),
        );
        assert_eq!(accident.dynamic.dross_state.breach_count, 0);
        assert!(
            accident
                .dynamic
                .dross_state
                .cells
                .iter()
                .all(|cell| cell.band < DrossBand::Scar)
        );
    }
}
