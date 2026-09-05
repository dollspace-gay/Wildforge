//! Authoritative population access and coordinated mob manifestation.

use crate::world::World;
use crate::mobs::Mob;
use crate::planet::EntityPos;

mod items;
mod death;
mod habitat;
mod wildlife;
mod step;
mod projectiles;
mod hostiles;

#[derive(Clone, Debug)]
pub struct SettledMobDeath {
    pub species: usize,
    pub pos: EntityPos,
}

impl World {

    pub fn mobs(&self) -> &[Mob] {
        self.population.mobs()
    }

    pub(crate) fn mobs_mut(&mut self) -> &mut [Mob] {
        self.population.mobs_mut()
    }

    pub fn mob(&self, index: usize) -> Option<&Mob> {
        self.population.mob(index)
    }

    pub fn mob_mut(&mut self, index: usize) -> Option<&mut Mob> {
        self.population.mob_mut(index)
    }

    pub fn mob_by_id(&self, id: u32) -> Option<&Mob> {
        self.population.mob_by_id(id)
    }

    pub fn mob_by_id_mut(&mut self, id: u32) -> Option<&mut Mob> {
        self.population.mob_by_id_mut(id)
    }

    pub fn mob_count(&self) -> usize {
        self.population.mob_count()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn npcs(&self) -> &[crate::npc::NpcInstance] {
        self.population.npcs()
    }

    pub fn npc_count(&self) -> usize {
        self.population.npc_count()
    }

    /// The NPC instance driving the given mob, if that mob is an NPC.
    pub fn npc_by_mob(&self, mob_id: u32) -> Option<&crate::npc::NpcInstance> {
        self.population.npc_by_mob(mob_id)
    }

    /// Spawn a def's companion mob plus its NPC instance in one call,
    /// returning the mob's stable id. Used by the script `spawn_npc` host fn
    /// and the assembly-marker consumer, which have only a def name/pos.
    pub fn spawn_npc_at(&mut self, def: usize, pos: crate::planet::EntityPos) -> Option<u32> {
        self.population.spawn_npc_at(&self.reg, def, pos)
    }

    pub fn spawn_mob(&mut self, mob: Mob) {
        let mut mob = mob;
        if mob.id == 0 {
            let Some(id) = self.population.reserve_mob_id() else {
                eprintln!("mobs: stable id space exhausted; refusing further spawns");
                return;
            };
            mob.id = id;
        }
        let Some(definition) = self.reg.animals.get(mob.species).cloned() else {
            return;
        };
        if definition.hostile
            && let Some(arcane) = definition.arcane.as_ref()
            && self.arcane_ledger.is_some()
        {
            let source = self
                .planet_atlas
                .as_ref()
                .and_then(|atlas| {
                    atlas
                        .country_at(mob.pos.surface())
                        .map(|country| country.id)
                })
                .map(crate::arcane::ArcaneOwner::Heart)
                .unwrap_or(crate::arcane::ArcaneOwner::Deep);
            let result = self
                .arcane_ledger
                .as_mut()
                .expect("checked above")
                .bind_new_owner(
                    source,
                    crate::arcane::ArcaneOwner::Mob(u64::from(mob.id)),
                    arcane.capacity,
                    arcane.resonance.keys().cloned().collect(),
                    &definition.name,
                    "warden manifestation",
                );
            if let Err(error) = result {
                eprintln!(
                    "arcane: warden {} could not manifest: {error}",
                    definition.name
                );
                return;
            }
        }
        self.population.push_mob(mob);
    }

    pub fn replace_mobs(&mut self, mobs: Vec<Mob>) {
        self.population.replace_mobs(mobs)
    }

    pub fn for_each_mob_mut(&mut self, update: impl FnMut(&mut Mob)) {
        self.population.for_each_mob_mut(update)
    }

}
