//! Register blocks in provider order with owned deferred references.

use super::super::pending::PendingDrop;
use super::super::{
    arcane_def, arcane_ecology_def, discovery_fixture_def, inferred_material_class, observation_def,
};
use super::Registration;
use crate::registry::blocks::resolve_light_rgb;
use crate::registry::schema::{RawMod, TexSpec};
use crate::registry::{
    BlockDef, BlockId, DiscoveryItemDef, ItemDef, ItemId, ObservationDef, Registry, qualify,
};

impl Registration {
    pub(super) fn blocks(&mut self, reg: &mut Registry, raw: &RawMod, errs: &mut Vec<String>) {
        for b in &raw.blocks {
            let full = qualify(&raw.info.id, &b.id);
            if reg.block_by_name.contains_key(&full) {
                errs.push(format!("duplicate block {full}"));
                continue;
            }
            let tiles = match &b.texture {
                TexSpec::One(t) => [self.textures.resolve(t, &raw.info.path, errs); 6],
                TexSpec::Faces { top, side, bottom } => {
                    let t = self.textures.resolve(top, &raw.info.path, errs);
                    let s = self.textures.resolve(side, &raw.info.path, errs);
                    let bo = bottom
                        .as_ref()
                        .map(|x| self.textures.resolve(x, &raw.info.path, errs))
                        .unwrap_or(t);
                    [s, s, t, bo, s, s]
                }
            };
            let fert_tiles = b.texture_fertility.as_ref().map(|v| {
                if v.len() != 4 {
                    errs.push(format!("{full}: texture_fertility wants 4 entries"));
                }
                let mut ft = [tiles[2]; 4];
                for (i, t) in v.iter().take(4).enumerate() {
                    ft[i] = self.textures.resolve(t, &raw.info.path, errs);
                }
                ft
            });
            let arcane =
                match arcane_def(b.arcane.as_ref(), &raw.info.id, &full, &reg.arcane_registry) {
                    Ok(definition) => definition,
                    Err(error) => {
                        errs.push(error.clone());
                        reg.arcane_errors.push(error);
                        None
                    }
                };
            let arcane_ecology = match arcane_ecology_def(
                b.arcane_ecology.as_ref(),
                &raw.info.id,
                &full,
                &reg.arcane_registry,
            ) {
                Ok(definition) => definition,
                Err(error) => {
                    errs.push(error.clone());
                    reg.arcane_errors.push(error);
                    None
                }
            };
            let mut observation = match observation_def(b.observation.as_ref(), &raw.info.id, &full)
            {
                Ok(definition) => definition,
                Err(error) => {
                    errs.push(error);
                    None
                }
            };
            if observation.is_none() && b.interaction.as_deref() == Some("heart") {
                observation = Some(ObservationDef {
                    categories: vec!["heart".into()],
                    properties: vec![
                        "strength".into(),
                        "stability".into(),
                        "resonance".into(),
                        "dross".into(),
                        "condition".into(),
                    ],
                });
            }
            let discovery_fixture = match discovery_fixture_def(b.discovery_fixture.as_ref(), &full)
            {
                Ok(definition) => definition,
                Err(error) => {
                    errs.push(error);
                    None
                }
            };
            let id = BlockId(reg.blocks.len() as u16);
            let is_fluid = b.water.is_some() || b.lava.is_some();
            reg.blocks.push(BlockDef {
                name: full.clone(),
                label: b.name.clone().unwrap_or_else(|| b.id.clone()),
                tiles,
                hardness: if b.unbreakable || is_fluid {
                    None
                } else {
                    b.hardness.or(Some(1.0))
                },
                tool: b.tool,
                requires_tool: b.requires_tool,
                drops: None,
                solid: b.solid && !is_fluid,
                opaque: b.opaque && !is_fluid,
                interaction: b.interaction.clone(),
                min_tier: b.min_tier,
                water_level: b.water.or(b.lava),
                lava: b.lava.is_some(),
                cross: b.cross,
                burns: b.burns,
                floats: b.floats,
                shape: b.shape.clone(),
                crop_next: None,
                crop_chance: 0.0,
                crop_any_soil: b.crop.as_ref().is_some_and(|c| c.any_soil),
                harvest: None,
                light_emit: b.light.min(15),
                sapling: b.sapling.as_ref().map(|t| t.tree.clone()),
                bonus_drop: None,
                brush: None,
                height: b.height.map(|h| h.clamp(0.05, 1.0)),
                falls: b.falls,
                glass: b.glass,
                light_filter: b
                    .light_filter
                    .map(|f| [f[0] > 0, f[1] > 0, f[2] > 0])
                    .unwrap_or([true; 3]),
                light_rgb: resolve_light_rgb(b.light.min(15), b.light_color),
                fert_tiles,
                crop_family: b
                    .crop
                    .as_ref()
                    .map(|c| {
                        c.family.map(|f| f.clamp(1, 3)).unwrap_or_else(|| {
                            // Stable name-derived family for mods.
                            let h = full
                                .bytes()
                                .fold(0u32, |a, ch| a.wrapping_mul(31).wrapping_add(ch as u32));
                            (h % 3 + 1) as u8
                        })
                    })
                    .unwrap_or(0),
                material_class: b
                    .material_class
                    .unwrap_or_else(|| inferred_material_class(&full)),
                materials: b.materials.clone(),
                dismantles_to: None,
                heat_retention: b.heat_retention,
                arcane: arcane.clone(),
                arcane_ecology: arcane_ecology.clone(),
                observation: observation.clone(),
                discovery_fixture: discovery_fixture.clone(),
            });
            if let Some(scar) = &b.dross_scar {
                let valid_carriers = !scar.carriers.is_empty()
                    && scar.carriers.len() <= 3
                    && scar.carriers.iter().all(|carrier| {
                        matches!(
                            carrier,
                            crate::dross::DrossCarrier::Air
                                | crate::dross::DrossCarrier::Water
                                | crate::dross::DrossCarrier::Soil
                        )
                    })
                    && scar
                        .carriers
                        .iter()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        == scar.carriers.len();
                let valid_band = matches!(
                    scar.min_band,
                    crate::dross::DrossBand::Seep
                        | crate::dross::DrossBand::Scar
                        | crate::dross::DrossBand::BreachRisk
                );
                if !valid_carriers || !valid_band || !(1..=8).contains(&scar.max_sites_per_region) {
                    let error = format!(
                        "{full}: dross scar needs 1..=3 unique environmental carriers, a seep-or-higher band, and 1..=8 sites per region"
                    );
                    errs.push(error.clone());
                    reg.arcane_errors.push(error);
                } else if reg.dross_scars.contains_key(&full) {
                    let error = format!("{full}: duplicate dross scar identity");
                    errs.push(error.clone());
                    reg.arcane_errors.push(error);
                } else {
                    reg.dross_scars.insert(
                        full.clone(),
                        crate::dross::DrossScarDef {
                            content_id: full.clone(),
                            provider: raw.info.id.clone(),
                            block: id,
                            kind: scar.kind,
                            handler: scar.handler,
                            carriers: scar.carriers.clone(),
                            min_band: scar.min_band,
                            status: scar.status,
                            activity: scar.activity,
                            max_sites_per_region: scar.max_sites_per_region,
                        },
                    );
                }
            }
            if let Some(ecology) = &arcane_ecology {
                reg.arcane_ecology.insert(full.clone(), ecology.clone());
            }
            reg.block_by_name.insert(full.clone(), id);
            if let Some(bd) = &b.bonus_drop {
                self.pending
                    .bonus
                    .push((raw.info.id.clone(), id.0 as usize, bd.clone()));
            }
            if let Some(br) = &b.brush {
                self.pending
                    .brush
                    .push((raw.info.id.clone(), id.0 as usize, br.clone()));
            }
            self.pending.drops.push(PendingDrop {
                modid: raw.info.id.clone(),
                block: id.0 as usize,
                rule: b.drops.clone().unwrap_or_else(|| {
                    if is_fluid {
                        "none".into()
                    } else {
                        "self".into()
                    }
                }),
                count: b.drop_count.unwrap_or(1),
            });
            if let Some(crop) = &b.crop {
                // Auto-register growth stages; each links to the next.
                let mut prev = id;
                for st in 1..crop.stages {
                    let sid = BlockId(reg.blocks.len() as u16);
                    let mut def = reg.blocks[id.0 as usize].clone();
                    def.name = format!("{full}/stage{st}");
                    if let Some(t) = crop.stage_textures.get(st as usize - 1) {
                        let s = self.textures.resolve(t, &raw.info.path, errs);
                        def.tiles = [s; 6];
                    }
                    reg.block_by_name.insert(def.name.clone(), sid);
                    reg.blocks.push(def);
                    reg.blocks[prev.0 as usize].crop_next = Some(sid);
                    reg.blocks[prev.0 as usize].crop_chance = crop.next_chance.unwrap_or(0.2);
                    prev = sid;
                }
                // The final stage grows no further (clones inherit the
                // base's link otherwise).
                reg.blocks[prev.0 as usize].crop_next = None;
                reg.blocks[prev.0 as usize].crop_chance = 0.0;
            }
            if let Some(h) = &b.harvest {
                // Harvest applies to the final growth stage (or the block
                // itself when it has no stages).
                let target = BlockId(reg.blocks.len() as u16 - 1);
                let target = if b.crop.is_some() { target } else { id };
                self.pending
                    .harvests
                    .push((raw.info.id.clone(), target, h.clone()));
            }
            if b.water == Some(0) || b.lava == Some(0) {
                // Auto-register the 7 flowing variants (either fluid).
                let ids = if b.lava.is_some() {
                    &mut reg.lava_ids
                } else {
                    &mut reg.water_ids
                };
                ids[0] = id;
                for l in 1..=7u8 {
                    let fid = BlockId(reg.blocks.len() as u16);
                    let mut def = reg.blocks[id.0 as usize].clone();
                    def.name = format!("{full}/flow{l}");
                    def.water_level = Some(l);
                    reg.block_by_name.insert(def.name.clone(), fid);
                    reg.blocks.push(def);
                    ids[l as usize] = fid;
                }
            }
            if b.item && !is_fluid {
                let icon_slot = b
                    .icon
                    .as_ref()
                    .map(|t| self.textures.resolve(t, &raw.info.path, errs))
                    .unwrap_or(tiles[0]);
                let iid = ItemId(reg.items.len() as u16);
                reg.items.push(ItemDef {
                    name: full.clone(),
                    label: reg.blocks[id.0 as usize].label.clone(),
                    icon: icon_slot,
                    max_stack: if arcane.is_some()
                        || discovery_fixture
                            .as_ref()
                            .is_some_and(|fixture| fixture.kind == "survey_folio")
                    {
                        1
                    } else {
                        64
                    },
                    tool: None,
                    durability: 0,
                    places: Some(id),
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
                    materials: b.materials.clone(),
                    materials_declared: !b.materials.is_empty(),
                    material_class: b
                        .material_class
                        .unwrap_or_else(|| inferred_material_class(&full)),
                    salvage: None,
                    broken_into: None,
                    arcane,
                    arcane_ecology,
                    observation,
                    discovery: discovery_fixture.as_ref().and_then(|fixture| {
                        (fixture.kind == "survey_folio").then(|| DiscoveryItemDef {
                            kind: "survey_folio".into(),
                            evidence_class: None,
                            authored_text: Vec::new(),
                            calibration: None,
                            experiment: None,
                        })
                    }),
                });
                reg.item_by_name.insert(full, iid);
            }
        }
    }
}
