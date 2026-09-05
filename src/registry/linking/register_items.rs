//! Register items in provider order with owned deferred references.

use super::Registration;
use crate::registry::{ArmorSlot, BowDef, FoodDef, ItemDef, ItemId, NUTRIENTS, Registry, ToolKind, qualify};
use crate::registry::schema::{CharmToml, RawMod};
use super::super::{arcane_def, arcane_ecology_def, discovery_item_def, inferred_material_class, observation_def, salvage_def};

impl Registration {
    pub(super) fn items(&mut self, reg: &mut Registry, raw: &RawMod, errs: &mut Vec<String>) {
        for it in &raw.items {
            let full = qualify(&raw.info.id, &it.id);
            if reg.item_by_name.contains_key(&full) {
                errs.push(format!("duplicate item {full}"));
                continue;
            }
            let icon = self.textures.resolve(&it.texture, &raw.info.path, errs);
            let tool = it
                .tool
                .map(|k| (k, it.tool_speed.unwrap_or(4.0), it.tool_tier.unwrap_or(1)));
            let iid = ItemId(reg.items.len() as u16);
            let food = it.food.as_ref().map(|f| {
                let mut n = [0.0f32; 5];
                for (k, v) in &f.nutrition {
                    if let Some(i) = NUTRIENTS.iter().position(|x| x == k) {
                        n[i] = *v;
                    }
                }
                FoodDef {
                    hunger: f.hunger,
                    eat_time: f.eat_time.unwrap_or(1.5),
                    nutrition: n,
                }
            });
            let damage = it.damage.unwrap_or(match tool {
                Some((ToolKind::Axe, _, _)) => 3.0,
                Some(_) => 2.0,
                None => 1.0,
            });
            let armor = it
                .armor
                .as_ref()
                .and_then(|a| ArmorSlot::parse(&a.slot).map(|s| (s, a.points)));
            let arcane = match arcane_def(
                it.arcane.as_ref(),
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
            let arcane_ecology = match arcane_ecology_def(
                it.arcane_ecology.as_ref(),
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
            let observation = match observation_def(it.observation.as_ref(), &raw.info.id, &full) {
                Ok(definition) => definition,
                Err(error) => {
                    errs.push(error);
                    None
                }
            };
            let discovery = match discovery_item_def(it.discovery.as_ref(), &raw.info.id, &full) {
                Ok(definition) => definition,
                Err(error) => {
                    errs.push(error);
                    None
                }
            };
            let charm_def = it.charm.as_ref().and_then(CharmToml::definition);
            if let Some(raw_charm) = &it.charm {
                match &charm_def {
                    Some(definition) => {
                        if let Err(error) = crate::implements::validate_charm(&full, definition) {
                            let error = error.to_string();
                            errs.push(error.clone());
                            reg.arcane_errors.push(error);
                        }
                    }
                    None => {
                        let error =
                            format!("{full}: unknown charm effect {}", raw_charm.effect_id());
                        errs.push(error.clone());
                        reg.arcane_errors.push(error);
                    }
                }
            }
            if let Some(component) = &it.wand_component
                && let Err(error) = crate::implements::validate_component(&full, component)
            {
                let error = error.to_string();
                errs.push(error.clone());
                reg.arcane_errors.push(error);
            }
            let one_only = tool.is_some()
                || it.bow.is_some()
                || armor.is_some()
                || arcane.is_some()
                || discovery.is_some()
                || charm_def.is_some()
                || it.wand_component.is_some()
                || it.implement.is_some()
                || it.frame.is_some()
                || it.component.is_some();
            let frame = match &it.frame {
                Some(raw) => {
                    let def = crate::equipment::FrameDef {
                        slots: raw
                            .slots
                            .iter()
                            .map(|s| crate::equipment::FrameSlotDef {
                                slot_type: s.kind.clone(),
                                max: s.max,
                            })
                            .collect(),
                    };
                    for error in def.validate() {
                        errs.push(format!("{full}: {error}"));
                    }
                    if it.component.is_some() {
                        errs.push(format!(
                            "{full}: an item cannot be both a frame and a component"
                        ));
                    }
                    Some(def)
                }
                None => {
                    if let Some(slot_type) = &it.component
                        && slot_type.is_empty()
                    {
                        errs.push(format!("{full}: component slot type must not be empty"));
                    }
                    None
                }
            };
            reg.items.push(ItemDef {
                name: full.clone(),
                label: it.name.clone().unwrap_or_else(|| it.id.clone()),
                icon,
                max_stack: if one_only {
                    1
                } else {
                    it.max_stack.unwrap_or(64)
                },
                tool,
                durability: it.durability.unwrap_or(if tool.is_some() { 59 } else { 0 }),
                places: None,
                food,
                damage,
                damage_type: it.damage_type.clone(),
                bow: it.bow.as_ref().map(|b| BowDef {
                    damage: b.damage,
                    speed: b.speed.unwrap_or(24.0),
                }),
                ammo: it.ammo.clone(),
                armor,
                carry_weight: it.carry_weight.unwrap_or(1),
                stats: it
                    .stats
                    .iter()
                    .filter_map(|s| {
                        let kind = match s.kind.parse() {
                            Ok(kind) => kind,
                            Err(error) => {
                                errs.push(format!("{full}: {error}"));
                                return None;
                            }
                        };
                        Some(crate::stats::StatModifier {
                            kind,
                            flat: s.flat.unwrap_or(0.0),
                            mult_permille: s.mult_permille.unwrap_or(1_000),
                        })
                    })
                    .collect(),
                frame,
                component: it.component.clone(),
                bedroll: it.bedroll,
                shears: it.shears,
                charm: it.charm.as_ref().map(CharmToml::effect_id),
                charm_def,
                wand_component: it.wand_component.clone(),
                implement: it.implement.clone(),
                tablet: it.tablet,
                striker: it.striker,
                creative_only: false,
                brush_tool: it.brush_tool,
                throw_speed: it.throw.as_ref().map(|t| t.speed.unwrap_or(18.0)),
                hammer: it.hammer,
                hack: it.hack,
                glow: it.glow,
                materials: it.materials.clone(),
                materials_declared: !it.materials.is_empty(),
                material_class: it
                    .material_class
                    .unwrap_or_else(|| inferred_material_class(&full)),
                salvage: salvage_def(&it.salvage, errs, &full),
                broken_into: None,
                arcane,
                arcane_ecology: arcane_ecology.clone(),
                observation,
                discovery,
            });
            if let Some(ecology) = arcane_ecology {
                reg.arcane_ecology.entry(full.clone()).or_insert(ecology);
            }
            reg.item_by_name.insert(full, iid);
        }
    }
}
