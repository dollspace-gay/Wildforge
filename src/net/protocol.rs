//! Wire encoding, protocol version, and allocation/time budgets.

use std::time::Duration;

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::identity::{AdmissionPolicy, IdentityPolicy, Role};
use crate::planet::{BlockPos, EntityPos};

/// Bump whenever a serialized DTO changes shape.
pub const PROTOCOL: u32 = 19;
pub(super) const PREAUTH_FRAME_MAX: usize = 4 * 1024;
pub(super) const CLIENT_FRAME_MAX: usize = 64 * 1024;
pub(super) const AUTH_TIMEOUT: Duration = Duration::from_secs(5);

/// Conservative floor for a QUIC datagram payload.
///
/// The live budget is `Connection::max_datagram_size()`, which depends on the
/// path MTU and the peer's advertised frame size; this is the value used when
/// a connection cannot answer (and the value snapshot batching is tested
/// against). QUIC datagrams are never fragmented — a payload over the limit is
/// refused outright, not split — so every snapshot has to be batched under it.
pub const DATAGRAM_FLOOR: usize = 1100;

/// The largest view distance, in chunks, a host will serve to one guest.
///
/// A guest asking for more is clamped to this and told what it was granted.
/// Without a cap a single client could ask a dedicated server to page in
/// sixteen thousand chunks on its behalf.
pub const MAX_GUEST_VIEW_DIST: u8 = 12;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StackSnap {
    pub item: u16,
    pub count: u32,
    pub durability: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlayerStateSnap {
    pub pos: EntityPos,
    pub yaw: f32,
    pub pitch: f32,
    pub spawn: EntityPos,
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
    pub pos: EntityPos,
    pub yaw: f32,
    pub growth: f32,
    pub hurt: f32,
    /// "Won't accept food right now" (fed, cooling down, or a juvenile).
    pub fed: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FallSnap {
    pub pos: EntityPos,
    pub block: u16,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BoltSnap {
    pub pos: EntityPos,
    /// Guests dead-reckon between snapshots.
    pub vel: Vec3,
    pub tile: u16,
    pub age: f32,
}

/// One part of a 20 Hz world snapshot.
///
/// Snapshots are latest-wins over unreliable datagrams, and a datagram that
/// does not fit is refused rather than fragmented. So a snapshot too big for
/// the path is split into parts that share a `seq`, and the receiver applies a
/// generation only once every part of it has landed. Losing a part costs that
/// generation, not the rest of the stream: the next one is 50 ms behind it.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Snapshot<T> {
    /// Generation counter. Parts of one snapshot share it; it moves on.
    pub seq: u32,
    pub part: u8,
    pub parts: u8,
    pub items: Vec<T>,
}

impl<T> Snapshot<T> {
    /// The whole snapshot in one part — what almost every tick sends, and
    /// what the tests build by hand.
    #[cfg(test)]
    pub fn whole(seq: u32, items: Vec<T>) -> Snapshot<T> {
        Snapshot {
            seq,
            part: 0,
            parts: 1,
            items,
        }
    }
}

/// Split `items` into snapshot parts that each encode within `budget`.
///
/// Halves until the encoding fits, so it needs no per-item size model and
/// stays correct when postcard's varints change width. A single item that
/// cannot fit is still emitted — dropping it silently is how the original
/// bug behaved, and one oversized datagram that visibly fails is better than
/// a mob that quietly never appears.
pub fn batch_snapshot<T, F>(seq: u32, items: Vec<T>, budget: usize, wrap: F) -> Vec<Vec<u8>>
where
    T: Serialize + Clone,
    F: Fn(Snapshot<T>) -> S2C + Copy,
{
    let mut groups: Vec<Vec<T>> = Vec::new();
    split_groups(items, budget, &wrap, seq, &mut groups);
    if groups.is_empty() {
        // An empty snapshot still ships: it is how a guest clears the last
        // tumble, the last bolt, and mobs that walked out of reach.
        groups.push(Vec::new());
    }
    let parts = groups.len() as u8;
    groups
        .into_iter()
        .enumerate()
        .map(|(i, items)| {
            encode(&wrap(Snapshot {
                seq,
                part: i as u8,
                parts,
                items,
            }))
        })
        .collect()
}

fn split_groups<T, F>(items: Vec<T>, budget: usize, wrap: &F, seq: u32, out: &mut Vec<Vec<T>>)
where
    T: Serialize + Clone,
    F: Fn(Snapshot<T>) -> S2C,
{
    if items.is_empty() {
        return;
    }
    // Probed at the widest possible part count: `part`/`parts` are varints, so
    // measuring with u8::MAX guarantees the probe never under-measures what
    // the batch will actually cost once the real part count is known.
    let probe = encode(&wrap(Snapshot {
        seq,
        part: u8::MAX,
        parts: u8::MAX,
        items: items.clone(),
    }));
    if probe.len() <= budget || items.len() == 1 {
        out.push(items);
        return;
    }
    let mut left = items;
    let right = left.split_off(left.len() / 2);
    split_groups(left, budget, wrap, seq, out);
    split_groups(right, budget, wrap, seq, out);
}

/// Rebuilds multi-part snapshots on the receiving side.
///
/// Holds at most one incomplete generation. A newer `seq` abandons an older
/// incomplete one rather than waiting for it — this is a latest-wins stream,
/// and a generation that lost a datagram is worth less than the one behind it.
pub struct SnapshotAssembler<T> {
    seq: Option<u32>,
    slots: Vec<Option<Vec<T>>>,
}

impl<T> Default for SnapshotAssembler<T> {
    fn default() -> Self {
        SnapshotAssembler {
            seq: None,
            slots: Vec::new(),
        }
    }
}

impl<T> SnapshotAssembler<T> {
    /// Feed one part; yields the whole generation once its last part lands.
    pub fn accept(&mut self, snap: Snapshot<T>) -> Option<Vec<T>> {
        // The overwhelmingly common case: it all fit in one datagram.
        if snap.parts <= 1 {
            self.seq = Some(snap.seq);
            self.slots.clear();
            return Some(snap.items);
        }
        // A straggler from a generation we have already moved past.
        if self.seq.is_some_and(|seen| snap.seq < seen) {
            return None;
        }
        if self.seq != Some(snap.seq) {
            self.seq = Some(snap.seq);
            self.slots = (0..snap.parts).map(|_| None).collect();
        }
        let slot = self.slots.get_mut(snap.part as usize)?;
        *slot = Some(snap.items);
        if self.slots.iter().all(Option::is_some) {
            let whole = self
                .slots
                .iter_mut()
                .filter_map(Option::take)
                .flatten()
                .collect();
            self.slots.clear();
            return Some(whole);
        }
        None
    }
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
        pos: EntityPos,
        yaw: f32,
        hotbar: u8,
        sprint: bool,
    },
    Break {
        pos: BlockPos,
    },
    Place {
        pos: BlockPos,
    },
    /// Bucket dip: ask the host to take a full water cell.
    Scoop {
        pos: BlockPos,
    },
    AttackMob {
        id: u32,
    },
    FireProjectile {
        direction: Vec3,
        charge: f32,
    },
    OpenContainer {
        pos: BlockPos,
    },
    /// One transactional click. The host owns and applies the cursor stack.
    ContainerClick {
        pos: BlockPos,
        slot: u8,
        right: bool,
    },
    CloseContainer,
    /// Ask for terrain the guest does not have (or has evicted and walked
    /// back to). The host used to push chunks and remember forever that it
    /// had — so a guest that dropped a chunk to stay inside its memory
    /// budget could never get it back, and stood in a hole until it
    /// reconnected. Chunk residency is the client's business; this is how it
    /// says so.
    RequestChunk {
        face: u8,
        u: u16,
        v: u16,
    },
    /// How far this guest wants to see, in chunks. Sent after the handshake
    /// (never inside it — the auth transcript stays exactly as it was) and
    /// again whenever the slider moves. The host clamps to
    /// `MAX_GUEST_VIEW_DIST` and answers with `S2C::ViewDistance`.
    SetViewDistance {
        chunks: u8,
    },
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
    /// Board or leave a vehicle mob (the boat follows its rider).
    RideMob {
        id: u32,
        mount: bool,
    },
    /// Buy one item from a market stall (host validates everything).
    StallBuy {
        pos: BlockPos,
    },
    /// Write a placed sign or waystone (host validates and broadcasts).
    SetSign {
        pos: BlockPos,
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
        pos: BlockPos,
    },
    /// Steelworks: ask the host to light a charged bloomery or covered log pile.
    LightBloomery {
        pos: BlockPos,
    },
    LightClamp {
        pos: BlockPos,
    },
    /// Anvil intents; held items and results remain host-authoritative.
    AnvilPut {
        pos: BlockPos,
    },
    AnvilStrike {
        pos: BlockPos,
    },
    AnvilTake {
        pos: BlockPos,
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
        spawn: EntityPos,
        world_name: String,
        player_state: PlayerStateSnap,
    },
    PlayerState(PlayerStateSnap),
    Refused(Refusal),
    /// Host mods dir (scripts excluded) when content hashes differ.
    ModFiles(Vec<(String, Vec<u8>)>),
    Chunk {
        face: u8,
        u: u16,
        v: u16,
        rle: Vec<u8>,
    },
    BlockSet {
        pos: crate::planet::BlockPos,
        id: u16,
        /// Octant mask for sub-voxel blocks; 0 for ordinary blocks.
        meta: u8,
    },
    /// (id, pos, yaw, held wire item id, packed style) for every player in
    /// this guest's reach, host included (u16::MAX = empty hand). Datagram.
    Players(Snapshot<(u32, EntityPos, f32, u16, u32)>),
    Mobs(Snapshot<MobSnap>),
    Bolts(Snapshot<BoltSnap>),
    /// Airborne gravity blocks (sand mid-tumble). Datagram.
    Falling(Snapshot<FallSnap>),
    /// The view distance the host actually granted, in chunks. The guest
    /// renders and evicts against this, so it never stares past what the
    /// host is willing to send.
    ViewDistance {
        chunks: u8,
    },
    TimeIre {
        time: f32,
        ire: f32,
        day: u32,
        weather: u8,
    },
    Hit {
        dmg: f32,
        from: EntityPos,
    },
    Give {
        item: u16,
        count: u32,
        durability: u32,
    },
    Container {
        pos: BlockPos,
        /// 0 chest, 1 furnace, 2 offering, 3 bloomery, 4 kiln.
        kind: u8,
        slots: Vec<Option<StackSnap>>,
        /// Live machine state: furnace [progress, burn_left,
        /// burn_total], bloomery/kiln [lit, progress 0..1].
        aux: Vec<f32>,
    },
    /// Sign text (broadcast on set; the full set arrives on join).
    SignText {
        pos: BlockPos,
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
