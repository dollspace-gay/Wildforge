//! Durable, principal-based admission and moderation state.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::identity::{AdmissionPolicy, PlayerId, Principal};
use crate::net::{Refusal, RefusalCode};

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
    #[cfg(test)]
    pub fn can_moderate(self) -> bool {
        matches!(self, Self::Owner | Self::Admin | Self::Moderator)
    }

    #[cfg(test)]
    pub fn can_administer(self) -> bool {
        matches!(self, Self::Owner | Self::Admin)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BanRecord {
    pub principal: Option<Principal>,
    pub player_id: Option<PlayerId>,
    pub reason: String,
    pub created_at: u64,
    pub created_by: String,
    pub expires_at: Option<u64>,
    pub last_handle: Option<String>,
    pub last_display_name: Option<String>,
}

impl BanRecord {
    fn active(&self, at: u64) -> bool {
        self.expires_at.is_none_or(|expiry| expiry > at)
    }

    fn matches(&self, principals: &[Principal], player_id: Option<PlayerId>) -> bool {
        self.player_id.is_some_and(|id| Some(id) == player_id)
            || self
                .principal
                .as_ref()
                .is_some_and(|principal| principals.contains(principal))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PrincipalRecord {
    principal: Principal,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PlayerRecord {
    player_id: PlayerId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RoleRecord {
    principal: Principal,
    role: Role,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MuteRecord {
    principal: Principal,
    reason: String,
    created_at: u64,
    created_by: String,
    expires_at: Option<u64>,
}

#[derive(Default, Serialize, Deserialize)]
struct BanFile {
    version: u32,
    #[serde(default)]
    ban: Vec<BanRecord>,
}

#[derive(Default, Serialize, Deserialize)]
struct AllowlistFile {
    version: u32,
    #[serde(default)]
    principal: Vec<PrincipalRecord>,
    #[serde(default)]
    player: Vec<PlayerRecord>,
}

#[derive(Default, Serialize, Deserialize)]
struct RoleFile {
    version: u32,
    #[serde(default)]
    grant: Vec<RoleRecord>,
}

#[derive(Default, Serialize, Deserialize)]
struct MuteFile {
    version: u32,
    #[serde(default)]
    mute: Vec<MuteRecord>,
}

pub struct ModerationStore {
    root: PathBuf,
    bans: Vec<BanRecord>,
    allowed_principals: Vec<Principal>,
    allowed_players: Vec<PlayerId>,
    roles: HashMap<String, Role>,
    mutes: Vec<MuteRecord>,
}

impl ModerationStore {
    pub fn load(world: &Path) -> io::Result<Self> {
        let root = world.join("moderation");
        std::fs::create_dir_all(&root)?;
        let bans: BanFile = read_or_default(&root.join("bans.toml"))?;
        let allow: AllowlistFile = read_or_default(&root.join("allowlist.toml"))?;
        let roles: RoleFile = read_or_default(&root.join("roles.toml"))?;
        let mutes: MuteFile = read_or_default(&root.join("mutes.toml"))?;
        for (name, version) in [
            ("bans", bans.version),
            ("allowlist", allow.version),
            ("roles", roles.version),
            ("mutes", mutes.version),
        ] {
            if version > 1 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unsupported {name} schema version {version}"),
                ));
            }
        }
        Ok(Self {
            root,
            bans: bans.ban,
            allowed_principals: allow.principal.into_iter().map(|r| r.principal).collect(),
            allowed_players: allow.player.into_iter().map(|r| r.player_id).collect(),
            roles: roles
                .grant
                .into_iter()
                .map(|r| (r.principal.storage_key(), r.role))
                .collect(),
            mutes: mutes.mute,
        })
    }

    pub fn admit(
        &mut self,
        principals: &[Principal],
        player_id: Option<PlayerId>,
        policy: AdmissionPolicy,
    ) -> Result<(), Refusal> {
        let at = now();
        let before = self.bans.len();
        self.bans.retain(|ban| ban.active(at));
        if self.bans.len() != before {
            let _ = self.save_bans();
        }
        if let Some(ban) = self
            .bans
            .iter()
            .find(|ban| ban.matches(principals, player_id))
        {
            return Err(Refusal::new(
                RefusalCode::Banned,
                format!("banned: {}", ban.reason),
            ));
        }
        if policy == AdmissionPolicy::Allowlist
            && !principals
                .iter()
                .any(|principal| self.allowed_principals.contains(principal))
            && !player_id.is_some_and(|id| self.allowed_players.contains(&id))
        {
            return Err(Refusal::new(
                RefusalCode::NotAllowlisted,
                "this identity is not on the server allowlist",
            ));
        }
        Ok(())
    }

    /// Pre-profile admission can enforce bans by credential but must defer an
    /// allowlist decision: an established player may be allowlisted solely by
    /// its server-owned PlayerId, which is not known until the index is read.
    pub fn check_bans(&mut self, principals: &[Principal]) -> Result<(), Refusal> {
        self.admit(principals, None, AdmissionPolicy::Open)
    }

    pub fn ban(
        &mut self,
        player_id: PlayerId,
        principals: &[Principal],
        display_name: &str,
        reason: &str,
        created_by: &str,
        duration_secs: Option<u64>,
    ) -> io::Result<()> {
        let created_at = now();
        let expires_at = duration_secs.map(|duration| created_at.saturating_add(duration));
        self.bans.retain(|ban| {
            ban.player_id != Some(player_id)
                && ban
                    .principal
                    .as_ref()
                    .is_none_or(|principal| !principals.contains(principal))
        });
        self.bans.push(BanRecord {
            principal: None,
            player_id: Some(player_id),
            reason: clean(reason, 160),
            created_at,
            created_by: clean(created_by, 80),
            expires_at,
            last_handle: None,
            last_display_name: Some(clean(display_name, 32)),
        });
        for principal in principals {
            self.bans.push(BanRecord {
                principal: Some(principal.clone()),
                player_id: Some(player_id),
                reason: clean(reason, 160),
                created_at,
                created_by: clean(created_by, 80),
                expires_at,
                last_handle: None,
                last_display_name: Some(clean(display_name, 32)),
            });
        }
        self.save_bans()?;
        self.audit(created_by, "ban", &player_id.to_string(), reason)
    }

    pub fn unban_player(&mut self, player_id: PlayerId, by: &str) -> io::Result<bool> {
        let before = self.bans.len();
        self.bans.retain(|ban| ban.player_id != Some(player_id));
        let changed = before != self.bans.len();
        if changed {
            self.save_bans()?;
            self.audit(by, "unban", &player_id.to_string(), "")?;
        }
        Ok(changed)
    }

    pub fn allow_principal(&mut self, principal: Principal, by: &str) -> io::Result<()> {
        if !self.allowed_principals.contains(&principal) {
            self.allowed_principals.push(principal.clone());
            self.save_allowlist()?;
            self.audit(by, "allow", &principal.storage_key(), "")?;
        }
        Ok(())
    }

    pub fn allow_player(&mut self, player_id: PlayerId, by: &str) -> io::Result<()> {
        if !self.allowed_players.contains(&player_id) {
            self.allowed_players.push(player_id);
            self.save_allowlist()?;
            self.audit(by, "allow", &player_id.to_string(), "")?;
        }
        Ok(())
    }

    pub fn set_role(&mut self, principal: Principal, role: Role, by: &str) -> io::Result<()> {
        self.roles.insert(principal.storage_key(), role);
        self.save_roles()?;
        self.audit(by, "role", &principal.storage_key(), &format!("{role:?}"))
    }

    pub fn role(&self, principal: &Principal) -> Role {
        self.roles
            .get(&principal.storage_key())
            .copied()
            .unwrap_or_default()
    }

    pub fn mute(
        &mut self,
        principal: Principal,
        reason: &str,
        by: &str,
        duration_secs: Option<u64>,
    ) -> io::Result<()> {
        let created_at = now();
        self.mutes.retain(|record| {
            record.principal != principal || record.expires_at.is_some_and(|e| e <= created_at)
        });
        self.mutes.push(MuteRecord {
            principal: principal.clone(),
            reason: clean(reason, 160),
            created_at,
            created_by: clean(by, 80),
            expires_at: duration_secs.map(|duration| created_at.saturating_add(duration)),
        });
        self.save_mutes()?;
        self.audit(by, "mute", &principal.storage_key(), reason)
    }

    pub fn is_muted(&self, principal: &Principal) -> bool {
        let at = now();
        self.mutes.iter().any(|record| {
            &record.principal == principal && record.expires_at.is_none_or(|expiry| expiry > at)
        })
    }

    fn save_bans(&self) -> io::Result<()> {
        write_toml(
            &self.root.join("bans.toml"),
            &BanFile {
                version: 1,
                ban: self.bans.clone(),
            },
        )
    }

    fn save_allowlist(&self) -> io::Result<()> {
        write_toml(
            &self.root.join("allowlist.toml"),
            &AllowlistFile {
                version: 1,
                principal: self
                    .allowed_principals
                    .iter()
                    .cloned()
                    .map(|principal| PrincipalRecord { principal })
                    .collect(),
                player: self
                    .allowed_players
                    .iter()
                    .copied()
                    .map(|player_id| PlayerRecord { player_id })
                    .collect(),
            },
        )
    }

    fn save_roles(&self) -> io::Result<()> {
        let mut grant: Vec<RoleRecord> = self
            .roles
            .iter()
            .filter_map(|(key, role)| {
                super::profiles::parse_principal_key(key).map(|principal| RoleRecord {
                    principal,
                    role: *role,
                })
            })
            .collect();
        grant.sort_by_key(|entry| entry.principal.storage_key());
        write_toml(
            &self.root.join("roles.toml"),
            &RoleFile { version: 1, grant },
        )
    }

    fn save_mutes(&self) -> io::Result<()> {
        write_toml(
            &self.root.join("mutes.toml"),
            &MuteFile {
                version: 1,
                mute: self.mutes.clone(),
            },
        )
    }

    fn audit(&self, actor: &str, action: &str, target: &str, reason: &str) -> io::Result<()> {
        let path = self.root.join("audit.log");
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(
            file,
            "{}\t{}\t{}\t{}\t{}",
            now(),
            clean(actor, 80),
            clean(action, 40),
            clean(target, 600),
            clean(reason, 160)
        )?;
        file.sync_data()
    }
}

fn read_or_default<T>(path: &Path) -> io::Result<T>
where
    T: Default + for<'de> Deserialize<'de>,
{
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid {}: {e}", path.display()),
            )
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e),
    }
}

fn write_toml(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let text = toml::to_string_pretty(value).map_err(io::Error::other)?;
    crate::identity::atomic_write(path, text.as_bytes(), false)
}

fn clean(value: &str, max: usize) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_control())
        .take(max)
        .collect()
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::DeviceKeyId;

    fn fixture(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "wildforge-moderation-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn allowlist_roles_bans_expiry_and_audit_are_durable() {
        let root = fixture("durable");
        let principal = Principal::LocalDevice(DeviceKeyId([4; 32]));
        let player = PlayerId([5; 16]);
        let mut store = ModerationStore::load(&root).unwrap();
        assert!(
            store
                .admit(
                    std::slice::from_ref(&principal),
                    Some(player),
                    AdmissionPolicy::Allowlist,
                )
                .is_err()
        );
        store.allow_player(player, "owner").unwrap();
        store
            .admit(
                std::slice::from_ref(&principal),
                Some(player),
                AdmissionPolicy::Allowlist,
            )
            .unwrap();
        store
            .set_role(principal.clone(), Role::Moderator, "owner")
            .unwrap();
        assert!(store.role(&principal).can_moderate());
        assert!(!store.role(&principal).can_administer());

        // A zero-duration ban expires immediately; a permanent one survives
        // reload and is removed by PlayerId, including its principal copies.
        store
            .ban(
                player,
                std::slice::from_ref(&principal),
                "MOSS",
                "test",
                "owner",
                Some(0),
            )
            .unwrap();
        store
            .admit(
                std::slice::from_ref(&principal),
                Some(player),
                AdmissionPolicy::Open,
            )
            .unwrap();
        store
            .ban(
                player,
                std::slice::from_ref(&principal),
                "MOSS",
                "spam",
                "owner",
                None,
            )
            .unwrap();
        assert!(
            store
                .admit(
                    std::slice::from_ref(&principal),
                    Some(player),
                    AdmissionPolicy::Open
                )
                .is_err()
        );
        drop(store);

        let mut reloaded = ModerationStore::load(&root).unwrap();
        assert_eq!(reloaded.role(&principal), Role::Moderator);
        assert!(reloaded.unban_player(player, "owner").unwrap());
        reloaded
            .admit(&[principal], Some(player), AdmissionPolicy::Open)
            .unwrap();
        let audit = std::fs::read_to_string(root.join("moderation/audit.log")).unwrap();
        assert!(audit.contains("\tallow\t"));
        assert!(audit.contains("\trole\t"));
        assert!(audit.contains("\tban\t"));
        assert!(audit.contains("\tunban\t"));
    }
}
