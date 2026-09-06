//! Resolve tags, block rewards, aliases, placement, and held block forms.

use super::lookups::{lookup_block, lookup_item};
use super::pending::PendingDrop;
use crate::registry::schema::{AliasToml, BonusDropToml, BrushToml, HarvestToml, TagToml};
use crate::registry::{AIR, BlockId, ItemDef, ItemId, Registry, qualify};

pub(super) fn tags(reg: &mut Registry, pending_tags: Vec<(String, TagToml)>) {
    // Tags first (recipes reference them). Multiple mods extend the same tag.
    for (modid, t) in pending_tags {
        let tag_name = qualify(&modid, &t.id);
        for item in &t.items {
            if let Some(id) = lookup_item(reg, &modid, item) {
                let entry = reg.tags.entry(tag_name.clone()).or_default();
                if !entry.contains(&id) {
                    entry.push(id);
                }
            }
        }
    }
}

pub(super) fn bonus(reg: &mut Registry, pending_bonus: Vec<(String, usize, BonusDropToml)>) {
    for (modid, bi, bd) in pending_bonus {
        if let Some(item) = lookup_item(reg, &modid, &bd.item) {
            reg.blocks[bi].bonus_drop = Some((item, bd.chance));
        }
    }
}

pub(super) fn brush(reg: &mut Registry, pending_brush: Vec<(String, usize, BrushToml)>) {
    for (modid, bi, br) in pending_brush {
        if let Some(becomes) = lookup_block(reg, &modid, &br.becomes) {
            reg.blocks[bi].brush = Some((qualify(&modid, &br.table), becomes));
        }
    }
}

pub(super) fn drops(reg: &mut Registry, pending_drops: Vec<PendingDrop>) {
    for pd in pending_drops {
        let d = match pd.rule.as_str() {
            "none" => None,
            "self" => {
                let name = reg.blocks[pd.block].name.clone();
                reg.item_id(&name).map(|i| (i, pd.count))
            }
            // Bare names qualify with the declaring mod, like every
            // other cross-reference field.
            other => lookup_item(reg, &pd.modid, other).map(|i| (i, pd.count)),
        };
        reg.blocks[pd.block].drops = d;
    }
}

pub(super) fn harvests(reg: &mut Registry, pending_harvests: Vec<(String, BlockId, HarvestToml)>) {
    for (modid, block, h) in pending_harvests {
        let becomes = reg
            .block_id(&qualify(&modid, &h.becomes))
            .or_else(|| reg.block_id(&h.becomes));
        let item = lookup_item(reg, &modid, &h.item);
        if let (Some(item), Some(becomes)) = (item, becomes) {
            reg.blocks[block.0 as usize].harvest = Some((item, h.count.unwrap_or(2), becomes));
        }
    }
}

pub(super) fn aliases(reg: &mut Registry, pending_aliases: Vec<(String, AliasToml)>) {
    // Aliases: old name -> already-registered new id (lossless renames).
    for (modid, a) in pending_aliases {
        let new = qualify(&modid, &a.new);
        if let Some(id) = reg.block_by_name.get(&new).copied() {
            reg.block_by_name.entry(a.old.clone()).or_insert(id);
        }
        if let Some(id) = reg.item_by_name.get(&new).copied() {
            reg.item_by_name.entry(a.old.clone()).or_insert(id);
        }
    }
}

pub(super) fn places(reg: &mut Registry, pending_places: Vec<(String, (String, String))>) {
    // Item `places` links (food items that plant crops).
    for (modid, it_toml) in &pending_places {
        if let (Some(item), Some(block)) = (
            reg.item_id(&qualify(modid, &it_toml.0)),
            reg.block_id(&qualify(modid, &it_toml.1))
                .or_else(|| reg.block_id(&it_toml.1)),
        ) {
            let inherited_ecology = reg.block(block).arcane_ecology.clone();
            let item_name = reg.item(item).name.clone();
            let definition = &mut reg.items[item.0 as usize];
            definition.places = Some(block);
            if definition.arcane_ecology.is_none() {
                definition.arcane_ecology = inherited_ecology.clone();
            }
            if let Some(ecology) = inherited_ecology {
                reg.arcane_ecology.entry(item_name).or_insert(ecology);
            }
        }
    }
}

pub(super) fn crop_drops(reg: &mut Registry) {
    // Crop stages inherit their parent's drops (after drop resolution).
    for i in 0..reg.blocks.len() {
        if reg.blocks[i].name.contains("/stage") {
            let base = reg.blocks[i]
                .name
                .split("/stage")
                .next()
                .unwrap()
                .to_string();
            if let Some(pid) = reg.block_by_name.get(&base).copied() {
                reg.blocks[i].drops = reg.blocks[pid.0 as usize].drops;
            }
        }
    }
}

pub(super) fn creative_items(reg: &mut Registry) {
    // Every block a builder cannot otherwise hold gets a creative-only
    // item: lava, fire, a heart, a crop mid-growth, a fluid at any
    // level. These never appear in survival, never craft, and never
    // count toward obtainability — they exist so the browser can offer
    // every state of every block the way a builder expects.
    let placeable: std::collections::HashSet<u16> = reg
        .items
        .iter()
        .filter_map(|i| i.places.map(|b| b.0))
        .collect();
    for bid in 0..reg.blocks.len() as u16 {
        if placeable.contains(&bid) || bid == AIR.0 {
            continue;
        }
        let d = &reg.blocks[bid as usize];
        let (name, label, icon) = (d.name.clone(), d.label.clone(), d.tiles[2]);
        // The placeholder block, and anything a pack has left without
        // art, would put a missing-texture tile in the browser.
        if icon == crate::atlas::UNKNOWN_SLOT {
            continue;
        }
        let iid = ItemId(reg.items.len() as u16);
        reg.items.push(ItemDef {
            name: format!("{name}/place"),
            label,
            icon,
            max_stack: if d.arcane.is_some() { 1 } else { 64 },
            tool: None,
            durability: 0,
            places: Some(BlockId(bid)),
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
            creative_only: true,
            brush_tool: false,
            throw_speed: None,
            hammer: false,
            hack: false,
            glow: None,
            materials: d.materials.clone(),
            materials_declared: !d.materials.is_empty(),
            material_class: d.material_class,
            salvage: None,
            broken_into: None,
            arcane: d.arcane.clone(),
            arcane_ecology: d.arcane_ecology.clone(),
            observation: d.observation.clone(),
            discovery: None,
        });
        reg.item_by_name.insert(format!("{name}/place"), iid);
    }
}
