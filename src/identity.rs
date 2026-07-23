//! Stable local identity and the identifiers used by multiplayer policy.
//!
//! Display names are presentation. A server-owned [`PlayerId`] owns saved
//! progress, while one or more authenticated [`Principal`] values may open it.

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use ring::digest::{SHA256, digest};
use ring::rand::{SecureRandom, SystemRandom};
use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
use serde::{Deserialize, Serialize};

#[path = "identity/atproto.rs"]
pub mod atproto;

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

pub struct LocalIdentity {
    key_pair: Ed25519KeyPair,
    public_key: [u8; 32],
    device_id: DeviceKeyId,
}

impl LocalIdentity {
    pub fn load_or_create(root: &Path) -> io::Result<Self> {
        fs::create_dir_all(root)?;
        let path = root.join("player-ed25519.pk8");
        Self::from_pkcs8(&load_or_create_ed25519_pkcs8(&path)?)
    }

    fn from_pkcs8(bytes: &[u8]) -> io::Result<Self> {
        let key_pair = Ed25519KeyPair::from_pkcs8(bytes)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid identity key"))?;
        let public: [u8; 32] = key_pair
            .public_key()
            .as_ref()
            .try_into()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid public key"))?;
        Ok(Self {
            device_id: DeviceKeyId::of_public_key(&public),
            key_pair,
            public_key: public,
        })
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.public_key
    }

    pub fn device_id(&self) -> DeviceKeyId {
        self.device_id
    }

    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.key_pair.sign(message).as_ref().try_into().unwrap()
    }
}

pub fn verify_signature(
    public_key: &[u8; 32],
    message: &[u8],
    signature: &[u8; 64],
) -> Result<(), IdentityError> {
    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(message, signature)
        .map_err(|_| IdentityError::Signature)
}

pub fn random_nonce() -> io::Result<[u8; NONCE_LEN]> {
    let mut nonce = [0; NONCE_LEN];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| io::Error::other("secure random generation failed"))?;
    Ok(nonce)
}

pub fn identity_dir() -> PathBuf {
    PathBuf::from("identity")
}

#[derive(Default, Serialize, Deserialize)]
struct LocalProfileIndex {
    version: u32,
    #[serde(default)]
    link: Vec<LocalProfileLink>,
}

#[derive(Serialize, Deserialize)]
struct LocalProfileLink {
    principal: Principal,
    player_id: PlayerId,
}

/// Return the authenticated local player's server-shaped profile path,
/// migrating the historical `player.toml` without using its display name.
/// The original remains until a new profile save succeeds.
pub fn local_profile_path(world: &Path, device: DeviceKeyId) -> io::Result<PathBuf> {
    let players = world.join("players");
    fs::create_dir_all(&players)?;
    let index_path = players.join("index.toml");
    let mut index = match fs::read_to_string(&index_path) {
        Ok(text) => toml::from_str::<LocalProfileIndex>(&text).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid player index: {error}"),
            )
        })?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => LocalProfileIndex {
            version: 1,
            link: Vec::new(),
        },
        Err(error) => return Err(error),
    };
    if index.version > 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsupported player index version",
        ));
    }
    index.version = 1;
    let principal = Principal::LocalDevice(device);
    let player_id = match index
        .link
        .iter()
        .find(|link| link.principal == principal)
        .map(|link| link.player_id)
    {
        Some(player_id) => player_id,
        None => {
            let player_id = PlayerId::random()?;
            index.link.push(LocalProfileLink {
                principal,
                player_id,
            });
            index.link.sort_by_key(|link| link.principal.storage_key());
            let text = toml::to_string_pretty(&index).map_err(io::Error::other)?;
            atomic_write(&index_path, text.as_bytes(), false)?;
            player_id
        }
    };
    let target = players.join(format!("{player_id}.toml"));
    let legacy = world.join("player.toml");
    if !target.exists() && legacy.exists() {
        let bytes = fs::read(&legacy)?;
        atomic_write(&world.join("player.toml.pre-identity"), &bytes, false)?;
        atomic_write(&target, &bytes, false)?;
    }
    Ok(target)
}

/// Called only after the new PlayerId-keyed file has been fsynced.
pub fn finish_local_profile_migration(world: &Path) {
    for path in [
        world.join("player.toml"),
        world.join("player.toml.pre-identity"),
    ] {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => eprintln!("identity: could not finish profile migration: {error}"),
        }
    }
}

pub(crate) fn load_or_create_ed25519_pkcs8(path: &Path) -> io::Result<Vec<u8>> {
    match fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let doc = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
                .map_err(|_| io::Error::other("identity key generation failed"))?;
            atomic_create_secret(path, doc.as_ref())?;
            fs::read(path)
        }
        Err(e) => Err(e),
    }
}

pub(crate) fn sha256(bytes: &[u8]) -> [u8; 32] {
    let hash = digest(&SHA256, bytes);
    hash.as_ref().try_into().unwrap()
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8], secret: bool) -> io::Result<()> {
    #[cfg(not(unix))]
    let _ = secret;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| io::Error::other("invalid output path"))?;
    let temp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    if secret {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn atomic_create_secret(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| io::Error::other("invalid identity path"))?;
    let temp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        match fs::hard_link(&temp, path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e),
        }
    })();
    drop(file);
    let _ = fs::remove_file(&temp);
    result
}

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
