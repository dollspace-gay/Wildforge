//! Derived physical salvage definitions and bounded material recovery chains.

use super::{ForgeSalvageDef, Ingredient, RecipeDef, ItemDef, ItemId, MaterialClass, MaterialVector, Registry};

fn split_recovery(
    materials: &MaterialVector,
    recovery_permille: u16,
) -> (MaterialVector, MaterialVector) {
    let mut recovered = MaterialVector::new();
    let mut remainder = MaterialVector::new();
    for (material, units) in materials {
        let keep = units.saturating_mul(u64::from(recovery_permille)) / 1000;
        if keep != 0 {
            recovered.insert(material.clone(), keep);
        }
        if *units != keep {
            remainder.insert(material.clone(), units - keep);
        }
    }
    (recovered, remainder)
}

fn push_salvage_item(
    reg: &mut Registry,
    source: &ItemDef,
    suffix: &str,
    label_prefix: &str,
    materials: MaterialVector,
) -> ItemId {
    let item = ItemId(reg.items.len() as u16);
    let name = format!("{}/{suffix}", source.name);
    reg.items.push(ItemDef {
        name: name.clone(),
        label: format!("{label_prefix} {}", source.label),
        icon: source.icon,
        max_stack: 64,
        tool: None,
        durability: 0,
        places: None,
        food: None,
        damage: 1.0,
        damage_type: None,
        bow: None,
        ammo: None,
        armor: None,
        carry_weight: 1,
        stats: Vec::new(),
        frame: None,
        component: None,
        bedroll: false,
        shears: false,
        charm: None,
        charm_def: None,
        wand_component: None,
        implement: None,
        tablet: false,
        striker: false,
        creative_only: false,
        brush_tool: false,
        throw_speed: None,
        hammer: false,
        hack: false,
        glow: None,
        materials,
        materials_declared: true,
        material_class: MaterialClass::GeologicallyFinite,
        salvage: None,
        broken_into: None,
        arcane: None,
        arcane_ecology: None,
        observation: None,
        discovery: None,
    });
    reg.item_by_name.insert(name, item);
    item
}

pub(super) fn register_salvage_content(reg: &mut Registry) {
    let durable = reg
        .items
        .iter()
        .take(reg.items.len())
        .enumerate()
        .filter(|(_, item)| {
            item.durability > 0 && item.food.is_none() && !item.materials.is_empty()
        })
        .map(|(index, item)| (ItemId(index as u16), item.clone()))
        .collect::<Vec<_>>();
    for (original_id, original) in durable {
        if original.name == "base:tuning_lens"
            && let Some(mount) = reg.item_id("base:tuning_lens_mount")
        {
            // The custom wear path consumes only the replaceable Wellglass
            // owner. The fitted frame and Echo Slate plate are one conserved
            // physical mount, not generic damaged salvage.
            reg.items[original_id.0 as usize].broken_into = Some(mount);
            continue;
        }
        let damaged = push_salvage_item(
            reg,
            &original,
            "damaged",
            "Damaged",
            original.materials.clone(),
        );
        reg.items[damaged.0 as usize].max_stack = 1;
        reg.items[damaged.0 as usize].salvage = Some(SalvageDef {
            station: "forge".into(),
            recovery_permille: 900,
        });
        reg.items[original_id.0 as usize].broken_into = Some(damaged);

        let (primitive, primitive_scale) = split_recovery(&original.materials, 750);
        let primitive_out = push_salvage_item(
            reg,
            &original,
            "primitive_scrap",
            "Crude Scrap from",
            primitive,
        );
        let primitive_tail = push_salvage_item(
            reg,
            &original,
            "primitive_scale",
            "Scale from",
            primitive_scale,
        );
        reg.recipes.push(RecipeDef {
            w: 1,
            h: 1,
            pattern: vec![Some(Ingredient::One(damaged))],
            output: primitive_out,
            count: 1,
            station: None,
            loss: MaterialVector::new(),
            byproducts: vec![(primitive_tail, 1)],
            tech: None,
            blueprint: None,
        });

        let (forge, forge_scale) = split_recovery(&original.materials, 900);
        let forge_out =
            push_salvage_item(reg, &original, "forge_scrap", "Forged Stock from", forge);
        let forge_tail = push_salvage_item(
            reg,
            &original,
            "forge_scale",
            "Forge Scale from",
            forge_scale,
        );
        reg.forge_salvage.push(ForgeSalvageDef {
            input: damaged,
            output: forge_out,
            byproduct: forge_tail,
            recovery_permille: 900,
        });
    }

    let machine_blocks = reg
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| {
            !block.materials.is_empty()
                && matches!(
                    block.interaction.as_deref(),
                    Some(
                        "furnace"
                            | "bloomery"
                            | "kiln"
                            | "forge"
                            | "anvil"
                            | "quern"
                            | "millstone"
                            | "sawmill"
                            | "lathe"
                            | "iron_lathe"
                            | "boring"
                            | "pump"
                            | "generator"
                            | "separator"
                            | "firebox"
                    )
                )
        })
        .filter_map(|(index, block)| {
            reg.item_id(&block.name)
                .map(|item| (BlockId(index as u16), reg.item(item).clone()))
        })
        .collect::<Vec<_>>();
    for (block, source) in machine_blocks {
        let bundle = push_salvage_item(
            reg,
            &source,
            "dismantling_bundle",
            "Dismantled",
            source.materials.clone(),
        );
        reg.items[bundle.0 as usize].max_stack = 1;
        reg.items[bundle.0 as usize].salvage = Some(SalvageDef {
            station: "dismantling".into(),
            recovery_permille: 950,
        });
        reg.blocks[block.0 as usize].dismantles_to = Some(bundle);
        let (recovered, scale) = split_recovery(&source.materials, 950);
        let output = push_salvage_item(
            reg,
            &source,
            "dismantled_stock",
            "Clean Stock from",
            recovered,
        );
        let byproduct = push_salvage_item(
            reg,
            &source,
            "dismantling_scale",
            "Dismantling Scale from",
            scale,
        );
        reg.forge_salvage.push(ForgeSalvageDef {
            input: bundle,
            output,
            byproduct,
            recovery_permille: 950,
        });
    }

    // Lit/running machine blocks deliberately have no item form; they drop
    // the canonical cold machine. Give those state variants the same clean
    // dismantling bundle instead of accidentally making a running machine a
    // 100%-recovery loophole.
    let inherited = reg
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| block.dismantles_to.is_none() && !block.materials.is_empty())
        .filter_map(|(index, block)| {
            let dropped_item = block.drops?.0;
            let canonical_block = reg.item(dropped_item).places?;
            let bundle = reg.block(canonical_block).dismantles_to?;
            Some((index, bundle))
        })
        .collect::<Vec<_>>();
    for (index, bundle) in inherited {
        reg.blocks[index].dismantles_to = Some(bundle);
    }
}

