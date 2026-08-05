//! Physical magical implements: declarative components, deterministic wand
//! resolution, and durable instance metadata.
//!
//! Current itself remains exclusively in [`crate::arcane::ArcaneLedger`].
//! This sidecar explains what each stable item identity physically is and how
//! quickly it may move that Current. Keeping those responsibilities separate
//! prevents a mod or UI path from inventing charge by editing item metadata.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::registry::MaterialVector;

pub const IMPLEMENTS_SCHEMA_VERSION: u32 = 2;
pub const IMPLEMENT_RESOLVER_VERSION: u32 = 1;
pub const IMPLEMENTS_FILE: &str = "implements.wfi";
const IMPLEMENTS_BACKUP: &str = "implements.wfi.bak";
/// A finite planet may support a large implement economy, but the durable
/// sidecar must remain inside its 64 MiB checkpoint budget.
pub const MAX_IMPLEMENT_INSTANCES: usize = 100_000;
pub const MAX_IMPLEMENT_AUDIT: usize = 4_096;
pub const MAX_IMPLEMENT_FILE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_IMPLEMENT_DEFINITIONS: usize = 65_536;
pub const MAX_IMPLEMENT_ID_BYTES: usize = 96;
pub const MAX_IMPLEMENT_PROVENANCE_BYTES: usize = 192;
pub const MAX_IMPLEMENT_AUDIT_TEXT_BYTES: usize = 512;
pub const MAX_IMPLEMENT_RESONANCES: usize = 16;
pub const MAX_IMPLEMENT_COMPONENTS: usize = 16;
pub const MAX_IMPLEMENT_MATERIALS: usize = 64;
pub const MAX_IMPLEMENT_INSTANCE_BYTES: usize = 4_096;
pub const MAX_IMPLEMENT_PUBLIC_BYTES: usize = 1_024;
/// Implement transactions are deliberately local. This bounds combined
/// debit/credit map entries before they reach the more general arcane API.
pub const MAX_IMPLEMENT_TRANSFER_OWNERS: usize = 8;

pub const MAX_COMPONENT_CAPACITY: u64 = 8_192;
pub const MAX_WAND_CAPACITY: u64 = 16_384;
pub const MIN_SAFE_TRANSFER: u64 = 1;
pub const MAX_SAFE_TRANSFER: u64 = 1_024;
pub const STRUCTURAL_SPARK_UNITS: u64 = 1;
pub const MAX_WAND_WEAR: u32 = 1_000;
pub const MAX_WAND_STRAIN: u32 = 10_000;
pub const MAX_CONDUCTOR_NETWORK: usize = 64;
pub const VESSEL_CAPACITY: u64 = 4_096;
pub const VESSEL_SAFE_TRANSFER: u64 = 256;
/// Existing shipped charm caps. Keeping these beside the authoritative charm
/// accounting prevents local and hosted survival paths from drifting apart.
pub const QUIET_CHARM_AGGRO_REDUCTION: f32 = 2.0;
pub const BARK_CHARM_ARMOR_POINTS: u32 = 1;
pub const HUNGER_CHARM_INTERVAL_SECS: f32 = 5.0;
pub const HUNGER_CHARM_MULTIPLIER: f32 = 0.85;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum FrameAction {
    /// Host chooses the obvious physical verb from authoritative mounts and
    /// held inventory. This is the ordinary right-click path.
    Contextual,
    ExchangeSelected,
    Assemble,
    BindCharm,
    Transfer,
    SafeDischarge,
    Disassemble,
    SwapFocus,
    Repair,
    Calibrate,
    Inspect,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FrameLayout {
    pub valid: bool,
    pub focus_mounts: u8,
    pub vessels: u8,
    pub conductor_endpoints: u8,
    pub containment: u16,
    pub network_size: u8,
    pub touches_unloaded: bool,
    pub problems: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FrameResult {
    pub success: bool,
    pub revision: u64,
    pub cue: ImplementCue,
    pub message: String,
    pub preview: Option<ResolvedWand>,
    pub lines: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ImplementCue {
    Use,
    Transfer,
    Strain,
    Empty,
    Failure,
}

pub fn error_cue(message: &str) -> ImplementCue {
    let lower = message.to_ascii_lowercase();
    if [
        "dormant",
        "not enough",
        "no usable",
        "already full",
        "visibly full",
        "depleted",
        "no measurable",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        ImplementCue::Empty
    } else {
        ImplementCue::Failure
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentRole {
    Body,
    Reservoir,
    Focus,
    Binding,
}

impl ComponentRole {
    pub const ALL: [Self; 4] = [Self::Body, Self::Reservoir, Self::Focus, Self::Binding];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::Reservoir => "reservoir",
            Self::Focus => "focus",
            Self::Binding => "binding",
        }
    }
}

/// A content-defined physical wand part. All values are integers so base and
/// mod content resolve bit-for-bit identically on every host platform.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WandComponentDef {
    pub role: ComponentRole,
    #[serde(default)]
    pub capacity: u64,
    pub conductivity: u16,
    pub stability: u16,
    #[serde(default)]
    pub resonance: BTreeMap<String, u16>,
    pub repair_material: String,
    #[serde(default)]
    pub heat_sensitive: bool,
    #[serde(default)]
    pub saturation_instability: u16,
    #[serde(default)]
    pub containment: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CharmEffect {
    Quiet,
    Bark,
    Hunger,
}

impl CharmEffect {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Quiet => "quiet",
            Self::Bark => "bark",
            Self::Hunger => "hunger",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CharmDef {
    pub effect: CharmEffect,
    pub charge_per_trigger: u64,
    pub capacity: u64,
    #[serde(default = "default_charm_stability")]
    pub stability: u16,
    #[serde(default = "default_charm_dross")]
    pub dross_per_transfer: u16,
}

const fn default_charm_stability() -> u16 {
    800
}

const fn default_charm_dross() -> u16 {
    25
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImplementItemKind {
    Wand,
    ChargeVessel,
    Fragment,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImplementItemDef {
    pub kind: ImplementItemKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WandParts {
    pub body: String,
    pub reservoir: String,
    pub focus: String,
    pub binding: String,
}

impl WandParts {
    pub fn ids(&self) -> [&str; 4] {
        [&self.body, &self.reservoir, &self.focus, &self.binding]
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedWand {
    pub resolver_version: u32,
    pub capacity: u64,
    pub safe_transfer: u64,
    pub stability: u16,
    pub dross_per_thousand: u16,
    pub resonance: BTreeMap<String, u16>,
    pub heat_sensitive: bool,
    pub saturation_instability: u16,
    pub containment: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ImplementKind {
    Wand {
        parts: WandParts,
        resolved: ResolvedWand,
    },
    Charm {
        effect: CharmEffect,
        capacity: u64,
        charge_per_trigger: u64,
        stability: u16,
        dross_per_transfer: u16,
    },
    Vessel {
        capacity: u64,
        safe_transfer: u64,
        containment: u16,
    },
    /// A physically conserved bundle left by catastrophic failure or by safe
    /// disassembly when a component's content pack is unavailable. The
    /// original construction remains in `ImplementInstance::construction`;
    /// mounting the bundle in a frame can therefore recover every component
    /// that still has a compatible registered item definition.
    Fragments {
        source: Box<ImplementKind>,
        pieces: u8,
    },
}

impl ImplementKind {
    pub fn capacity(&self) -> u64 {
        match self {
            Self::Wand { resolved, .. } => resolved.capacity,
            Self::Charm { capacity, .. } | Self::Vessel { capacity, .. } => *capacity,
            Self::Fragments { .. } => 0,
        }
    }
}

/// The exact tracked matter contributed by one physical construction part.
/// Keeping this beside the stable implement identity is what lets a variable
/// wand survive mod removal and catastrophic fragmentation without pretending
/// that every possible wand has one static registry material vector.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImplementComponent {
    pub content_id: String,
    pub materials: MaterialVector,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImplementInstance {
    pub instance_id: u64,
    pub content_id: String,
    pub kind: ImplementKind,
    pub construction: Vec<ImplementComponent>,
    pub wear: u32,
    pub strain: u32,
    pub provenance: String,
    pub created_day: u32,
    pub format_version: u32,
    pub creative: bool,
}

/// Bounded, interest-managed state a client may know for an implement it owns
/// or can legitimately inspect. Provenance and exact resonance quantities
/// remain host-side; component ids and resolved physical behavior are needed
/// for truthful tooltips and held models.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImplementPublicState {
    pub instance_id: u64,
    pub kind: ImplementKind,
    pub wear: u32,
    pub strain: u32,
    pub dross: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImplementVisual {
    /// Host registry item ids; guests remap them through the Welcome item map.
    pub body: u16,
    pub reservoir: u16,
    pub focus: u16,
    pub binding: u16,
    /// 1 slate/reading, 2 choirstone/movement, 3 iron/change, 4 living/binding.
    pub focus_shape: u8,
    /// Authoritative qualitative usable-charge band, 0..=3.
    pub charge_band: u8,
}

/// Qualitative state for visible embodied apparatus. This contains no exact
/// Current amount or provenance, so a nearby guest can render truthful light
/// and warning cues without receiving private ledger data.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApparatusCue {
    pub pos: crate::planet::BlockPos,
    /// 0 dormant, 1 faint, 2 steady, 3 saturated.
    pub charge_band: u8,
    /// 0 sound, 1 worn, 2 strained, 3 critical.
    pub strain_band: u8,
}

impl ImplementPublicState {
    pub fn from_authority(instance: &ImplementInstance, dross: u64) -> Self {
        Self {
            instance_id: instance.instance_id,
            kind: instance.kind.clone(),
            wear: instance.wear,
            strain: instance.strain,
            dross,
        }
    }

    pub fn tooltip(&self, ledger_total: u64, exact: bool) -> Vec<String> {
        let transient = ImplementInstance {
            instance_id: self.instance_id,
            content_id: String::new(),
            kind: self.kind.clone(),
            construction: Vec::new(),
            wear: self.wear,
            strain: self.strain,
            provenance: String::new(),
            created_day: 0,
            format_version: IMPLEMENT_RESOLVER_VERSION,
            creative: false,
        };
        tooltip(&transient, ledger_total, self.dross, exact)
    }
}

impl ImplementInstance {
    pub fn usable_capacity(&self) -> u64 {
        self.kind.capacity()
    }

    pub fn condition_band(&self) -> &'static str {
        let burden = u64::from(self.wear).saturating_mul(10)
            + u64::from(self.strain).min(u64::from(MAX_WAND_STRAIN));
        if burden == 0 {
            "sound"
        } else if burden < 2_500 {
            "worn"
        } else if burden < 7_500 {
            "strained"
        } else {
            "critical"
        }
    }

    pub fn tracked_materials(&self) -> Result<MaterialVector, ImplementError> {
        let mut total = MaterialVector::new();
        for component in &self.construction {
            for (material, units) in &component.materials {
                let value = total.entry(material.clone()).or_default();
                *value = value.checked_add(*units).ok_or_else(|| {
                    ImplementError::Corrupt(format!(
                        "implement {} tracked material total overflowed",
                        self.instance_id
                    ))
                })?;
            }
        }
        Ok(total)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImplementAuditEvent {
    pub operation_id: u64,
    pub kind: String,
    pub instance_id: u64,
    pub units: u64,
    pub dross: u64,
    pub actor: String,
    pub note: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImplementsState {
    pub schema_version: u32,
    pub resolver_version: u32,
    pub content_hash: u64,
    pub next_operation_id: u64,
    pub instances: BTreeMap<u64, ImplementInstance>,
    pub component_manifests: BTreeMap<String, WandComponentDef>,
    pub charm_manifests: BTreeMap<String, CharmDef>,
    pub migrated_legacy: BTreeSet<String>,
    pub audit: VecDeque<ImplementAuditEvent>,
    #[serde(skip)]
    path: PathBuf,
}

#[derive(Debug)]
pub enum ImplementError {
    InvalidContent(String),
    InvalidOperation(String),
    Corrupt(String),
    Io(std::io::Error),
}

impl std::fmt::Display for ImplementError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidContent(message)
            | Self::InvalidOperation(message)
            | Self::Corrupt(message) => f.write_str(message),
            Self::Io(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ImplementError {}

impl From<std::io::Error> for ImplementError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl ImplementsState {
    pub fn load_or_initialize(world: &Path, content_hash: u64) -> Result<Self, ImplementError> {
        let path = world.join(IMPLEMENTS_FILE);
        if !path.exists() {
            return Ok(Self {
                schema_version: IMPLEMENTS_SCHEMA_VERSION,
                resolver_version: IMPLEMENT_RESOLVER_VERSION,
                content_hash,
                next_operation_id: 1,
                instances: BTreeMap::new(),
                component_manifests: BTreeMap::new(),
                charm_manifests: BTreeMap::new(),
                migrated_legacy: BTreeSet::new(),
                audit: VecDeque::new(),
                path,
            });
        }
        let metadata = std::fs::metadata(&path)?;
        if metadata.len() > MAX_IMPLEMENT_FILE_BYTES {
            return Err(ImplementError::Corrupt(format!(
                "implements sidecar is {} bytes; limit is {MAX_IMPLEMENT_FILE_BYTES}",
                metadata.len()
            )));
        }
        let bytes = std::fs::read(&path)?;
        let mut state: Self = postcard::from_bytes(&bytes)
            .map_err(|error| ImplementError::Corrupt(error.to_string()))?;
        state.path = path;
        state.validate()?;
        Ok(state)
    }

    pub fn validate(&self) -> Result<(), ImplementError> {
        if self.schema_version != IMPLEMENTS_SCHEMA_VERSION
            || self.resolver_version != IMPLEMENT_RESOLVER_VERSION
        {
            return Err(ImplementError::Corrupt(format!(
                "unsupported implements schema {}/{}",
                self.schema_version, self.resolver_version
            )));
        }
        if self.instances.len() > MAX_IMPLEMENT_INSTANCES || self.audit.len() > MAX_IMPLEMENT_AUDIT
        {
            return Err(ImplementError::Corrupt(
                "implements state exceeds its bounded census".into(),
            ));
        }
        if self.component_manifests.len() > MAX_IMPLEMENT_DEFINITIONS
            || self.charm_manifests.len() > MAX_IMPLEMENT_DEFINITIONS
            || self.migrated_legacy.len() > MAX_IMPLEMENT_INSTANCES
        {
            return Err(ImplementError::Corrupt(
                "implements manifests exceed their bounded census".into(),
            ));
        }
        for (id, definition) in &self.component_manifests {
            validate_component(id, definition)
                .map_err(|error| ImplementError::Corrupt(error.to_string()))?;
        }
        for (id, definition) in &self.charm_manifests {
            validate_charm(id, definition)
                .map_err(|error| ImplementError::Corrupt(error.to_string()))?;
        }
        if self
            .migrated_legacy
            .iter()
            .any(|key| !bounded_text(key, MAX_IMPLEMENT_AUDIT_TEXT_BYTES))
        {
            return Err(ImplementError::Corrupt(
                "legacy implement migration key exceeds its metadata budget".into(),
            ));
        }
        for event in &self.audit {
            if !bounded_text(&event.kind, MAX_IMPLEMENT_ID_BYTES)
                || !bounded_text(&event.actor, MAX_IMPLEMENT_PROVENANCE_BYTES)
                || !bounded_text(&event.note, MAX_IMPLEMENT_AUDIT_TEXT_BYTES)
            {
                return Err(ImplementError::Corrupt(format!(
                    "implement audit event {} exceeds its metadata budget",
                    event.operation_id
                )));
            }
        }
        for (id, instance) in &self.instances {
            if *id == 0
                || *id != instance.instance_id
                || !bounded_text(&instance.content_id, MAX_IMPLEMENT_ID_BYTES)
                || !bounded_text(&instance.provenance, MAX_IMPLEMENT_PROVENANCE_BYTES)
                || instance.format_version != IMPLEMENT_RESOLVER_VERSION
            {
                return Err(ImplementError::Corrupt(
                    "implement instance identity/content is invalid".into(),
                ));
            }
            if instance.wear > MAX_WAND_WEAR || instance.strain > MAX_WAND_STRAIN {
                return Err(ImplementError::Corrupt(format!(
                    "implement {id} exceeds wear/strain bounds"
                )));
            }
            if instance.construction.len() > MAX_IMPLEMENT_COMPONENTS {
                return Err(ImplementError::Corrupt(format!(
                    "implement {id} exceeds its physical component budget"
                )));
            }
            for component in &instance.construction {
                if !bounded_text(&component.content_id, MAX_IMPLEMENT_ID_BYTES) {
                    return Err(ImplementError::Corrupt(format!(
                        "implement {id} has an invalid physical component id"
                    )));
                }
                validate_material_vector(&component.materials).map_err(|error| {
                    ImplementError::Corrupt(format!(
                        "implement {id} has invalid tracked matter: {error}"
                    ))
                })?;
            }
            let tracked = instance.tracked_materials()?;
            validate_material_vector(&tracked).map_err(|error| {
                ImplementError::Corrupt(format!(
                    "implement {id} aggregate tracked matter is invalid: {error}"
                ))
            })?;
            validate_kind_bounds(*id, &instance.kind, true)?;
            match &instance.kind {
                ImplementKind::Wand { parts, .. } => {
                    let mut expected = parts
                        .ids()
                        .into_iter()
                        .map(str::to_string)
                        .collect::<Vec<_>>();
                    let mut actual = instance
                        .construction
                        .iter()
                        .map(|component| component.content_id.clone())
                        .collect::<Vec<_>>();
                    expected.sort();
                    actual.sort();
                    if expected != actual {
                        return Err(ImplementError::Corrupt(format!(
                            "wand {id} construction does not match its resolved parts"
                        )));
                    }
                }
                ImplementKind::Charm { .. } => {
                    if instance.construction.is_empty() {
                        return Err(ImplementError::Corrupt(format!(
                            "charm {id} has no physical construction"
                        )));
                    }
                }
                ImplementKind::Vessel { .. } => {
                    if instance.construction.len() != 1 {
                        return Err(ImplementError::Corrupt(format!(
                            "vessel {id} does not name exactly one physical shell"
                        )));
                    }
                }
                ImplementKind::Fragments { pieces, .. } => {
                    if instance.construction.is_empty()
                        || usize::from(*pieces) != instance.construction.len()
                        || instance.content_id != "base:implement_fragment"
                    {
                        return Err(ImplementError::Corrupt(format!(
                            "fragment bundle {id} does not match its conserved construction"
                        )));
                    }
                }
            }
            let instance_bytes = postcard::to_allocvec(instance)
                .map_err(|error| ImplementError::Corrupt(error.to_string()))?;
            if instance_bytes.len() > MAX_IMPLEMENT_INSTANCE_BYTES {
                return Err(ImplementError::Corrupt(format!(
                    "implement {id} metadata is {} bytes; limit is {MAX_IMPLEMENT_INSTANCE_BYTES}",
                    instance_bytes.len()
                )));
            }
            let public = ImplementPublicState::from_authority(instance, u64::MAX);
            let public_bytes = postcard::to_allocvec(&public)
                .map_err(|error| ImplementError::Corrupt(error.to_string()))?;
            if public_bytes.len() > MAX_IMPLEMENT_PUBLIC_BYTES {
                return Err(ImplementError::Corrupt(format!(
                    "implement {id} public metadata is {} bytes; limit is {MAX_IMPLEMENT_PUBLIC_BYTES}",
                    public_bytes.len()
                )));
            }
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, ImplementError> {
        self.validate()?;
        let bytes = postcard::to_allocvec(self)
            .map_err(|error| ImplementError::Corrupt(error.to_string()))?;
        if bytes.len() as u64 > MAX_IMPLEMENT_FILE_BYTES {
            return Err(ImplementError::InvalidOperation(format!(
                "implements sidecar would be {} bytes; checkpoint budget is {MAX_IMPLEMENT_FILE_BYTES}",
                bytes.len()
            )));
        }
        Ok(bytes)
    }

    pub fn save(&self) -> Result<(), ImplementError> {
        let bytes = self.encode()?;
        if let Ok(old) = std::fs::read(&self.path) {
            crate::identity::atomic_write(
                &self.path.with_file_name(IMPLEMENTS_BACKUP),
                &old,
                false,
            )?;
        }
        crate::identity::atomic_write(&self.path, &bytes, false)?;
        Ok(())
    }

    pub fn operation_id(&mut self) -> Result<u64, ImplementError> {
        let id = self.next_operation_id;
        self.next_operation_id = self
            .next_operation_id
            .checked_add(1)
            .ok_or_else(|| ImplementError::Corrupt("implement operation id overflow".into()))?;
        Ok(id)
    }

    pub fn record(&mut self, event: ImplementAuditEvent) {
        self.audit.push_back(event);
        while self.audit.len() > MAX_IMPLEMENT_AUDIT {
            self.audit.pop_front();
        }
    }

    pub fn instance(&self, id: u64) -> Option<&ImplementInstance> {
        self.instances.get(&id)
    }

    pub fn insert(&mut self, instance: ImplementInstance) -> Result<(), ImplementError> {
        if instance.instance_id == 0 || self.instances.contains_key(&instance.instance_id) {
            return Err(ImplementError::InvalidOperation(
                "implement identity is zero or already exists".into(),
            ));
        }
        if self.instances.len() >= MAX_IMPLEMENT_INSTANCES {
            return Err(ImplementError::InvalidOperation(
                "world implement census is full".into(),
            ));
        }
        self.instances.insert(instance.instance_id, instance);
        self.validate()
    }

    /// Remove construction records whose finite Current custody was rolled
    /// back by the parent ledger's durable-owner crash recovery. Arcane
    /// commits intentionally land before sparse player/entity files; if a
    /// process stops between them, the ledger removes an unmaterialized item
    /// account on reopen and this matching pass removes its now-ghost sidecar
    /// record. It never reconstructs or invents charge.
    pub fn reconcile_ledger(
        &mut self,
        ledger: &crate::arcane::ArcaneLedger,
    ) -> Result<usize, ImplementError> {
        let orphaned = self
            .instances
            .keys()
            .copied()
            .filter(|id| {
                ledger
                    .account(&crate::arcane::ArcaneOwner::Item(*id))
                    .is_none()
                    && ledger
                        .account(&crate::arcane::ArcaneOwner::ItemDross(*id))
                        .is_none()
            })
            .collect::<Vec<_>>();
        if orphaned.is_empty() {
            return Ok(0);
        }
        for id in &orphaned {
            self.instances.remove(id);
        }
        let operation_id = self.operation_id()?;
        self.record(ImplementAuditEvent {
            operation_id,
            kind: "crash_recovery".into(),
            instance_id: 0,
            units: 0,
            dross: 0,
            actor: "system".into(),
            note: format!(
                "removed {} construction record(s) whose unpersisted physical owner was rolled back by the Current ledger",
                orphaned.len()
            ),
        });
        self.validate()?;
        Ok(orphaned.len())
    }
}

/// Closed-world operator evidence for implement metadata and its parent
/// Current custody. This is intentionally independent of rendering/content
/// packs: saved component manifests remain authoritative after mod removal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImplementsAudit {
    pub schema_version: u32,
    pub resolver_version: u32,
    pub content_hash: u64,
    pub file_bytes: u64,
    pub instances: usize,
    pub wands: usize,
    pub charms: usize,
    pub vessels: usize,
    pub fragment_bundles: usize,
    pub creative_marked: usize,
    pub clean_current: u64,
    pub retained_dross: u64,
    pub tracked_material_units: u64,
    pub missing_accounts: usize,
    pub content_mismatches: usize,
    pub over_capacity: usize,
    pub max_instance_bytes: usize,
    pub max_public_bytes: usize,
    pub audit_events: usize,
    pub arcane_balanced: bool,
    pub material_balanced: bool,
}

impl ImplementsAudit {
    pub fn is_qualified(&self) -> bool {
        self.schema_version == IMPLEMENTS_SCHEMA_VERSION
            && self.resolver_version == IMPLEMENT_RESOLVER_VERSION
            && self.file_bytes <= MAX_IMPLEMENT_FILE_BYTES
            && self.instances <= MAX_IMPLEMENT_INSTANCES
            && self.missing_accounts == 0
            && self.content_mismatches == 0
            && self.over_capacity == 0
            && self.max_instance_bytes <= MAX_IMPLEMENT_INSTANCE_BYTES
            && self.max_public_bytes <= MAX_IMPLEMENT_PUBLIC_BYTES
            && self.audit_events <= MAX_IMPLEMENT_AUDIT
            && self.arcane_balanced
            && self.material_balanced
    }

    pub fn render(&self) -> String {
        format!(
            concat!(
                "Implements audit schema {} resolver {}\n",
                "Content hash: {:016x}\n",
                "Sidecar: {} / {} bytes\n",
                "Instances: {} ({} wands, {} charms, {} vessels, {} fragment bundles; {} creative-marked)\n",
                "Custody: {} clean Current, {} retained dross\n",
                "Tracked construction matter: {} material units\n",
                "Integrity: {} missing accounts, {} content mismatches, {} over capacity\n",
                "Metadata: max instance {} / {} bytes, max public {} / {} bytes\n",
                "Audit events: {} / {}\n",
                "Parent arcane audit balanced: {}\n",
                "Parent material audit balanced: {}\n",
                "Qualified: {}\n"
            ),
            self.schema_version,
            self.resolver_version,
            self.content_hash,
            self.file_bytes,
            MAX_IMPLEMENT_FILE_BYTES,
            self.instances,
            self.wands,
            self.charms,
            self.vessels,
            self.fragment_bundles,
            self.creative_marked,
            self.clean_current,
            self.retained_dross,
            self.tracked_material_units,
            self.missing_accounts,
            self.content_mismatches,
            self.over_capacity,
            self.max_instance_bytes,
            MAX_IMPLEMENT_INSTANCE_BYTES,
            self.max_public_bytes,
            MAX_IMPLEMENT_PUBLIC_BYTES,
            self.audit_events,
            MAX_IMPLEMENT_AUDIT,
            self.arcane_balanced,
            self.material_balanced,
            self.is_qualified(),
        )
    }
}

pub fn audit_world(world: &Path) -> Result<ImplementsAudit, ImplementError> {
    let state = ImplementsState::load_or_initialize(world, 0)?;
    state.validate()?;
    let ledger = crate::arcane::ArcaneLedger::load(world)
        .map_err(|error| ImplementError::Corrupt(error.to_string()))?;
    let ledger_audit = ledger
        .audit()
        .map_err(|error| ImplementError::Corrupt(error.to_string()))?;
    let durable = ledger
        .durable_item_status(world)
        .map_err(|error| ImplementError::Corrupt(error.to_string()))?;
    let arcane_balanced = ledger_audit.unexplained_delta == 0
        && durable.orphan_accounts == 0
        && durable.invalid_references == 0
        && durable.duplicate_references == 0;
    let material_balanced = crate::materials::MaterialLedger::load(world)
        .map(|ledger| ledger.audit().is_balanced())
        .map_err(ImplementError::Io)?;
    let file_bytes = std::fs::metadata(world.join(IMPLEMENTS_FILE))
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let mut audit = ImplementsAudit {
        schema_version: state.schema_version,
        resolver_version: state.resolver_version,
        content_hash: state.content_hash,
        file_bytes,
        instances: state.instances.len(),
        wands: 0,
        charms: 0,
        vessels: 0,
        fragment_bundles: 0,
        creative_marked: 0,
        clean_current: 0,
        retained_dross: 0,
        tracked_material_units: 0,
        missing_accounts: 0,
        content_mismatches: 0,
        over_capacity: 0,
        max_instance_bytes: 0,
        max_public_bytes: 0,
        audit_events: state.audit.len(),
        arcane_balanced,
        material_balanced,
    };
    for instance in state.instances.values() {
        match instance.kind {
            ImplementKind::Wand { .. } => audit.wands += 1,
            ImplementKind::Charm { .. } => audit.charms += 1,
            ImplementKind::Vessel { .. } => audit.vessels += 1,
            ImplementKind::Fragments { .. } => audit.fragment_bundles += 1,
        }
        audit.creative_marked += usize::from(instance.creative);
        audit.tracked_material_units = audit
            .tracked_material_units
            .checked_add(
                instance
                    .tracked_materials()?
                    .values()
                    .try_fold(0u64, |sum, units| sum.checked_add(*units))
                    .ok_or_else(|| {
                        ImplementError::Corrupt(
                            "tracked implement material audit total overflowed".into(),
                        )
                    })?,
            )
            .ok_or_else(|| {
                ImplementError::Corrupt("tracked implement material census overflowed".into())
            })?;
        let clean = ledger
            .account(&crate::arcane::ArcaneOwner::Item(instance.instance_id))
            .map(|account| account.current.total());
        let dross = ledger
            .account(&crate::arcane::ArcaneOwner::ItemDross(instance.instance_id))
            .map_or(0, |account| account.current.total());
        let Some(clean) = clean else {
            audit.missing_accounts += 1;
            continue;
        };
        audit.clean_current = audit.clean_current.saturating_add(clean);
        audit.retained_dross = audit.retained_dross.saturating_add(dross);
        if !ledger.item_matches(instance.instance_id, &instance.content_id) {
            audit.content_mismatches += 1;
        }
        if usable_charge(clean) > instance.usable_capacity() {
            audit.over_capacity += 1;
        }
        let instance_bytes = postcard::to_allocvec(instance)
            .map_err(|error| ImplementError::Corrupt(error.to_string()))?
            .len();
        audit.max_instance_bytes = audit.max_instance_bytes.max(instance_bytes);
        let public_bytes =
            postcard::to_allocvec(&ImplementPublicState::from_authority(instance, dross))
                .map_err(|error| ImplementError::Corrupt(error.to_string()))?
                .len();
        audit.max_public_bytes = audit.max_public_bytes.max(public_bytes);
    }
    Ok(audit)
}

fn validate_material_vector(materials: &MaterialVector) -> Result<(), ImplementError> {
    if materials.len() > MAX_IMPLEMENT_MATERIALS
        || materials
            .iter()
            .any(|(name, units)| !bounded_text(name, MAX_IMPLEMENT_ID_BYTES) || *units == 0)
    {
        return Err(ImplementError::InvalidContent(
            "tracked implement matter exceeds its bounded material schema".into(),
        ));
    }
    Ok(())
}

fn validate_kind_bounds(
    id: u64,
    kind: &ImplementKind,
    allow_fragments: bool,
) -> Result<(), ImplementError> {
    match kind {
        ImplementKind::Wand { parts, resolved } => {
            if parts
                .ids()
                .iter()
                .any(|part| !bounded_text(part, MAX_IMPLEMENT_ID_BYTES))
                || resolved.resolver_version != IMPLEMENT_RESOLVER_VERSION
                || resolved.capacity > MAX_WAND_CAPACITY
                || !(MIN_SAFE_TRANSFER..=MAX_SAFE_TRANSFER).contains(&resolved.safe_transfer)
                || resolved.stability > 1_000
                || resolved.dross_per_thousand == 0
                || resolved.dross_per_thousand > 500
                || resolved.resonance.is_empty()
                || resolved.resonance.len() > MAX_IMPLEMENT_RESONANCES
                || resolved.resonance.iter().any(|(name, amount)| {
                    !bounded_text(name, MAX_IMPLEMENT_ID_BYTES) || *amount == 0
                })
            {
                return Err(ImplementError::Corrupt(format!(
                    "wand {id} has invalid stored resolution"
                )));
            }
        }
        ImplementKind::Charm {
            capacity,
            charge_per_trigger,
            stability,
            dross_per_transfer,
            ..
        } => validate_charm_numbers(
            *capacity,
            *charge_per_trigger,
            *stability,
            *dross_per_transfer,
        )?,
        ImplementKind::Vessel {
            capacity,
            safe_transfer,
            containment,
        } => {
            if *capacity == 0
                || *capacity > MAX_WAND_CAPACITY
                || !(MIN_SAFE_TRANSFER..=MAX_SAFE_TRANSFER).contains(safe_transfer)
                || *containment > 1_000
            {
                return Err(ImplementError::Corrupt(format!(
                    "vessel {id} has invalid bounds"
                )));
            }
        }
        ImplementKind::Fragments { source, pieces } => {
            if !allow_fragments
                || *pieces == 0
                || usize::from(*pieces) > MAX_IMPLEMENT_COMPONENTS
                || matches!(source.as_ref(), ImplementKind::Fragments { .. })
            {
                return Err(ImplementError::Corrupt(format!(
                    "fragment bundle {id} has invalid or recursive provenance"
                )));
            }
            validate_kind_bounds(id, source, false)?;
        }
    }
    Ok(())
}

pub fn validate_component(
    content_id: &str,
    definition: &WandComponentDef,
) -> Result<(), ImplementError> {
    if !bounded_text(content_id, MAX_IMPLEMENT_ID_BYTES)
        || definition.capacity > MAX_COMPONENT_CAPACITY
        || definition.conductivity == 0
        || definition.conductivity > 1_000
        || definition.stability > 1_000
        || definition.saturation_instability > 1_000
        || definition.containment > 1_000
        || !bounded_text(&definition.repair_material, MAX_IMPLEMENT_ID_BYTES)
        || definition.resonance.is_empty()
        || definition.resonance.len() > MAX_IMPLEMENT_RESONANCES
        || definition.resonance.iter().any(|(id, weight)| {
            !bounded_text(id, MAX_IMPLEMENT_ID_BYTES) || *weight == 0 || *weight > 1_000
        })
    {
        return Err(ImplementError::InvalidContent(format!(
            "{content_id}: wand_component fields are absent or outside bounded ranges"
        )));
    }
    if definition.role == ComponentRole::Focus && definition.capacity > 512 {
        return Err(ImplementError::InvalidContent(format!(
            "{content_id}: a focus cannot smuggle in a reservoir-sized capacity"
        )));
    }
    Ok(())
}

pub fn validate_charm(content_id: &str, definition: &CharmDef) -> Result<(), ImplementError> {
    if !bounded_text(content_id, MAX_IMPLEMENT_ID_BYTES) {
        return Err(ImplementError::InvalidContent(format!(
            "{content_id}: charm identity exceeds its metadata budget"
        )));
    }
    validate_charm_numbers(
        definition.capacity,
        definition.charge_per_trigger,
        definition.stability,
        definition.dross_per_transfer,
    )
    .map_err(|_| {
        ImplementError::InvalidContent(format!(
            "{content_id}: charm requires bounded capacity, debit, stability, and dross"
        ))
    })
}

fn bounded_text(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max_bytes && !value.chars().any(char::is_control)
}

fn validate_charm_numbers(
    capacity: u64,
    charge_per_trigger: u64,
    stability: u16,
    dross_per_transfer: u16,
) -> Result<(), ImplementError> {
    if capacity == 0
        || capacity > MAX_WAND_CAPACITY
        || charge_per_trigger == 0
        || charge_per_trigger > capacity
        || stability > 1_000
        || dross_per_transfer > 1_000
    {
        return Err(ImplementError::InvalidContent(
            "charm values are outside bounded ranges".into(),
        ));
    }
    Ok(())
}

/// Resolve one part of each physical role. The formula is deliberately
/// compact and documented in integer arithmetic:
///
/// - capacity = reservoir + half body + quarter binding (focus contributes no
///   hidden tank), clamped to 1..MAX_WAND_CAPACITY;
/// - throughput = harmonic mean of body/reservoir/binding conductivity, then
///   capped by focus conductivity;
/// - stability weights body 4, binding 3, reservoir 2, focus 1;
/// - transfer dross is the complement of stability plus saturation penalty,
///   never a zero-cost path;
/// - resonance is the role-weighted sum (focus 4, body 2, others 1), reduced
///   by the gcd so equivalent declarations compare identically.
pub fn resolve_wand(
    parts: &[(String, WandComponentDef); 4],
) -> Result<(WandParts, ResolvedWand), ImplementError> {
    let mut by_role = BTreeMap::new();
    for (id, definition) in parts {
        validate_component(id, definition)?;
        if by_role.insert(definition.role, (id, definition)).is_some() {
            return Err(ImplementError::InvalidOperation(format!(
                "wand assembly contains two {} components",
                definition.role.label()
            )));
        }
    }
    if by_role.len() != ComponentRole::ALL.len() {
        return Err(ImplementError::InvalidOperation(
            "wand assembly requires one body, reservoir, focus, and binding".into(),
        ));
    }
    let (body_id, body) = by_role[&ComponentRole::Body];
    let (reservoir_id, reservoir) = by_role[&ComponentRole::Reservoir];
    let (focus_id, focus) = by_role[&ComponentRole::Focus];
    let (binding_id, binding) = by_role[&ComponentRole::Binding];

    let capacity = reservoir
        .capacity
        .saturating_add(body.capacity / 2)
        .saturating_add(binding.capacity / 4)
        .clamp(1, MAX_WAND_CAPACITY);
    let path = [
        body.conductivity,
        reservoir.conductivity,
        binding.conductivity,
    ];
    let reciprocal_sum = path.into_iter().fold(0u64, |sum, value| {
        sum.saturating_add(1_000_000 / u64::from(value))
    });
    let harmonic = 3_000_000u64 / reciprocal_sum.max(1);
    let safe_transfer = harmonic
        .min(u64::from(focus.conductivity))
        .div_ceil(4)
        .clamp(MIN_SAFE_TRANSFER, MAX_SAFE_TRANSFER);
    let weighted_stability = (u64::from(body.stability) * 4
        + u64::from(binding.stability) * 3
        + u64::from(reservoir.stability) * 2
        + u64::from(focus.stability))
        / 10;
    let saturation_instability = body
        .saturation_instability
        .saturating_add(reservoir.saturation_instability)
        .saturating_add(focus.saturation_instability)
        .saturating_add(binding.saturation_instability)
        .min(1_000);
    let stability = weighted_stability
        .saturating_sub(u64::from(saturation_instability) / 5)
        .clamp(1, 1_000) as u16;
    let dross_per_thousand = (1_000u16.saturating_sub(stability) / 4)
        .saturating_add(saturation_instability / 10)
        .clamp(1, 500);

    let mut resonance = BTreeMap::<String, u16>::new();
    for (definition, weight) in [(body, 2u16), (reservoir, 1), (focus, 4), (binding, 1)] {
        for (id, amount) in &definition.resonance {
            let next = u32::from(resonance.get(id).copied().unwrap_or_default())
                .saturating_add(u32::from(*amount) * u32::from(weight))
                .min(u32::from(u16::MAX));
            resonance.insert(id.clone(), next as u16);
        }
    }
    let divisor = resonance.values().copied().reduce(gcd).unwrap_or(1).max(1);
    for amount in resonance.values_mut() {
        *amount /= divisor;
    }
    let containment = [body, reservoir, focus, binding]
        .into_iter()
        .fold(0u32, |sum, definition| {
            sum + u32::from(definition.containment)
        })
        .div_ceil(4)
        .min(1_000) as u16;

    Ok((
        WandParts {
            body: body_id.clone(),
            reservoir: reservoir_id.clone(),
            focus: focus_id.clone(),
            binding: binding_id.clone(),
        },
        ResolvedWand {
            resolver_version: IMPLEMENT_RESOLVER_VERSION,
            capacity,
            safe_transfer,
            stability,
            dross_per_thousand,
            resonance,
            heat_sensitive: body.heat_sensitive
                || reservoir.heat_sensitive
                || focus.heat_sensitive
                || binding.heat_sensitive,
            saturation_instability,
            containment,
        },
    ))
}

fn gcd(mut a: u16, mut b: u16) -> u16 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

pub fn usable_charge(total: u64) -> u64 {
    total.saturating_sub(STRUCTURAL_SPARK_UNITS)
}

pub fn qualitative_charge(total: u64, capacity: u64) -> &'static str {
    crate::arcane::qualitative_current(usable_charge(total), capacity)
}

pub fn charge_band(total: u64, capacity: u64) -> u8 {
    let usable = usable_charge(total);
    if usable == 0 {
        0
    } else if usable.saturating_mul(4) < capacity.max(1) {
        1
    } else if usable.saturating_mul(4) < capacity.max(1).saturating_mul(3) {
        2
    } else {
        3
    }
}

pub fn tooltip(
    instance: &ImplementInstance,
    ledger_total: u64,
    dross: u64,
    exact: bool,
) -> Vec<String> {
    let mut lines = vec![match &instance.kind {
        ImplementKind::Wand { parts, .. } => format!(
            "Bound wand — body {}, reservoir {}, focus {}, binding {}",
            short_id(&parts.body),
            short_id(&parts.reservoir),
            short_id(&parts.focus),
            short_id(&parts.binding)
        ),
        ImplementKind::Charm { effect, .. } => format!("Charm of {}", effect.id()),
        ImplementKind::Vessel { .. } => "Stationary charge vessel".into(),
        ImplementKind::Fragments { pieces, .. } => {
            format!("Conserved implement fragment bundle — {pieces} piece(s)")
        }
    }];
    lines.push(format!("Condition: {}", instance.condition_band()));
    lines.push(format!(
        "Charge: {}",
        qualitative_charge(ledger_total, instance.usable_capacity())
    ));
    if exact {
        lines.push(format!(
            "Lens: {} / {} usable Current; {} dross; wear {}; strain {}",
            usable_charge(ledger_total),
            instance.usable_capacity(),
            dross,
            instance.wear,
            instance.strain
        ));
        if let ImplementKind::Wand { resolved, .. } = &instance.kind {
            lines.push(format!(
                "Lens: transfer {}, stability {}, containment {} permille",
                resolved.safe_transfer, resolved.stability, resolved.containment
            ));
        }
    }
    lines
}

fn short_id(id: &str) -> &str {
    id.split_once(':').map_or(id, |(_, short)| short)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn component(
        role: ComponentRole,
        capacity: u64,
        conductivity: u16,
        stability: u16,
    ) -> WandComponentDef {
        WandComponentDef {
            role,
            capacity,
            conductivity,
            stability,
            resonance: BTreeMap::from([("base:root".into(), 1)]),
            repair_material: "base:stick".into(),
            heat_sensitive: false,
            saturation_instability: 0,
            containment: 0,
        }
    }

    #[test]
    fn resolver_is_role_order_independent_and_bounded() {
        let a = [
            ("body".into(), component(ComponentRole::Body, 200, 300, 900)),
            (
                "reservoir".into(),
                component(ComponentRole::Reservoir, 500, 800, 600),
            ),
            ("focus".into(), component(ComponentRole::Focus, 0, 700, 500)),
            (
                "binding".into(),
                component(ComponentRole::Binding, 100, 600, 800),
            ),
        ];
        let b = [a[3].clone(), a[1].clone(), a[0].clone(), a[2].clone()];
        let (_, first) = resolve_wand(&a).unwrap();
        let (_, second) = resolve_wand(&b).unwrap();
        assert_eq!(first, second);
        assert!((1..=MAX_WAND_CAPACITY).contains(&first.capacity));
        assert!((MIN_SAFE_TRANSFER..=MAX_SAFE_TRANSFER).contains(&first.safe_transfer));
        assert!((1..=1_000).contains(&first.stability));
        assert!((1..=500).contains(&first.dross_per_thousand));
    }

    #[test]
    fn invalid_components_cannot_create_free_or_unbounded_paths() {
        let mut bad = component(ComponentRole::Body, MAX_COMPONENT_CAPACITY + 1, 0, 1_001);
        bad.resonance.clear();
        assert!(validate_component("mod:bad", &bad).is_err());
        let bad_charm = CharmDef {
            effect: CharmEffect::Bark,
            charge_per_trigger: 0,
            capacity: u64::MAX,
            stability: 1_001,
            dross_per_transfer: 1_001,
        };
        assert!(validate_charm("mod:bad_charm", &bad_charm).is_err());
    }

    #[test]
    fn structural_spark_keeps_depleted_identity_without_hidden_benefit() {
        assert_eq!(usable_charge(STRUCTURAL_SPARK_UNITS), 0);
        assert_eq!(qualitative_charge(STRUCTURAL_SPARK_UNITS, 120), "dormant");
    }
}
