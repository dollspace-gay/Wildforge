//! Structures chunks transaction coordination.

use crate::registry::AIR;
use crate::world::BlockEntity;
use crate::planet::BlockPos;
use crate::world::CHEST_SLOTS;
use crate::chunk::CHUNK_X;
use crate::chunk::CHUNK_Y;
use crate::chunk::CHUNK_Z;
use crate::world::ChestState;
use crate::chunk::ChunkPos;
use crate::inventory::ItemStack;
use crate::chunk::SEA_LEVEL;
use crate::world::World;

impl World {
    // ---------------- ruins ----------------

    /// Deterministic per-chunk structure roll (at most one per chunk).
    pub(in crate::world) fn seed_structures(&mut self, pos: ChunkPos) {
        // A chunk already claimed by a structure or piece assembly never
        // rolls its own: multi-chunk assemblies reserve every touched chunk
        // in `structure_chunks` *before* `ensure_chunk`, so a neighbor that
        // arrives mid-walk (and any re-generation) early-returns here.
        if self.structure_chunks.contains(&pos) {
            return;
        }
        let reg = self.reg.clone();
        let center = crate::planet::SurfacePos::new(
            pos.face(),
            pos.u() * CHUNK_X as u16 + CHUNK_X as u16 / 2,
            pos.v() * CHUNK_Z as u16 + CHUNK_Z as u16 / 2,
        )
        .expect("chunk center is canonical");
        let biome = self.generator.biome_at(center).name().to_lowercase();
        for (si, st) in reg.structures.iter().enumerate() {
            if !st.biomes.contains(&biome) {
                continue;
            }
            let h = self.mob_hash_at(center, 9000 + si as u32);
            // The takers' cities stand in barren country because the
            // barrenness is the receipt: they are twice as common on
            // ground that stopped giving. (Worldgen cannot know which
            // hearts a player will kill, so it reads the signature —
            // exhausted, thin-soiled country — instead.)
            let barren = matches!(
                self.generator.biome_at(center),
                crate::worldgen::Biome::Badlands
                    | crate::worldgen::Biome::Scrubland
                    | crate::worldgen::Biome::Tundra
                    | crate::worldgen::Biome::Desert
            );
            let rarity = if barren {
                (st.rarity / 2).max(1)
            } else {
                st.rarity
            };
            if !h.is_multiple_of(rarity) {
                continue;
            }
            let w = st.layers[0].first().map(|r| r.len()).unwrap_or(0) as i32;
            let d = st.layers[0].len() as i32;
            if w == 0 || w > 14 || d > 14 {
                continue;
            }
            let origin_surface = crate::planet::SurfacePos::new(
                pos.face(),
                pos.u() * CHUNK_X as u16
                    + (1 + ((h >> 8) as i32).rem_euclid((15 - w).max(1))) as u16,
                pos.v() * CHUNK_Z as u16
                    + (1 + ((h >> 16) as i32).rem_euclid((15 - d).max(1))) as u16,
            )
            .expect("structure origin is inside its chunk");
            let sample = crate::planet::SurfacePos::canonicalized(
                origin_surface.face(),
                i32::from(origin_surface.u()) + w / 2,
                i32::from(origin_surface.v()) + d / 2,
            )
            .expect("structure center canonicalizes");
            let surface_y = self.surface_height_at(sample);
            if surface_y <= SEA_LEVEL + 1 || surface_y >= CHUNK_Y as i32 - 24 {
                continue;
            }
            let y0 = match st.buried {
                None => surface_y,
                Some((min, max)) => {
                    let depth = min + (h >> 4).rem_euclid((max - min + 1) as u32) as i32;
                    (surface_y - depth).max(6)
                }
            };
            let origin = BlockPos::new(
                origin_surface.face(),
                origin_surface.u(),
                y0 as u8,
                origin_surface.v(),
            )
            .expect("structure base is inside the world");
            self.place_structure_at(si, origin, h);
            // A fixed-template ruin won this chunk; skip assemblies.
            return;
        }
        // Piece assemblies (spec Part 2.3). At most one structure roll wins
        // per chunk; the ruin loop already returned if one landed, so the
        // origin chunk is unreserved here.
        for (ai, asm) in reg.assemblies.iter().enumerate() {
            if !asm.biomes.contains(&biome) {
                continue;
            }
            let h = self.mob_hash_at(center, 9000 + 0x10000 + ai as u32);
            if !h.is_multiple_of(asm.rarity) {
                continue;
            }
            let (markers, _) = self.place_assembly(asm.clone(), pos, h);
            // Spec 2.4/2.5 seam — first consumer: `spawn:npc:<id>` markers
            // place their NPC at the resolved world position. The NPC id is
            // `mod:npc` qualified; an unknown id is silently skipped (the
            // piece stays, its occupant just isn't there).
            for marker in markers {
                if let Some(npc_name) = marker.kind.strip_prefix("spawn:npc:")
                    && let Some(ni) = reg.npc_id(npc_name)
                {
                    let at = marker.at;
                    let surface = at.surface();
                    let y = self.surface_height_at(surface) as u8;
                    if let Ok(pos) = crate::planet::EntityPos::new(
                        surface.face(),
                        f32::from(surface.u()) + 0.5,
                        f32::from(y) + 1.05,
                        f32::from(surface.v()) + 0.5,
                    ) {
                        self.spawn_npc_at(ni, pos);
                    }
                }
                // Spec 2.5: `feature:<id>` markers place a sealed block at
                // the resolved position, locked until the player's KV flag
                // reads the gate's `value`. The gate id is `mod:gate`
                // qualified; an unknown id is silently skipped so a missing
                // def cannot leave a permanent unbreakable wall.
                if let Some(gate_name) = marker.kind.strip_prefix("feature:")
                    && let Some(gate) = reg.gate_id(gate_name)
                {
                    self.place_gate_at(gate, marker.at);
                }
                // Capability E9: `spawn:nest:<id>` markers place a nest
                // spawn-gate block at the resolved position. Placing the
                // block through the ordinary block path records the nest
                // automatically; an unknown id is silently skipped.
                if let Some(nest_name) = marker.kind.strip_prefix("spawn:nest:")
                    && let Some(nest_index) = reg.nests.iter().position(|nest| {
                        nest.id == nest_name || nest.id == format!("base:{nest_name}")
                    })
                {
                    let nest = &reg.nests[nest_index];
                    self.set_block_at(marker.at, nest.block);
                }
            }
            break;
        }
    }

    /// Stamp a structure template into the world (clipped writes via
    /// set_block; chests get rolled loot and belong to the wild).
    pub fn place_structure_at(&mut self, si: usize, origin: BlockPos, seed: u32) {
        let reg = self.reg.clone();
        let Some(st) = reg.structures.get(si).cloned() else {
            return;
        };
        self.structure_chunks.insert(origin.chunk());
        let chest_block = reg.block_id("base:chest");
        let mut rng = seed ^ 0x5f37_59df;
        let mut inherited_stacks = Vec::new();
        let mut inherited_placements = Vec::new();
        for (ly, layer) in st.layers.iter().enumerate() {
            for (lz, row) in layer.iter().enumerate() {
                for (lx, ch) in row.chars().enumerate() {
                    let Some(pos) = origin.offset(lx as i32, ly as i32, lz as i32) else {
                        continue;
                    };
                    match ch {
                        '.' => {}
                        '~' => self.set_block_at(pos, AIR),
                        'C' => {
                            if let Some(cb) = chest_block {
                                self.set_block_at(pos, cb);
                                let mut state = ChestState {
                                    wild_owned: true,
                                    ..Default::default()
                                };
                                if let Some(table) = &st.loot {
                                    let n = 3 + (rng % 3) as usize;
                                    let mut loot = self.roll_loot(table, n as u32, &mut rng);
                                    if st.name == "base:observational_outpost" {
                                        for name in [
                                            "base:etched_tablet",
                                            "base:maker_calibration_plate",
                                            "base:spent_charm_fitting",
                                            "base:broken_focus",
                                            "base:sealed_dross_ampoule",
                                            "base:site_survey_marks",
                                            "base:failed_containment_fragment",
                                        ] {
                                            if let Some(item) = reg.item_id(name) {
                                                loot.push(ItemStack::new(&reg, item, 1));
                                            }
                                        }
                                    }
                                    for (i, mut stck) in loot.into_iter().enumerate() {
                                        if i < CHEST_SLOTS {
                                            if self.arcane_ledger.is_some()
                                                && let Err(error) = self.bind_arcane_stack_at(
                                                    pos,
                                                    &mut stck,
                                                    "ruin inheritance",
                                                )
                                            {
                                                eprintln!(
                                                    "arcane: ruin loot could not bind: {error}"
                                                );
                                                continue;
                                            }
                                            if self.discovery_state.is_some()
                                                && let Err(error) =
                                                    self.bind_discovery_stack_at(pos, &mut stck)
                                            {
                                                eprintln!(
                                                    "discovery: ruin artifact could not bind: {error}"
                                                );
                                                continue;
                                            }
                                            // Scatter through the chest.
                                            let preferred =
                                                (i * 7 + (rng % 5) as usize) % CHEST_SLOTS;
                                            let slot = (0..CHEST_SLOTS)
                                                .map(|offset| (preferred + offset) % CHEST_SLOTS)
                                                .find(|slot| state.slots[*slot].is_none())
                                                .unwrap_or(preferred);
                                            inherited_stacks.push(stck);
                                            state.slots[slot] = Some(stck);
                                        }
                                    }
                                }
                                self.installations.insert(pos, BlockEntity::Chest(state));
                            }
                        }
                        c => {
                            if let Some(b) = st.palette.get(&c) {
                                self.set_block_at(pos, *b);
                                let materials = reg.block(*b).materials.clone();
                                if !materials.is_empty() {
                                    inherited_placements.push((pos, materials));
                                }
                            }
                        }
                    }
                }
            }
        }
        // Buried ruins leave a hint on the surface: a chimney stub.
        if st.buried.is_some()
            && let Some(cob) = reg.block_id("base:cobblestone")
            && let Some(hint) = origin.offset(1, 0, 1)
        {
            let sy = self.surface_height_at(hint.surface());
            if let Ok(base) = BlockPos::new(hint.face(), hint.u(), (sy + 1) as u8, hint.v()) {
                self.set_block_at(base, cob);
                if let Some(top) = base.offset(0, 1, 0) {
                    self.set_block_at(top, cob);
                }
            }
        }
        if let Some(ledger) = &mut self.material_ledger
            && let Err(error) = ledger.record_external_world_content(
                &reg,
                &inherited_stacks,
                &inherited_placements,
                "pre-genesis ruin inheritance",
            )
        {
            eprintln!("materials: ruin inheritance accounting failed: {error}");
        }
    }

    #[cfg(test)]
    pub fn place_structure(&mut self, si: usize, x: i32, y: i32, z: i32, seed: u32) {
        if let Some(origin) = BlockPos::of_world(x, y, z) {
            self.place_structure_at(si, origin, seed);
        }
    }
}
