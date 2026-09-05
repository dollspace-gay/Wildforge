//! Mob death, harvest, hacking, and ordered Current/material disposition.

use crate::world::World;
use super::SettledMobDeath;
use crate::inventory::ItemStack;
use crate::planet::EntityPos;

impl World {
    pub(super) fn arcane_region_at(&self, pos: EntityPos) -> Option<crate::planet_atlas::AtlasPos> {
        self.planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
    }

    pub(super) fn retire_warden_current(
        &mut self,
        mob_id: u32,
        pos: EntityPos,
        disposition: crate::registry::ArcaneDisposition,
        reason: &str,
    ) {
        let Some(region) = self.arcane_region_at(pos) else {
            return;
        };
        let country = self
            .planet_atlas
            .as_ref()
            .and_then(|atlas| atlas.country_at(pos.surface()))
            .map(|country| country.id);
        let Some(ledger) = &mut self.arcane_ledger else {
            return;
        };
        let owner = crate::arcane::ArcaneOwner::Mob(u64::from(mob_id));
        if ledger.account(&owner).is_none() {
            return;
        }
        let recover_to_heart = (reason.contains("dissolved") || reason.contains("stood down"))
            && country.is_some_and(|country| !ledger.heart_frozen(country));
        let destination = if recover_to_heart {
            crate::arcane::ArcaneOwner::Heart(country.expect("checked above"))
        } else {
            match disposition {
                crate::registry::ArcaneDisposition::Ambient => {
                    crate::arcane::ArcaneOwner::Ambient(region)
                }
                crate::registry::ArcaneDisposition::Dross
                | crate::registry::ArcaneDisposition::Scar => crate::arcane::ArcaneOwner::Dross {
                    region,
                    medium: crate::arcane::DrossMedium::Soil,
                },
            }
        };
        if let Err(error) = ledger.move_all(owner, destination, reason) {
            eprintln!("arcane: could not retire warden Current: {error}");
        }
    }

    /// Authoritative death settlement shared by windowed, dedicated, and
    /// loopback hosts. Presentation consumes the returned records; loot and
    /// accounting have already landed exactly once here.
    /// Disable a construct at `index` (spec 3.6): freeze it and roll its
    /// core drops out at its feet. Destroying a construct still yields its
    /// scrap `drops`; this path yields the separate hack table. Returns
    /// how many items landed.
    pub fn hack_mob(&mut self, index: usize, rng: &mut u32) -> u32 {
        let (species, at) = {
            let Some(mob) = self.population.mobs_mut().get_mut(index) else {
                return 0;
            };
            mob.hacked = true;
            let Some(at) = mob.pos.block() else {
                return 0;
            };
            (mob.species, at)
        };
        let Some(def) = self.reg.animals.get(species).cloned() else {
            return 0;
        };
        let Some(hack) = &def.hack else {
            return 0;
        };
        let mut landed = 0;
        for (item, min, max) in &hack.drops {
            *rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let span = max.saturating_sub(*min).saturating_add(1);
            let count = min.saturating_add((*rng >> 8) % span.max(1)).min(*max);
            if count != 0 {
                self.push_drop_at(at, ItemStack::new(&self.reg, *item, count));
                landed += 1;
            }
        }
        landed
    }

    pub fn settle_dead_mobs(&mut self, rng: &mut u32) -> Vec<SettledMobDeath> {
        let reg = self.reg.clone();
        let mut settled = Vec::new();
        let mut index = 0;
        while index < self.population.mobs().len() {
            if self.population.mobs()[index].health > 0.0 {
                index += 1;
                continue;
            }
            let mob = self.population.remove_mob(index);
            let Some(definition) = reg.animals.get(mob.species).cloned() else {
                continue;
            };
            let at = mob.pos.block();
            if definition.hostile {
                if let Some(at) = at {
                    self.wild_falls_at(&definition.name, at);
                }
            } else if !definition.vehicle
                && !definition.name.ends_with(":carcass")
                && self.ruleset().ire
            {
                self.add_ire_at_surface(mob.pos.surface(), if mob.tamed { 1.0 } else { 2.0 });
            }
            if let (Some(at), Some(cargo)) = (at, mob.cargo) {
                for stack in cargo.into_iter().flatten() {
                    self.push_drop_at(at, stack);
                }
            }

            let mut drops = Vec::<ItemStack>::new();
            if mob.growth >= 1.0 {
                for (item, min, max) in &definition.drops {
                    *rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let span = max.saturating_sub(*min).saturating_add(1);
                    let count = min.saturating_add((*rng >> 8) % span.max(1)).min(*max);
                    let item_definition = reg.item(*item);
                    if item_definition.arcane.is_some() {
                        drops.extend((0..count).map(|_| ItemStack::new(&reg, *item, 1)));
                    } else if count != 0 {
                        drops.push(ItemStack::new(&reg, *item, count));
                    }
                }
            }

            if definition.hostile {
                let charged_count = drops
                    .iter()
                    .filter(|stack| reg.item(stack.item).arcane.is_some())
                    .count();
                let mut remaining_charged = charged_count;
                for stack in &mut drops {
                    let Some(mut arcane_definition) = reg.item(stack.item).arcane.clone() else {
                        continue;
                    };
                    let source = crate::arcane::ArcaneOwner::Mob(u64::from(mob.id));
                    let available = self
                        .arcane_ledger
                        .as_ref()
                        .and_then(|ledger| ledger.account(&source))
                        .map_or(0, |account| account.current.total());
                    if available == 0 || remaining_charged == 0 {
                        continue;
                    }
                    arcane_definition.capacity = arcane_definition
                        .capacity
                        .min(available.div_ceil(remaining_charged as u64));
                    if let Some(ledger) = &mut self.arcane_ledger {
                        match ledger.bind_new_item(
                            source,
                            &arcane_definition,
                            &reg.item(stack.item).name,
                            "warden drop",
                        ) {
                            Ok(item_id) => stack.arcane_id = item_id,
                            Err(error) => eprintln!("arcane: warden drop could not bind: {error}"),
                        }
                    }
                    remaining_charged -= 1;
                }
                if let Some(arcane) = definition.arcane.as_ref() {
                    self.retire_warden_current(
                        mob.id,
                        mob.pos,
                        arcane.on_destroy,
                        "warden death remainder",
                    );
                }
            }

            for stack in drops {
                if let Some(ledger) = &mut self.material_ledger
                    && let Err(error) =
                        ledger.record_external_stack(&reg, stack, "wild creature drop")
                {
                    eprintln!("materials: creature drop accounting failed: {error}");
                }
                if mob.last_hit_by != 0 {
                    self.queue_give(mob.last_hit_by, stack);
                } else if let Some(at) = at {
                    self.push_drop_at(at, stack);
                }
            }
            // E9: a swarm releases its brood where the mother fell.
            if definition.hostile
                && definition.behavior == crate::registry::BehaviorArchetype::Swarm
                && let Some(swarm) = definition.archetype.swarm.as_ref()
                && let Some(brood) = reg.animal_id(&swarm.spawn)
                && let Some(brood_def) = reg.animals.get(brood)
            {
                let n = swarm.count.clamp(1, 8);
                for i in 0..n {
                    *rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let ang = *rng as f32;
                    let step = ((*rng >> 8) % 7) as f32 + 1.0;
                    let du = ang.cos() * step;
                    let dv = ang.sin() * step;
                    let Some(moved) = mob
                        .pos
                        .translated(glam::Vec3::new(du, 0.0, dv))
                        .ok()
                        .map(|m| m.pos)
                    else {
                        continue;
                    };
                    *rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let mut hatchling = crate::mobs::Mob::new_at(
                        brood,
                        moved,
                        (*rng % 1024) as f32 / 1024.0 * std::f32::consts::TAU,
                    );
                    hatchling.health = brood_def.health;
                    if self.population.mobs().len() + (n - i) as usize >= crate::world::MOB_CAP {
                        break;
                    }
                    self.spawn_mob(hatchling);
                }
            }
            settled.push(SettledMobDeath {
                species: mob.species,
                pos: mob.pos,
            });
        }
        settled
    }

    /// The strike connects: the nearest swimmer within reach of the
    /// bobber leaves the water. Returns its species — real fish get
    /// caught before any luck table gets a say.
    #[cfg(test)]
    pub fn catch_fish_near(&mut self, at: glam::Vec3, radius: f32) -> Option<usize> {
        let at = crate::planet::EntityPos::from_local(crate::planet::Face::PosZ, at).ok()?;
        self.catch_fish_near_at(at, radius)
    }

    pub fn catch_fish_near_at(
        &mut self,
        at: crate::planet::EntityPos,
        radius: f32,
    ) -> Option<usize> {
        let reg = self.reg.clone();
        let idx = self.population.mobs()
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                reg.animals.get(m.species).is_some_and(|d| d.movement_swim)
                    && m.pos.distance_to(at) < radius
            })
            .min_by(|(_, a), (_, b)| a.pos.distance_to(at).total_cmp(&b.pos.distance_to(at)))
            .map(|(i, _)| i)?;
        let fish = self.population.remove_mob(idx);
        Some(fish.species)
    }

    /// A grazer's bite lands: a grown crop reverts to its planted
    /// base, grass to bare dirt (which heals). The animal never
    /// breaks a placed block — it eats what the plant grew, not what
    /// the farmer built.
    pub fn apply_bite_at(&mut self, pos: crate::planet::BlockPos) {
        let b = self.get_block_at(pos);
        let d = self.reg.block(b);
        if matches!(
            d.name.as_str(),
            "base:rainbell" | "base:lantern_reed" | "base:lantern_reed_dim" | "base:tidekelp"
        ) {
            if let Err(error) = self.settle_arcane_ecology_destruction(pos) {
                eprintln!("arcane ecology: wildlife bite cancelled at {pos:?}: {error}");
                return;
            }
            self.set_block_at(pos, crate::registry::AIR);
            return;
        }
        if d.crop_family != 0 && d.name.contains("/stage") {
            let base = d.name.split("/stage").next().unwrap_or("").to_string();
            if let Some(base_id) = self.reg.block_id(&base) {
                self.set_block_at(pos, base_id);
            }
            return;
        }
        if d.name == "base:grass"
            && let Some(dirt) = self.reg.block_id("base:dirt")
        {
            self.set_block_at(pos, dirt);
        }
    }
}
