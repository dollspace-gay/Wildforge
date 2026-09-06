//! Archaeology scenarios.

use super::*;

#[test]
fn brushing_yields_once_and_transmutes() {
    let reg = base_reg();
    // Archaeological finds may carry finite Current, so this fixture needs
    // the same qualified atlas + ledger a production world always owns.
    let mut w = World::load_or_create(tmp_dir("brushing").join("world"), reg.clone()).unwrap();
    w.ensure_chunk(tchunk(0, 0));
    let masonry = reg.block_id("base:cracked_masonry").unwrap();
    w.set_block(3, 150, 3, masonry);
    let mut rng = 7u32;
    let found = w.brush_block(3, 150, 3, &mut rng).expect("artifact found");
    assert!(found.count >= 1);
    if reg.item(found.item).arcane.is_some() {
        assert_ne!(found.arcane_id, 0);
        assert!(
            w.arcane_ledger
                .as_ref()
                .unwrap()
                .item_current_total(found.arcane_id)
                .is_some(),
            "a charged find remains in clean or explicitly sequestered item custody"
        );
    }
    assert_eq!(
        w.get_block(3, 150, 3),
        reg.block_id("base:cobblestone").unwrap(),
        "remnant becomes plain stone"
    );
    assert!(
        w.brush_block(3, 150, 3, &mut rng).is_none(),
        "artifact only once"
    );
    // Breaking a remnant instead just drops cobble (greed loses the find).
    let d = reg.drops_for(masonry, None).unwrap();
    assert_eq!(reg.item(d.0).name, "base:cobblestone");
}

#[test]
fn sealed_archaeological_dross_is_funded_and_materializes_once() {
    use crate::arcane::ArcaneOwner;

    let reg = base_reg();
    let mut world =
        World::load_or_create(tmp_dir("sealed-dross-brush").join("world"), reg.clone()).unwrap();
    world.ensure_chunk(tchunk(0, 0));
    let masonry = reg.block_id("base:cracked_masonry").unwrap();
    let sealed = reg.item_id("base:sealed_dross_ampoule").unwrap();
    let mut rng = 17u32;
    let mut recovered = None;
    for x in 1..15 {
        for z in 1..15 {
            world.set_block(x, 150, z, masonry);
            let stack = world.brush_block(x, 150, z, &mut rng).unwrap();
            if stack.item == sealed {
                recovered = Some((x, z, stack));
                break;
            }
        }
        if recovered.is_some() {
            break;
        }
    }
    let (x, z, stack) = recovered.expect("the deterministic site set includes sealed dross");
    let ledger = world.arcane_ledger.as_ref().unwrap();
    assert!(
        ledger
            .account(&ArcaneOwner::Item(stack.arcane_id))
            .is_none()
    );
    let contained = ledger.item_dross_total(stack.arcane_id);
    assert!(
        contained > 0,
        "the ampoule owns an actual contained reservoir"
    );
    assert!(ledger.audit().unwrap().is_balanced());

    assert!(world.brush_block(x, 150, z, &mut rng).is_none());
    let ledger = world.arcane_ledger.as_ref().unwrap();
    assert_eq!(ledger.item_dross_total(stack.arcane_id), contained);
    assert!(ledger.audit().unwrap().is_balanced());
}
