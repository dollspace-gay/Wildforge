//! Shared receivers preserve authoritative fields without simulation powers.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use glam::Vec3;

use super::EntitySnapshots;
use crate::client_session::{ContentMap, GuestSession, PresentationRequirement};
use crate::net::{BoltSnap, FallSnap, LooseItemSnap, MobSnap, Snapshot};
use crate::planet::{EntityPos, Face};
use crate::registry;

fn pos() -> EntityPos {
    EntityPos::new(Face::PosZ, 100.0, 80.0, 100.0).unwrap()
}

fn content() -> ContentMap {
    let reg = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
    ContentMap::new(
        Arc::clone(&reg),
        vec!["base:stone".into()],
        vec![reg.items[1].name.clone()],
    )
}

#[test]
fn streams_advance_independently_and_a_new_session_resets_all_of_them() {
    let content = content();
    let mut entities = EntitySnapshots::default();
    for sequence in [90, 0] {
        assert!(
            entities
                .players(Snapshot::whole(sequence, vec![]))
                .is_some()
        );
        assert!(
            entities
                .mobs(Snapshot::whole(sequence + 5, vec![]), content.registry())
                .is_some()
        );
        assert!(
            entities
                .bolts(Snapshot::whole(sequence + 10, vec![]))
                .is_some()
        );
        assert!(
            entities
                .loose_items(Snapshot::whole(sequence + 15, vec![]), &content)
                .is_some()
        );
        assert!(
            entities
                .falling(Snapshot::whole(sequence + 20, vec![]), &content)
                .is_some()
        );
        entities = EntitySnapshots::default();
    }
}

#[test]
fn mobs_wait_for_complete_snapshots_preserve_host_fields_and_reject_unknown_species() {
    let content = content();
    let snapshot = MobSnap {
        id: 847,
        species: 0,
        pos: pos(),
        yaw: 0.3,
        growth: 0.6,
        hurt: 0.4,
        health: 7.25,
        fed: true,
    };
    let mut entities = EntitySnapshots::default();
    assert!(
        entities
            .mobs(
                Snapshot {
                    seq: 9,
                    part: 1,
                    parts: 2,
                    items: vec![MobSnap {
                        species: u16::MAX,
                        ..snapshot.clone()
                    }],
                },
                content.registry()
            )
            .is_none()
    );
    let mobs = entities
        .mobs(
            Snapshot {
                seq: 9,
                part: 0,
                parts: 2,
                items: vec![snapshot.clone()],
            },
            content.registry(),
        )
        .unwrap();
    assert_eq!(mobs.len(), 1);
    let mob = &mobs[0];
    assert_eq!(mob.id, snapshot.id);
    assert_eq!(mob.species, usize::from(snapshot.species));
    assert_eq!(mob.pos, snapshot.pos);
    assert_eq!(mob.yaw, snapshot.yaw);
    assert_eq!(mob.growth, snapshot.growth);
    assert_eq!(mob.health, snapshot.health);
    assert_eq!(mob.hurt_flash, snapshot.hurt);
    assert_eq!(mob.fed, snapshot.fed);
    assert!(
        entities
            .mobs(Snapshot::whole(9, vec![]), content.registry())
            .is_none()
    );
    assert!(
        entities
            .mobs(Snapshot::whole(10, vec![]), content.registry())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn projectile_replicas_keep_motion_and_identity_without_authoritative_damage_or_drops() {
    let snapshot = BoltSnap {
        id: 719,
        pos: pos(),
        vel: Vec3::new(1.0, -2.0, 3.0),
        tile: 8,
        age: 1.5,
    };
    let mut entities = EntitySnapshots::default();
    let projectiles = entities
        .bolts(Snapshot::whole(10, vec![snapshot.clone()]))
        .unwrap();
    let projectile = &projectiles[0];
    assert_eq!(projectile.stable_id, snapshot.id);
    assert_eq!(projectile.pos, snapshot.pos);
    assert_eq!(projectile.vel, snapshot.vel);
    assert_eq!(projectile.tile, snapshot.tile);
    assert_eq!(projectile.age, snapshot.age);
    assert_eq!(projectile.damage, 0.0);
    assert!(projectile.damage_type.is_none());
    assert!(projectile.drop_item.is_none());
    assert!(projectile.preparation_payload.is_none());
    assert!(!projectile.from_player);
    assert_eq!(projectile.owner, 0);
}

#[test]
fn loose_items_and_falling_blocks_use_the_session_content_map() {
    let content = content();
    let mut entities = EntitySnapshots::default();
    let snapshot = LooseItemSnap {
        id: 53,
        pos: pos(),
        vel: Vec3::X,
        item: 0,
        count: 17,
        age: 3.0,
        durability: 0,
        arcane_id: 875,
    };
    let items = entities
        .loose_items(
            Snapshot::whole(
                2,
                vec![
                    snapshot.clone(),
                    LooseItemSnap {
                        item: u16::MAX,
                        ..snapshot.clone()
                    },
                ],
            ),
            &content,
        )
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].item, content.item(0).unwrap());
    assert_eq!(items[0].stable_id, snapshot.id);
    assert_eq!(items[0].arcane_id, snapshot.arcane_id);
    assert_eq!(items[0].count, snapshot.count);
    let falling = entities
        .falling(
            Snapshot::whole(
                4,
                vec![
                    FallSnap {
                        pos: pos(),
                        block: 0,
                    },
                    FallSnap {
                        pos: pos(),
                        block: u16::MAX,
                    },
                ],
            ),
            &content,
        )
        .unwrap();
    assert_eq!(falling[0].block, content.block(0));
    assert_eq!(falling[0].pos, pos());
    assert_eq!(falling[0].vel, 0.0);
    assert_eq!(falling[1].block, content.registry().unknown_block);
}

#[test]
fn entity_streams_require_welcome_and_stop_when_the_session_closes() {
    let reg = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
    let mut session = GuestSession::new(
        Arc::clone(&reg),
        PresentationRequirement::TerrainOnly,
        Instant::now(),
    );
    for welcomed in [false, true, false] {
        if welcomed {
            session.begin(
                ContentMap::empty(Arc::clone(&reg)),
                "fixture".into(),
                pos(),
                Instant::now(),
            );
        }
        assert_eq!(
            session.players(Snapshot::whole(0, vec![])).is_some(),
            welcomed
        );
        assert_eq!(session.mobs(Snapshot::whole(0, vec![])).is_some(), welcomed);
        assert_eq!(
            session.bolts(Snapshot::whole(0, vec![])).is_some(),
            welcomed
        );
        assert_eq!(
            session.loose_items(Snapshot::whole(0, vec![])).is_some(),
            welcomed
        );
        assert_eq!(
            session.falling(Snapshot::whole(0, vec![])).is_some(),
            welcomed
        );
        session.close();
    }
}
