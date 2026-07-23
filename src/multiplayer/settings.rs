//! Per-world multiplayer policy loaded before the listening socket opens.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::identity::{AdmissionPolicy, IdentityPolicy};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerSettings {
    pub identity: IdentityPolicy,
    pub admission: AdmissionPolicy,
    pub port: u16,
    /// How long a previously verified ATProto binding may survive a PDS outage.
    pub verification_grace_secs: u64,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            identity: IdentityPolicy::Local,
            admission: AdmissionPolicy::Open,
            port: crate::net::GAME_PORT,
            verification_grace_secs: 3_600,
        }
    }
}

impl ServerSettings {
    pub fn load_or_create(world: &Path) -> io::Result<Self> {
        let path = world.join("server.toml");
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let mut settings: Self = toml::from_str(&text).map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("invalid server.toml: {e}"),
                    )
                })?;
                // Outage tolerance must not turn revocation into a permanent
                // bypass. Seven days is the operator-facing hard ceiling.
                settings.verification_grace_secs =
                    settings.verification_grace_secs.min(7 * 24 * 60 * 60);
                Ok(settings)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let settings = Self::default();
                let text = toml::to_string_pretty(&settings).map_err(io::Error::other)?;
                crate::identity::atomic_write(&path, text.as_bytes(), false)?;
                Ok(settings)
            }
            Err(e) => Err(e),
        }
    }
}
