//! Funded offerings scenarios.

use super::*;

#[test]
fn charged_offering_returns_exact_current_to_its_country_heart() {
    use crate::arcane::ArcaneOwner;
    use crate::world::{BlockEntity, OfferingState};

    let (reg, mut world) = charged_offering_world("charged-country-offering", 42);
    // A heart holding at least two bindings stays open while the gift is out
    // on loan, so the mid-loan balance below is observable. The drained case
    // is covered by offering_revives_a_country_heart_drained_by_its_own_gift.
    let (surface, country, heart_before) = country_heart_holding(&world, |total| total >= 512)
        .expect("fixture seed 42 offers a country heart holding at least two bindings");
    let at = crate::planet::BlockPos::new(surface.face(), surface.u(), 100, surface.v()).unwrap();
    let heart = ArcaneOwner::Heart(country);
    let mut gift = ItemStack::new(&reg, it(&reg, "base:thorn_fiber"), 1);
    world
        .bind_arcane_stack_at(at, &mut gift, "charged offering fixture")
        .unwrap();
    let gift_owner = ArcaneOwner::Item(gift.arcane_id);
    assert_eq!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&heart)
            .unwrap()
            .current
            .total(),
        heart_before - 256
    );
    let mut offering = OfferingState::default();
    offering.slots[0] = Some(gift);
    world.insert_block_entity_at(at, BlockEntity::Offering(offering));
    assert!(world.accept_offerings() > 0.0);
    let ledger = world.arcane_ledger.as_ref().unwrap();
    assert!(ledger.account(&gift_owner).is_none());
    assert_eq!(
        ledger.account(&heart).unwrap().current.total(),
        heart_before
    );
    assert!(ledger.audit().unwrap().is_balanced());
}

#[test]
fn offering_revives_a_country_heart_drained_by_its_own_gift() {
    use crate::arcane::ArcaneOwner;
    use crate::world::{BlockEntity, OfferingState};

    // Seed 24's first country is a single atlas cell, so its heart holds
    // exactly one binding (256 units): lending the gift drains it to zero
    // and the ledger prunes the empty account. Accepting the offering must
    // still return the exact current and revive the heart.
    let (reg, mut world) = charged_offering_world("charged-offering-drained-heart", 24);
    let (surface, country, heart_before) = country_heart_holding(&world, |total| total == 256)
        .expect("fixture seed 24 offers a one-cell country heart");
    let at = crate::planet::BlockPos::new(surface.face(), surface.u(), 100, surface.v()).unwrap();
    let heart = ArcaneOwner::Heart(country);
    let mut gift = ItemStack::new(&reg, it(&reg, "base:thorn_fiber"), 1);
    world
        .bind_arcane_stack_at(at, &mut gift, "drained offering fixture")
        .unwrap();
    let gift_owner = ArcaneOwner::Item(gift.arcane_id);
    assert!(
        world
            .arcane_ledger
            .as_ref()
            .unwrap()
            .account(&heart)
            .is_none(),
        "a heart drained to zero is pruned while its gift is out on loan"
    );
    let mut offering = OfferingState::default();
    offering.slots[0] = Some(gift);
    world.insert_block_entity_at(at, BlockEntity::Offering(offering));
    assert!(world.accept_offerings() > 0.0);
    let ledger = world.arcane_ledger.as_ref().unwrap();
    assert!(ledger.account(&gift_owner).is_none());
    assert_eq!(
        ledger.account(&heart).unwrap().current.total(),
        heart_before
    );
    assert!(ledger.audit().unwrap().is_balanced());
}
