# Multiplayer Phases 2–3 — The Wire & The Feel

Drafted and **IMPLEMENTED** 2026-07-18. All three phases shipped: the
sim/client split (`server.rs`), the wire (`net.rs`, `mp.rs` — quinn
transport, LAN discovery, content sync, chunk streaming, authoritative
edits, snapshots), and the feel (chat, sleep votes, shared ire, remote
player models + name tags, `--server` headless, snapshot interpolation
for mobs/players + dead-reckoned bolts, transactional container clicks
with full cursor semantics, guest feeding and brushing). Mobs carry
stable wire ids; guests target by id, never by racy index. Friend-code
rendezvous remains v2/out-of-repo as specified. Loopback-tested.

## Dependencies

`quinn` (QUIC: encryption via rustls, reliable streams + unreliable
datagrams), `tokio` (network thread only — the sim stays synchronous),
`rcgen` (self-signed certs; QUIC requires TLS, guests trust the host's
cert on first join like SSH), `serde` + `postcard` (compact wire
encoding). All pure Rust — the windows-gnu cross-build stays clean.

## Module: `net.rs`

```rust
pub enum C2S {
    Hello { name: String, content_hash: u64 },
    NeedContent,                       // mods mismatch: request sync
    Move { pos: Vec3, yaw: f32, pitch: f32 },     // 20 Hz datagram
    BreakBlock { x: i32, y: i32, z: i32 },
    PlaceBlock { x: i32, y: i32, z: i32, block: String },
    AttackMob { index: u32, dmg: f32 },
    UseContainer { .. }, ContainerClick { .. },   // chest/furnace/offering
    FireArrow { pos: Vec3, vel: Vec3, dmg: f32 },
    SleepRequest, WakeRequest,
    Chat(String),
    Bye,
}
pub enum S2C {
    Welcome { seed: u32, mode: String, time: f32, ire: f32,
              palette: Vec<String>,    // block id -> name (guest remaps)
              your_id: u32, spawn: Vec3 },
    Content { files: Vec<(String, Vec<u8>)> },    // mods dir streamed
    Chunk { x: i32, z: i32, rle: Vec<u8> },       // WFC3 bytes, reused
    BlockSet { x: i32, y: i32, z: i32, id: u16 },
    Players(Vec<(u32, String, Vec3, f32)>),       // 20 Hz datagram
    Mobs(Vec<MobSnap>),                           // 15 Hz datagram
    Projectiles(Vec<ProjSnap>),
    TimeIre { time: f32, ire: f32 },
    YouWereHit { dmg: f32, from: Vec3 },
    Entities { .. },                              // container deltas
    Sleep { sleepers: u8, present: u8 },          // vote progress
    Dawn, Chat(String, String), PlayerJoined(..), PlayerLeft(..),
}
```

Reliable streams: Welcome/Content/Chunk/BlockSet/Entities/Chat.
Unreliable datagrams: Move/Players/Mobs/Projectiles (latest-wins).

## Host (listen server)

- Pause menu **OPEN TO FRIENDS** starts a tokio thread with a quinn
  endpoint (default port 27431) + a UDP LAN beacon
  (`WILDFORGE|port|world_name`, 2 s cadence).
- Per-connection tasks bridge to the Game loop via mpsc channels; the
  host drains inbound messages once per frame *before*
  `Server::advance`, applies them through the same code paths local
  play uses (server-authoritative by construction), and broadcasts
  outbound deltas after.
- Validation: reach ≤ 6 blocks, edit rate ≤ 10/s, tier gates checked
  server-side, movement speed sanity (teleport clamp). Whitelist +
  host kick from the pause menu.
- Content sync: `content_hash` = hash of the mods dir; on mismatch the
  host streams every file under `mods/` (data + textures + scripts stay
  host-side — scripts are never sent). Guest caches under
  `saves/.remote/<host>/mods`, rebuilds its registry/atlas from it,
  then proceeds. Texture packs remain the guest's own.
- Guests' survival state (health/hunger/inventory/armor) lives
  guest-side v1 (trusted friends), persisted to the host as
  `saves/<world>/players/<name>.toml` on disconnect/save.

## Guest mode

- Title screen **JOIN GAME**: LAN-discovered list + direct IP field
  (the search-box input plumbing reused).
- `Game.remote: Option<Connection>`; when set:
  - `World.remote = true`: `ensure_chunk` never generates — chunks
    arrive only from `S2C::Chunk` (same RLE decode as saves, palette
    remapped by name).
  - `Server::advance` does not run; time/ire/mobs/projectiles are
    applied from snapshots. Own movement is predicted locally
    (unchanged physics); block edits render immediately as a local
    guess, corrected by the authoritative `BlockSet` echo.
  - Interactions send C2S requests; container screens render from
    `Entities` deltas.
- Remote players render as box models (the mob model system: head,
  body, arms, legs — a built-in "player" model) with name tags
  projected via the view matrix.

## Phase 3 — the feel

- Interpolation: mobs/players render from a 2-snapshot buffer with
  ~100 ms delay; own-player prediction already exists.
- **Chat**: T opens an input line (search plumbing), Enter sends;
  messages toast with the sender's name.
- **Shared ire**: it already is (one world, one meter). The inventory
  meter adds "mostly you" / "mostly <name>" from per-player
  contribution tallies the host keeps.
- **Sleep votes**: dawn requires every present player in a bedroll
  (`Sleep{sleepers, present}` shown as a toast); wardens-near still
  vetoes individually.
- **`--server <world>`**: headless host — registry, world, `Server`,
  net loop at 30 Hz, autosave every 5 min, no window (winit/wgpu never
  initialized). Same binary.
- Friend-code rendezvous (v2, explicitly out of repo scope): a tiny
  hole-punch service; direct IP + LAN ship first.

## Tests (all headless, loopback)

- Two quinn endpoints on localhost: join handshake, palette remap,
  chunk round-trip (RLE bytes identical), block edit → broadcast echo.
- Validation: out-of-reach edit rejected; rate limit trips.
- Content hash mismatch triggers Content and the guest registry builds
  from the streamed files.
- Guest world never generates chunks; snapshots apply.
- Sleep vote requires all present players.
- Serialization round-trips for every message variant.

## Implementation order

1. net.rs types + postcard round-trip tests.
2. Host endpoint + LAN beacon + join handshake + chunk streaming
   (loopback-tested).
3. Guest mode: remote world, snapshot application, edit requests.
4. Remote player models + interpolation + chat.
5. Sleep votes, ire contributions, kick/whitelist UI, `--server`.
6. Two-instance manual verification, README, ship.
