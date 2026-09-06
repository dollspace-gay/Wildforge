//! Restore saved names and conserved payloads before remapping removed content.

use super::{BlockId, ItemDef, ItemId, MaterialClass, MaterialVector, Registry};
use std::path::Path;

impl Registry {
    pub fn install_saved_arcane_placeholders(
        &mut self,
        ledger: &crate::arcane::ArcaneLedger,
    ) -> usize {
        let mut added = 0;
        for (name, saved) in &ledger.block_manifests {
            if let Some(id) = self.block_id(name) {
                self.blocks[id.0 as usize].arcane = Some(saved.arcane.clone());
                continue;
            }
            let id = BlockId(self.blocks.len() as u16);
            let mut placeholder = self.block(self.unknown_block).clone();
            placeholder.name = name.clone();
            placeholder.label = format!("Missing charged content: {name}");
            placeholder.material_class = MaterialClass::Exceptional;
            placeholder.arcane = Some(saved.arcane.clone());
            self.blocks.push(placeholder);
            self.block_by_name.insert(name.clone(), id);
            added += 1;
        }
        for (name, saved) in &ledger.item_manifests {
            if let Some(id) = self.item_id(name) {
                self.items[id.0 as usize].arcane = Some(saved.arcane.clone());
                self.items[id.0 as usize].max_stack = 1;
                continue;
            }
            let id = ItemId(self.items.len() as u16);
            self.items.push(ItemDef {
                name: name.clone(),
                label: format!("Missing charged content: {name}"),
                icon: crate::atlas::UNKNOWN_SLOT,
                max_stack: 1,
                tool: None,
                durability: saved.durability,
                places: self.block_id(name),
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
                materials: MaterialVector::new(),
                materials_declared: true,
                material_class: MaterialClass::Exceptional,
                salvage: None,
                broken_into: None,
                arcane: Some(saved.arcane.clone()),
                arcane_ecology: None,
                observation: None,
                discovery: None,
            });
            self.item_by_name.insert(name.clone(), id);
            added += 1;
        }
        added
    }

    /// Recreate named save placeholders before palette remapping. Their
    /// qualified names remain the removed mod's names, so chunks never get
    /// rewritten as an anonymous `base:unknown`; reinstalling the mod maps
    /// the same palette names back to the real definitions.
    pub fn install_saved_placeholders(
        &mut self,
        world: &Path,
        ledger: &crate::materials::MaterialLedger,
    ) -> std::io::Result<usize> {
        let Ok(palette) = std::fs::read_to_string(world.join("palette")) else {
            return Ok(0);
        };
        let mut added = 0;
        for name in palette
            .lines()
            .filter_map(|line| line.split_once(' ').map(|(_, name)| name.trim()))
        {
            if self.block_by_name.contains_key(name) {
                continue;
            }
            let mod_id = name.split_once(':').map(|(id, _)| id).unwrap_or_default();
            let materials = ledger
                .block_manifests
                .get(name)
                .map(|definition| definition.materials.clone())
                .or_else(|| {
                    ledger
                        .retrogen
                        .values()
                        .find(|record| record.resource_key == name || record.mod_id == mod_id)
                        .map(|record| record.unit_materials.clone())
                })
                .unwrap_or_default();
            let id = BlockId(self.blocks.len() as u16);
            let mut placeholder = self.block(self.unknown_block).clone();
            placeholder.name = name.to_string();
            placeholder.label = format!("Missing content: {name}");
            placeholder.materials = materials;
            if !placeholder.materials.is_empty() {
                placeholder.material_class = MaterialClass::GeologicallyFinite;
            }
            self.blocks.push(placeholder);
            self.block_by_name.insert(name.to_string(), id);
            added += 1;
        }
        for (name, saved) in &ledger.item_manifests {
            if self.item_by_name.contains_key(name) {
                continue;
            }
            let id = ItemId(self.items.len() as u16);
            self.items.push(ItemDef {
                name: name.clone(),
                label: format!("Missing content: {name}"),
                icon: crate::atlas::UNKNOWN_SLOT,
                max_stack: saved.max_stack.max(1),
                tool: None,
                durability: saved.durability,
                places: self.block_id(name),
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
                materials: saved.materials.clone(),
                materials_declared: true,
                material_class: saved.material_class,
                salvage: None,
                broken_into: None,
                arcane: None,
                arcane_ecology: None,
                observation: None,
                discovery: None,
            });
            self.item_by_name.insert(name.clone(), id);
            added += 1;
        }
        Ok(added)
    }
}
