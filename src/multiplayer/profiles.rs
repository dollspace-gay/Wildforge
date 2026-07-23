//! Server-owned multiplayer player profiles.

use std::collections::HashMap;
use std::io;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::identity::{DisplayName, PlayerId, Principal};
use crate::inventory::{HOTBAR_SLOTS, Inventory, ItemStack, TOTAL_SLOTS};
use crate::net::{PlayerStateSnap, StackSnap};
use crate::registry::Registry;

#[derive(Serialize, Deserialize)]
struct ProfileIndex {
    version: u32,
    #[serde(default)]
    link: Vec<ProfileLink>,
}

impl Default for ProfileIndex {
    fn default() -> Self {
        Self {
            version: 1,
            link: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct ProfileLink {
    principal: Principal,
    player_id: PlayerId,
}

#[derive(Serialize, Deserialize)]
struct StoredStack {
    index: usize,
    item: String,
    count: u32,
    durability: u32,
}

#[derive(Serialize, Deserialize)]
struct StoredProfile {
    version: u32,
    player_id: PlayerId,
    principals: Vec<Principal>,
    display_name: String,
    #[serde(default)]
    previous_names: Vec<String>,
    pos: [f32; 3],
    yaw: f32,
    pitch: f32,
    spawn: [f32; 3],
    health: f32,
    hunger: f32,
    nutrition: [f32; 5],
    hotbar: usize,
    style: u32,
    held: u16,
    #[serde(default)]
    inventory: Vec<StoredStack>,
    #[serde(default)]
    armor: Vec<StoredStack>,
    cursor: Option<StoredStack>,
    first_seen: u64,
    last_seen: u64,
    /// Time of the write represented by this file. Because the file is
    /// atomically replaced, a stored value is necessarily the last successful
    /// profile save.
    #[serde(default)]
    last_saved_at: u64,
}

#[derive(Deserialize)]
struct LegacyLocalProfile {
    pos: [f32; 3],
    yaw: f32,
    pitch: f32,
    health: f32,
    hunger: f32,
    nutrition: [f32; 5],
    hotbar: usize,
    spawn: Option<[f32; 3]>,
    #[serde(default)]
    slot: Vec<StoredStack>,
    #[serde(default)]
    armor: Vec<StoredStack>,
}

pub(super) struct PlayerRuntime {
    pub player_id: PlayerId,
    pub principals: Vec<Principal>,
    pub display_name: String,
    pub previous_names: Vec<String>,
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub spawn: Vec3,
    pub health: f32,
    pub hunger: f32,
    pub nutrition: [f32; 5],
    pub hotbar: usize,
    pub style: u32,
    pub held: u16,
    pub inventory: Inventory,
    pub armor: [Option<ItemStack>; 5],
    pub cursor: Option<ItemStack>,
    pub first_seen: u64,
}

impl PlayerRuntime {
    pub fn from_guest(guest: &super::Guest) -> Self {
        Self {
            player_id: guest.player_id,
            principals: guest.principals.clone(),
            display_name: guest.name.clone(),
            previous_names: guest.previous_names.clone(),
            pos: guest.pos,
            yaw: guest.yaw,
            pitch: guest.pitch,
            spawn: guest.spawn,
            health: guest.health,
            hunger: guest.hunger,
            nutrition: guest.nutrition,
            hotbar: guest.hotbar,
            style: guest.style,
            held: guest.held,
            inventory: clone_inventory(&guest.inventory),
            armor: guest.armor,
            cursor: guest.cursor,
            first_seen: guest.first_seen,
        }
    }

    pub fn to_snap(&self) -> PlayerStateSnap {
        PlayerStateSnap {
            pos: self.pos,
            yaw: self.yaw,
            pitch: self.pitch,
            spawn: self.spawn,
            health: self.health,
            hunger: self.hunger,
            nutrition: self.nutrition,
            hotbar: self.hotbar as u8,
            inventory: self.inventory.slots.iter().map(stack_snap).collect(),
            armor: self.armor.iter().map(stack_snap).collect(),
            cursor: stack_snap(&self.cursor),
        }
    }
}

pub(super) struct ProfileStore {
    root: PathBuf,
    by_principal: HashMap<String, PlayerId>,
    index_dirty: bool,
    // Populated by the first open and used only for best-effort Drop/kick
    // saves where the caller cannot conveniently pass the active registry.
    registry: Option<std::sync::Arc<Registry>>,
}

impl ProfileStore {
    pub fn load(world: PathBuf) -> io::Result<Self> {
        let root = world.join("players");
        std::fs::create_dir_all(&root)?;
        let index_path = root.join("index.toml");
        let index = match std::fs::read_to_string(&index_path) {
            Ok(text) => toml::from_str::<ProfileIndex>(&text).map_err(invalid_data)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => ProfileIndex::default(),
            Err(e) => return Err(e),
        };
        if index.version != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported player index version {}", index.version),
            ));
        }
        let mut store = Self {
            root,
            by_principal: index
                .link
                .into_iter()
                .map(|link| (link.principal.storage_key(), link.player_id))
                .collect(),
            index_dirty: false,
            registry: None,
        };
        if store.repair_index_from_profiles()? {
            store.save_index()?;
        }
        Ok(store)
    }

    pub fn open_or_create(
        &mut self,
        principals: &[Principal],
        display_name: &DisplayName,
        style: u32,
        spawn: Vec3,
        reg: &std::sync::Arc<Registry>,
    ) -> io::Result<PlayerRuntime> {
        self.registry = Some(reg.clone());
        if principals.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a player profile needs an authenticated principal",
            ));
        }
        let mut existing: Vec<PlayerId> = principals
            .iter()
            .filter_map(|principal| self.by_principal.get(&principal.storage_key()).copied())
            .collect();
        existing.sort_by_key(|id| id.0);
        existing.dedup();
        if existing.len() > 1 {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "the verified account and device belong to different profiles; an operator must merge them",
            ));
        }
        let player_id = match existing.first().copied() {
            Some(id) => id,
            None => PlayerId::random()?,
        };
        let path = self.profile_path(player_id);
        let mut runtime = match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<StoredProfile>(&text) {
                Ok(stored) => stored_to_runtime(stored, reg)?,
                Err(_) => legacy_local_to_runtime(
                    toml::from_str::<LegacyLocalProfile>(&text).map_err(invalid_data)?,
                    player_id,
                    principals,
                    display_name,
                    style,
                    reg,
                ),
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => PlayerRuntime {
                player_id,
                principals: principals.to_vec(),
                display_name: display_name.to_string(),
                previous_names: Vec::new(),
                pos: spawn,
                yaw: 0.0,
                pitch: 0.0,
                spawn,
                health: 14.0,
                hunger: 20.0,
                nutrition: [0.0; 5],
                hotbar: 0,
                style,
                held: u16::MAX,
                inventory: Inventory::new(),
                armor: [None; 5],
                cursor: None,
                first_seen: now(),
            },
            Err(e) => return Err(e),
        };
        if runtime.player_id != player_id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "player index/profile id mismatch",
            ));
        }
        for principal in principals {
            if !runtime.principals.contains(principal) {
                runtime.principals.push(principal.clone());
            }
        }
        if runtime.display_name != display_name.as_str() {
            if !runtime.previous_names.contains(&runtime.display_name) {
                runtime.previous_names.push(runtime.display_name.clone());
                runtime.previous_names.truncate(8);
            }
            runtime.display_name = display_name.to_string();
        }
        runtime.style = style;
        // Persist the profile and its complete principal set first. If the
        // subsequent index write is interrupted, load() reconstructs the
        // missing links from this authoritative profile instead of assigning
        // a second PlayerId on reconnect.
        self.save(&runtime, reg)?;
        let mut index_changed = false;
        for principal in principals {
            let key = principal.storage_key();
            if self.by_principal.get(&key) != Some(&player_id) {
                self.by_principal.insert(key, player_id);
                index_changed = true;
            }
        }
        self.index_dirty |= index_changed;
        if self.index_dirty {
            // Keep the recovered/new mapping in memory if persistence fails:
            // that prevents a retry in this process from inventing another
            // profile. The dirty bit retries the index write on the next open,
            // and load() can always reconstruct it from the durable profile.
            self.save_index()?;
            self.index_dirty = false;
        }
        Ok(runtime)
    }

    pub fn save(&self, player: &PlayerRuntime, reg: &Registry) -> io::Result<()> {
        let stored = runtime_to_stored(player, reg);
        let text = toml::to_string_pretty(&stored).map_err(io::Error::other)?;
        crate::identity::atomic_write(&self.profile_path(player.player_id), text.as_bytes(), false)
    }

    pub fn registry_hint(&self) -> &Registry {
        self.registry
            .as_deref()
            .expect("profile registry initialized before guest admission")
    }

    fn save_index(&self) -> io::Result<()> {
        let mut link: Vec<ProfileLink> = self
            .by_principal
            .iter()
            .filter_map(|(key, player_id)| {
                parse_principal_key(key).map(|principal| ProfileLink {
                    principal,
                    player_id: *player_id,
                })
            })
            .collect();
        link.sort_by_key(|entry| entry.principal.storage_key());
        let text =
            toml::to_string_pretty(&ProfileIndex { version: 1, link }).map_err(io::Error::other)?;
        crate::identity::atomic_write(&self.root.join("index.toml"), text.as_bytes(), false)
    }

    fn profile_path(&self, id: PlayerId) -> PathBuf {
        self.root.join(format!("{id}.toml"))
    }

    /// Profiles carry their own principal list so an interrupted profile/index
    /// pair can be recovered without inventing a new PlayerId. Stale index
    /// links whose profile is missing are dropped; conflicting ownership is a
    /// hard error requiring operator intervention.
    fn repair_index_from_profiles(&mut self) -> io::Result<bool> {
        let mut changed = false;
        let root = self.root.clone();
        self.by_principal.retain(|_, player_id| {
            let exists = root.join(format!("{player_id}.toml")).is_file();
            changed |= !exists;
            exists
        });
        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if path.file_name().and_then(|name| name.to_str()) == Some("index.toml")
                || path.extension().and_then(|ext| ext.to_str()) != Some("toml")
            {
                continue;
            }
            let text = std::fs::read_to_string(&path)?;
            let Ok(profile) = toml::from_str::<StoredProfile>(&text) else {
                // A legacy local profile has no principal list. Its explicit
                // migration index remains authoritative until first save.
                continue;
            };
            if profile.version != 1 {
                continue;
            }
            let expected = self.profile_path(profile.player_id);
            if path != expected {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "player profile id does not match filename: {}",
                        path.display()
                    ),
                ));
            }
            for principal in profile.principals {
                let key = principal.storage_key();
                match self.by_principal.get(&key) {
                    Some(existing) if *existing != profile.player_id => {
                        return Err(io::Error::new(
                            io::ErrorKind::AlreadyExists,
                            format!("principal {key} is claimed by two player profiles"),
                        ));
                    }
                    Some(_) => {}
                    None => {
                        self.by_principal.insert(key, profile.player_id);
                        changed = true;
                    }
                }
            }
        }
        Ok(changed)
    }
}

fn legacy_local_to_runtime(
    profile: LegacyLocalProfile,
    player_id: PlayerId,
    principals: &[Principal],
    display_name: &DisplayName,
    style: u32,
    reg: &Registry,
) -> PlayerRuntime {
    let mut inventory = Inventory::new();
    for stack in profile.slot {
        if stack.index < TOTAL_SLOTS {
            inventory.slots[stack.index] = restore_stack(&stack, reg);
        }
    }
    let mut armor = [None; 5];
    for stack in profile.armor {
        if stack.index < armor.len() {
            armor[stack.index] = restore_stack(&stack, reg);
        }
    }
    PlayerRuntime {
        player_id,
        principals: principals.to_vec(),
        display_name: display_name.to_string(),
        previous_names: Vec::new(),
        pos: Vec3::from_array(profile.pos),
        yaw: profile.yaw,
        pitch: profile.pitch,
        spawn: Vec3::from_array(profile.spawn.unwrap_or(profile.pos)),
        health: profile.health.clamp(0.0, 14.0),
        hunger: profile.hunger.clamp(0.0, 20.0),
        nutrition: profile.nutrition.map(|value| value.max(0.0)),
        hotbar: profile.hotbar.min(HOTBAR_SLOTS - 1),
        style,
        held: u16::MAX,
        inventory,
        armor,
        cursor: None,
        first_seen: now(),
    }
}

fn runtime_to_stored(player: &PlayerRuntime, reg: &Registry) -> StoredProfile {
    let saved_at = now();
    let stacks = |slots: &[Option<ItemStack>]| {
        slots
            .iter()
            .enumerate()
            .filter_map(|(index, stack)| stored_stack(index, *stack, reg))
            .collect()
    };
    StoredProfile {
        version: 1,
        player_id: player.player_id,
        principals: player.principals.clone(),
        display_name: player.display_name.clone(),
        previous_names: player.previous_names.clone(),
        pos: player.pos.to_array(),
        yaw: player.yaw,
        pitch: player.pitch,
        spawn: player.spawn.to_array(),
        health: player.health,
        hunger: player.hunger,
        nutrition: player.nutrition,
        hotbar: player.hotbar,
        style: player.style,
        held: player.held,
        inventory: stacks(&player.inventory.slots),
        armor: stacks(&player.armor),
        cursor: stored_stack(0, player.cursor, reg),
        first_seen: player.first_seen,
        last_seen: saved_at,
        last_saved_at: saved_at,
    }
}

fn stored_to_runtime(profile: StoredProfile, reg: &Registry) -> io::Result<PlayerRuntime> {
    if profile.version != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported player profile version {}", profile.version),
        ));
    }
    let mut inventory = Inventory::new();
    for stack in profile.inventory {
        if stack.index < TOTAL_SLOTS {
            inventory.slots[stack.index] = restore_stack(&stack, reg);
        }
    }
    let mut armor = [None; 5];
    for stack in profile.armor {
        if stack.index < armor.len() {
            armor[stack.index] = restore_stack(&stack, reg);
        }
    }
    Ok(PlayerRuntime {
        player_id: profile.player_id,
        principals: profile.principals,
        display_name: profile.display_name,
        previous_names: profile.previous_names,
        pos: Vec3::from_array(profile.pos),
        yaw: profile.yaw,
        pitch: profile.pitch,
        spawn: Vec3::from_array(profile.spawn),
        health: profile.health.clamp(0.0, 14.0),
        hunger: profile.hunger.clamp(0.0, 20.0),
        nutrition: profile.nutrition.map(|value| value.max(0.0)),
        hotbar: profile.hotbar.min(HOTBAR_SLOTS - 1),
        style: profile.style,
        held: profile.held,
        inventory,
        armor,
        cursor: profile
            .cursor
            .as_ref()
            .and_then(|stack| restore_stack(stack, reg)),
        first_seen: profile.first_seen,
    })
}

fn stored_stack(index: usize, stack: Option<ItemStack>, reg: &Registry) -> Option<StoredStack> {
    let stack = stack?;
    Some(StoredStack {
        index,
        item: reg.item(stack.item).name.clone(),
        count: stack.count,
        durability: stack.durability,
    })
}

fn restore_stack(stack: &StoredStack, reg: &Registry) -> Option<ItemStack> {
    if stack.count == 0 {
        return None;
    }
    let item = reg.item_id(&stack.item)?;
    Some(ItemStack {
        item,
        count: stack.count.min(reg.item(item).max_stack),
        durability: stack.durability.min(reg.item(item).durability),
    })
}

fn stack_snap(stack: &Option<ItemStack>) -> Option<StackSnap> {
    stack.map(|stack| StackSnap {
        item: stack.item.0,
        count: stack.count,
        durability: stack.durability,
    })
}

fn clone_inventory(inventory: &Inventory) -> Inventory {
    Inventory {
        slots: inventory.slots,
    }
}

pub(super) fn parse_principal_key(key: &str) -> Option<Principal> {
    if let Some(value) = key.strip_prefix("device:") {
        return crate::identity::DeviceKeyId::parse(value).map(Principal::LocalDevice);
    }
    key.strip_prefix("atproto:")
        .and_then(|value| crate::identity::AtprotoDid::parse(value).ok())
        .map(Principal::Atproto)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn invalid_data(error: toml::de::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{AtprotoDid, DeviceKeyId};

    fn fixture(name: &str) -> (PathBuf, std::sync::Arc<Registry>) {
        let root =
            std::env::temp_dir().join(format!("wildforge-profile-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let reg = std::sync::Arc::new(crate::registry::load(Path::new("/nonexistent-mods")));
        (root, reg)
    }

    fn device(seed: u8) -> Principal {
        Principal::LocalDevice(DeviceKeyId([seed; 32]))
    }

    #[test]
    fn reconnect_reopens_one_profile_and_keeps_state_and_name_history() {
        let (root, reg) = fixture("reconnect");
        let principal = device(1);
        let first_id = {
            let mut store = ProfileStore::load(root.clone()).unwrap();
            let mut profile = store
                .open_or_create(
                    std::slice::from_ref(&principal),
                    &DisplayName::parse("Moss").unwrap(),
                    7,
                    Vec3::new(1.0, 2.0, 3.0),
                    &reg,
                )
                .unwrap();
            profile.pos = Vec3::new(8.0, 70.0, 9.0);
            let item = reg.item_id("base:torch").unwrap();
            profile.inventory.slots[0] = Some(ItemStack::new(&reg, item, 3));
            store.save(&profile, &reg).unwrap();
            let stored: StoredProfile = toml::from_str(
                &std::fs::read_to_string(
                    root.join("players")
                        .join(format!("{}.toml", profile.player_id)),
                )
                .unwrap(),
            )
            .unwrap();
            assert!(stored.last_saved_at > 0);
            assert_eq!(stored.last_seen, stored.last_saved_at);
            profile.player_id
        };
        let mut store = ProfileStore::load(root).unwrap();
        let profile = store
            .open_or_create(
                &[principal],
                &DisplayName::parse("Fern").unwrap(),
                8,
                Vec3::ZERO,
                &reg,
            )
            .unwrap();
        assert_eq!(profile.player_id, first_id);
        assert_eq!(profile.pos, Vec3::new(8.0, 70.0, 9.0));
        assert_eq!(profile.inventory.slots[0].unwrap().count, 3);
        assert_eq!(profile.display_name, "FERN");
        assert!(profile.previous_names.contains(&"MOSS".to_string()));
    }

    #[test]
    fn atproto_link_attaches_but_conflicting_profiles_do_not_merge() {
        let (root, reg) = fixture("links");
        let did = Principal::Atproto(AtprotoDid::parse("did:plc:accountone").unwrap());
        let mut store = ProfileStore::load(root).unwrap();
        let first = store
            .open_or_create(
                &[device(2)],
                &DisplayName::parse("One").unwrap(),
                0,
                Vec3::ZERO,
                &reg,
            )
            .unwrap();
        let linked = store
            .open_or_create(
                &[did.clone(), device(2)],
                &DisplayName::parse("One").unwrap(),
                0,
                Vec3::ZERO,
                &reg,
            )
            .unwrap();
        assert_eq!(linked.player_id, first.player_id);
        let torch = reg.item_id("base:torch").unwrap();
        let mut linked = linked;
        linked.inventory.slots[0] = Some(ItemStack::new(&reg, torch, 5));
        store.save(&linked, &reg).unwrap();
        let second_device = store
            .open_or_create(
                &[did.clone(), device(4)],
                &DisplayName::parse("Renamed").unwrap(),
                1,
                Vec3::ZERO,
                &reg,
            )
            .unwrap();
        assert_eq!(second_device.player_id, first.player_id);
        assert_eq!(second_device.inventory.slots[0].unwrap().count, 5);
        assert_eq!(second_device.display_name, "RENAMED");
        let second = store
            .open_or_create(
                &[device(3)],
                &DisplayName::parse("Two").unwrap(),
                0,
                Vec3::ZERO,
                &reg,
            )
            .unwrap();
        assert_ne!(second.player_id, first.player_id);
        let error = store
            .open_or_create(
                &[did, device(3)],
                &DisplayName::parse("Two").unwrap(),
                0,
                Vec3::ZERO,
                &reg,
            )
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn missing_index_is_rebuilt_from_authoritative_profiles() {
        let (root, reg) = fixture("index-recovery");
        let principal = device(8);
        let player_id = {
            let mut store = ProfileStore::load(root.clone()).unwrap();
            store
                .open_or_create(
                    std::slice::from_ref(&principal),
                    &DisplayName::parse("Moss").unwrap(),
                    0,
                    Vec3::ZERO,
                    &reg,
                )
                .unwrap()
                .player_id
        };
        std::fs::remove_file(root.join("players/index.toml")).unwrap();
        std::fs::write(root.join("players/.index.toml.interrupted.tmp"), "partial").unwrap();

        let mut recovered = ProfileStore::load(root.clone()).unwrap();
        let profile = recovered
            .open_or_create(
                std::slice::from_ref(&principal),
                &DisplayName::parse("Moss").unwrap(),
                0,
                Vec3::ZERO,
                &reg,
            )
            .unwrap();
        assert_eq!(profile.player_id, player_id);
        let index: ProfileIndex =
            toml::from_str(&std::fs::read_to_string(root.join("players/index.toml")).unwrap())
                .unwrap();
        assert!(
            index
                .link
                .iter()
                .any(|link| { link.principal == principal && link.player_id == player_id })
        );
    }

    #[test]
    fn failed_index_write_retries_without_creating_a_second_profile() {
        let (root, reg) = fixture("index-write-retry");
        let principal = device(10);
        let mut store = ProfileStore::load(root.clone()).unwrap();
        let index_path = root.join("players/index.toml");
        std::fs::create_dir(&index_path).unwrap();

        assert!(
            store
                .open_or_create(
                    std::slice::from_ref(&principal),
                    &DisplayName::parse("Moss").unwrap(),
                    0,
                    Vec3::ZERO,
                    &reg,
                )
                .is_err()
        );
        std::fs::remove_dir(&index_path).unwrap();
        let recovered = store
            .open_or_create(
                std::slice::from_ref(&principal),
                &DisplayName::parse("Moss").unwrap(),
                0,
                Vec3::ZERO,
                &reg,
            )
            .unwrap();
        let profile_files = std::fs::read_dir(root.join("players"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.path().extension().and_then(|ext| ext.to_str()) == Some("toml")
                    && entry.file_name() != "index.toml"
            })
            .count();
        assert_eq!(profile_files, 1);
        assert_eq!(
            store.by_principal.get(&principal.storage_key()).copied(),
            Some(recovered.player_id)
        );
    }

    #[test]
    fn legacy_profile_is_upgraded_and_conflicting_index_ownership_is_rejected() {
        let (root, reg) = fixture("schema-recovery");
        let principal = device(9);
        let player_id = PlayerId::random().unwrap();
        let players = root.join("players");
        std::fs::create_dir_all(&players).unwrap();
        let index = ProfileIndex {
            version: 1,
            link: vec![ProfileLink {
                principal: principal.clone(),
                player_id,
            }],
        };
        std::fs::write(
            players.join("index.toml"),
            toml::to_string_pretty(&index).unwrap(),
        )
        .unwrap();
        std::fs::write(
            players.join(format!("{player_id}.toml")),
            "pos = [3.0, 70.0, 4.0]\nyaw = 1.0\npitch = 0.25\nhealth = 11.0\nhunger = 17.0\nnutrition = [1.0, 2.0, 3.0, 4.0, 5.0]\nhotbar = 2\n",
        )
        .unwrap();

        let mut store = ProfileStore::load(root.clone()).unwrap();
        let upgraded = store
            .open_or_create(
                std::slice::from_ref(&principal),
                &DisplayName::parse("Legacy").unwrap(),
                3,
                Vec3::ZERO,
                &reg,
            )
            .unwrap();
        assert_eq!(upgraded.player_id, player_id);
        assert_eq!(upgraded.pos, Vec3::new(3.0, 70.0, 4.0));
        assert_eq!(upgraded.health, 11.0);
        let profile_path = players.join(format!("{player_id}.toml"));
        let text = std::fs::read_to_string(&profile_path).unwrap();
        let mut duplicate: StoredProfile = toml::from_str(&text).unwrap();
        assert_eq!(duplicate.version, 1);

        duplicate.player_id = PlayerId::random().unwrap();
        std::fs::write(
            players.join(format!("{}.toml", duplicate.player_id)),
            toml::to_string_pretty(&duplicate).unwrap(),
        )
        .unwrap();
        let error = ProfileStore::load(root).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    }
}
