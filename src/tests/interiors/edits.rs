//! Edits scenarios.

use super::*;

#[test]
fn structure_break_and_place_round_trip() {
    use crate::world::BlockEntity;

    let rc = base_reg();

    // Build a single‑cell structure, place a block, break it, check drop.
    {
        let mut w = test_world_with("str-break-place", rc.clone());
        w.set_block_at(bp_abs(4080, MY + 50, 4080), b(&rc, "base:stone"));
        let tpl = capture_region(
            &w,
            bp_abs(4080, MY + 50, 4080),
            bp_abs(4080, MY + 50, 4080),
            "s",
        )
        .expect("capture");
        let id = w
            .spawn_structure(&tpl, bp_abs(TU + 10, MY, TV), Rotation::R0)
            .expect("spawn");

        let s = w.local_structure_mut(id).unwrap();
        assert!(s.place_block((5, 0, 0), b(&rc, "base:stone")));
        assert_eq!(s.get_block((5, 0, 0)), b(&rc, "base:stone"));

        let pick = it(&rc, "base:wood_pickaxe");
        let drop = s.break_block((5, 0, 0), Some(pick));
        let Some(drop) = drop else {
            panic!("expected drop")
        };
        assert!(drop.item == rc.item_id("base:cobblestone").unwrap());
        assert_eq!(s.get_block((5, 0, 0)), AIR, "cell cleared");
    }

    // Breaking a firebrick inside a forge shell douses the machine
    // (revalidation).
    {
        let mut w = test_world_with("str-breach-revalidate", rc.clone());
        build_forge(&mut w, &rc, 1, MY, 1);
        let tpl =
            capture_region(&w, bp(0, MY, 0), bp(3, MY + 5, 2), "forge").expect("forge capture");
        let id = w
            .spawn_structure(&tpl, bp_abs(30, MY, 30), Rotation::R0)
            .expect("spawns");

        let s = w.local_structure_mut(id).unwrap();
        assert!(
            rc.machine_kind("base:forge")
                .unwrap_or_default()
                .validate(s, MOUTH)
                .is_some(),
            "shell validates fresh from spawn"
        );

        s.block_entities_mut()
            .insert(MOUTH, BlockEntity::Multiblock(charged_forge(&rc)));
        let matched = rc
            .machine_kind("base:forge")
            .unwrap_or_default()
            .validate(s, MOUTH)
            .expect("still validates");
        light_machine_at(
            s,
            MOUTH,
            rc.machine_kind("base:forge").unwrap_or_default(),
            matched,
        )
        .expect("lights");
        assert_eq!(s.get_block(MOUTH), b(&rc, "base:forge_lit"));

        s.break_block((1, 0, 0), None);
        let Some(BlockEntity::Multiblock(m)) = s.block_entities().get(&MOUTH) else {
            panic!("machine entity survives the break")
        };
        assert!(!m.lit, "the breach douses the forge");
        assert_eq!(
            s.get_block(MOUTH),
            b(&rc, "base:forge"),
            "mouth cools on breach"
        );
        let charge: u32 = m.charge.iter().flatten().map(|s| s.count).sum();
        assert_eq!(charge, 8, "charge is preserved through dousing");

        s.place_block((1, 0, 0), b(&rc, "base:firebrick"));
        assert!(
            rc.machine_kind("base:forge")
                .unwrap_or_default()
                .validate(s, MOUTH)
                .is_some(),
            "repair re-validates"
        );
        let Some(BlockEntity::Multiblock(m)) = s.block_entities().get(&MOUTH) else {
            panic!("machine present after repair")
        };
        assert!(!m.lit, "repair does not auto-relight (matches world)");
    }
}
