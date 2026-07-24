//! Wire encoding, protocol version, and allocation/time budgets.

use std::time::Duration;

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::identity::{AdmissionPolicy, IdentityPolicy, Role};

/// Bump whenever a serialized DTO changes shape.
pub const PROTOCOL: u32 = 15;
pub(super) const PREAUTH_FRAME_MAX: usize = 4 * 1024;
pub(super) const CLIENT_FRAME_MAX: usize = 64 * 1024;
pub(super) const AUTH_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StackSnap {
    pub item: u16,
    pub count: u32,
    pub durability: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlayerStateSnap {
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub spawn: Vec3,
    pub health: f32,
    pub hunger: f32,
    pub nutrition: [f32; 5],
    pub hotbar: u8,
    pub inventory: Vec<Option<StackSnap>>,
    pub armor: Vec<Option<StackSnap>>,
    pub cursor: Option<StackSnap>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MobSnap {
    /// Stable host-assigned id: guests interpolate and target by it.
    pub id: u32,
    pub species: u16,
    pub pos: Vec3,
    pub yaw: f32,
    pub growth: f32,
    pub hurt: f32,
    /// "Won't accept food right now" (fed, cooling down, or a juvenile).
    pub fed: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FallSnap {
    pub pos: Vec3,
    pub block: u16,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BoltSnap {
    pub pos: Vec3,
    /// Guests dead-reckon between snapshots.
    pub vel: Vec3,
    pub tile: u16,
    pub age: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AtprotoClaim {
    pub did: String,
    pub binding: String,
    pub share_handle: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Eq, PartialEq)]
pub enum InventoryArea {
    Inventory,
    Craft,
    Armor,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerPresence {
    pub id: u32,
    pub display_name: String,
    pub verified: bool,
    pub cached_verification: bool,
    /// Public ATProto handle, disclosed only when that player opted in.
    pub handle: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModerationAction {
    Kick,
    Mute { seconds: u64 },
    Ban { seconds: Option<u64> },
    Allow,
    CycleRole,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefusalCode {
    Protocol,
    InvalidName,
    Authentication,
    VerificationRequired,
    NameInUse,
    Banned,
    NotAllowlisted,
    AlreadyConnected,
    Content,
    ProfileConflict,
    Kicked,
    Server,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct Refusal {
    pub code: RefusalCode,
    pub detail: String,
}

impl Refusal {
    pub fn new(code: RefusalCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum C2S {
    Hello {
        protocol: u32,
        display_name: String,
        device_public_key: [u8; 32],
        client_nonce: [u8; 32],
        content_hash: u64,
        /// Packed appearance Style (style.rs) — how others draw you.
        style: u32,
    },
    Authenticate {
        signature: Vec<u8>,
        /// Sent only after the server advertises an optional/required policy.
        atproto: Option<AtprotoClaim>,
    },
    Move {
        pos: Vec3,
        yaw: f32,
        hotbar: u8,
        sprint: bool,
    },
    Break {
        x: i32,
        y: i32,
        z: i32,
    },
    Place {
        x: i32,
        y: i32,
        z: i32,
    },
    /// Bucket dip: ask the host to take a full water cell.
    Scoop {
        x: i32,
        y: i32,
        z: i32,
    },
    AttackMob {
        id: u32,
    },
    FireProjectile {
        direction: Vec3,
        charge: f32,
    },
    OpenContainer {
        x: i32,
        y: i32,
        z: i32,
    },
    /// One transactional click. The host owns and applies the cursor stack.
    ContainerClick {
        x: i32,
        y: i32,
        z: i32,
        slot: u8,
        right: bool,
    },
    CloseContainer,
    /// Ask the host to feed an adult mob from authoritative inventory.
    FeedMob {
        id: u32,
    },
    /// Attach a held lead to a tamed mob (host consumes the lead).
    LeadMob {
        id: u32,
    },
    /// Strap held saddlebags onto a tamed carrier.
    SaddleMob {
        id: u32,
    },
    /// Open a tamed carrier's pack (host answers with MobCargo).
    OpenMobCargo {
        id: u32,
    },
    /// Buy one item from a market stall (host validates everything).
    StallBuy {
        x: i32,
        y: i32,
        z: i32,
    },
    /// Write a placed sign or waystone (host validates and broadcasts).
    SetSign {
        x: i32,
        y: i32,
        z: i32,
        lines: [String; 3],
    },
    /// One transactional click in a mob's pack.
    MobCargoClick {
        id: u32,
        slot: u8,
        right: bool,
    },
    /// Report a completed brush channel; the host validates and awards it.
    BrushBlock {
        x: i32,
        y: i32,
        z: i32,
    },
    /// Steelworks: ask the host to light a charged bloomery or covered log pile.
    LightBloomery {
        x: i32,
        y: i32,
        z: i32,
    },
    LightClamp {
        x: i32,
        y: i32,
        z: i32,
    },
    /// Anvil intents; held items and results remain host-authoritative.
    AnvilPut {
        x: i32,
        y: i32,
        z: i32,
    },
    AnvilStrike {
        x: i32,
        y: i32,
        z: i32,
    },
    AnvilTake {
        x: i32,
        y: i32,
        z: i32,
    },
    InventoryClick {
        area: InventoryArea,
        slot: u8,
        right: bool,
    },
    CraftResult {
        size: u8,
    },
    EatSelected,
    Respawn,
    SleepRequest,
    SleepCancel,
    Chat(String),
    Moderate {
        target: u32,
        action: ModerationAction,
    },
    Bye,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum S2C {
    Challenge {
        nonce: [u8; 32],
        server_fingerprint: [u8; 32],
        identity_policy: IdentityPolicy,
        admission_policy: AdmissionPolicy,
    },
    Welcome {
        seed: u32,
        mode: String,
        time: f32,
        ire: f32,
        /// Host block-id -> name; guests remap to their own registry.
        palette: Vec<String>,
        /// Host item-id -> name.
        items: Vec<String>,
        your_id: u32,
        your_role: Role,
        roster: Vec<PlayerPresence>,
        spawn: Vec3,
        world_name: String,
        player_state: PlayerStateSnap,
    },
    PlayerState(PlayerStateSnap),
    Refused(Refusal),
    /// Host mods dir (scripts excluded) when content hashes differ.
    ModFiles(Vec<(String, Vec<u8>)>),
    Chunk {
        x: i32,
        z: i32,
        rle: Vec<u8>,
    },
    BlockSet {
        x: i32,
        y: i32,
        z: i32,
        id: u16,
        /// Octant mask for sub-voxel blocks; 0 for ordinary blocks.
        meta: u8,
    },
    /// (id, pos, yaw, held wire item id, packed style) for every
    /// player, host included (u16::MAX = empty hand). Datagram.
    Players(Vec<(u32, Vec3, f32, u16, u32)>),
    Mobs(Vec<MobSnap>),
    Bolts(Vec<BoltSnap>),
    /// Airborne gravity blocks (sand mid-tumble). Datagram.
    Falling(Vec<FallSnap>),
    TimeIre {
        time: f32,
        ire: f32,
        day: u32,
        weather: u8,
    },
    Hit {
        dmg: f32,
        from: Vec3,
    },
    Give {
        item: u16,
        count: u32,
        durability: u32,
    },
    Container {
        x: i32,
        y: i32,
        z: i32,
        /// 0 chest, 1 furnace, 2 offering, 3 bloomery, 4 kiln.
        kind: u8,
        slots: Vec<Option<StackSnap>>,
        /// Live machine state: furnace [progress, burn_left,
        /// burn_total], bloomery/kiln [lit, progress 0..1].
        aux: Vec<f32>,
    },
    /// Sign text (broadcast on set; the full set arrives on join).
    SignText {
        x: i32,
        y: i32,
        z: i32,
        lines: [String; 3],
    },
    /// A mob pack's contents (sent on open and after each change).
    MobCargo {
        id: u32,
        slots: Vec<Option<StackSnap>>,
    },
    /// The authoritative cursor stack after an inventory/container click.
    HeldResult(Option<StackSnap>),
    Sleep {
        sleeping: u32,
        present: u32,
    },
    Toast(String),
    Chat {
        from: String,
        msg: String,
    },
    Joined {
        presence: PlayerPresence,
    },
    Left {
        id: u32,
    },
    RoleChanged {
        role: Role,
    },
}

pub fn encode<T: Serialize>(message: &T) -> Vec<u8> {
    postcard::to_allocvec(message).unwrap_or_default()
}

pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Option<T> {
    postcard::from_bytes(bytes).ok()
}

/// The first C2S variant is Hello and its first field is always the protocol.
/// Peeking those two varints lets a newer host return a structured mismatch
/// even when the remainder of an older Hello no longer deserializes.
pub(super) fn hello_protocol(bytes: &[u8]) -> Option<u32> {
    let (variant, remaining) = postcard::take_from_bytes::<u32>(bytes).ok()?;
    if variant != 0 {
        return None;
    }
    postcard::take_from_bytes::<u32>(remaining)
        .ok()
        .map(|(protocol, _)| protocol)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    enum LegacyC2S {
        Hello {
            protocol: u32,
            name: String,
        },
        #[allow(dead_code)]
        Bye,
    }

    #[test]
    fn protocol_can_be_peeked_from_an_old_hello_shape() {
        let old = postcard::to_allocvec(&LegacyC2S::Hello {
            protocol: 7,
            name: "MOSS".into(),
        })
        .unwrap();
        assert_eq!(hello_protocol(&old), Some(7));
        assert!(decode::<C2S>(&old).is_none());
    }

    #[test]
    fn authentication_and_gameplay_frames_have_separate_hard_budgets() {
        assert_eq!(PREAUTH_FRAME_MAX, 4 * 1024);
        assert_eq!(CLIENT_FRAME_MAX, 64 * 1024);
        assert_eq!(AUTH_TIMEOUT, Duration::from_secs(5));

        let largest_stock_auth = encode(&C2S::Authenticate {
            signature: vec![0; 64],
            atproto: Some(AtprotoClaim {
                did: format!("did:web:{}", "a".repeat(500)),
                binding: format!("device-{}", "b".repeat(64)),
                share_handle: true,
            }),
        });
        assert!(largest_stock_auth.len() < PREAUTH_FRAME_MAX);
    }
}
