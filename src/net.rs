//! Typed multiplayer wire facade.
//!
//! DTOs and handshake transcripts are deliberately independent of the QUIC
//! implementation. Callers use this module while endpoint, stream, framing,
//! and discovery details remain in `transport`.

#[path = "net/protocol.rs"]
mod protocol;
pub use protocol::{
    AtprotoClaim, BoltSnap, C2S, FallSnap, InventoryArea, MobSnap, ModerationAction, PROTOCOL,
    PlayerPresence, PlayerStateSnap, Refusal, RefusalCode, S2C, StackSnap, decode, encode,
};

#[path = "net/handshake.rs"]
mod handshake;

#[path = "net/transport.rs"]
mod transport;
pub use transport::{
    Client, DiscoveredServer, Discovery, GAME_PORT, Host, HostEvent, collect_mod_files,
    content_hash,
};
