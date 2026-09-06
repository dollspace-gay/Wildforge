//! Forced wand draw crosses the safe floor visibly and is audited scenarios.

use super::*;

#[test]
fn forced_wand_draw_crosses_the_safe_floor_visibly_and_is_audited() {
    use crate::arcane::ArcaneOwner;

    fn prepared(tag: &str) -> (World, crate::planet::BlockPos, ItemStack) {
        let (mut world, source, wand) = workings_world(tag);
        let region = world.planet_atlas().unwrap().atlas_pos(source.surface());
        move_fixture_current_to_deep(
            &mut world,
            ArcaneOwner::Item(wand.arcane_id),
            crate::implements::STRUCTURAL_SPARK_UNITS,
        );
        move_fixture_current_to_deep(&mut world, ArcaneOwner::Ambient(region), 0);
        (world, source, wand)
    }

    let (mut safe_world, safe_source, safe_wand) = prepared("workings-safe-draw");
    let safe_total = safe_world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .total;
    let safe = safe_world
        .begin_gleam_working(
            [31; 16],
            "safe-draw",
            safe_source,
            safe_wand.arcane_id,
            safe_source,
            1,
            20,
            false,
        )
        .unwrap();
    let safe_transaction =
        safe_world.workings_state.as_ref().unwrap().active[&safe.stable_id].clone();
    let safe_region = safe_world
        .planet_atlas()
        .unwrap()
        .atlas_pos(safe_source.surface());
    assert!(!safe_transaction.forced);
    assert_eq!(
        safe_world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::Ambient(safe_region))
            .unwrap()
            .current
            .total(),
        64,
        "an ordinary draw must preserve the measured local floor"
    );

    let (mut forced_world, forced_source, forced_wand) = prepared("workings-forced-draw");
    let forced_total = forced_world
        .arcane_ledger
        .as_ref()
        .unwrap()
        .audit()
        .unwrap()
        .total;
    let forced = forced_world
        .begin_gleam_working(
            [32; 16],
            "forced-draw",
            forced_source,
            forced_wand.arcane_id,
            forced_source,
            1,
            20,
            true,
        )
        .unwrap();
    let forced_transaction =
        forced_world.workings_state.as_ref().unwrap().active[&forced.stable_id].clone();
    let forced_region = forced_world
        .planet_atlas()
        .unwrap()
        .atlas_pos(forced_source.surface());
    assert!(forced_transaction.forced);
    assert!(forced_transaction.strain.strain > safe_transaction.strain.strain);
    assert!(forced_transaction.dross_current.total() > safe_transaction.dross_current.total());
    assert!(forced_transaction.strain.warning_band >= 2);
    assert!(
        forced_world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&ArcaneOwner::Ambient(forced_region))
            .is_none(),
        "forced draw must actually cross rather than silently refill the floor"
    );

    settle_channel(&mut forced_world, forced.stable_id);
    forced_world.release_working(forced.stable_id).unwrap();
    let audit = forced_world
        .workings_state
        .as_ref()
        .unwrap()
        .history
        .back()
        .unwrap();
    assert!(audit.forced);
    assert_eq!(audit.warning_band, forced_transaction.strain.warning_band);
    assert_eq!(
        forced_world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        forced_total
    );
    settle_channel(&mut safe_world, safe.stable_id);
    safe_world.release_working(safe.stable_id).unwrap();
    assert_eq!(
        safe_world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .audit()
            .unwrap()
            .total,
        safe_total
    );

    let mut refusals = Vec::new();
    for tag in ["workings-forced-refusal-a", "workings-forced-refusal-b"] {
        let (mut world, source, wand) = prepared(tag);
        let total = world.arcane_ledger.as_ref().unwrap().audit().unwrap().total;
        let instance = world
            .implements_state
            .as_mut()
            .unwrap()
            .instances
            .get_mut(&wand.arcane_id)
            .unwrap();
        instance.wear = crate::implements::MAX_WAND_WEAR - 1;
        instance.strain = crate::implements::MAX_WAND_STRAIN - 1;
        let refusal = world
            .begin_gleam_working(
                [44; 16],
                "forced-refusal",
                source,
                wand.arcane_id,
                source,
                8,
                20,
                true,
            )
            .unwrap_err();
        assert!(refusal.contains("forced overdraw"), "{refusal}");
        assert!(world.workings_state.as_ref().unwrap().active.is_empty());
        assert_eq!(
            world.arcane_ledger.as_ref().unwrap().audit().unwrap().total,
            total
        );
        refusals.push(refusal);
    }
    assert_eq!(refusals[0], refusals[1]);
}
