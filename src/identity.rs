//! Stable local identity and the identifiers used by multiplayer policy.
//!
//! Display names are presentation. A server-owned [`PlayerId`] owns saved
//! progress, while one or more authenticated [`Principal`] values may open it.

use std::fmt;
use std::io;

use ring::digest::{SHA256, digest};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};

#[path = "identity/atproto.rs"]
pub mod atproto;
#[path = "identity/local.rs"]
mod local;
pub use local::{
    LocalIdentity, finish_local_profile_migration, identity_dir, local_profile_path, random_nonce,
    verify_signature,
};
pub(crate) use local::{atomic_write, load_or_create_ed25519_pkcs8, sha256};

pub const DISPLAY_NAME_MAX: usize = 16;
pub const NONCE_LEN: usize = 32;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct PlayerId(pub [u8; 16]);

impl PlayerId {
    pub fn random() -> io::Result<Self> {
        let mut bytes = [0; 16];
        SystemRandom::new()
            .fill(&mut bytes)
            .map_err(|_| io::Error::other("secure random generation failed"))?;
        // Keep the familiar UUID variant/version bits while retaining a plain
        // dependency-free wire representation.
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Ok(Self(bytes))
    }

    pub fn parse(value: &str) -> Option<Self> {
        decode_hex::<16>(&value.replace('-', "")).map(Self)
    }
}

impl fmt::Display for PlayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let h = encode_hex(&self.0);
        write!(
            f,
            "{}-{}-{}-{}-{}",
            &h[0..8],
            &h[8..12],
            &h[12..16],
            &h[16..20],
            &h[20..32]
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct DeviceKeyId(pub [u8; 32]);

impl DeviceKeyId {
    pub fn of_public_key(public_key: &[u8]) -> Self {
        let hash = digest(&SHA256, public_key);
        let mut out = [0; 32];
        out.copy_from_slice(hash.as_ref());
        Self(out)
    }

    pub fn parse(value: &str) -> Option<Self> {
        decode_hex::<32>(value).map(Self)
    }

    pub fn short(&self) -> String {
        encode_hex(&self.0[..6])
    }
}

impl fmt::Display for DeviceKeyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&encode_hex(&self.0))
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct AtprotoDid(String);

impl AtprotoDid {
    pub fn parse(value: &str) -> Result<Self, IdentityError> {
        let value = value.trim();
        if value.len() > 512
            || !value.is_ascii()
            || !(value.starts_with("did:plc:") || value.starts_with("did:web:"))
            || jacquard_common::types::did::validate_did(value).is_err()
            || value
                .split_once(':')
                .and_then(|(_, rest)| rest.split_once(':'))
                .is_none_or(|(_, identifier)| identifier.is_empty())
            || value.chars().any(char::is_whitespace)
        {
            return Err(IdentityError::Did);
        }
        // ATProto emits canonical lowercase DIDs, but DID method-specific
        // identifiers are not generally safe to case-fold (notably did:web
        // path components). Preserve the asserted identifier exactly.
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn short(&self) -> String {
        const MAX: usize = 24;
        if self.0.len() <= MAX {
            return self.0.clone();
        }
        let prefix = if self.0.starts_with("did:plc:") {
            "did:plc:"
        } else {
            "did:web:"
        };
        let remaining = MAX.saturating_sub(prefix.len() + 1);
        format!(
            "{prefix}{}…",
            &self.0[prefix.len()..prefix.len() + remaining]
        )
    }
}

impl fmt::Display for AtprotoDid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum Principal {
    LocalDevice(DeviceKeyId),
    Atproto(AtprotoDid),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityPolicy {
    #[default]
    Local,
    AtprotoOptional,
    AtprotoRequired,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionPolicy {
    #[default]
    Open,
    Allowlist,
}

/// Server-side authority granted to an authenticated principal.
///
/// Roles are identity policy rather than UI state: every privileged request is
/// re-checked by the authoritative host before it changes durable moderation
/// data or another player's session.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    Admin,
    Moderator,
    #[default]
    Player,
}

impl Role {
    pub fn can_moderate(self) -> bool {
        matches!(self, Self::Owner | Self::Admin | Self::Moderator)
    }

    pub fn can_administer(self) -> bool {
        matches!(self, Self::Owner | Self::Admin)
    }
}

impl IdentityPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::AtprotoOptional => "atproto_optional",
            Self::AtprotoRequired => "atproto_required",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "local" => Some(Self::Local),
            "atproto_optional" => Some(Self::AtprotoOptional),
            "atproto_required" => Some(Self::AtprotoRequired),
            _ => None,
        }
    }
}

impl AdmissionPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Allowlist => "allowlist",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "open" => Some(Self::Open),
            "allowlist" => Some(Self::Allowlist),
            _ => None,
        }
    }
}

impl Principal {
    pub fn storage_key(&self) -> String {
        match self {
            Self::LocalDevice(id) => format!("device:{id}"),
            Self::Atproto(did) => format!("atproto:{did}"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplayName(String);

impl DisplayName {
    /// Validate against the glyphs the built-in 5x7 font can render today.
    /// Keeping this deliberately small makes spoofing and collision behavior
    /// deterministic until the renderer gains a real Unicode font.
    pub fn parse(value: &str) -> Result<Self, IdentityError> {
        if value
            .chars()
            .any(|ch| ch.is_ascii_whitespace() && ch != ' ')
        {
            return Err(IdentityError::DisplayName);
        }
        let collapsed = value.split_ascii_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed.is_empty() || collapsed.chars().count() > DISPLAY_NAME_MAX {
            return Err(IdentityError::DisplayName);
        }
        if !collapsed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '-' | '.'))
        {
            return Err(IdentityError::DisplayName);
        }
        Ok(Self(collapsed.to_ascii_uppercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn collision_key(&self) -> String {
        self.0.to_ascii_lowercase()
    }
}

impl fmt::Display for DisplayName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentityError {
    Did,
    DisplayName,
    Signature,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Did => "invalid ATProto DID",
            Self::DisplayName => "invalid display name",
            Self::Signature => "invalid identity signature",
        })
    }
}

impl std::error::Error for IdentityError {}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn decode_hex<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N * 2 || !value.is_ascii() {
        return None;
    }
    let mut out = [0; N];
    for (i, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let hi = hex_nibble(pair[0])?;
        let lo = hex_nibble(pair[1])?;
        out[i] = hi << 4 | lo;
    }
    Some(out)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
