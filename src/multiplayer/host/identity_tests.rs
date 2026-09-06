//! Existing host identity and discovery contracts.

use super::discovery_context::discovery_reachable_from;
use super::*;
use crate::identity::{AtprotoDid, DeviceKeyId};

#[test]
fn two_devices_for_one_did_share_an_active_principal() {
    let did = Principal::Atproto(AtprotoDid::parse("did:plc:sharedaccount").unwrap());
    let first = vec![did.clone(), Principal::LocalDevice(DeviceKeyId([1; 32]))];
    let second = vec![did, Principal::LocalDevice(DeviceKeyId([2; 32]))];
    assert!(shares_principal(&first, &second));
    assert!(!shares_principal(
        &first,
        &[Principal::LocalDevice(DeviceKeyId([3; 32]))]
    ));
}

#[test]
fn remote_moderation_permissions_are_host_enforced() {
    assert!(!moderation_action_allowed(
        Role::Player,
        ModerationAction::Kick
    ));
    assert!(moderation_action_allowed(
        Role::Moderator,
        ModerationAction::Kick
    ));
    assert!(moderation_action_allowed(
        Role::Moderator,
        ModerationAction::Mute { seconds: 600 }
    ));
    assert!(moderation_action_allowed(
        Role::Moderator,
        ModerationAction::Ban {
            seconds: Some(3600)
        }
    ));
    assert!(!moderation_action_allowed(
        Role::Moderator,
        ModerationAction::Ban { seconds: None }
    ));
    assert!(!moderation_action_allowed(
        Role::Moderator,
        ModerationAction::CycleRole
    ));
    assert!(moderation_action_allowed(
        Role::Admin,
        ModerationAction::Ban { seconds: None }
    ));
    assert!(moderation_action_allowed(
        Role::Admin,
        ModerationAction::Allow
    ));
    assert!(moderation_action_allowed(
        Role::Admin,
        ModerationAction::CycleRole
    ));
    assert!(!moderation_action_allowed(
        Role::Owner,
        ModerationAction::Mute { seconds: 0 }
    ));
    assert!(!moderation_action_allowed(
        Role::Owner,
        ModerationAction::Ban {
            seconds: Some(86_401)
        }
    ));
}

#[test]
fn discovery_completion_requires_a_matching_host_elapsed_ticket() {
    let began = Instant::now();
    let target = net::DiscoveryTargetSnap::Region;
    let pending = PendingDiscovery {
        kind: PendingDiscoveryKind::Observation(target),
        began,
    };
    assert!(!pending.settled_for(
        PendingDiscoveryKind::Observation(target),
        began + DISCOVERY_SETTLE - Duration::from_millis(1)
    ));
    assert!(!pending.settled_for(
        PendingDiscoveryKind::Observation(net::DiscoveryTargetSnap::Held { slot: 2 }),
        began + DISCOVERY_SETTLE
    ));
    assert!(pending.settled_for(
        PendingDiscoveryKind::Observation(target),
        began + DISCOVERY_SETTLE
    ));
}

#[test]
fn discovery_raycast_refuses_occluded_and_out_of_range_blocks() {
    let reg = std::sync::Arc::new(crate::registry::load(std::path::Path::new("mods")));
    let mut world = World::new(5, std::path::PathBuf::new(), reg.clone());
    let actor =
        EntityPos::from_local(crate::planet::Face::PosZ, Vec3::new(0.5, 80.0, 0.5)).unwrap();
    let target = BlockPos::of_world(0, 81, 3).unwrap();
    world.ensure_chunk(target.chunk());
    world.set_block_at(target, reg.block_id("base:stone").unwrap());
    assert!(discovery_reachable_from(&world, actor, target));

    let wall = BlockPos::of_world(0, 81, 2).unwrap();
    world.set_block_at(wall, reg.block_id("base:stone").unwrap());
    assert!(!discovery_reachable_from(&world, actor, target));

    let far = BlockPos::of_world(0, 81, 12).unwrap();
    world.set_block_at(far, reg.block_id("base:stone").unwrap());
    assert!(!discovery_reachable_from(&world, actor, far));
}

#[test]
fn placed_folio_copying_requires_physical_writing_surface_adjacency() {
    let writing = BlockPos::of_world(0, 80, 0).unwrap();
    assert!(discovery_holder_at_writing_surface(
        net::RecordHolderSnap::Inventory { slot: 2 },
        writing
    ));
    assert!(discovery_holder_at_writing_surface(
        net::RecordHolderSnap::Folio {
            pos: BlockPos::of_world(1, 80, 0).unwrap(),
        },
        writing
    ));
    assert!(!discovery_holder_at_writing_surface(
        net::RecordHolderSnap::Folio {
            pos: BlockPos::of_world(2, 80, 0).unwrap(),
        },
        writing
    ));
}
