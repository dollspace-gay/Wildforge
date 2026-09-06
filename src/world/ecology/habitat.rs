//! Read-only animal habitat, climate, and connected-water queries.

use crate::chunk::ChunkPos;
use crate::planet::SurfacePos;
use crate::world::World;

impl World {
    pub(in crate::world) fn mob_hash_at(&self, pos: SurfacePos, salt: u32) -> u32 {
        crate::planet::seeded_surface_roll(self.seed, pos, salt)
    }

    pub(super) fn animal_environment_suitable(
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

    pub(super) fn animal_habitat_suitable(
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
        let prey_present = self.population.mobs().iter().any(|mob| {
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

    pub(super) fn animal_biome_name(&self, pos: SurfacePos, wet: bool) -> String {
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

    pub(super) fn atlas_animal_context(
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
    pub(super) fn animal_habitat_network_connected(
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
}
