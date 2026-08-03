//! Typed multiplayer wire facade.
//!
//! DTOs and handshake transcripts are deliberately independent of the QUIC
//! implementation. Callers use this module while endpoint, stream, framing,
//! and discovery details remain in `transport`.

#[path = "net/protocol.rs"]
mod protocol;
#[cfg_attr(not(test), allow(unused_imports))]
pub use protocol::{
    AtprotoClaim, BoltSnap, C2S, DATAGRAM_FLOOR, DiscoveryTargetSnap, FallSnap, InventoryArea,
    LooseItemSnap, MAX_GUEST_VIEW_DIST, MobSnap, ModerationAction, PROTOCOL, PlayerPresence,
    PlayerSnap, PlayerStateSnap, RecordHolderSnap, Refusal, RefusalCode, S2C, Snapshot,
    SnapshotAssembler, StackSnap, batch_snapshot, decode, encode,
};

#[path = "net/handshake.rs"]
mod handshake;

#[path = "net/transport.rs"]
mod transport;
pub use transport::{
    Client, DiscoveredServer, Discovery, GAME_PORT, Host, HostEvent, collect_mod_files,
    content_hash,
};
