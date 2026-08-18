//! Wildlife seeding, mob/projectile ticking, and hostile spawning.

use super::*;
use crate::planet::{BlockPos, EntityPos, SurfacePos};

#[derive(Clone, Debug)]
pub struct SettledMobDeath {
    pub species: usize,
    pub pos: EntityPos,
}

impl World {
    pub fn loose_items(&self) -> &[crate::entity::ItemEntity] {
        &self.loose_items
    }

    pub fn loose_items_mut(&mut self) -> &mut Vec<crate::entity::ItemEntity> {
        &mut self.loose_items
    }

    pub fn spawn_loose_item(&mut self, mut item: crate::entity::ItemEntity) -> u64 {
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

    pub fn replace_loose_items(&mut self, items: Vec<crate::entity::ItemEntity>) {
        self.next_loose_item_id = items
            .iter()
            .map(|item| item.stable_id)
            .max()
            .unwrap_or(LOOSE_ITEM_ID_BASE - 1)
            .saturating_add(1)
            .max(LOOSE_ITEM_ID_BASE);
        self.loose_items = items;
    }

    pub fn take_loose_items(&mut self) -> Vec<crate::entity::ItemEntity> {
        std::mem::take(&mut self.loose_items)
    }

    pub fn clear_loose_items(&mut self) {
        self.loose_items.clear();
    }

    pub fn for_each_loose_item_mut(
        &mut self,
        mut update: impl FnMut(&mut crate::entity::ItemEntity),
    ) {
        for item in &mut self.loose_items {
            update(item);
        }
    }

    /// Advance ordinary dropped-item physics and settle material/arcane loss
    /// at the same host authority that owns Nudge and pickup.
    pub(super) fn tick_loose_items(&mut self, dt: f32) {
        let pending = std::mem::take(&mut self.pending_drops);
        for (pos, stack) in pending {
            let id = self.next_loose_item_id.max(LOOSE_ITEM_ID_BASE);
            let angle = ((id ^ (id >> 31)) as u32) as f32 / u32::MAX as f32 * std::f32::consts::TAU;
            let mut item = crate::entity::ItemEntity::new(
                pos.entity_center(),
                glam::Vec3::new(angle.cos() * 1.5, 2.5, angle.sin() * 1.5),
                stack.item,
                stack.count,
            );
            item.durability = stack.durability;
            item.arcane_id = stack.arcane_id;
            self.spawn_loose_item(item);
        }

        let mut kept = Vec::with_capacity(self.loose_items.len());
        let mut lost = Vec::new();
        for mut item in std::mem::take(&mut self.loose_items) {
            if item.update(self, dt) {
                kept.push(item);
            } else {
                lost.push(item);
            }
        }
        self.loose_items = kept;
        let reg = self.reg.clone();
        for item in lost {
            let reason = item.loss_reason(self);
            let Some(pos) = item.pos.block() else {
                continue;
            };
            let mut stack = ItemStack::new(&reg, item.item, item.count);
            stack.durability = item.durability;
            stack.arcane_id = item.arcane_id;
            let implement_materials_handled = self.retire_arcane_stack_at(pos, stack, reason);
            if !implement_materials_handled
                && let Some(ledger) = &mut self.material_ledger
                && let Err(error) = ledger.bury_stack(&reg, pos, stack, reason)
            {
                eprintln!("materials: dropped-item salvage failed: {error}");
            }
        }
    }

    pub fn mobs(&self) -> &[Mob] {
        &self.mobs
    }

    pub(crate) fn mobs_mut(&mut self) -> &mut Vec<Mob> {
        &mut self.mobs
    }

    pub fn mob(&self, index: usize) -> Option<&Mob> {
        self.mobs.get(index)
    }

    pub fn mob_mut(&mut self, index: usize) -> Option<&mut Mob> {
        self.mobs.get_mut(index)
    }

    pub fn mob_by_id(&self, id: u32) -> Option<&Mob> {
        self.mobs.iter().find(|m| m.id == id)
    }

    pub fn mob_by_id_mut(&mut self, id: u32) -> Option<&mut Mob> {
        self.mobs.iter_mut().find(|mob| mob.id == id)
    }

    pub fn mob_count(&self) -> usize {
        self.mobs.len()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn npcs(&self) -> &[crate::npc::NpcInstance] {
        &self.npcs
    }

    pub fn npc_count(&self) -> usize {
        self.npcs.len()
    }

    /// The NPC instance driving the given mob, if that mob is an NPC.
    pub fn npc_by_mob(&self, mob_id: u32) -> Option<&crate::npc::NpcInstance> {
        self.npcs.iter().find(|n| n.mob_id == mob_id)
    }

    /// Spawn a def's companion mob plus its NPC instance in one call,
    /// returning the mob's stable id. Used by the script `spawn_npc` host fn
    /// and the assembly-marker consumer, which have only a def name/pos.
    pub fn spawn_npc_at(&mut self, def: usize, pos: crate::planet::EntityPos) -> Option<u32> {
        let npc = self.reg.npcs.get(def)?.clone();
        if self.mobs.len() >= MOB_CAP || self.npcs.len() >= NPC_CAP {
            return None;
        }
        let species = npc.species;
        let mob_id = if self.next_mob_id == u32::MAX {
            return None;
        } else {
            self.next_mob_id
        };
        self.next_mob_id += 1;
        let mut m = crate::mobs::Mob::new_at(species, pos, 0.0);
        m.health = self.reg.animals[species].health;
        m.id = mob_id;
        self.mobs.push(m);
        let mut instance = crate::npc::NpcInstance::new(&npc, pos, mob_id).with_def(def);
        instance.species = species;
        self.npcs.push(instance);
        Some(mob_id)
    }

    pub fn spawn_mob(&mut self, mob: Mob) {
        let mut mob = mob;
        if mob.id == 0 {
            if self.next_mob_id == u32::MAX {
                eprintln!("mobs: stable id space exhausted; refusing further spawns");
                return;
            }
            mob.id = self.next_mob_id;
            self.next_mob_id += 1;
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
        self.mobs.push(mob);
    }

    pub fn replace_mobs(&mut self, mobs: Vec<Mob>) {
        self.mobs = mobs;
    }

    fn arcane_region_at(&self, pos: EntityPos) -> Option<crate::planet_atlas::AtlasPos> {
        self.planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(pos.surface()))
    }

    fn retire_warden_current(
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
            let Some(mob) = self.mobs.get_mut(index) else {
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
        while index < self.mobs.len() {
            if self.mobs[index].health > 0.0 {
                index += 1;
                continue;
            }
            let mob = self.mobs.swap_remove(index);
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
            settled.push(SettledMobDeath {
                species: mob.species,
                pos: mob.pos,
            });
        }
        settled
    }

    pub fn for_each_mob_mut(&mut self, mut update: impl FnMut(&mut Mob)) {
        for mob in &mut self.mobs {
            update(mob);
        }
    }

    pub fn projectiles(&self) -> &[Projectile] {
        &self.projectiles
    }

    pub fn spawn_projectile(&mut self, projectile: Projectile) {
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

    pub fn replace_projectiles(&mut self, projectiles: Vec<Projectile>) {
        self.next_projectile_id = projectiles
            .iter()
            .map(|projectile| projectile.stable_id)
            .max()
            .unwrap_or_default()
            .saturating_add(1)
            .max(1);
        self.projectiles = projectiles;
    }

    pub fn for_each_projectile_mut(&mut self, mut update: impl FnMut(&mut Projectile)) {
        for projectile in &mut self.projectiles {
            update(projectile);
        }
    }

    pub(super) fn mob_hash_at(&self, pos: SurfacePos, salt: u32) -> u32 {
        crate::planet::seeded_surface_roll(self.seed, pos, salt)
    }

    fn animal_environment_suitable(
        &self,
        def: &crate::registry::AnimalDef,
        pos: SurfacePos,
        wet: bool,
        biome: &str,
    ) -> bool {
        if !def.biomes.iter().any(|allowed| allowed == biome) {
            return false;
        }
        let temperature = self.weather_at_surface(pos).temperature_c;
        if def
            .temperature_c
            .is_some_and(|range| temperature < range[0] || temperature > range[1])
        {
            return false;
        }
        let elevation = if self.chunks.contains_key(&ChunkPos::from_surface(pos)) {
            self.surface_height_at(pos)
        } else {
            self.generator.surface_estimate_at(pos)
        } as i16;
        if def
            .elevation
            .is_some_and(|range| elevation < range[0] || elevation > range[1])
        {
            return false;
        }
        let Some(atlas) = &self.planet_atlas else {
            return true;
        };
        let sample = atlas.biome_sample(pos);
        if def.vegetation.is_some_and(|range| {
            sample.vegetation_potential < range[0] || sample.vegetation_potential > range[1]
        }) {
            return false;
        }
        let climate = atlas.genesis.climate.values()[atlas.atlas_pos(pos).index(atlas.side())];
        def.habitats.iter().all(|tag| match tag.as_str() {
            "warm" => climate.mean_temperature >= 15.0,
            "cold" => climate.mean_temperature <= 7.0,
            "humid" => climate.mean_precipitation >= 780.0,
            "arid" => climate.aridity >= 1.02,
            "riparian" => sample.habitat_flags & crate::planet_atlas::HABITAT_RIPARIAN != 0,
            "wetland" => sample.habitat_flags & crate::planet_atlas::HABITAT_WETLAND != 0,
            "freshwater" => {
                sample.habitat_flags
                    & (crate::planet_atlas::HABITAT_AQUATIC_FRESH
                        | crate::planet_atlas::HABITAT_RIPARIAN
                        | crate::planet_atlas::HABITAT_WETLAND
                        | crate::planet_atlas::HABITAT_SPRING
                        | crate::planet_atlas::HABITAT_LAKESHORE)
                    != 0
            }
            "marine" => {
                sample.habitat_flags
                    & (crate::planet_atlas::HABITAT_AQUATIC_SALT
                        | crate::planet_atlas::HABITAT_BEACH_DUNE
                        | crate::planet_atlas::HABITAT_SALT_MARSH)
                    != 0
            }
            "alpine" => sample.habitat_flags & crate::planet_atlas::HABITAT_ALPINE != 0,
            "saline" => {
                sample.salinity >= 64
                    || sample.habitat_flags
                        & (crate::planet_atlas::HABITAT_AQUATIC_BRACKISH
                            | crate::planet_atlas::HABITAT_AQUATIC_SALT
                            | crate::planet_atlas::HABITAT_SALT_MARSH)
                        != 0
            }
            "volcanic_soil" => {
                sample.habitat_flags & crate::planet_atlas::HABITAT_VOLCANIC_SOIL != 0
            }
            "ocean" => wet && biome == "ocean",
            other => biome == other,
        })
    }

    fn animal_habitat_suitable(
        &self,
        def: &crate::registry::AnimalDef,
        pos: SurfacePos,
        wet: bool,
        biome: &str,
    ) -> bool {
        if !self.animal_environment_suitable(def, pos, wet, biome) {
            return false;
        }
        if def.prey.is_empty() {
            return true;
        }
        let prey_present = self.mobs.iter().any(|mob| {
            def.prey.contains(&mob.species)
                && mob.pos.block().is_some_and(|at| {
                    crate::planet::geodesic_distance(at.surface().center(), pos.center()) < 128.0
                })
        });
        prey_present
            || def.prey.iter().any(|species| {
                self.reg
                    .animals
                    .get(*species)
                    .is_some_and(|prey| self.animal_environment_suitable(prey, pos, wet, biome))
            })
    }

    fn animal_biome_name(&self, pos: SurfacePos, wet: bool) -> String {
        if wet {
            let salinity = self.surface_water_salinity_at(pos);
            if let Some(atlas) = &self.planet_atlas {
                let sample = atlas.biome_sample(pos);
                let atlas_salt = sample.habitat_flags
                    & (crate::planet_atlas::HABITAT_AQUATIC_BRACKISH
                        | crate::planet_atlas::HABITAT_AQUATIC_SALT)
                    != 0;
                if salinity.is_some_and(|value| value >= 64) || (salinity.is_none() && atlas_salt) {
                    return "ocean".to_string();
                }
                // Freshwater is an overlay on the surrounding terrestrial
                // ecology, not a salt ocean biome. A newly materialized
                // channel can have no water at this exact surface column
                // even though the wet caller and immutable atlas identify
                // its connected freshwater habitat, so use that atlas fact
                // as the fallback rather than the country's ocean label.
                let atlas_fresh =
                    sample.habitat_flags & crate::planet_atlas::HABITAT_AQUATIC_FRESH != 0;
                if salinity.is_none() && !atlas_fresh {
                    return self.country_biome_at(pos).name().to_lowercase();
                }
                let local = crate::worldgen::Biome::from_index(sample.zonal_biome)
                    .filter(|biome| *biome != crate::worldgen::Biome::Ocean)
                    .or_else(|| {
                        atlas.country(sample.country_id).and_then(|country| {
                            crate::worldgen::Biome::from_index(country.dominant_biome)
                        })
                    });
                if let Some(local) = local {
                    return local.name().to_lowercase();
                }
            }
            if salinity.is_some_and(|value| value >= 64) {
                return "ocean".to_string();
            }
        }
        self.country_biome_at(pos).name().to_lowercase()
    }

    fn atlas_animal_context(
        &self,
        pos: crate::planet_atlas::AtlasPos,
    ) -> (SurfacePos, bool, String) {
        let atlas = self
            .planet_atlas
            .as_ref()
            .expect("atlas context requires atlas");
        let center = pos.center(atlas.side());
        let surface = SurfacePos::new(
            center.face,
            center
                .u
                .floor()
                .clamp(0.0, f64::from(crate::planet::FACE_BLOCKS - 1)) as u16,
            center
                .v
                .floor()
                .clamp(0.0, f64::from(crate::planet::FACE_BLOCKS - 1)) as u16,
        )
        .expect("atlas center is a canonical surface cell");
        let sample = atlas.biome_sample(surface);
        let wet = sample.habitat_flags
            & (crate::planet_atlas::HABITAT_AQUATIC_FRESH
                | crate::planet_atlas::HABITAT_AQUATIC_BRACKISH
                | crate::planet_atlas::HABITAT_AQUATIC_SALT)
            != 0;
        let biome = if sample.habitat_flags
            & (crate::planet_atlas::HABITAT_AQUATIC_BRACKISH
                | crate::planet_atlas::HABITAT_AQUATIC_SALT)
            != 0
        {
            "ocean".to_string()
        } else if sample.habitat_flags & crate::planet_atlas::HABITAT_WETLAND != 0
            && !matches!(
                sample.zonal_biome,
                crate::planet_atlas::BIOME_ARCTIC
                    | crate::planet_atlas::BIOME_TUNDRA
                    | crate::planet_atlas::BIOME_MOUNTAINS
            )
        {
            // Wetland is an edaphic override of the broad zonal biome.
            // Mirroring WorldGenerator::biome_at here matters: otherwise a
            // frog can live in a loaded swamp but cannot migrate through the
            // same swamp while its neighboring chunk is unloaded.
            "swamp".to_string()
        } else {
            crate::worldgen::Biome::from_index(sample.zonal_biome)
                .filter(|biome| *biome != crate::worldgen::Biome::Ocean)
                .or_else(|| {
                    atlas.country(sample.country_id).and_then(|country| {
                        crate::worldgen::Biome::from_index(country.dominant_biome)
                    })
                })
                .unwrap_or_else(|| self.country_biome_at(surface))
                .name()
                .to_lowercase()
        };
        (surface, wet, biome)
    }

    /// Recovery/migration may use only a habitat cell joined to another
    /// suitable cell in the seam-safe atlas graph. Initial populations may
    /// occupy small refugia; once lost, those do not respawn from arbitrary
    /// chunk odds.
    fn animal_habitat_network_connected(
        &self,
        def: &crate::registry::AnimalDef,
        pos: SurfacePos,
    ) -> bool {
        let Some(atlas) = &self.planet_atlas else {
            return true;
        };
        atlas
            .atlas_pos(pos)
            .neighbors4(atlas.side())
            .into_iter()
            .map(|neighbor| self.atlas_animal_context(neighbor))
            .any(|(surface, wet, biome)| {
                self.animal_environment_suitable(def, surface, wet, &biome)
            })
    }

    #[cfg(test)]
    pub fn animal_habitat_suitable_at(&self, species: usize, pos: SurfacePos, wet: bool) -> bool {
        let Some(def) = self.reg.animals.get(species) else {
            return false;
        };
        let biome = self.animal_biome_name(pos, wet);
        self.animal_habitat_suitable(def, pos, wet, &biome)
    }

    #[cfg(test)]
    pub fn animal_habitat_network_connected_at(&self, species: usize, pos: SurfacePos) -> bool {
        self.reg
            .animals
            .get(species)
            .is_some_and(|def| self.animal_habitat_network_connected(def, pos))
    }

    /// Deterministic per-chunk wildlife roll: at most one species' group.
    pub(super) fn seed_wildlife(&mut self, pos: ChunkPos) {
        if self.mobs.len() >= MOB_CAP {
            return;
        }
        // Dead country restocks nothing.
        let center = SurfacePos::new(
            pos.face(),
            pos.u() * CHUNK_X as u16 + CHUNK_X as u16 / 2,
            pos.v() * CHUNK_Z as u16 + CHUNK_Z as u16 / 2,
        )
        .expect("chunk center is canonical");
        if !self.heart_alive_at_surface(center) {
            return;
        }
        let reg = self.reg.clone();
        // Salt sea keeps its own roster; fresh water inherits the climate
        // and habitat around its watershed.
        let here = self.animal_biome_name(center, true);
        for (si, def) in reg.animals.iter().enumerate() {
            // Wildlife only — wardens come and go with the spawner.
            // Swimmers roll in the water pass below; letting them share
            // this slot meant a fish only ever spawned in a chunk where
            // every land animal of the biome had already failed its
            // rarity roll, which is why the sea looked empty.
            if def.hostile
                || def.movement_swim
                || !self.animal_habitat_suitable(def, center, false, &here)
            {
                continue;
            }
            let roll = self.mob_hash_at(center, 7000 + si as u32);
            if !roll.is_multiple_of(def.rarity) {
                continue;
            }
            let span = def.group[1].saturating_sub(def.group[0]) + 1;
            let n = def.group[0] + (roll >> 8) % span;
            for i in 0..n {
                let h = self.mob_hash_at(center, 7100 + si as u32 * 31 + i);
                let surface = SurfacePos::new(
                    pos.face(),
                    pos.u() * CHUNK_X as u16 + (h % CHUNK_X as u32) as u16,
                    pos.v() * CHUNK_Z as u16 + ((h >> 8) % CHUNK_Z as u32) as u16,
                )
                .expect("sampled wildlife column is canonical");
                self.try_spawn_at(si, surface, (h >> 16) as f32 / 65535.0);
            }
            break; // one species per chunk keeps groups readable
        }
        // The water has its own roster, rolled independently of the land
        // above it — a chunk can carry deer on the bank and trout in the
        // river. Fresh water stocks the country's fish; salt water its
        // own.
        let swimmers: Vec<usize> = reg
            .animals
            .iter()
            .enumerate()
            .filter_map(|(index, def)| {
                (!def.hostile
                    && def.movement_swim
                    && self.animal_habitat_suitable(def, center, true, &here))
                .then_some(index)
            })
            .collect();
        let swimmer_start = if swimmers.is_empty() {
            0
        } else {
            // Use the high half of the mixed hash. Taking `% 2` here
            // accidentally made the low-bit quality of the coordinate
            // hash decide between the two ocean fish, and one species
            // could win every early chunk before the aquatic budget
            // filled.
            ((u64::from(self.mob_hash_at(center, 9_101)) * swimmers.len() as u64) >> 32) as usize
        };
        for step in 0..swimmers.len() {
            let si = swimmers[(swimmer_start + step) % swimmers.len()];
            let def = &reg.animals[si];
            let roll = self.mob_hash_at(center, 9200 + si as u32);
            if !roll.is_multiple_of(def.rarity) {
                continue;
            }
            let span = def.group[1].saturating_sub(def.group[0]) + 1;
            let n = def.group[0] + (roll >> 8) % span;
            let mut spawned = false;
            for i in 0..n {
                let h = self.mob_hash_at(center, 9300 + si as u32 * 31 + i);
                let surface = SurfacePos::new(
                    pos.face(),
                    pos.u() * CHUNK_X as u16 + (h % CHUNK_X as u32) as u16,
                    pos.v() * CHUNK_Z as u16 + ((h >> 8) % CHUNK_Z as u32) as u16,
                )
                .expect("sampled fish column is canonical");
                // Dry chunks simply fail every attempt: try_spawn wants a
                // water cell two deep and finds none.
                spawned |= self.try_spawn_at(si, surface, (h >> 16) as f32 / 65535.0);
            }
            // One shoal per chunk. Candidate order rotates by canonical
            // address so the first fish in the data file cannot fill the
            // global mob cap before later ocean natives ever get a turn.
            if spawned {
                break;
            }
        }
        // The dark has its own roster: underground species roll
        // independently of the surface (a chunk can carry deer above
        // and a bat colony below).
        for (si, def) in reg.animals.iter().enumerate() {
            if def.hostile || def.biomes.iter().all(|b| b != "underground") {
                continue;
            }
            let roll = self.mob_hash_at(center, 8600 + si as u32);
            if !roll.is_multiple_of(def.rarity) {
                continue;
            }
            let span = def.group[1].saturating_sub(def.group[0]) + 1;
            let n = def.group[0] + (roll >> 8) % span;
            for i in 0..n {
                let h = self.mob_hash_at(center, 8700 + si as u32 * 31 + i);
                let surface = SurfacePos::new(
                    pos.face(),
                    pos.u() * CHUNK_X as u16 + (h % CHUNK_X as u32) as u16,
                    pos.v() * CHUNK_Z as u16 + ((h >> 8) % CHUNK_Z as u32) as u16,
                )
                .expect("sampled cave column is canonical");
                // A pocket of cave: two air cells under a solid roof.
                let base = 8 + (h >> 16) % 32;
                let spot = (base as i32..(base as i32 + 24).min(52)).find(|&y| {
                    let at =
                        BlockPos::new(surface.face(), surface.u(), y as u8, surface.v()).unwrap();
                    self.get_block_at(at) == AIR
                        && at
                            .offset(0, 1, 0)
                            .is_some_and(|p| self.get_block_at(p) == AIR)
                        && at
                            .offset(0, 2, 0)
                            .is_some_and(|p| self.reg.is_solid(self.get_block_at(p)))
                });
                if let Some(y) = spot
                    && self.mobs.len() < MOB_CAP
                {
                    let at = EntityPos::new(
                        surface.face(),
                        f32::from(surface.u()) + 0.5,
                        y as f32 + 0.4,
                        f32::from(surface.v()) + 0.5,
                    )
                    .expect("cave spawn is canonical");
                    let mut m = Mob::new_at(si, at, (h >> 12) as f32);
                    m.health = reg.animals[si].health;
                    self.mobs.push(m);
                }
            }
        }
    }

    /// Spawn on dry solid ground at the surface — or, for swimmers,
    /// submerged in a water column at least two deep. Skips bad spots.
    pub(super) fn try_spawn_at(&mut self, species: usize, surface: SurfacePos, yaw01: f32) -> bool {
        if self.mobs.len() >= MOB_CAP {
            return false;
        }
        let Some(definition) = self.reg.animals.get(species) else {
            return false;
        };
        let swim = definition.movement_swim;
        if swim {
            let Some(habitat) = self.aquatic_habitat_at(surface) else {
                return false;
            };
            let preference = definition.aquatic.unwrap_or_default();
            if !(preference.depth_blocks[0]..=preference.depth_blocks[1])
                .contains(&habitat.depth_blocks)
                || (self.planet_atlas.is_some()
                    && (!(preference.temperature_c[0]..=preference.temperature_c[1])
                        .contains(&habitat.temperature_c)
                        || !(preference.discharge[0]..=preference.discharge[1])
                            .contains(&habitat.discharge)
                        || !(preference.salinity[0]..=preference.salinity[1])
                            .contains(&habitat.salinity)))
            {
                return false;
            }
        }
        // Category budget: a full lake never starves the land spawns.
        if swim {
            let reg = self.reg.clone();
            let fish = self
                .mobs
                .iter()
                .filter(|m| reg.animals.get(m.species).is_some_and(|d| d.movement_swim))
                .count();
            if fish >= 90 {
                return false;
            }
        }
        let spawn_at = if swim {
            // The first water cell from the sky down, needing depth.
            (4..=96)
                .rev()
                .filter_map(|y| {
                    BlockPos::new(surface.face(), surface.u(), y, surface.v())
                        .ok()
                        .map(|pos| (pos, self.get_block_at(pos)))
                })
                .find(|&(_, b)| self.reg.is_water(b))
                .filter(|&(pos, _)| {
                    pos.offset(0, -1, 0)
                        .is_some_and(|below| self.reg.is_water(self.get_block_at(below)))
                })
                .map(|(pos, _)| f32::from(pos.y()) - 0.6)
        } else {
            let y = self.surface_height_at(surface);
            let ground = BlockPos::new(surface.face(), surface.u(), y as u8, surface.v()).unwrap();
            let dry = y > SEA_LEVEL && self.reg.is_solid(self.get_block_at(ground));
            // A seabird has nowhere to stand, and the whole point of it
            // is that it is over the water. Wings only need air.
            let airborne = self.reg.animals[species].movement_float
                && BlockPos::new(
                    surface.face(),
                    surface.u(),
                    (SEA_LEVEL - 1) as u8,
                    surface.v(),
                )
                .is_ok_and(|pos| self.reg.is_water(self.get_block_at(pos)));
            if dry {
                Some(y as f32 + 1.05)
            } else if airborne {
                Some(SEA_LEVEL as f32 + 4.0)
            } else {
                None
            }
        };
        let Some(sy) = spawn_at else {
            return false;
        };
        let pos = EntityPos::new(
            surface.face(),
            f32::from(surface.u()) + 0.5,
            sy,
            f32::from(surface.v()) + 0.5,
        )
        .expect("wildlife spawn is canonical");
        let mut m = Mob::new_at(species, pos, yaw01 * std::f32::consts::TAU);
        m.health = self.reg.animals[species].health;
        self.mobs.push(m);
        true
    }

    /// Tick AI/physics for all mobs, plus the slow repopulation roll.
    /// Returns events (player hits, projectile casts) for the game loop.
    pub fn tick_mobs(
        &mut self,
        players: &[crate::server::PlayerCtx],
        fallback_daylight: f32,
        dt: f32,
        rng: &mut u32,
    ) -> Vec<MobEvent> {
        let fallback_player = players.first().map(|p| p.pos);
        let reg = self.reg.clone();
        let mut events = Vec::new();
        let fallback_season = fallback_player
            .map(|player| self.season_at_surface(player.surface()))
            .unwrap_or_else(|| crate::planet_atlas::local_season(self.day, 0.0));
        // Stamp stable ids on anything new (spawns, births, loaded saves).
        for m in &mut self.mobs {
            if m.id == 0 {
                m.id = self.next_mob_id;
                self.next_mob_id += 1;
            }
        }
        // Herd pulls are averaged in each animal's local tangent frame.
        // This costs little at the mob cap and lets a herd straddle a face
        // seam without splitting into two coordinate buckets.
        let herd_members: Vec<(usize, EntityPos)> = self
            .mobs
            .iter()
            .filter_map(|m| {
                let d = reg.animals.get(m.species)?;
                (!d.hostile && !d.vehicle && d.group[1] >= 2 && m.growth >= 1.0)
                    .then_some((m.species, m.pos))
            })
            .collect();
        let local_conditions: HashMap<u32, (usize, f32)> = self
            .mobs
            .iter()
            .map(|mob| {
                let surface = mob.pos.surface();
                (
                    mob.id,
                    (
                        self.season_at_surface(surface),
                        self.daylight_at_surface(surface),
                    ),
                )
            })
            .collect();
        // The trophic pre-pass: hungry predators pick their quarry,
        // desperation is graded (deep hunger plus night or winter),
        // and prey with a stalker on top of it bolts.
        let snapshot: Vec<(u32, usize, crate::planet::EntityPos)> =
            self.mobs.iter().map(|m| (m.id, m.species, m.pos)).collect();
        let mut spooked: Vec<(u32, crate::planet::EntityPos)> = Vec::new();
        for m in &mut self.mobs {
            let Some(d) = reg.animals.get(m.species) else {
                continue;
            };
            let (season, daylight) = local_conditions
                .get(&m.id)
                .copied()
                .unwrap_or((fallback_season, fallback_daylight));
            m.bold = !d.hostile
                && !d.fierce
                && !d.prey.is_empty()
                && d.attack > 0.0
                && m.belly < crate::mobs::BELLY_DESPERATE
                && (season == 3 || daylight < 0.35);
            if d.prey.is_empty() || d.belly_secs <= 0.0 || m.belly > 0.0 || m.growth < 1.0 {
                m.quarry = None;
                continue;
            }
            let mut best: Option<(u32, crate::planet::EntityPos, f32)> = None;
            for &(id, sp, pos) in &snapshot {
                if id == m.id || !d.prey.contains(&sp) {
                    continue;
                }
                let delta = m.pos.local_delta_to(pos);
                let dist = delta.length();
                if dist < crate::mobs::HUNT_RANGE && best.is_none_or(|(_, _, bd)| dist < bd) {
                    best = Some((id, pos, dist));
                }
            }
            m.quarry = best.map(|(id, pos, _)| (id, pos));
            if m.state == crate::mobs::MobState::Stalk
                && let Some((id, _, dist)) = best
                && dist < 7.0
            {
                spooked.push((id, m.pos));
            }
        }
        for (id, from) in spooked {
            if let Some(p) = self.mob_by_id_mut(id)
                && p.state != crate::mobs::MobState::Flee
            {
                p.state = crate::mobs::MobState::Flee;
                p.state_timer = 4.0;
                p.target = from;
            }
        }
        let mut mobs = std::mem::take(&mut self.mobs);
        // NPC walkers drive their companion mobs before the ordinary AI
        // pass: fixed NPCs idle, patrol NPCs walk their loop (spec 3.1).
        let mut npcs = std::mem::take(&mut self.npcs);
        for npc in &mut npcs {
            let Some(m) = mobs.iter_mut().find(|m| m.id == npc.mob_id) else {
                continue;
            };
            if let Some(cp) = m.pos.chunk()
                && self.chunks.contains_key(&cp)
            {
                npc.tick(m, dt);
            }
        }
        for m in &mut mobs {
            // Frozen until its chunk streams in: an unloaded chunk reads as
            // air, and ticking against it drops the mob through the world.
            let Some(cp) = m.pos.chunk() else {
                continue;
            };
            if !self.chunks.contains_key(&cp) {
                continue;
            }
            if let Some(def) = reg.animals.get(m.species) {
                let mut sum = glam::Vec3::ZERO;
                let mut count = 0.0;
                for &(species, other) in &herd_members {
                    if species == m.species {
                        let delta = m.pos.local_delta_to(other);
                        if delta.length_squared() <= 32.0 * 32.0 {
                            sum += delta;
                            count += 1.0;
                        }
                    }
                }
                m.herd_pull = (count >= 2.0)
                    .then(|| m.pos.translated(sum / count).ok().map(|moved| moved.pos))
                    .flatten();
                m.unstick(self, def);
                let before = m.pos;
                m.tick(self, def, players, dt, rng, &mut events);
                if def.name.contains(":warden")
                    && let Some(cell) = m.pos.block()
                    && self.resist_supernatural_pressure_at(
                        cell,
                        "warden",
                        def.attack.max(1.0).ceil() as u64,
                    )
                {
                    // A supplied ward resists rather than destroys the Wild's
                    // creature. Overload returns false and lets the crossing
                    // stand; successful resistance restores the exact ordinary
                    // pre-step position and leaves collision/AI authoritative.
                    m.pos = before;
                    m.vel = glam::Vec3::ZERO;
                    m.state_timer = m.state_timer.max(0.2);
                }
            }
        }
        // The kill lands: the prey leaves a carcass where it fell
        // (a scavenged carcass just goes — never a carcass's
        // carcass), and a laden pack spills. Predation moves the
        // ire meter not at all: the wild's own violence is its own.
        let killed: Vec<u32> = events
            .iter()
            .filter_map(|e| match e {
                MobEvent::Killed(id) => Some(*id),
                _ => None,
            })
            .collect();
        events.retain(|e| !matches!(e, MobEvent::Killed(_)));
        for id in killed {
            if let Some(i) = mobs.iter().position(|m| m.id == id) {
                let prey = mobs.swap_remove(i);
                let at = prey.pos.block();
                if let Some(cargo) = prey.cargo {
                    for st in cargo.into_iter().flatten() {
                        if let Some(at) = at {
                            self.push_drop_at(at, st);
                        }
                    }
                }
                let was_carcass = reg
                    .animals
                    .get(prey.species)
                    .is_some_and(|d| d.name.ends_with(":carcass"));
                if !was_carcass && let Some(ci) = reg.animal_id("base:carcass") {
                    let mut c = Mob::new_at(ci, prey.pos, prey.yaw);
                    c.health = reg.animals[ci].health;
                    c.rot = crate::mobs::CARCASS_ROT_SECS;
                    mobs.push(c);
                }
            }
        }
        // Rot: the ground takes whatever the vultures leave.
        let mut rotted: Vec<crate::planet::EntityPos> = Vec::new();
        let mut retired_cargo: Vec<(BlockPos, ItemStack)> = Vec::new();
        mobs.retain_mut(|m| {
            if reg
                .animals
                .get(m.species)
                .is_some_and(|d| d.name.ends_with(":carcass"))
            {
                m.rot -= dt;
                if m.rot <= 0.0 {
                    rotted.push(m.pos);
                    if let Some(cargo) = m.cargo.take() {
                        let at = m.pos.block().unwrap_or_else(|| {
                            let surface = m.pos.surface();
                            BlockPos::new(surface.face(), surface.u(), 1, surface.v())
                                .expect("canonical mob surface has a shell floor")
                        });
                        retired_cargo.extend(cargo.into_iter().flatten().map(|stack| (at, stack)));
                    }
                    return false;
                }
            }
            true
        });
        for p in rotted {
            if let Some(pos) = p
                .translated(glam::Vec3::new(0.0, -0.5, 0.0))
                .ok()
                .and_then(|moved| moved.pos.block())
            {
                self.feed_soil_at(pos, 8);
            }
        }
        // Wardens are expressions of the wild, not creatures: they dissolve
        // in daylight (sky-lit cells only — torchlight never banishes them)
        // and when the player leaves them far behind.
        let mut retired_current = Vec::new();
        mobs.retain_mut(|m| {
            let Some(def) = reg.animals.get(m.species) else {
                if let Some(cargo) = m.cargo.take() {
                    let at = m.pos.block().unwrap_or_else(|| {
                        let surface = m.pos.surface();
                        BlockPos::new(surface.face(), surface.u(), 1, surface.v())
                            .expect("canonical mob surface has a shell floor")
                    });
                    retired_cargo.extend(cargo.into_iter().flatten().map(|stack| (at, stack)));
                }
                return false;
            };
            let keep = if m.pos.y() < -20.0 {
                false // fell out of the world somehow
            } else if def.hostile && m.masterless {
                // Left over when the heart died and never recalled:
                // no daylight dissolves them, nothing sends them, and
                // they do not stop. Only distance retires them.
                let near = players
                    .iter()
                    .map(|p| m.pos.local_delta_to(p.pos).length_squared())
                    .fold(f32::INFINITY, f32::min);
                near <= 120.0 * 120.0
            } else if !def.hostile {
                // Fish are ambience-plus-resource: the water has
                // fish while someone's there to see it.
                if def.movement_swim {
                    let near = players
                        .iter()
                        .map(|p| m.pos.local_delta_to(p.pos).length_squared())
                        .fold(f32::INFINITY, f32::min);
                    near <= 96.0 * 96.0
                } else {
                    true
                }
            } else {
                let near = players
                    .iter()
                    .map(|p| m.pos.local_delta_to(p.pos).length_squared())
                    .fold(f32::INFINITY, f32::min);
                if near > 80.0 * 80.0 {
                    false
                } else if let Some(light_pos) = m
                    .pos
                    .translated(glam::Vec3::new(0.0, 0.5, 0.0))
                    .ok()
                    .and_then(|p| p.pos.block())
                {
                    let (_, sl) = self.light_at_pos(light_pos);
                    let local_daylight = local_conditions
                        .get(&m.id)
                        .map_or(fallback_daylight, |(_, daylight)| *daylight);
                    sl as f32 * local_daylight < 7.0
                } else {
                    false
                }
            };
            if !keep {
                if def.hostile
                    && let Some(arcane) = def.arcane.as_ref()
                {
                    retired_current.push((m.id, m.pos, arcane.on_destroy));
                }
                if let Some(cargo) = m.cargo.take() {
                    let at = m.pos.block().unwrap_or_else(|| {
                        let surface = m.pos.surface();
                        BlockPos::new(surface.face(), surface.u(), 1, surface.v())
                            .expect("canonical mob surface has a shell floor")
                    });
                    retired_cargo.extend(cargo.into_iter().flatten().map(|stack| (at, stack)));
                }
            }
            keep
        });
        for (id, pos, disposition) in retired_current {
            self.retire_warden_current(id, pos, disposition, "warden dissolved");
        }
        for (at, stack) in retired_cargo {
            self.push_drop_at(at, stack);
        }
        // Husbandry: two fed adults of a species near each other bear
        // young - but not in winter; spring is the birthing season.
        let mut births: Vec<(usize, usize)> = Vec::new();
        for i in 0..mobs.len() {
            let winter = local_conditions
                .get(&mobs[i].id)
                .is_some_and(|(season, _)| *season == 3);
            if winter || births.iter().any(|&(a, b)| a == i || b == i) {
                continue;
            }
            if !mobs[i].fed || mobs[i].growth < 1.0 {
                continue;
            }
            for j in (i + 1)..mobs.len() {
                if mobs[j].species == mobs[i].species
                    && mobs[j].fed
                    && mobs[j].growth >= 1.0
                    && mobs[i].pos.distance_to(mobs[j].pos).powi(2) < 16.0
                {
                    births.push((i, j));
                    break;
                }
            }
        }
        for (i, j) in births {
            let mid = mobs[i]
                .pos
                .translated(mobs[i].pos.local_delta_to(mobs[j].pos) * 0.5)
                .map(|p| p.pos)
                .unwrap_or(mobs[i].pos);
            mobs[i].fed = false;
            mobs[j].fed = false;
            mobs[i].breed_cd = 300.0;
            mobs[j].breed_cd = 300.0;
            if mobs.len() < MOB_CAP {
                let mut baby = Mob::new_at(mobs[i].species, mid, 0.0);
                baby.health = reg.animals[mobs[i].species].health;
                baby.growth = 0.05;
                mobs.push(baby);
                // Life returned to the world.
                self.ire = (self.ire - 1.0).max(0.0);
                events.push(MobEvent::Bred);
            }
        }
        self.mobs = mobs;
        self.npcs = npcs;

        // Repopulation: overhunted wildlife slowly recovers, away from the
        // player and only under the local cap.
        // Spring teems, winter starves: the repop clock runs at double
        // or half speed with the season.
        let repop_season = fallback_player
            .map(|player| self.season_at_surface(player.surface()))
            .unwrap_or(fallback_season);
        self.repop_timer += dt
            * match repop_season {
                0 => 2.0,
                3 => 0.5,
                _ => 1.0,
            };
        if self.repop_timer >= 16.0 {
            self.repop_timer = 0.0;
            // One player's ring per cycle, chosen at random — the same shape
            // the warden spawner already uses. Restocking only ever followed
            // players.first(), so on a shared world every guest but one lived
            // in a country that never recovered from being hunted.
            *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
            let player = players
                .get((*rng >> 8) as usize % players.len().max(1))
                .map(|p| p.pos)
                .or(fallback_player);
            let Some(player) = player else {
                return events;
            };
            let near = self
                .mobs
                .iter()
                .filter(|m| m.pos.distance_to(player) < 96.0)
                .count();
            if near < 40 && !reg.animals.is_empty() {
                *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                let r = *rng;
                // A ring 32-72 blocks out at a random angle.
                let ang = (r % 1024) as f32 / 1024.0 * std::f32::consts::TAU;
                let dist = 32.0 + ((r >> 10) % 40) as f32;
                let Some(surface) = player
                    .translated(glam::Vec3::new(ang.sin() * dist, 0.0, ang.cos() * dist))
                    .ok()
                    .and_then(|moved| moved.pos.block())
                    .map(BlockPos::surface)
                else {
                    return events;
                };
                let cp = ChunkPos::from_surface(surface);
                if self.chunks.contains_key(&cp) && self.heart_alive_at_surface(surface) {
                    // Restock what the spot can actually hold: a column
                    // of water gets swimmers, dry ground gets landfolk.
                    // Drawing both from one pool wasted most rolls out at
                    // sea, where every land pick fails to place.
                    let surface_y = self.surface_height_at(surface);
                    let wet = BlockPos::new(
                        surface.face(),
                        surface.u(),
                        (surface_y + 1).clamp(0, CHUNK_Y as i32 - 1) as u8,
                        surface.v(),
                    )
                    .is_ok_and(|pos| self.reg.is_water(self.get_block_at(pos)));
                    let biome = self.animal_biome_name(surface, wet);
                    // Wildlife only — wardens have their own spawner.
                    let eligible: Vec<usize> = reg
                        .animals
                        .iter()
                        .enumerate()
                        .filter(|(_, d)| {
                            let placeable = if wet {
                                // Over water: fish below it, wings above it.
                                d.movement_swim || d.movement_float
                            } else {
                                !d.movement_swim
                            };
                            !d.hostile
                                && placeable
                                && self.animal_habitat_suitable(d, surface, wet, &biome)
                                && self.animal_habitat_network_connected(d, surface)
                        })
                        .map(|(i, _)| i)
                        .collect();
                    if let Some(&si) = eligible.get(((r >> 20) as usize) % eligible.len().max(1)) {
                        self.try_spawn_at(si, surface, (r >> 8) as f32 / (u32::MAX >> 8) as f32);
                    }
                }
            }
        }
        events
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
        let idx = self
            .mobs
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                reg.animals.get(m.species).is_some_and(|d| d.movement_swim)
                    && m.pos.distance_to(at) < radius
            })
            .min_by(|(_, a), (_, b)| a.pos.distance_to(at).total_cmp(&b.pos.distance_to(at)))
            .map(|(i, _)| i)?;
        let fish = self.mobs.swap_remove(idx);
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

    /// Advance all bolts and arrows; returns (player index, damage) hits.
    /// Player arrows strike mobs through the normal hurt path and stick
    /// into blocks as recoverable item drops.
    pub fn tick_projectiles(
        &mut self,
        players: &[crate::server::PlayerCtx],
        dt: f32,
    ) -> Vec<(usize, f32, Option<String>)> {
        let mut dmg: Vec<(usize, f32, Option<String>)> = Vec::new();
        let mut mob_hits: Vec<(usize, f32, Option<String>, crate::planet::EntityPos)> = Vec::new();
        let mut drops: Vec<(crate::planet::BlockPos, crate::registry::ItemId)> = Vec::new();
        let mut preparation_spills: Vec<(crate::planet::BlockPos, ItemStack)> = Vec::new();
        let mut projectiles = std::mem::take(&mut self.projectiles);
        projectiles.retain_mut(|p| {
            if self.projectile_reserved_by_working(p.stable_id) {
                return true;
            }
            let prior_cell = p.pos.block();
            let hit = p.tick(self, players, dt);
            // Resistance is checked before dispatching the hit. Otherwise a
            // bolt that reaches a player in this very tick bypasses the ward
            // while a slower bolt one cell away is stopped.
            if !p.from_player
                && p.pos.block().is_some_and(|cell| {
                    self.resist_supernatural_pressure_at(
                        cell,
                        "projectile",
                        p.damage.max(1.0).ceil() as u64,
                    )
                })
            {
                if let (Some(at), Some(stack)) =
                    (p.pos.block().or(prior_cell), p.preparation_payload.take())
                {
                    preparation_spills.push((at, stack));
                }
                return false;
            }
            if !matches!(hit, ProjHit::None)
                && let (Some(at), Some(stack)) =
                    (p.pos.block().or(prior_cell), p.preparation_payload.take())
            {
                preparation_spills.push((at, stack));
            }
            match hit {
                ProjHit::None => true,
                ProjHit::Expired => false,
                ProjHit::Player(i) => {
                    // PvE-only mode: a player's arrow passes through other
                    // players instead of hurting them (capability E1).
                    if p.from_player && !self.ruleset().pvp {
                        return true;
                    }
                    dmg.push((i, p.damage, p.damage_type.clone()));
                    false
                }
                ProjHit::Mob(i) => {
                    let from = p
                        .pos
                        .translated(-p.vel * dt)
                        .map(|moved| moved.pos)
                        .unwrap_or(p.pos);
                    mob_hits.push((i, p.damage, p.damage_type.clone(), from));
                    false
                }
                ProjHit::Block => {
                    if let Some(it) = p.drop_item {
                        if p.owner != 0 {
                            // A guest's arrow: hand it back over the wire.
                            let stack = ItemStack::new(&self.reg, it, 1);
                            self.pending_gives.push((p.owner, stack));
                        } else {
                            let back = p
                                .pos
                                .translated(-p.vel * dt * 2.0)
                                .map(|moved| moved.pos)
                                .unwrap_or(p.pos);
                            if let Some(back) = back.block() {
                                drops.push((back, it));
                            }
                        }
                    }
                    false
                }
            }
        });
        self.projectiles = projectiles;
        let reg = self.reg.clone();
        for (i, d, dmg_type, from) in mob_hits {
            if let Some(m) = self.mobs.get_mut(i)
                && let Some(def) = reg.animals.get(m.species)
            {
                m.hurt(def, d, dmg_type.as_deref(), from);
            }
        }
        for (pos, it) in drops {
            self.push_drop_at(pos, ItemStack::new(&reg, it, 1));
        }
        for (pos, stack) in preparation_spills {
            match self.destroy_preparation_container_at(pos, stack, "thrown vessel impact") {
                Ok(true) => {}
                Ok(false) => {
                    // An unexpected non-preparation stable payload remains
                    // recoverable rather than being silently erased.
                    self.push_drop_at(pos, stack);
                }
                Err(error) => {
                    eprintln!(
                        "alchemy: failed to settle thrown vessel {} at {:?}: {error}",
                        stack.arcane_id, pos
                    );
                    self.push_drop_at(pos, stack);
                }
            }
        }
        dmg
    }

    /// Ire-driven warden spawner: territorial lurkers roll into the dark
    /// ring around the player. Never near the world spawn, never in light.
    /// Grade the vigils: a watcher stands down when its ground is
    /// mended (fading without a corpse), and graduates to the hunt
    /// when the grievance stands too long ignored.
    pub(crate) fn grade_watchers(&mut self) {
        let mut i = 0;
        while i < self.mobs.len() {
            let m = &self.mobs[i];
            if !m.watcher {
                i += 1;
                continue;
            }
            let Some(surface) = m.pos.block().map(BlockPos::surface) else {
                let retired = self.mobs.swap_remove(i);
                let disposition = self
                    .reg
                    .animals
                    .get(retired.species)
                    .and_then(|definition| definition.arcane.as_ref())
                    .map(|arcane| arcane.on_destroy);
                if let Some(disposition) = disposition {
                    self.retire_warden_current(
                        retired.id,
                        retired.pos,
                        disposition,
                        "watcher left valid terrain",
                    );
                }
                continue;
            };
            let cell = self.regional_ire_at_surface(surface);
            if cell < m.watch_baseline - 1.5 || self.ire_tier_at_surface(surface) == 0 {
                // The land was answered while it watched.
                self.whispers
                    .push("The watcher melts back into the trees.".to_string());
                let retired = self.mobs.swap_remove(i);
                let disposition = self
                    .reg
                    .animals
                    .get(retired.species)
                    .and_then(|definition| definition.arcane.as_ref())
                    .map(|arcane| arcane.on_destroy);
                if let Some(disposition) = disposition {
                    self.retire_warden_current(
                        retired.id,
                        retired.pos,
                        disposition,
                        "watcher stood down",
                    );
                }
                continue;
            }
            if m.watch_timer > 45.0 {
                self.whispers.push("The watching is over.".to_string());
                self.mobs[i].watcher = false;
            }
            i += 1;
        }
    }

    pub fn tick_hostile_spawns(
        &mut self,
        player: EntityPos,
        world_spawn: EntityPos,
        daylight: f32,
        dt: f32,
        rng: &mut u32,
    ) {
        if !self.ruleset().hostile_spawns {
            return;
        }
        self.hostile_spawn_timer += dt;
        if self.hostile_spawn_timer < 4.0 {
            return;
        }
        self.hostile_spawn_timer = 0.0;
        self.grade_watchers();
        // The wardens are the spirit's immune response. Where the
        // heart is dead they simply stop coming — and the silence is
        // the loudest thing this game ever does, because the player
        // has spent the whole game reading warden pressure as danger.
        let Some(player_surface) = player.block().map(BlockPos::surface) else {
            return;
        };
        if !self.heart_alive_at_surface(player_surface) {
            return;
        }
        let reg = self.reg.clone();
        // The tier as THIS ground feels it: an angry forest hunts
        // harder, a tended valley softer, wherever the world's mood.
        let tier = self.ire_tier_at_surface(player_surface);
        let mut budget = [2usize, 6, 10, 14][tier];
        // While a watcher watches, nothing else comes: the warning IS
        // the encounter until it's answered or it graduates.
        let watcher_near = self
            .mobs
            .iter()
            .any(|m| m.watcher && m.pos.distance_to(player) < 96.0);
        if watcher_near {
            return;
        }
        if self.ruleset().weather_extremes
            && self.weather_at_surface(player_surface).kind
                == crate::planet_atlas::LocalWeather::Storm
            && tier >= 2
        {
            budget += 1; // dark skies are cover
        }
        let near_hostiles = self
            .mobs
            .iter()
            .filter(|m| {
                reg.animals.get(m.species).is_some_and(|d| d.hostile)
                    && m.pos.distance_to(player) < 96.0
            })
            .count();
        if near_hostiles >= budget || self.mobs.len() >= MOB_CAP {
            return;
        }
        let roll = |rng: &mut u32| {
            *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
            *rng >> 8
        };
        for _ in 0..6 {
            let r = roll(rng);
            let ang = (r % 1024) as f32 / 1024.0 * std::f32::consts::TAU;
            let dist = 24.0 + ((r >> 10) % 32) as f32;
            let Some(surface) = player
                .translated(glam::Vec3::new(ang.sin() * dist, 0.0, ang.cos() * dist))
                .ok()
                .and_then(|moved| moved.pos.block())
                .map(BlockPos::surface)
            else {
                continue;
            };
            if !self.chunks.contains_key(&ChunkPos::from_surface(surface)) {
                continue;
            }
            let spawn_probe = EntityPos::new(
                surface.face(),
                f32::from(surface.u()) + 0.5,
                player.y(),
                f32::from(surface.v()) + 0.5,
            )
            .expect("hostile spawn probe is canonical");
            if spawn_probe.horizontal_distance_to(world_spawn) < 16.0 {
                continue;
            }
            // The wardens a country fields follow its heart too.
            let biome = self.country_biome_at(surface).name().to_lowercase();
            // Split the roster: surface wardens spawn at the surface, the
            // deep's own ("underground" biome tag) in caves below.
            let surface_y = self.surface_height_at(surface);
            let candidates: Vec<(usize, i32)> = reg
                .animals
                .iter()
                .enumerate()
                .filter(|(_, d)| {
                    let local =
                        (self.ire + self.regional_ire_at_surface(surface) * 3.0).clamp(0.0, 100.0);
                    d.hostile && local >= d.ire_min
                })
                .filter_map(|(i, d)| {
                    if d.biomes.iter().any(|b| b == "underground") {
                        // A random depth with a 2-tall air pocket.
                        let y = 6 + (roll(rng) % (surface_y.max(12) as u32 - 6)) as i32;
                        let at = BlockPos::new(surface.face(), surface.u(), y as u8, surface.v())
                            .ok()?;
                        let ground = at
                            .offset(0, -1, 0)
                            .map_or(AIR, |pos| self.get_block_at(pos));
                        let a1 = self.get_block_at(at);
                        let a2 = at.offset(0, 1, 0).map_or(AIR, |pos| self.get_block_at(pos));
                        (self.reg.is_solid(ground) && a1 == AIR && a2 == AIR).then_some((i, y))
                    } else if self.animal_habitat_suitable(d, surface, false, &biome)
                        && surface_y > SEA_LEVEL
                    {
                        Some((i, surface_y + 1))
                    } else {
                        None
                    }
                })
                .collect();
            if candidates.is_empty() {
                continue;
            }
            let (si, y) = candidates[(roll(rng) as usize) % candidates.len()];
            let def = &reg.animals[si];
            // Only one wrathwood walks at a time.
            if def.name.ends_with("wrathwood")
                && self.mobs.iter().any(|m| {
                    reg.animals
                        .get(m.species)
                        .is_some_and(|d| d.name.ends_with("wrathwood"))
                })
            {
                continue;
            }
            let Some(at) = BlockPos::new(surface.face(), surface.u(), y as u8, surface.v()).ok()
            else {
                continue;
            };
            let (bl, sl) = self.light_at_pos(at);
            let eff = (bl as f32).max(sl as f32 * daylight);
            if eff >= def.spawn_light_max as f32 {
                continue;
            }
            let entity = EntityPos::new(
                surface.face(),
                f32::from(surface.u()) + 0.5,
                y as f32 + 0.05,
                f32::from(surface.v()) + 0.5,
            )
            .expect("hostile spawn is canonical");
            let mut m = Mob::new_at(
                si,
                entity,
                (roll(rng) % 1024) as f32 / 1024.0 * std::f32::consts::TAU,
            );
            m.health = def.health;
            // The first surface warden into aggrieved country arrives
            // as a WATCHER: one warning at the treeline before any
            // hunt. (The deep gives no warnings.)
            let cell_ire = self.regional_ire_at_surface(surface);
            let is_surface = y == surface_y + 1;
            if is_surface && near_hostiles == 0 && cell_ire > 4.0 {
                m.watcher = true;
                m.watch_baseline = cell_ire;
                self.whispers
                    .push("Something watches from the treeline.".to_string());
            }
            self.spawn_mob(m);
            return; // one spawn per cycle
        }
    }
}
