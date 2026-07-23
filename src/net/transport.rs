//! Multiplayer wire: QUIC (quinn) transport with a compact postcard
//! protocol. One binary — every copy can host ("OPEN TO FRIENDS") or
//! join (LAN discovery + direct IP). The host is authoritative: guests
//! send requests, the host's Server applies them through the same code
//! paths local play uses and broadcasts the results.
//!
//! Reliable messages ride one bidirectional stream per peer with u32
//! length framing; high-rate state (movement, snapshots) rides
//! unreliable datagrams, latest-wins.

use std::io;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::identity::atproto::{AtprotoAccount, ProofCache};
use crate::identity::{
    AdmissionPolicy, DeviceKeyId, DisplayName, IdentityPolicy, LocalIdentity, Principal,
};

#[cfg(test)]
use super::handshake::certificate_from_pkcs8;
use super::handshake::{
    auth_transcript, peer_certificate_fingerprint, pin_server_identity, server_certificate,
};
use super::protocol::{AUTH_TIMEOUT, CLIENT_FRAME_MAX, PREAUTH_FRAME_MAX, hello_protocol};
use super::{C2S, PROTOCOL, Refusal, RefusalCode, S2C, decode, encode};

pub const GAME_PORT: u16 = 27431;
pub const BEACON_PORT: u16 = 27430;
const HANDSHAKE_WINDOW: Duration = Duration::from_secs(10);
const HANDSHAKES_PER_WINDOW: u16 = 12;

// ---------------- host ----------------

pub enum HostEvent {
    Joined {
        id: u32,
        display_name: DisplayName,
        principal: Principal,
        principals: Vec<Principal>,
        verification_cached: bool,
        verified_handle: Option<String>,
        public_handle: Option<String>,
        content_hash: u64,
        style: u32,
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
    reliable: UnboundedSender<HostOutbound>,
    conn: quinn::Connection,
}

enum HostOutbound {
    Frame(Vec<u8>),
    Finish,
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
    pub fn start(
        world_name: String,
        port: u16,
        identity_policy: IdentityPolicy,
        admission_policy: AdmissionPolicy,
        verification_grace_secs: u64,
    ) -> std::io::Result<Host> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let (ev_tx, events) = unbounded_channel();
        let (peer_tx, peer_rx) = unbounded_channel();
        let stop = Arc::new(AtomicBool::new(false));

        let (chain, key, server_fingerprint) = server_certificate()?;
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

        #[cfg(not(test))]
        let proof_cache_path = std::path::PathBuf::from("saves")
            .join(&world_name)
            .join("moderation/atproto-cache.toml");
        #[cfg(test)]
        let proof_cache_path = std::env::temp_dir().join(format!(
            "wildforge-atproto-cache-{}-{port}.toml",
            std::process::id()
        ));
        let proof_cache = Arc::new(ProofCache::load(proof_cache_path, verification_grace_secs)?);

        // Accept loop.
        rt.spawn(accept_loop(
            endpoint,
            ev_tx,
            peer_tx,
            server_fingerprint,
            identity_policy,
            admission_policy,
            proof_cache,
        ));

        // LAN beacon on a plain std thread.
        {
            let stop = stop.clone();
            std::thread::spawn(move || {
                let Ok(sock) = UdpSocket::bind(("0.0.0.0", 0)) else {
                    return;
                };
                let _ = sock.set_broadcast(true);
                let msg = format!(
                    "WILDFORGE|{port}|{world_name}|{}|{}",
                    identity_policy.as_str(),
                    admission_policy.as_str()
                );
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
            let _ = p.reliable.send(HostOutbound::Frame(encode(msg)));
        }
    }

    pub fn broadcast(&self, msg: &S2C) {
        let bytes = encode(msg);
        for p in self.peers.values() {
            let _ = p.reliable.send(HostOutbound::Frame(bytes.clone()));
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
            // Queue a graceful stream finish behind any structured refusal.
            // Closing the QUIC connection here could discard that final frame
            // before the writer task had a chance to put it on the wire.
            let _ = p.reliable.send(HostOutbound::Finish);
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
    server_fingerprint: [u8; 32],
    identity_policy: IdentityPolicy,
    admission_policy: AdmissionPolicy,
    proof_cache: Arc<ProofCache>,
) {
    let mut next_id: u32 = 1;
    let attempts = Arc::new(std::sync::Mutex::new(std::collections::HashMap::<
        IpAddr,
        (Instant, u16),
    >::new()));
    while let Some(incoming) = endpoint.accept().await {
        let Ok(conn) = incoming.await else { continue };
        if !allow_handshake_attempt(&attempts, conn.remote_address().ip()) {
            conn.close(9u32.into(), b"too many connection attempts");
            continue;
        }
        let id = next_id;
        next_id += 1;
        let events = events.clone();
        let peers = peers.clone();
        let proof_cache = proof_cache.clone();
        tokio::spawn(async move {
            // The guest opens the reliable stream and speaks first.
            let Ok((mut send, mut recv)) = conn.accept_bi().await else {
                return;
            };
            let Ok(Some(first)) = tokio::time::timeout(
                AUTH_TIMEOUT,
                read_frame_limited(&mut recv, PREAUTH_FRAME_MAX),
            )
            .await
            else {
                return;
            };
            let Some(C2S::Hello {
                protocol,
                display_name,
                device_public_key,
                client_nonce,
                content_hash,
                style,
            }) = decode(&first)
            else {
                if let Some(protocol) = hello_protocol(&first)
                    && protocol != PROTOCOL
                {
                    let _ = write_frame(
                        &mut send,
                        &encode(&S2C::Refused(Refusal::new(
                            RefusalCode::Protocol,
                            format!("protocol {protocol} is incompatible with {PROTOCOL}"),
                        ))),
                    )
                    .await;
                }
                return;
            };
            if protocol != PROTOCOL {
                let _ = write_frame(
                    &mut send,
                    &encode(&S2C::Refused(Refusal::new(
                        RefusalCode::Protocol,
                        format!("protocol {protocol} is incompatible with {PROTOCOL}"),
                    ))),
                )
                .await;
                conn.close(1u32.into(), b"protocol mismatch");
                return;
            }
            let Ok(display_name) = DisplayName::parse(&display_name) else {
                let _ = write_frame(
                    &mut send,
                    &encode(&S2C::Refused(Refusal::new(
                        RefusalCode::InvalidName,
                        "name must be 1-16 letters, numbers, spaces, dots, or hyphens",
                    ))),
                )
                .await;
                conn.close(2u32.into(), b"invalid display name");
                return;
            };
            let nonce = match crate::identity::random_nonce() {
                Ok(nonce) => nonce,
                Err(_) => return,
            };
            if write_frame(
                &mut send,
                &encode(&S2C::Challenge {
                    nonce,
                    server_fingerprint,
                    identity_policy,
                    admission_policy,
                }),
            )
            .await
            .is_err()
            {
                return;
            }
            let Ok(Some(auth_frame)) = tokio::time::timeout(
                AUTH_TIMEOUT,
                read_frame_limited(&mut recv, PREAUTH_FRAME_MAX),
            )
            .await
            else {
                return;
            };
            let Some(C2S::Authenticate { signature, atproto }) = decode(&auth_frame) else {
                return;
            };
            let Ok(signature): Result<[u8; 64], _> = signature.try_into() else {
                return;
            };
            let transcript = auth_transcript(
                protocol,
                display_name.as_str(),
                &device_public_key,
                &client_nonce,
                atproto.as_ref(),
                content_hash,
                style,
                &nonce,
                &server_fingerprint,
            );
            if crate::identity::verify_signature(&device_public_key, &transcript, &signature)
                .is_err()
            {
                let _ = write_frame(
                    &mut send,
                    &encode(&S2C::Refused(Refusal::new(
                        RefusalCode::Authentication,
                        "device signature did not verify",
                    ))),
                )
                .await;
                conn.close(4u32.into(), b"authentication failed");
                return;
            }
            let local = Principal::LocalDevice(DeviceKeyId::of_public_key(&device_public_key));
            let (principal, principals, verification_cached, verified_handle, public_handle) =
                match atproto.as_ref() {
                    Some(claim) => match proof_cache.verify(claim, &device_public_key).await {
                        Ok(verified) => {
                            let online = Principal::Atproto(verified.did);
                            let public_handle = claim
                                .share_handle
                                .then(|| verified.handle.clone())
                                .flatten();
                            (
                                online.clone(),
                                vec![online, local.clone()],
                                verified.cached,
                                verified.handle,
                                public_handle,
                            )
                        }
                        Err(error) => {
                            let _ = write_frame(
                                &mut send,
                                &encode(&S2C::Refused(Refusal::new(
                                    RefusalCode::Authentication,
                                    format!("ATProto device binding did not verify: {error}"),
                                ))),
                            )
                            .await;
                            conn.close(5u32.into(), b"ATProto verification failed");
                            return;
                        }
                    },
                    None if identity_policy == IdentityPolicy::AtprotoRequired => {
                        let _ = write_frame(
                            &mut send,
                            &encode(&S2C::Refused(Refusal::new(
                                RefusalCode::VerificationRequired,
                                "this server requires a linked ATProto account",
                            ))),
                        )
                        .await;
                        conn.close(3u32.into(), b"verification required");
                        return;
                    }
                    None => (local.clone(), vec![local], false, None, None),
                };
            let (tx, rx) = unbounded_channel::<HostOutbound>();
            let _ = peers.send((
                id,
                Peer {
                    reliable: tx,
                    conn: conn.clone(),
                },
            ));
            let _ = events.send(HostEvent::Joined {
                id,
                display_name,
                principal,
                principals,
                verification_cached,
                verified_handle,
                public_handle,
                content_hash,
                style,
            });

            // Writer task.
            tokio::spawn(host_write_loop(send, rx));
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
            while let Some(frame) = read_frame_limited(&mut recv, CLIENT_FRAME_MAX).await {
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

fn allow_handshake_attempt(
    attempts: &std::sync::Mutex<std::collections::HashMap<IpAddr, (Instant, u16)>>,
    ip: IpAddr,
) -> bool {
    let Ok(mut attempts) = attempts.lock() else {
        return false;
    };
    let now = Instant::now();
    attempts.retain(|_, (started, _)| now.duration_since(*started) <= HANDSHAKE_WINDOW * 2);
    let entry = attempts.entry(ip).or_insert((now, 0));
    if now.duration_since(entry.0) > HANDSHAKE_WINDOW {
        *entry = (now, 0);
    }
    if entry.1 >= HANDSHAKES_PER_WINDOW {
        return false;
    }
    entry.1 += 1;
    true
}

async fn write_loop(mut send: quinn::SendStream, mut rx: UnboundedReceiver<Vec<u8>>) {
    while let Some(bytes) = rx.recv().await {
        if write_frame(&mut send, &bytes).await.is_err() {
            break;
        }
    }
}

async fn host_write_loop(mut send: quinn::SendStream, mut rx: UnboundedReceiver<HostOutbound>) {
    while let Some(command) = rx.recv().await {
        match command {
            HostOutbound::Frame(bytes) => {
                if write_frame(&mut send, &bytes).await.is_err() {
                    return;
                }
            }
            HostOutbound::Finish => {
                let _ = send.finish();
                return;
            }
        }
    }
    let _ = send.finish();
}

async fn read_frame(recv: &mut quinn::RecvStream) -> Option<Vec<u8>> {
    read_frame_limited(recv, 32 * 1024 * 1024).await
}

async fn read_frame_limited(recv: &mut quinn::RecvStream, limit: usize) -> Option<Vec<u8>> {
    let mut len = [0u8; 4];
    recv.read_exact(&mut len).await.ok()?;
    let n = u32::from_le_bytes(len) as usize;
    if n > limit {
        return None; // sanity
    }
    let mut buf = vec![0u8; n];
    recv.read_exact(&mut buf).await.ok()?;
    Some(buf)
}

async fn write_frame(send: &mut quinn::SendStream, bytes: &[u8]) -> std::io::Result<()> {
    let len = u32::try_from(bytes.len())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "frame too large"))?
        .to_le_bytes();
    send.write_all(&len).await.map_err(io::Error::other)?;
    send.write_all(bytes).await.map_err(io::Error::other)
}

// ---------------- client ----------------

pub struct Client {
    rt: tokio::runtime::Runtime,
    inbound: UnboundedReceiver<S2C>,
    reliable: UnboundedSender<Vec<u8>>,
    conn: quinn::Connection,
    pub connected: Arc<AtomicBool>,
    pub identity_policy: IdentityPolicy,
    pub admission_policy: AdmissionPolicy,
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
    pub fn connect(
        addr: SocketAddr,
        display_name: String,
        content_hash: u64,
        style: u32,
        identity: &LocalIdentity,
        atproto: Option<&AtprotoAccount>,
    ) -> std::io::Result<Client> {
        let display_name = DisplayName::parse(&display_name)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?
            .to_string();
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

        let server_fingerprint = peer_certificate_fingerprint(&conn)?;
        #[cfg(not(test))]
        pin_server_identity(
            &crate::identity::identity_dir().join("known-hosts.toml"),
            addr,
            server_fingerprint,
        )?;

        // Authenticate before the connection is admitted to gameplay.
        let (mut send, mut recv) = rt.block_on(conn.open_bi()).map_err(std::io::Error::other)?;
        let client_nonce = crate::identity::random_nonce()?;
        let hello_msg = C2S::Hello {
            protocol: PROTOCOL,
            display_name: display_name.clone(),
            device_public_key: identity.public_key(),
            client_nonce,
            content_hash,
            style,
        };
        let hello = encode(&hello_msg);
        rt.block_on(write_frame(&mut send, &hello))?;
        let challenge = rt
            .block_on(async {
                tokio::time::timeout(
                    AUTH_TIMEOUT,
                    read_frame_limited(&mut recv, PREAUTH_FRAME_MAX),
                )
                .await
            })
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "host auth timeout"))?
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "host closed"))?;
        let (nonce, challenge_fingerprint, identity_policy, admission_policy) =
            match decode::<S2C>(&challenge) {
                Some(S2C::Challenge {
                    nonce,
                    server_fingerprint,
                    identity_policy,
                    admission_policy,
                }) => (nonce, server_fingerprint, identity_policy, admission_policy),
                Some(S2C::Refused(why)) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        why.detail,
                    ));
                }
                _ => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "host sent an invalid authentication challenge",
                    ));
                }
            };
        if challenge_fingerprint != server_fingerprint {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "host certificate and challenge identity disagree",
            ));
        }
        let atproto_claim = match identity_policy {
            IdentityPolicy::Local => None,
            IdentityPolicy::AtprotoOptional => atproto.map(AtprotoAccount::claim),
            IdentityPolicy::AtprotoRequired => Some(
                atproto
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "this server requires a linked ATProto account",
                        )
                    })?
                    .claim(),
            ),
        };
        let transcript = auth_transcript(
            PROTOCOL,
            &display_name,
            &identity.public_key(),
            &client_nonce,
            atproto_claim.as_ref(),
            content_hash,
            style,
            &nonce,
            &server_fingerprint,
        );
        let auth = encode(&C2S::Authenticate {
            signature: identity.sign(&transcript).to_vec(),
            atproto: atproto_claim,
        });
        rt.block_on(write_frame(&mut send, &auth))?;

        // The framed writer takes over only after authentication.
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
            identity_policy,
            admission_policy,
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

#[derive(Clone, Debug)]
pub struct DiscoveredServer {
    pub addr: SocketAddr,
    pub name: String,
    pub identity: IdentityPolicy,
    pub admission: AdmissionPolicy,
}

pub struct Discovery {
    sock: UdpSocket,
    pub found: Vec<DiscoveredServer>,
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
            let identity = parts
                .next()
                .and_then(IdentityPolicy::parse)
                .unwrap_or(IdentityPolicy::Local);
            let admission = parts
                .next()
                .and_then(AdmissionPolicy::parse)
                .unwrap_or(AdmissionPolicy::Open);
            let addr = SocketAddr::new(from.ip(), port);
            if let Some(found) = self.found.iter_mut().find(|found| found.addr == addr) {
                found.name = name;
                found.identity = identity;
                found.admission = admission;
            } else {
                self.found.push(DiscoveredServer {
                    addr,
                    name,
                    identity,
                    admission,
                });
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

#[cfg(test)]
mod security_tests {
    use super::*;

    #[test]
    fn changed_host_key_is_refused_without_overwriting_the_pin() {
        let root =
            std::env::temp_dir().join(format!("wildforge-known-hosts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("known-hosts.toml");
        let addr: SocketAddr = "127.0.0.1:27431".parse().unwrap();
        pin_server_identity(&path, addr, [1; 32]).unwrap();
        pin_server_identity(&path, addr, [1; 32]).unwrap();
        let before = std::fs::read_to_string(&path).unwrap();
        let error = pin_server_identity(&path, addr, [2; 32]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(error.to_string().contains("host identity changed"));
        assert_eq!(std::fs::read_to_string(path).unwrap(), before);
    }

    #[test]
    fn persisted_host_key_recreates_the_same_certificate_fingerprint() {
        let root = std::env::temp_dir().join(format!(
            "wildforge-persistent-host-key-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let key_path = root.join("server-ed25519.pk8");
        let first_key = crate::identity::load_or_create_ed25519_pkcs8(&key_path).unwrap();
        let second_key = crate::identity::load_or_create_ed25519_pkcs8(&key_path).unwrap();
        assert_eq!(first_key, second_key);
        let (first_cert, _) = certificate_from_pkcs8(first_key).unwrap();
        let (second_cert, _) = certificate_from_pkcs8(second_key).unwrap();
        assert_eq!(
            crate::identity::sha256(first_cert.as_ref()),
            crate::identity::sha256(second_cert.as_ref())
        );
    }

    #[test]
    fn auth_transcript_binds_nonce_server_and_hello_fields() {
        let root =
            std::env::temp_dir().join(format!("wildforge-transcript-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let identity = LocalIdentity::load_or_create(&root).unwrap();
        let client_nonce = [3; 32];
        let challenge = [4; 32];
        let server = [5; 32];
        let transcript = auth_transcript(
            PROTOCOL,
            "MOSS",
            &identity.public_key(),
            &client_nonce,
            None,
            9,
            7,
            &challenge,
            &server,
        );
        let signature = identity.sign(&transcript);
        crate::identity::verify_signature(&identity.public_key(), &transcript, &signature).unwrap();
        for changed in [
            auth_transcript(
                PROTOCOL,
                "FERN",
                &identity.public_key(),
                &client_nonce,
                None,
                9,
                7,
                &challenge,
                &server,
            ),
            auth_transcript(
                PROTOCOL,
                "MOSS",
                &identity.public_key(),
                &client_nonce,
                None,
                9,
                7,
                &[6; 32],
                &server,
            ),
            auth_transcript(
                PROTOCOL,
                "MOSS",
                &identity.public_key(),
                &client_nonce,
                None,
                9,
                7,
                &challenge,
                &[8; 32],
            ),
        ] {
            assert!(
                crate::identity::verify_signature(&identity.public_key(), &changed, &signature)
                    .is_err()
            );
        }
    }

    #[test]
    fn handshake_attempts_are_bounded_per_source_window() {
        let attempts = std::sync::Mutex::new(std::collections::HashMap::new());
        let ip: IpAddr = "192.0.2.10".parse().unwrap();
        for _ in 0..HANDSHAKES_PER_WINDOW {
            assert!(allow_handshake_attempt(&attempts, ip));
        }
        assert!(!allow_handshake_attempt(&attempts, ip));
    }
}
