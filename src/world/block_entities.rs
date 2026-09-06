//! Block entities and compatibility adapters.

use crate::{inventory::ItemStack, planet::BlockPos, registry::BlockId};

/// Per-block persistent state for interactive machines.
// Chest dwarfs the others; entity counts are tiny, so boxing would
// only add indirection.
#[allow(clippy::large_enum_variant)]
pub enum BlockEntity {
    Furnace(FurnaceState),
    Chest(ChestState),
    Offering(OfferingState),
    /// A validated multiblock shell (bloomery/forge/kiln/separator):
    /// one generic representation carrying the shape ID and the state
    /// any furnace-style machine needs (spec Part 2.1 groundwork).
    Multiblock(MachineInstance),
    Clamp(ClampState),
    Anvil(AnvilState),
    /// Three short lines on a post (a waystone uses line 0 as its name).
    Sign(SignState),
    /// A market stall counter: goods, a price, and the owner's till.
    Stall(StallState),
    /// A settlement depot (capability E13): needs deliveries + staging.
    Depot(DepotState),
    /// A smoking rack: raw cuts curing over a live torch.
    Smoker(SmokerState),
    /// A steam firebox: banked fire and exact boiler water.
    Steam(SteamState),
    /// A placed shared library. The records themselves remain signed in the
    /// discovery state; this block owns the physical object that indexes them.
    SurveyFolio(SurveyFolioState),
    /// A controlled trial holds its sample and calibrated reference in world
    /// custody. Experiments read these bays without consuming either.
    DiscoveryApparatus(DiscoveryApparatusState),
    /// Physical mounts and completed output of a binding frame. Components
    /// remain ordinary item stacks while the frame holds them.
    BindingFrame(BindingFrameState),
    /// The placed shell owns exactly one stable vessel item identity.
    ChargeVessel(ChargeVesselState),
    /// A rail switch: which exit is currently selected. The rest of the rail
    /// piece is ordinary block data (spec Part 2.2, scoped).
    Switch(SwitchState),
}

/// One mutable rail-switch block: the currently-selected exit. `None` on a
/// switch block (no entity) reads as the straight default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitchState {
    pub selected: crate::planet::Direction4,
}

impl Default for SwitchState {
    fn default() -> Self {
        Self {
            selected: crate::planet::Direction4::North,
        }
    }
}

#[derive(Default)]
pub struct SurveyFolioState {
    pub object_id: u64,
}

#[derive(Default)]
pub struct DiscoveryApparatusState {
    pub sample: Option<ItemStack>,
    pub reference: Option<ItemStack>,
}

#[derive(Default)]
pub struct BindingFrameState {
    pub body: Option<ItemStack>,
    pub reservoir: Option<ItemStack>,
    pub focus: Option<ItemStack>,
    pub binding: Option<ItemStack>,
    pub output: Option<ItemStack>,
    pub revision: u64,
}

impl BindingFrameState {
    pub fn mounts(&self) -> [Option<ItemStack>; 4] {
        [self.body, self.reservoir, self.focus, self.binding]
    }

    pub fn is_empty(&self) -> bool {
        self.mounts().into_iter().all(|stack| stack.is_none()) && self.output.is_none()
    }
}

#[derive(Default)]
pub struct ChargeVesselState {
    pub vessel: Option<ItemStack>,
    pub damage: u16,
    pub revision: u64,
}

#[derive(Default)]
pub struct MachineInstance {
    /// Which registered machine this is — its shape ID.
    pub kind: crate::world::multiblock::MachineKind,
    /// Whether the fire is lit.
    pub lit: bool,
    /// Seconds fired so far.
    pub progress: f32,
    /// Hollow core cell of the validated shell (set on lighting).
    pub core: Option<BlockPos>,
    /// Four primary input slots (bloomery/forge charge, kiln sand).
    pub charge: [Option<ItemStack>; 4],
    /// A single reagent slot (the kiln's pigment).
    pub reagent: Option<ItemStack>,
    /// Four fuel slots.
    pub fuel: [Option<ItemStack>; 4],
    /// Fractional, already-recovered stock waiting to add up to ordinary
    /// recipe units. Forge instances use it; bloomeries leave it empty.
    pub reclaim: crate::registry::MaterialVector,
    /// Separator input/output counts (rare-earth powder and charcoal in;
    /// neodymium and cerium out).
    pub powder: u32,
    pub separator_fuel: u32,
    pub neodymium: u32,
    pub cerium: u32,
    /// Folded Pattern A stats of the validated shell (spec Part 2.1).
    /// Recomputed on revalidation, never per tick.
    pub stats: crate::world::multiblock::EffectiveStats,
    /// Qualitative capabilities the installed slot modules grant their
    /// frame (spec Part 1.3). Folded on revalidation alongside stats;
    /// distinct from stats because these change what a structure can do,
    /// not how well it does it.
    pub capabilities: crate::world::multiblock::Capabilities,
}

/// Seconds per separator batch (1 powder + 1 fuel -> 1 Nd + 2 Ce).
pub const SEPARATE_SECS: f32 = 45.0;

/// Two and a half minutes of white heat per glass batch. Deliberately
/// left on the wall clock when the day doubled: how long a player
/// stands waiting on a kiln is a question about patience, not about
/// the calendar.
pub const KILN_FIRE_SECS: f32 = 150.0;

/// How far a running generator's field reaches (lamps, the quern).
pub const ELEC_RADIUS: i32 = 6;

#[derive(Default)]
pub struct SteamState {
    /// Seconds of fire banked (coal fed by hand at the door).
    pub fuel: f32,
    /// Exact boiler reservoir; evaporation leaves dissolved salt here.
    pub water: crate::planet_atlas::ReservoirMass,
    /// Physical draft latch operated by hand or a bounded Nudge. This belongs
    /// to the embodied firebox, not generic voxel metadata, whose bits have
    /// unrelated meanings for other blocks and must reset on visual swaps.
    pub draft_closed: bool,
    /// Numerator remainder for HU consumption at 256 HU / 15 seconds.
    pub steam_numerator_remainder: u64,
}

/// One full water cell banks this many seconds of steam.
pub const STEAM_SECS_PER_WATER: f32 = 15.0;
/// The firebox holds at most this much banked fire (seconds).
pub const STEAM_FUEL_CAP: f32 = 1800.0;
/// What a running engine delivers to its shaft line.
pub const STEAM_RATE: f32 = 1.4;

#[derive(Default)]
pub struct SmokerState {
    pub meat: [Option<ItemStack>; 4],
    pub progress: f32,
}

/// A rack-load of cuts cures in four minutes over a steady flame.
pub const SMOKE_SECS: f32 = 240.0;

#[derive(Default)]
pub struct StallState {
    /// The seller (server-shaped PlayerId bytes; zero = unclaimed).
    pub owner: [u8; 16],
    pub owner_name: String,
    pub goods: [Option<ItemStack>; 6],
    /// Price per item sold: any stack is a legal price (barter-native).
    pub price: Option<ItemStack>,
    pub till: [Option<ItemStack>; 6],
}

/// A settlement depot (capability E13): the logistics mouth of a
/// settlement. Deliveries of the bound settlement's declared needs are
/// consumed into the depot and pay reputation through the game layer;
/// belt-fed stock counts toward the same needs without attribution.
#[derive(Clone)]
pub struct DepotState {
    /// Qualified settlement id this depot serves.
    pub settlement: String,
    /// Delivered goods staged here (bounded; surplus of unneeded items is
    /// refused at the door rather than stored).
    pub storage: Box<[Option<crate::inventory::ItemStack>; 12]>,
}

#[derive(Default, Clone)]
pub struct SignState {
    pub lines: [String; 3],
}

/// The glass kiln's fire duration.
#[derive(Default)]
pub struct ClampState {
    pub logs: Vec<BlockPos>,
    /// Seconds remaining until the whole pile converts.
    pub timer: f32,
}

/// A bloom resting on the anvil, part-way worked.
#[derive(Default)]
pub struct AnvilState {
    pub bloom: Option<ItemStack>,
    pub strikes: u32,
}

/// A gravity block mid-fall (host-simulated; guests get snapshots).
#[derive(Clone, Copy)]
pub struct FallingBlock {
    pub pos: crate::planet::EntityPos,
    pub vel: f32,
    pub block: BlockId,
}

/// Five minutes of fire per batch, and the forge's two is still the
/// upgrade worth building. On the wall clock for the same reason the
/// kiln is.
pub const BLOOMERY_FIRE_SECS: f32 = 300.0;
/// The forge runs hotter and shorter than the open stack — capital
/// pays for itself in wall-clock too (economy plan, leg 2).
pub const FORGE_FIRE_SECS: f32 = 120.0;
/// One fuel smelts this many charge items in a forge (a furnace
/// burns roughly one per item — the chimney draft earns its keep).
pub const FORGE_ITEMS_PER_FUEL: u32 = 2;
/// Seconds of smolder per log in a charcoal clamp.
pub const CLAMP_SECS_PER_LOG: f32 = 300.0;
/// Shaft-seconds a powered station banks per strike (a hand strike
/// is a 2 s channel; rate scales this, it never skips it).
pub const STATION_STRIKE_SECS: f32 = 2.0;
/// The helve hammer strikes at half a smith's pace — and all day.
pub const HELVE_STRIKE_SECS: f32 = 4.0;
/// Seconds per pump stroke (one water cell lifted per stroke).
pub const PUMP_STROKE_SECS: f32 = 2.0;
/// How deep a pump's suction column reaches.
pub const PUMP_REACH: i32 = 24;

#[derive(Default)]
pub struct OfferingState {
    pub slots: [Option<ItemStack>; 3],
}

pub const CHEST_SLOTS: usize = 27;

pub struct ChestState {
    pub slots: [Option<ItemStack>; CHEST_SLOTS],
    /// A ruin's chest: first opening costs 1 ire (the wild keeps its
    /// trophies).
    pub wild_owned: bool,
}

impl Default for ChestState {
    fn default() -> ChestState {
        ChestState {
            slots: [None; CHEST_SLOTS],
            wild_owned: false,
        }
    }
}

#[derive(Default)]
pub struct FurnaceState {
    pub input: Option<ItemStack>,
    pub fuel: Option<ItemStack>,
    pub output: Option<ItemStack>,
    pub progress: f32,
    pub burn_left: f32,
    pub burn_total: f32,
    /// Smelt-speed multiplier of the currently burning fuel (embers 2x).
    pub burn_speed: f32,
}
