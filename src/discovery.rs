//! Host-owned, physical magical knowledge.
//!
//! Discovery is deliberately not a character unlock tree.  The authoritative
//! world signs qualitative measurements, while ledgers and archaeological
//! objects merely hold references to those records.  A client can ask the
//! host to observe or copy something it can physically reach; it never sends
//! a reading, an author, a location, or an object id to be minted.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};

use crate::identity::PlayerId;
use crate::planet::BlockPos;

pub const DISCOVERY_SCHEMA_VERSION: u32 = 1;
// TOML integers are signed 64-bit. Keep physical-knowledge identities in a
// disjoint, enormous positive range without producing undecodable saves.
pub const KNOWLEDGE_ID_BIT: u64 = 1 << 62;

pub const fn is_knowledge_id(id: u64) -> bool {
    id & KNOWLEDGE_ID_BIT != 0
}
pub const FIELD_LEDGER_RECORDS: usize = 32;
pub const SURVEY_FOLIO_RECORDS: usize = 128;
pub const MAX_DISCOVERY_OBJECTS: usize = 16_384;
pub const MAX_DISCOVERY_RECORDS: usize = 65_536;
pub const MAX_DISCOVERY_FILE_BYTES: usize = 32 * 1024 * 1024;
pub const LABEL_MAX_CHARS: usize = 40;

const STATE_FILE: &str = "discovery.toml";
const BACKUP_FILE: &str = "discovery.toml.bak";

pub const EVIDENCE_CLASSES: [&str; 7] = [
    "maker_tablet",
    "calibration_plate",
    "spent_charm_fitting",
    "broken_focus",
    "sealed_dross_ampoule",
    "site_survey_marks",
    "failed_containment_fragment",
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrengthBand {
    Still,
    Faint,
    Steady,
    Strong,
    Saturated,
}

impl fmt::Display for StrengthBand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Still => "still",
            Self::Faint => "faint",
            Self::Steady => "steady",
            Self::Strong => "strong",
            Self::Saturated => "saturated",
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StabilityBand {
    Stable,
    Variable,
    Strained,
    Fouled,
}

impl fmt::Display for StabilityBand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Stable => "stable",
            Self::Variable => "variable",
            Self::Strained => "strained",
            Self::Fouled => "fouled",
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationGrade {
    Uncalibrated,
    Field,
    Plate,
}

impl CalibrationGrade {
    pub fn uncertainty_reduction(self) -> u8 {
        match self {
            Self::Uncalibrated => 0,
            Self::Field => 18,
            Self::Plate => 34,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentKind {
    Capacity,
    Conductivity,
    Stability,
    BiologicalResponse,
    DrossResponse,
}

impl ExperimentKind {
    pub const ALL: [Self; 5] = [
        Self::Capacity,
        Self::Conductivity,
        Self::Stability,
        Self::BiologicalResponse,
        Self::DrossResponse,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Capacity => "capacity comparison",
            Self::Conductivity => "conductivity trace",
            Self::Stability => "stability trial",
            Self::BiologicalResponse => "biological response",
            Self::DrossResponse => "dross response",
        }
    }

    pub fn reference_item(self) -> &'static str {
        match self {
            Self::Capacity => "base:capacity_reference",
            Self::Conductivity => "base:conductivity_reference",
            Self::Stability => "base:stability_reference",
            Self::BiologicalResponse => "base:biological_reference",
            Self::DrossResponse => "base:dross_reference",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value
            .trim()
            .to_ascii_lowercase()
            .replace([' ', '-'], "_")
            .as_str()
        {
            "capacity" => Some(Self::Capacity),
            "conductivity" => Some(Self::Conductivity),
            "stability" => Some(Self::Stability),
            "biological_response" | "biological" => Some(Self::BiologicalResponse),
            "dross_response" | "dross" => Some(Self::DrossResponse),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QualitativeReading {
    pub strength: StrengthBand,
    pub stability: StabilityBand,
    /// At most two host-selected, named resonances.  Exact mixtures stay in
    /// the Current ledger and never enter a discovery record.
    pub resonances: Vec<String>,
    pub dross: StrengthBand,
    pub drift: Option<String>,
    /// 0 is the best possible qualitative reading; 100 is nearly useless.
    pub uncertainty: u8,
    /// Bounded, visible experimental properties. Values are prose bands, not
    /// hidden numerical state.
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}

impl QualitativeReading {
    pub fn normalize(&mut self) {
        self.resonances.truncate(2);
        for resonance in &mut self.resonances {
            resonance.truncate(48);
        }
        self.drift = self.drift.take().map(|mut drift| {
            drift.truncate(48);
            drift
        });
        self.uncertainty = self.uncertainty.min(100);
        self.properties = std::mem::take(&mut self.properties)
            .into_iter()
            .take(12)
            .map(|(mut key, mut value)| {
                key.truncate(48);
                value.truncate(96);
                (key, value)
            })
            .collect();
    }

    pub fn compact_text(&self) -> String {
        let resonance = if self.resonances.is_empty() {
            "unclear resonance".to_string()
        } else {
            self.resonances.join(" / ")
        };
        let drift = self
            .drift
            .as_deref()
            .map_or_else(|| "no clear drift".to_string(), |d| format!("drifting {d}"));
        format!(
            "{} Current; {}; {}; dross {}; {}; uncertainty {}%",
            self.strength, self.stability, resonance, self.dross, drift, self.uncertainty
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlanetaryProvenance {
    pub face: String,
    pub atlas_u: Option<u16>,
    pub atlas_v: Option<u16>,
    pub biome: String,
    pub place: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservationRecord {
    pub id: u64,
    pub phenomenon_id: String,
    pub category: String,
    /// Exact position is host custody.  Copies may deliberately omit it.
    pub position: Option<BlockPos>,
    pub provenance: PlanetaryProvenance,
    pub day: u32,
    pub time_permille: u16,
    pub season: String,
    pub calibration: CalibrationGrade,
    pub reading: QualitativeReading,
    pub observer: PlayerId,
    /// Presentation attribution captured at observation time. Identity policy
    /// still uses `observer`, never this mutable display string.
    pub observer_name: String,
    pub label: Option<String>,
    pub schema_version: u32,
    pub content_version: u64,
    pub copied_from: Option<u64>,
    pub signature: [u8; 32],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum KnowledgeKind {
    FieldLedger,
    SurveyFolio,
    Artifact {
        evidence_class: String,
        text: String,
    },
    CalibrationPlate {
        grade: CalibrationGrade,
        evidence_class: Option<String>,
        text: Option<String>,
    },
    ReferenceObject {
        experiment: ExperimentKind,
    },
}

impl KnowledgeKind {
    fn record_capacity(&self) -> usize {
        match self {
            Self::FieldLedger => FIELD_LEDGER_RECORDS,
            Self::SurveyFolio => SURVEY_FOLIO_RECORDS,
            _ => 0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct KnowledgeObject {
    pub id: u64,
    pub content_id: String,
    pub kind: KnowledgeKind,
    #[serde(default)]
    pub records: Vec<u64>,
    pub created_day: u32,
    pub origin: Option<BlockPos>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ObservationSummary {
    pub record_id: u64,
    pub phenomenon_id: String,
    pub category: String,
    pub reading: String,
    pub properties: Vec<(String, String)>,
    pub observer: PlayerId,
    pub observer_name: String,
    pub label: Option<String>,
    pub day: u32,
    pub season: String,
    pub location: Option<BlockPos>,
    pub provenance: PlanetaryProvenance,
    pub obsolete_content: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LibraryIndex {
    pub records: Vec<ObservationSummary>,
    pub phenomenon_counts: Vec<(String, u16)>,
    pub disagreements: Vec<String>,
    pub omitted: u16,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DiscoveryFile {
    version: u32,
    world_seed: u32,
    current_content_version: u64,
    next_object_id: u64,
    next_record_id: u64,
    signing_key: [u8; 32],
    objects: BTreeMap<u64, KnowledgeObject>,
    records: BTreeMap<u64, ObservationRecord>,
    #[serde(default)]
    recovered_sites: BTreeSet<String>,
    #[serde(default)]
    installed_sites: BTreeSet<String>,
}

pub struct DiscoveryState {
    path: PathBuf,
    file: DiscoveryFile,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiscoveryError {
    Io(String),
    Corrupt(String),
    Full(&'static str),
    MissingObject(u64),
    MissingRecord(u64),
    WrongObjectKind,
    DuplicateRecord,
    ForgedRecord(u64),
    InvalidLabel,
}

impl fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) | Self::Corrupt(e) => f.write_str(e),
            Self::Full(kind) => write!(f, "{kind} has reached its bounded capacity"),
            Self::MissingObject(id) => write!(f, "unknown physical knowledge object {id}"),
            Self::MissingRecord(id) => write!(f, "unknown observation record {id}"),
            Self::WrongObjectKind => f.write_str("that physical object cannot hold observations"),
            Self::DuplicateRecord => f.write_str("that exact observation is already present"),
            Self::ForgedRecord(id) => write!(f, "observation {id} failed its host signature"),
            Self::InvalidLabel => f.write_str("the note contains unsupported or excessive text"),
        }
    }
}

impl std::error::Error for DiscoveryError {}

impl From<std::io::Error> for DiscoveryError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

#[derive(Clone, Debug)]
pub struct NewObservation {
    pub phenomenon_id: String,
    pub category: String,
    pub position: Option<BlockPos>,
    pub provenance: PlanetaryProvenance,
    pub day: u32,
    pub time_permille: u16,
    pub season: String,
    pub calibration: CalibrationGrade,
    pub reading: QualitativeReading,
    pub observer: PlayerId,
    pub observer_name: String,
    pub label: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryAudit {
    pub objects: usize,
    pub records: usize,
    pub invalid_signatures: usize,
    pub dangling_references: usize,
    pub evidence_counts: BTreeMap<String, usize>,
    pub installed_sites: usize,
    pub recovered_sites: usize,
    pub file_bytes: u64,
}

impl DiscoveryAudit {
    pub fn is_qualified(&self) -> bool {
        self.invalid_signatures == 0
            && self.dangling_references == 0
            && self.objects <= MAX_DISCOVERY_OBJECTS
            && self.records <= MAX_DISCOVERY_RECORDS
            && self.file_bytes <= MAX_DISCOVERY_FILE_BYTES as u64
            && self.installed_sites >= 3
    }

    pub fn render(&self) -> String {
        format!(
            "Discovery audit: {} objects, {} records, {} invalid signatures, {} dangling references, {} bytes\nSites: {} installed, {} recovered\nEvidence: {:?}\nstatus: {}\n",
            self.objects,
            self.records,
            self.invalid_signatures,
            self.dangling_references,
            self.file_bytes,
            self.installed_sites,
            self.recovered_sites,
            self.evidence_counts,
            if self.is_qualified() {
                "qualified"
            } else {
                "FAILED"
            }
        )
    }
}

impl DiscoveryState {
    pub fn load_or_initialize(
        world: &Path,
        world_seed: u32,
        content_version: u64,
    ) -> Result<Self, DiscoveryError> {
        let path = world.join(STATE_FILE);
        let backup = world.join(BACKUP_FILE);
        if !path.exists() {
            let mut signing_key = [0u8; 32];
            SystemRandom::new()
                .fill(&mut signing_key)
                .map_err(|_| DiscoveryError::Io("secure discovery key generation failed".into()))?;
            let mut state = Self {
                path,
                file: DiscoveryFile {
                    version: DISCOVERY_SCHEMA_VERSION,
                    world_seed,
                    current_content_version: content_version,
                    next_object_id: KNOWLEDGE_ID_BIT | 1,
                    next_record_id: 1,
                    signing_key,
                    objects: BTreeMap::new(),
                    records: BTreeMap::new(),
                    recovered_sites: BTreeSet::new(),
                    installed_sites: BTreeSet::new(),
                },
            };
            state.save()?;
            return Ok(state);
        }
        let primary = Self::load_candidate(&path, world_seed);
        let (mut state, restored) = match primary {
            Ok(file) => (Self { path, file }, false),
            Err(primary_error) => match Self::load_candidate(&backup, world_seed) {
                Ok(file) => (Self { path, file }, true),
                Err(backup_error) => {
                    return Err(DiscoveryError::Corrupt(format!(
                        "discovery state and backup are unrecoverable (primary: {primary_error}; backup: {backup_error})"
                    )));
                }
            },
        };
        state.verify_all()?;
        state.file.current_content_version = content_version;
        if restored {
            state.save()?;
        }
        Ok(state)
    }

    fn load_candidate(path: &Path, world_seed: u32) -> Result<DiscoveryFile, DiscoveryError> {
        let metadata = std::fs::metadata(path)?;
        if metadata.len() > MAX_DISCOVERY_FILE_BYTES as u64 {
            return Err(DiscoveryError::Corrupt(
                "discovery file exceeds its hard budget".into(),
            ));
        }
        let text = std::fs::read_to_string(path)?;
        let file: DiscoveryFile = toml::from_str(&text).map_err(|error| {
            DiscoveryError::Corrupt(format!("invalid discovery state: {error}"))
        })?;
        if file.version != DISCOVERY_SCHEMA_VERSION || file.world_seed != world_seed {
            return Err(DiscoveryError::Corrupt(
                "discovery state belongs to a different schema or world".into(),
            ));
        }
        if file.objects.len() > MAX_DISCOVERY_OBJECTS || file.records.len() > MAX_DISCOVERY_RECORDS
        {
            return Err(DiscoveryError::Corrupt(
                "discovery collection exceeds its hard budget".into(),
            ));
        }
        Ok(file)
    }

    pub fn save(&mut self) -> Result<(), DiscoveryError> {
        self.verify_all()?;
        let bytes = toml::to_string(&self.file)
            .map_err(|error| DiscoveryError::Io(error.to_string()))?
            .into_bytes();
        if bytes.len() > MAX_DISCOVERY_FILE_BYTES {
            return Err(DiscoveryError::Full("discovery save"));
        }
        if let Ok(previous) = std::fs::read(&self.path) {
            crate::persist::atomic_write(&self.path.with_file_name(BACKUP_FILE), &previous, true)?;
        }
        crate::persist::atomic_write(&self.path, &bytes, true)?;
        Ok(())
    }

    fn verify_all(&self) -> Result<(), DiscoveryError> {
        if self.file.objects.len() > MAX_DISCOVERY_OBJECTS {
            return Err(DiscoveryError::Full("knowledge object census"));
        }
        if self.file.records.len() > MAX_DISCOVERY_RECORDS {
            return Err(DiscoveryError::Full("observation census"));
        }
        for (id, object) in &self.file.objects {
            if *id != object.id || object.records.len() > object.kind.record_capacity() {
                return Err(DiscoveryError::Corrupt(format!(
                    "invalid physical knowledge object {id}"
                )));
            }
            for record in &object.records {
                if !self.file.records.contains_key(record) {
                    return Err(DiscoveryError::MissingRecord(*record));
                }
            }
        }
        for (id, record) in &self.file.records {
            if *id != record.id || !self.verify_record(record) {
                return Err(DiscoveryError::ForgedRecord(*id));
            }
        }
        Ok(())
    }

    fn record_bytes(record: &ObservationRecord) -> Vec<u8> {
        let mut unsigned = record.clone();
        unsigned.signature = [0; 32];
        serde_json::to_vec(&unsigned).unwrap_or_default()
    }

    fn sign_record(&self, record: &ObservationRecord) -> [u8; 32] {
        let key = hmac::Key::new(hmac::HMAC_SHA256, &self.file.signing_key);
        let tag = hmac::sign(&key, &Self::record_bytes(record));
        let mut signature = [0u8; 32];
        signature.copy_from_slice(tag.as_ref());
        signature
    }

    pub fn verify_record(&self, record: &ObservationRecord) -> bool {
        let key = hmac::Key::new(hmac::HMAC_SHA256, &self.file.signing_key);
        hmac::verify(&key, &Self::record_bytes(record), &record.signature).is_ok()
    }

    fn allocate_object_id(&mut self) -> Result<u64, DiscoveryError> {
        if self.file.objects.len() >= MAX_DISCOVERY_OBJECTS {
            return Err(DiscoveryError::Full("knowledge object census"));
        }
        let id = self.file.next_object_id | KNOWLEDGE_ID_BIT;
        self.file.next_object_id = id
            .checked_add(1)
            .ok_or(DiscoveryError::Full("knowledge object id space"))?;
        Ok(id)
    }

    pub fn create_object(
        &mut self,
        content_id: impl Into<String>,
        kind: KnowledgeKind,
        day: u32,
        origin: Option<BlockPos>,
    ) -> Result<u64, DiscoveryError> {
        let id = self.allocate_object_id()?;
        self.file.objects.insert(
            id,
            KnowledgeObject {
                id,
                content_id: content_id.into(),
                kind,
                records: Vec::new(),
                created_day: day,
                origin,
            },
        );
        Ok(id)
    }

    pub fn create_artifact(
        &mut self,
        content_id: impl Into<String>,
        evidence_class: impl Into<String>,
        authored_text: &[String],
        day: u32,
        origin: Option<BlockPos>,
    ) -> Result<u64, DiscoveryError> {
        let id = self.allocate_object_id()?;
        let evidence_class = evidence_class.into();
        let text = artifact_phrase(id, &evidence_class, authored_text);
        self.file.objects.insert(
            id,
            KnowledgeObject {
                id,
                content_id: content_id.into(),
                kind: KnowledgeKind::Artifact {
                    evidence_class,
                    text,
                },
                records: Vec::new(),
                created_day: day,
                origin,
            },
        );
        Ok(id)
    }

    pub fn create_calibration_plate(
        &mut self,
        content_id: impl Into<String>,
        grade: CalibrationGrade,
        evidence_class: Option<String>,
        authored_text: &[String],
        day: u32,
        origin: Option<BlockPos>,
    ) -> Result<u64, DiscoveryError> {
        let id = self.allocate_object_id()?;
        let text = evidence_class
            .as_deref()
            .map(|class| artifact_phrase(id, class, authored_text));
        self.file.objects.insert(
            id,
            KnowledgeObject {
                id,
                content_id: content_id.into(),
                kind: KnowledgeKind::CalibrationPlate {
                    grade,
                    evidence_class,
                    text,
                },
                records: Vec::new(),
                created_day: day,
                origin,
            },
        );
        Ok(id)
    }

    /// Attach historical meaning to a pre-existing charged item identity.
    /// This is how a sealed dross ampoule can have one physical id while its
    /// exact contained Current remains in the Current ledger.
    pub fn ensure_object_with_id(
        &mut self,
        id: u64,
        content_id: impl Into<String>,
        kind: KnowledgeKind,
        day: u32,
        origin: Option<BlockPos>,
    ) -> Result<(), DiscoveryError> {
        if id == 0 {
            return Err(DiscoveryError::Corrupt(
                "zero is not a physical object id".into(),
            ));
        }
        if let Some(existing) = self.file.objects.get(&id) {
            if existing.kind != kind {
                return Err(DiscoveryError::Corrupt(format!(
                    "physical object {id} changed discovery identity"
                )));
            }
            return Ok(());
        }
        if self.file.objects.len() >= MAX_DISCOVERY_OBJECTS {
            return Err(DiscoveryError::Full("knowledge object census"));
        }
        self.file.objects.insert(
            id,
            KnowledgeObject {
                id,
                content_id: content_id.into(),
                kind,
                records: Vec::new(),
                created_day: day,
                origin,
            },
        );
        Ok(())
    }

    #[cfg(test)]
    pub fn object(&self, id: u64) -> Option<&KnowledgeObject> {
        self.file.objects.get(&id)
    }

    #[cfg(test)]
    pub fn record(&self, id: u64) -> Option<&ObservationRecord> {
        self.file.records.get(&id)
    }

    pub fn artifact_text(&self, id: u64) -> Option<&str> {
        match &self.file.objects.get(&id)?.kind {
            KnowledgeKind::Artifact { text, .. } => Some(text),
            KnowledgeKind::CalibrationPlate { text, .. } => text.as_deref(),
            _ => None,
        }
    }

    pub fn artifact_origin(&self, id: u64) -> Option<BlockPos> {
        let object = self.file.objects.get(&id)?;
        matches!(&object.kind, KnowledgeKind::Artifact { .. }).then_some(object.origin)?
    }

    pub fn calibration_of(&self, id: u64) -> Option<CalibrationGrade> {
        match self.file.objects.get(&id)?.kind {
            KnowledgeKind::CalibrationPlate { grade, .. } => Some(grade),
            _ => None,
        }
    }

    pub fn create_observation(
        &mut self,
        holder_id: u64,
        mut new: NewObservation,
    ) -> Result<u64, DiscoveryError> {
        let label = match new.label.take() {
            Some(label) => Some(sanitize_label(&label)?),
            None => None,
        };
        new.reading.normalize();
        let capacity = self
            .file
            .objects
            .get(&holder_id)
            .ok_or(DiscoveryError::MissingObject(holder_id))?
            .kind
            .record_capacity();
        if capacity == 0 {
            return Err(DiscoveryError::WrongObjectKind);
        }
        if self.file.objects[&holder_id].records.len() >= capacity {
            return Err(DiscoveryError::Full("field record holder"));
        }
        if self.file.records.len() >= MAX_DISCOVERY_RECORDS {
            return Err(DiscoveryError::Full("observation census"));
        }
        let id = self.file.next_record_id;
        self.file.next_record_id = id
            .checked_add(1)
            .ok_or(DiscoveryError::Full("observation id space"))?;
        let mut record = ObservationRecord {
            id,
            phenomenon_id: bounded(new.phenomenon_id, 96),
            category: bounded(new.category, 48),
            position: new.position,
            provenance: new.provenance,
            day: new.day,
            time_permille: new.time_permille.min(999),
            season: bounded(new.season, 24),
            calibration: new.calibration,
            reading: new.reading,
            observer: new.observer,
            observer_name: bounded(new.observer_name, 32),
            label,
            schema_version: DISCOVERY_SCHEMA_VERSION,
            content_version: self.file.current_content_version,
            copied_from: None,
            signature: [0; 32],
        };
        record.signature = self.sign_record(&record);
        self.file.records.insert(id, record);
        self.file
            .objects
            .get_mut(&holder_id)
            .unwrap()
            .records
            .push(id);
        Ok(id)
    }

    pub fn copy_record(
        &mut self,
        source_holder: u64,
        source_record: u64,
        destination_holder: u64,
        include_location: bool,
    ) -> Result<u64, DiscoveryError> {
        let source = self
            .file
            .objects
            .get(&source_holder)
            .ok_or(DiscoveryError::MissingObject(source_holder))?;
        if !source.records.contains(&source_record) {
            return Err(DiscoveryError::MissingRecord(source_record));
        }
        let mut record = self
            .file
            .records
            .get(&source_record)
            .cloned()
            .ok_or(DiscoveryError::MissingRecord(source_record))?;
        if !self.verify_record(&record) {
            return Err(DiscoveryError::ForgedRecord(source_record));
        }
        let destination = self
            .file
            .objects
            .get(&destination_holder)
            .ok_or(DiscoveryError::MissingObject(destination_holder))?;
        let capacity = destination.kind.record_capacity();
        if capacity == 0 {
            return Err(DiscoveryError::WrongObjectKind);
        }
        let source_root = self.record_root(source_record);
        if destination
            .records
            .iter()
            .any(|record| self.record_root(*record) == source_root)
        {
            return Err(DiscoveryError::DuplicateRecord);
        }
        if destination.records.len() >= capacity || self.file.records.len() >= MAX_DISCOVERY_RECORDS
        {
            return Err(DiscoveryError::Full("record copy destination"));
        }
        let id = self.file.next_record_id;
        self.file.next_record_id = id
            .checked_add(1)
            .ok_or(DiscoveryError::Full("observation id space"))?;
        record.id = id;
        if !include_location {
            record.position = None;
            record.provenance.atlas_u = None;
            record.provenance.atlas_v = None;
            record.provenance.place = None;
        }
        record.copied_from = Some(source_record);
        record.signature = [0; 32];
        record.signature = self.sign_record(&record);
        self.file.records.insert(id, record);
        self.file
            .objects
            .get_mut(&destination_holder)
            .unwrap()
            .records
            .push(id);
        Ok(id)
    }

    fn record_root(&self, mut id: u64) -> u64 {
        for _ in 0..32 {
            let Some(parent) = self
                .file
                .records
                .get(&id)
                .and_then(|record| record.copied_from)
            else {
                break;
            };
            if parent == id {
                break;
            }
            id = parent;
        }
        id
    }

    pub fn summaries(
        &self,
        holder_id: u64,
        reveal_locations: bool,
        current_content_version: u64,
    ) -> Result<Vec<ObservationSummary>, DiscoveryError> {
        let object = self
            .file
            .objects
            .get(&holder_id)
            .ok_or(DiscoveryError::MissingObject(holder_id))?;
        object
            .records
            .iter()
            .map(|id| {
                let record = self
                    .file
                    .records
                    .get(id)
                    .ok_or(DiscoveryError::MissingRecord(*id))?;
                if !self.verify_record(record) {
                    return Err(DiscoveryError::ForgedRecord(*id));
                }
                Ok(ObservationSummary {
                    record_id: *id,
                    phenomenon_id: record.phenomenon_id.clone(),
                    category: record.category.clone(),
                    reading: record.reading.compact_text(),
                    properties: record
                        .reading
                        .properties
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect(),
                    observer: record.observer,
                    observer_name: record.observer_name.clone(),
                    label: record.label.clone(),
                    day: record.day,
                    season: record.season.clone(),
                    location: reveal_locations.then_some(record.position).flatten(),
                    provenance: record.provenance.clone(),
                    obsolete_content: record.content_version != current_content_version,
                })
            })
            .collect()
    }

    pub fn library_index(
        &self,
        holder_id: u64,
        reveal_locations: bool,
        current_content_version: u64,
    ) -> Result<LibraryIndex, DiscoveryError> {
        let all = self.summaries(holder_id, reveal_locations, current_content_version)?;
        let mut counts = BTreeMap::<String, u16>::new();
        let mut readings = BTreeMap::<String, BTreeSet<String>>::new();
        for summary in &all {
            *counts.entry(summary.phenomenon_id.clone()).or_default() += 1;
            readings
                .entry(summary.phenomenon_id.clone())
                .or_default()
                .insert(summary.reading.clone());
        }
        let disagreements = readings
            .into_iter()
            .filter(|(_, readings)| readings.len() > 1)
            .map(|(phenomenon, readings)| {
                format!("{phenomenon}: {} distinct field readings", readings.len())
            })
            .take(32)
            .collect();
        let omitted = all.len().saturating_sub(SURVEY_FOLIO_RECORDS) as u16;
        Ok(LibraryIndex {
            records: all.into_iter().take(SURVEY_FOLIO_RECORDS).collect(),
            phenomenon_counts: counts.into_iter().take(64).collect(),
            disagreements,
            omitted,
        })
    }

    pub fn mark_site_recovered(&mut self, key: impl Into<String>) -> bool {
        self.file.recovered_sites.insert(bounded(key.into(), 128))
    }

    pub fn site_was_recovered(&self, key: &str) -> bool {
        self.file.recovered_sites.contains(key)
    }

    pub fn mark_site_installed(&mut self, key: impl Into<String>) -> bool {
        self.file.installed_sites.insert(key.into())
    }

    pub fn site_was_installed(&self, key: &str) -> bool {
        self.file.installed_sites.contains(key)
    }

    pub fn audit(&self) -> DiscoveryAudit {
        let invalid_signatures = self
            .file
            .records
            .values()
            .filter(|record| !self.verify_record(record))
            .count();
        let dangling_references = self
            .file
            .objects
            .values()
            .flat_map(|object| &object.records)
            .filter(|record| !self.file.records.contains_key(record))
            .count();
        let mut evidence_counts = BTreeMap::new();
        for object in self.file.objects.values() {
            if let KnowledgeKind::Artifact { evidence_class, .. } = &object.kind {
                *evidence_counts.entry(evidence_class.clone()).or_default() += 1;
            } else if let KnowledgeKind::CalibrationPlate {
                evidence_class: Some(evidence_class),
                ..
            } = &object.kind
            {
                *evidence_counts.entry(evidence_class.clone()).or_default() += 1;
            }
        }
        DiscoveryAudit {
            objects: self.file.objects.len(),
            records: self.file.records.len(),
            invalid_signatures,
            dangling_references,
            evidence_counts,
            installed_sites: self.file.installed_sites.len(),
            recovered_sites: self.file.recovered_sites.len(),
            file_bytes: std::fs::metadata(&self.path).map_or(0, |metadata| metadata.len()),
        }
    }

    #[cfg(test)]
    pub fn corrupt_signature_for_test(&mut self, id: u64) {
        if let Some(record) = self.file.records.get_mut(&id) {
            record.signature[0] ^= 0xff;
        }
    }
}

pub fn audit_world(world: &Path) -> Result<DiscoveryAudit, DiscoveryError> {
    let path = world.join(STATE_FILE);
    let metadata = std::fs::metadata(&path)?;
    if metadata.len() > MAX_DISCOVERY_FILE_BYTES as u64 {
        return Err(DiscoveryError::Corrupt(
            "discovery file exceeds its hard budget".into(),
        ));
    }
    let text = std::fs::read_to_string(&path)?;
    let file: DiscoveryFile = toml::from_str(&text)
        .map_err(|error| DiscoveryError::Corrupt(format!("invalid discovery state: {error}")))?;
    if file.version != DISCOVERY_SCHEMA_VERSION {
        return Err(DiscoveryError::Corrupt(
            "unsupported discovery schema".into(),
        ));
    }
    let state = DiscoveryState { path, file };
    state.verify_all()?;
    Ok(state.audit())
}

pub fn sanitize_label(label: &str) -> Result<String, DiscoveryError> {
    if label.chars().any(|ch| ch.is_control()) {
        return Err(DiscoveryError::InvalidLabel);
    }
    let collapsed = label.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty()
        || collapsed.chars().count() > LABEL_MAX_CHARS
        || !collapsed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || " :_-'.,()/".contains(ch))
    {
        return Err(DiscoveryError::InvalidLabel);
    }
    Ok(collapsed)
}

fn bounded(mut value: String, max: usize) -> String {
    value.truncate(max);
    value
}

/// Stable, bounded wording for an original artifact. The chosen text is
/// stored on the physical object, so changing/removing a content pack later
/// never rewrites history.
pub fn artifact_phrase(instance_id: u64, evidence_class: &str, authored: &[String]) -> String {
    let fallback: &[&str] = match evidence_class {
        "maker_tablet" => &[
            "The hand is not the source. It only gives the Current a road.",
            "We agreed on the measure and disagreed on what the measure permitted.",
            "Nothing was made. The working borrowed, carried, and returned.",
        ],
        "calibration_plate" => &[
            "Set the slate at still water; distrust the first answer.",
            "Two marks agreed. The third wandered after the wellglass warmed.",
        ],
        "spent_charm_fitting" => &[
            "The fitting is sound. The charge left by the path we gave it.",
            "Replace the heart of the charm; keep the bronze cage.",
        ],
        "broken_focus" => &[
            "A cracked focus gives a confident lie. Retire it before it fouls the bench.",
            "The fracture followed the strongest resonance and spared the frame.",
        ],
        "sealed_dross_ampoule" => &[
            "Do not empty this into soil or running water. The seal is the mercy.",
            "Waste is still Current. Containment changes custody, not quantity.",
        ],
        "site_survey_marks" => &[
            "The south transect strengthened after rain; the ridge did not.",
            "Observed twice at dawn. Drift follows the valley, not the compass.",
        ],
        "failed_containment_fragment" => &[
            "The wall held the clean bands and passed the foul. We had built a sieve.",
            "Failure began where unlike plates touched. Distance bought one minute.",
        ],
        _ => &["Its maker left a measurement, but the convention is no longer known."],
    };
    let count = if authored.is_empty() {
        fallback.len()
    } else {
        authored.len()
    };
    let mut hash = instance_id ^ 0x6469_7363_6f76_6572;
    for byte in evidence_class.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    let index = (mix64(hash) as usize) % count.max(1);
    if authored.is_empty() {
        fallback[index].to_string()
    } else {
        bounded(authored[index].clone(), 240)
    }
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "wildforge-discovery-{name}-{}-{}",
            std::process::id(),
            mix64(name.bytes().map(u64::from).sum())
        ))
    }

    fn sample_observation() -> NewObservation {
        NewObservation {
            phenomenon_id: "base:echo_slate".into(),
            category: "mineral".into(),
            position: BlockPos::of_world(1, 70, 2),
            provenance: PlanetaryProvenance {
                face: "pos_z".into(),
                atlas_u: Some(1),
                atlas_v: Some(2),
                biome: "forest".into(),
                place: None,
            },
            day: 4,
            time_permille: 250,
            season: "spring".into(),
            calibration: CalibrationGrade::Field,
            reading: QualitativeReading {
                strength: StrengthBand::Steady,
                stability: StabilityBand::Stable,
                resonances: vec!["echo".into(), "stone".into()],
                dross: StrengthBand::Faint,
                drift: Some("north".into()),
                uncertainty: 22,
                properties: BTreeMap::new(),
            },
            observer: PlayerId([7; 16]),
            observer_name: "MOSS".into(),
            label: Some("ridge sample".into()),
        }
    }

    #[test]
    fn signed_records_copy_with_optional_location_and_survive_reload() {
        let root = root("copy");
        let _ = std::fs::remove_dir_all(&root);
        let mut state = DiscoveryState::load_or_initialize(&root, 41, 9).unwrap();
        let ledger = state
            .create_object("base:field_ledger", KnowledgeKind::FieldLedger, 0, None)
            .unwrap();
        let folio = state
            .create_object("base:survey_folio", KnowledgeKind::SurveyFolio, 0, None)
            .unwrap();
        let source = state
            .create_observation(ledger, sample_observation())
            .unwrap();
        let copy = state.copy_record(ledger, source, folio, false).unwrap();
        assert_eq!(
            state.copy_record(ledger, source, folio, false),
            Err(DiscoveryError::DuplicateRecord)
        );
        assert!(state.record(copy).unwrap().position.is_none());
        assert!(state.verify_record(state.record(copy).unwrap()));
        state.save().unwrap();
        let loaded = DiscoveryState::load_or_initialize(&root, 41, 10).unwrap();
        let summaries = loaded.summaries(folio, false, 10).unwrap();
        assert_eq!(summaries.len(), 1);
        assert!(summaries[0].obsolete_content);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn host_signature_rejects_forged_records() {
        let root = root("forge");
        let _ = std::fs::remove_dir_all(&root);
        let mut state = DiscoveryState::load_or_initialize(&root, 42, 1).unwrap();
        let ledger = state
            .create_object("base:field_ledger", KnowledgeKind::FieldLedger, 0, None)
            .unwrap();
        let id = state
            .create_observation(ledger, sample_observation())
            .unwrap();
        state.corrupt_signature_for_test(id);
        assert_eq!(state.save(), Err(DiscoveryError::ForgedRecord(id)));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn records_and_text_are_bounded_and_deterministic() {
        assert!(sanitize_label(&"x".repeat(LABEL_MAX_CHARS + 1)).is_err());
        let one = artifact_phrase(12, "maker_tablet", &[]);
        let two = artifact_phrase(12, "maker_tablet", &[]);
        assert_eq!(one, two);
        assert!(one.len() <= 240);
    }

    #[test]
    fn physical_record_capacities_and_indices_are_hard_bounded() {
        let root = root("capacity");
        let _ = std::fs::remove_dir_all(&root);
        let mut state = DiscoveryState::load_or_initialize(&root, 43, 1).unwrap();
        let ledger = state
            .create_object("base:field_ledger", KnowledgeKind::FieldLedger, 0, None)
            .unwrap();
        for day in 0..FIELD_LEDGER_RECORDS {
            let mut observation = sample_observation();
            observation.day = day as u32;
            observation.reading.uncertainty = (day % 100) as u8;
            state.create_observation(ledger, observation).unwrap();
        }
        assert_eq!(
            state.create_observation(ledger, sample_observation()),
            Err(DiscoveryError::Full("field record holder"))
        );
        let index = state.library_index(ledger, true, 1).unwrap();
        assert_eq!(index.records.len(), FIELD_LEDGER_RECORDS);
        assert!(index.phenomenon_counts.iter().any(|(phenomenon, count)| {
            phenomenon == "base:echo_slate" && usize::from(*count) == FIELD_LEDGER_RECORDS
        }));
        assert!(!index.disagreements.is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn knowledge_ids_round_trip_through_toml_and_stay_out_of_arcane_range() {
        let root = root("toml-ids");
        let _ = std::fs::remove_dir_all(&root);
        let mut state = DiscoveryState::load_or_initialize(&root, 44, 1).unwrap();
        let id = state
            .create_object("base:field_ledger", KnowledgeKind::FieldLedger, 0, None)
            .unwrap();
        assert!(id >= KNOWLEDGE_ID_BIT);
        assert!(id <= i64::MAX as u64);
        state.save().unwrap();
        let loaded = DiscoveryState::load_or_initialize(&root, 44, 1).unwrap();
        assert_eq!(loaded.object(id).unwrap().id, id);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn two_observers_share_without_collapsing_identity_or_disagreement() {
        let root = root("two-observers");
        let _ = std::fs::remove_dir_all(&root);
        let mut state = DiscoveryState::load_or_initialize(&root, 45, 1).unwrap();
        let first = state
            .create_object("base:field_ledger", KnowledgeKind::FieldLedger, 0, None)
            .unwrap();
        let second = state
            .create_object("base:field_ledger", KnowledgeKind::FieldLedger, 0, None)
            .unwrap();
        let folio = state
            .create_object("base:survey_folio", KnowledgeKind::SurveyFolio, 0, None)
            .unwrap();
        let mut a = sample_observation();
        a.observer = PlayerId([1; 16]);
        a.observer_name = "FERN".into();
        let a_id = state.create_observation(first, a).unwrap();
        let mut b = sample_observation();
        b.observer = PlayerId([2; 16]);
        b.observer_name = "MOSS".into();
        b.reading.strength = StrengthBand::Strong;
        let b_id = state.create_observation(second, b).unwrap();
        state.copy_record(first, a_id, folio, false).unwrap();
        state.copy_record(second, b_id, folio, true).unwrap();

        let index = state.library_index(folio, true, 1).unwrap();
        assert_eq!(index.records.len(), 2);
        assert!(index.records.iter().any(|record| {
            record.observer == PlayerId([1; 16]) && record.observer_name == "FERN"
        }));
        assert!(index.records.iter().any(|record| {
            record.observer == PlayerId([2; 16]) && record.observer_name == "MOSS"
        }));
        assert!(index.records.iter().any(|record| record.location.is_none()));
        assert!(index.records.iter().any(|record| record.location.is_some()));
        assert!(!index.disagreements.is_empty());
        let _ = std::fs::remove_dir_all(root);
    }
}
