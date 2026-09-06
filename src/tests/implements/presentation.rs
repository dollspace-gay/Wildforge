//! Presentation scenarios.

use super::*;

#[test]
fn authored_implement_art_and_remote_or_removed_component_fallbacks_are_legible() {
    use crate::implements::{
        IMPLEMENT_RESOLVER_VERSION, ImplementKind, ImplementPublicState, ResolvedWand, WandParts,
    };

    let reg = base_reg();
    for (finished, ancestor, file) in [
        (
            "base:charm_quiet",
            "base:quiet_charm_blank",
            "crafted_charm_quiet.png",
        ),
        (
            "base:charm_bark",
            "base:bark_charm_blank",
            "crafted_charm_bark.png",
        ),
        (
            "base:charm_hunger",
            "base:hunger_charm_blank",
            "crafted_charm_hunger.png",
        ),
        ("base:bound_wand", "base:stick", "bound_wand.png"),
    ] {
        assert_ne!(
            reg.item(it(&reg, finished)).icon,
            reg.item(it(&reg, ancestor)).icon,
            "{finished} still uses its procedural ancestor art"
        );
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("base/textures")
            .join(file);
        let metadata = std::fs::metadata(path).unwrap();
        assert!(metadata.len() > 64, "authored implement texture is empty");
    }

    // Public implement state is enough to draw a remote held wand. Removed
    // mod components retain their stable ids/stats, while each missing visual
    // role falls back to a base silhouette rather than hiding the wand.
    let mut world = ReplicaWorld::new(0, reg.clone(), 0.0);
    let instance_id = 77;
    world
        .observations_mut()
        .extend_charges(vec![(instance_id, 61)]);
    world
        .observations_mut()
        .extend_implements(vec![ImplementPublicState {
            instance_id,
            kind: ImplementKind::Wand {
                parts: WandParts {
                    body: "removedmod:body".into(),
                    reservoir: "removedmod:reservoir".into(),
                    focus: "removedmod:focus".into(),
                    binding: "removedmod:binding".into(),
                },
                resolved: ResolvedWand {
                    resolver_version: IMPLEMENT_RESOLVER_VERSION,
                    capacity: 120,
                    safe_transfer: 12,
                    stability: 700,
                    dross_per_thousand: 30,
                    resonance: std::collections::BTreeMap::from([("base:echo".into(), 1)]),
                    heat_sensitive: false,
                    saturation_instability: 0,
                    containment: 200,
                },
            },
            wear: 4,
            strain: 8,
            dross: 2,
        }]);
    let stack = ItemStack {
        arcane_id: instance_id,
        ..ItemStack::new(&reg, it(&reg, "base:bound_wand"), 1)
    };
    let visual = world.implement_visual(stack).unwrap();
    assert_eq!(visual.body, it(&reg, "base:seasoned_wand_body").0);
    assert_eq!(visual.reservoir, it(&reg, "base:ritual_rod_socket").0);
    assert_eq!(visual.focus, it(&reg, "base:echo_slate").0);
    assert_eq!(visual.binding, it(&reg, "base:bronze_wand_binding").0);
    assert_eq!(visual.charge_band, 2);
    assert!(!world.implement_tooltip(stack, true).is_empty());
}
