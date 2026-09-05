//! Per-column vegetation and tree forms; seed salts and planting order are stable.

use super::Generator;
use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk, ChunkPos, SEA_LEVEL};
use crate::registry::AIR;
use super::Biome;

impl Generator {
    fn height_hint(&self, heights: &[[i32; CHUNK_Z]; CHUNK_X], lx: usize, lz: usize) -> i32 {
        heights[lx][lz]
    }

    pub(super) fn plant_trees(
        &self,
        c: &mut Chunk,
        pos: ChunkPos,
        heights: &[[i32; CHUNK_Z]; CHUNK_X],
        biomes: &[[Biome; CHUNK_Z]; CHUNK_X],
    ) {
        for lx in 2..CHUNK_X - 2 {
            for lz in 2..CHUNK_Z - 2 {
                let surface = Self::surface_in_chunk(pos, lx as i32, lz as i32);
                let zonal_biome = biomes[lx][lz];
                let ecology = self.atlas.as_ref().map(|atlas| atlas.biome_sample(surface));
                let biome = ecology.map_or(zonal_biome, |sample| {
                    if sample.habitat_flags
                        & (crate::planet_atlas::HABITAT_RIPARIAN
                            | crate::planet_atlas::HABITAT_OASIS)
                        != 0
                        && matches!(
                            zonal_biome,
                            Biome::Desert | Biome::Badlands | Biome::Scrubland
                        )
                    {
                        // A watered desert supports an open river woodland or
                        // oasis, not a closed temperate oak/birch forest.
                        Biome::Savanna
                    } else {
                        zonal_biome
                    }
                });
                let mut density = match biome {
                    Biome::Jungle => 22,
                    Biome::Taiga => 70,
                    Biome::Forest => 97,
                    Biome::Scrubland => 240,
                    Biome::Savanna => 150,
                    Biome::Swamp => 130,
                    Biome::Plains => 550,
                    Biome::Desert => 190, // cacti
                    Biome::Arctic
                    | Biome::Mountains
                    | Biome::Tundra
                    | Biome::Badlands
                    | Biome::Ocean => 0,
                };
                if let Some(sample) = ecology {
                    if i32::from(sample.tree_line_y) <= heights[lx][lz]
                        || sample.salinity >= 160
                        || sample.edaphic_flags & crate::planet_atlas::EDAPHIC_SHALLOW_ROCK != 0
                    {
                        density = 0;
                    } else if density > 0 {
                        density = (density * 128 / u32::from(sample.vegetation_potential.max(32)))
                            .clamp(12, 1_200);
                    }
                }
                // Jungle floor: undergrowth independent of trees —
                // lush, but no longer an endless buffet.
                if biome == Biome::Jungle {
                    let ur = self.hash_surface(0x0f01, surface);
                    // This bush bears real food. Keep the jungle visually
                    // tree-dense without turning its floor into a free
                    // orchard that makes farming irrelevant.
                    if ur.is_multiple_of(384) {
                        let h2 = self.height_hint(heights, lx, lz);
                        if h2 > SEA_LEVEL + 1
                            && h2 + 2 < CHUNK_Y as i32
                            && c.get(lx, h2 as usize, lz) == self.grass
                            && c.get(lx, (h2 + 1) as usize, lz) == AIR
                        {
                            let cover = if ur.is_multiple_of(48) {
                                self.mushroom
                            } else {
                                self.jungle_bush
                            };
                            c.set(lx, (h2 + 1) as usize, lz, cover);
                        }
                    }
                }
                // Wild food plants grow in scattered forage patches: a
                // coarse cell rolls for a patch, and only inside one do
                // columns roll for a plant (~1/384 overall, arriving as
                // clusters of a handful). Finding a berry patch or a
                // stand of wild wheat is a real find — and a seed
                // source — instead of groceries every few steps.
                let food_roll = self.hash_surface(0x5eed, surface);
                let patch = self.chunk_hash(0xf00d, pos).is_multiple_of(8);
                if patch && food_roll.is_multiple_of(48) && biome != Biome::Desert {
                    let h2 = self.height_hint(heights, lx, lz);
                    let plant = match biome {
                        Biome::Plains | Biome::Savanna => self.wild_wheat,
                        Biome::Swamp => self.mushroom,
                        Biome::Forest => {
                            if food_roll.is_multiple_of(2) {
                                self.wild_carrot
                            } else {
                                self.berry_bush
                            }
                        }
                        Biome::Taiga => {
                            if food_roll.is_multiple_of(2) {
                                self.wild_potato
                            } else {
                                self.mushroom
                            }
                        }
                        Biome::Jungle => self.jungle_bush,
                        _ => AIR,
                    };
                    if plant != AIR
                        && h2 > SEA_LEVEL + 1
                        && h2 + 2 < CHUNK_Y as i32
                        && c.get(lx, h2 as usize, lz) == self.grass
                        && c.get(lx, (h2 + 1) as usize, lz) == AIR
                    {
                        c.set(lx, (h2 + 1) as usize, lz, plant);
                        continue;
                    }
                }
                // The deep grows its own light: lantern fungus takes
                // root in cave pockets, the underground's first
                // native lamp.
                {
                    let cr = self.hash_surface(0xca9e, surface);
                    if cr.is_multiple_of(20) {
                        let start = (8 + (cr >> 8) % 30) as i32;
                        if let Some(fy) = (start..(start + 12).min(44)).find(|&fy| {
                            c.get(lx, fy as usize, lz) == AIR
                                && c.get(lx, (fy + 1) as usize, lz) == AIR
                                && {
                                    let floor = c.get(lx, (fy - 1) as usize, lz);
                                    floor != AIR && floor != self.water
                                }
                        }) {
                            c.set(lx, fy as usize, lz, self.lantern_fungus);
                        }
                    }
                }
                // Meadow flowers: scattered, useless, and worth it —
                // and they thicken toward a country's heart, which is
                // the only navigation the game offers between one
                // province and the next. No compass, no marker: the
                // ground gets busier and you follow it. Every biome
                // shows it, not just the flowery ones, because it is a
                // signal before it is decoration.
                {
                    let fr = self.hash_surface(0xf10e, surface);
                    let flowery = matches!(biome, Biome::Plains | Biome::Forest | Biome::Savanna);
                    let rate = match self.heart_nearness_at(surface) {
                        n if n > 0.82 => 6, // you are all but on it
                        n if n > 0.60 => 22,
                        n if n > 0.35 => 90,
                        n if n > 0.12 => 220,
                        _ if flowery => 340, // ordinary meadow
                        _ => 0,              // ordinary elsewhere: none
                    };
                    if rate > 0 && fr.is_multiple_of(rate) {
                        let h2 = self.height_hint(heights, lx, lz);
                        if h2 > SEA_LEVEL + 1
                            && h2 + 2 < CHUNK_Y as i32
                            && c.get(lx, h2 as usize, lz) == self.grass
                            && c.get(lx, (h2 + 1) as usize, lz) == AIR
                        {
                            let f = if fr.is_multiple_of(2) {
                                self.meadow_bloom
                            } else {
                                self.ember_poppy
                            };
                            c.set(lx, (h2 + 1) as usize, lz, f);
                        }
                    }
                }
                // The waterline flora: cattails stand where the land
                // meets the water table, lilies float on swamp glass,
                // kelp sways in the deeper cold.
                {
                    let h2 = self.height_hint(heights, lx, lz);
                    let wr = self.hash_surface(0x77a7, surface);
                    let shore = (SEA_LEVEL - 1..=SEA_LEVEL + 1).contains(&h2);
                    let touches_water = h2 >= 0
                        && h2 + 1 < CHUNK_Y as i32
                        && [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)]
                            .into_iter()
                            .any(|(dx, dz)| {
                                let x = (lx as i32 + dx) as usize;
                                let z = (lz as i32 + dz) as usize;
                                c.get(x, h2 as usize, z) == self.water
                                    || c.get(x, (h2 + 1) as usize, z) == self.water
                            });
                    let reed_odds = if biome == Biome::Swamp { 5 } else { 14 };
                    let salt_marsh = ecology.is_some_and(|sample| {
                        sample.habitat_flags & crate::planet_atlas::HABITAT_SALT_MARSH != 0
                    });
                    if shore
                        && touches_water
                        && wr.is_multiple_of(reed_odds)
                        && (c.get(lx, h2 as usize, lz) == self.grass
                            || (salt_marsh && c.get(lx, h2 as usize, lz) == self.mud))
                        && c.get(lx, (h2 + 1) as usize, lz) == AIR
                    {
                        c.set(lx, (h2 + 1) as usize, lz, self.cattail);
                    }
                    if h2 < SEA_LEVEL - 2 {
                        // Underwater ground with real depth above it.
                        if biome == Biome::Swamp
                            && wr.is_multiple_of(10)
                            && (SEA_LEVEL + 1) < CHUNK_Y as i32
                            && c.get(lx, (SEA_LEVEL + 1) as usize, lz) == AIR
                        {
                            c.set(lx, (SEA_LEVEL + 1) as usize, lz, self.water_lily);
                        } else if wr.is_multiple_of(9) {
                            let fronds = 1 + (wr >> 8) % 3;
                            for dy in 1..=fronds as i32 {
                                let y = h2 + dy;
                                if y < SEA_LEVEL && c.get(lx, y as usize, lz) == self.water {
                                    c.set(lx, y as usize, lz, self.kelp_frond);
                                }
                            }
                        }
                    }
                }
                if density == 0 || !self.hash_surface(0, surface).is_multiple_of(density) {
                    continue;
                }
                let h = heights[lx][lz];
                if h <= SEA_LEVEL + 1 || h + 11 >= CHUNK_Y as i32 {
                    continue;
                }
                let surface_block = c.get(lx, h as usize, lz);
                let rnd = self.hash_surface(0xabcd, surface);

                if biome == Biome::Desert {
                    if surface_block == self.sand {
                        let ch = 2 + (rnd % 2) as i32;
                        for y in 1..=ch {
                            c.set(lx, (h + y) as usize, lz, self.cactus);
                        }
                    }
                    continue;
                }
                if surface_block != self.grass {
                    continue;
                }
                c.set(lx, h as usize, lz, self.dirt);
                let base = h + 1;

                // Wood family per biome; forests mix oak with birch.
                let (wood, leaf) = match biome {
                    Biome::Taiga => (self.spruce_log, self.spruce_leaves),
                    Biome::Jungle => (self.jungle_log, self.jungle_leaves),
                    Biome::Scrubland | Biome::Savanna => (self.acacia_log, self.acacia_leaves),
                    Biome::Forest if rnd % 10 < 3 => (self.birch_log, self.birch_leaves),
                    _ => (self.log, self.leaves),
                };

                match biome {
                    Biome::Scrubland | Biome::Savanna => {
                        c.set(lx, base as usize, lz, wood);
                        for dx in -1i32..=1 {
                            for dz in -1i32..=1 {
                                let (x, y, z) = (lx as i32 + dx, base + 1, lz as i32 + dz);
                                if c.get(x as usize, y as usize, z as usize) == AIR {
                                    c.set(x as usize, y as usize, z as usize, leaf);
                                }
                            }
                        }
                    }
                    Biome::Taiga => {
                        let trunk_h = 5 + (rnd % 3) as i32;
                        for y in 0..trunk_h {
                            c.set(lx, (base + y) as usize, lz, wood);
                        }
                        let top = base + trunk_h;
                        for (dy, r) in [(-3i32, 2i32), (-2, 1), (-1, 2), (0, 1), (1, 1)] {
                            let r = if dy == -3 || dy == -1 { r } else { 1 };
                            for dx in -r..=r {
                                for dz in -r..=r {
                                    if dx.abs() == r && dz.abs() == r && r > 1 {
                                        continue;
                                    }
                                    if dx == 0 && dz == 0 && dy < 0 {
                                        continue;
                                    }
                                    let (x, y, z) = (lx as i32 + dx, top + dy, lz as i32 + dz);
                                    if y < 0 || y >= CHUNK_Y as i32 {
                                        continue;
                                    }
                                    if c.get(x as usize, y as usize, z as usize) == AIR {
                                        c.set(x as usize, y as usize, z as usize, leaf);
                                    }
                                }
                            }
                        }
                        c.set(lx, (top + 2).min(CHUNK_Y as i32 - 1) as usize, lz, leaf);
                    }
                    _ => {
                        let jungle = biome == Biome::Jungle;
                        // Jungle canopy rides high — and one tree in
                        // seven is an emergent towering over the rest.
                        let trunk_h = if jungle {
                            if rnd.is_multiple_of(7) {
                                13 + (rnd % 4) as i32
                            } else {
                                8 + (rnd % 4) as i32
                            }
                        } else {
                            4 + (rnd % 3) as i32
                        };
                        for y in 0..trunk_h {
                            c.set(lx, (base + y) as usize, lz, wood);
                        }
                        let top = base + trunk_h;
                        let big: i32 = if jungle { 3 } else { 2 };
                        for (dy, r) in [(-2i32, big), (-1, big), (0, 1), (1, 1)] {
                            for dx in -r..=r {
                                for dz in -r..=r {
                                    if dx == 0 && dz == 0 && dy < 0 {
                                        continue;
                                    }
                                    if dx.abs() == r && dz.abs() == r && (r >= 2 || dy == 1) {
                                        continue;
                                    }
                                    let (x, y, z) = (lx as i32 + dx, top + dy, lz as i32 + dz);
                                    if x < 0
                                        || x >= CHUNK_X as i32
                                        || z < 0
                                        || z >= CHUNK_Z as i32
                                        || y < 0
                                        || y >= CHUNK_Y as i32
                                    {
                                        continue;
                                    }
                                    if c.get(x as usize, y as usize, z as usize) == AIR {
                                        c.set(x as usize, y as usize, z as usize, leaf);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

    }
}
