//! Authoritative live entities, companion links, and stable identity cursors.
//!
//! Physical custody/accounting stays with World transaction coordination. This
//! owner controls insertion, restoration, removal, and paired NPC/mob publication.

use crate::entity::ItemEntity;
use crate::mobs::{Mob, Projectile};
use crate::npc::NpcInstance;
use crate::registry::Registry;
use super::{LOOSE_ITEM_ID_BASE, MOB_CAP, NPC_CAP};

pub(super) struct Population {
    mobs: Vec<Mob>,
    npcs: Vec<NpcInstance>,
    projectiles: Vec<Projectile>,
    loose_items: Vec<ItemEntity>,
    next_mob_id: u32,
    next_projectile_id: u64,
    next_loose_item_id: u64,
}

impl Default for Population {
    fn default() -> Self {
        Self { mobs: Vec::new(), npcs: Vec::new(), projectiles: Vec::new(),
            loose_items: Vec::new(), next_mob_id: 1, next_projectile_id: 1,
            next_loose_item_id: LOOSE_ITEM_ID_BASE }
    }
}

impl Population {
    pub(super) fn loose_items(&self) -> &[crate::entity::ItemEntity] {
        &self.loose_items
    }

    pub(super) fn loose_items_mut(&mut self) -> &mut [crate::entity::ItemEntity] {
        &mut self.loose_items
    }

    pub(super) fn spawn_loose_item(&mut self, mut item: crate::entity::ItemEntity) -> u64 {
        if item.stable_id == 0 {
            item.stable_id = self.next_loose_item_id.max(LOOSE_ITEM_ID_BASE);
        }
        self.next_loose_item_id = self
            .next_loose_item_id
            .max(item.stable_id.saturating_add(1))
            .max(LOOSE_ITEM_ID_BASE);
        let id = item.stable_id;
        self.loose_items.push(item);
        id
    }

    pub(super) fn replace_loose_items(&mut self, items: Vec<crate::entity::ItemEntity>) {
        self.next_loose_item_id = items
            .iter()
            .map(|item| item.stable_id)
            .max()
            .unwrap_or(LOOSE_ITEM_ID_BASE - 1)
            .saturating_add(1)
            .max(LOOSE_ITEM_ID_BASE);
        self.loose_items = items;
    }

    pub(super) fn take_loose_items(&mut self) -> Vec<crate::entity::ItemEntity> {
        std::mem::take(&mut self.loose_items)
    }

    pub(super) fn clear_loose_items(&mut self) {
        self.loose_items.clear();
    }

    pub(super) fn for_each_loose_item_mut(
        &mut self,
        mut update: impl FnMut(&mut crate::entity::ItemEntity),
    ) {
        for item in &mut self.loose_items {
            update(item);
        }
    }

    pub(super) fn mobs(&self) -> &[Mob] {
        &self.mobs
    }

    pub(super) fn mobs_mut(&mut self) -> &mut [Mob] {
        &mut self.mobs
    }

    pub(super) fn mob(&self, index: usize) -> Option<&Mob> {
        self.mobs.get(index)
    }

    pub(super) fn mob_mut(&mut self, index: usize) -> Option<&mut Mob> {
        self.mobs.get_mut(index)
    }

    pub(super) fn mob_by_id(&self, id: u32) -> Option<&Mob> {
        self.mobs.iter().find(|m| m.id == id)
    }

    pub(super) fn mob_by_id_mut(&mut self, id: u32) -> Option<&mut Mob> {
        self.mobs.iter_mut().find(|mob| mob.id == id)
    }

    pub(super) fn mob_count(&self) -> usize {
        self.mobs.len()
    }

    pub(super) fn npcs(&self) -> &[crate::npc::NpcInstance] {
        &self.npcs
    }

    pub(super) fn npc_count(&self) -> usize {
        self.npcs.len()
    }

    pub(super) fn npc_by_mob(&self, mob_id: u32) -> Option<&crate::npc::NpcInstance> {
        self.npcs.iter().find(|n| n.mob_id == mob_id)
    }

    pub(super) fn spawn_npc_at(&mut self, registry: &Registry, def: usize, pos: crate::planet::EntityPos) -> Option<u32> {
        let npc = registry.npcs.get(def)?.clone();
        if self.mobs.len() >= MOB_CAP || self.npcs.len() >= NPC_CAP {
            return None;
        }
        let species = npc.species;
        let mob_id = self.reserve_mob_id()?;
        let mut m = crate::mobs::Mob::new_at(species, pos, 0.0);
        m.health = registry.animals[species].health;
        m.id = mob_id;
        self.mobs.push(m);
        let mut instance = crate::npc::NpcInstance::new(&npc, pos, mob_id).with_def(def);
        instance.species = species;
        self.npcs.push(instance);
        Some(mob_id)
    }

    pub(super) fn replace_mobs(&mut self, mobs: Vec<Mob>) {
        self.mobs = mobs;
    }

    pub(super) fn for_each_mob_mut(&mut self, mut update: impl FnMut(&mut Mob)) {
        for mob in &mut self.mobs {
            update(mob);
        }
    }

    pub(super) fn projectiles(&self) -> &[Projectile] {
        &self.projectiles
    }

    pub(super) fn spawn_projectile(&mut self, projectile: Projectile) {
        let mut projectile = projectile;
        if projectile.stable_id == 0 {
            projectile.stable_id = self.next_projectile_id.max(1);
            self.next_projectile_id = projectile.stable_id.saturating_add(1).max(1);
        } else {
            self.next_projectile_id = self
                .next_projectile_id
                .max(projectile.stable_id.saturating_add(1));
        }
        self.projectiles.push(projectile);
    }

    pub(super) fn replace_projectiles(&mut self, projectiles: Vec<Projectile>) {
        self.next_projectile_id = projectiles
            .iter()
            .map(|projectile| projectile.stable_id)
            .max()
            .unwrap_or_default()
            .saturating_add(1)
            .max(1);
        self.projectiles = projectiles;
    }

    pub(super) fn for_each_projectile_mut(&mut self, mut update: impl FnMut(&mut Projectile)) {
        for projectile in &mut self.projectiles {
            update(projectile);
        }
    }

    pub(super) fn next_loose_item_id(&self) -> u64 { self.next_loose_item_id.max(LOOSE_ITEM_ID_BASE) }

    pub(super) fn reserve_mob_id(&mut self) -> Option<u32> {
        if self.next_mob_id == u32::MAX { return None; }
        let id = self.next_mob_id;
        self.next_mob_id += 1;
        Some(id)
    }

    pub(super) fn push_mob(&mut self, mob: Mob) { self.mobs.push(mob); }
    pub(super) fn remove_mob(&mut self, index: usize) -> Mob { self.mobs.swap_remove(index) }
    pub(super) fn take_mobs(&mut self) -> Vec<Mob> { std::mem::take(&mut self.mobs) }
    pub(super) fn take_npcs(&mut self) -> Vec<NpcInstance> { std::mem::take(&mut self.npcs) }
    pub(super) fn restore_npcs(&mut self, npcs: Vec<NpcInstance>) { self.npcs = npcs; }
    pub(super) fn take_projectiles(&mut self) -> Vec<Projectile> { std::mem::take(&mut self.projectiles) }
    /// Restore a simulation pass without rewinding its already allocated IDs.
    pub(super) fn restore_projectiles(&mut self, projectiles: Vec<Projectile>) { self.projectiles = projectiles; }
    pub(super) fn restore_loose_items(&mut self, items: Vec<ItemEntity>) { self.loose_items = items; }
    pub(super) fn remap_loose_items(&mut self, old: &Registry, registry: &Registry) {
        crate::entity::remap_items(&mut self.loose_items, old, registry);
    }

    pub(super) fn stamp_unassigned_mobs(&mut self) {
        for mob in &mut self.mobs {
            if mob.id == 0 { mob.id = self.next_mob_id; self.next_mob_id += 1; }
        }
    }

    /// Loading appends a companion, then joins the NPC to its ID immediately.
    /// Preserve the existing saturating load cursor, distinct from live spawn.
    pub(super) fn attach_loaded_npc(&mut self, definition: &crate::registry::NpcDef, index: usize, position: crate::planet::EntityPos) {
        let mob = self.mobs.last_mut().expect("mob just pushed");
        mob.id = self.next_mob_id;
        self.next_mob_id = self.next_mob_id.saturating_add(1);
        self.npcs.push(NpcInstance::new(definition, position, mob.id).with_def(index));
    }
}
