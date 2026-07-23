use std::collections::HashSet;

use crate::identity::{
    AtprotoDid, DeviceKeyId, DisplayName, LocalIdentity, PlayerId, Principal, random_nonce,
    verify_signature,
};

use super::tmp_dir;

#[test]
fn display_names_match_the_renderer_and_collide_case_insensitively() {
    let name = DisplayName::parse("  moss   keeper ").unwrap();
    assert_eq!(name.as_str(), "MOSS KEEPER");
    assert_eq!(name.collision_key(), "moss keeper");
    assert_eq!(
        DisplayName::parse("Moss Keeper").unwrap().collision_key(),
        name.collision_key()
    );
    for bad in [
        "",
        "   ",
        "name_with_gap",
        "line\nbreak",
        "zero\u{200b}width",
        "é",
        "abcdefghijklmnopq",
    ] {
        assert!(DisplayName::parse(bad).is_err(), "rejected: {bad:?}");
    }
}

#[test]
fn typed_identity_ids_round_trip() {
    let id = PlayerId::random().unwrap();
    assert_eq!(PlayerId::parse(&id.to_string()), Some(id));
    let key = DeviceKeyId::of_public_key(b"public key material");
    assert_eq!(DeviceKeyId::parse(&key.to_string()), Some(key));
    assert_eq!(key.short().len(), 12);

    let did = AtprotoDid::parse("did:plc:abcdefghijklmnopqrstuvwxyz").unwrap();
    let principal = Principal::Atproto(did.clone());
    assert_eq!(principal.storage_key(), format!("atproto:{did}"));
    assert!(AtprotoDid::parse("https://bsky.app/profile/a").is_err());
}

#[test]
fn local_identity_persists_and_proves_key_possession() {
    let dir = tmp_dir("identity-key");
    let first = LocalIdentity::load_or_create(&dir).unwrap();
    let public = first.public_key();
    let id = first.device_id();
    let nonce = random_nonce().unwrap();
    let sig = first.sign(&nonce);
    verify_signature(&public, &nonce, &sig).unwrap();

    let again = LocalIdentity::load_or_create(&dir).unwrap();
    assert_eq!(again.public_key(), public);
    assert_eq!(again.device_id(), id);
    assert!(verify_signature(&public, b"another message", &sig).is_err());

    let mut ids = HashSet::new();
    for _ in 0..32 {
        ids.insert(PlayerId::random().unwrap());
    }
    assert_eq!(ids.len(), 32);
}

#[test]
fn legacy_local_profile_migrates_to_player_id_atomically() {
    let world = tmp_dir("identity-profile-migration");
    let legacy = b"pos = [1.0, 2.0, 3.0]\nyaw = 0.0\npitch = 0.0\nhealth = 14\nhunger = 20\nnutrition = [0,0,0,0,0]\nhotbar = 0\n";
    std::fs::write(world.join("player.toml"), legacy).unwrap();
    let device = DeviceKeyId::of_public_key(b"migration device");
    let path = crate::identity::local_profile_path(&world, device).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), legacy);
    assert!(world.join("player.toml.pre-identity").exists());
    assert!(world.join("players/index.toml").exists());

    crate::identity::atomic_write(&path, b"saved", false).unwrap();
    crate::identity::finish_local_profile_migration(&world);
    assert_eq!(std::fs::read(&path).unwrap(), b"saved");
    assert!(!world.join("player.toml").exists());
    assert!(!world.join("player.toml.pre-identity").exists());
    assert_eq!(
        crate::identity::local_profile_path(&world, device).unwrap(),
        path,
        "the same authenticated device retains its PlayerId"
    );
}

#[test]
fn did_web_path_case_is_not_silently_changed() {
    let did = AtprotoDid::parse("did:web:Example.COM:CaseSensitive").unwrap();
    assert_eq!(did.as_str(), "did:web:Example.COM:CaseSensitive");
}
