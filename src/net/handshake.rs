//! Authentication transcripts and persistent host trust.

use std::io;
use std::net::SocketAddr;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::AtprotoClaim;
#[cfg(not(test))]
use crate::identity::load_or_create_ed25519_pkcs8;
use crate::identity::{DeviceKeyId, atomic_write, sha256};

#[derive(Serialize)]
struct AuthTranscript<'a> {
    domain: &'static str,
    protocol: u32,
    display_name: &'a str,
    device_public_key: &'a [u8; 32],
    client_nonce: &'a [u8; 32],
    atproto: Option<&'a AtprotoClaim>,
    content_hash: u64,
    style: u32,
    challenge: &'a [u8; 32],
    server_fingerprint: &'a [u8; 32],
}

#[allow(clippy::too_many_arguments)]
pub(super) fn auth_transcript(
    protocol: u32,
    display_name: &str,
    device_public_key: &[u8; 32],
    client_nonce: &[u8; 32],
    atproto: Option<&AtprotoClaim>,
    content_hash: u64,
    style: u32,
    challenge: &[u8; 32],
    server_fingerprint: &[u8; 32],
) -> Vec<u8> {
    postcard::to_allocvec(&AuthTranscript {
        domain: "wildforge-auth-v1",
        protocol,
        display_name,
        device_public_key,
        client_nonce,
        atproto,
        content_hash,
        style,
        challenge,
        server_fingerprint,
    })
    .expect("auth transcript serializes")
}

pub(super) fn server_certificate() -> io::Result<(
    Vec<rustls::pki_types::CertificateDer<'static>>,
    rustls::pki_types::PrivatePkcs8KeyDer<'static>,
    [u8; 32],
)> {
    #[cfg(test)]
    let (cert_der, key_bytes) = {
        let cert = rcgen::generate_simple_self_signed(vec!["wildforge".into()])
            .map_err(io::Error::other)?;
        (cert.cert.der().clone(), cert.key_pair.serialize_der())
    };

    #[cfg(not(test))]
    let (cert_der, key_bytes) = {
        let key_path = crate::identity::identity_dir().join("server-ed25519.pk8");
        let key_bytes = load_or_create_ed25519_pkcs8(&key_path)?;
        let private = rustls::pki_types::PrivatePkcs8KeyDer::from(key_bytes.clone());
        let key_pair = rcgen::KeyPair::from_pkcs8_der_and_sign_algo(&private, &rcgen::PKCS_ED25519)
            .map_err(io::Error::other)?;
        let cert = rcgen::CertificateParams::new(vec!["wildforge".into()])
            .map_err(io::Error::other)?
            .self_signed(&key_pair)
            .map_err(io::Error::other)?;
        (cert.der().clone(), key_bytes)
    };

    let fingerprint = sha256(cert_der.as_ref());
    Ok((
        vec![cert_der],
        rustls::pki_types::PrivatePkcs8KeyDer::from(key_bytes),
        fingerprint,
    ))
}

pub(super) fn peer_certificate_fingerprint(connection: &quinn::Connection) -> io::Result<[u8; 32]> {
    let identity = connection
        .peer_identity()
        .ok_or_else(|| io::Error::other("host supplied no certificate"))?;
    let certificates = identity
        .downcast::<Vec<rustls::pki_types::CertificateDer<'static>>>()
        .map_err(|_| io::Error::other("host certificate has an unexpected type"))?;
    let certificate = certificates
        .first()
        .ok_or_else(|| io::Error::other("host supplied an empty certificate chain"))?;
    Ok(sha256(certificate.as_ref()))
}

#[derive(Default, Serialize, Deserialize)]
struct KnownHosts {
    #[serde(default)]
    host: Vec<KnownHost>,
}

#[derive(Serialize, Deserialize)]
struct KnownHost {
    address: String,
    fingerprint: String,
}

pub(super) fn pin_server_identity(
    path: &Path,
    address: SocketAddr,
    fingerprint: [u8; 32],
) -> io::Result<()> {
    let mut known = match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str::<KnownHosts>(&text).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid known hosts: {error}"),
            )
        })?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => KnownHosts::default(),
        Err(error) => return Err(error),
    };
    let address = address.to_string();
    let observed = DeviceKeyId(fingerprint).to_string();
    if let Some(entry) = known.host.iter().find(|entry| entry.address == address) {
        if entry.fingerprint != observed {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "host identity changed for {address}: expected {}, observed {}",
                    &entry.fingerprint[..entry.fingerprint.len().min(12)],
                    &observed[..12]
                ),
            ));
        }
        return Ok(());
    }
    known.host.push(KnownHost {
        address,
        fingerprint: observed,
    });
    known.host.sort_by(|a, b| a.address.cmp(&b.address));
    let text = toml::to_string_pretty(&known).map_err(io::Error::other)?;
    atomic_write(path, text.as_bytes(), false)
}
