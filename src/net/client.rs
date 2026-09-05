//! Owned QUIC guest connection, authentication, and incoming/outgoing streams.

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use super::{read_frame, read_frame_limited, write_frame};
use crate::identity::atproto::AtprotoAccount;
use crate::identity::{AdmissionPolicy, DisplayName, IdentityPolicy, LocalIdentity};
#[cfg(not(test))]
use crate::net::handshake::pin_server_identity;
use crate::net::handshake::{auth_transcript, peer_certificate_fingerprint};
use crate::net::protocol::{AUTH_TIMEOUT, PREAUTH_FRAME_MAX};
use crate::net::{C2S, PROTOCOL, S2C, decode, encode};

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

async fn write_loop(mut send: quinn::SendStream, mut rx: UnboundedReceiver<Vec<u8>>) {
    while let Some(bytes) = rx.recv().await {
        if write_frame(&mut send, &bytes).await.is_err() {
            break;
        }
    }
}
