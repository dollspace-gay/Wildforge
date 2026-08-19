//! Runtime registry lookup and gameplay queries.

use super::*;

impl Registry {
    #[inline]
    pub fn block(&self, id: BlockId) -> &BlockDef {
        &self.blocks[id.0 as usize]
    }

    /// Mean albedo of every block, indexed by id, for the bounce grid.
    ///
    /// One colour per block, averaged over its six faces: the grid stores a
    /// cell, not a face, so a block that differs top from side (grass, a log)
    /// reports the compromise. That costs nothing on the plain materials most
    /// of a room is built from, and bounced light is too diffuse to have
    /// resolved the difference anyway.
    pub fn block_albedo(&self, slots: &[[u8; 3]]) -> Vec<[u8; 3]> {
        self.blocks
            .iter()
            .map(|def| {
                let mut sum = [0u32; 3];
                for &tile in &def.tiles {
                    let a = slots.get(tile as usize).copied().unwrap_or([0; 3]);
                    for c in 0..3 {
                        sum[c] += a[c] as u32;
                    }
                }
                [(sum[0] / 6) as u8, (sum[1] / 6) as u8, (sum[2] / 6) as u8]
            })
            .collect()
    }

    #[inline]
    pub fn item(&self, id: ItemId) -> &ItemDef {
        &self.items[id.0 as usize]
    }

    pub fn block_id(&self, name: &str) -> Option<BlockId> {
        self.block_by_name.get(name).copied()
    }

    pub fn animal_id(&self, name: &str) -> Option<usize> {
        self.animals.iter().position(|a| a.name == name)
    }

    pub fn npc_id(&self, name: &str) -> Option<usize> {
        self.npcs.iter().position(|n| n.name == name)
    }

    pub fn dialogue_id(&self, name: &str) -> Option<usize> {
        self.dialogues.iter().position(|d| d.id == name)
    }

    pub fn quest_id(&self, name: &str) -> Option<usize> {
        self.quests.iter().position(|q| q.id == name)
    }

    /// Index of a flag-gated feature by its qualified id (`feature:<id>`
    /// markers reference this).
    pub fn gate_id(&self, name: &str) -> Option<usize> {
        self.gates.iter().position(|g| g.id == name)
    }

    /// The gate backing a sealed block, if any (reverse of `gate_id`; lets
    /// the runtime find a gate from a placed block without marker
    /// provenance). The map is `block -> gate index`.
    pub fn gate_for_block(&self, block: BlockId) -> Option<usize> {
        self.gate_for_block.get(&block).copied()
    }

    /// Index of a settlement by its qualified id (spec 3.4).
    pub fn settlement_id(&self, name: &str) -> Option<usize> {
        self.settlements.iter().position(|s| s.id == name)
    }

    /// `true` when a mob species is a friendly NPC (spec 3.1).
    pub fn is_npc_species(&self, species: usize) -> bool {
        self.animals.get(species).is_some_and(|a| a.npc.is_some())
    }

    pub fn item_id(&self, name: &str) -> Option<ItemId> {
        self.item_by_name.get(name).copied()
    }

    #[inline]
    pub fn is_air(&self, id: BlockId) -> bool {
        id == AIR
    }

    #[inline]
    pub fn is_solid(&self, id: BlockId) -> bool {
        self.block(id).solid
    }

    #[inline]
    pub fn is_opaque(&self, id: BlockId) -> bool {
        self.block(id).opaque
    }

    #[inline]
    pub fn is_water(&self, id: BlockId) -> bool {
        let d = self.block(id);
        d.water_level.is_some() && !d.lava
    }

    /// Can a placed block take this cell? Air, any fluid, and thin
    /// ground layers (snow, litter, lily pads) give way — you build
    /// into a pond or over a drift, and the cell's old contents are
    /// displaced. Anything standing (crops, saplings, torches) does
    /// NOT: a placement must never quietly eat a player's work.
    #[inline]
    pub fn is_replaceable(&self, id: BlockId) -> bool {
        if id == AIR {
            return true;
        }
        let d = self.block(id);
        d.water_level.is_some() || (d.height.is_some() && !d.solid)
    }

    #[inline]
    pub fn is_lava(&self, id: BlockId) -> bool {
        self.block(id).lava
    }

    /// Any finite fluid — what renders translucent, what rays pass
    /// through, what a bucket can dip.
    #[inline]
    pub fn is_fluid(&self, id: BlockId) -> bool {
        self.block(id).water_level.is_some()
    }

    #[cfg(test)]
    #[inline]
    pub fn water_level(&self, id: BlockId) -> Option<u8> {
        self.block(id).water_level
    }

    pub fn water_block(&self, level: u8) -> BlockId {
        self.water_ids[(level as usize).min(7)]
    }

    pub fn water_height(&self, id: BlockId) -> f32 {
        match self.block(id).water_level {
            Some(l) => (8 - l) as f32 / 9.0,
            None => 1.0,
        }
    }

    /// Finite-water volume of a cell: level 0 holds 8 units, level 7
    /// holds 1. None for anything that isn't water (lava included).
    #[inline]
    pub fn water_volume(&self, id: BlockId) -> Option<u8> {
        let d = self.block(id);
        if d.lava {
            None
        } else {
            d.water_level.map(|l| 8 - l)
        }
    }

    /// Finite-lava volume of a cell (the same 8-unit scale).
    #[inline]
    pub fn lava_volume(&self, id: BlockId) -> Option<u8> {
        let d = self.block(id);
        if d.lava {
            d.water_level.map(|l| 8 - l)
        } else {
            None
        }
    }

    /// Volume of either fluid — the bucket doesn't care which.
    #[inline]
    pub fn fluid_volume(&self, id: BlockId) -> Option<u8> {
        self.block(id).water_level.map(|l| 8 - l)
    }

    /// The block holding `v` units of water; 0 units is air.
    pub fn water_for_volume(&self, v: u8) -> BlockId {
        if v == 0 {
            AIR
        } else {
            self.water_ids[(8 - v.min(8)) as usize]
        }
    }

    /// The block holding `v` units of lava; 0 units is air.
    pub fn lava_for_volume(&self, v: u8) -> BlockId {
        if v == 0 {
            AIR
        } else {
            self.lava_ids[(8 - v.min(8)) as usize]
        }
    }

    /// Seconds to break `block` holding `held`.
    pub fn effective_hardness(&self, block: BlockId, held: Option<ItemId>) -> Option<f32> {
        let d = self.block(block);
        let base = d.hardness?;
        let mult = match (held.and_then(|i| self.item(i).tool), d.tool) {
            (Some((kind, speed, _)), Some(class)) if kind == class => speed,
            _ => 1.0,
        };
        Some(base / mult)
    }

    pub fn recipes_for(&self, item: ItemId) -> Vec<&RecipeDef> {
        self.recipes.iter().filter(|r| r.output == item).collect()
    }

    pub fn smelts_for(&self, item: ItemId) -> Vec<&SmeltDef> {
        self.smelts.iter().filter(|s| s.output == item).collect()
    }

    /// (recipes using it, smelts using it, is-a-fuel)
    pub fn uses_of(&self, item: ItemId) -> (Vec<&RecipeDef>, Vec<&SmeltDef>, bool) {
        let r = self
            .recipes
            .iter()
            .filter(|r| r.pattern.iter().flatten().any(|i| i.matches(item)))
            .collect();
        let s = self
            .smelts
            .iter()
            .filter(|s| s.input.matches(item))
            .collect();
        let f = self.fuels.iter().any(|(i, _, _)| i.matches(item));
        (r, s, f)
    }

    pub fn smelt_for(&self, item: ItemId) -> Option<&SmeltDef> {
        self.smelts.iter().find(|s| s.input.matches(item))
    }

    /// (burn seconds, smelt-speed multiplier) for a fuel item.
    pub fn fuel_value(&self, item: ItemId) -> Option<(f32, f32)> {
        self.fuels
            .iter()
            .find(|(f, _, _)| f.matches(item))
            .map(|(_, b, sp)| (*b, *sp))
    }

    /// Drop for breaking `block` with `held` (requires_tool gating).
    pub fn drops_for(&self, block: BlockId, held: Option<ItemId>) -> Option<(ItemId, u32)> {
        let d = self.block(block);
        if d.requires_tool {
            let ok = match (held.and_then(|i| self.item(i).tool), d.tool) {
                (Some((kind, _, tier)), Some(class)) => kind == class && tier >= d.min_tier,
                _ => false,
            };
            if !ok {
                return None;
            }
        }
        d.drops
    }

    // ---------------- capability E7: data-driven machines ----------------

    /// The kind for a qualified machine id (e.g. `base:bloomery`), if the
    /// pack declares it.
    pub fn machine_kind(&self, name: &str) -> Option<crate::world::multiblock::MachineKind> {
        let index = self.machines.iter().position(|m| m.id == name)?;
        Some(crate::world::multiblock::MachineKind(index as u16))
    }

    /// The declared machine for a kind (bounds-checked; kind 0 in a pack
    /// with no base machines yields `None` rather than panicking).
    pub fn machine(
        &self,
        kind: crate::world::multiblock::MachineKind,
    ) -> Option<&crate::machines::MachineDef> {
        self.machines.get(kind.index())
    }

    /// Resolve a block `interaction` string to a machine kind. The string
    /// is a qualified id when it carries a `:`, otherwise a bare name
    /// (base's `interaction = "bloomery"` names `base:bloomery`).
    pub fn machine_by_interaction(&self, interaction: &str) -> Option<crate::world::multiblock::MachineKind> {
        if let Some(kind) = self.machine_kind(interaction) {
            return Some(kind);
        }
        if !interaction.contains(':')
            && let Some(kind) = self.machine_kind(&format!("base:{interaction}"))
        {
            return Some(kind);
        }
        None
    }

    /// Every recipe bound to `machine` by its `station` field — the list a
    /// workbench-style screen shows.
    pub fn machine_recipes_for(
        &self,
        machine: crate::world::multiblock::MachineKind,
    ) -> Vec<&RecipeDef> {
        let Some(def) = self.machine(machine) else {
            return Vec::new();
        };
        self.recipes
            .iter()
            .filter(|r| r.station.as_deref() == Some(def.id.as_str()))
            .collect()
    }
}
