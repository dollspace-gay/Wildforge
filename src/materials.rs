//! Finite-material manifest, conservation ledger, salvage pools, and audit.
//!
//! The ledger tracks economically meaningful integer units, not simulated
//! atoms. Geological material begins in an immutable deposit account and can
//! only move between named compartments or an explicit sink/source.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk};
use crate::inventory::ItemStack;
use crate::planet::{BlockPos, ChunkPos, Face, SurfacePoint, geodesic_distance};
use crate::planet_atlas::{BedrockFamily, MineralKind, PlanetAtlas};
use crate::registry::{ItemId, MaterialClass, MaterialVector, Registry};

pub const MATERIAL_LEDGER_SCHEMA: u32 = 1;
pub const CANONICAL_INGOT_UNITS: u64 = 1_200;
pub const REQUIRED_TECHNOLOGY_ARCS: u64 = 16;
const LEDGER_FILE: &str = "materials.wfm";
const LEDGER_BACKUP: &str = "materials.wfm.bak";
const DELTA_FILE: &str = "materials.wfm.log";
const DELTA_BACKUP: &str = "materials.wfm.log.bak";
const DELTA_PENDING: &str = "materials.wfm.log.pending";
const DELTA_MAGIC: &[u8; 4] = b"WMD1";
const MAX_LEDGER_BYTES: u64 = 64 * 1024 * 1024;
const MAX_DELTA_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Default)]
struct DirtyFlag(AtomicBool);

impl DirtyFlag {
    fn get(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    fn set(&self, value: bool) {
        self.0.store(value, Ordering::Relaxed);
    }
}

impl Clone for DirtyFlag {
    fn clone(&self) -> Self {
        Self(AtomicBool::new(self.get()))
    }
}

impl PartialEq for DirtyFlag {
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

impl Eq for DirtyFlag {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DepositAccount {
    pub id: u32,
    pub resource_family: String,
    pub resource_key: String,
    pub site: crate::planet_atlas::AtlasPos,
    pub radius_blocks: u16,
    pub host_geology: BedrockFamily,
    pub grade_ppm: u32,
    pub original_recoverable: MaterialVector,
    pub materialized_underground: MaterialVector,
    pub extracted: MaterialVector,
    pub remaining_unmaterialized: MaterialVector,
    pub tailings_secondary: MaterialVector,
    pub discovered: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReservationSlice {
    pub deposit_id: u32,
    pub count: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChunkReservation {
    pub chunk: ChunkPos,
    /// Stable block name -> deposit slices. Counts are exact ore voxels.
    pub blocks: BTreeMap<String, Vec<ReservationSlice>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct SalvageRegion {
    pub face: Face,
    pub u: u8,
    pub v: u8,
}

impl SalvageRegion {
    pub const BLOCKS: u16 = 256;

    pub const fn at(pos: BlockPos) -> Self {
        Self {
            face: pos.face(),
            u: (pos.u() / Self::BLOCKS) as u8,
            v: (pos.v() / Self::BLOCKS) as u8,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SalvagePool {
    pub materials: MaterialVector,
    pub events: u64,
    pub last_reason: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrogenPolicyRecord {
    UntouchedHostOnly,
    SecondaryRecovery,
    WorldEvent,
    NoRetrogen,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RetrogenRecord {
    pub mod_id: String,
    pub resource_key: String,
    pub policy: RetrogenPolicyRecord,
    pub algorithm_version: u32,
    pub added_mass: MaterialVector,
    pub unit_materials: MaterialVector,
    pub applied_chunks: BTreeSet<ChunkPos>,
    pub status: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialOperationKind {
    MineNatural,
    BreakPlaced,
    Place,
    AuthoredPlace,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MaterialOperation {
    pub id: u64,
    pub kind: MaterialOperationKind,
    pub pos: BlockPos,
    pub before: String,
    pub after: String,
    pub materials: MaterialVector,
}

impl MaterialOperation {
    const AUTHORED_SOURCE_SEPARATOR: char = '\0';

    /// The pre-edit block name. Authored placement uses the otherwise
    /// impossible NUL suffix to carry its audit source without changing the
    /// serialized operation shape (old pending journals remain readable).
    pub fn before_block_name(&self) -> &str {
        self.before
            .split_once(Self::AUTHORED_SOURCE_SEPARATOR)
            .map_or(self.before.as_str(), |(before, _)| before)
    }

    fn authored_source(&self) -> &str {
        self.before
            .split_once(Self::AUTHORED_SOURCE_SEPARATOR)
            .map_or("authored world edit", |(_, source)| source)
    }
}

/// The material-bearing part of a content definition is persisted with the
/// world. If its mod is absent, a named placeholder can still carry the same
/// mass and later resolve back to the real definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SavedMaterialDefinition {
    pub materials: MaterialVector,
    pub material_class: MaterialClass,
    pub max_stack: u32,
    pub durability: u32,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MaterialQualification {
    pub major_continents: usize,
    pub major_continents_with_flux: usize,
    pub treasure_site_counts: BTreeMap<String, usize>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MaterialLedger {
    pub schema_version: u32,
    pub content_hash: u64,
    pub deposits: BTreeMap<u32, DepositAccount>,
    pub chunk_reservations: BTreeMap<ChunkPos, ChunkReservation>,
    /// Positions are needed only to distinguish a placed ore block from
    /// virgin geology. The aggregate audit lives in `placed`.
    pub placed_positions: BTreeMap<BlockPos, MaterialVector>,
    pub circulating: MaterialVector,
    #[serde(default)]
    pub secondary_physical: MaterialVector,
    pub placed: MaterialVector,
    pub salvage: BTreeMap<SalvageRegion, SalvagePool>,
    pub explicit_consumption: MaterialVector,
    pub explicit_loss: MaterialVector,
    pub external_additions: MaterialVector,
    #[serde(default)]
    pub external_sources: BTreeMap<String, MaterialVector>,
    pub retrogen: BTreeMap<String, RetrogenRecord>,
    #[serde(default)]
    pub item_manifests: BTreeMap<String, SavedMaterialDefinition>,
    #[serde(default)]
    pub block_manifests: BTreeMap<String, SavedMaterialDefinition>,
    #[serde(default)]
    pub qualification: MaterialQualification,
    pub next_operation_id: u64,
    pub last_applied_operation: u64,
    #[serde(default)]
    pub last_delta_seq: u64,
    #[serde(skip)]
    path: PathBuf,
    /// Fresh chunk reservations are deterministic and may be checkpointed as
    /// one batch. They must reach the snapshot before the first journaled
    /// material mutation, but never need four durable file replacements per
    /// streamed chunk.
    #[serde(skip)]
    reservations_dirty: DirtyFlag,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct MaterialDelta {
    seq: u64,
    action: MaterialDeltaAction,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
enum MaterialDeltaAction {
    Operation(MaterialOperation),
    RecipeLoss(MaterialVector),
    Consumption(MaterialVector),
    Bury {
        region: SalvageRegion,
        materials: MaterialVector,
        from_secondary: bool,
        reason: String,
    },
    Recover {
        region: SalvageRegion,
        material: String,
        processed: u64,
        recovered: u64,
    },
    AdminDeletion(MaterialVector),
    SecondaryOutput(MaterialVector),
    SecondaryRecovery(MaterialVector),
    External {
        materials: MaterialVector,
        secondary: bool,
        source: String,
    },
}

impl MaterialLedger {
    pub fn initialize(path: PathBuf, atlas: &PlanetAtlas, reg: &Registry) -> Self {
        let mut deposits = BTreeMap::new();
        for deposit in &atlas.geology.deposits {
            let per_block = mineral_materials(deposit.mineral, reg);
            let mut original = MaterialVector::new();
            add_vector(&mut original, &per_block, deposit.tonnage_blocks);
            // Geodes may express any of their three stable constituents. Each
            // band is finite; a generated block reserves from its own band.
            if deposit.mineral == MineralKind::Geode {
                for material in ["quartz", "amethyst", "sulfur"] {
                    original.insert(
                        material.into(),
                        deposit.tonnage_blocks.saturating_mul(CANONICAL_INGOT_UNITS),
                    );
                }
            }
            deposits.insert(
                deposit.id,
                DepositAccount {
                    id: deposit.id,
                    resource_family: deposit.mineral.label().into(),
                    resource_key: format!("base:{}", deposit.mineral.label().replace(' ', "_")),
                    site: deposit.pos,
                    radius_blocks: deposit.radius_blocks,
                    host_geology: deposit.host,
                    grade_ppm: deposit.grade_ppm,
                    remaining_unmaterialized: original.clone(),
                    original_recoverable: original,
                    materialized_underground: MaterialVector::new(),
                    extracted: MaterialVector::new(),
                    tailings_secondary: MaterialVector::new(),
                    discovered: false,
                },
            );
        }
        let mut ledger = Self {
            schema_version: MATERIAL_LEDGER_SCHEMA,
            content_hash: atlas.manifest.content_hash,
            deposits,
            chunk_reservations: BTreeMap::new(),
            placed_positions: BTreeMap::new(),
            circulating: MaterialVector::new(),
            secondary_physical: MaterialVector::new(),
            placed: MaterialVector::new(),
            salvage: BTreeMap::new(),
            explicit_consumption: MaterialVector::new(),
            explicit_loss: MaterialVector::new(),
            external_additions: MaterialVector::new(),
            external_sources: BTreeMap::new(),
            retrogen: BTreeMap::new(),
            item_manifests: BTreeMap::new(),
            block_manifests: BTreeMap::new(),
            qualification: MaterialQualification::default(),
            next_operation_id: 1,
            last_applied_operation: 0,
            last_delta_seq: 0,
            path,
            reservations_dirty: DirtyFlag::default(),
        };
        ledger.reconcile_mod_manifests(atlas, reg);
        ledger.reconcile_saved_definitions(reg);
        ledger.reconcile_qualification(atlas);
        ledger
    }

    pub fn load_or_initialize(
        world: &Path,
        atlas: &PlanetAtlas,
        reg: &Registry,
    ) -> std::io::Result<Self> {
        let path = world.join(LEDGER_FILE);
        if path.exists() {
            let mut ledger = Self::load(world)?;
            ledger.path = path;
            let changed = ledger.reconcile_mod_manifests(atlas, reg)
                | ledger.reconcile_saved_definitions(reg)
                | ledger.reconcile_qualification(atlas);
            if changed || ledger.content_hash != reg.content_hash {
                ledger.content_hash = reg.content_hash;
                ledger.save()?;
            }
            return Ok(ledger);
        }
        let ledger = Self::initialize(path, atlas, reg);
        ledger.save()?;
        Ok(ledger)
    }

    /// Install genesis manifests or post-creation policy records for every
    /// loaded mod resource. Untouched-host resources receive deterministic
    /// finite accounts cloned from the atlas's generic mod-mineral sites;
    /// other policies add no geological mass.
    pub fn reconcile_mod_manifests(&mut self, atlas: &PlanetAtlas, reg: &Registry) -> bool {
        let mut changed = false;
        let post_creation = self.content_hash != reg.content_hash;
        for ore in reg.ores.iter().filter(|ore| ore.mod_id != "base") {
            if self.retrogen.contains_key(&ore.resource_key) {
                continue;
            }
            let policy = match ore.retrogen {
                crate::registry::RetrogenPolicy::UntouchedHostOnly => {
                    RetrogenPolicyRecord::UntouchedHostOnly
                }
                crate::registry::RetrogenPolicy::SecondaryRecovery => {
                    RetrogenPolicyRecord::SecondaryRecovery
                }
                crate::registry::RetrogenPolicy::WorldEvent => RetrogenPolicyRecord::WorldEvent,
                crate::registry::RetrogenPolicy::NoRetrogen => RetrogenPolicyRecord::NoRetrogen,
            };
            let mut added_mass = MaterialVector::new();
            if !post_creation || matches!(policy, RetrogenPolicyRecord::UntouchedHostOnly) {
                let materials = reg.block(ore.block).materials.clone();
                for site in atlas
                    .geology
                    .deposits
                    .iter()
                    .filter(|site| site.mineral == MineralKind::Other)
                {
                    let mut id = stable_resource_id(&ore.resource_key, site.id);
                    while self.deposits.contains_key(&id) {
                        id = id.wrapping_add(1).max(0x8000_0000);
                    }
                    let original = scaled(&materials, site.tonnage_blocks);
                    add_vector(&mut added_mass, &original, 1);
                    self.deposits.insert(
                        id,
                        DepositAccount {
                            id,
                            resource_family: ore.resource_key.clone(),
                            resource_key: ore.resource_key.clone(),
                            site: site.pos,
                            radius_blocks: site.radius_blocks,
                            host_geology: site.host,
                            grade_ppm: site.grade_ppm,
                            original_recoverable: original.clone(),
                            materialized_underground: MaterialVector::new(),
                            extracted: MaterialVector::new(),
                            remaining_unmaterialized: original,
                            tailings_secondary: MaterialVector::new(),
                            discovered: false,
                        },
                    );
                }
            }
            let status = match (post_creation, policy) {
                (false, RetrogenPolicyRecord::UntouchedHostOnly) => {
                    "included in atlas genesis".to_string()
                }
                (true, RetrogenPolicyRecord::UntouchedHostOnly) => {
                    "deterministic untouched-host retrogen pending chunk visits".to_string()
                }
                (_, RetrogenPolicyRecord::SecondaryRecovery) => {
                    "secondary recovery only; terrain unchanged".to_string()
                }
                (_, RetrogenPolicyRecord::WorldEvent) => {
                    "world event required; no mass added yet".to_string()
                }
                (false, RetrogenPolicyRecord::NoRetrogen) => {
                    "included in atlas genesis; no later retrogen".to_string()
                }
                (true, RetrogenPolicyRecord::NoRetrogen) => {
                    "unavailable in this existing world; create a new planet".to_string()
                }
            };
            self.retrogen.insert(
                ore.resource_key.clone(),
                RetrogenRecord {
                    mod_id: ore.mod_id.clone(),
                    resource_key: ore.resource_key.clone(),
                    policy,
                    algorithm_version: 1,
                    added_mass,
                    unit_materials: reg.block(ore.block).materials.clone(),
                    applied_chunks: BTreeSet::new(),
                    status,
                },
            );
            changed = true;
        }
        changed
    }

    pub fn reconcile_saved_definitions(&mut self, reg: &Registry) -> bool {
        let mut changed = false;
        for item in &reg.items {
            if item.materials.is_empty() {
                continue;
            }
            let saved = SavedMaterialDefinition {
                materials: item.materials.clone(),
                material_class: item.material_class,
                max_stack: item.max_stack,
                durability: item.durability,
            };
            if self.item_manifests.get(&item.name) != Some(&saved) {
                self.item_manifests.insert(item.name.clone(), saved);
                changed = true;
            }
        }
        for block in &reg.blocks {
            if block.materials.is_empty() {
                continue;
            }
            let saved = SavedMaterialDefinition {
                materials: block.materials.clone(),
                material_class: block.material_class,
                max_stack: 64,
                durability: 0,
            };
            if self.block_manifests.get(&block.name) != Some(&saved) {
                self.block_manifests.insert(block.name.clone(), saved);
                changed = true;
            }
        }
        changed
    }

    /// Existing named content may gain presentation or behavior, but its
    /// material identity cannot change under live stacks/voxels without an
    /// explicit migration. Removed content is represented by definitions
    /// reconstructed from these same manifests and therefore passes.
    pub fn validate_saved_definitions(&self, reg: &Registry) -> std::io::Result<()> {
        for item in &reg.items {
            if let Some(saved) = self.item_manifests.get(&item.name)
                && (saved.materials != item.materials
                    || saved.material_class != item.material_class)
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "{} changes saved material identity; an explicit migration is required",
                        item.name
                    ),
                ));
            }
        }
        for block in &reg.blocks {
            if let Some(saved) = self.block_manifests.get(&block.name)
                && (saved.materials != block.materials
                    || saved.material_class != block.material_class)
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "{} changes saved material identity; an explicit migration is required",
                        block.name
                    ),
                ));
            }
        }
        Ok(())
    }

    fn reconcile_qualification(&mut self, atlas: &PlanetAtlas) -> bool {
        let major = atlas
            .geology
            .continents
            .iter()
            .filter(|continent| continent.major)
            .map(|continent| continent.id)
            .collect::<BTreeSet<_>>();
        let flux = atlas
            .genesis
            .terrain
            .values()
            .iter()
            .zip(atlas.genesis.tectonics.values())
            .filter_map(|(terrain, tectonic)| {
                (major.contains(&terrain.landmass_id)
                    && matches!(
                        BedrockFamily::from_id(tectonic.bedrock_family),
                        BedrockFamily::Limestone | BedrockFamily::Marble
                    ))
                .then_some(terrain.landmass_id)
            })
            .collect::<BTreeSet<_>>();
        let treasure_site_counts = [
            ("diamond", MineralKind::Diamond),
            ("rare earth", MineralKind::RareEarth),
            ("pitchblende", MineralKind::Pitchblende),
        ]
        .into_iter()
        .map(|(name, kind)| {
            (
                name.to_string(),
                atlas
                    .geology
                    .deposits
                    .iter()
                    .filter(|site| site.mineral == kind)
                    .count(),
            )
        })
        .collect();
        let qualification = MaterialQualification {
            major_continents: major.len(),
            major_continents_with_flux: flux.len(),
            treasure_site_counts,
        };
        if self.qualification == qualification {
            false
        } else {
            self.qualification = qualification;
            true
        }
    }

    pub fn retrogen_pending_for(&self, resource_key: &str, chunk: ChunkPos) -> bool {
        self.retrogen.get(resource_key).is_some_and(|record| {
            record.policy == RetrogenPolicyRecord::UntouchedHostOnly
                && record.status.contains("pending")
                && !record.applied_chunks.contains(&chunk)
        })
    }

    /// Coordinate-free policy notices safe for ordinary clients. Exact
    /// reserves and eligible chunks remain operator-only audit information.
    pub fn retrogen_notices(&self) -> Vec<String> {
        self.retrogen
            .values()
            .map(|record| {
                let policy = match record.policy {
                    RetrogenPolicyRecord::UntouchedHostOnly => "untouched host only",
                    RetrogenPolicyRecord::SecondaryRecovery => "secondary recovery",
                    RetrogenPolicyRecord::WorldEvent => "world event",
                    RetrogenPolicyRecord::NoRetrogen => "no retrogen",
                };
                format!(
                    "Finite material {} ({policy}): {}.",
                    record.resource_key,
                    record.status.trim_end_matches('.')
                )
            })
            .collect()
    }

    pub fn mark_retrogen_chunk(
        &mut self,
        resource_keys: impl IntoIterator<Item = String>,
        chunk: ChunkPos,
    ) -> std::io::Result<()> {
        for resource_key in resource_keys {
            if let Some(record) = self.retrogen.get_mut(&resource_key) {
                record.applied_chunks.insert(chunk);
            }
        }
        self.save()
    }

    pub fn load(world: &Path) -> std::io::Result<Self> {
        let path = world.join(LEDGER_FILE);
        let bytes = std::fs::read(&path)?;
        if bytes.len() as u64 > MAX_LEDGER_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "material ledger exceeds safety bound",
            ));
        }
        let mut ledger: Self = postcard::from_bytes(&bytes)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        if ledger.schema_version != MATERIAL_LEDGER_SCHEMA {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "unsupported material ledger schema {}",
                    ledger.schema_version
                ),
            ));
        }
        ledger.path = path;
        ledger.replay_delta_log()?;
        Ok(ledger)
    }

    /// Compact the append journal into the full operator-auditable snapshot.
    /// Ordinary play does not call this per action; world saves and manifest
    /// changes do. A snapshot stores `last_delta_seq`, so a crash after the
    /// snapshot rename but before log truncation merely re-encounters already
    /// applied records and cannot duplicate them.
    pub fn save(&self) -> std::io::Result<()> {
        let bytes = postcard::to_allocvec(self).map_err(std::io::Error::other)?;
        if let Ok(old) = std::fs::read(&self.path) {
            crate::identity::atomic_write(&self.path.with_file_name(LEDGER_BACKUP), &old, false)?;
        }
        let delta_path = self.delta_path();
        let old_delta = std::fs::read(&delta_path).unwrap_or_else(|_| DELTA_MAGIC.to_vec());
        crate::identity::atomic_write(&self.path.with_file_name(DELTA_BACKUP), &old_delta, false)?;
        crate::identity::atomic_write(&self.path, &bytes, false)?;
        crate::identity::atomic_write(&delta_path, DELTA_MAGIC, false)?;
        let _ = crate::persist::remove_if_exists(&self.pending_delta_path());
        self.reservations_dirty.set(false);
        Ok(())
    }

    fn checkpoint_reservations(&self) -> std::io::Result<()> {
        if self.reservations_dirty.get() {
            self.save()?;
        }
        Ok(())
    }

    fn delta_path(&self) -> PathBuf {
        self.path.with_file_name(DELTA_FILE)
    }

    fn pending_delta_path(&self) -> PathBuf {
        self.path.with_file_name(DELTA_PENDING)
    }

    #[cfg(test)]
    pub(crate) fn force_checkpoint_failure_at(&mut self, path: PathBuf) {
        self.path = path;
        self.reservations_dirty.set(true);
    }

    fn replay_delta_log(&mut self) -> std::io::Result<()> {
        let path = self.delta_path();
        let pending_path = self.pending_delta_path();
        let pending = match std::fs::read(&pending_path) {
            Ok(bytes) => Some(
                postcard::from_bytes::<MaterialDelta>(&bytes)
                    .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?,
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let mut bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => DELTA_MAGIC.to_vec(),
            Err(error) => return Err(error),
        };
        if bytes.len() as u64 > MAX_DELTA_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "material delta log exceeds safety bound",
            ));
        }
        if !bytes.starts_with(DELTA_MAGIC) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "material delta log has an invalid header",
            ));
        }

        let (mut records, valid_end, complete) = decode_delta_frames(&bytes);
        if !complete && pending.is_none() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "material delta log has a corrupt or truncated tail",
            ));
        }
        if let Some(pending) = pending {
            let logged = records.iter().any(|record| record.seq == pending.seq);
            if !complete || !logged {
                let last = records
                    .last()
                    .map_or(self.last_delta_seq, |record| record.seq);
                if pending.seq != last.saturating_add(1) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "pending material delta does not continue the durable sequence",
                    ));
                }
                bytes.truncate(valid_end);
                bytes.extend_from_slice(&encode_delta_frame(&pending)?);
                crate::identity::atomic_write(&path, &bytes, false)?;
                records.push(pending);
            }
            crate::persist::remove_if_exists(&pending_path)?;
        }

        for record in records {
            if record.seq <= self.last_delta_seq {
                continue;
            }
            if record.seq != self.last_delta_seq.saturating_add(1) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "material delta sequence has a gap",
                ));
            }
            self.apply_delta_action(&record.action)?;
            self.last_delta_seq = record.seq;
        }
        Ok(())
    }

    fn commit_delta(&mut self, action: MaterialDeltaAction) -> std::io::Result<()> {
        self.validate_delta_action(&action)?;
        // A delta may refer to material whose finite account was assigned by
        // a freshly streamed chunk. Land the whole reservation batch before
        // publishing that delta so replay can always validate it.
        self.checkpoint_reservations()?;
        let record = MaterialDelta {
            seq: self.last_delta_seq.saturating_add(1),
            action,
        };
        let payload = postcard::to_allocvec(&record).map_err(std::io::Error::other)?;
        crate::identity::atomic_write(&self.pending_delta_path(), &payload, false)?;
        let frame = encode_delta_frame(&record)?;
        let path = self.delta_path();
        let needs_header = std::fs::metadata(&path).map_or(true, |metadata| metadata.len() == 0);
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        if needs_header {
            file.write_all(DELTA_MAGIC)?;
        }
        file.write_all(&frame)?;
        file.sync_data()?;
        self.apply_delta_action(&record.action)?;
        self.last_delta_seq = record.seq;
        let _ = crate::persist::remove_if_exists(&self.pending_delta_path());
        Ok(())
    }

    fn validate_delta_action(&self, action: &MaterialDeltaAction) -> std::io::Result<()> {
        match action {
            MaterialDeltaAction::Operation(operation)
                if operation.id > self.last_applied_operation
                    && operation.kind == MaterialOperationKind::MineNatural =>
            {
                let reservation = self
                    .chunk_reservations
                    .get(&operation.pos.chunk())
                    .ok_or_else(|| {
                        std::io::Error::other("mined finite block has no chunk reservation")
                    })?;
                let slice = reservation
                    .blocks
                    .get(&operation.before)
                    .and_then(|slices| slices.iter().find(|slice| slice.count != 0))
                    .ok_or_else(|| {
                        std::io::Error::other("finite block reservation is exhausted")
                    })?;
                let account = self
                    .deposits
                    .get(&slice.deposit_id)
                    .ok_or_else(|| std::io::Error::other("reservation names a missing deposit"))?;
                for (material, units) in &operation.materials {
                    if account
                        .materialized_underground
                        .get(material)
                        .copied()
                        .unwrap_or_default()
                        < *units
                    {
                        return Err(std::io::Error::other(format!(
                            "deposit {} lacks reserved {material}",
                            account.id
                        )));
                    }
                }
            }
            MaterialDeltaAction::Recover {
                region,
                material,
                processed,
                recovered,
            } => {
                if recovered > processed {
                    return Err(std::io::Error::other(
                        "salvage recovery cannot exceed processed material",
                    ));
                }
                let available = self
                    .salvage
                    .get(region)
                    .and_then(|pool| pool.materials.get(material))
                    .copied()
                    .unwrap_or_default();
                if available < *processed {
                    return Err(std::io::Error::other(format!(
                        "regional salvage has {available} {material}, needs {processed}"
                    )));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn apply_delta_action(&mut self, action: &MaterialDeltaAction) -> std::io::Result<()> {
        match action {
            MaterialDeltaAction::Operation(operation) => {
                self.apply_operation_in_memory(operation)?;
            }
            MaterialDeltaAction::RecipeLoss(materials) => {
                subtract_or_external(
                    &mut self.circulating,
                    materials,
                    &mut self.external_additions,
                )?;
                add_vector(&mut self.explicit_loss, materials, 1);
            }
            MaterialDeltaAction::Consumption(materials) => {
                subtract_or_external(
                    &mut self.circulating,
                    materials,
                    &mut self.external_additions,
                )?;
                add_vector(&mut self.explicit_consumption, materials, 1);
            }
            MaterialDeltaAction::Bury {
                region,
                materials,
                from_secondary,
                reason,
            } => {
                let source = if *from_secondary {
                    &mut self.secondary_physical
                } else {
                    &mut self.circulating
                };
                subtract_or_external(source, materials, &mut self.external_additions)?;
                let pool = self.salvage.entry(*region).or_default();
                add_vector(&mut pool.materials, materials, 1);
                pool.events = pool.events.saturating_add(1);
                pool.last_reason = reason.clone();
            }
            MaterialDeltaAction::Recover {
                region,
                material,
                processed,
                recovered,
            } => {
                let pool = self.salvage.get_mut(region).ok_or_else(|| {
                    std::io::Error::other("regional salvage pool disappeared during recovery")
                })?;
                subtract_one(&mut pool.materials, material, *processed)?;
                *self.circulating.entry(material.clone()).or_default() += recovered;
                *self.explicit_loss.entry(material.clone()).or_default() += processed - recovered;
                self.salvage.retain(|_, pool| !pool.materials.is_empty());
            }
            MaterialDeltaAction::AdminDeletion(materials) => {
                subtract_or_external(
                    &mut self.circulating,
                    materials,
                    &mut self.external_additions,
                )?;
                add_vector(&mut self.explicit_loss, materials, 1);
            }
            MaterialDeltaAction::SecondaryOutput(materials) => {
                subtract_or_external(
                    &mut self.circulating,
                    materials,
                    &mut self.external_additions,
                )?;
                add_vector(&mut self.secondary_physical, materials, 1);
            }
            MaterialDeltaAction::SecondaryRecovery(materials) => {
                subtract_or_external(
                    &mut self.secondary_physical,
                    materials,
                    &mut self.external_additions,
                )?;
                add_vector(&mut self.circulating, materials, 1);
            }
            MaterialDeltaAction::External {
                materials,
                secondary,
                source,
            } => {
                add_vector(&mut self.external_additions, materials, 1);
                add_vector(
                    self.external_sources.entry(source.clone()).or_default(),
                    materials,
                    1,
                );
                if *secondary {
                    add_vector(&mut self.secondary_physical, materials, 1);
                } else {
                    add_vector(&mut self.circulating, materials, 1);
                }
            }
        }
        Ok(())
    }

    pub fn reserve_fresh_chunk(
        &mut self,
        atlas: &PlanetAtlas,
        reg: &Registry,
        chunk_pos: ChunkPos,
        chunk: &Chunk,
    ) -> std::io::Result<bool> {
        let existing = self.chunk_reservations.get(&chunk_pos).cloned();
        let existing_blocks = existing
            .as_ref()
            .map(|reservation| reservation.blocks.keys().cloned().collect::<BTreeSet<_>>())
            .unwrap_or_default();
        let mut counts = BTreeMap::<String, (MineralKind, MaterialVector, u32)>::new();
        for y in 0..CHUNK_Y {
            for z in 0..CHUNK_Z {
                for x in 0..CHUNK_X {
                    let definition = reg.block(chunk.get(x, y, z));
                    let pos = BlockPos::new(
                        chunk_pos.face(),
                        chunk_pos.u() * CHUNK_X as u16 + x as u16,
                        y as u8,
                        chunk_pos.v() * CHUNK_Z as u16 + z as u16,
                    )
                    .expect("chunk-local material voxel canonicalizes");
                    if definition.materials.is_empty()
                        || existing_blocks.contains(&definition.name)
                        || self.placed_positions.contains_key(&pos)
                    {
                        continue;
                    }
                    let mineral = MineralKind::from_block_name(&definition.name);
                    if mineral == MineralKind::Other
                        && !self
                            .deposits
                            .values()
                            .any(|deposit| deposit.resource_key == definition.name)
                    {
                        continue;
                    }
                    let entry = counts
                        .entry(definition.name.clone())
                        .or_insert_with(|| (mineral, definition.materials.clone(), 0));
                    entry.2 = entry.2.saturating_add(1);
                }
            }
        }

        if counts.is_empty() {
            if existing.is_none() {
                self.chunk_reservations.insert(
                    chunk_pos,
                    ChunkReservation {
                        chunk: chunk_pos,
                        blocks: BTreeMap::new(),
                    },
                );
                self.reservations_dirty.set(true);
                return Ok(true);
            }
            return Ok(false);
        }

        let center = SurfacePoint {
            face: chunk_pos.face(),
            u: f64::from(chunk_pos.u()) * CHUNK_X as f64 + CHUNK_X as f64 * 0.5,
            v: f64::from(chunk_pos.v()) * CHUNK_Z as f64 + CHUNK_Z as f64 * 0.5,
        };
        let mut reservation = existing.unwrap_or(ChunkReservation {
            chunk: chunk_pos,
            blocks: BTreeMap::new(),
        });
        // Stage only the nearby accounts touched by this chunk. A capacity
        // error must not partially debit the live manifest, and cloning the
        // entire planetary ledger on every chunk would make ordinary
        // exploration scale with world history.
        let mut staged_accounts = BTreeMap::<u32, DepositAccount>::new();
        for (block_name, (mineral, materials, mut count)) in counts {
            let resource_key = if mineral == MineralKind::Other {
                block_name.clone()
            } else {
                format!("base:{}", mineral.label().replace(' ', "_"))
            };
            let mut candidates = self
                .deposits
                .values()
                .filter(|deposit| deposit.resource_key == resource_key)
                .filter(|deposit| {
                    geodesic_distance(center, deposit.site.center(atlas.side()))
                        <= f64::from(deposit.radius_blocks)
                })
                .map(|deposit| deposit.id)
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                // Some crystal pockets are stamped by the geode pass rather
                // than the ordinary ore feature pass. They still belong to a
                // finite geode site: select the nearest matching manifest
                // account deterministically rather than inventing mass.
                if let Some(nearest) = self
                    .deposits
                    .values()
                    .filter(|deposit| deposit.resource_key == resource_key)
                    .min_by(|a, b| {
                        geodesic_distance(center, a.site.center(atlas.side()))
                            .total_cmp(&geodesic_distance(center, b.site.center(atlas.side())))
                            .then_with(|| a.id.cmp(&b.id))
                    })
                {
                    candidates.push(nearest.id);
                }
            }
            candidates.sort_unstable();
            let mut slices = Vec::new();
            for deposit_id in candidates {
                if count == 0 {
                    break;
                }
                let Some(original) = self.deposits.get(&deposit_id) else {
                    continue;
                };
                let account = staged_accounts
                    .entry(deposit_id)
                    .or_insert_with(|| original.clone());
                let available = materials
                    .iter()
                    .map(|(material, units)| {
                        if *units == 0 {
                            u64::MAX
                        } else {
                            account
                                .remaining_unmaterialized
                                .get(material)
                                .copied()
                                .unwrap_or_default()
                                / units
                        }
                    })
                    .min()
                    .unwrap_or_default()
                    .min(u64::from(count)) as u32;
                if available == 0 {
                    continue;
                }
                let moved = scaled(&materials, u64::from(available));
                subtract_vector(&mut account.remaining_unmaterialized, &moved)?;
                add_vector(&mut account.materialized_underground, &moved, 1);
                slices.push(ReservationSlice {
                    deposit_id,
                    count: available,
                });
                count -= available;
            }
            if count != 0 {
                return Err(std::io::Error::other(format!(
                    "material manifest has no remaining {mineral:?} capacity for {count} {block_name} blocks in {chunk_pos:?}"
                )));
            }
            reservation.blocks.insert(block_name, slices);
        }
        for (id, account) in staged_accounts {
            self.deposits.insert(id, account);
        }
        self.chunk_reservations.insert(chunk_pos, reservation);
        self.reservations_dirty.set(true);
        Ok(true)
    }

    pub fn begin_operation(
        &mut self,
        kind: MaterialOperationKind,
        pos: BlockPos,
        before: String,
        after: String,
        materials: MaterialVector,
    ) -> std::io::Result<MaterialOperation> {
        // The voxel journal is allowed to outlive the process. Its referenced
        // reservation must therefore already exist in the durable snapshot.
        self.checkpoint_reservations()?;
        let operation = MaterialOperation {
            id: self.next_operation_id,
            kind,
            pos,
            before,
            after,
            materials,
        };
        self.next_operation_id = self.next_operation_id.saturating_add(1);
        let bytes = postcard::to_allocvec(&operation).map_err(std::io::Error::other)?;
        crate::identity::atomic_write(&self.journal_path(), &bytes, false)?;
        Ok(operation)
    }

    pub fn begin_break(
        &mut self,
        pos: BlockPos,
        block_name: &str,
        materials: &MaterialVector,
    ) -> std::io::Result<Option<MaterialOperation>> {
        if materials.is_empty() {
            return Ok(None);
        }
        let kind = if self.placed_positions.contains_key(&pos) {
            MaterialOperationKind::BreakPlaced
        } else {
            MaterialOperationKind::MineNatural
        };
        self.begin_operation(
            kind,
            pos,
            block_name.into(),
            "base:air".into(),
            materials.clone(),
        )
        .map(Some)
    }

    pub fn begin_place(
        &mut self,
        pos: BlockPos,
        before: &str,
        block_name: &str,
        materials: &MaterialVector,
    ) -> std::io::Result<Option<MaterialOperation>> {
        if materials.is_empty() {
            return Ok(None);
        }
        self.begin_operation(
            MaterialOperationKind::Place,
            pos,
            before.into(),
            block_name.into(),
            materials.clone(),
        )
        .map(Some)
    }

    pub fn begin_authored_place(
        &mut self,
        pos: BlockPos,
        before: &str,
        block_name: &str,
        materials: &MaterialVector,
        source: &str,
    ) -> std::io::Result<Option<MaterialOperation>> {
        if materials.is_empty() {
            return Ok(None);
        }
        self.begin_operation(
            MaterialOperationKind::AuthoredPlace,
            pos,
            format!(
                "{before}{}{source}",
                MaterialOperation::AUTHORED_SOURCE_SEPARATOR
            ),
            block_name.into(),
            materials.clone(),
        )
        .map(Some)
    }

    pub fn apply_operation(&mut self, operation: &MaterialOperation) -> std::io::Result<()> {
        if operation.id <= self.last_applied_operation {
            return Ok(());
        }
        self.commit_delta(MaterialDeltaAction::Operation(operation.clone()))
    }

    fn apply_operation_in_memory(&mut self, operation: &MaterialOperation) -> std::io::Result<()> {
        self.next_operation_id = self.next_operation_id.max(operation.id.saturating_add(1));
        if operation.id <= self.last_applied_operation {
            return Ok(());
        }
        match operation.kind {
            MaterialOperationKind::MineNatural => {
                self.extract_reserved(
                    operation.pos.chunk(),
                    &operation.before,
                    &operation.materials,
                )?;
                add_vector(&mut self.circulating, &operation.materials, 1);
            }
            MaterialOperationKind::BreakPlaced => {
                let stored = self
                    .placed_positions
                    .remove(&operation.pos)
                    .unwrap_or_else(|| operation.materials.clone());
                subtract_or_external(&mut self.placed, &stored, &mut self.external_additions)?;
                add_vector(&mut self.circulating, &stored, 1);
            }
            MaterialOperationKind::Place => {
                subtract_or_external(
                    &mut self.circulating,
                    &operation.materials,
                    &mut self.external_additions,
                )?;
                add_vector(&mut self.placed, &operation.materials, 1);
                self.placed_positions
                    .insert(operation.pos, operation.materials.clone());
            }
            MaterialOperationKind::AuthoredPlace => {
                add_vector(&mut self.external_additions, &operation.materials, 1);
                add_vector(
                    self.external_sources
                        .entry(operation.authored_source().into())
                        .or_default(),
                    &operation.materials,
                    1,
                );
                add_vector(&mut self.placed, &operation.materials, 1);
                self.placed_positions
                    .insert(operation.pos, operation.materials.clone());
            }
        }
        self.last_applied_operation = operation.id;
        Ok(())
    }

    pub fn finish_operation(&self) -> std::io::Result<()> {
        crate::persist::remove_if_exists(&self.journal_path())
    }

    pub fn pending_operation(&self) -> std::io::Result<Option<MaterialOperation>> {
        match std::fs::read(self.journal_path()) {
            Ok(bytes) => postcard::from_bytes(&bytes)
                .map(Some)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn journal_path(&self) -> PathBuf {
        self.path.with_extension("wfm.pending")
    }

    fn extract_reserved(
        &mut self,
        chunk: ChunkPos,
        block_name: &str,
        materials: &MaterialVector,
    ) -> std::io::Result<()> {
        let reservation = self
            .chunk_reservations
            .get_mut(&chunk)
            .ok_or_else(|| std::io::Error::other("mined finite block has no chunk reservation"))?;
        let slices = reservation.blocks.get_mut(block_name).ok_or_else(|| {
            std::io::Error::other("mined finite block is absent from reservation")
        })?;
        let slice = slices
            .iter_mut()
            .find(|slice| slice.count != 0)
            .ok_or_else(|| std::io::Error::other("finite block reservation is exhausted"))?;
        slice.count -= 1;
        let account = self
            .deposits
            .get_mut(&slice.deposit_id)
            .ok_or_else(|| std::io::Error::other("reservation names a missing deposit"))?;
        subtract_vector(&mut account.materialized_underground, materials)?;
        add_vector(&mut account.extracted, materials, 1);
        account.discovered = true;
        Ok(())
    }

    pub fn record_recipe_loss(&mut self, loss: &MaterialVector) -> std::io::Result<()> {
        if loss.is_empty() {
            return Ok(());
        }
        self.commit_delta(MaterialDeltaAction::RecipeLoss(loss.clone()))
    }

    pub fn record_recipe_loss_scaled(
        &mut self,
        loss: &MaterialVector,
        count: u32,
    ) -> std::io::Result<()> {
        self.record_recipe_loss(&scaled(loss, u64::from(count)))
    }

    pub fn record_consumption(&mut self, materials: &MaterialVector) -> std::io::Result<()> {
        if materials.is_empty() {
            return Ok(());
        }
        self.commit_delta(MaterialDeltaAction::Consumption(materials.clone()))
    }

    pub fn bury_stack(
        &mut self,
        reg: &Registry,
        pos: BlockPos,
        stack: ItemStack,
        reason: &str,
    ) -> std::io::Result<()> {
        let materials = scaled(&reg.item(stack.item).materials, u64::from(stack.count));
        if materials.is_empty() {
            return Ok(());
        }
        self.commit_delta(MaterialDeltaAction::Bury {
            region: SalvageRegion::at(pos),
            materials,
            from_secondary: is_secondary_item(reg, stack.item),
            reason: reason.into(),
        })
    }

    pub fn bury_materials(
        &mut self,
        pos: BlockPos,
        materials: &MaterialVector,
        reason: &str,
    ) -> std::io::Result<()> {
        if materials.is_empty() {
            return Ok(());
        }
        self.commit_delta(MaterialDeltaAction::Bury {
            region: SalvageRegion::at(pos),
            materials: materials.clone(),
            from_secondary: false,
            reason: reason.into(),
        })
    }

    #[allow(dead_code)] // public simulation hook; a separator UI consumes this next
    pub fn recover_salvage(
        &mut self,
        region: SalvageRegion,
        material: &str,
        requested: u64,
        recovery_permille: u16,
    ) -> std::io::Result<u64> {
        let Some(pool) = self.salvage.get(&region) else {
            return Ok(0);
        };
        let available = pool.materials.get(material).copied().unwrap_or_default();
        let processed = available.min(requested);
        let recovered = processed.saturating_mul(u64::from(recovery_permille.min(1000))) / 1000;
        if processed != 0 {
            self.commit_delta(MaterialDeltaAction::Recover {
                region,
                material: material.into(),
                processed,
                recovered,
            })?;
        }
        Ok(recovered)
    }

    /// Sift one recipe-usable item from a regional buried-material pool.
    ///
    /// Primitive recovery is deliberately lossy. We debit the smallest
    /// source quantity whose 75%-style yield can fund the exact material
    /// vector of a real registered item; the rest becomes explicit process
    /// loss. Sub-unit dust remains pooled until later losses make a complete
    /// batch possible, so the bounded regional ledger never needs one entity
    /// per fleck.
    pub fn recover_salvage_stack(
        &mut self,
        reg: &Registry,
        region: SalvageRegion,
        recovery_permille: u16,
    ) -> std::io::Result<Option<ItemStack>> {
        let recovery = u64::from(recovery_permille.clamp(1, 1000));
        let Some(pool) = self.salvage.get(&region) else {
            return Ok(None);
        };
        let choice = pool.materials.iter().find_map(|(material, available)| {
            let (item, units) = recovery_item_for(reg, material)?;
            let processed = units.saturating_mul(1000).div_ceil(recovery);
            (*available >= processed).then_some((material.clone(), item, units, processed))
        });
        let Some((material, item, output_units, processed)) = choice else {
            return Ok(None);
        };
        self.commit_delta(MaterialDeltaAction::Recover {
            region,
            material,
            processed,
            recovered: output_units,
        })?;
        Ok(Some(ItemStack::new(reg, item, 1)))
    }

    pub fn record_admin_deletion(&mut self, materials: &MaterialVector) -> std::io::Result<()> {
        if materials.is_empty() {
            return Ok(());
        }
        self.commit_delta(MaterialDeltaAction::AdminDeletion(materials.clone()))
    }

    pub fn record_secondary_output(&mut self, materials: &MaterialVector) -> std::io::Result<()> {
        if materials.is_empty() {
            return Ok(());
        }
        self.commit_delta(MaterialDeltaAction::SecondaryOutput(materials.clone()))
    }

    pub fn record_secondary_recovery(&mut self, materials: &MaterialVector) -> std::io::Result<()> {
        if materials.is_empty() {
            return Ok(());
        }
        self.commit_delta(MaterialDeltaAction::SecondaryRecovery(materials.clone()))
    }

    pub fn record_external_stack(
        &mut self,
        reg: &Registry,
        stack: ItemStack,
        source: &str,
    ) -> std::io::Result<()> {
        let materials = stack_materials(reg, stack);
        self.record_external_materials(&materials, is_secondary_item(reg, stack.item), source)
    }

    /// Register material introduced by an explicitly named non-planetary
    /// source. Development fixtures sometimes store fractional recovery stock
    /// directly rather than as an item stack, so this is the vector-level
    /// counterpart to [`Self::record_external_stack`].
    pub fn record_external_materials(
        &mut self,
        materials: &MaterialVector,
        secondary: bool,
        source: &str,
    ) -> std::io::Result<()> {
        if materials.is_empty() {
            return Ok(());
        }
        self.commit_delta(MaterialDeltaAction::External {
            materials: materials.clone(),
            secondary,
            source: source.into(),
        })
    }

    /// Remove an authored/debug stack without pretending it was transformed
    /// by ordinary play. Secondary stock moves through circulation first so
    /// the existing deletion delta keeps both physical compartments exact.
    pub fn record_admin_stack_deletion(
        &mut self,
        reg: &Registry,
        stack: ItemStack,
    ) -> std::io::Result<()> {
        let materials = stack_materials(reg, stack);
        if materials.is_empty() {
            return Ok(());
        }
        if is_secondary_item(reg, stack.item) {
            self.record_secondary_recovery(&materials)?;
        }
        self.record_admin_deletion(&materials)
    }

    pub fn record_admin_secondary_deletion(
        &mut self,
        materials: &MaterialVector,
    ) -> std::io::Result<()> {
        if materials.is_empty() {
            return Ok(());
        }
        self.record_secondary_recovery(materials)?;
        self.record_admin_deletion(materials)
    }

    /// Register finite material authored by world generation rather than
    /// drawn from a geological account. Ruin masonry is already in the
    /// placed compartment; chest and archaeology loot begins circulating.
    pub fn record_external_world_content(
        &mut self,
        reg: &Registry,
        stacks: &[ItemStack],
        placements: &[(BlockPos, MaterialVector)],
        source: &str,
    ) -> std::io::Result<()> {
        let mut changed = false;
        for &stack in stacks {
            let materials = stack_materials(reg, stack);
            if materials.is_empty() {
                continue;
            }
            add_vector(&mut self.external_additions, &materials, 1);
            add_vector(
                self.external_sources.entry(source.into()).or_default(),
                &materials,
                1,
            );
            if is_secondary_item(reg, stack.item) {
                add_vector(&mut self.secondary_physical, &materials, 1);
            } else {
                add_vector(&mut self.circulating, &materials, 1);
            }
            changed = true;
        }
        for (pos, materials) in placements {
            if materials.is_empty() {
                continue;
            }
            add_vector(&mut self.external_additions, materials, 1);
            add_vector(
                self.external_sources.entry(source.into()).or_default(),
                materials,
                1,
            );
            add_vector(&mut self.placed, materials, 1);
            self.placed_positions.insert(*pos, materials.clone());
            changed = true;
        }
        if changed { self.save() } else { Ok(()) }
    }

    pub fn audit(&self) -> MaterialAudit {
        let mut audit = MaterialAudit::default();
        for account in self.deposits.values() {
            add_vector(&mut audit.original, &account.original_recoverable, 1);
            add_vector(
                &mut audit.unmaterialized,
                &account.remaining_unmaterialized,
                1,
            );
            add_vector(&mut audit.underground, &account.materialized_underground, 1);
            add_vector(&mut audit.secondary, &account.tailings_secondary, 1);
        }
        add_vector(&mut audit.circulating, &self.circulating, 1);
        add_vector(&mut audit.placed, &self.placed, 1);
        add_vector(&mut audit.secondary, &self.secondary_physical, 1);
        for pool in self.salvage.values() {
            add_vector(&mut audit.secondary, &pool.materials, 1);
        }
        add_vector(&mut audit.consumption_loss, &self.explicit_consumption, 1);
        add_vector(&mut audit.consumption_loss, &self.explicit_loss, 1);
        audit.external_additions = self.external_additions.clone();
        audit.external_sources = self.external_sources.clone();
        let mut attributed_external = MaterialVector::new();
        for vector in self.external_sources.values() {
            add_vector(&mut attributed_external, vector, 1);
        }
        let mut implicit_external = self.external_additions.clone();
        for (material, attributed) in attributed_external {
            let available = implicit_external
                .get(&material)
                .copied()
                .unwrap_or_default();
            if attributed >= available {
                implicit_external.remove(&material);
            } else {
                implicit_external.insert(material, available - attributed);
            }
        }
        if !implicit_external.is_empty() {
            audit
                .external_sources
                .insert("legacy/admin reconciliation".into(), implicit_external);
        }

        let mut names = BTreeSet::new();
        for vector in [
            &audit.original,
            &audit.unmaterialized,
            &audit.underground,
            &audit.circulating,
            &audit.placed,
            &audit.secondary,
            &audit.consumption_loss,
            &audit.external_additions,
        ] {
            names.extend(vector.keys().cloned());
        }
        for name in names {
            let source = i128::from(*audit.original.get(&name).unwrap_or(&0))
                + i128::from(*audit.external_additions.get(&name).unwrap_or(&0));
            let accounted = [
                &audit.unmaterialized,
                &audit.underground,
                &audit.circulating,
                &audit.placed,
                &audit.secondary,
                &audit.consumption_loss,
            ]
            .iter()
            .map(|vector| i128::from(*vector.get(&name).unwrap_or(&0)))
            .sum::<i128>();
            audit.unexplained.insert(name, source - accounted);
        }
        audit.critical_site_counts = ["copper", "tin", "iron", "coal"]
            .into_iter()
            .map(|material| {
                (
                    material.to_string(),
                    self.deposits
                        .values()
                        .filter(|deposit| deposit.original_recoverable.contains_key(material))
                        .count(),
                )
            })
            .collect();
        audit.bronze_bootstrap_regions = audit
            .critical_site_counts
            .get("tin")
            .copied()
            .unwrap_or_default();
        let arc_cost = [
            ("copper", 24 * CANONICAL_INGOT_UNITS),
            ("tin", 8 * CANONICAL_INGOT_UNITS),
            ("iron", 64 * CANONICAL_INGOT_UNITS),
            ("coal", 32 * CANONICAL_INGOT_UNITS),
        ];
        audit.pessimistic_technology_arcs = arc_cost
            .into_iter()
            .map(|(material, cost)| {
                audit.original.get(material).copied().unwrap_or_default() * 750 / 1000 / cost
            })
            .min()
            .unwrap_or_default();
        audit.major_continents = self.qualification.major_continents;
        audit.major_continents_with_flux = self.qualification.major_continents_with_flux;
        audit.treasure_site_counts = self.qualification.treasure_site_counts.clone();
        audit.retrogen_status = self
            .retrogen
            .iter()
            .map(|(resource, record)| (resource.clone(), record.status.clone()))
            .collect();
        if audit.bronze_bootstrap_regions < 3 {
            audit.qualification_failures.push(format!(
                "only {} independent bronze bootstrap regions (need 3)",
                audit.bronze_bootstrap_regions
            ));
        }
        for (material, count) in &audit.critical_site_counts {
            if *count < 2 {
                audit
                    .qualification_failures
                    .push(format!("{material} has {count} independent sites (need 2)"));
            }
        }
        if audit.major_continents_with_flux != audit.major_continents {
            audit.qualification_failures.push(format!(
                "carbonate flux reaches {} of {} major continents",
                audit.major_continents_with_flux, audit.major_continents
            ));
        }
        for (category, count) in &audit.treasure_site_counts {
            if *count < 2 {
                audit.qualification_failures.push(format!(
                    "treasure category {category} has {count} sites (need 2)"
                ));
            }
        }
        if audit.pessimistic_technology_arcs < REQUIRED_TECHNOLOGY_ARCS {
            audit.qualification_failures.push(format!(
                "only {} pessimistic technology arcs (need {})",
                audit.pessimistic_technology_arcs, REQUIRED_TECHNOLOGY_ARCS
            ));
        }
        audit
    }

    #[cfg(test)]
    pub fn salvage_cardinality(&self) -> usize {
        self.salvage.values().map(|pool| pool.materials.len()).sum()
    }
}

/// Select a stable, ordinary item that puts recovered material directly back
/// into the existing recipe graph. Multi-material goods and generated salvage
/// intermediates are excluded: a regional batch recovers one constituent at
/// a time, while alloyed stock remains a forge concern.
fn delta_checksum(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

fn encode_delta_frame(record: &MaterialDelta) -> std::io::Result<Vec<u8>> {
    let payload = postcard::to_allocvec(record).map_err(std::io::Error::other)?;
    let length = u32::try_from(payload.len())
        .map_err(|_| std::io::Error::other("material delta record is too large"))?;
    let mut frame = Vec::with_capacity(4 + payload.len() + 8);
    frame.extend_from_slice(&length.to_le_bytes());
    frame.extend_from_slice(&payload);
    frame.extend_from_slice(&delta_checksum(&payload).to_le_bytes());
    Ok(frame)
}

/// Decode as much of the log as is certainly durable. `complete == false`
/// identifies a corrupt/truncated tail that can only be repaired from the
/// separately atomic pending record.
fn decode_delta_frames(bytes: &[u8]) -> (Vec<MaterialDelta>, usize, bool) {
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
        let payload_start = offset + 4;
        let payload_end = payload_start + length;
        let checksum = u64::from_le_bytes(bytes[payload_end..payload_end + 8].try_into().unwrap());
        if delta_checksum(&bytes[payload_start..payload_end]) != checksum {
            return (records, offset, false);
        }
        let Ok(record) = postcard::from_bytes::<MaterialDelta>(&bytes[payload_start..payload_end])
        else {
            return (records, offset, false);
        };
        records.push(record);
        offset = payload_end + 8;
    }
    (records, offset, true)
}

fn recovery_item_for(reg: &Registry, material: &str) -> Option<(ItemId, u64)> {
    reg.items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            if !item.materials_declared
                || item.materials.len() != 1
                || item.durability != 0
                || item.places.is_some()
                || item.name.contains('/')
            {
                return None;
            }
            let units = item.materials.get(material).copied()?;
            if units == 0 {
                return None;
            }
            let leaf = item.name.rsplit(':').next().unwrap_or(&item.name);
            let rank = if leaf.ends_with("_ingot") {
                0
            } else if leaf.ends_with("_powder") || leaf.ends_with("_concentrate") {
                1
            } else if leaf == material || leaf.ends_with(&format!("_{material}")) {
                2
            } else if leaf.starts_with("raw_") {
                3
            } else {
                4
            };
            Some((rank, units, item.name.as_str(), ItemId(index as u16)))
        })
        .min_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.cmp(right.2))
        })
        .map(|(_, units, _, item)| (item, units))
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MaterialAudit {
    pub original: MaterialVector,
    pub unmaterialized: MaterialVector,
    pub underground: MaterialVector,
    /// Inventories, containers, item entities, and entity cargo.
    pub circulating: MaterialVector,
    pub placed: MaterialVector,
    /// Scrap, slag, tailings, and regional salvage.
    pub secondary: MaterialVector,
    pub consumption_loss: MaterialVector,
    pub external_additions: MaterialVector,
    pub external_sources: BTreeMap<String, MaterialVector>,
    pub unexplained: BTreeMap<String, i128>,
    pub critical_site_counts: BTreeMap<String, usize>,
    pub bronze_bootstrap_regions: usize,
    pub pessimistic_technology_arcs: u64,
    pub major_continents: usize,
    pub major_continents_with_flux: usize,
    pub treasure_site_counts: BTreeMap<String, usize>,
    pub retrogen_status: BTreeMap<String, String>,
    pub qualification_failures: Vec<String>,
}

impl MaterialAudit {
    pub fn is_balanced(&self) -> bool {
        self.unexplained.values().all(|delta| *delta == 0)
    }

    pub fn is_qualified(&self) -> bool {
        self.qualification_failures.is_empty()
    }

    pub fn render(&self) -> String {
        let mut names = BTreeSet::new();
        names.extend(self.original.keys().cloned());
        names.extend(self.external_additions.keys().cloned());
        let mut out = String::from(
            "material              original  unmaterialized  underground  inventory/entity  placed  secondary  consumed/lost  external  unexplained\n",
        );
        for name in names {
            out.push_str(&format!(
                "{name:<20} {:>10} {:>15} {:>12} {:>17} {:>7} {:>10} {:>14} {:>9} {:>12}\n",
                self.original.get(&name).copied().unwrap_or_default(),
                self.unmaterialized.get(&name).copied().unwrap_or_default(),
                self.underground.get(&name).copied().unwrap_or_default(),
                self.circulating.get(&name).copied().unwrap_or_default(),
                self.placed.get(&name).copied().unwrap_or_default(),
                self.secondary.get(&name).copied().unwrap_or_default(),
                self.consumption_loss
                    .get(&name)
                    .copied()
                    .unwrap_or_default(),
                self.external_additions
                    .get(&name)
                    .copied()
                    .unwrap_or_default(),
                self.unexplained.get(&name).copied().unwrap_or_default(),
            ));
        }
        out.push_str(if self.is_balanced() {
            "status: BALANCED (all unexplained deltas are zero)\n"
        } else {
            "status: CORRUPTION GAP (restore from backup; mass was not invented)\n"
        });
        out.push_str(&format!(
            "qualification: bronze regions {}, pessimistic complete technology arcs {} (required {}), critical sites {:?}\n",
            self.bronze_bootstrap_regions,
            self.pessimistic_technology_arcs,
            REQUIRED_TECHNOLOGY_ARCS,
            self.critical_site_counts
        ));
        out.push_str(&format!(
            "qualification: carbonate flux on {}/{} major continents; treasure sites {:?}\n",
            self.major_continents_with_flux, self.major_continents, self.treasure_site_counts
        ));
        if self.qualification_failures.is_empty() {
            out.push_str("qualification status: PASS\n");
        } else {
            for failure in &self.qualification_failures {
                out.push_str(&format!("qualification failure: {failure}\n"));
            }
        }
        for (resource, status) in &self.retrogen_status {
            out.push_str(&format!("retrogen {resource}: {status}\n"));
        }
        for (source, materials) in &self.external_sources {
            out.push_str(&format!("external source {source}: {materials:?}\n"));
        }
        out
    }
}

pub fn audit_world(world: &Path) -> std::io::Result<MaterialAudit> {
    Ok(MaterialLedger::load(world)?.audit())
}

fn mineral_materials(mineral: MineralKind, reg: &Registry) -> MaterialVector {
    if let Some(materials) = reg.ores.iter().find_map(|ore| {
        (MineralKind::from_block_name(&reg.block(ore.block).name) == mineral)
            .then(|| reg.block(ore.block).materials.clone())
            .filter(|materials| !materials.is_empty())
    }) {
        return materials;
    }
    let (material, units) = match mineral {
        MineralKind::Copper => ("copper", 1_200),
        MineralKind::Tin => ("tin", 1_200),
        MineralKind::Iron => ("iron", 1_200),
        MineralKind::Cobalt => ("cobalt", 1_200),
        MineralKind::Cinnabar => ("mercury", 1_200),
        MineralKind::Manganese => ("manganese", 1_200),
        MineralKind::Coal => ("coal", 1_200),
        MineralKind::Gold => ("gold", 1_200),
        MineralKind::Galena => ("lead", 1_200),
        MineralKind::Chromite => ("chromium", 1_200),
        MineralKind::Diamond => ("diamond", 1_200),
        MineralKind::RareEarth => ("rare_earth", 1_200),
        MineralKind::Halite => ("salt", 2_400),
        MineralKind::Pitchblende => ("uranium", 1_200),
        MineralKind::Geode => ("quartz", 1_200),
        MineralKind::Other => ("unknown_mineral", 1_200),
    };
    BTreeMap::from([(material.into(), units)])
}

fn stable_resource_id(resource_key: &str, site_id: u32) -> u32 {
    let mut hash = 0x811c_9dc5u32 ^ site_id.rotate_left(13);
    for byte in resource_key.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash | 0x8000_0000
}

pub fn stack_materials(reg: &Registry, stack: ItemStack) -> MaterialVector {
    scaled(&reg.item(stack.item).materials, u64::from(stack.count))
}

pub fn is_secondary_item(reg: &Registry, item: crate::registry::ItemId) -> bool {
    let name = &reg.item(item).name;
    name == "base:iron_slag"
        || name.ends_with("/primitive_scale")
        || name.ends_with("/forge_scale")
        || name.ends_with("/dismantling_scale")
}

/// Accounted recovered stock produced by primitive salvage, forge salvage,
/// or clean machine dismantling. A hot forge can consolidate these fractional
/// vectors; scale/slag is intentionally excluded and keeps its later-tech
/// secondary recovery path.
pub fn is_reclaimable_stock(reg: &Registry, item: ItemId) -> bool {
    let name = &reg.item(item).name;
    name.ends_with("/primitive_scrap")
        || name.ends_with("/forge_scrap")
        || name.ends_with("/dismantled_stock")
}

/// Drain every complete, recipe-usable material unit from a forge's
/// fractional stock bank. Mixed vectors (notably bronze) are preferred before
/// their constituents, preserving an alloy where the bank can fund it. Any
/// remainder stays in the persisted bank for the next batch.
pub fn consolidate_reclaimed_stock(reg: &Registry, bank: &mut MaterialVector) -> Vec<ItemStack> {
    let mut candidates = reg
        .items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            if !item.materials_declared
                || item.materials.is_empty()
                || item.durability != 0
                || item.places.is_some()
                || item.name.contains('/')
                || is_secondary_item(reg, ItemId(index as u16))
            {
                return None;
            }
            let leaf = item.name.rsplit(':').next().unwrap_or(&item.name);
            let rank = if leaf.ends_with("_ingot") {
                0
            } else if leaf.ends_with("_powder") || leaf.ends_with("_concentrate") {
                1
            } else if leaf.starts_with("raw_") {
                3
            } else {
                2
            };
            Some((
                rank,
                std::cmp::Reverse(item.materials.len()),
                item.name.as_str(),
                ItemId(index as u16),
                item.materials.clone(),
                item.max_stack.max(1),
            ))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(right.2))
    });

    let mut outputs = Vec::new();
    for (_, _, _, item, materials, max_stack) in candidates {
        let count = materials
            .iter()
            .map(|(material, units)| {
                bank.get(material).copied().unwrap_or_default() / units.max(&1)
            })
            .min()
            .unwrap_or_default();
        if count == 0 {
            continue;
        }
        let debit = scaled(&materials, count);
        // The quotient above proved every constituent is available.
        subtract_vector(bank, &debit).expect("forge stock quotient prevents underflow");
        let mut remaining = count;
        while remaining != 0 {
            let take = remaining.min(u64::from(max_stack)) as u32;
            outputs.push(ItemStack::new(reg, item, take));
            remaining -= u64::from(take);
        }
    }
    outputs
}

fn scaled(vector: &MaterialVector, multiplier: u64) -> MaterialVector {
    let mut out = MaterialVector::new();
    add_vector(&mut out, vector, multiplier);
    out
}

fn add_vector(into: &mut MaterialVector, from: &MaterialVector, multiplier: u64) {
    for (material, units) in from {
        let add = units.saturating_mul(multiplier);
        into.entry(material.clone())
            .and_modify(|n| *n = n.saturating_add(add))
            .or_insert(add);
    }
    into.retain(|_, units| *units != 0);
}

fn subtract_one(vector: &mut MaterialVector, material: &str, units: u64) -> std::io::Result<()> {
    let available = vector.get(material).copied().unwrap_or_default();
    if available < units {
        return Err(std::io::Error::other(format!(
            "material underflow for {material}: have {available}, need {units}"
        )));
    }
    if available == units {
        vector.remove(material);
    } else {
        vector.insert(material.into(), available - units);
    }
    Ok(())
}

fn subtract_vector(vector: &mut MaterialVector, sub: &MaterialVector) -> std::io::Result<()> {
    for (material, units) in sub {
        subtract_one(vector, material, *units)?;
    }
    Ok(())
}

/// Survival content can enter through ruins, trade, authored events, or
/// admin tools. If a sink/placement sees more circulating material than was
/// mined, name that difference as an external addition instead of creating an
/// unexplained deficit.
fn subtract_or_external(
    vector: &mut MaterialVector,
    sub: &MaterialVector,
    external: &mut MaterialVector,
) -> std::io::Result<()> {
    for (material, units) in sub {
        let available = vector.get(material).copied().unwrap_or_default();
        if available < *units {
            let addition = *units - available;
            *external.entry(material.clone()).or_default() += addition;
            *vector.entry(material.clone()).or_default() += addition;
        }
    }
    subtract_vector(vector, sub)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_authored_source_operations_keep_postcard_compatibility() {
        #[derive(Serialize)]
        struct LegacyOperation {
            id: u64,
            kind: MaterialOperationKind,
            pos: BlockPos,
            before: String,
            after: String,
            materials: MaterialVector,
        }

        let legacy = LegacyOperation {
            id: 7,
            kind: MaterialOperationKind::Place,
            pos: BlockPos::new(Face::PosZ, 10, 64, 10).unwrap(),
            before: "base:air".into(),
            after: "base:iron_block".into(),
            materials: MaterialVector::from([("iron".into(), 10_800)]),
        };
        let bytes = postcard::to_allocvec(&legacy).unwrap();
        let decoded: MaterialOperation = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.id, legacy.id);
        assert_eq!(decoded.kind, legacy.kind);
        assert_eq!(decoded.materials, legacy.materials);
        assert_eq!(decoded.before_block_name(), legacy.before);
    }

    #[test]
    fn base_content_material_graph_balances() {
        let reg = crate::registry::load(Path::new("__no_material_test_mods__"));
        assert!(
            reg.material_errors.is_empty(),
            "material graph errors:\n{}",
            reg.material_errors.join("\n")
        );
        assert!(reg.items.iter().all(|item| matches!(
            item.material_class,
            crate::registry::MaterialClass::Renewable
                | crate::registry::MaterialClass::GeologicallyFinite
                | crate::registry::MaterialClass::TransformativeFinite
                | crate::registry::MaterialClass::Consumptive
                | crate::registry::MaterialClass::Exceptional
        )));
        assert!(
            reg.items
                .iter()
                .filter(|item| {
                    item.durability > 0 && item.food.is_none() && !item.materials.is_empty()
                })
                .all(|item| item.broken_into.is_some()),
            "every material-bearing tool and armor piece retains a damaged object"
        );
        let missing_machine_salvage = reg
            .blocks
            .iter()
            .filter(|block| {
                !block.materials.is_empty()
                    && block.interaction.as_deref().is_some_and(|interaction| {
                        matches!(
                            interaction,
                            "furnace"
                                | "bloomery"
                                | "kiln"
                                | "forge"
                                | "anvil"
                                | "quern"
                                | "millstone"
                                | "sawmill"
                                | "lathe"
                                | "iron_lathe"
                                | "boring"
                                | "pump"
                                | "generator"
                                | "separator"
                                | "firebox"
                        )
                    })
            })
            .filter(|block| block.dismantles_to.is_none())
            .map(|block| block.name.clone())
            .collect::<Vec<_>>();
        assert!(
            missing_machine_salvage.is_empty(),
            "material-bearing machines without clean dismantling: {missing_machine_salvage:?}"
        );
    }

    #[test]
    fn regional_salvage_is_bounded_by_region_and_material() {
        let atlas = PlanetAtlas::fixture(8, 8).unwrap();
        let reg = crate::registry::load(Path::new("__no_material_test_mods__"));
        let root = std::env::temp_dir().join(format!(
            "wildforge-material-ledger-{}-{}",
            std::process::id(),
            8
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut ledger = MaterialLedger::initialize(root.join(LEDGER_FILE), &atlas, &reg);
        let copper = reg.item_id("base:copper_ingot").unwrap();
        let pos = BlockPos::new(Face::PosZ, 1, 64, 1).unwrap();
        for _ in 0..10_000 {
            ledger
                .bury_stack(&reg, pos, ItemStack::new(&reg, copper, 1), "despawn")
                .unwrap();
        }
        assert_eq!(ledger.salvage_cardinality(), 1);
        let region = SalvageRegion::at(pos);
        let recovered = ledger
            .recover_salvage(region, "copper", 1_200, 750)
            .unwrap();
        assert_eq!(recovered, 900);
        assert!(ledger.audit().is_balanced());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn primitive_sifting_returns_recipe_usable_stock_at_exact_seventy_five_percent() {
        let atlas = PlanetAtlas::fixture(12, 8).unwrap();
        let reg = crate::registry::load(Path::new("__no_material_test_mods__"));
        let root =
            std::env::temp_dir().join(format!("wildforge-material-sifting-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut ledger = MaterialLedger::initialize(root.join(LEDGER_FILE), &atlas, &reg);
        let copper = reg.item_id("base:copper_ingot").unwrap();
        let pos = BlockPos::new(Face::PosZ, 300, 64, 300).unwrap();
        let four = ItemStack::new(&reg, copper, 4);
        ledger
            .record_external_stack(&reg, four, "sifting fixture")
            .unwrap();
        ledger.bury_stack(&reg, pos, four, "despawn").unwrap();
        let region = SalvageRegion::at(pos);
        for _ in 0..3 {
            let recovered = ledger
                .recover_salvage_stack(&reg, region, 750)
                .unwrap()
                .expect("four lost ingots fund three recovered ingots");
            assert_eq!(recovered.item, copper);
            assert_eq!(recovered.count, 1);
        }
        assert!(
            ledger
                .recover_salvage_stack(&reg, region, 750)
                .unwrap()
                .is_none(),
            "the unrecovered quarter cannot mint another item"
        );
        let audit = ledger.audit();
        assert_eq!(audit.circulating.get("copper"), Some(&3_600));
        assert_eq!(audit.consumption_loss.get("copper"), Some(&1_200));
        assert!(audit.is_balanced());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn ordinary_accounting_appends_bounded_deltas_and_compacts_on_save() {
        let atlas = PlanetAtlas::fixture(14, 8).unwrap();
        let reg = crate::registry::load(Path::new("__no_material_test_mods__"));
        let root = std::env::temp_dir().join(format!(
            "wildforge-material-delta-budget-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut ledger = MaterialLedger::initialize(root.join(LEDGER_FILE), &atlas, &reg);
        // Inflate snapshot-only positional state. A hot-path operation must
        // not serialize any of it.
        for index in 0..20_000u16 {
            let pos = BlockPos::new(
                Face::PosZ,
                index % crate::planet::FACE_BLOCKS,
                64,
                index / crate::planet::FACE_BLOCKS,
            )
            .unwrap();
            ledger.placed_positions.insert(pos, MaterialVector::new());
        }
        ledger.save().unwrap();
        let snapshot_before = std::fs::read(root.join(LEDGER_FILE)).unwrap();
        assert!(
            snapshot_before.len() > 100_000,
            "fixture has a large snapshot"
        );

        let coal = MaterialVector::from([("coal".into(), CANONICAL_INGOT_UNITS)]);
        ledger.record_consumption(&coal).unwrap();
        assert_eq!(
            std::fs::read(root.join(LEDGER_FILE)).unwrap(),
            snapshot_before,
            "ordinary consumption does not rewrite the full snapshot"
        );
        let first_log = std::fs::metadata(root.join(DELTA_FILE)).unwrap().len();
        assert!(first_log < 256, "one bounded delta is small: {first_log}");
        ledger.record_consumption(&coal).unwrap();
        let second_log = std::fs::metadata(root.join(DELTA_FILE)).unwrap().len();
        assert!(
            second_log - first_log < 256,
            "the next action has constant bounded growth"
        );
        let audit = ledger.audit();
        assert!(audit.is_balanced());
        drop(ledger);
        let reloaded = MaterialLedger::load(&root).unwrap();
        assert_eq!(reloaded.audit(), audit, "append deltas replay exactly");
        reloaded.save().unwrap();
        assert_eq!(std::fs::read(root.join(DELTA_FILE)).unwrap(), DELTA_MAGIC);
        assert_eq!(MaterialLedger::load(&root).unwrap().audit(), audit);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn streamed_chunk_reservations_batch_until_a_durable_material_action() {
        let atlas = PlanetAtlas::fixture(141, 8).unwrap();
        let reg = crate::registry::load(Path::new("__no_material_batch_test_mods__"));
        let root = std::env::temp_dir().join(format!(
            "wildforge-material-reservation-batch-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut ledger = MaterialLedger::initialize(root.join(LEDGER_FILE), &atlas, &reg);
        ledger.save().unwrap();
        let snapshot_before = std::fs::read(root.join(LEDGER_FILE)).unwrap();
        let empty = Chunk::new();

        for u in 100..108 {
            let pos = ChunkPos::new(Face::PosZ, u, 100).unwrap();
            assert!(
                ledger
                    .reserve_fresh_chunk(&atlas, &reg, pos, &empty)
                    .unwrap()
            );
        }
        assert_eq!(ledger.chunk_reservations.len(), 8);
        assert!(ledger.reservations_dirty.get());
        assert_eq!(
            std::fs::read(root.join(LEDGER_FILE)).unwrap(),
            snapshot_before,
            "stream arrival must not rewrite the full ledger once per chunk"
        );

        // Beginning a durable voxel operation checkpoints every pending
        // reservation before its operation journal can reference one.
        let block = reg.block(reg.block_id("base:copper_block").unwrap());
        let operation = ledger
            .begin_place(
                BlockPos::new(Face::PosZ, 1, 64, 1).unwrap(),
                "base:air",
                &block.name,
                &block.materials,
            )
            .unwrap();
        assert!(operation.is_some());
        assert!(!ledger.reservations_dirty.get());
        let reloaded = MaterialLedger::load(&root).unwrap();
        assert_eq!(reloaded.chunk_reservations.len(), 8);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn atomic_pending_delta_repairs_only_a_truncated_log_tail() {
        let atlas = PlanetAtlas::fixture(15, 8).unwrap();
        let reg = crate::registry::load(Path::new("__no_material_test_mods__"));
        let root = std::env::temp_dir().join(format!(
            "wildforge-material-delta-crash-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let ledger = MaterialLedger::initialize(root.join(LEDGER_FILE), &atlas, &reg);
        ledger.save().unwrap();
        let record = MaterialDelta {
            seq: 1,
            action: MaterialDeltaAction::Consumption(MaterialVector::from([(
                "coal".into(),
                CANONICAL_INGOT_UNITS,
            )])),
        };
        let payload = postcard::to_allocvec(&record).unwrap();
        crate::identity::atomic_write(&root.join(DELTA_PENDING), &payload, false).unwrap();
        let frame = encode_delta_frame(&record).unwrap();
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(root.join(DELTA_FILE))
            .unwrap();
        file.write_all(&frame[..frame.len() / 2]).unwrap();
        file.sync_all().unwrap();
        drop(file);

        let repaired = MaterialLedger::load(&root).unwrap();
        assert_eq!(repaired.last_delta_seq, 1);
        assert_eq!(
            repaired.audit().consumption_loss.get("coal"),
            Some(&CANONICAL_INGOT_UNITS)
        );
        assert!(repaired.audit().is_balanced());
        assert!(!root.join(DELTA_PENDING).exists());

        // With no separately atomic record, damage is reported rather than
        // guessed away or balanced by invented material.
        let mut corrupt = std::fs::OpenOptions::new()
            .append(true)
            .open(root.join(DELTA_FILE))
            .unwrap();
        corrupt.write_all(&[7, 0, 0]).unwrap();
        corrupt.sync_all().unwrap();
        drop(corrupt);
        assert!(MaterialLedger::load(&root).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn placed_block_journal_replays_exactly_once() {
        let atlas = PlanetAtlas::fixture(31, 8).unwrap();
        let reg = crate::registry::load(Path::new("__no_material_test_mods__"));
        let root =
            std::env::temp_dir().join(format!("wildforge-material-journal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut ledger = MaterialLedger::initialize(root.join(LEDGER_FILE), &atlas, &reg);
        ledger.save().unwrap();
        let block = reg.block(reg.block_id("base:copper_block").unwrap());
        let pos = BlockPos::new(Face::PosZ, 4, 65, 4).unwrap();
        let place = ledger
            .begin_place(pos, "base:air", &block.name, &block.materials)
            .unwrap()
            .unwrap();
        ledger.apply_operation(&place).unwrap();
        ledger.apply_operation(&place).unwrap();
        assert_eq!(ledger.placed, block.materials);
        ledger.finish_operation().unwrap();
        drop(ledger);
        let mut ledger = MaterialLedger::load(&root).unwrap();
        assert_eq!(ledger.next_operation_id, 2);
        let broken = ledger
            .begin_break(pos, &block.name, &block.materials)
            .unwrap()
            .unwrap();
        ledger.apply_operation(&broken).unwrap();
        ledger.apply_operation(&broken).unwrap();
        assert!(ledger.placed.is_empty());
        assert_eq!(ledger.circulating, block.materials);
        assert!(ledger.audit().is_balanced());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn metal_breakage_and_recovery_keep_all_constituents() {
        let reg = crate::registry::load(Path::new("__no_material_test_mods__"));
        let pick = reg.item_id("base:bronze_pickaxe").unwrap();
        let damaged = reg
            .item(pick)
            .broken_into
            .expect("finite tool damage object");
        assert_eq!(reg.item(damaged).materials, reg.item(pick).materials);
        let primitive = reg
            .recipes
            .iter()
            .find(|recipe| {
                matches!(recipe.pattern.as_slice(), [Some(crate::registry::Ingredient::One(item))] if *item == damaged)
            })
            .unwrap();
        let mut accounted = reg.item(primitive.output).materials.clone();
        for (item, count) in &primitive.byproducts {
            add_vector(
                &mut accounted,
                &reg.item(*item).materials,
                u64::from(*count),
            );
        }
        assert_eq!(accounted, reg.item(pick).materials);
        let recovered = reg.item(primitive.output).materials.values().sum::<u64>();
        let original = reg.item(pick).materials.values().sum::<u64>();
        assert_eq!(recovered * 1000 / original, 750);
        let forge = reg
            .forge_salvage
            .iter()
            .find(|recipe| recipe.input == damaged)
            .unwrap();
        assert_eq!(forge.recovery_permille, 900);
    }

    #[test]
    fn mod_resources_get_finite_policy_manifests() {
        let atlas = PlanetAtlas::fixture(55, 8).unwrap();
        let reg = crate::registry::load(Path::new("mods"));
        let ledger = MaterialLedger::initialize(PathBuf::from("unused.wfm"), &atlas, &reg);
        let ruby = ledger.retrogen.get("gems:ruby_ore").unwrap();
        assert_eq!(ruby.policy, RetrogenPolicyRecord::UntouchedHostOnly);
        assert!(!ruby.added_mass.is_empty());
        assert!(
            ledger
                .deposits
                .values()
                .any(|deposit| deposit.resource_key == "gems:ruby_ore")
        );
    }

    #[test]
    fn removed_mod_content_keeps_named_mass_bearing_placeholders() {
        let atlas = PlanetAtlas::fixture(57, 8).unwrap();
        let with_gems = crate::registry::load(Path::new("mods"));
        let root = std::env::temp_dir().join(format!(
            "wildforge-material-placeholder-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("palette"), "0 base:air\n1 gems:ruby_ore\n").unwrap();
        let ledger = MaterialLedger::initialize(root.join(LEDGER_FILE), &atlas, &with_gems);
        let ruby_materials = with_gems
            .item(with_gems.item_id("gems:ruby").unwrap())
            .materials
            .clone();

        let mut without_gems = crate::registry::load(Path::new("__no_material_test_mods__"));
        let added = without_gems
            .install_saved_placeholders(&root, &ledger)
            .unwrap();
        assert!(added >= 2, "ore block and held item both get placeholders");
        let item = without_gems.item(without_gems.item_id("gems:ruby").unwrap());
        assert_eq!(item.materials, ruby_materials);
        assert!(item.label.contains("Missing content"));
        let block = without_gems.block(without_gems.block_id("gems:ruby_ore").unwrap());
        assert_eq!(block.materials, ruby_materials);
        ledger.validate_saved_definitions(&without_gems).unwrap();

        let reinstalled = crate::registry::load(Path::new("mods"));
        ledger.validate_saved_definitions(&reinstalled).unwrap();
        assert_eq!(
            reinstalled
                .item(reinstalled.item_id("gems:ruby").unwrap())
                .materials,
            ruby_materials
        );
        assert_eq!(
            reinstalled
                .block(reinstalled.block_id("gems:ruby_ore").unwrap())
                .materials,
            ruby_materials
        );
        let mut changed = crate::registry::load(Path::new("mods"));
        let ruby_item = changed.item_id("gems:ruby").unwrap();
        changed.items[ruby_item.0 as usize]
            .materials
            .insert("ruby".into(), 2_400);
        assert!(ledger.validate_saved_definitions(&changed).is_err());
        let mut changed = crate::registry::load(Path::new("mods"));
        let ruby_block = changed.block_id("gems:ruby_ore").unwrap();
        changed.blocks[ruby_block.0 as usize]
            .materials
            .insert("ruby".into(), 2_400);
        assert!(ledger.validate_saved_definitions(&changed).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn every_post_creation_retrogen_policy_is_finite_and_reported() {
        use crate::registry::RetrogenPolicy;

        let atlas = PlanetAtlas::fixture(19, 8).unwrap();
        let base = crate::registry::load(Path::new("__no_material_test_mods__"));
        let policies = [
            (
                RetrogenPolicy::UntouchedHostOnly,
                true,
                "untouched host only",
            ),
            (
                RetrogenPolicy::SecondaryRecovery,
                false,
                "secondary recovery",
            ),
            (RetrogenPolicy::WorldEvent, false, "world event"),
            (RetrogenPolicy::NoRetrogen, false, "no retrogen"),
        ];
        for (index, (policy, adds_geology, label)) in policies.into_iter().enumerate() {
            let root = std::env::temp_dir().join(format!(
                "wildforge-retrogen-policy-{}-{index}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            let mut ledger = MaterialLedger::initialize(root.join(LEDGER_FILE), &atlas, &base);
            let base_deposits = ledger.deposits.len();
            let mut content = crate::registry::load(Path::new("mods"));
            for ore in content.ores.iter_mut().filter(|ore| ore.mod_id == "gems") {
                ore.retrogen = policy;
            }
            ledger.content_hash = content.content_hash ^ u64::MAX;
            assert!(ledger.reconcile_mod_manifests(&atlas, &content));
            let record = ledger.retrogen.get("gems:ruby_ore").unwrap();
            let expected = match policy {
                RetrogenPolicy::UntouchedHostOnly => RetrogenPolicyRecord::UntouchedHostOnly,
                RetrogenPolicy::SecondaryRecovery => RetrogenPolicyRecord::SecondaryRecovery,
                RetrogenPolicy::WorldEvent => RetrogenPolicyRecord::WorldEvent,
                RetrogenPolicy::NoRetrogen => RetrogenPolicyRecord::NoRetrogen,
            };
            assert_eq!(record.policy, expected);
            assert_eq!(ledger.deposits.len() > base_deposits, adds_geology);
            assert_eq!(!record.added_mass.is_empty(), adds_geology);
            let notices = ledger.retrogen_notices();
            assert_eq!(notices.len(), 1);
            assert!(notices[0].contains(label));
            assert!(!notices[0].contains("face="));
            assert!(ledger.audit().is_balanced());
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[test]
    fn qualification_proves_flux_treasures_redundancy_and_arc_capacity() {
        let atlas = PlanetAtlas::fixture(91, 64).unwrap();
        let reg = crate::registry::load(Path::new("__no_material_test_mods__"));
        let ledger = MaterialLedger::initialize(PathBuf::from("unused.wfm"), &atlas, &reg);
        let audit = ledger.audit();
        assert!(
            audit.is_qualified(),
            "qualification failures: {:?}",
            audit.qualification_failures
        );
        assert_eq!(audit.major_continents_with_flux, audit.major_continents);
        assert!(audit.treasure_site_counts.values().all(|count| *count >= 2));
        assert!(audit.pessimistic_technology_arcs >= REQUIRED_TECHNOLOGY_ARCS);
    }

    #[test]
    fn secondary_scale_cannot_bypass_the_recovery_yield_through_repair() {
        let reg = crate::registry::load(Path::new("__no_material_test_mods__"));
        let tool = reg.item_id("base:bronze_pickaxe").unwrap();
        let damaged = reg.item(tool).broken_into.unwrap();
        let primitive = reg
            .recipes
            .iter()
            .find(|recipe| {
                matches!(recipe.pattern.as_slice(), [Some(crate::registry::Ingredient::One(item))] if *item == damaged)
            })
            .unwrap();
        let scale = primitive.byproducts[0].0;
        let worn = ItemStack {
            item: tool,
            count: 1,
            durability: 1,
        };
        assert!(
            crate::crafting::match_repair(
                &reg,
                &[Some(worn), Some(ItemStack::new(&reg, scale, 1))]
            )
            .is_none(),
            "secondary scale needs a later recovery process"
        );
        assert!(
            crate::crafting::match_repair(
                &reg,
                &[Some(worn), Some(ItemStack::new(&reg, primitive.output, 1))]
            )
            .is_some(),
            "recovered stock remains useful for repair"
        );
    }

    #[test]
    fn repair_consumes_a_real_part_and_preserves_the_objects_material() {
        let atlas = PlanetAtlas::fixture(93, 8).unwrap();
        let reg = crate::registry::load(Path::new("__no_material_test_mods__"));
        let root =
            std::env::temp_dir().join(format!("wildforge-material-repair-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut ledger = MaterialLedger::initialize(root.join(LEDGER_FILE), &atlas, &reg);
        ledger.save().unwrap();
        let tool = reg.item_id("base:bronze_pickaxe").unwrap();
        let part = reg.item_id("base:bronze_ingot").unwrap();
        let worn = ItemStack {
            item: tool,
            count: 1,
            durability: 1,
        };
        let part_stack = ItemStack::new(&reg, part, 1);
        ledger
            .record_external_stack(&reg, worn, "repair fixture tool")
            .unwrap();
        ledger
            .record_external_stack(&reg, part_stack, "repair fixture part")
            .unwrap();
        let repair = crate::crafting::match_repair(&reg, &[Some(worn), Some(part_stack)])
            .expect("bronze ingot repairs a worn bronze tool");
        assert!(repair.output.durability > worn.durability);
        assert_eq!(repair.output.item, tool);
        ledger.record_recipe_loss(&repair.scale_loss).unwrap();
        let audit = ledger.audit();
        assert_eq!(audit.circulating, reg.item(tool).materials);
        assert_eq!(audit.consumption_loss, reg.item(part).materials);
        assert!(audit.is_balanced());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn mining_moves_exact_reserved_mass_without_loading_an_audit_census() {
        let root =
            std::env::temp_dir().join(format!("wildforge-material-mining-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        crate::world::create_world_fixture_atomic(
            &root,
            91,
            "survival",
            8,
            &crate::planet_atlas::CancellationToken::default(),
            |_| {},
        )
        .unwrap();
        let reg = std::sync::Arc::new(crate::registry::load(Path::new(
            "__no_material_test_mods__",
        )));
        let mut world = crate::world::World::load_or_create(root.clone(), reg.clone()).unwrap();
        let atlas = world.planet_atlas().unwrap();
        let mut ore_pos = None;
        for site in atlas
            .geology
            .deposits
            .iter()
            .filter(|site| site.mineral == MineralKind::Copper)
        {
            let surface = crate::planet::SurfacePos::new(
                site.pos.face,
                site.pos.u * atlas.cell_blocks() + atlas.cell_blocks() / 2,
                site.pos.v * atlas.cell_blocks() + atlas.cell_blocks() / 2,
            )
            .unwrap();
            let chunk = ChunkPos::from_surface(surface);
            world.ensure_chunk(chunk);
            'scan: for y in 1..CHUNK_Y {
                for z in 0..CHUNK_Z {
                    for x in 0..CHUNK_X {
                        let pos = BlockPos::new(
                            chunk.face(),
                            chunk.u() * CHUNK_X as u16 + x as u16,
                            y as u8,
                            chunk.v() * CHUNK_Z as u16 + z as u16,
                        )
                        .unwrap();
                        if reg.block(world.get_block_at(pos)).name == "base:copper_ore" {
                            ore_pos = Some(pos);
                            break 'scan;
                        }
                    }
                }
            }
            if ore_pos.is_some() {
                break;
            }
        }
        let ore_pos = ore_pos.expect("fixture manifests at least one copper voxel");
        let before = world.material_ledger.as_ref().unwrap().audit();
        let pick = reg.item_id("base:bronze_pickaxe").unwrap();
        let broken = world
            .break_block_at(ore_pos, Some(pick), true, false)
            .expect("manifest ore breaks");
        assert_eq!(
            broken.drop.unwrap().item,
            reg.item_id("base:raw_copper").unwrap()
        );
        let after = world.material_ledger.as_ref().unwrap().audit();
        assert_eq!(
            after.circulating.get("copper").copied().unwrap_or_default(),
            before
                .circulating
                .get("copper")
                .copied()
                .unwrap_or_default()
                + CANONICAL_INGOT_UNITS
        );
        assert_eq!(
            after.underground.get("copper").copied().unwrap_or_default() + CANONICAL_INGOT_UNITS,
            before
                .underground
                .get("copper")
                .copied()
                .unwrap_or_default()
        );
        assert!(after.is_balanced());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn real_item_despawn_and_lava_retirement_enter_regional_salvage() {
        let (root, reg, mut world) = accounted_world("item-retirement", 97);
        let copper = reg.item_id("base:copper_ingot").unwrap();
        world
            .material_ledger
            .as_mut()
            .unwrap()
            .record_external_stack(
                &reg,
                ItemStack::new(&reg, copper, 2),
                "item retirement fixture",
            )
            .unwrap();
        let lava_block = BlockPos::of_world(10, 121, 10).unwrap();
        let pos = lava_block.entity_center();
        let mut despawn = crate::entity::ItemEntity::new(pos, glam::Vec3::ZERO, copper, 1);
        despawn.age = 299.5;
        assert!(!despawn.update(&world, 1.0));
        let reason = despawn.loss_reason(&world);
        assert_eq!(reason, "dropped-item despawn");
        world
            .material_ledger
            .as_mut()
            .unwrap()
            .bury_stack(
                &reg,
                despawn.pos.block().unwrap(),
                ItemStack::new(&reg, copper, 1),
                reason,
            )
            .unwrap();

        let lava_source = reg.lava_for_volume(8);
        world.set_block_state_at(lava_block, lava_source, 0, 0, 0);
        assert!(reg.is_lava(world.get_block_at(lava_block)));
        let mut lava = crate::entity::ItemEntity::new(pos, glam::Vec3::ZERO, copper, 1);
        assert!(!lava.update(&world, 0.1));
        let reason = lava.loss_reason(&world);
        assert_eq!(reason, "lava oxidation/dispersal");
        world
            .material_ledger
            .as_mut()
            .unwrap()
            .bury_stack(
                &reg,
                lava.pos.block().unwrap(),
                ItemStack::new(&reg, copper, 1),
                reason,
            )
            .unwrap();
        let audit = world.material_ledger.as_ref().unwrap().audit();
        assert_eq!(audit.secondary.get("copper"), Some(&2_400));
        assert_eq!(audit.circulating.get("copper").copied().unwrap_or(0), 0);
        assert!(audit.is_balanced());
        let _ = std::fs::remove_dir_all(root);
    }

    fn accounted_world(
        name: &str,
        seed: u32,
    ) -> (PathBuf, std::sync::Arc<Registry>, crate::world::World) {
        let root =
            std::env::temp_dir().join(format!("wildforge-material-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        crate::world::create_world_fixture_atomic(
            &root,
            seed,
            "survival",
            8,
            &crate::planet_atlas::CancellationToken::default(),
            |_| {},
        )
        .unwrap();
        let reg = std::sync::Arc::new(crate::registry::load(Path::new(
            "__no_material_test_mods__",
        )));
        let mut world = crate::world::World::load_or_create(root.clone(), reg.clone()).unwrap();
        let machine = BlockPos::of_world(10, 120, 10).unwrap();
        world.ensure_chunk(machine.chunk());
        (root, reg, world)
    }

    #[test]
    fn authored_machine_buffers_are_external_and_replacement_is_an_explicit_sink() {
        use crate::world::multiblock::MachineKind;
        use crate::world::{BlockEntity, MachineInstance, SignState};

        let (root, reg, mut world) = accounted_world("authored-machine-buffers", 127);
        let forge_pos = BlockPos::of_world(10, 120, 10).unwrap();
        let iron = reg.item_id("base:iron_ingot").unwrap();
        let mut forge = MachineInstance {
            kind: MachineKind::Forge,
            ..Default::default()
        };
        forge.charge[0] = Some(ItemStack::new(&reg, iron, 2));
        forge.reclaim.insert("iron".into(), 300);
        world.insert_block_entity_authored_at(
            forge_pos,
            BlockEntity::Multiblock(forge),
            "development fixture",
        );

        let first = world.material_ledger.as_ref().unwrap().audit();
        assert_eq!(first.circulating.get("iron"), Some(&2_400));
        assert_eq!(first.secondary.get("iron"), Some(&300));
        assert_eq!(first.external_additions.get("iron"), Some(&2_700));
        assert!(first.is_balanced());

        world.insert_block_entity_authored_at(
            forge_pos,
            BlockEntity::Sign(SignState::default()),
            "development fixture",
        );
        let replaced = world.material_ledger.as_ref().unwrap().audit();
        assert_eq!(replaced.circulating.get("iron").copied().unwrap_or(0), 0);
        assert_eq!(replaced.secondary.get("iron").copied().unwrap_or(0), 0);
        assert_eq!(replaced.consumption_loss.get("iron"), Some(&2_700));
        assert!(replaced.is_balanced());

        let separator_pos = BlockPos::of_world(11, 120, 10).unwrap();
        world.insert_block_entity_authored_at(
            separator_pos,
            BlockEntity::Multiblock(MachineInstance {
                kind: MachineKind::Separator,
                powder: 2,
                separator_fuel: 3,
                neodymium: 1,
                cerium: 2,
                ..Default::default()
            }),
            "development fixture",
        );
        let separator = world.material_ledger.as_ref().unwrap().audit();
        assert_eq!(separator.circulating.get("rare_earth"), Some(&2_000));
        assert_eq!(separator.external_additions.get("rare_earth"), Some(&2_000));
        assert!(separator.is_balanced());

        world.insert_block_entity_authored_at(
            separator_pos,
            BlockEntity::Sign(SignState::default()),
            "development fixture",
        );
        let removed = world.material_ledger.as_ref().unwrap().audit();
        assert_eq!(
            removed.circulating.get("rare_earth").copied().unwrap_or(0),
            0
        );
        assert_eq!(removed.consumption_loss.get("rare_earth"), Some(&2_000));
        assert!(removed.is_balanced());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn authored_block_placement_replays_or_aborts_atomically() {
        let (root, reg, mut world) = accounted_world("authored-block-crash", 131);
        let block = reg.block_id("base:iron_block").unwrap();
        let definition = reg.block(block);
        let committed = BlockPos::of_world(10, 121, 10).unwrap();
        let before = reg.block(world.get_block_at(committed)).name.clone();
        world
            .material_ledger
            .as_mut()
            .unwrap()
            .begin_authored_place(
                committed,
                &before,
                &definition.name,
                &definition.materials,
                "crash fixture",
            )
            .unwrap()
            .unwrap();
        world.set_block_at(committed, block);
        let report = world.save_modified();
        assert!(report.is_ok(), "{}", report.summary());
        drop(world);

        let mut reloaded = crate::world::World::load_or_create(root.clone(), reg.clone()).unwrap();
        assert_eq!(reloaded.get_block_at(committed), block);
        let committed_audit = reloaded.material_ledger.as_ref().unwrap().audit();
        assert_eq!(committed_audit.placed, definition.materials);
        assert_eq!(committed_audit.external_additions, definition.materials);
        assert_eq!(
            committed_audit.external_sources.get("crash fixture"),
            Some(&definition.materials)
        );
        assert!(committed_audit.is_balanced());

        let aborted = BlockPos::of_world(11, 121, 10).unwrap();
        let before = reg.block(reloaded.get_block_at(aborted)).name.clone();
        reloaded
            .material_ledger
            .as_mut()
            .unwrap()
            .begin_authored_place(
                aborted,
                &before,
                &definition.name,
                &definition.materials,
                "aborted fixture",
            )
            .unwrap()
            .unwrap();
        drop(reloaded);

        let recovered = crate::world::World::load_or_create(root.clone(), reg.clone()).unwrap();
        assert_ne!(recovered.get_block_at(aborted), block);
        let recovered_audit = recovered.material_ledger.as_ref().unwrap().audit();
        assert_eq!(recovered_audit.placed, definition.materials);
        assert_eq!(recovered_audit.external_additions, definition.materials);
        assert!(
            !recovered_audit
                .external_sources
                .contains_key("aborted fixture")
        );
        assert!(recovered_audit.is_balanced());
        let _ = std::fs::remove_dir_all(root);
    }

    fn build_accounted_bloomery(world: &mut crate::world::World, reg: &Registry) {
        let firebrick = reg.block_id("base:firebrick").unwrap();
        let mouth = reg.block_id("base:bloomery").unwrap();
        let (mx, my, mz) = (10, 120, 10);
        let (cx, cz) = (mx + 1, mz);
        for ly in 0..3 {
            for rx in -1..=1i32 {
                for rz in -1..=1i32 {
                    if rx != 0 || rz != 0 {
                        world.set_block(cx + rx, my + ly, cz + rz, firebrick);
                    }
                }
            }
            world.set_block(cx, my + ly, cz, crate::registry::AIR);
        }
        world.set_block(mx, my, mz, mouth);
    }

    fn build_accounted_forge(world: &mut crate::world::World, reg: &Registry) {
        let firebrick = reg.block_id("base:firebrick").unwrap();
        let mouth = reg.block_id("base:forge").unwrap();
        let anvil = reg.block_id("base:stone_anvil").unwrap();
        let (mx, my, mz) = (10, 120, 10);
        let (cx, cz) = (mx + 1, mz);
        for ly in 0..6 {
            for rx in -1..=1i32 {
                for rz in -1..=1i32 {
                    if rx != 0 || rz != 0 {
                        world.set_block(cx + rx, my + ly, cz + rz, firebrick);
                    }
                }
            }
            world.set_block(cx, my + ly, cz, crate::registry::AIR);
        }
        world.set_block(mx, my, mz, mouth);
        world.set_block(mx - 1, my, mz, anvil);
    }

    #[test]
    fn furnace_runtime_moves_fuel_to_a_named_sink_and_survives_reload() {
        let (root, reg, mut world) = accounted_world("furnace-runtime", 101);
        let pos = (10, 120, 10);
        let raw = reg.item_id("base:raw_copper").unwrap();
        let coal = reg.item_id("base:coal").unwrap();
        world
            .material_ledger
            .as_mut()
            .unwrap()
            .record_external_stack(&reg, ItemStack::new(&reg, raw, 1), "test charge")
            .unwrap();
        world
            .material_ledger
            .as_mut()
            .unwrap()
            .record_external_stack(&reg, ItemStack::new(&reg, coal, 1), "test fuel")
            .unwrap();
        world.set_block(pos.0, pos.1, pos.2, reg.block_id("base:furnace").unwrap());
        world.insert_block_entity(
            pos,
            crate::world::BlockEntity::Furnace(crate::world::FurnaceState {
                input: Some(ItemStack::new(&reg, raw, 1)),
                fuel: Some(ItemStack::new(&reg, coal, 1)),
                ..Default::default()
            }),
        );
        for _ in 0..100 {
            world.tick_entities(0.1);
        }
        let audit = world.material_ledger.as_ref().unwrap().audit();
        assert_eq!(
            audit.circulating.get("copper"),
            Some(&CANONICAL_INGOT_UNITS)
        );
        assert_eq!(
            audit.consumption_loss.get("coal"),
            Some(&CANONICAL_INGOT_UNITS)
        );
        assert!(audit.is_balanced());
        let report = world.save_modified();
        assert!(report.is_ok(), "{}", report.summary());
        drop(world);

        let reloaded = crate::world::World::load_or_create(root.clone(), reg).unwrap();
        assert_eq!(reloaded.material_ledger.as_ref().unwrap().audit(), audit);
        assert_eq!(audit_world(&root).unwrap(), audit);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn food_consumption_offering_and_spoilage_are_explicit_material_sinks() {
        use crate::world::{BlockEntity, ChestState, OfferingState};

        let (root, reg, mut world) = accounted_world("finite-food-sinks", 102);
        let salted = reg.item_id("base:salted_meat").unwrap();
        let mush = reg.item_id("base:spoiled_mush").unwrap();
        assert_eq!(reg.item(salted).materials.get("salt"), Some(&1_200));
        assert!(
            reg.item(salted).broken_into.is_none(),
            "freshness is not forge durability"
        );
        assert!(reg.item(mush).materials.is_empty());

        let stock = ItemStack::new(&reg, salted, 3);
        world
            .material_ledger
            .as_mut()
            .unwrap()
            .record_external_stack(&reg, stock, "finite food fixture")
            .unwrap();

        let mut offering = OfferingState::default();
        offering.slots[0] = Some(ItemStack::new(&reg, salted, 1));
        world.insert_block_entity((10, 120, 10), BlockEntity::Offering(offering));
        assert!(world.accept_offerings() > 0.0);

        let mut chest = ChestState::default();
        let mut nearly_spoiled = ItemStack::new(&reg, salted, 1);
        nearly_spoiled.durability = 1;
        chest.slots[0] = Some(nearly_spoiled);
        world.insert_block_entity((12, 120, 10), BlockEntity::Chest(chest));
        world.tick_entities(20.0);
        assert!(matches!(
            world.block_entity(&(12, 120, 10)),
            Some(BlockEntity::Chest(chest)) if chest.slots[0].is_some_and(|stack| stack.item == mush)
        ));

        // Eating, animal feed, and compost call this same world boundary.
        world
            .record_consumed_stacks([ItemStack::new(&reg, salted, 1)])
            .unwrap();
        let audit = world.material_ledger.as_ref().unwrap().audit();
        assert_eq!(audit.consumption_loss.get("salt"), Some(&3_600));
        assert_eq!(audit.circulating.get("salt").copied().unwrap_or(0), 0);
        assert!(audit.is_balanced());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn bloomery_never_mints_a_free_bloom_and_reports_physical_slag() {
        use crate::world::multiblock::MachineKind;
        use crate::world::{BLOOMERY_FIRE_SECS, BlockEntity, MachineInstance};

        let (root, reg, mut world) = accounted_world("bloomery-runtime", 103);
        build_accounted_bloomery(&mut world, &reg);
        let iron = reg.item_id("base:iron_ingot").unwrap();
        let charcoal = reg.item_id("base:charcoal").unwrap();
        let bloom = reg.item_id("base:steel_bloom").unwrap();
        let core = BlockPos::of_world(11, 120, 10).unwrap();

        world
            .material_ledger
            .as_mut()
            .unwrap()
            .record_external_stack(&reg, ItemStack::new(&reg, iron, 1), "test charge")
            .unwrap();
        let mut short = MachineInstance {
            kind: MachineKind::Bloomery,
            lit: true,
            progress: BLOOMERY_FIRE_SECS,
            core: Some(core),
            ..Default::default()
        };
        short.charge[0] = Some(ItemStack::new(&reg, iron, 1));
        short.fuel[0] = Some(ItemStack::new(&reg, charcoal, 1));
        world.insert_block_entity((10, 120, 10), BlockEntity::Multiblock(short));
        world.tick_entities(0.1);
        let Some(BlockEntity::Multiblock(short)) = world.block_entity(&(10, 120, 10)) else {
            panic!("bloomery state")
        };
        assert!(
            short
                .charge
                .iter()
                .flatten()
                .all(|stack| stack.item != bloom)
        );
        assert_eq!(
            short
                .charge
                .iter()
                .flatten()
                .filter(|stack| stack.item == iron)
                .map(|stack| stack.count)
                .sum::<u32>(),
            1,
            "an undersized batch remains intact"
        );

        world
            .material_ledger
            .as_mut()
            .unwrap()
            .record_external_stack(&reg, ItemStack::new(&reg, iron, 8), "test charge")
            .unwrap();
        let mut full = MachineInstance {
            kind: MachineKind::Bloomery,
            lit: true,
            progress: BLOOMERY_FIRE_SECS,
            core: Some(core),
            ..Default::default()
        };
        for slot in 0..4 {
            full.charge[slot] = Some(ItemStack::new(&reg, iron, 2));
            full.fuel[slot] = Some(ItemStack::new(&reg, charcoal, 2));
        }
        world.insert_block_entity((10, 120, 10), BlockEntity::Multiblock(full));
        world.tick_entities(0.1);
        let audit = world.material_ledger.as_ref().unwrap().audit();
        // One untouched test ingot plus six blooms remain primary; the two
        // lost bloom units exist as recoverable physical slag.
        assert_eq!(
            audit.circulating.get("iron"),
            Some(&(7 * CANONICAL_INGOT_UNITS))
        );
        assert_eq!(
            audit.secondary.get("iron"),
            Some(&(2 * CANONICAL_INGOT_UNITS))
        );
        assert!(audit.is_balanced());
        let slag = reg.item_id("base:iron_slag").unwrap();
        assert_eq!(
            world
                .pending_drops()
                .iter()
                .filter(|(_, stack)| stack.item == slag)
                .map(|(_, stack)| stack.count)
                .sum::<u32>(),
            2
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn forge_runtime_enforces_ninety_and_ninety_five_percent_recovery() {
        use crate::world::multiblock::MachineKind;
        use crate::world::{BlockEntity, FORGE_FIRE_SECS, MachineInstance};

        let (root, reg, mut world) = accounted_world("forge-salvage-runtime", 109);
        build_accounted_forge(&mut world, &reg);
        let tool = reg.item_id("base:bronze_pickaxe").unwrap();
        let damaged = reg.item(tool).broken_into.unwrap();
        let machine = reg.block_id("base:separator").unwrap();
        let bundle = reg.block(machine).dismantles_to.unwrap();
        let inputs = [damaged, bundle];
        let mut original = MaterialVector::new();
        let mut expected_secondary = MaterialVector::new();
        for input in inputs {
            let stack = ItemStack::new(&reg, input, 1);
            add_vector(&mut original, &stack_materials(&reg, stack), 1);
            world
                .material_ledger
                .as_mut()
                .unwrap()
                .record_external_stack(&reg, stack, "test salvage stock")
                .unwrap();
            let salvage = reg
                .forge_salvage
                .iter()
                .find(|recipe| recipe.input == input)
                .unwrap();
            add_vector(
                &mut expected_secondary,
                &reg.item(salvage.byproduct).materials,
                1,
            );
        }
        let charcoal = reg.item_id("base:charcoal").unwrap();
        let mut state = MachineInstance {
            kind: MachineKind::Forge,
            lit: true,
            progress: FORGE_FIRE_SECS,
            core: Some(BlockPos::of_world(11, 120, 10).unwrap()),
            ..Default::default()
        };
        state.charge[0] = Some(ItemStack::new(&reg, damaged, 1));
        state.charge[1] = Some(ItemStack::new(&reg, bundle, 1));
        state.fuel[0] = Some(ItemStack::new(&reg, charcoal, 1));
        world.insert_block_entity((10, 120, 10), BlockEntity::Multiblock(state));
        world.tick_entities(0.1);

        let audit = world.material_ledger.as_ref().unwrap().audit();
        assert_eq!(audit.secondary, expected_secondary);
        let mut expected_primary = original;
        subtract_vector(&mut expected_primary, &expected_secondary).unwrap();
        assert_eq!(audit.circulating, expected_primary);
        assert!(audit.is_balanced());
        assert_eq!(
            world.pending_drops().len(),
            4,
            "recovered stock and scale for each source object"
        );

        let stock = world
            .take_pending_drops()
            .into_iter()
            .map(|(_, stack)| stack)
            .filter(|stack| is_reclaimable_stock(&reg, stack.item))
            .collect::<Vec<_>>();
        assert_eq!(stock.len(), 2);
        let mut second = MachineInstance {
            kind: MachineKind::Forge,
            lit: true,
            progress: FORGE_FIRE_SECS,
            core: Some(BlockPos::of_world(11, 120, 10).unwrap()),
            ..Default::default()
        };
        for (slot, stack) in stock.iter().copied().enumerate() {
            second.charge[slot] = Some(stack);
        }
        second.fuel[0] = Some(ItemStack::new(&reg, charcoal, 1));
        world.insert_block_entity((10, 120, 10), BlockEntity::Multiblock(second));
        world.tick_entities(0.1);
        let products = world
            .take_pending_drops()
            .into_iter()
            .map(|(_, stack)| stack)
            .collect::<Vec<_>>();
        assert!(
            products
                .iter()
                .all(|stack| !reg.item(stack.item).name.contains('/')),
            "fractional salvage consolidates into ordinary recipe stock"
        );
        let Some(BlockEntity::Multiblock(forge)) = world.block_entity(&(10, 120, 10)) else {
            panic!("forge state")
        };
        let mut material_after_consolidation = forge.reclaim.clone();
        for stack in products {
            add_vector(
                &mut material_after_consolidation,
                &stack_materials(&reg, stack),
                1,
            );
        }
        assert_eq!(material_after_consolidation, expected_primary);
        assert!(
            world
                .material_ledger
                .as_ref()
                .unwrap()
                .audit()
                .is_balanced()
        );

        let bank_before_save = forge.reclaim.clone();
        let report = world.save_modified();
        assert!(report.is_ok(), "{}", report.summary());
        drop(world);
        let reloaded = crate::world::World::load_or_create(root.clone(), reg.clone()).unwrap();
        let Some(BlockEntity::Multiblock(forge)) = reloaded.block_entity(&(10, 120, 10)) else {
            panic!("persisted forge state")
        };
        assert_eq!(forge.reclaim, bank_before_save);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn worked_and_kiln_runtime_paths_declare_every_finite_loss() {
        use crate::world::multiblock::MachineKind;
        use crate::world::{BlockEntity, KILN_FIRE_SECS, MachineInstance};

        let (root, reg, mut world) = accounted_world("worked-kiln-runtime", 113);
        let monazite = reg.item_id("base:monazite_grit").unwrap();
        world
            .material_ledger
            .as_mut()
            .unwrap()
            .record_external_stack(&reg, ItemStack::new(&reg, monazite, 1), "test mineral")
            .unwrap();
        world.set_block(10, 120, 10, reg.block_id("base:quern").unwrap());
        assert!(world.anvil_put((10, 120, 10), ItemStack::new(&reg, monazite, 1)));
        assert!(world.anvil_strike((10, 120, 10)).is_none());
        assert!(world.anvil_strike((10, 120, 10)).is_none());
        let powder = world
            .anvil_strike((10, 120, 10))
            .expect("third turn produces rare-earth powder");
        assert_eq!(powder.count, 2);
        let after_work = world.material_ledger.as_ref().unwrap().audit();
        assert_eq!(after_work.circulating.get("rare_earth"), Some(&800));
        assert_eq!(after_work.consumption_loss.get("rare_earth"), Some(&400));
        assert!(after_work.is_balanced());

        build_accounted_bloomery(&mut world, &reg);
        world.set_block(10, 120, 10, reg.block_id("base:kiln").unwrap());
        let gold = reg.item_id("base:raw_gold").unwrap();
        let sand = reg.item_id("base:sand").unwrap();
        let charcoal = reg.item_id("base:charcoal").unwrap();
        world
            .material_ledger
            .as_mut()
            .unwrap()
            .record_external_stack(&reg, ItemStack::new(&reg, gold, 1), "test pigment")
            .unwrap();
        let mut kiln = MachineInstance {
            kind: MachineKind::Kiln,
            lit: true,
            progress: KILN_FIRE_SECS,
            core: Some(BlockPos::of_world(11, 120, 10).unwrap()),
            ..Default::default()
        };
        kiln.charge[0] = Some(ItemStack::new(&reg, sand, 2));
        kiln.fuel[0] = Some(ItemStack::new(&reg, charcoal, 2));
        kiln.reagent = Some(ItemStack::new(&reg, gold, 1));
        world.insert_block_entity((10, 120, 10), BlockEntity::Multiblock(kiln));
        world.force_local_weather("clear");
        world.tick_entities(0.1);
        let audit = world.material_ledger.as_ref().unwrap().audit();
        assert_eq!(
            audit.consumption_loss.get("gold"),
            Some(&CANONICAL_INGOT_UNITS)
        );
        assert!(audit.is_balanced());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn post_creation_retrogen_is_host_only_touched_safe_idempotent_and_reserved() {
        let root = std::env::temp_dir().join(format!(
            "wildforge-material-retrogen-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        crate::world::create_world_fixture_atomic(
            &root,
            127,
            "survival",
            8,
            &crate::planet_atlas::CancellationToken::default(),
            |_| {},
        )
        .unwrap();
        let base = std::sync::Arc::new(crate::registry::load(Path::new(
            "__no_material_test_mods__",
        )));
        let gems = std::sync::Arc::new(crate::registry::load(Path::new("mods")));
        let mut before_world =
            crate::world::World::load_or_create(root.clone(), base.clone()).unwrap();
        let atlas = before_world.planet_atlas().unwrap();
        let generator = crate::worldgen::Generator::with_atlas(127, &gems, atlas.clone());
        let ruby = gems.block_id("gems:ruby_ore").unwrap();
        let mut candidates = Vec::new();
        for site in atlas
            .geology
            .deposits
            .iter()
            .filter(|site| site.mineral == MineralKind::Other)
        {
            let surface = crate::planet::SurfacePos::new(
                site.pos.face,
                site.pos.u * atlas.cell_blocks() + atlas.cell_blocks() / 2,
                site.pos.v * atlas.cell_blocks() + atlas.cell_blocks() / 2,
            )
            .unwrap();
            let center = ChunkPos::from_surface(surface);
            for du in -6..=6 {
                for dv in -6..=6 {
                    let chunk = center.offset(du, dv);
                    if candidates.contains(&chunk) {
                        continue;
                    }
                    let reference = generator.generate(chunk, &gems);
                    if reference.raw().contains(&ruby.0) {
                        before_world.ensure_chunk(chunk);
                        if !before_world.chunks().get(&chunk).unwrap().modified {
                            candidates.push(chunk);
                        }
                    }
                    if candidates.len() == 4 {
                        break;
                    }
                }
                if candidates.len() == 4 {
                    break;
                }
            }
            if candidates.len() == 4 {
                break;
            }
        }
        assert_eq!(candidates.len(), 4, "fixture exposes four ruby host chunks");
        let untouched = candidates[0];
        let touched = candidates[1];
        let structured = candidates[2];
        let block_entity = candidates[3];
        let names = |world: &crate::world::World, chunk: ChunkPos| {
            world.chunks()[&chunk]
                .raw()
                .into_iter()
                .map(|id| world.reg.block(crate::registry::BlockId(id)).name.clone())
                .collect::<Vec<_>>()
        };
        let untouched_before = names(&before_world, untouched);
        let authored = BlockPos::new(
            touched.face(),
            touched.u() * CHUNK_X as u16 + 8,
            200,
            touched.v() * CHUNK_Z as u16 + 8,
        )
        .unwrap();
        assert!(before_world.place_block_at(authored, base.block_id("base:cobblestone").unwrap()));
        let touched_before = names(&before_world, touched);
        before_world.mark_structure_chunk_for_test(structured);
        let structured_before = names(&before_world, structured);
        let entity_pos = BlockPos::new(
            block_entity.face(),
            block_entity.u() * CHUNK_X as u16 + 8,
            200,
            block_entity.v() * CHUNK_Z as u16 + 8,
        )
        .unwrap();
        before_world.insert_block_entity_at(
            entity_pos,
            crate::world::BlockEntity::Anvil(Default::default()),
        );
        before_world
            .chunks_mut()
            .get_mut(&block_entity)
            .unwrap()
            .modified = true;
        let entity_before = names(&before_world, block_entity);
        // Force the genuinely untouched chunk to disk without giving it
        // authored provenance; save success clears the transient dirty flag.
        before_world
            .chunks_mut()
            .get_mut(&untouched)
            .unwrap()
            .modified = true;
        let report = before_world.save_modified();
        assert!(report.is_ok(), "{}", report.summary());
        drop(before_world);

        let mut world = crate::world::World::load_or_create(root.clone(), gems.clone()).unwrap();
        world.ensure_chunk(untouched);
        world.ensure_chunk(touched);
        world.ensure_chunk(structured);
        world.ensure_chunk(block_entity);
        let untouched_after = names(&world, untouched);
        let touched_after = names(&world, touched);
        let structured_after = names(&world, structured);
        let entity_after = names(&world, block_entity);
        let changed = untouched_before
            .iter()
            .zip(&untouched_after)
            .filter(|(before, after)| before != after)
            .count();
        assert!(changed > 0, "untouched exact host rock receives ruby");
        let declared_hosts = gems
            .ores
            .iter()
            .filter(|ore| ore.resource_key == "gems:ruby_ore")
            .map(|ore| gems.block(ore.replaces).name.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert!(
            untouched_before
                .iter()
                .zip(&untouched_after)
                .all(|(before, after)| before == after
                    || (declared_hosts.contains(before.as_str()) && after == "gems:ruby_ore")),
            "retrogen replaces only exact declared hosts"
        );
        assert_eq!(
            touched_after, touched_before,
            "a player-touched chunk stays voxel-identical"
        );
        assert_eq!(
            structured_after, structured_before,
            "a structure chunk stays voxel-identical"
        );
        assert_eq!(
            entity_after, entity_before,
            "a chunk containing a block entity stays voxel-identical"
        );
        let record = &world.material_ledger.as_ref().unwrap().retrogen["gems:ruby_ore"];
        assert!(record.applied_chunks.contains(&untouched));
        assert!(!record.applied_chunks.contains(&touched));
        assert!(!record.applied_chunks.contains(&structured));
        assert!(!record.applied_chunks.contains(&block_entity));
        assert_eq!(record.algorithm_version, 1);

        world.unload_chunk(untouched);
        world.ensure_chunk(untouched);
        let reloaded = names(&world, untouched);
        assert!(
            reloaded == untouched_after,
            "retrogen rerun/reload changed {} cells",
            reloaded
                .iter()
                .zip(&untouched_after)
                .filter(|(left, right)| left != right)
                .count()
        );

        let ruby_pos = 'found: {
            for y in 1..CHUNK_Y {
                for z in 0..CHUNK_Z {
                    for x in 0..CHUNK_X {
                        let pos = BlockPos::new(
                            untouched.face(),
                            untouched.u() * CHUNK_X as u16 + x as u16,
                            y as u8,
                            untouched.v() * CHUNK_Z as u16 + z as u16,
                        )
                        .unwrap();
                        if world.get_block_at(pos) == ruby {
                            break 'found pos;
                        }
                    }
                }
            }
            panic!("retrogen ruby position")
        };
        let before = world.material_ledger.as_ref().unwrap().audit();
        let pick = gems.item_id("base:bronze_pickaxe").unwrap();
        let drop = world
            .break_block_at(ruby_pos, Some(pick), true, false)
            .unwrap()
            .drop
            .unwrap();
        assert_eq!(drop.item, gems.item_id("gems:ruby").unwrap());
        let after = world.material_ledger.as_ref().unwrap().audit();
        assert_eq!(
            after.circulating.get("ruby").copied().unwrap_or_default(),
            before.circulating.get("ruby").copied().unwrap_or_default() + CANONICAL_INGOT_UNITS
        );
        assert_eq!(
            after.underground.get("ruby").copied().unwrap_or_default() + CANONICAL_INGOT_UNITS,
            before.underground.get("ruby").copied().unwrap_or_default()
        );
        assert!(after.is_balanced());
        assert_eq!(
            world.material_ledger.as_ref().unwrap().content_hash,
            gems.content_hash
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_primary_ledger_refuses_to_disable_accounting_silently() {
        let (root, reg, mut world) = accounted_world("corrupt-ledger", 107);
        let copper = reg.item_id("base:copper_ingot").unwrap();
        world
            .material_ledger
            .as_mut()
            .unwrap()
            .record_external_stack(&reg, ItemStack::new(&reg, copper, 1), "test")
            .unwrap();
        world.material_ledger.as_ref().unwrap().save().unwrap();
        assert!(root.join(LEDGER_BACKUP).is_file());
        std::fs::write(root.join(LEDGER_FILE), b"corrupt material ledger").unwrap();
        drop(world);
        assert!(audit_world(&root).is_err());
        assert!(
            crate::world::World::load_or_create(root.clone(), reg).is_err(),
            "a corrupt ledger must stop the finite world, not turn accounting off"
        );
        assert!(root.join(LEDGER_BACKUP).is_file());
        let _ = std::fs::remove_dir_all(root);
    }
}
