//! Multiplayer wire: QUIC (quinn) transport with a compact postcard
//! protocol. One binary — every copy can host ("OPEN TO FRIENDS") or
//! join (LAN discovery + direct IP). The host is authoritative: guests
//! send requests, the host's Server applies them through the same code
//! paths local play uses and broadcasts the results.
//!
//! Reliable messages ride one bidirectional stream per peer with u32
//! length framing; high-rate state (movement, snapshots) rides
//! unreliable datagrams, latest-wins.

use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use glam::Vec3;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

pub const GAME_PORT: u16 = 27431;
pub const BEACON_PORT: u16 = 27430;
/// Bump when the protocol changes shape.
pub const PROTOCOL: u32 = 5;

// ---------------- protocol ----------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StackSnap {
    pub item: u16,
    pub count: u32,
    pub durability: u32,
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
pub enum C2S {
    Hello {
        protocol: u32,
        name: String,
        content_hash: u64,
    },
    Move {
        pos: Vec3,
        yaw: f32,
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
        block: u16,
    },
    AttackMob {
        id: u32,
        dmg: f32,
        from: Vec3,
    },
    FireArrow {
        pos: Vec3,
        vel: Vec3,
        dmg: f32,
        tile: u16,
        recover: bool,
    },
    OpenContainer {
        x: i32,
        y: i32,
        z: i32,
    },
    /// One transactional click: the guest's cursor stack rides along,
    /// the host applies the same click_stack local play uses and echoes
    /// the container plus HeldResult. The cursor stays guest-owned.
    ContainerClick {
        x: i32,
        y: i32,
        z: i32,
        slot: u8,
        right: bool,
        held: Option<StackSnap>,
    },
    CloseContainer,
    /// Right-clicked an adult with its favorite food (consumed guest-side).
    FeedMob {
        id: u32,
    },
    /// Finished the 1.5 s brush channel on a remnant block.
    BrushBlock {
        x: i32,
        y: i32,
        z: i32,
    },
    /// Steelworks: light a charged bloomery / a covered log pile.
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
    /// Anvil: rest a workable item, strike with a hammer, take it back.
    AnvilPut {
        x: i32,
        y: i32,
        z: i32,
        item: u16,
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
    SleepRequest,
    SleepCancel,
    Chat(String),
    Bye,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum S2C {
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
        spawn: Vec3,
        world_name: String,
    },
    Refused(String),
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
    },
    /// (id, pos, yaw) for every player, host included. Datagram.
    Players(Vec<(u32, Vec3, f32)>),
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
        /// 0 chest, 1 furnace, 2 offering, 3 bloomery.
        kind: u8,
        slots: Vec<Option<StackSnap>>,
        /// Live machine state: furnace [progress, burn_left,
        /// burn_total], bloomery [lit, progress 0..1].
        aux: Vec<f32>,
    },
    /// The authoritative cursor stack after a ContainerClick.
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
        id: u32,
        name: String,
    },
    Left {
        id: u32,
    },
}

pub fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    postcard::to_allocvec(msg).unwrap_or_default()
}

pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Option<T> {
    postcard::from_bytes(bytes).ok()
}

// ---------------- host ----------------

pub enum HostEvent {
    Joined {
        id: u32,
        name: String,
        content_hash: u64,
    },
    Msg {
        id: u32,
        msg: C2S,
    },
    Left {
        id: u32,
    },
}

struct Peer {
    reliable: UnboundedSender<Vec<u8>>,
    conn: quinn::Connection,
}

pub struct Host {
    rt: tokio::runtime::Runtime,
    events: UnboundedReceiver<HostEvent>,
    peers: std::collections::HashMap<u32, Peer>,
    peer_rx: UnboundedReceiver<(u32, Peer)>,
    stop: Arc<AtomicBool>,
    pub port: u16,
}

impl Host {
    /// Bind and start accepting. The beacon announces on the LAN.
    /// Port 0 = OS-assigned (tests, second host on one machine).
    pub fn start(world_name: String, port: u16) -> std::io::Result<Host> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let (ev_tx, events) = unbounded_channel();
        let (peer_tx, peer_rx) = unbounded_channel();
        let stop = Arc::new(AtomicBool::new(false));

        let cert = rcgen::generate_simple_self_signed(vec!["wildforge".into()])
            .map_err(std::io::Error::other)?;
        let key = rustls::pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());
        let chain = vec![cert.cert.der().clone()];
        let mut server_config = quinn::ServerConfig::with_single_cert(chain, key.into())
            .map_err(std::io::Error::other)?;
        Arc::get_mut(&mut server_config.transport)
            .unwrap()
            .max_idle_timeout(Some(quinn::VarInt::from_u32(15_000).into()));

        let endpoint = {
            let _guard = rt.enter();
            quinn::Endpoint::server(
                server_config.clone(),
                SocketAddr::from(([0, 0, 0, 0], port)),
            )
            .or_else(|_| {
                // Port busy (another host here): take any free one.
                quinn::Endpoint::server(server_config, SocketAddr::from(([0, 0, 0, 0], 0)))
            })?
        };
        let port = endpoint.local_addr()?.port();

        // Accept loop.
        rt.spawn(accept_loop(endpoint, ev_tx, peer_tx));

        // LAN beacon on a plain std thread.
        {
            let stop = stop.clone();
            std::thread::spawn(move || {
                let Ok(sock) = UdpSocket::bind(("0.0.0.0", 0)) else {
                    return;
                };
                let _ = sock.set_broadcast(true);
                let msg = format!("WILDFORGE|{port}|{world_name}");
                while !stop.load(Ordering::Relaxed) {
                    let _ = sock.send_to(msg.as_bytes(), ("255.255.255.255", BEACON_PORT));
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
            });
        }

        Ok(Host {
            rt,
            events,
            peers: Default::default(),
            peer_rx,
            stop,
            port,
        })
    }

    /// Drain inbound events; call once per frame.
    pub fn poll(&mut self) -> Vec<HostEvent> {
        while let Ok((id, peer)) = self.peer_rx.try_recv() {
            self.peers.insert(id, peer);
        }
        let mut out = Vec::new();
        while let Ok(ev) = self.events.try_recv() {
            if let HostEvent::Left { id } = &ev {
                self.peers.remove(id);
            }
            out.push(ev);
        }
        out
    }

    pub fn send(&self, id: u32, msg: &S2C) {
        if let Some(p) = self.peers.get(&id) {
            let _ = p.reliable.send(encode(msg));
        }
    }

    pub fn broadcast(&self, msg: &S2C) {
        let bytes = encode(msg);
        for p in self.peers.values() {
            let _ = p.reliable.send(bytes.clone());
        }
    }

    /// Unreliable, latest-wins state.
    pub fn broadcast_datagram(&self, msg: &S2C) {
        let bytes = encode(msg);
        for p in self.peers.values() {
            let _ = p.conn.send_datagram(bytes.clone().into());
        }
    }

    pub fn kick(&mut self, id: u32) {
        if let Some(p) = self.peers.remove(&id) {
            p.conn.close(0u32.into(), b"kicked");
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for p in self.peers.values() {
            p.conn.close(0u32.into(), b"host closed");
        }
        // Runtime shuts down when dropped.
        let _ = &self.rt;
    }
}

async fn accept_loop(
    endpoint: quinn::Endpoint,
    events: UnboundedSender<HostEvent>,
    peers: UnboundedSender<(u32, Peer)>,
) {
    let mut next_id: u32 = 1;
    while let Some(incoming) = endpoint.accept().await {
        let Ok(conn) = incoming.await else { continue };
        let id = next_id;
        next_id += 1;
        let events = events.clone();
        let peers = peers.clone();
        tokio::spawn(async move {
            // The guest opens the reliable stream and speaks first.
            let Ok((send, mut recv)) = conn.accept_bi().await else {
                return;
            };
            let Some(first) = read_frame(&mut recv).await else {
                return;
            };
            let Some(C2S::Hello {
                protocol,
                name,
                content_hash,
            }) = decode(&first)
            else {
                return;
            };
            if protocol != PROTOCOL {
                conn.close(1u32.into(), b"protocol mismatch");
                return;
            }
            let (tx, rx) = unbounded_channel::<Vec<u8>>();
            let _ = peers.send((
                id,
                Peer {
                    reliable: tx,
                    conn: conn.clone(),
                },
            ));
            let _ = events.send(HostEvent::Joined {
                id,
                name,
                content_hash,
            });

            // Writer task.
            tokio::spawn(write_loop(send, rx));
            // Datagrams from this guest (movement).
            {
                let conn2 = conn.clone();
                let events2 = events.clone();
                tokio::spawn(async move {
                    while let Ok(d) = conn2.read_datagram().await {
                        if let Some(msg) = decode::<C2S>(&d) {
                            let _ = events2.send(HostEvent::Msg { id, msg });
                        }
                    }
                });
            }
            // Reliable reader.
            while let Some(frame) = read_frame(&mut recv).await {
                let Some(msg) = decode::<C2S>(&frame) else {
                    continue;
                };
                let bye = matches!(msg, C2S::Bye);
                let _ = events.send(HostEvent::Msg { id, msg });
                if bye {
                    break;
                }
            }
            let _ = events.send(HostEvent::Left { id });
        });
    }
}

async fn write_loop(mut send: quinn::SendStream, mut rx: UnboundedReceiver<Vec<u8>>) {
    while let Some(bytes) = rx.recv().await {
        let len = (bytes.len() as u32).to_le_bytes();
        if send.write_all(&len).await.is_err() || send.write_all(&bytes).await.is_err() {
            break;
        }
    }
}

async fn read_frame(recv: &mut quinn::RecvStream) -> Option<Vec<u8>> {
    let mut len = [0u8; 4];
    recv.read_exact(&mut len).await.ok()?;
    let n = u32::from_le_bytes(len) as usize;
    if n > 32 * 1024 * 1024 {
        return None; // sanity
    }
    let mut buf = vec![0u8; n];
    recv.read_exact(&mut buf).await.ok()?;
    Some(buf)
}

// ---------------- client ----------------

pub struct Client {
    rt: tokio::runtime::Runtime,
    inbound: UnboundedReceiver<S2C>,
    reliable: UnboundedSender<Vec<u8>>,
    conn: quinn::Connection,
    pub connected: Arc<AtomicBool>,
}

/// Accept any certificate: friends-and-LAN trust model (the transport
/// is still encrypted; identity is the whitelist, not a CA).
#[derive(Debug)]
struct TrustAny;

impl rustls::client::danger::ServerCertVerifier for TrustAny {
    fn verify_server_cert(
        &self,
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &[rustls::pki_types::CertificateDer<'_>],
        _: &rustls::pki_types::ServerName<'_>,
        _: &[u8],
        _: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

impl Client {
    pub fn connect(addr: SocketAddr, name: String, content_hash: u64) -> std::io::Result<Client> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let (in_tx, inbound) = unbounded_channel();
        let (rel_tx, rel_rx) = unbounded_channel::<Vec<u8>>();
        let connected = Arc::new(AtomicBool::new(false));

        let crypto = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(TrustAny))
            .with_no_client_auth();
        let crypto = quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
            .map_err(std::io::Error::other)?;
        let client_cfg = quinn::ClientConfig::new(Arc::new(crypto));

        let conn = rt.block_on(async {
            let mut endpoint = quinn::Endpoint::client(SocketAddr::from(([0, 0, 0, 0], 0)))?;
            endpoint.set_default_client_config(client_cfg);
            let conn = endpoint
                .connect(addr, "wildforge")
                .map_err(std::io::Error::other)?
                .await
                .map_err(std::io::Error::other)?;
            Ok::<_, std::io::Error>(conn)
        })?;

        // Open the reliable stream and say hello.
        let (send, recv) = rt.block_on(conn.open_bi()).map_err(std::io::Error::other)?;
        let hello = encode(&C2S::Hello {
            protocol: PROTOCOL,
            name,
            content_hash,
        });
        let _ = rel_tx.send(hello);
        rt.spawn(write_loop(send, rel_rx));

        // Reliable reader.
        {
            let in_tx = in_tx.clone();
            let connected = connected.clone();
            connected.store(true, Ordering::Relaxed);
            let conn2 = conn.clone();
            rt.spawn(async move {
                let mut recv = recv;
                while let Some(frame) = read_frame(&mut recv).await {
                    if let Some(msg) = decode::<S2C>(&frame) {
                        let _ = in_tx.send(msg);
                    }
                }
                connected.store(false, Ordering::Relaxed);
                drop(conn2);
            });
        }
        // Datagrams (snapshots).
        {
            let in_tx = in_tx.clone();
            let conn2 = conn.clone();
            rt.spawn(async move {
                while let Ok(d) = conn2.read_datagram().await {
                    if let Some(msg) = decode::<S2C>(&d) {
                        let _ = in_tx.send(msg);
                    }
                }
            });
        }
        Ok(Client {
            rt,
            inbound,
            reliable: rel_tx,
            conn,
            connected,
        })
    }

    pub fn poll(&mut self) -> Vec<S2C> {
        let mut out = Vec::new();
        while let Ok(m) = self.inbound.try_recv() {
            out.push(m);
        }
        out
    }

    pub fn send(&self, msg: &C2S) {
        let _ = self.reliable.send(encode(msg));
    }

    pub fn send_datagram(&self, msg: &C2S) {
        let _ = self.conn.send_datagram(encode(msg).into());
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.send(&C2S::Bye);
        self.conn.close(0u32.into(), b"bye");
        let _ = &self.rt;
    }
}

// ---------------- LAN discovery ----------------

pub struct Discovery {
    sock: UdpSocket,
    pub found: Vec<(SocketAddr, String)>,
}

impl Discovery {
    pub fn start() -> std::io::Result<Discovery> {
        let sock = UdpSocket::bind(("0.0.0.0", BEACON_PORT))?;
        sock.set_nonblocking(true)?;
        Ok(Discovery {
            sock,
            found: Vec::new(),
        })
    }

    /// Poll for beacons; dedupes by address.
    pub fn poll(&mut self) {
        let mut buf = [0u8; 256];
        while let Ok((n, from)) = self.sock.recv_from(&mut buf) {
            let Ok(text) = std::str::from_utf8(&buf[..n]) else {
                continue;
            };
            let mut parts = text.split('|');
            if parts.next() != Some("WILDFORGE") {
                continue;
            }
            let Some(port) = parts.next().and_then(|p| p.parse::<u16>().ok()) else {
                continue;
            };
            let name = parts.next().unwrap_or("world").to_string();
            let addr = SocketAddr::new(from.ip(), port);
            if !self.found.iter().any(|(a, _)| *a == addr) {
                self.found.push((addr, name));
            }
        }
    }
}

/// Hash of the mods directory contents (scripts excluded — they never
/// leave the host). Equal hashes mean identical data content.
pub fn content_hash(dir: &std::path::Path) -> u64 {
    fn hash_bytes(h: &mut u64, bytes: &[u8]) {
        for &b in bytes {
            *h = h.wrapping_mul(0x100000001b3) ^ b as u64;
        }
    }
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    fn walk(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_none_or(|e| e != "rhai") {
                out.push(p);
            }
        }
    }
    walk(dir, &mut files);
    files.sort();
    let mut h: u64 = 0xcbf29ce484222325;
    for f in &files {
        hash_bytes(&mut h, f.to_string_lossy().as_bytes());
        if let Ok(bytes) = std::fs::read(f) {
            hash_bytes(&mut h, &bytes);
        }
    }
    h
}

/// The host's mods dir as (relative path, bytes), scripts excluded.
pub fn collect_mod_files(dir: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    fn walk(root: &std::path::Path, d: &std::path::Path, out: &mut Vec<(String, Vec<u8>)>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else if p.extension().is_none_or(|e| e != "rhai")
                && let (Ok(rel), Ok(bytes)) = (p.strip_prefix(root), std::fs::read(&p))
            {
                out.push((rel.to_string_lossy().replace('\\', "/"), bytes));
            }
        }
    }
    walk(dir, dir, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}
