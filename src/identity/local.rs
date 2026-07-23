//! Local device keys, challenge signing, and local-profile migration.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use ring::digest::{SHA256, digest};
use ring::rand::{SecureRandom, SystemRandom};
use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
use serde::{Deserialize, Serialize};

use super::{DeviceKeyId, IdentityError, NONCE_LEN, PlayerId, Principal};

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
        replace_file(&temp, path)
    })();
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(not(windows))]
fn replace_file(temp: &Path, path: &Path) -> io::Result<()> {
    fs::rename(temp, path)
}

#[cfg(windows)]
fn replace_file(temp: &Path, path: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: both buffers are NUL-terminated UTF-16 paths and remain alive
    // for the duration of the call.
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
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
        #[cfg(unix)]
        let publish = fs::hard_link(&temp, path);
        // Creating a hard link is not supported on every filesystem Windows
        // can launch the game from (including some network, removable, and
        // compatibility filesystems). A same-directory rename is still
        // atomic on Windows and refuses to replace an existing destination.
        #[cfg(windows)]
        let publish = fs::rename(&temp, path);
        #[cfg(not(any(unix, windows)))]
        let publish = fs::hard_link(&temp, path);
        match publish {
            Ok(()) => Ok(()),
            Err(e)
                if e.kind() == io::ErrorKind::AlreadyExists || cfg!(windows) && path.exists() =>
            {
                Ok(())
            }
            Err(e) => Err(e),
        }
    })();
    drop(file);
    let _ = fs::remove_file(&temp);
    result
}
