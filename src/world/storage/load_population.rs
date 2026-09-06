//! Load population storage transaction coordination.

use crate::planet::BlockPos;
use crate::chunk::ChunkPos;
use crate::world::Heart;
use crate::inventory::ItemStack;
use crate::mobs::Mob;
use crate::world::RegionCell;
use crate::world::RevealKey;
use crate::world::World;
use std::fs;

impl World {
    pub(in crate::world) fn load_mobs(&mut self) {
        use serde::Deserialize;
        fn one() -> f32 {
            1.0
        }
        #[derive(Deserialize)]
        struct PackT {
            index: usize,
            item: String,
            count: u32,
            #[serde(default)]
            durability: u32,
            #[serde(default)]
            arcane_id: u64,
        }
        #[derive(Deserialize)]
        struct MobT {
            species: String,
            face: u8,
            u: f32,
            y: f32,
            v: f32,
            yaw: f32,
            health: f32,
            #[serde(default)]
            fed: bool,
            #[serde(default = "one")]
            growth: f32,
            #[serde(default)]
            tamed: bool,
            #[serde(default)]
            tame_fed: u8,
            #[serde(default)]
            tame_need: u8,
            #[serde(default)]
            saddled: bool,
            #[serde(default)]
            pack: Vec<PackT>,
            #[serde(default = "grace")]
            belly: f32,
        }
        fn grace() -> f32 {
            240.0
        }
        #[derive(Deserialize)]
        struct FileT {
            version: u32,
            #[serde(default)]
            mob: Vec<MobT>,
        }
        if let Ok(text) = fs::read_to_string(self.mobs_path())
            && let Ok(f) = toml::from_str::<FileT>(&text)
            && f.version == 2
        {
            for t in f.mob {
                // Unknown species (mod removed) skip cleanly.
                let Some(si) = self.reg.animal_id(&t.species) else {
                    continue;
                };
                let Some(face) = crate::planet::Face::from_u8(t.face) else {
                    continue;
                };
                let Ok(pos) = crate::planet::EntityPos::new(face, t.u, t.y, t.v) else {
                    continue;
                };
                let mut m = Mob::new_at(si, pos, t.yaw);
                m.health = t.health.min(self.reg.animals[si].health);
                m.fed = t.fed;
                m.growth = t.growth.clamp(0.05, 1.0);
                m.tamed = t.tamed;
                m.tame_fed = t.tame_fed;
                m.tame_need = t.tame_need;
                m.belly = t.belly;
                if t.saddled {
                    let mut cargo: Box<[Option<ItemStack>; 12]> = Default::default();
                    for sl in &t.pack {
                        if sl.index < 12
                            && let Some(item) = self.reg.item_id(&sl.item)
                        {
                            cargo[sl.index] = Some(ItemStack {
                                item,
                                count: sl.count,
                                durability: sl.durability,
                                arcane_id: sl.arcane_id,
                            });
                        }
                    }
                    m.cargo = Some(cargo);
                }
                self.population.push_mob(m);
                // NPC companion species keep a runtime NpcInstance so their
                // patrol/dialogue survive a reload (spec 3.1 persistence).
                if let Some(species_idx) = self.reg.animal_id(&t.species)
                    && let Some((def_idx, npc)) = self
                        .reg
                        .npcs
                        .iter()
                        .enumerate()
                        .find(|(_, d)| d.species == species_idx)
                {
                    // The instance's mob_id must equal the companion Mob's
                    // stable id. Stamp it now instead of waiting for the lazy
                    // id pass in tick_mobs, so the two match immediately.
                    self.population.attach_loaded_npc(&npc.clone(), def_idx, pos);
                }
            }
        }
        if let Ok(data) = fs::read(self.save_dir.join("rire"))
            && let Some(body) = data.strip_prefix(b"WFR1")
        {
            for p in body.chunks_exact(8) {
                if let Some(face) = crate::planet::Face::from_u8(p[0]) {
                    let v = f32::from_le_bytes([p[4], p[5], p[6], p[7]]);
                    self.regional_ire.insert(
                        RegionCell {
                            face,
                            u: p[1],
                            v: p[2],
                        },
                        v.clamp(-20.0, 20.0),
                    );
                }
            }
        }
        if let Ok(data) = fs::read(self.save_dir.join("bloom"))
            && let Some(body) = data.strip_prefix(b"WFB1")
        {
            for p in body.chunks_exact(8) {
                if let Some(face) = crate::planet::Face::from_u8(p[0]) {
                    let v = f32::from_le_bytes([p[4], p[5], p[6], p[7]]);
                    self.bloom.insert(
                        RegionCell {
                            face,
                            u: p[1],
                            v: p[2],
                        },
                        v.clamp(0.0, 9.0),
                    );
                }
            }
        }
        self.calendar_state.long_winter() = fs::read(self.save_dir.join("longwinter"))
            .map(|d| d.first() == Some(&b'1'))
            .unwrap_or(false);
        if let Ok(data) = fs::read(self.save_dir.join("bspent"))
            && let Some(body) = data.strip_prefix(b"WFS1")
        {
            for p in body.chunks_exact(8) {
                if let Some(face) = crate::planet::Face::from_u8(p[0]) {
                    let v = f32::from_le_bytes([p[4], p[5], p[6], p[7]]);
                    self.bloom_spent.insert(
                        RegionCell {
                            face,
                            u: p[1],
                            v: p[2],
                        },
                        v.max(0.0),
                    );
                }
            }
        }
        if let Ok(data) = fs::read(self.save_dir.join("hearts"))
            && let Some(body) = data.strip_prefix(b"WFH4")
        {
            for p in body.chunks_exact(28) {
                let f32_at = |o: usize| f32::from_le_bytes([p[o], p[o + 1], p[o + 2], p[o + 3]]);
                let Some(key_face) = crate::planet::Face::from_u8(p[0]) else {
                    continue;
                };
                let Some(pos_face) = crate::planet::Face::from_u8(p[4]) else {
                    continue;
                };
                let Ok(pos) = BlockPos::new(
                    pos_face,
                    u16::from_le_bytes([p[5], p[6]]),
                    p[7],
                    u16::from_le_bytes([p[8], p[9]]),
                ) else {
                    continue;
                };
                self.hearts.insert(
                    crate::worldgen::ProvinceKey {
                        face: key_face,
                        u: p[1],
                        v: p[2],
                    },
                    Heart {
                        pos,
                        stage: p[10],
                        strain: f32_at(11),
                        rooting: f32_at(15),
                        graft: crate::worldgen::Biome::from_index(p[19]),
                        drift: f32_at(20),
                        regrow: f32_at(24),
                    },
                );
            }
        }
        if let Ok(data) = fs::read(self.save_dir.join("aseeded")) {
            for p in data
                .strip_prefix(b"WFA1")
                .unwrap_or_default()
                .chunks_exact(5)
            {
                if let Some(face) = crate::planet::Face::from_u8(p[0])
                    && let Ok(pos) = ChunkPos::new(
                        face,
                        u16::from_le_bytes([p[1], p[2]]),
                        u16::from_le_bytes([p[3], p[4]]),
                    )
                {
                    self.population.record_seeded(pos);
                }
            }
        }
        if let Ok(data) = fs::read(self.save_dir.join("ptouched")) {
            for p in data
                .strip_prefix(b"WFP1")
                .unwrap_or_default()
                .chunks_exact(5)
            {
                if let Some(face) = crate::planet::Face::from_u8(p[0])
                    && let Ok(pos) = ChunkPos::new(
                        face,
                        u16::from_le_bytes([p[1], p[2]]),
                        u16::from_le_bytes([p[3], p[4]]),
                    )
                {
                    self.player_touched.insert(pos);
                }
            }
        }
        if let Ok(data) = fs::read(self.save_dir.join("structured")) {
            for p in data
                .strip_prefix(b"WFS1")
                .unwrap_or_default()
                .chunks_exact(5)
            {
                if let Some(face) = crate::planet::Face::from_u8(p[0])
                    && let Ok(pos) = ChunkPos::new(
                        face,
                        u16::from_le_bytes([p[1], p[2]]),
                        u16::from_le_bytes([p[3], p[4]]),
                    )
                {
                    self.structure_chunks.insert(pos);
                }
            }
        }
        if let Ok(data) = fs::read(self.save_dir.join("gated")) {
            for p in data
                .strip_prefix(b"WFG1")
                .unwrap_or_default()
                .chunks_exact(8)
            {
                let u = u16::from_le_bytes([p[1], p[2]]);
                let v = u16::from_le_bytes([p[4], p[5]]);
                let Some(face) = crate::planet::Face::from_u8(p[0]) else {
                    continue;
                };
                let Ok(pos) = crate::planet::BlockPos::new(face, u, p[3], v) else {
                    continue;
                };
                let gate = u16::from_le_bytes([p[6], p[7]]) as usize;
                // Only keep gates that still resolve (a mod that removed a
                // gate def leaves the sealed block breakable-by-registry —
                // an unbreakable wall with no unlock is a softlock).
                if gate < self.reg.gates.len() {
                    self.gated.insert(pos, gate);
                }
            }
        }
        // Nest spawn-gates (capability E9): 8-byte records like the gates.
        // Only keep nests that still resolve; a stale record whose marker
        // block is gone is cleaned by the spawner when its chunk loads.
        if let Ok(data) = fs::read(self.save_dir.join("nests")) {
            for p in data
                .strip_prefix(b"WFN1")
                .unwrap_or_default()
                .chunks_exact(8)
            {
                let u = u16::from_le_bytes([p[1], p[2]]);
                let v = u16::from_le_bytes([p[4], p[5]]);
                let Some(face) = crate::planet::Face::from_u8(p[0]) else {
                    continue;
                };
                let Ok(pos) = crate::planet::BlockPos::new(face, u, p[3], v) else {
                    continue;
                };
                let nest = u16::from_le_bytes([p[6], p[7]]) as usize;
                if nest < self.reg.nests.len() {
                    self.nests.insert(pos, nest);
                }
            }
        }
        // Settlement hidden cells (spec 3.4): 6-byte position + 2-byte
        // settlement index + 1-byte tier, 9 bytes per record.
        if let Ok(data) = fs::read(self.save_dir.join("settlements")) {
            for p in data
                .strip_prefix(b"WFST1")
                .unwrap_or_default()
                .chunks_exact(9)
            {
                let u = u16::from_le_bytes([p[1], p[2]]);
                let v = u16::from_le_bytes([p[4], p[5]]);
                let Some(face) = crate::planet::Face::from_u8(p[0]) else {
                    continue;
                };
                let Ok(pos) = crate::planet::BlockPos::new(face, u, p[3], v) else {
                    continue;
                };
                let settlement = u16::from_le_bytes([p[6], p[7]]) as usize;
                let tier = p[8];
                // Only keep cells whose settlement and tier still resolve;
                // a removed tier/def is treated as already revealed.
                if settlement < self.reg.settlements.len()
                    && self.reg.settlements[settlement]
                        .tiers
                        .iter()
                        .any(|t| t.tier == u32::from(tier))
                {
                    self.hidden.insert(
                        pos,
                        RevealKey {
                            settlement,
                            tier: u32::from(tier),
                        },
                    );
                }
            }
        }
    }
}
