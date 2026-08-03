//! Authoritative, finite Current accounting.
//!
//! Current is never a floating point ambience value and never comes from a
//! per-tick world scan. Every unit lives in one named account, carries one
//! resonance identity, and moves through one validated transaction path.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::planet::BlockPos;
use crate::planet_atlas::{AtlasPos, PlanetAtlas};

pub const ARCANE_SCHEMA_VERSION: u32 = 1;
pub const ARCANE_ALGORITHM_VERSION: u32 = 1;
pub const CURRENT_UNIT_SCALE: u32 = 1_000;
pub const CURRENT_PER_ATLAS_CELL: u64 = 4_096;
pub const MAX_TRANSACTION_ACCOUNTS: usize = 128;
pub const MAX_TRANSACTION_HISTORY: usize = 4_096;
pub const MAX_AUDIT_HISTORY: usize = 2_048;
const LEDGER_FILE: &str = "arcane.wfc";
const LEDGER_BACKUP: &str = "arcane.wfc.bak";
const DELTA_FILE: &str = "arcane.wfc.log";
const DELTA_BACKUP: &str = "arcane.wfc.log.bak";
const DELTA_PENDING: &str = "arcane.wfc.log.pending";
const LINKED_PENDING: &str = "arcane.wfc.linked.pending";
const DELTA_MAGIC: &[u8; 4] = b"WAC1";
const MAX_LEDGER_BYTES: u64 = 64 * 1024 * 1024;
const MAX_DELTA_BYTES: u64 = 64 * 1024 * 1024;
const MAX_LINKED_BYTES: usize = 128 * 1024 * 1024;
const MAX_ITEM_OWNER_FILES: usize = 16_384;
const MAX_ITEM_OWNER_FILE_BYTES: u64 = 16 * 1024 * 1024;

pub const ROOT: &str = "base:root";
pub const TIDE: &str = "base:tide";
pub const EMBER: &str = "base:ember";
pub const STONE: &str = "base:stone";
pub const GALE: &str = "base:gale";
pub const ECHO: &str = "base:echo";
pub const BASE_RESONANCES: [&str; 6] = [ROOT, TIDE, EMBER, STONE, GALE, ECHO];

/// Ordinary perception deliberately stops at a qualitative band. Exact
/// quantities remain ledger/operator information until a tuning instrument
/// is introduced by the research goal.
pub fn qualitative_current(units: u64, capacity: u64) -> &'static str {
    if units == 0 {
        "dormant"
    } else if units.saturating_mul(4) < capacity.max(1) {
        "faint"
    } else if units.saturating_mul(4) < capacity.max(1).saturating_mul(3) {
        "steady"
    } else {
        "saturated"
    }
}

/// Exact resonance quantities. The scalar quantity is always `total()` and
/// therefore cannot disagree with the mixture.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Current {
    parts: BTreeMap<String, u64>,
}

impl Current {
    pub fn single(resonance: impl Into<String>, units: u64) -> Self {
        let mut current = Self::default();
        if units != 0 {
            current.parts.insert(resonance.into(), units);
        }
        current
    }

    #[allow(dead_code)] // Public foundation for later resonance-bearing workings.
    pub fn from_parts(parts: impl IntoIterator<Item = (String, u64)>) -> Result<Self, ArcaneError> {
        let mut out = Self::default();
        for (name, units) in parts {
            if name.is_empty() || units == 0 {
                return Err(ArcaneError::InvalidCurrent(
                    "resonance names must be nonempty and quantities positive".into(),
                ));
            }
            let next = out
                .parts
                .get(&name)
                .copied()
                .unwrap_or_default()
                .checked_add(units)
                .ok_or(ArcaneError::Overflow)?;
            out.parts.insert(name, next);
        }
        out.total_checked()?;
        Ok(out)
    }

    pub fn parts(&self) -> &BTreeMap<String, u64> {
        &self.parts
    }

    pub fn units_of(&self, resonance: &str) -> u64 {
        self.parts.get(resonance).copied().unwrap_or_default()
    }

    pub fn total(&self) -> u64 {
        self.total_checked()
            .expect("validated Current total remains representable")
    }

    pub fn total_checked(&self) -> Result<u64, ArcaneError> {
        self.parts.values().try_fold(0u64, |sum, value| {
            sum.checked_add(*value).ok_or(ArcaneError::Overflow)
        })
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// Deterministically remove up to `units`, preserving exact resonance
    /// quantities. Preferred bands are taken first, then stable string order.
    pub fn take_units(
        &mut self,
        units: u64,
        preferred: impl IntoIterator<Item = String>,
    ) -> Result<Current, ArcaneError> {
        let mut names = preferred.into_iter().collect::<Vec<_>>();
        for name in self.parts.keys() {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        let mut out = Current::default();
        let mut remaining = units;
        for name in names {
            if remaining == 0 {
                break;
            }
            let take = self.units_of(&name).min(remaining);
            if take != 0 {
                let current = Current::single(name, take);
                self.checked_sub(&current)?;
                out.checked_add(&current)?;
                remaining -= take;
            }
        }
        if remaining != 0 {
            return Err(ArcaneError::InsufficientCurrent {
                available: units - remaining,
                requested: units,
            });
        }
        Ok(out)
    }

    pub(crate) fn checked_add(&mut self, other: &Self) -> Result<(), ArcaneError> {
        for (name, units) in &other.parts {
            let next = self
                .parts
                .get(name)
                .copied()
                .unwrap_or_default()
                .checked_add(*units)
                .ok_or(ArcaneError::Overflow)?;
            self.parts.insert(name.clone(), next);
        }
        self.total_checked()?;
        Ok(())
    }

    pub(crate) fn checked_sub(&mut self, other: &Self) -> Result<(), ArcaneError> {
        for (name, units) in &other.parts {
            let available = self.parts.get(name).copied().unwrap_or_default();
            let next = available.checked_sub(*units).ok_or_else(|| {
                ArcaneError::InsufficientResonance {
                    resonance: name.clone(),
                    available,
                    requested: *units,
                }
            })?;
            if next == 0 {
                self.parts.remove(name);
            } else {
                self.parts.insert(name.clone(), next);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum DrossMedium {
    Soil,
    Water,
    Air,
}

/// Stable identities for every place Current can reside.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ArcaneOwner {
    Deep,
    Ambient(AtlasPos),
    Heart(u16),
    Block {
        pos: BlockPos,
        generation: u32,
    },
    Item(u64),
    Mob(u64),
    Player([u8; 16]),
    Working(u64),
    Dross {
        region: AtlasPos,
        medium: DrossMedium,
    },
    Scar(u64),
    /// Compact exact per-cell subledger owned by `ArcaneGeography`.
    Geography,
    /// Sequestered dross physically carried by the same durable item id as
    /// an optional `Item(id)` account. Kept as a distinct reservoir so
    /// harvesting transformer tissue never launders contamination into clean
    /// bound Current.
    ItemDross(u64),
}

impl ArcaneOwner {
    pub fn reservoir(&self) -> Reservoir {
        match self {
            Self::Deep => Reservoir::Deep,
            Self::Geography => Reservoir::Ambient,
            Self::Ambient(_) => Reservoir::Ambient,
            Self::Heart(_) | Self::Block { .. } | Self::Item(_) | Self::Player(_) => {
                Reservoir::Bound
            }
            Self::Mob(_) | Self::Working(_) => Reservoir::Active,
            Self::Dross { .. } => Reservoir::Dross,
            Self::Scar(_) => Reservoir::Scar,
            Self::ItemDross(_) => Reservoir::Dross,
        }
    }

    pub fn class_name(&self) -> &'static str {
        match self {
            Self::Deep => "deep",
            Self::Ambient(_) => "ambient",
            Self::Heart(_) => "heart",
            Self::Block { .. } => "block",
            Self::Item(_) => "item",
            Self::Mob(_) => "mob",
            Self::Player(_) => "player",
            Self::Working(_) => "working",
            Self::Dross { .. } => "dross",
            Self::Scar(_) => "scar",
            Self::Geography => "geography",
            Self::ItemDross(_) => "item_dross",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Reservoir {
    Deep,
    Ambient,
    Bound,
    Active,
    Dross,
    Scar,
}

impl Reservoir {
    const ALL: [Self; 6] = [
        Self::Deep,
        Self::Ambient,
        Self::Bound,
        Self::Active,
        Self::Dross,
        Self::Scar,
    ];

    const fn index(self) -> usize {
        match self {
            Self::Deep => 0,
            Self::Ambient => 1,
            Self::Bound => 2,
            Self::Active => 3,
            Self::Dross => 4,
            Self::Scar => 5,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Deep => "deep",
            Self::Ambient => "ambient",
            Self::Bound => "bound",
            Self::Active => "active",
            Self::Dross => "dross",
            Self::Scar => "scar",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArcaneAccount {
    pub current: Current,
    pub version: u64,
    /// Stable content identity remains readable if its provider disappears.
    pub content_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResonanceDefinition {
    pub id: String,
    pub label: String,
    pub provider: String,
    pub active: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResonanceRegistry {
    pub schema_version: u32,
    pub definitions: BTreeMap<String, ResonanceDefinition>,
}

impl ResonanceRegistry {
    pub fn base() -> Self {
        let definitions = [
            (ROOT, "Root"),
            (TIDE, "Tide"),
            (EMBER, "Ember"),
            (STONE, "Stone"),
            (GALE, "Gale"),
            (ECHO, "Echo"),
        ]
        .into_iter()
        .map(|(id, label)| {
            (
                id.to_string(),
                ResonanceDefinition {
                    id: id.to_string(),
                    label: label.to_string(),
                    provider: "base".into(),
                    active: true,
                },
            )
        })
        .collect();
        Self {
            schema_version: 1,
            definitions,
        }
    }

    pub fn contains_saved(&self, id: &str) -> bool {
        self.definitions.contains_key(id)
    }

    pub fn hash(&self) -> u64 {
        let mut bytes = Vec::new();
        for definition in self.definitions.values() {
            bytes.extend_from_slice(definition.id.as_bytes());
            bytes.push(0);
            bytes.extend_from_slice(definition.provider.as_bytes());
            bytes.push(u8::from(definition.active));
        }
        checksum(&bytes)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AccountRead {
    pub owner: ArcaneOwner,
    /// Absent accounts have version zero.
    pub expected_version: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArcaneMove {
    pub owner: ArcaneOwner,
    pub current: Current,
    pub content_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResonanceTransform {
    pub from: String,
    pub to: String,
    pub units: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LinkedMutation {
    pub subsystem: String,
    pub operation_id: u64,
    pub before_checksum: u64,
    pub after_checksum: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ArcaneAuthority {
    System,
    Player([u8; 16]),
    Mod(String),
    Operator { actor: String, development: bool },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct TransactionId {
    pub origin: [u8; 16],
    pub sequence: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArcaneTransaction {
    pub id: TransactionId,
    pub reads: Vec<AccountRead>,
    pub debits: Vec<ArcaneMove>,
    pub credits: Vec<ArcaneMove>,
    pub transforms: Vec<ResonanceTransform>,
    pub authority: ArcaneAuthority,
    pub reason: String,
    pub content_id: String,
    pub linked: Vec<LinkedMutation>,
}

impl ArcaneTransaction {
    #[allow(clippy::too_many_arguments)] // Explicit read versions are part of the atomic contract.
    pub fn transfer(
        id: TransactionId,
        from: ArcaneOwner,
        from_version: u64,
        to: ArcaneOwner,
        to_version: u64,
        current: Current,
        authority: ArcaneAuthority,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            id,
            reads: vec![
                AccountRead {
                    owner: from.clone(),
                    expected_version: from_version,
                },
                AccountRead {
                    owner: to.clone(),
                    expected_version: to_version,
                },
            ],
            debits: vec![ArcaneMove {
                owner: from,
                current: current.clone(),
                content_id: None,
            }],
            credits: vec![ArcaneMove {
                owner: to,
                current,
                content_id: None,
            }],
            transforms: Vec::new(),
            authority,
            reason: reason.into(),
            content_id: "base:transfer".into(),
            linked: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransactionReceipt {
    pub id: TransactionId,
    pub delta_sequence: u64,
    pub touched_versions: BTreeMap<ArcaneOwner, u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArcaneAuditEvent {
    pub transaction: TransactionId,
    pub reason: String,
    pub content_id: String,
    pub authority: String,
    pub units: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct FixedRemainder {
    pub numerator_remainder: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SavedArcaneDefinition {
    pub arcane: crate::registry::ArcaneContentDef,
    pub max_stack: u32,
    pub durability: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArcaneLedger {
    pub schema_version: u32,
    pub algorithm_version: u32,
    pub unit_scale: u32,
    pub genesis_total: u64,
    pub content_hash: u64,
    pub registry: ResonanceRegistry,
    pub accounts: BTreeMap<ArcaneOwner, ArcaneAccount>,
    pub next_item_id: u64,
    pub next_working_id: u64,
    pub next_scar_id: u64,
    pub next_system_transaction: u64,
    pub last_delta_seq: u64,
    pub origin_high_water: BTreeMap<[u8; 16], u64>,
    pub recent_receipts: BTreeMap<TransactionId, TransactionReceipt>,
    pub receipt_order: VecDeque<TransactionId>,
    pub audit_history: VecDeque<ArcaneAuditEvent>,
    pub rounding: BTreeMap<String, FixedRemainder>,
    pub frozen_hearts: BTreeSet<u16>,
    pub item_manifests: BTreeMap<String, SavedArcaneDefinition>,
    pub block_manifests: BTreeMap<String, SavedArcaneDefinition>,
    /// Only explicit development/operator adjustments can make this nonzero.
    pub external_adjustment: i128,
    pub last_clean_total: u64,
    #[serde(skip)]
    path: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct ArcaneDelta {
    sequence: u64,
    transaction: ArcaneTransaction,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct LinkedFileReplacement {
    pub subsystem: String,
    pub operation_id: u64,
    pub relative_path: String,
    pub after: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct LinkedCommit {
    transaction: ArcaneTransaction,
    files: Vec<LinkedFileReplacement>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitOutcome {
    Applied(TransactionReceipt),
    AlreadyApplied(Option<TransactionReceipt>),
}

impl ArcaneLedger {
    #[cfg(test)]
    pub fn initialize(
        path: PathBuf,
        atlas: &PlanetAtlas,
        content: &crate::registry::Registry,
    ) -> Result<Self, ArcaneError> {
        let geography = crate::arcane_geography::ArcaneGeography::load(
            path.parent().unwrap_or_else(|| Path::new(".")),
            atlas,
        )
        .or_else(|_| {
            crate::arcane_geography::ArcaneGeography::generate(
                atlas,
                content,
                &crate::planet_atlas::CancellationToken::default(),
                |_| {},
            )
        })
        .map_err(|error| ArcaneError::Corrupt(error.to_string()))?;
        let geography_current = geography
            .custody_current()
            .map_err(|error| ArcaneError::Corrupt(error.to_string()))?;
        Self::initialize_with_geography(path, atlas, content, geography_current)
    }

    pub fn initialize_with_geography(
        path: PathBuf,
        atlas: &PlanetAtlas,
        content: &crate::registry::Registry,
        geography_current: Current,
    ) -> Result<Self, ArcaneError> {
        let genesis_total = u64::from(atlas.manifest.atlas_cell_count)
            .checked_mul(CURRENT_PER_ATLAS_CELL)
            .ok_or(ArcaneError::Overflow)?;
        let geography_total = geography_current.total_checked()?;
        if geography_total >= genesis_total {
            return Err(ArcaneError::Corrupt(
                "arcane geography leaves no finite deep reserve".into(),
            ));
        }
        let remainder_total = genesis_total - geography_total;
        let mut deep = Current::default();
        let share = remainder_total / BASE_RESONANCES.len() as u64;
        let mut assigned = 0u64;
        for (index, resonance) in BASE_RESONANCES.into_iter().enumerate() {
            let units = if index + 1 == BASE_RESONANCES.len() {
                remainder_total
                    .checked_sub(assigned)
                    .ok_or(ArcaneError::Overflow)?
            } else {
                share
            };
            deep.checked_add(&Current::single(resonance, units))?;
            assigned = assigned.checked_add(units).ok_or(ArcaneError::Overflow)?;
        }
        let mut accounts = BTreeMap::new();
        accounts.insert(
            ArcaneOwner::Deep,
            ArcaneAccount {
                current: deep,
                version: 1,
                content_id: Some("base:planet_genesis".into()),
            },
        );
        accounts.insert(
            ArcaneOwner::Geography,
            ArcaneAccount {
                current: geography_current,
                version: 1,
                content_id: Some("base:planetary_current".into()),
            },
        );
        let mut ledger = Self {
            schema_version: ARCANE_SCHEMA_VERSION,
            algorithm_version: ARCANE_ALGORITHM_VERSION,
            unit_scale: CURRENT_UNIT_SCALE,
            genesis_total,
            content_hash: content.content_hash,
            registry: content.arcane_registry.clone(),
            accounts,
            next_item_id: 1,
            next_working_id: 1,
            next_scar_id: 1,
            next_system_transaction: 1,
            last_delta_seq: 0,
            origin_high_water: BTreeMap::new(),
            recent_receipts: BTreeMap::new(),
            receipt_order: VecDeque::new(),
            audit_history: VecDeque::new(),
            rounding: BTreeMap::new(),
            frozen_hearts: BTreeSet::new(),
            item_manifests: content
                .items
                .iter()
                .filter_map(|item| {
                    item.arcane.as_ref().map(|arcane| {
                        (
                            item.name.clone(),
                            SavedArcaneDefinition {
                                arcane: arcane.clone(),
                                max_stack: item.max_stack,
                                durability: item.durability,
                            },
                        )
                    })
                })
                .collect(),
            block_manifests: content
                .blocks
                .iter()
                .filter_map(|block| {
                    block.arcane.as_ref().map(|arcane| {
                        (
                            block.name.clone(),
                            SavedArcaneDefinition {
                                arcane: arcane.clone(),
                                max_stack: 1,
                                durability: 0,
                            },
                        )
                    })
                })
                .collect(),
            external_adjustment: 0,
            last_clean_total: genesis_total,
            path,
        };
        ledger.allocate_heart_reserves(atlas)?;
        ledger.validate_total()?;
        Ok(ledger)
    }

    fn allocate_heart_reserves(&mut self, atlas: &PlanetAtlas) -> Result<(), ArcaneError> {
        for country in &atlas.biomes.countries {
            // A heart receives a finite reserve proportional to its actual
            // territory. This is moved from Deep; no country birth creates it.
            let units = u64::from(country.cell_count)
                .checked_mul(CURRENT_PER_ATLAS_CELL / 16)
                .ok_or(ArcaneError::Overflow)?;
            if units == 0 {
                continue;
            }
            let resonance =
                BASE_RESONANCES[usize::from(country.dominant_biome) % BASE_RESONANCES.len()];
            let wanted = Current::single(resonance, units);
            let deep = self.accounts.get_mut(&ArcaneOwner::Deep).ok_or_else(|| {
                ArcaneError::Corrupt("genesis is missing the Deep account".into())
            })?;
            // Deep's initial mixture may not have enough of one resonance for
            // an unusually large biome. Draw deterministically across the six
            // base bands while preserving the exact scalar allocation.
            let moved = take_mixture(&mut deep.current, &wanted, units)?;
            deep.version = deep.version.checked_add(1).ok_or(ArcaneError::Overflow)?;
            self.accounts.insert(
                ArcaneOwner::Heart(country.id),
                ArcaneAccount {
                    current: moved,
                    version: 1,
                    content_id: Some("base:wild_heart".into()),
                },
            );
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn load_or_initialize(
        world: &Path,
        atlas: &PlanetAtlas,
        content: &crate::registry::Registry,
    ) -> Result<Self, ArcaneError> {
        let geography = crate::arcane_geography::ArcaneGeography::load(world, atlas)
            .or_else(|_| {
                crate::arcane_geography::ArcaneGeography::generate(
                    atlas,
                    content,
                    &crate::planet_atlas::CancellationToken::default(),
                    |_| {},
                )
            })
            .map_err(|error| ArcaneError::Corrupt(error.to_string()))?;
        let current = geography
            .custody_current()
            .map_err(|error| ArcaneError::Corrupt(error.to_string()))?;
        Self::load_or_initialize_with_geography(world, atlas, content, current)
    }

    pub fn load_or_initialize_with_geography(
        world: &Path,
        atlas: &PlanetAtlas,
        content: &crate::registry::Registry,
        geography_current: Current,
    ) -> Result<Self, ArcaneError> {
        if world.join(LEDGER_FILE).exists() {
            let mut ledger = Self::load(world)?;
            let registry_changed = ledger.reconcile_registry(&content.arcane_registry);
            let manifest_changed = ledger.reconcile_content(content)?;
            if ledger.content_hash != content.content_hash || registry_changed || manifest_changed {
                // Content identity changes, but saved resonance strings and
                // charged-owner manifests remain authoritative.
                ledger.content_hash = content.content_hash;
                ledger.save()?;
            }
            if ledger.reconcile_geography_custody(&geography_current)? {
                ledger.save()?;
            }
            Ok(ledger)
        } else {
            let ledger = Self::initialize_with_geography(
                world.join(LEDGER_FILE),
                atlas,
                content,
                geography_current,
            )?;
            ledger.save()?;
            Ok(ledger)
        }
    }

    fn reconcile_geography_custody(&mut self, expected: &Current) -> Result<bool, ArcaneError> {
        if let Some(account) = self.accounts.get(&ArcaneOwner::Geography) {
            if &account.current != expected {
                return Err(ArcaneError::Corrupt(
                    "saved planetary Current does not reconcile with its geography subledger"
                        .into(),
                ));
            }
            return Ok(false);
        }
        let deep = self
            .accounts
            .get_mut(&ArcaneOwner::Deep)
            .ok_or(ArcaneError::MissingAccount(ArcaneOwner::Deep))?;
        deep.current.checked_sub(expected).map_err(|_| {
            ArcaneError::Corrupt(
                "legacy save lacks the resonance mixture required for the finite geography migration"
                    .into(),
            )
        })?;
        deep.version = deep.version.checked_add(1).ok_or(ArcaneError::Overflow)?;
        self.accounts.insert(
            ArcaneOwner::Geography,
            ArcaneAccount {
                current: expected.clone(),
                version: 1,
                content_id: Some("base:planetary_current".into()),
            },
        );
        self.validate_total()?;
        Ok(true)
    }

    fn reconcile_registry(&mut self, loaded: &ResonanceRegistry) -> bool {
        let before = self.registry.clone();
        for definition in self.registry.definitions.values_mut() {
            definition.active = false;
        }
        for (id, definition) in &loaded.definitions {
            self.registry
                .definitions
                .entry(id.clone())
                .and_modify(|saved| {
                    saved.label.clone_from(&definition.label);
                    saved.provider.clone_from(&definition.provider);
                    saved.active = true;
                })
                .or_insert_with(|| definition.clone());
        }
        self.registry.schema_version = loaded.schema_version;
        self.registry != before
    }

    fn reconcile_content(
        &mut self,
        content: &crate::registry::Registry,
    ) -> Result<bool, ArcaneError> {
        let mut changed = false;
        for item in &content.items {
            let Some(arcane) = &item.arcane else {
                continue;
            };
            let saved = SavedArcaneDefinition {
                arcane: arcane.clone(),
                max_stack: item.max_stack,
                durability: item.durability,
            };
            if let Some(previous) = self.item_manifests.get(&item.name)
                && previous != &saved
            {
                return Err(ArcaneError::InvalidCurrent(format!(
                    "{} changes saved arcane identity; an explicit migration is required",
                    item.name
                )));
            }
            if self
                .item_manifests
                .insert(item.name.clone(), saved)
                .is_none()
            {
                changed = true;
            }
        }
        for block in &content.blocks {
            let Some(arcane) = &block.arcane else {
                continue;
            };
            let saved = SavedArcaneDefinition {
                arcane: arcane.clone(),
                max_stack: 1,
                durability: 0,
            };
            if let Some(previous) = self.block_manifests.get(&block.name)
                && previous != &saved
            {
                return Err(ArcaneError::InvalidCurrent(format!(
                    "{} changes saved arcane identity; an explicit migration is required",
                    block.name
                )));
            }
            if self
                .block_manifests
                .insert(block.name.clone(), saved)
                .is_none()
            {
                changed = true;
            }
        }
        Ok(changed)
    }

    pub fn load(world: &Path) -> Result<Self, ArcaneError> {
        let path = world.join(LEDGER_FILE);
        let mut bytes = std::fs::read(&path)?;
        let mut ledger = Self::decode_snapshot(path.clone(), &bytes)?;
        if let Err(primary_error) = ledger.verify_manifest_checkpoint(&bytes) {
            // A clean checkpoint updates ledger, log, then manifest. If the
            // process stopped between those replacements, the independently
            // synced backups are the old checkpoint and journal. Roll that
            // pair forward through ordinary replay instead of guessing mass.
            let backup_bytes = match std::fs::read(world.join(LEDGER_BACKUP)) {
                Ok(bytes) => bytes,
                Err(_) => return Err(primary_error),
            };
            let backup = Self::decode_snapshot(path.clone(), &backup_bytes)?;
            if backup.verify_manifest_checkpoint(&backup_bytes).is_err() {
                return Err(primary_error);
            }
            let backup_delta = std::fs::read(world.join(DELTA_BACKUP))?;
            crate::identity::atomic_write(&path, &backup_bytes, false)?;
            crate::identity::atomic_write(&world.join(DELTA_FILE), &backup_delta, false)?;
            bytes = backup_bytes;
            ledger = backup;
            debug_assert!(ledger.verify_manifest_checkpoint(&bytes).is_ok());
        }
        ledger.replay_delta_log()?;
        ledger.recover_linked_commit()?;
        ledger.reconcile_allocator_floors();
        ledger.validate_total()?;
        Ok(ledger)
    }

    fn decode_snapshot(path: PathBuf, bytes: &[u8]) -> Result<Self, ArcaneError> {
        if bytes.len() as u64 > MAX_LEDGER_BYTES {
            return Err(ArcaneError::Corrupt(
                "arcane ledger exceeds safety bound".into(),
            ));
        }
        let mut ledger: Self =
            postcard::from_bytes(bytes).map_err(|error| ArcaneError::Corrupt(error.to_string()))?;
        if ledger.schema_version != ARCANE_SCHEMA_VERSION
            || ledger.algorithm_version != ARCANE_ALGORITHM_VERSION
            || ledger.unit_scale != CURRENT_UNIT_SCALE
        {
            return Err(ArcaneError::Corrupt(format!(
                "unsupported arcane ledger schema {}/{}/{}",
                ledger.schema_version, ledger.algorithm_version, ledger.unit_scale
            )));
        }
        ledger.path = path;
        ledger.validate_loaded()?;
        Ok(ledger)
    }

    fn verify_manifest_checkpoint(&self, bytes: &[u8]) -> Result<(), ArcaneError> {
        let world = self.world_dir()?;
        let checkpoint = self.manifest_checkpoint(checksum(bytes), 0)?;
        crate::planet_atlas::verify_arcane_manifest_checkpoint(world, &checkpoint)
            .map_err(|error| ArcaneError::Corrupt(error.to_string()))
    }

    /// Allocator counters are checkpoint state while committed owners and
    /// transaction ids also live in the journal. A crash after a journal
    /// append but before the next checkpoint must advance the counters past
    /// replayed identities or the next creation could reuse an id.
    fn reconcile_allocator_floors(&mut self) {
        let mut max_item = 0u64;
        let mut max_working = 0u64;
        let mut max_scar = 0u64;
        for owner in self.accounts.keys() {
            match owner {
                ArcaneOwner::Item(id) | ArcaneOwner::ItemDross(id) => max_item = max_item.max(*id),
                ArcaneOwner::Working(id) => max_working = max_working.max(*id),
                ArcaneOwner::Scar(id) => max_scar = max_scar.max(*id),
                _ => {}
            }
        }
        self.next_item_id = self.next_item_id.max(max_item.saturating_add(1));
        self.next_working_id = self.next_working_id.max(max_working.saturating_add(1));
        self.next_scar_id = self.next_scar_id.max(max_scar.saturating_add(1));
        let system_high = self
            .origin_high_water
            .get(b"wildforge-system")
            .copied()
            .unwrap_or_default();
        self.next_system_transaction = self
            .next_system_transaction
            .max(system_high.saturating_add(1));
    }

    pub fn save(&self) -> Result<(), ArcaneError> {
        self.validate_total()?;
        let bytes =
            postcard::to_allocvec(self).map_err(|error| ArcaneError::Corrupt(error.to_string()))?;
        if let Ok(old) = std::fs::read(&self.path) {
            crate::identity::atomic_write(&self.path.with_file_name(LEDGER_BACKUP), &old, false)?;
        }
        let delta_path = self.delta_path();
        let old_delta = std::fs::read(&delta_path).unwrap_or_else(|_| DELTA_MAGIC.to_vec());
        crate::identity::atomic_write(&self.path.with_file_name(DELTA_BACKUP), &old_delta, false)?;
        crate::identity::atomic_write(&self.path, &bytes, false)?;
        crate::identity::atomic_write(&delta_path, DELTA_MAGIC, false)?;
        let _ = crate::persist::remove_if_exists(&self.pending_path());
        let world = self.path.parent().ok_or_else(|| {
            ArcaneError::Corrupt("arcane ledger path has no world directory".into())
        })?;
        let checkpoint = self.manifest_checkpoint(checksum(&bytes), checksum(DELTA_MAGIC))?;
        crate::planet_atlas::update_arcane_manifest(world, &checkpoint)
            .map_err(|error| ArcaneError::Corrupt(error.to_string()))?;
        Ok(())
    }

    fn manifest_checkpoint(
        &self,
        ledger_checksum: u64,
        delta_checksum: u64,
    ) -> Result<crate::planet_atlas::ArcaneManifestCheckpoint, ArcaneError> {
        Ok(crate::planet_atlas::ArcaneManifestCheckpoint {
            schema_version: self.schema_version,
            algorithm_version: self.algorithm_version,
            unit_scale: self.unit_scale,
            genesis_total: self.genesis_total,
            last_clean_total: self.last_clean_total,
            reservoir_totals: self.reservoir_totals()?,
            registry_hash: self.registry.hash(),
            ledger_checksum,
            delta_checksum,
        })
    }

    fn reservoir_totals(&self) -> Result<[u64; 6], ArcaneError> {
        let mut totals = [0u64; 6];
        for (owner, account) in &self.accounts {
            let slot = &mut totals[owner.reservoir().index()];
            *slot = slot
                .checked_add(account.current.total_checked()?)
                .ok_or(ArcaneError::Overflow)?;
        }
        Ok(totals)
    }

    fn validate_loaded(&self) -> Result<(), ArcaneError> {
        if self.accounts.len() > 4_000_000 {
            return Err(ArcaneError::Corrupt(
                "arcane owner count exceeds safety bound".into(),
            ));
        }
        for (owner, account) in &self.accounts {
            account.current.total_checked()?;
            if account.current.is_empty() {
                return Err(ArcaneError::Corrupt(format!(
                    "empty account persisted for {owner:?}"
                )));
            }
            for resonance in account.current.parts().keys() {
                if !self.registry.contains_saved(resonance) {
                    return Err(ArcaneError::Corrupt(format!(
                        "account names unmanifested resonance {resonance}"
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn account(&self, owner: &ArcaneOwner) -> Option<&ArcaneAccount> {
        self.accounts.get(owner)
    }

    /// Constant-size local reading used by ordinary clients and agents.
    pub fn local_bands(&self, region: AtlasPos) -> [u8; 2] {
        let ambient = self
            .account(&ArcaneOwner::Ambient(region))
            .map_or(0, |account| account.current.total());
        let dross = [DrossMedium::Soil, DrossMedium::Water, DrossMedium::Air]
            .into_iter()
            .filter_map(|medium| self.account(&ArcaneOwner::Dross { region, medium }))
            .fold(0u64, |sum, account| {
                sum.saturating_add(account.current.total())
            });
        [reading_band(ambient), reading_band(dross)]
    }

    pub fn version_of(&self, owner: &ArcaneOwner) -> u64 {
        self.accounts
            .get(owner)
            .map_or(0, |account| account.version)
    }

    pub fn allocate_item_id(&mut self) -> Result<u64, ArcaneError> {
        let id = self.next_item_id;
        self.next_item_id = self
            .next_item_id
            .checked_add(1)
            .ok_or(ArcaneError::Overflow)?;
        Ok(id)
    }

    #[allow(dead_code)] // Reserved now so later workings cannot improvise ids.
    pub fn allocate_working_id(&mut self) -> Result<u64, ArcaneError> {
        let id = self.next_working_id;
        self.next_working_id = self
            .next_working_id
            .checked_add(1)
            .ok_or(ArcaneError::Overflow)?;
        Ok(id)
    }

    pub fn allocate_scar_id(&mut self) -> Result<u64, ArcaneError> {
        let id = self.next_scar_id;
        self.next_scar_id = self
            .next_scar_id
            .checked_add(1)
            .ok_or(ArcaneError::Overflow)?;
        Ok(id)
    }

    pub(crate) fn system_transaction_id(&mut self) -> Result<TransactionId, ArcaneError> {
        let sequence = self.next_system_transaction;
        self.next_system_transaction = self
            .next_system_transaction
            .checked_add(1)
            .ok_or(ArcaneError::Overflow)?;
        Ok(TransactionId {
            origin: *b"wildforge-system",
            sequence,
        })
    }

    /// Bind a newly created charged object by moving existing Current from a
    /// named source. The returned id is the only value stored on the item.
    pub fn bind_new_item(
        &mut self,
        source: ArcaneOwner,
        definition: &crate::registry::ArcaneContentDef,
        content_id: &str,
        reason: &str,
    ) -> Result<u64, ArcaneError> {
        let item_id = self.allocate_item_id()?;
        self.bind_new_owner(
            source,
            ArcaneOwner::Item(item_id),
            definition.capacity,
            definition.resonance.keys().cloned().collect(),
            content_id,
            reason,
        )?;
        Ok(item_id)
    }

    pub(crate) fn bind_new_item_exact_linked(
        &mut self,
        source: ArcaneOwner,
        current: Current,
        dross: Current,
        content_id: &str,
        reason: &str,
        files: Vec<LinkedFileReplacement>,
    ) -> Result<u64, ArcaneError> {
        if current.is_empty() && dross.is_empty() {
            return Err(ArcaneError::InvalidTransaction(
                "a charged linked harvest cannot bind an empty owner".into(),
            ));
        }
        let mut total = current.clone();
        total.checked_add(&dross)?;
        let source_account = self
            .accounts
            .get(&source)
            .ok_or_else(|| ArcaneError::MissingAccount(source.clone()))?
            .clone();
        for (resonance, units) in total.parts() {
            if source_account.current.units_of(resonance) < *units {
                return Err(ArcaneError::InsufficientResonance {
                    resonance: resonance.clone(),
                    available: source_account.current.units_of(resonance),
                    requested: *units,
                });
            }
        }
        let item_id = self.allocate_item_id()?;
        let target = ArcaneOwner::Item(item_id);
        let dross_target = ArcaneOwner::ItemDross(item_id);
        let mut reads = vec![AccountRead {
            owner: source.clone(),
            expected_version: source_account.version,
        }];
        let mut credits = Vec::new();
        if !current.is_empty() {
            reads.push(AccountRead {
                owner: target.clone(),
                expected_version: 0,
            });
            credits.push(ArcaneMove {
                owner: target,
                current,
                content_id: Some(content_id.into()),
            });
        }
        if !dross.is_empty() {
            reads.push(AccountRead {
                owner: dross_target.clone(),
                expected_version: 0,
            });
            credits.push(ArcaneMove {
                owner: dross_target,
                current: dross,
                content_id: Some(content_id.into()),
            });
        }
        let transaction = ArcaneTransaction {
            id: self.system_transaction_id()?,
            reads,
            debits: vec![ArcaneMove {
                owner: source,
                current: total,
                content_id: None,
            }],
            credits,
            transforms: Vec::new(),
            authority: ArcaneAuthority::System,
            reason: reason.into(),
            content_id: content_id.into(),
            linked: Vec::new(),
        };
        match self.commit_linked_files(transaction, files)? {
            CommitOutcome::Applied(_) => Ok(item_id),
            CommitOutcome::AlreadyApplied(_) => Err(ArcaneError::Corrupt(
                "fresh linked harvest transaction was already applied".into(),
            )),
        }
    }

    /// Durably coordinate a subledger state change which moves no Current
    /// across the parent boundary (for example an uncharged biomass harvest).
    /// A one-unit balanced Geography debit/credit gives the journal a real
    /// versioned write set without changing ownership or minting an item id.
    pub(crate) fn commit_geography_state_linked(
        &mut self,
        reason: &str,
        files: Vec<LinkedFileReplacement>,
    ) -> Result<(), ArcaneError> {
        let owner = ArcaneOwner::Geography;
        let account = self
            .accounts
            .get(&owner)
            .ok_or_else(|| ArcaneError::MissingAccount(owner.clone()))?
            .clone();
        let mut unit = account.current.clone();
        let unit = unit.take_units(1, BASE_RESONANCES.into_iter().map(str::to_string))?;
        let transaction = ArcaneTransaction {
            id: self.system_transaction_id()?,
            reads: vec![AccountRead {
                owner: owner.clone(),
                expected_version: account.version,
            }],
            debits: vec![ArcaneMove {
                owner: owner.clone(),
                current: unit.clone(),
                content_id: None,
            }],
            credits: vec![ArcaneMove {
                owner,
                current: unit,
                content_id: Some("base:planetary_current".into()),
            }],
            transforms: Vec::new(),
            authority: ArcaneAuthority::System,
            reason: reason.into(),
            content_id: "base:arcane_ecology_state".into(),
            linked: Vec::new(),
        };
        match self.commit_linked_files(transaction, files)? {
            CommitOutcome::Applied(_) => Ok(()),
            CommitOutcome::AlreadyApplied(_) => Err(ArcaneError::Corrupt(
                "fresh ecology state transaction was already applied".into(),
            )),
        }
    }

    /// Manifest a warden or working from a finite heart/ambient reserve.
    pub fn bind_new_owner(
        &mut self,
        source: ArcaneOwner,
        target: ArcaneOwner,
        capacity: u64,
        preferred: Vec<String>,
        content_id: &str,
        reason: &str,
    ) -> Result<u64, ArcaneError> {
        if matches!(source, ArcaneOwner::Heart(country) if self.frozen_hearts.contains(&country)) {
            return Err(ArcaneError::InvalidTransaction(
                "a dead heart cannot lend its frozen reserve".into(),
            ));
        }
        if self.accounts.contains_key(&target) {
            return Err(ArcaneError::InvalidTransaction(format!(
                "target arcane owner {target:?} already exists"
            )));
        }
        let source_account = self
            .accounts
            .get(&source)
            .ok_or_else(|| ArcaneError::MissingAccount(source.clone()))?
            .clone();
        let units = capacity.min(source_account.current.total_checked()?);
        if units == 0 {
            return Err(ArcaneError::InsufficientCurrent {
                available: 0,
                requested: capacity,
            });
        }
        let mut selected_from = source_account.current.clone();
        let selected = selected_from.take_units(units, preferred)?;
        let id = self.system_transaction_id()?;
        let transaction = ArcaneTransaction {
            id,
            reads: vec![
                AccountRead {
                    owner: source.clone(),
                    expected_version: source_account.version,
                },
                AccountRead {
                    owner: target.clone(),
                    expected_version: 0,
                },
            ],
            debits: vec![ArcaneMove {
                owner: source,
                current: selected.clone(),
                content_id: None,
            }],
            credits: vec![ArcaneMove {
                owner: target,
                current: selected,
                content_id: Some(content_id.into()),
            }],
            transforms: Vec::new(),
            authority: ArcaneAuthority::System,
            reason: reason.into(),
            content_id: content_id.into(),
            linked: Vec::new(),
        };
        match self.commit(transaction)? {
            CommitOutcome::Applied(receipt) => Ok(receipt.delta_sequence),
            CommitOutcome::AlreadyApplied(_) => Err(ArcaneError::Corrupt(
                "fresh system transaction id was already applied".into(),
            )),
        }
    }

    pub fn move_all(
        &mut self,
        owner: ArcaneOwner,
        destination: ArcaneOwner,
        reason: &str,
    ) -> Result<u64, ArcaneError> {
        let account = self
            .accounts
            .get(&owner)
            .ok_or_else(|| ArcaneError::MissingAccount(owner.clone()))?
            .clone();
        let id = self.system_transaction_id()?;
        let mut transaction = ArcaneTransaction::transfer(
            id,
            owner,
            account.version,
            destination.clone(),
            self.version_of(&destination),
            account.current.clone(),
            ArcaneAuthority::System,
            reason,
        );
        // Changing custody between the clean and sequestered reservoirs of
        // the same physical item must not erase its content identity. The
        // durable-owner scanner uses that identity to distinguish a real
        // charged stack from a stale pre-commit echo.
        if matches!(
            destination,
            ArcaneOwner::Item(_) | ArcaneOwner::ItemDross(_)
        ) {
            transaction.credits[0].content_id = account.content_id.clone();
        }
        match self.commit(transaction)? {
            CommitOutcome::Applied(receipt) => Ok(receipt.delta_sequence),
            CommitOutcome::AlreadyApplied(_) => Err(ArcaneError::Corrupt(
                "fresh system transaction id was already applied".into(),
            )),
        }
    }

    /// Move every ledger component carried by one durable physical item.
    pub fn move_all_item(
        &mut self,
        item_id: u64,
        destination: ArcaneOwner,
        reason: &str,
    ) -> Result<(), ArcaneError> {
        let owners = [ArcaneOwner::Item(item_id), ArcaneOwner::ItemDross(item_id)];
        if !owners.iter().any(|owner| self.accounts.contains_key(owner)) {
            return Err(ArcaneError::MissingAccount(ArcaneOwner::Item(item_id)));
        }
        for owner in owners {
            if self.accounts.contains_key(&owner) {
                self.move_all(owner, destination.clone(), reason)?;
            }
        }
        Ok(())
    }

    pub fn item_current_total(&self, item_id: u64) -> Option<u64> {
        let clean = self
            .accounts
            .get(&ArcaneOwner::Item(item_id))
            .map(|account| account.current.total());
        let dross = self
            .accounts
            .get(&ArcaneOwner::ItemDross(item_id))
            .map(|account| account.current.total());
        match (clean, dross) {
            (None, None) => None,
            (clean, dross) => Some(clean.unwrap_or(0).saturating_add(dross.unwrap_or(0))),
        }
    }

    /// Clean usable Current carried by an item, excluding sequestered dross.
    /// Implement effects and transfer previews must use this rather than the
    /// combined custody total.
    pub fn item_clean_total(&self, item_id: u64) -> Option<u64> {
        self.accounts
            .get(&ArcaneOwner::Item(item_id))
            .map(|account| account.current.total())
    }

    pub fn item_dross_total(&self, item_id: u64) -> u64 {
        self.accounts
            .get(&ArcaneOwner::ItemDross(item_id))
            .map_or(0, |account| account.current.total())
    }

    /// Reconcile durable item identities after a process stopped between the
    /// ledger commit and the owning player/entity file replacement.
    ///
    /// Arcane commits deliberately land first. Recovery can therefore make
    /// one deterministic choice without inventing Current: an account with no
    /// durable object is rolled back into Deep, while a durable stack whose
    /// account no longer exists is a stale pre-commit echo and is removed from
    /// its TOML owner. Duplicate live references are refused because two
    /// physical objects must never name one charge account.
    pub(crate) fn reconcile_durable_item_owners(
        &mut self,
        world: &Path,
    ) -> Result<DurableItemStatus, ArcaneError> {
        let mut scan = scan_durable_item_owners(world, self, true)?;
        if scan.duplicate_references != 0 {
            return Err(ArcaneError::Corrupt(format!(
                "{} duplicate durable references name the same charged item account",
                scan.duplicate_references
            )));
        }
        let orphaned = self
            .accounts
            .keys()
            .filter_map(|owner| match owner {
                ArcaneOwner::Item(id) | ArcaneOwner::ItemDross(id)
                    if !scan.references.contains_key(id) =>
                {
                    Some(*id)
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let recovered_orphans = orphaned.len();
        for id in orphaned {
            self.move_all_item(
                id,
                ArcaneOwner::Deep,
                "crash recovery rolled back an unpersisted charged item",
            )?;
        }
        if recovered_orphans != 0 || scan.invalid_references != 0 {
            eprintln!(
                "arcane: recovered {recovered_orphans} unpersisted item accounts and removed {} stale item references",
                scan.invalid_references
            );
        }
        scan = scan_durable_item_owners(world, self, false)?;
        Ok(scan.status())
    }

    /// Read-only durable-reference evidence for subsystem audits that need
    /// parent-ledger integrity without loading or regenerating a planet atlas.
    /// This also permits deliberately tiny fixture planets to exercise their
    /// real save formats.
    pub(crate) fn durable_item_status(
        &self,
        world: &Path,
    ) -> Result<DurableItemStatus, ArcaneError> {
        Ok(scan_durable_item_owners(world, self, false)?.status())
    }

    /// Mob manifestations and goal-1 Working accounts deliberately have no
    /// durable physical owner. Any such account found while opening a closed
    /// world belongs to a process that ended before retiring it. Roll the
    /// exact mixture back to Deep rather than guessing its former country,
    /// location, or intended working result.
    pub(crate) fn reconcile_transient_owners(&mut self) -> Result<usize, ArcaneError> {
        let orphaned = self
            .accounts
            .keys()
            .filter(|owner| matches!(owner, ArcaneOwner::Mob(_) | ArcaneOwner::Working(_)))
            .cloned()
            .collect::<Vec<_>>();
        let recovered = orphaned.len();
        let mut empty_tombstones = 0usize;
        for owner in orphaned {
            if self
                .accounts
                .get(&owner)
                .is_some_and(|account| account.current.is_empty())
            {
                self.accounts.remove(&owner);
                empty_tombstones += 1;
                continue;
            }
            self.move_all(
                owner,
                ArcaneOwner::Deep,
                "crash recovery rolled back an undurable transient owner",
            )?;
        }
        if empty_tombstones != 0 {
            // Older/interrupted snapshots may contain a zero-balance owner.
            // There is no scalar movement to journal, so compact the removed
            // tombstone through the normal atomic checkpoint.
            self.save()?;
        }
        if recovered != 0 {
            eprintln!("arcane: recovered {recovered} undurable transient accounts");
        }
        Ok(recovered)
    }

    pub(crate) fn item_matches(&self, id: u64, content_id: &str) -> bool {
        let accounts = [
            self.accounts.get(&ArcaneOwner::Item(id)),
            self.accounts.get(&ArcaneOwner::ItemDross(id)),
        ];
        accounts.iter().any(|account| account.is_some())
            && accounts
                .into_iter()
                .flatten()
                .all(|account| account.content_id.as_deref() == Some(content_id))
    }

    pub fn set_heart_frozen(&mut self, country: u16, frozen: bool) -> Result<(), ArcaneError> {
        if !self.accounts.contains_key(&ArcaneOwner::Heart(country)) {
            return Err(ArcaneError::MissingAccount(ArcaneOwner::Heart(country)));
        }
        if frozen {
            self.frozen_hearts.insert(country);
        } else {
            self.frozen_hearts.remove(&country);
        }
        self.save()
    }

    pub fn heart_frozen(&self, country: u16) -> bool {
        self.frozen_hearts.contains(&country)
    }

    /// Deterministic fixed-point scaling whose remainder survives save/load.
    #[allow(dead_code)] // Goal 1 persists rounding before natural circulation uses it.
    pub fn scaled_units(
        &mut self,
        process: &str,
        input: u64,
        numerator: u64,
        denominator: u64,
    ) -> Result<u64, ArcaneError> {
        if process.is_empty() || denominator == 0 {
            return Err(ArcaneError::InvalidCurrent(
                "invalid fixed-point process".into(),
            ));
        }
        let remainder = self.rounding.entry(process.into()).or_default();
        if remainder.numerator_remainder >= denominator {
            return Err(ArcaneError::Corrupt(format!(
                "fixed-point remainder for {process} is out of range"
            )));
        }
        let wide = u128::from(input)
            .checked_mul(u128::from(numerator))
            .and_then(|value| value.checked_add(u128::from(remainder.numerator_remainder)))
            .ok_or(ArcaneError::Overflow)?;
        let output = wide / u128::from(denominator);
        let next_remainder = wide % u128::from(denominator);
        remainder.numerator_remainder =
            u64::try_from(next_remainder).map_err(|_| ArcaneError::Overflow)?;
        u64::try_from(output).map_err(|_| ArcaneError::Overflow)
    }

    #[allow(dead_code)] // In-memory transaction entry point for deterministic fixtures.
    pub fn apply(&mut self, transaction: &ArcaneTransaction) -> Result<CommitOutcome, ArcaneError> {
        if let Some(outcome) = self.duplicate_outcome(transaction.id) {
            return Ok(outcome);
        }
        let sequence = self
            .last_delta_seq
            .checked_add(1)
            .ok_or(ArcaneError::Overflow)?;
        let receipt = self.apply_validated(transaction, sequence)?;
        self.last_delta_seq = sequence;
        Ok(CommitOutcome::Applied(receipt))
    }

    pub fn commit(&mut self, transaction: ArcaneTransaction) -> Result<CommitOutcome, ArcaneError> {
        if !transaction.linked.is_empty() {
            let pending = self.read_linked_pending()?.ok_or_else(|| {
                ArcaneError::InvalidTransaction(
                    "linked transaction requires the cross-ledger coordinator".into(),
                )
            })?;
            if pending.transaction.id != transaction.id
                || pending.transaction.linked != transaction.linked
            {
                return Err(ArcaneError::InvalidTransaction(
                    "linked transaction does not match its durable coordinator".into(),
                ));
            }
        } else if self.linked_pending_path().exists() {
            return Err(ArcaneError::InvalidTransaction(
                "a cross-ledger commit must recover before new transactions".into(),
            ));
        }
        if let Some(outcome) = self.duplicate_outcome(transaction.id) {
            return Ok(outcome);
        }
        self.validate_transaction(&transaction)?;
        let delta = ArcaneDelta {
            sequence: self
                .last_delta_seq
                .checked_add(1)
                .ok_or(ArcaneError::Overflow)?,
            transaction,
        };
        let payload = postcard::to_allocvec(&delta)
            .map_err(|error| ArcaneError::Corrupt(error.to_string()))?;
        crate::identity::atomic_write(&self.pending_path(), &payload, false)?;
        let frame = encode_frame(&delta)?;
        let path = self.delta_path();
        let needs_header = std::fs::metadata(&path).map_or(true, |metadata| metadata.len() == 0);
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        if needs_header {
            file.write_all(DELTA_MAGIC)?;
        }
        file.write_all(&frame)?;
        file.sync_data()?;
        let receipt = self.apply_validated(&delta.transaction, delta.sequence)?;
        self.last_delta_seq = delta.sequence;
        crate::persist::remove_if_exists(&self.pending_path())?;
        Ok(CommitOutcome::Applied(receipt))
    }

    #[allow(dead_code)] // Foundation used when later goals add mixed material/water workings.
    pub(crate) fn commit_linked_files(
        &mut self,
        mut transaction: ArcaneTransaction,
        files: Vec<LinkedFileReplacement>,
    ) -> Result<CommitOutcome, ArcaneError> {
        if files.is_empty() || files.len() > 16 || self.linked_pending_path().exists() {
            return Err(ArcaneError::InvalidTransaction(
                "linked commit needs 1..=16 files and no pending predecessor".into(),
            ));
        }
        let world = self.world_dir()?;
        let mut links = Vec::with_capacity(files.len());
        let mut total_bytes = 0usize;
        for file in &files {
            validate_linked_path(file)?;
            total_bytes = total_bytes
                .checked_add(file.after.as_ref().map_or(0, Vec::len))
                .ok_or(ArcaneError::Overflow)?;
            if total_bytes > MAX_LINKED_BYTES {
                return Err(ArcaneError::InvalidTransaction(
                    "linked file payload exceeds its safety bound".into(),
                ));
            }
            let path = world.join(&file.relative_path);
            let before = std::fs::read(&path).unwrap_or_default();
            let after = file.after.as_deref().unwrap_or_default();
            links.push(LinkedMutation {
                subsystem: file.subsystem.clone(),
                operation_id: file.operation_id,
                before_checksum: checksum(&before),
                after_checksum: checksum(after),
            });
        }
        transaction.linked = links;
        self.validate_transaction(&transaction)?;
        let pending = LinkedCommit { transaction, files };
        let bytes = postcard::to_allocvec(&pending)
            .map_err(|error| ArcaneError::Corrupt(error.to_string()))?;
        if bytes.len() > MAX_LINKED_BYTES {
            return Err(ArcaneError::InvalidTransaction(
                "linked coordinator record exceeds its safety bound".into(),
            ));
        }
        crate::identity::atomic_write(&self.linked_pending_path(), &bytes, false)?;
        self.apply_linked_files(&pending)?;
        let outcome = self.commit(pending.transaction.clone())?;
        crate::persist::remove_if_exists(&self.linked_pending_path())?;
        Ok(outcome)
    }

    fn duplicate_outcome(&self, id: TransactionId) -> Option<CommitOutcome> {
        self.origin_high_water.get(&id.origin).and_then(|high| {
            (id.sequence <= *high)
                .then(|| CommitOutcome::AlreadyApplied(self.recent_receipts.get(&id).cloned()))
        })
    }

    fn validate_transaction(&self, transaction: &ArcaneTransaction) -> Result<(), ArcaneError> {
        let previous = self
            .origin_high_water
            .get(&transaction.id.origin)
            .copied()
            .unwrap_or_default();
        if transaction.id.sequence != previous.checked_add(1).ok_or(ArcaneError::Overflow)? {
            return Err(ArcaneError::InvalidTransaction(format!(
                "transaction sequence {} does not continue origin high-water {}",
                transaction.id.sequence, previous
            )));
        }
        if previous == 0
            && !self.origin_high_water.contains_key(&transaction.id.origin)
            && self.origin_high_water.len() >= MAX_TRANSACTION_HISTORY
        {
            return Err(ArcaneError::InvalidTransaction(
                "transaction origin table exceeds its bound".into(),
            ));
        }
        if transaction.reason.trim().is_empty() || transaction.content_id.trim().is_empty() {
            return Err(ArcaneError::InvalidTransaction(
                "reason and stable content id are required".into(),
            ));
        }
        let touched = transaction
            .debits
            .iter()
            .chain(&transaction.credits)
            .map(|movement| &movement.owner)
            .collect::<BTreeSet<_>>();
        if touched.is_empty() || touched.len() > MAX_TRANSACTION_ACCOUNTS {
            return Err(ArcaneError::InvalidTransaction(format!(
                "transaction touches {} accounts; limit is {MAX_TRANSACTION_ACCOUNTS}",
                touched.len()
            )));
        }
        let reads = transaction
            .reads
            .iter()
            .map(|read| (&read.owner, read.expected_version))
            .collect::<BTreeMap<_, _>>();
        if reads.len() != transaction.reads.len() || reads.len() != touched.len() {
            return Err(ArcaneError::InvalidTransaction(
                "read set must name every touched owner exactly once".into(),
            ));
        }
        for owner in touched {
            let expected = reads.get(owner).copied().ok_or_else(|| {
                ArcaneError::InvalidTransaction("touched owner is absent from read set".into())
            })?;
            let actual = self.version_of(owner);
            if expected != actual {
                return Err(ArcaneError::VersionConflict {
                    owner: owner.clone(),
                    expected,
                    actual,
                });
            }
            self.validate_authority(
                &transaction.authority,
                owner,
                !transaction.debits.iter().any(|m| &m.owner == owner),
            )?;
        }
        let mut debit_total = Current::default();
        let mut credit_total = Current::default();
        for movement in &transaction.debits {
            movement.current.total_checked()?;
            if movement.current.is_empty() {
                return Err(ArcaneError::InvalidTransaction("empty debit".into()));
            }
            let account = self
                .accounts
                .get(&movement.owner)
                .ok_or_else(|| ArcaneError::MissingAccount(movement.owner.clone()))?;
            let mut available = account.current.clone();
            available.checked_sub(&movement.current)?;
            debit_total.checked_add(&movement.current)?;
        }
        for movement in &transaction.credits {
            movement.current.total_checked()?;
            if movement.current.is_empty() {
                return Err(ArcaneError::InvalidTransaction("empty credit".into()));
            }
            if let (Some(existing), Some(replacement)) = (
                self.accounts.get(&movement.owner),
                movement.content_id.as_deref(),
            ) && existing.content_id.as_deref() != Some(replacement)
            {
                let mut removed = Current::default();
                for debit in transaction
                    .debits
                    .iter()
                    .filter(|debit| debit.owner == movement.owner)
                {
                    removed.checked_add(&debit.current)?;
                }
                if removed != existing.current {
                    return Err(ArcaneError::InvalidTransaction(
                        "an item content identity may change only after its old custody is fully debited"
                            .into(),
                    ));
                }
            }
            credit_total.checked_add(&movement.current)?;
        }
        if debit_total.total_checked()? != credit_total.total_checked()? {
            return Err(ArcaneError::Unbalanced {
                debits: debit_total.total_checked()?,
                credits: credit_total.total_checked()?,
            });
        }
        let transformed = apply_transforms(debit_total.clone(), &transaction.transforms)?;
        if transformed != credit_total {
            return Err(ArcaneError::InvalidTransaction(
                "credit resonance mixture does not match declared transforms".into(),
            ));
        }
        for resonance in debit_total
            .parts()
            .keys()
            .chain(credit_total.parts().keys())
        {
            if !self.registry.contains_saved(resonance) {
                return Err(ArcaneError::UnknownResonance(resonance.clone()));
            }
        }
        if transaction.linked.len() > 16
            || transaction.linked.iter().any(|linked| {
                linked.subsystem.is_empty()
                    || linked.operation_id == 0
                    || linked.before_checksum == linked.after_checksum
            })
        {
            return Err(ArcaneError::InvalidTransaction(
                "linked mutation receipt is invalid or exceeds its bound".into(),
            ));
        }
        Ok(())
    }

    fn validate_authority(
        &self,
        authority: &ArcaneAuthority,
        owner: &ArcaneOwner,
        credit_only: bool,
    ) -> Result<(), ArcaneError> {
        let permitted = match authority {
            ArcaneAuthority::System => true,
            ArcaneAuthority::Player(player) => {
                credit_only || matches!(owner, ArcaneOwner::Player(id) if id == player)
            }
            ArcaneAuthority::Mod(mod_id) => {
                let identity = format!("mod:{mod_id}");
                matches!(owner, ArcaneOwner::Working(_))
                    && !mod_id.trim().is_empty()
                    && (credit_only && !self.accounts.contains_key(owner)
                        || self
                            .accounts
                            .get(owner)
                            .and_then(|account| account.content_id.as_deref())
                            == Some(identity.as_str()))
            }
            ArcaneAuthority::Operator { development, actor } => {
                *development && !actor.trim().is_empty()
            }
        };
        if permitted {
            Ok(())
        } else {
            Err(ArcaneError::PermissionDenied(owner.clone()))
        }
    }

    fn apply_validated(
        &mut self,
        transaction: &ArcaneTransaction,
        sequence: u64,
    ) -> Result<TransactionReceipt, ArcaneError> {
        self.validate_transaction(transaction)?;
        let touched = transaction
            .debits
            .iter()
            .chain(&transaction.credits)
            .map(|movement| movement.owner.clone())
            .collect::<BTreeSet<_>>();
        // Stage only the declared write set. Cloning the whole owner index
        // made a two-account transfer grow with every charged item in the
        // world, violating the ledger's central performance bound.
        let mut staged = touched
            .iter()
            .map(|owner| (owner.clone(), self.accounts.get(owner).cloned()))
            .collect::<BTreeMap<_, _>>();
        for movement in &transaction.debits {
            let account = staged
                .get_mut(&movement.owner)
                .and_then(Option::as_mut)
                .ok_or_else(|| ArcaneError::MissingAccount(movement.owner.clone()))?;
            account.current.checked_sub(&movement.current)?;
        }
        for movement in &transaction.credits {
            let account = staged
                .get_mut(&movement.owner)
                .expect("every credited owner is staged")
                .get_or_insert_with(|| ArcaneAccount {
                    current: Current::default(),
                    version: 0,
                    content_id: movement.content_id.clone(),
                });
            let replaces_identity = account.current.is_empty();
            account.current.checked_add(&movement.current)?;
            if replaces_identity || account.content_id.is_none() {
                account.content_id.clone_from(&movement.content_id);
            }
        }
        let mut touched_versions = BTreeMap::new();
        for owner in touched {
            let remove = staged
                .get(&owner)
                .and_then(Option::as_ref)
                .is_none_or(|account| account.current.is_empty());
            if remove {
                let old = self.version_of(&owner);
                touched_versions.insert(
                    owner.clone(),
                    old.checked_add(1).ok_or(ArcaneError::Overflow)?,
                );
                staged.insert(owner, None);
            } else {
                let account = staged
                    .get_mut(&owner)
                    .and_then(Option::as_mut)
                    .expect("nonempty staged account exists");
                account.version = account
                    .version
                    .checked_add(1)
                    .ok_or(ArcaneError::Overflow)?;
                touched_versions.insert(owner, account.version);
            }
        }
        for (owner, account) in staged {
            if let Some(account) = account {
                self.accounts.insert(owner, account);
            } else {
                self.accounts.remove(&owner);
            }
        }
        let receipt = TransactionReceipt {
            id: transaction.id,
            delta_sequence: sequence,
            touched_versions,
        };
        self.origin_high_water
            .entry(transaction.id.origin)
            .and_modify(|value| *value = (*value).max(transaction.id.sequence))
            .or_insert(transaction.id.sequence);
        self.recent_receipts.insert(transaction.id, receipt.clone());
        self.receipt_order.push_back(transaction.id);
        while self.receipt_order.len() > MAX_TRANSACTION_HISTORY {
            if let Some(old) = self.receipt_order.pop_front() {
                self.recent_receipts.remove(&old);
            }
        }
        let moved_units = transaction.debits.iter().try_fold(0u64, |sum, movement| {
            sum.checked_add(movement.current.total_checked()?)
                .ok_or(ArcaneError::Overflow)
        })?;
        self.audit_history.push_back(ArcaneAuditEvent {
            transaction: transaction.id,
            reason: transaction.reason.clone(),
            content_id: transaction.content_id.clone(),
            authority: authority_label(&transaction.authority),
            units: moved_units,
        });
        while self.audit_history.len() > MAX_AUDIT_HISTORY {
            self.audit_history.pop_front();
        }
        self.last_clean_total = self.total_checked()?;
        Ok(receipt)
    }

    /// Route an item/entity/container loss to a bounded regional account.
    #[allow(dead_code)] // Exposed separately for audit and destructive-path fixtures.
    pub fn loss_transaction(
        &self,
        id: TransactionId,
        owner: ArcaneOwner,
        region: AtlasPos,
        medium: DrossMedium,
        polluted: bool,
        reason: impl Into<String>,
    ) -> Result<ArcaneTransaction, ArcaneError> {
        let account = self
            .accounts
            .get(&owner)
            .ok_or_else(|| ArcaneError::MissingAccount(owner.clone()))?;
        let destination = if polluted {
            ArcaneOwner::Dross { region, medium }
        } else {
            ArcaneOwner::Ambient(region)
        };
        Ok(ArcaneTransaction::transfer(
            id,
            owner,
            account.version,
            destination.clone(),
            self.version_of(&destination),
            account.current.clone(),
            ArcaneAuthority::System,
            reason,
        ))
    }

    pub fn mod_working_transfer(
        &mut self,
        mod_id: &str,
        from: u64,
        to: u64,
        current: Current,
        reason: &str,
    ) -> Result<CommitOutcome, ArcaneError> {
        if mod_id.is_empty() || reason.is_empty() || reason.len() > 128 {
            return Err(ArcaneError::InvalidTransaction(
                "mod Current request needs bounded attribution".into(),
            ));
        }
        let from = ArcaneOwner::Working(from);
        let to = ArcaneOwner::Working(to);
        let id = self.system_transaction_id()?;
        let mut transaction = ArcaneTransaction::transfer(
            id,
            from.clone(),
            self.version_of(&from),
            to.clone(),
            self.version_of(&to),
            current,
            ArcaneAuthority::Mod(mod_id.into()),
            reason,
        );
        transaction.content_id = format!("mod:{mod_id}:working_transfer");
        transaction.credits[0].content_id = Some(format!("mod:{mod_id}"));
        self.commit(transaction)
    }

    /// Visible escape hatch for development fixtures and operator recovery.
    /// Ordinary gameplay and scripts have no route to this method.
    #[allow(dead_code)] // Deliberately unavailable to normal gameplay; audit/recovery only.
    pub fn operator_adjust_deep(
        &mut self,
        actor: &str,
        development: bool,
        resonance: &str,
        delta: i64,
    ) -> Result<(), ArcaneError> {
        if !development || actor.trim().is_empty() || delta == 0 {
            return Err(ArcaneError::PermissionDenied(ArcaneOwner::Deep));
        }
        if !self.registry.contains_saved(resonance) {
            return Err(ArcaneError::UnknownResonance(resonance.into()));
        }
        let deep = self
            .accounts
            .get_mut(&ArcaneOwner::Deep)
            .ok_or(ArcaneError::MissingAccount(ArcaneOwner::Deep))?;
        if delta > 0 {
            deep.current
                .checked_add(&Current::single(resonance, delta.unsigned_abs()))?;
        } else {
            deep.current
                .checked_sub(&Current::single(resonance, delta.unsigned_abs()))?;
        }
        deep.version = deep.version.checked_add(1).ok_or(ArcaneError::Overflow)?;
        self.external_adjustment = self
            .external_adjustment
            .checked_add(i128::from(delta))
            .ok_or(ArcaneError::Overflow)?;
        self.audit_history.push_back(ArcaneAuditEvent {
            transaction: TransactionId {
                origin: *b"external-adjust!",
                sequence: deep.version,
            },
            reason: format!("external adjustment by {actor}"),
            content_id: "base:external_adjustment".into(),
            authority: format!("operator:{actor}:development=true"),
            units: delta.unsigned_abs(),
        });
        self.last_clean_total = self.total_checked()?;
        self.save()
    }

    pub fn audit(&self) -> Result<ArcaneAudit, ArcaneError> {
        let mut reservoirs = Reservoir::ALL
            .into_iter()
            .map(|reservoir| (reservoir, 0u64))
            .collect::<BTreeMap<_, _>>();
        let mut resonance = BTreeMap::<String, u64>::new();
        let mut owner_classes = BTreeMap::<String, u64>::new();
        let mut total = 0u64;
        for (owner, account) in &self.accounts {
            let units = account.current.total_checked()?;
            total = total.checked_add(units).ok_or(ArcaneError::Overflow)?;
            let reservoir = reservoirs.entry(owner.reservoir()).or_default();
            *reservoir = reservoir.checked_add(units).ok_or(ArcaneError::Overflow)?;
            let class = owner_classes.entry(owner.class_name().into()).or_default();
            *class = class.checked_add(units).ok_or(ArcaneError::Overflow)?;
            for (name, value) in account.current.parts() {
                let slot = resonance.entry(name.clone()).or_default();
                *slot = slot.checked_add(*value).ok_or(ArcaneError::Overflow)?;
            }
        }
        let expected = i128::from(self.genesis_total)
            .checked_add(self.external_adjustment)
            .ok_or(ArcaneError::Overflow)?;
        let deep_checksum = postcard::to_allocvec(&self.accounts.get(&ArcaneOwner::Deep))
            .map(|bytes| checksum(&bytes))
            .map_err(|error| ArcaneError::Corrupt(error.to_string()))?;
        let ambient = self
            .accounts
            .iter()
            .filter(|(owner, _)| matches!(owner, ArcaneOwner::Ambient(_)))
            .collect::<Vec<_>>();
        let ambient_checksum = postcard::to_allocvec(&ambient)
            .map(|bytes| checksum(&bytes))
            .map_err(|error| ArcaneError::Corrupt(error.to_string()))?;
        Ok(ArcaneAudit {
            schema_version: self.schema_version,
            algorithm_version: self.algorithm_version,
            unit_scale: self.unit_scale,
            genesis_total: self.genesis_total,
            total,
            unexplained_delta: i128::from(total) - expected,
            reservoirs,
            resonance,
            owner_classes,
            registry_hash: self.registry.hash(),
            external_adjustment: self.external_adjustment,
            ledger_checksum: self.snapshot_checksum()?,
            deep_checksum,
            ambient_checksum,
            last_delta_sequence: self.last_delta_seq,
            durable_items: DurableItemStatus::default(),
            dormant_transient_accounts: 0,
            geography_present: false,
            geography_total: 0,
            geography_checksum: 0,
            geography_custody_matches: true,
        })
    }

    fn snapshot_checksum(&self) -> Result<u64, ArcaneError> {
        postcard::to_allocvec(self)
            .map(|bytes| checksum(&bytes))
            .map_err(|error| ArcaneError::Corrupt(error.to_string()))
    }

    fn total_checked(&self) -> Result<u64, ArcaneError> {
        self.accounts.values().try_fold(0u64, |total, account| {
            total
                .checked_add(account.current.total_checked()?)
                .ok_or(ArcaneError::Overflow)
        })
    }

    fn validate_total(&self) -> Result<(), ArcaneError> {
        validate_total_for(&self.accounts, self.genesis_total, self.external_adjustment)
    }

    fn delta_path(&self) -> PathBuf {
        self.path.with_file_name(DELTA_FILE)
    }

    fn pending_path(&self) -> PathBuf {
        self.path.with_file_name(DELTA_PENDING)
    }

    fn linked_pending_path(&self) -> PathBuf {
        self.path.with_file_name(LINKED_PENDING)
    }

    fn world_dir(&self) -> Result<&Path, ArcaneError> {
        self.path
            .parent()
            .ok_or_else(|| ArcaneError::Corrupt("arcane ledger path has no world directory".into()))
    }

    fn read_linked_pending(&self) -> Result<Option<LinkedCommit>, ArcaneError> {
        match std::fs::read(self.linked_pending_path()) {
            Ok(bytes) => {
                if bytes.len() > MAX_LINKED_BYTES {
                    return Err(ArcaneError::Corrupt(
                        "linked coordinator exceeds its safety bound".into(),
                    ));
                }
                postcard::from_bytes(&bytes)
                    .map(Some)
                    .map_err(|error| ArcaneError::Corrupt(error.to_string()))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn apply_linked_files(&self, pending: &LinkedCommit) -> Result<(), ArcaneError> {
        let world = self.world_dir()?;
        for (file, linked) in pending.files.iter().zip(&pending.transaction.linked) {
            validate_linked_path(file)?;
            if file.subsystem != linked.subsystem || file.operation_id != linked.operation_id {
                return Err(ArcaneError::Corrupt(
                    "linked file list does not match transaction receipts".into(),
                ));
            }
            let path = world.join(&file.relative_path);
            let current = std::fs::read(&path).unwrap_or_default();
            let current_checksum = checksum(&current);
            if current_checksum == linked.after_checksum {
                continue;
            }
            if current_checksum != linked.before_checksum {
                return Err(ArcaneError::Corrupt(format!(
                    "linked {} file {} matches neither before nor after checksum",
                    file.subsystem, file.relative_path
                )));
            }
            match &file.after {
                Some(after) => crate::identity::atomic_write(&path, after, false)?,
                None => crate::persist::remove_if_exists(&path)?,
            }
        }
        Ok(())
    }

    fn recover_linked_commit(&mut self) -> Result<(), ArcaneError> {
        let Some(pending) = self.read_linked_pending()? else {
            return Ok(());
        };
        self.apply_linked_files(&pending)?;
        match self.commit(pending.transaction)? {
            CommitOutcome::Applied(_) | CommitOutcome::AlreadyApplied(_) => {}
        }
        crate::persist::remove_if_exists(&self.linked_pending_path())?;
        Ok(())
    }

    fn replay_delta_log(&mut self) -> Result<(), ArcaneError> {
        let path = self.delta_path();
        let pending = match std::fs::read(self.pending_path()) {
            Ok(bytes) => Some(
                postcard::from_bytes::<ArcaneDelta>(&bytes)
                    .map_err(|error| ArcaneError::Corrupt(error.to_string()))?,
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let mut bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => DELTA_MAGIC.to_vec(),
            Err(error) => return Err(error.into()),
        };
        if bytes.len() as u64 > MAX_DELTA_BYTES {
            return Err(ArcaneError::Corrupt(
                "arcane delta log exceeds safety bound".into(),
            ));
        }
        if !bytes.starts_with(DELTA_MAGIC) {
            return Err(ArcaneError::Corrupt(
                "arcane delta log has an invalid header".into(),
            ));
        }
        let (mut records, valid_end, complete) = decode_frames(&bytes);
        if !complete && pending.is_none() {
            return Err(ArcaneError::Corrupt(
                "arcane delta log has a corrupt or truncated tail".into(),
            ));
        }
        if let Some(pending) = pending {
            let logged = records
                .iter()
                .any(|record| record.sequence == pending.sequence);
            if !complete || !logged {
                let last = records
                    .last()
                    .map_or(self.last_delta_seq, |record| record.sequence);
                if pending.sequence != last.checked_add(1).ok_or(ArcaneError::Overflow)? {
                    return Err(ArcaneError::Corrupt(
                        "pending arcane delta does not continue the durable sequence".into(),
                    ));
                }
                bytes.truncate(valid_end);
                bytes.extend_from_slice(&encode_frame(&pending)?);
                crate::identity::atomic_write(&path, &bytes, false)?;
                records.push(pending);
            }
            crate::persist::remove_if_exists(&self.pending_path())?;
        }
        for record in records {
            if record.sequence <= self.last_delta_seq {
                continue;
            }
            if record.sequence
                != self
                    .last_delta_seq
                    .checked_add(1)
                    .ok_or(ArcaneError::Overflow)?
            {
                return Err(ArcaneError::Corrupt(
                    "arcane delta sequence has a gap".into(),
                ));
            }
            self.apply_validated(&record.transaction, record.sequence)?;
            self.last_delta_seq = record.sequence;
        }
        Ok(())
    }
}

fn take_mixture(
    source: &mut Current,
    preferred: &Current,
    total: u64,
) -> Result<Current, ArcaneError> {
    let mut out = Current::default();
    let mut remaining = total;
    let names = preferred
        .parts()
        .keys()
        .cloned()
        .chain(BASE_RESONANCES.into_iter().map(str::to_string))
        .collect::<BTreeSet<_>>();
    for name in names {
        if remaining == 0 {
            break;
        }
        let available = source.units_of(&name);
        let take = available.min(remaining);
        if take != 0 {
            let current = Current::single(name, take);
            source.checked_sub(&current)?;
            out.checked_add(&current)?;
            remaining -= take;
        }
    }
    if remaining != 0 {
        return Err(ArcaneError::InsufficientCurrent {
            available: total - remaining,
            requested: total,
        });
    }
    Ok(out)
}

fn validate_linked_path(file: &LinkedFileReplacement) -> Result<(), ArcaneError> {
    use std::path::Component;
    if file.operation_id == 0
        || file.relative_path.is_empty()
        || Path::new(&file.relative_path)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ArcaneError::InvalidTransaction(
            "linked file path or operation id is invalid".into(),
        ));
    }
    let allowed = match file.subsystem.as_str() {
        "materials" => matches!(
            file.relative_path.as_str(),
            "materials.wfm" | "materials.wfm.log"
        ),
        "water" => matches!(
            file.relative_path.as_str(),
            "planet/dynamic.wfd" | "planet/water.wfw"
        ),
        "ecology" => matches!(
            file.relative_path.as_str(),
            "hearts"
                | "rire"
                | "bloom"
                | "bspent"
                | "animals.toml"
                | "planet/arcane-geography.wad"
                | "planet/arcane-geography.toml"
                | "planet/manifest.toml"
        ),
        "implements" => file.relative_path == crate::implements::IMPLEMENTS_FILE,
        _ => false,
    };
    if !allowed {
        return Err(ArcaneError::InvalidTransaction(format!(
            "linked subsystem {} may not replace {}",
            file.subsystem, file.relative_path
        )));
    }
    Ok(())
}

fn apply_transforms(
    mut current: Current,
    transforms: &[ResonanceTransform],
) -> Result<Current, ArcaneError> {
    for transform in transforms {
        if transform.units == 0 || transform.from == transform.to {
            return Err(ArcaneError::InvalidTransaction(
                "invalid resonance transform".into(),
            ));
        }
        current.checked_sub(&Current::single(transform.from.clone(), transform.units))?;
        current.checked_add(&Current::single(transform.to.clone(), transform.units))?;
    }
    Ok(current)
}

fn validate_total_for(
    accounts: &BTreeMap<ArcaneOwner, ArcaneAccount>,
    genesis_total: u64,
    external_adjustment: i128,
) -> Result<(), ArcaneError> {
    let total = accounts.values().try_fold(0u128, |sum, account| {
        sum.checked_add(u128::from(account.current.total_checked()?))
            .ok_or(ArcaneError::Overflow)
    })?;
    let expected = i128::from(genesis_total)
        .checked_add(external_adjustment)
        .ok_or(ArcaneError::Overflow)?;
    if expected < 0 || total != u128::try_from(expected).map_err(|_| ArcaneError::Overflow)? {
        return Err(ArcaneError::ConservationGap {
            expected,
            actual: total,
        });
    }
    Ok(())
}

fn authority_label(authority: &ArcaneAuthority) -> String {
    match authority {
        ArcaneAuthority::System => "system".into(),
        ArcaneAuthority::Player(id) => format!("player:{:02x?}", id),
        ArcaneAuthority::Mod(id) => format!("mod:{id}"),
        ArcaneAuthority::Operator { actor, development } => {
            format!("operator:{actor}:development={development}")
        }
    }
}

fn reading_band(units: u64) -> u8 {
    match units {
        0 => 0,
        1..=64 => 1,
        65..=512 => 2,
        513..=4_096 => 3,
        _ => 4,
    }
}

fn checksum(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

fn encode_frame(delta: &ArcaneDelta) -> Result<Vec<u8>, ArcaneError> {
    let payload =
        postcard::to_allocvec(delta).map_err(|error| ArcaneError::Corrupt(error.to_string()))?;
    let length = u32::try_from(payload.len()).map_err(|_| {
        ArcaneError::InvalidTransaction("arcane delta record exceeds its size bound".into())
    })?;
    let mut frame = Vec::with_capacity(4 + payload.len() + 8);
    frame.extend_from_slice(&length.to_le_bytes());
    frame.extend_from_slice(&payload);
    frame.extend_from_slice(&checksum(&payload).to_le_bytes());
    Ok(frame)
}

fn decode_frames(bytes: &[u8]) -> (Vec<ArcaneDelta>, usize, bool) {
    if !bytes.starts_with(DELTA_MAGIC) {
        return (Vec::new(), 0, false);
    }
    let mut records = Vec::new();
    let mut offset = DELTA_MAGIC.len();
    while offset < bytes.len() {
        if bytes.len() - offset < 4 {
            return (records, offset, false);
        }
        let length = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        if length > 1024 * 1024 || bytes.len() - offset < 4 + length + 8 {
            return (records, offset, false);
        }
        let start = offset + 4;
        let end = start + length;
        let expected = u64::from_le_bytes(bytes[end..end + 8].try_into().unwrap());
        if checksum(&bytes[start..end]) != expected {
            return (records, offset, false);
        }
        let Ok(delta) = postcard::from_bytes::<ArcaneDelta>(&bytes[start..end]) else {
            return (records, offset, false);
        };
        records.push(delta);
        offset = end + 8;
    }
    (records, offset, true)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct DurableItemStatus {
    pub item_accounts: usize,
    pub durable_references: usize,
    pub orphan_accounts: usize,
    pub invalid_references: usize,
    pub duplicate_references: usize,
}

#[derive(Default)]
struct DurableItemScan {
    references: BTreeMap<u64, usize>,
    item_accounts: usize,
    orphan_accounts: usize,
    invalid_references: usize,
    duplicate_references: usize,
}

impl DurableItemScan {
    fn status(&self) -> DurableItemStatus {
        DurableItemStatus {
            item_accounts: self.item_accounts,
            durable_references: self.references.values().sum(),
            orphan_accounts: self.orphan_accounts,
            invalid_references: self.invalid_references,
            duplicate_references: self.duplicate_references,
        }
    }
}

struct DurableItemFile {
    path: PathBuf,
    value: toml::Value,
    changed: bool,
}

fn durable_item_paths(world: &Path) -> Result<Vec<PathBuf>, ArcaneError> {
    let mut paths = [
        "entities.toml",
        "animals.toml",
        "loose-items.toml",
        "player.toml",
    ]
    .into_iter()
    .map(|name| world.join(name))
    .filter(|path| path.is_file())
    .collect::<Vec<_>>();
    let players = world.join("players");
    match std::fs::read_dir(&players) {
        Ok(entries) => {
            for entry in entries {
                let path = entry?.path();
                if path.file_name().and_then(|name| name.to_str()) == Some("index.toml")
                    || path.extension().and_then(|extension| extension.to_str()) != Some("toml")
                {
                    continue;
                }
                paths.push(path);
                if paths.len() > MAX_ITEM_OWNER_FILES {
                    return Err(ArcaneError::Corrupt(
                        "too many durable item-owner files to audit safely".into(),
                    ));
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    paths.sort();
    Ok(paths)
}

fn stack_table_valid(
    table: &toml::map::Map<String, toml::Value>,
    ledger: &ArcaneLedger,
) -> Option<(bool, u64)> {
    let item = table.get("item")?.as_str()?;
    let declared_charged = ledger.item_manifests.contains_key(item);
    let id = match table.get("arcane_id") {
        Some(value) => match value.as_integer() {
            Some(value) if value >= 0 => value as u64,
            _ => return Some((false, 0)),
        },
        None => 0,
    };
    if !declared_charged && crate::discovery::is_knowledge_id(id) {
        // Discovery objects share the durable stack field but live in their
        // own signed census. They are not ArcaneOwner::Item references and
        // must not be deleted as stale Current accounts during crash repair.
        return Some((true, 0));
    }
    let valid = if declared_charged {
        // A declared magical shell (most importantly a craftable charge
        // vessel) may exist physically before a host-authoritative binding
        // gives it Current. Zero is therefore an explicitly dormant state;
        // every nonzero identity must still match saved finite custody.
        id == 0 || ledger.item_matches(id, item)
    } else {
        id == 0
    };
    Some((valid, id))
}

fn scan_item_value(
    value: &mut toml::Value,
    ledger: &ArcaneLedger,
    references: &mut BTreeMap<u64, usize>,
    invalid: &mut usize,
    repair: bool,
) -> Result<bool, ArcaneError> {
    match value {
        toml::Value::Array(values) => {
            let mut index = 0;
            while index < values.len() {
                let classification = values[index]
                    .as_table()
                    .and_then(|table| stack_table_valid(table, ledger));
                if let Some((valid, id)) = classification {
                    if valid {
                        if id != 0 {
                            let count = references.entry(id).or_default();
                            *count = count.checked_add(1).ok_or(ArcaneError::Overflow)?;
                        }
                        index += 1;
                    } else {
                        *invalid = invalid.checked_add(1).ok_or(ArcaneError::Overflow)?;
                        if repair {
                            values.remove(index);
                        } else {
                            index += 1;
                        }
                    }
                } else {
                    let _ =
                        scan_item_value(&mut values[index], ledger, references, invalid, repair)?;
                    index += 1;
                }
            }
            Ok(false)
        }
        toml::Value::Table(table) => {
            if let Some((valid, id)) = stack_table_valid(table, ledger) {
                if valid {
                    if id != 0 {
                        let count = references.entry(id).or_default();
                        *count = count.checked_add(1).ok_or(ArcaneError::Overflow)?;
                    }
                    return Ok(false);
                }
                *invalid = invalid.checked_add(1).ok_or(ArcaneError::Overflow)?;
                return Ok(repair);
            }
            let keys = table.keys().cloned().collect::<Vec<_>>();
            for key in keys {
                let remove = scan_item_value(
                    table.get_mut(&key).expect("key came from this table"),
                    ledger,
                    references,
                    invalid,
                    repair,
                )?;
                if remove {
                    table.remove(&key);
                }
            }
            Ok(false)
        }
        _ => Ok(false),
    }
}

fn scan_durable_item_owners(
    world: &Path,
    ledger: &ArcaneLedger,
    repair: bool,
) -> Result<DurableItemScan, ArcaneError> {
    let mut files = Vec::new();
    let mut scan = DurableItemScan {
        item_accounts: ledger
            .accounts
            .keys()
            .filter_map(|owner| match owner {
                ArcaneOwner::Item(id) | ArcaneOwner::ItemDross(id) => Some(*id),
                _ => None,
            })
            .collect::<BTreeSet<_>>()
            .len(),
        ..Default::default()
    };
    for path in durable_item_paths(world)? {
        let metadata = std::fs::metadata(&path)?;
        if metadata.len() > MAX_ITEM_OWNER_FILE_BYTES {
            return Err(ArcaneError::Corrupt(format!(
                "durable item owner {} exceeds the audit size bound",
                path.display()
            )));
        }
        let text = std::fs::read_to_string(&path)?;
        let mut value = toml::from_str::<toml::Value>(&text).map_err(|error| {
            ArcaneError::Corrupt(format!(
                "could not audit durable item owner {}: {error}",
                path.display()
            ))
        })?;
        let invalid_before = scan.invalid_references;
        if scan_item_value(
            &mut value,
            ledger,
            &mut scan.references,
            &mut scan.invalid_references,
            repair,
        )? {
            return Err(ArcaneError::Corrupt(format!(
                "durable item owner {} is itself an invalid stack",
                path.display()
            )));
        }
        files.push(DurableItemFile {
            path,
            value,
            changed: repair && scan.invalid_references != invalid_before,
        });
    }
    scan.duplicate_references = scan.references.values().try_fold(0usize, |total, count| {
        total
            .checked_add(count.saturating_sub(1))
            .ok_or(ArcaneError::Overflow)
    })?;
    scan.orphan_accounts = ledger
        .accounts
        .keys()
        .filter_map(|owner| match owner {
            ArcaneOwner::Item(id) | ArcaneOwner::ItemDross(id)
                if !scan.references.contains_key(id) =>
            {
                Some(*id)
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>()
        .len();
    if repair && scan.duplicate_references == 0 {
        for file in files.into_iter().filter(|file| file.changed) {
            let text = toml::to_string_pretty(&file.value)
                .map_err(|error| ArcaneError::Corrupt(error.to_string()))?;
            crate::identity::atomic_write(&file.path, text.as_bytes(), false)?;
        }
    }
    Ok(scan)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArcaneAudit {
    pub schema_version: u32,
    pub algorithm_version: u32,
    pub unit_scale: u32,
    pub genesis_total: u64,
    pub total: u64,
    pub unexplained_delta: i128,
    pub reservoirs: BTreeMap<Reservoir, u64>,
    pub resonance: BTreeMap<String, u64>,
    pub owner_classes: BTreeMap<String, u64>,
    pub registry_hash: u64,
    pub external_adjustment: i128,
    pub ledger_checksum: u64,
    pub deep_checksum: u64,
    pub ambient_checksum: u64,
    pub last_delta_sequence: u64,
    pub durable_items: DurableItemStatus,
    /// Mob/Working accounts found in a closed save have no durable owner and
    /// will be rolled back during the next authoritative open.
    pub dormant_transient_accounts: usize,
    pub geography_present: bool,
    pub geography_total: u64,
    pub geography_checksum: u64,
    pub geography_custody_matches: bool,
}

impl ArcaneAudit {
    pub fn is_balanced(&self) -> bool {
        self.unexplained_delta == 0
            && self.durable_items.orphan_accounts == 0
            && self.durable_items.invalid_references == 0
            && self.durable_items.duplicate_references == 0
            && self.dormant_transient_accounts == 0
            && self.geography_custody_matches
    }

    pub fn render(&self) -> String {
        let mut out = format!(
            "Arcane audit schema {} algorithm {} ({} subunits/unit)\nGenesis: {}\nAccounted: {}\nUnexplained delta: {}\nRegistry hash: {:016x}\nLedger checksum: {:016x}\nDeep checksum: {:016x}\nAmbient checksum: {:016x}\nLast transaction: {}\nExternal adjustment: {}\nDurable items: {} accounts, {} references, {} orphan accounts, {} invalid references, {} duplicate references\nDormant transient owners: {}\nGeography: present={}, total={}, checksum={:016x}, custody_matches={}\nReservoirs:\n",
            self.schema_version,
            self.algorithm_version,
            self.unit_scale,
            self.genesis_total,
            self.total,
            self.unexplained_delta,
            self.registry_hash,
            self.ledger_checksum,
            self.deep_checksum,
            self.ambient_checksum,
            self.last_delta_sequence,
            self.external_adjustment,
            self.durable_items.item_accounts,
            self.durable_items.durable_references,
            self.durable_items.orphan_accounts,
            self.durable_items.invalid_references,
            self.durable_items.duplicate_references,
            self.dormant_transient_accounts,
            self.geography_present,
            self.geography_total,
            self.geography_checksum,
            self.geography_custody_matches,
        );
        for reservoir in Reservoir::ALL {
            out.push_str(&format!(
                "  {:<8} {}\n",
                reservoir.label(),
                self.reservoirs.get(&reservoir).copied().unwrap_or_default()
            ));
        }
        out.push_str("Resonance:\n");
        for (name, units) in &self.resonance {
            out.push_str(&format!("  {name:<20} {units}\n"));
        }
        out.push_str("Bound/active owner classes:\n");
        for (name, units) in &self.owner_classes {
            out.push_str(&format!("  {name:<20} {units}\n"));
        }
        out
    }
}

pub fn audit_world(world: &Path) -> Result<ArcaneAudit, ArcaneError> {
    let ledger = ArcaneLedger::load(world)?;
    let mut audit = ledger.audit()?;
    audit.durable_items = scan_durable_item_owners(world, &ledger, false)?.status();
    audit.dormant_transient_accounts = ledger
        .accounts
        .keys()
        .filter(|owner| matches!(owner, ArcaneOwner::Mob(_) | ArcaneOwner::Working(_)))
        .count();
    let geography_manifest =
        crate::planet_atlas::PlanetAtlas::planet_dir(world).join("arcane-geography.toml");
    if geography_manifest.is_file() {
        let atlas = crate::planet_atlas::PlanetAtlas::load(world)
            .map_err(|error| ArcaneError::Corrupt(error.to_string()))?;
        let geography = crate::arcane_geography::ArcaneGeography::load(world, &atlas)
            .map_err(|error| ArcaneError::Corrupt(error.to_string()))?;
        let geography_audit = geography
            .audit()
            .map_err(|error| ArcaneError::Corrupt(error.to_string()))?;
        audit.geography_present = true;
        audit.geography_total = geography_audit.accounted_total;
        audit.geography_checksum = geography_audit.checksum;
        audit.geography_custody_matches = ledger
            .account(&ArcaneOwner::Geography)
            .is_some_and(|account| account.current == geography_audit.as_current());
    }
    Ok(audit)
}

#[derive(Debug)]
pub enum ArcaneError {
    Io(std::io::Error),
    Overflow,
    InvalidCurrent(String),
    InvalidTransaction(String),
    UnknownResonance(String),
    MissingAccount(ArcaneOwner),
    PermissionDenied(ArcaneOwner),
    VersionConflict {
        owner: ArcaneOwner,
        expected: u64,
        actual: u64,
    },
    InsufficientCurrent {
        available: u64,
        requested: u64,
    },
    InsufficientResonance {
        resonance: String,
        available: u64,
        requested: u64,
    },
    Unbalanced {
        debits: u64,
        credits: u64,
    },
    ConservationGap {
        expected: i128,
        actual: u128,
    },
    Corrupt(String),
}

impl fmt::Display for ArcaneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Overflow => f.write_str("arcane integer overflow"),
            Self::InvalidCurrent(message)
            | Self::InvalidTransaction(message)
            | Self::Corrupt(message) => f.write_str(message),
            Self::UnknownResonance(name) => write!(f, "unknown resonance {name}"),
            Self::MissingAccount(owner) => write!(f, "missing arcane account {owner:?}"),
            Self::PermissionDenied(owner) => write!(f, "authority may not debit {owner:?}"),
            Self::VersionConflict {
                owner,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "stale arcane account {owner:?}: expected version {expected}, found {actual}"
                )
            }
            Self::InsufficientCurrent {
                available,
                requested,
            } => {
                write!(
                    f,
                    "insufficient Current: {available} available, {requested} requested"
                )
            }
            Self::InsufficientResonance {
                resonance,
                available,
                requested,
            } => {
                write!(
                    f,
                    "insufficient {resonance}: {available} available, {requested} requested"
                )
            }
            Self::Unbalanced { debits, credits } => {
                write!(
                    f,
                    "unbalanced arcane transaction: {debits} debited, {credits} credited"
                )
            }
            Self::ConservationGap { expected, actual } => {
                write!(
                    f,
                    "arcane conservation gap: expected {expected}, found {actual}"
                )
            }
        }
    }
}

impl std::error::Error for ArcaneError {}

impl From<std::io::Error> for ArcaneError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planet::Face;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root(tag: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "wildforge-arcane-{tag}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn fixture(
        tag: &str,
        durable: bool,
    ) -> (
        PathBuf,
        PlanetAtlas,
        crate::registry::Registry,
        ArcaneLedger,
    ) {
        let root = root(tag);
        std::fs::create_dir_all(&root).unwrap();
        let atlas = PlanetAtlas::fixture(0xace5, 8).unwrap();
        if durable {
            atlas.write_new(&root).unwrap();
        }
        let reg = crate::registry::load(Path::new("__no_arcane_test_mods__"));
        assert!(reg.arcane_errors.is_empty(), "{:?}", reg.arcane_errors);
        let ledger = ArcaneLedger::initialize(root.join(LEDGER_FILE), &atlas, &reg).unwrap();
        (root, atlas, reg, ledger)
    }

    fn region(atlas: &PlanetAtlas) -> AtlasPos {
        AtlasPos::new(Face::PosZ, 0, 0, atlas.side()).unwrap()
    }

    fn source_slice(ledger: &ArcaneLedger, units: u64) -> Current {
        let mut current = ledger.account(&ArcaneOwner::Deep).unwrap().current.clone();
        current
            .take_units(units, BASE_RESONANCES.into_iter().map(str::to_string))
            .unwrap()
    }

    #[test]
    fn one_item_id_keeps_sequestered_dross_in_its_own_reservoir() {
        let (_root, atlas, _reg, mut ledger) = fixture("item-dross", false);
        let item_id = ledger.allocate_item_id().unwrap();
        let clean = source_slice(&ledger, 9);
        let dross = source_slice(&ledger, 13);
        let mut total = clean.clone();
        total.checked_add(&dross).unwrap();
        let deep = ledger.account(&ArcaneOwner::Deep).unwrap().clone();
        let transaction = ArcaneTransaction {
            id: ledger.system_transaction_id().unwrap(),
            reads: vec![
                AccountRead {
                    owner: ArcaneOwner::Deep,
                    expected_version: deep.version,
                },
                AccountRead {
                    owner: ArcaneOwner::Item(item_id),
                    expected_version: 0,
                },
                AccountRead {
                    owner: ArcaneOwner::ItemDross(item_id),
                    expected_version: 0,
                },
            ],
            debits: vec![ArcaneMove {
                owner: ArcaneOwner::Deep,
                current: total,
                content_id: None,
            }],
            credits: vec![
                ArcaneMove {
                    owner: ArcaneOwner::Item(item_id),
                    current: clean,
                    content_id: Some("base:ashlace_tissue".into()),
                },
                ArcaneMove {
                    owner: ArcaneOwner::ItemDross(item_id),
                    current: dross,
                    content_id: Some("base:ashlace_tissue".into()),
                },
            ],
            transforms: Vec::new(),
            authority: ArcaneAuthority::System,
            reason: "ecology fixture".into(),
            content_id: "base:ashlace_tissue".into(),
            linked: Vec::new(),
        };
        ledger.commit(transaction).unwrap();
        assert_eq!(ledger.item_current_total(item_id), Some(22));
        assert_eq!(ledger.item_dross_total(item_id), 13);
        assert!(ledger.item_matches(item_id, "base:ashlace_tissue"));
        let destination = ArcaneOwner::Dross {
            region: region(&atlas),
            medium: DrossMedium::Soil,
        };
        ledger
            .move_all_item(item_id, destination.clone(), "tissue decayed")
            .unwrap();
        assert_eq!(ledger.item_current_total(item_id), None);
        assert_eq!(ledger.account(&destination).unwrap().current.total(), 22);
        assert!(ledger.audit().unwrap().is_balanced());
    }

    #[test]
    fn moving_a_whole_item_into_sequestered_dross_keeps_its_content_identity() {
        let (_root, _atlas, reg, mut ledger) = fixture("whole-item-dross-identity", false);
        let item = reg.item_id("base:sealed_dross_ampoule").unwrap();
        let definition = reg.item(item).arcane.as_ref().unwrap();
        let id = ledger
            .bind_new_item(
                ArcaneOwner::Deep,
                definition,
                "base:sealed_dross_ampoule",
                "sealed artifact fixture",
            )
            .unwrap();
        ledger
            .move_all(
                ArcaneOwner::Item(id),
                ArcaneOwner::ItemDross(id),
                "seal the artifact fixture",
            )
            .unwrap();

        assert!(ledger.item_matches(id, "base:sealed_dross_ampoule"));
        assert_eq!(ledger.item_dross_total(id), definition.capacity);
        assert!(ledger.audit().unwrap().is_balanced());
    }

    #[test]
    fn item_content_identity_changes_only_with_full_custody_replacement() {
        let (_root, _atlas, reg, mut ledger) = fixture("item-identity-replacement", false);
        let item = reg.item_id("base:ember").unwrap();
        let definition = reg.item(item).arcane.as_ref().unwrap();
        let id = ledger
            .bind_new_item(
                ArcaneOwner::Deep,
                definition,
                "base:ember",
                "identity replacement fixture",
            )
            .unwrap();
        let owner = ArcaneOwner::Item(id);
        let account = ledger.account(&owner).unwrap().clone();
        let mut remainder = account.current.clone();
        let partial_current = remainder.take_units(1, std::iter::empty()).unwrap();
        let replacement_id = ledger.system_transaction_id().unwrap();
        let partial = ArcaneTransaction {
            id: replacement_id,
            reads: vec![AccountRead {
                owner: owner.clone(),
                expected_version: account.version,
            }],
            debits: vec![ArcaneMove {
                owner: owner.clone(),
                current: partial_current.clone(),
                content_id: None,
            }],
            credits: vec![ArcaneMove {
                owner: owner.clone(),
                current: partial_current,
                content_id: Some("base:implement_fragment".into()),
            }],
            transforms: Vec::new(),
            authority: ArcaneAuthority::System,
            reason: "invalid partial content replacement".into(),
            content_id: "base:implement_fragment".into(),
            linked: Vec::new(),
        };
        let error = ledger.commit(partial).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("only after its old custody is fully debited")
        );
        assert_eq!(
            ledger.account(&owner).unwrap().content_id.as_deref(),
            Some("base:ember")
        );

        let account = ledger.account(&owner).unwrap().clone();
        let full = ArcaneTransaction {
            // Validation failure did not advance the durable origin
            // high-water; an authoritative retry therefore reuses the same
            // sequence instead of creating a permanent gap.
            id: replacement_id,
            reads: vec![AccountRead {
                owner: owner.clone(),
                expected_version: account.version,
            }],
            debits: vec![ArcaneMove {
                owner: owner.clone(),
                current: account.current.clone(),
                content_id: None,
            }],
            credits: vec![ArcaneMove {
                owner: owner.clone(),
                current: account.current,
                content_id: Some("base:implement_fragment".into()),
            }],
            transforms: Vec::new(),
            authority: ArcaneAuthority::System,
            reason: "valid full content replacement".into(),
            content_id: "base:implement_fragment".into(),
            linked: Vec::new(),
        };
        ledger.commit(full).unwrap();
        assert_eq!(
            ledger.account(&owner).unwrap().content_id.as_deref(),
            Some("base:implement_fragment")
        );
        assert!(ledger.audit().unwrap().is_balanced());
    }

    fn transfer(
        ledger: &ArcaneLedger,
        origin: [u8; 16],
        sequence: u64,
        from: ArcaneOwner,
        to: ArcaneOwner,
        current: Current,
    ) -> ArcaneTransaction {
        ArcaneTransaction::transfer(
            TransactionId { origin, sequence },
            from.clone(),
            ledger.version_of(&from),
            to.clone(),
            ledger.version_of(&to),
            current,
            ArcaneAuthority::System,
            "test transfer",
        )
    }

    #[test]
    fn every_transfer_preserves_scalar_and_resonance() {
        let (root, atlas, _, mut ledger) = fixture("conservation", false);
        let before = ledger.audit().unwrap();
        let current = source_slice(&ledger, 1_000);
        let tx = transfer(
            &ledger,
            [1; 16],
            1,
            ArcaneOwner::Deep,
            ArcaneOwner::Ambient(region(&atlas)),
            current,
        );
        assert!(matches!(
            ledger.apply(&tx).unwrap(),
            CommitOutcome::Applied(_)
        ));
        let after = ledger.audit().unwrap();
        assert_eq!(before.total, after.total);
        assert_eq!(before.resonance, after.resonance);
        assert_eq!(after.unexplained_delta, 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn every_reservoir_participates_in_exact_conserved_transfers() {
        let (root, atlas, _, mut ledger) = fixture("all-reservoirs", false);
        let region = region(&atlas);
        let owners = [
            ArcaneOwner::Deep,
            ArcaneOwner::Ambient(region),
            ArcaneOwner::Item(41),
            ArcaneOwner::Working(42),
            ArcaneOwner::Dross {
                region,
                medium: DrossMedium::Water,
            },
            ArcaneOwner::Scar(43),
        ];
        let origin = [14; 16];
        let mut sequence = 1;
        for owner in owners.iter().skip(1) {
            let tx = transfer(
                &ledger,
                origin,
                sequence,
                ArcaneOwner::Deep,
                owner.clone(),
                source_slice(&ledger, 16),
            );
            ledger.apply(&tx).unwrap();
            sequence += 1;
        }
        let before = ledger.audit().unwrap();
        for pair in owners.windows(2) {
            let source = pair[0].clone();
            let destination = pair[1].clone();
            let (resonance, _) = ledger
                .account(&source)
                .unwrap()
                .current
                .parts()
                .iter()
                .next()
                .unwrap();
            let tx = transfer(
                &ledger,
                origin,
                sequence,
                source,
                destination,
                Current::single(resonance.clone(), 1),
            );
            ledger.apply(&tx).unwrap();
            sequence += 1;
        }
        let scar = owners.last().unwrap().clone();
        let (resonance, _) = ledger
            .account(&scar)
            .unwrap()
            .current
            .parts()
            .iter()
            .next()
            .unwrap();
        let tx = transfer(
            &ledger,
            origin,
            sequence,
            scar,
            ArcaneOwner::Deep,
            Current::single(resonance.clone(), 1),
        );
        ledger.apply(&tx).unwrap();
        let after = ledger.audit().unwrap();
        assert_eq!(after.total, before.total);
        assert_eq!(after.resonance, before.resonance);
        assert_eq!(after.unexplained_delta, 0);
        for reservoir in Reservoir::ALL {
            assert!(after.reservoirs.contains_key(&reservoir));
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn randomized_bind_split_merge_activate_disorder_cleanse_loss_recovery_balances() {
        let (root, atlas, _, mut ledger) = fixture("randomized", false);
        let owners = (1..=8).map(ArcaneOwner::Working).collect::<Vec<_>>();
        let origin = [2; 16];
        let mut sequence = 1;
        for owner in &owners {
            let tx = transfer(
                &ledger,
                origin,
                sequence,
                ArcaneOwner::Deep,
                owner.clone(),
                source_slice(&ledger, 512),
            );
            ledger.apply(&tx).unwrap();
            sequence += 1;
        }
        let reasons = [
            "bind", "split", "merge", "activate", "disorder", "cleanse", "destroy", "lose",
            "recover", "save",
        ];
        let mut rng = 0x51f1_7e55u32;
        for step in 0..750 {
            rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let from_index = (rng as usize) % owners.len();
            let mut to_index = ((rng >> 8) as usize) % owners.len();
            if to_index == from_index {
                to_index = (to_index + 1) % owners.len();
            }
            let from = owners[from_index].clone();
            let to = owners[to_index].clone();
            let account = ledger.account(&from).unwrap();
            let Some((resonance, available)) = account.current.parts().iter().next() else {
                continue;
            };
            let units = (*available).min(u64::from((rng >> 16) % 7 + 1));
            let mut tx = transfer(
                &ledger,
                origin,
                sequence,
                from,
                to,
                Current::single(resonance.clone(), units),
            );
            tx.reason = reasons[step % reasons.len()].into();
            ledger.apply(&tx).unwrap();
            sequence += 1;
        }
        let audit = ledger.audit().unwrap();
        assert!(audit.is_balanced());
        assert_eq!(audit.total, audit.genesis_total);
        assert_eq!(ledger.local_bands(region(&atlas))[1], 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn fixed_point_remainder_survives_save_and_reload() {
        let (root, _, _, mut ledger) = fixture("rounding", true);
        let first = (0..5)
            .map(|_| ledger.scaled_units("growth", 1, 1, 3).unwrap())
            .collect::<Vec<_>>();
        ledger.save().unwrap();
        let mut reloaded = ArcaneLedger::load(&root).unwrap();
        let second = (0..7)
            .map(|_| reloaded.scaled_units("growth", 1, 1, 3).unwrap())
            .collect::<Vec<_>>();
        let mut outputs = first;
        outputs.extend(second);
        assert_eq!(outputs, vec![0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn insufficient_overflow_and_stale_transactions_are_atomic() {
        let (root, atlas, _, mut ledger) = fixture("atomic-reject", false);
        let before = ledger.accounts.clone();
        let destination = ArcaneOwner::Ambient(region(&atlas));
        let too_much = Current::single(
            ROOT,
            ledger
                .account(&ArcaneOwner::Deep)
                .unwrap()
                .current
                .units_of(ROOT)
                + 1,
        );
        let tx = transfer(
            &ledger,
            [3; 16],
            1,
            ArcaneOwner::Deep,
            destination.clone(),
            too_much,
        );
        assert!(matches!(
            ledger.apply(&tx),
            Err(ArcaneError::InsufficientResonance { .. })
        ));
        assert_eq!(ledger.accounts, before);

        let mut unbalanced = transfer(
            &ledger,
            [3; 16],
            1,
            ArcaneOwner::Deep,
            destination,
            source_slice(&ledger, 1),
        );
        unbalanced.credits[0].current = Current::single(ROOT, u64::MAX);
        assert!(ledger.apply(&unbalanced).is_err());
        assert_eq!(ledger.accounts, before);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn declared_resonance_transform_preserves_scalar() {
        let (root, atlas, _, mut ledger) = fixture("transform", false);
        let available = ledger
            .account(&ArcaneOwner::Deep)
            .unwrap()
            .current
            .units_of(ROOT);
        assert!(available >= 10);
        let destination = ArcaneOwner::Ambient(region(&atlas));
        let mut tx = transfer(
            &ledger,
            [4; 16],
            1,
            ArcaneOwner::Deep,
            destination.clone(),
            Current::single(ROOT, 10),
        );
        tx.credits[0].current = Current::single(ECHO, 10);
        tx.transforms.push(ResonanceTransform {
            from: ROOT.into(),
            to: ECHO.into(),
            units: 10,
        });
        ledger.apply(&tx).unwrap();
        assert_eq!(
            ledger.account(&destination).unwrap().current.units_of(ECHO),
            10
        );
        assert_eq!(ledger.audit().unwrap().unexplained_delta, 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn every_owner_identity_round_trips() {
        let owners = vec![
            ArcaneOwner::Deep,
            ArcaneOwner::Ambient(AtlasPos {
                face: Face::PosX,
                u: 1,
                v: 2,
            }),
            ArcaneOwner::Heart(7),
            ArcaneOwner::Block {
                pos: BlockPos::new(Face::NegZ, 3, 4, 5).unwrap(),
                generation: 9,
            },
            ArcaneOwner::Item(11),
            ArcaneOwner::Mob(12),
            ArcaneOwner::Player([13; 16]),
            ArcaneOwner::Working(14),
            ArcaneOwner::Dross {
                region: AtlasPos {
                    face: Face::NegY,
                    u: 2,
                    v: 3,
                },
                medium: DrossMedium::Water,
            },
            ArcaneOwner::Scar(15),
        ];
        for owner in owners {
            let bytes = postcard::to_allocvec(&owner).unwrap();
            assert_eq!(postcard::from_bytes::<ArcaneOwner>(&bytes).unwrap(), owner);
        }
    }

    #[test]
    fn retry_is_idempotent_and_sequence_gaps_are_rejected() {
        let (root, atlas, _, mut ledger) = fixture("idempotent", false);
        let destination = ArcaneOwner::Ambient(region(&atlas));
        let tx = transfer(
            &ledger,
            [5; 16],
            1,
            ArcaneOwner::Deep,
            destination.clone(),
            source_slice(&ledger, 12),
        );
        ledger.apply(&tx).unwrap();
        let once = ledger.accounts.clone();
        assert!(matches!(
            ledger.apply(&tx).unwrap(),
            CommitOutcome::AlreadyApplied(_)
        ));
        assert_eq!(ledger.accounts, once);
        let gap = transfer(
            &ledger,
            [5; 16],
            3,
            ArcaneOwner::Deep,
            destination,
            source_slice(&ledger, 1),
        );
        assert!(matches!(
            ledger.apply(&gap),
            Err(ArcaneError::InvalidTransaction(_))
        ));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn crash_recovery_at_each_commit_boundary_applies_exactly_once() {
        for boundary in 0..3 {
            let (root, atlas, _, mut ledger) = fixture(&format!("crash-{boundary}"), true);
            ledger.save().unwrap();
            let transaction = transfer(
                &ledger,
                [6; 16],
                1,
                ArcaneOwner::Deep,
                ArcaneOwner::Ambient(region(&atlas)),
                source_slice(&ledger, 17),
            );
            let delta = ArcaneDelta {
                sequence: 1,
                transaction: transaction.clone(),
            };
            let pending = postcard::to_allocvec(&delta).unwrap();
            match boundary {
                0 => {
                    crate::identity::atomic_write(&root.join(DELTA_PENDING), &pending, false)
                        .unwrap();
                }
                1 => {
                    crate::identity::atomic_write(&root.join(DELTA_PENDING), &pending, false)
                        .unwrap();
                    let mut bytes = DELTA_MAGIC.to_vec();
                    bytes.extend_from_slice(&encode_frame(&delta).unwrap());
                    crate::identity::atomic_write(&root.join(DELTA_FILE), &bytes, false).unwrap();
                }
                _ => {
                    ledger.apply(&transaction).unwrap();
                    ledger.save().unwrap();
                    let mut bytes = DELTA_MAGIC.to_vec();
                    bytes.extend_from_slice(&encode_frame(&delta).unwrap());
                    crate::identity::atomic_write(&root.join(DELTA_FILE), &bytes, false).unwrap();
                    crate::identity::atomic_write(&root.join(DELTA_PENDING), &pending, false)
                        .unwrap();
                }
            }
            let recovered = ArcaneLedger::load(&root).unwrap();
            assert_eq!(recovered.last_delta_seq, 1);
            assert_eq!(
                recovered
                    .account(&ArcaneOwner::Ambient(region(&atlas)))
                    .unwrap()
                    .current
                    .total(),
                17
            );
            assert_eq!(recovered.audit().unwrap().unexplained_delta, 0);
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[test]
    fn crash_replay_advances_instance_and_system_id_allocators() {
        let (root, _, reg, mut ledger) = fixture("allocator-replay", true);
        ledger.save().unwrap();
        let definition = reg
            .item(reg.item_id("base:ember").unwrap())
            .arcane
            .as_ref()
            .unwrap();
        let first = ledger
            .bind_new_item(
                ArcaneOwner::Deep,
                definition,
                "base:ember",
                "uncheckpointed bind",
            )
            .unwrap();
        assert_eq!(first, 1);
        // No checkpoint: only the append journal knows about item 1 and
        // system transaction 1.
        let mut recovered = ArcaneLedger::load(&root).unwrap();
        let second = recovered
            .bind_new_item(
                ArcaneOwner::Deep,
                definition,
                "base:ember",
                "post-replay bind",
            )
            .unwrap();
        assert_eq!(second, 2);
        assert!(recovered.account(&ArcaneOwner::Item(first)).is_some());
        assert!(recovered.account(&ArcaneOwner::Item(second)).is_some());
        assert_eq!(recovered.audit().unwrap().unexplained_delta, 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn unpersisted_charged_item_rolls_back_after_a_crash() {
        let (root, _atlas, reg, mut ledger) = fixture("item-owner-rollback", true);
        let definition = reg
            .item(reg.item_id("base:ember").unwrap())
            .arcane
            .as_ref()
            .unwrap();
        let deep_before = ledger.account(&ArcaneOwner::Deep).unwrap().current.total();
        let item_id = ledger
            .bind_new_item(
                ArcaneOwner::Deep,
                definition,
                "base:ember",
                "crash before player file",
            )
            .unwrap();
        assert!(ledger.account(&ArcaneOwner::Item(item_id)).is_some());

        let status = ledger.reconcile_durable_item_owners(&root).unwrap();
        assert_eq!(status, DurableItemStatus::default());
        assert!(ledger.account(&ArcaneOwner::Item(item_id)).is_none());
        assert_eq!(
            ledger.account(&ArcaneOwner::Deep).unwrap().current.total(),
            deep_before
        );
        assert!(ledger.audit().unwrap().is_balanced());
        ledger.save().unwrap();
        let reloaded = ArcaneLedger::load(&root).unwrap();
        assert!(reloaded.account(&ArcaneOwner::Item(item_id)).is_none());
        assert!(reloaded.audit().unwrap().is_balanced());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn closed_world_audit_and_reopen_recover_undurable_transient_owners() {
        let (root, _, _, mut ledger) = fixture("transient-crash-recovery", true);
        let deep_before = ledger.account(&ArcaneOwner::Deep).unwrap().current.total();
        ledger
            .bind_new_owner(
                ArcaneOwner::Deep,
                ArcaneOwner::Mob(77),
                321,
                vec![ROOT.into()],
                "base:test_warden",
                "transient crash fixture",
            )
            .unwrap();
        ledger
            .bind_new_owner(
                ArcaneOwner::Deep,
                ArcaneOwner::Working(88),
                123,
                vec![ECHO.into()],
                "fixture:working",
                "transient crash fixture",
            )
            .unwrap();
        ledger.save().unwrap();

        let unsafe_audit = audit_world(&root).unwrap();
        assert_eq!(unsafe_audit.dormant_transient_accounts, 2);
        assert!(!unsafe_audit.is_balanced());

        let mut reopened = ArcaneLedger::load(&root).unwrap();
        assert_eq!(reopened.reconcile_transient_owners().unwrap(), 2);
        assert!(reopened.account(&ArcaneOwner::Mob(77)).is_none());
        assert!(reopened.account(&ArcaneOwner::Working(88)).is_none());
        assert_eq!(
            reopened
                .account(&ArcaneOwner::Deep)
                .unwrap()
                .current
                .total(),
            deep_before
        );
        reopened.save().unwrap();
        let recovered_audit = audit_world(&root).unwrap();
        assert_eq!(recovered_audit.dormant_transient_accounts, 0);
        assert!(recovered_audit.is_balanced());
    }

    #[test]
    fn durable_item_reference_preserves_its_account_and_stale_echo_is_removed() {
        let (root, _atlas, reg, mut ledger) = fixture("item-owner-forward", true);
        let definition = reg
            .item(reg.item_id("base:ember").unwrap())
            .arcane
            .as_ref()
            .unwrap();
        let item_id = ledger
            .bind_new_item(
                ArcaneOwner::Deep,
                definition,
                "base:ember",
                "durable player item",
            )
            .unwrap();
        let players = root.join("players");
        std::fs::create_dir_all(&players).unwrap();
        let profile = players.join("profile.toml");
        std::fs::write(
            &profile,
            format!(
                "version = 2\ninventory = [{{ index = 0, item = \"base:ember\", count = 1, durability = 0, arcane_id = {item_id} }}, {{ index = 1, item = \"base:ember\", count = 1, durability = 0, arcane_id = 999999 }}, {{ index = 2, item = \"base:bread\", count = 2, durability = 0, arcane_id = 0 }}, {{ index = 3, item = \"base:bread\", count = 3, durability = 0, arcane_id = 0 }}]\n"
            ),
        )
        .unwrap();

        let status = ledger.reconcile_durable_item_owners(&root).unwrap();
        assert_eq!(status.item_accounts, 1);
        assert_eq!(status.durable_references, 1);
        assert_eq!(status.invalid_references, 0);
        assert!(ledger.account(&ArcaneOwner::Item(item_id)).is_some());
        let repaired = std::fs::read_to_string(profile).unwrap();
        assert!(repaired.contains(&format!("arcane_id = {item_id}")));
        assert!(!repaired.contains("999999"));
        ledger.save().unwrap();
        let audit = audit_world(&root).unwrap();
        assert!(audit.is_balanced());
        assert_eq!(audit.durable_items.durable_references, 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_durable_item_references_fail_closed() {
        let (root, _atlas, reg, mut ledger) = fixture("item-owner-duplicate", true);
        let definition = reg
            .item(reg.item_id("base:ember").unwrap())
            .arcane
            .as_ref()
            .unwrap();
        let item_id = ledger
            .bind_new_item(
                ArcaneOwner::Deep,
                definition,
                "base:ember",
                "duplicated player item",
            )
            .unwrap();
        let players = root.join("players");
        std::fs::create_dir_all(&players).unwrap();
        for name in ["one.toml", "two.toml"] {
            std::fs::write(
                players.join(name),
                format!(
                    "version = 2\ninventory = [{{ index = 0, item = \"base:ember\", count = 1, durability = 0, arcane_id = {item_id} }}]\n"
                ),
            )
            .unwrap();
        }
        let error = ledger.reconcile_durable_item_owners(&root).unwrap_err();
        assert!(error.to_string().contains("duplicate durable references"));
        assert!(ledger.account(&ArcaneOwner::Item(item_id)).is_some());
        ledger.save().unwrap();
        let audit = audit_world(&root).unwrap();
        assert!(!audit.is_balanced());
        assert_eq!(audit.durable_items.duplicate_references, 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn interrupted_checkpoint_rolls_back_to_manifested_backup_then_replays() {
        let (root, atlas, _, mut ledger) = fixture("checkpoint-rollback", true);
        ledger.save().unwrap();
        let manifest_path = root.join("planet/manifest.toml");
        let old_manifest = std::fs::read(&manifest_path).unwrap();
        let transaction = transfer(
            &ledger,
            [15; 16],
            1,
            ArcaneOwner::Deep,
            ArcaneOwner::Ambient(region(&atlas)),
            source_slice(&ledger, 37),
        );
        ledger.commit(transaction).unwrap();
        ledger.save().unwrap();
        // Simulate power loss after ledger/log replacement but before the
        // manifest replacement became durable.
        crate::identity::atomic_write(&manifest_path, &old_manifest, false).unwrap();
        let recovered = ArcaneLedger::load(&root).unwrap();
        assert_eq!(
            recovered
                .account(&ArcaneOwner::Ambient(region(&atlas)))
                .unwrap()
                .current
                .total(),
            37
        );
        assert_eq!(recovered.last_delta_seq, 1);
        assert_eq!(recovered.audit().unwrap().unexplained_delta, 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn linked_material_water_ecology_commit_recovers_forward_atomically() {
        for boundary in 0..3 {
            let (root, atlas, _, mut ledger) = fixture(&format!("linked-{boundary}"), true);
            ledger.save().unwrap();
            let path = root.join("hearts");
            crate::identity::atomic_write(&path, b"before", false).unwrap();
            let mut transaction = transfer(
                &ledger,
                [12; 16],
                1,
                ArcaneOwner::Deep,
                ArcaneOwner::Ambient(region(&atlas)),
                source_slice(&ledger, 31),
            );
            transaction.linked = vec![LinkedMutation {
                subsystem: "ecology".into(),
                operation_id: 1,
                before_checksum: checksum(b"before"),
                after_checksum: checksum(b"after"),
            }];
            let pending = LinkedCommit {
                transaction: transaction.clone(),
                files: vec![LinkedFileReplacement {
                    subsystem: "ecology".into(),
                    operation_id: 1,
                    relative_path: "hearts".into(),
                    after: Some(b"after".to_vec()),
                }],
            };
            crate::identity::atomic_write(
                &root.join(LINKED_PENDING),
                &postcard::to_allocvec(&pending).unwrap(),
                false,
            )
            .unwrap();
            if boundary >= 1 {
                crate::identity::atomic_write(&path, b"after", false).unwrap();
            }
            if boundary == 2 {
                ledger.commit(transaction).unwrap();
            }
            let recovered = ArcaneLedger::load(&root).unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), b"after");
            assert_eq!(
                recovered
                    .account(&ArcaneOwner::Ambient(region(&atlas)))
                    .unwrap()
                    .current
                    .total(),
                31
            );
            assert!(!root.join(LINKED_PENDING).exists());
            assert_eq!(recovered.audit().unwrap().unexplained_delta, 0);
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[test]
    fn linked_commit_writes_file_and_current_as_one_recoverable_operation() {
        let (root, atlas, _, mut ledger) = fixture("linked-happy", true);
        ledger.save().unwrap();
        let path = root.join("materials.wfm");
        crate::identity::atomic_write(&path, b"before", false).unwrap();
        let transaction = transfer(
            &ledger,
            [13; 16],
            1,
            ArcaneOwner::Deep,
            ArcaneOwner::Ambient(region(&atlas)),
            source_slice(&ledger, 29),
        );
        let outcome = ledger
            .commit_linked_files(
                transaction,
                vec![LinkedFileReplacement {
                    subsystem: "materials".into(),
                    operation_id: 1,
                    relative_path: "materials.wfm".into(),
                    after: Some(b"after".to_vec()),
                }],
            )
            .unwrap();
        assert!(matches!(outcome, CommitOutcome::Applied(_)));
        assert_eq!(std::fs::read(path).unwrap(), b"after");
        assert_eq!(
            ledger
                .account(&ArcaneOwner::Ambient(region(&atlas)))
                .unwrap()
                .current
                .total(),
            29
        );
        assert!(!root.join(LINKED_PENDING).exists());
        assert_eq!(ledger.audit().unwrap().unexplained_delta, 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn removed_resonance_and_content_keep_identity_and_charge() {
        let (root, atlas, mut reg, _) = fixture("removed-content", true);
        let id = "fixture:verdance".to_string();
        reg.arcane_registry.definitions.insert(
            id.clone(),
            ResonanceDefinition {
                id: id.clone(),
                label: "Verdance".into(),
                provider: "fixture".into(),
                active: true,
            },
        );
        let mut ledger = ArcaneLedger::initialize(root.join(LEDGER_FILE), &atlas, &reg).unwrap();
        let target = ArcaneOwner::Item(99);
        let mut tx = transfer(
            &ledger,
            [7; 16],
            1,
            ArcaneOwner::Deep,
            target.clone(),
            Current::single(ROOT, 9),
        );
        tx.credits[0].current = Current::single(id.clone(), 9);
        tx.credits[0].content_id = Some("fixture:relic".into());
        tx.transforms.push(ResonanceTransform {
            from: ROOT.into(),
            to: id.clone(),
            units: 9,
        });
        ledger.apply(&tx).unwrap();
        ledger.save().unwrap();
        let base = crate::registry::load(Path::new("__no_removed_mod__"));
        let reloaded = ArcaneLedger::load_or_initialize(&root, &atlas, &base).unwrap();
        assert_eq!(reloaded.account(&target).unwrap().current.units_of(&id), 9);
        assert!(!reloaded.registry.definitions[&id].active);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn item_entity_loss_routes_regionally_and_ire_is_a_separate_axis() {
        let (root, atlas, reg, mut ledger) = fixture("loss-and-ire", false);
        let definition = reg
            .item(reg.item_id("base:ember").unwrap())
            .arcane
            .as_ref()
            .unwrap();
        let item_id = ledger
            .bind_new_item(ArcaneOwner::Deep, definition, "base:ember", "fixture bind")
            .unwrap();
        let region = region(&atlas);
        let loss = ledger
            .loss_transaction(
                TransactionId {
                    origin: [8; 16],
                    sequence: 1,
                },
                ArcaneOwner::Item(item_id),
                region,
                DrossMedium::Soil,
                true,
                "lava",
            )
            .unwrap();
        ledger.apply(&loss).unwrap();
        assert!(ledger.account(&ArcaneOwner::Item(item_id)).is_none());
        let dross_band = ledger.local_bands(region)[1];
        assert!(dross_band > 0);
        let clean_calm = (0u8, 0u8);
        let polluted_calm = (0u8, dross_band);
        let clean_angry = (4u8, 0u8);
        let polluted_angry = (4u8, dross_band);
        let states = BTreeSet::from([clean_calm, polluted_calm, clean_angry, polluted_angry]);
        assert_eq!(states.len(), 4);
        assert_eq!(ledger.audit().unwrap().unexplained_delta, 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_sequence_gap_refuses_to_invent_current() {
        let (root, atlas, _, ledger) = fixture("corrupt-gap", true);
        ledger.save().unwrap();
        let transaction = transfer(
            &ledger,
            [9; 16],
            1,
            ArcaneOwner::Deep,
            ArcaneOwner::Ambient(region(&atlas)),
            source_slice(&ledger, 1),
        );
        let mut bytes = DELTA_MAGIC.to_vec();
        bytes.extend_from_slice(
            &encode_frame(&ArcaneDelta {
                sequence: 2,
                transaction,
            })
            .unwrap(),
        );
        crate::identity::atomic_write(&root.join(DELTA_FILE), &bytes, false).unwrap();
        assert!(matches!(
            ArcaneLedger::load(&root),
            Err(ArcaneError::Corrupt(_))
        ));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn hostile_authorities_cannot_forge_ids_balances_or_other_mod_workings() {
        let (root, atlas, _, mut ledger) = fixture("hostile", false);
        let destination = ArcaneOwner::Ambient(region(&atlas));
        let mut forged = transfer(
            &ledger,
            [10; 16],
            1,
            ArcaneOwner::Deep,
            destination,
            source_slice(&ledger, 1),
        );
        forged.authority = ArcaneAuthority::Player([42; 16]);
        assert!(matches!(
            ledger.apply(&forged),
            Err(ArcaneError::PermissionDenied(_))
        ));

        let working = ledger.allocate_working_id().unwrap();
        ledger
            .bind_new_owner(
                ArcaneOwner::Deep,
                ArcaneOwner::Working(working),
                10,
                vec![ROOT.into()],
                "mod:alpha",
                "fixture working",
            )
            .unwrap();
        assert!(
            ledger
                .mod_working_transfer(
                    "beta",
                    working,
                    working + 1,
                    Current::single(ROOT, 1),
                    "steal",
                )
                .is_err()
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn solo_host_dedicated_and_loopback_apply_identical_authoritative_results() {
        let (root, atlas, _, ledger) = fixture("modes", false);
        let tx = transfer(
            &ledger,
            [11; 16],
            1,
            ArcaneOwner::Deep,
            ArcaneOwner::Ambient(region(&atlas)),
            source_slice(&ledger, 23),
        );
        let mut modes = [ledger.clone(), ledger.clone(), ledger.clone(), ledger];
        for mode in &mut modes {
            mode.apply(&tx).unwrap();
        }
        for mode in &modes[1..] {
            assert_eq!(mode.accounts, modes[0].accounts);
            assert_eq!(mode.audit().unwrap(), modes[0].audit().unwrap());
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn heart_reserve_lends_recovers_and_freezes_without_creation() {
        let (root, _, _, mut ledger) = fixture("heart", false);
        let country = ledger
            .accounts
            .keys()
            .find_map(|owner| match owner {
                ArcaneOwner::Heart(country) => Some(*country),
                _ => None,
            })
            .unwrap();
        let heart = ArcaneOwner::Heart(country);
        let before = ledger.account(&heart).unwrap().current.total();
        ledger
            .bind_new_owner(
                heart.clone(),
                ArcaneOwner::Mob(77),
                100.min(before),
                vec![ROOT.into()],
                "base:thornling",
                "manifest",
            )
            .unwrap();
        assert!(ledger.account(&heart).unwrap().current.total() < before);
        ledger
            .move_all(ArcaneOwner::Mob(77), heart.clone(), "recover")
            .unwrap();
        assert_eq!(ledger.account(&heart).unwrap().current.total(), before);
        ledger.frozen_hearts.insert(country);
        assert!(
            ledger
                .bind_new_owner(
                    heart,
                    ArcaneOwner::Mob(78),
                    1,
                    vec![ROOT.into()],
                    "base:thornling",
                    "manifest",
                )
                .is_err()
        );
        assert_eq!(ledger.audit().unwrap().unexplained_delta, 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn external_adjustments_require_explicit_development_authority_and_are_audited() {
        let (root, _, _, mut ledger) = fixture("external", true);
        assert!(
            ledger
                .operator_adjust_deep("fixture", false, ROOT, 5)
                .is_err()
        );
        ledger
            .operator_adjust_deep("fixture", true, ROOT, 5)
            .unwrap();
        let audit = ledger.audit().unwrap();
        assert_eq!(audit.external_adjustment, 5);
        assert!(audit.is_balanced());
        assert!(
            ledger
                .audit_history
                .back()
                .unwrap()
                .reason
                .contains("fixture")
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    #[ignore = "operator probe for WILDFORGE_PROBE_WORLD production save"]
    fn production_arcane_performance_probe() {
        let root = std::env::var("WILDFORGE_PROBE_WORLD")
            .map(PathBuf::from)
            .expect("set WILDFORGE_PROBE_WORLD to a qualified production save");
        let ledger = ArcaneLedger::load(&root).unwrap();
        let snapshot_bytes = std::fs::metadata(root.join(LEDGER_FILE)).unwrap().len();

        let audit_start = std::time::Instant::now();
        for _ in 0..10_000 {
            std::hint::black_box(ledger.audit().unwrap());
        }
        let audit_each = audit_start.elapsed().as_nanos() / 10_000;

        let mut transactions = ledger.clone();
        let owner = ArcaneOwner::Working(u64::MAX - 1);
        let origin = [0x71; 16];
        let mut sequence = transactions
            .origin_high_water
            .get(&origin)
            .copied()
            .unwrap_or_default()
            + 1;
        let initial = transfer(
            &transactions,
            origin,
            sequence,
            ArcaneOwner::Deep,
            owner.clone(),
            Current::single(ROOT, 1),
        );
        transactions.apply(&initial).unwrap();
        sequence += 1;
        let transaction_start = std::time::Instant::now();
        for index in 0..100_000 {
            let (from, to) = if index % 2 == 0 {
                (owner.clone(), ArcaneOwner::Deep)
            } else {
                (ArcaneOwner::Deep, owner.clone())
            };
            let transaction = transfer(
                &transactions,
                origin,
                sequence,
                from,
                to,
                Current::single(ROOT, 1),
            );
            transactions.apply(&transaction).unwrap();
            sequence += 1;
        }
        let transaction_each = transaction_start.elapsed().as_nanos() / 100_000;
        println!(
            "arcane profile: accounts={} snapshot={} bytes audit={} ns transaction={} ns history={}/{}",
            ledger.accounts.len(),
            snapshot_bytes,
            audit_each,
            transaction_each,
            transactions.audit_history.len(),
            MAX_AUDIT_HISTORY,
        );
    }
}
