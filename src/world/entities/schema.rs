//! Existing versioned block-entity sidecar input schema.

use serde::Deserialize;
#[derive(Deserialize)]
pub(super) struct SlotT {
    pub(super) item: String,
    pub(super) count: u32,
    pub(super) durability: u32,
    #[serde(default)]
    pub(super) arcane_id: u64,
}
#[derive(Deserialize)]
pub(super) struct FurnaceT {
    pub(super) pos: crate::planet::BlockPos,
    pub(super) input: Option<SlotT>,
    pub(super) fuel: Option<SlotT>,
    pub(super) output: Option<SlotT>,
    #[serde(default)]
    pub(super) progress: f32,
    #[serde(default)]
    pub(super) burn_left: f32,
    #[serde(default)]
    pub(super) burn_total: f32,
    #[serde(default)]
    pub(super) burn_speed: f32,
}
#[derive(Deserialize)]
pub(super) struct ChestSlotT {
    pub(super) index: usize,
    pub(super) item: String,
    pub(super) count: u32,
    pub(super) durability: u32,
    #[serde(default)]
    pub(super) arcane_id: u64,
}
#[derive(Deserialize)]
pub(super) struct ChestT {
    pub(super) pos: crate::planet::BlockPos,
    #[serde(default)]
    pub(super) wild_owned: bool,
    #[serde(default)]
    pub(super) slot: Vec<ChestSlotT>,
}
#[derive(Deserialize)]
pub(super) struct MachineT {
    pub(super) pos: crate::planet::BlockPos,
    #[serde(default)]
    pub(super) kind: String,
    #[serde(default)]
    pub(super) lit: bool,
    #[serde(default)]
    pub(super) progress: f32,
    #[serde(default)]
    pub(super) core: Option<crate::planet::BlockPos>,
    #[serde(default)]
    pub(super) slot: Vec<ChestSlotT>,
    #[serde(default)]
    pub(super) reclaim: Vec<MaterialT>,
    #[serde(default)]
    pub(super) powder: u32,
    #[serde(default)]
    pub(super) separator_fuel: u32,
    #[serde(default)]
    pub(super) neodymium: u32,
    #[serde(default)]
    pub(super) cerium: u32,
}
#[derive(Deserialize)]
pub(super) struct MaterialT {
    pub(super) material: String,
    pub(super) units: u64,
}
#[derive(Deserialize)]
pub(super) struct SignT {
    pub(super) pos: crate::planet::BlockPos,
    #[serde(default)]
    pub(super) lines: Vec<String>,
}
#[derive(Deserialize)]
pub(super) struct StallT {
    pub(super) pos: crate::planet::BlockPos,
    #[serde(default)]
    pub(super) owner: String,
    #[serde(default)]
    pub(super) owner_name: String,
    #[serde(default)]
    pub(super) slot: Vec<ChestSlotT>,
}
#[derive(Deserialize)]
pub(super) struct SmokerT {
    pub(super) pos: crate::planet::BlockPos,
    #[serde(default)]
    pub(super) progress: f32,
    #[serde(default)]
    pub(super) slot: Vec<ChestSlotT>,
}
#[derive(Deserialize)]
pub(super) struct ClampT {
    pub(super) pos: crate::planet::BlockPos,
    pub(super) timer: f32,
    #[serde(default)]
    pub(super) logs: Vec<crate::planet::BlockPos>,
}
#[derive(Deserialize)]
pub(super) struct AnvilT {
    pub(super) pos: crate::planet::BlockPos,
    #[serde(default)]
    pub(super) strikes: u32,
    #[serde(default)]
    pub(super) bloom: Option<SlotT>,
}
#[derive(Deserialize)]
pub(super) struct SteamT {
    pub(super) pos: crate::planet::BlockPos,
    #[serde(default)]
    pub(super) fuel: f32,
    #[serde(default)]
    pub(super) water: Option<f32>,
    #[serde(default)]
    pub(super) water_hu: Option<u64>,
    #[serde(default)]
    pub(super) salt_mass: u64,
    #[serde(default)]
    pub(super) draft_closed: bool,
    #[serde(default)]
    pub(super) steam_numerator_remainder: u64,
}
#[derive(Deserialize)]
pub(super) struct SurveyFolioT {
    pub(super) pos: crate::planet::BlockPos,
    pub(super) object_id: u64,
}
#[derive(Deserialize)]
pub(super) struct DiscoveryApparatusT {
    pub(super) pos: crate::planet::BlockPos,
    #[serde(default)]
    pub(super) sample: Option<SlotT>,
    #[serde(default)]
    pub(super) reference: Option<SlotT>,
}
#[derive(Deserialize)]
pub(super) struct BindingFrameT {
    pub(super) pos: crate::planet::BlockPos,
    #[serde(default)]
    pub(super) body: Option<SlotT>,
    #[serde(default)]
    pub(super) reservoir: Option<SlotT>,
    #[serde(default)]
    pub(super) focus: Option<SlotT>,
    #[serde(default)]
    pub(super) binding: Option<SlotT>,
    #[serde(default)]
    pub(super) output: Option<SlotT>,
    #[serde(default)]
    pub(super) revision: u64,
}
#[derive(Deserialize)]
pub(super) struct ChargeVesselT {
    pub(super) pos: crate::planet::BlockPos,
    #[serde(default)]
    pub(super) vessel: Option<SlotT>,
    #[serde(default)]
    pub(super) damage: u16,
    #[serde(default)]
    pub(super) revision: u64,
}
#[derive(Deserialize)]
pub(super) struct SwitchT {
    pub(super) pos: crate::planet::BlockPos,
    #[serde(default)]
    pub(super) selected: String,
}
#[derive(Deserialize)]
pub(super) struct BeltSlotT {
    // Written for hand-editing; the VecDeque order is authoritative
    // on load, so the index is not consulted.
    #[allow(dead_code)]
    pub(super) index: usize,
    pub(super) item: String,
    pub(super) count: u32,
    pub(super) durability: u32,
    #[serde(default)]
    pub(super) arcane_id: u64,
}
#[derive(Deserialize)]
pub(super) struct DepotSlotT {
    pub(super) index: usize,
    pub(super) item: String,
    pub(super) count: u32,
    pub(super) durability: u32,
    #[serde(default)]
    pub(super) arcane_id: u64,
}
#[derive(Deserialize)]
pub(super) struct DepotT {
    pub(super) pos: crate::planet::BlockPos,
    #[serde(default)]
    pub(super) settlement: String,
    #[serde(default)]
    pub(super) slot: Vec<DepotSlotT>,
}
#[derive(Deserialize)]
pub(super) struct BeltT {
    pub(super) pos: crate::planet::BlockPos,
    #[serde(default)]
    pub(super) entry_dir: String,
    #[serde(default)]
    pub(super) progress: f32,
    #[serde(default)]
    pub(super) split_phase: bool,
    #[serde(default)]
    pub(super) slot: Vec<BeltSlotT>,
}
#[derive(Deserialize)]
pub(super) struct FileT {
    pub(super) version: u32,
    #[serde(default)]
    pub(super) furnace: Vec<FurnaceT>,
    #[serde(default)]
    pub(super) chest: Vec<ChestT>,
    #[serde(default)]
    pub(super) offering: Vec<ChestT>,
    #[serde(default)]
    pub(super) clamp: Vec<ClampT>,
    #[serde(default)]
    pub(super) anvil: Vec<AnvilT>,
    #[serde(default)]
    pub(super) sign: Vec<SignT>,
    #[serde(default)]
    pub(super) stall: Vec<StallT>,
    #[serde(default)]
    pub(super) smoker: Vec<SmokerT>,
    #[serde(default)]
    pub(super) steam: Vec<SteamT>,
    #[serde(default)]
    pub(super) machine: Vec<MachineT>,
    #[serde(default)]
    pub(super) survey_folio: Vec<SurveyFolioT>,
    #[serde(default)]
    pub(super) discovery_apparatus: Vec<DiscoveryApparatusT>,
    #[serde(default)]
    pub(super) binding_frame: Vec<BindingFrameT>,
    #[serde(default)]
    pub(super) charge_vessel: Vec<ChargeVesselT>,
    #[serde(default)]
    pub(super) switch: Vec<SwitchT>,
    #[serde(default)]
    pub(super) belt: Vec<BeltT>,
    #[serde(default)]
    pub(super) depot: Vec<DepotT>,
}
