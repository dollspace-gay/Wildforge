//! Mob/chunk persistence, planetary chunk streaming, saves, and registry remapping.

mod decoder;
mod encoder;
#[cfg(test)]
pub(crate) use encoder::encode_chunk;
pub(crate) use encoder::encode_stream_chunk;
mod palette;
mod palette_store;
mod reader;
mod region_store;
pub(super) use palette_store::PaletteStore;
pub(crate) use reader::{ChunkLoader, ChunkRead, ChunkRevision};
pub(super) use region_store::RegionStore;

use super::*;

impl World {
    pub(super) fn encode_loose_items(&self) -> std::io::Result<Vec<u8>> {
        use serde::Serialize;

        #[derive(Serialize)]
        struct StoredDrop {
            stable_id: u64,
            pos: crate::planet::EntityPos,
            vel: [f32; 3],
            item: String,
            count: u32,
            age: f32,
            durability: u32,
            arcane_id: u64,
        }
        #[derive(Serialize)]
        struct File {
            version: u32,
            drop: Vec<StoredDrop>,
        }
        let drop = self.population.loose_items()
            .iter()
            .filter(|item| item.stable_id != 0 && item.count != 0)
            .map(|item| StoredDrop {
                stable_id: item.stable_id,
                pos: item.pos,
                vel: item.vel.to_array(),
                item: self.reg.item(item.item).name.clone(),
                count: item.count,
                age: item.age,
                durability: item.durability,
                arcane_id: item.arcane_id,
            })
            .collect();
        toml::to_string_pretty(&File { version: 3, drop })
            .map(String::into_bytes)
            .map_err(std::io::Error::other)
    }

    pub(super) fn save_loose_items(&self) -> std::io::Result<()> {
        #[cfg(test)]
        if self.fail_loose_item_save {
            return Err(std::io::Error::other(
                "injected loose-item sidecar save failure",
            ));
        }
        crate::identity::atomic_write(
            &self.save_dir.join("loose-items.toml"),
            &self.encode_loose_items()?,
            false,
        )
    }

    pub(super) fn load_loose_items(&mut self) {
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct StoredDrop {
            #[serde(default)]
            stable_id: u64,
            pos: crate::planet::EntityPos,
            vel: [f32; 3],
            item: String,
            count: u32,
            age: f32,
            durability: u32,
            #[serde(default)]
            arcane_id: u64,
        }
        #[derive(Deserialize)]
        struct File {
            version: u32,
            #[serde(default)]
            drop: Vec<StoredDrop>,
        }
        let Ok(text) = fs::read_to_string(self.save_dir.join("loose-items.toml")) else {
            return;
        };
        let Ok(file) = toml::from_str::<File>(&text) else {
            eprintln!("items: could not parse loose-items.toml; file left untouched");
            return;
        };
        if !(1..=3).contains(&file.version) {
            eprintln!(
                "items: unsupported loose item save version {}",
                file.version
            );
            return;
        }
        let mut loaded = Vec::new();
        for stored in file.drop {
            let Some(item) = self.reg.item_id(&stored.item) else {
                eprintln!("items: retained unknown loose item name {}", stored.item);
                continue;
            };
            if stored.count == 0
                || !stored.age.is_finite()
                || stored.vel.iter().any(|value| !value.is_finite())
            {
                continue;
            }
            let mut entity = crate::entity::ItemEntity::new(
                stored.pos,
                glam::Vec3::from_array(stored.vel),
                item,
                stored.count,
            );
            entity.stable_id = stored.stable_id;
            entity.age = stored.age.max(0.0);
            entity.durability = stored.durability.min(self.reg.item(item).durability);
            entity.arcane_id = stored.arcane_id;
            loaded.push(entity);
        }
        for item in loaded {
            self.spawn_loose_item(item);
        }
    }

    pub(super) fn mobs_path(&self) -> PathBuf {
        self.save_dir.join("animals.toml")
    }

    pub(super) fn save_mobs(&self) -> Vec<SaveFailure> {
        use std::fmt::Write as _;
        let mut report = SaveReport::default();
        let mut out = String::from("version = 2\n");
        for m in self.population.mobs() {
            let Some(def) = self.reg.animals.get(m.species) else {
                continue;
            };
            if def.hostile || def.movement_swim {
                // Wardens dissolve on save; fish are the water's,
                // not individuals — both respawn from their sources.
                continue;
            }
            let _ = writeln!(
                out,
                "[[mob]]\nspecies = \"{}\"\nface = {}\nu = {:?}\ny = {:?}\nv = {:?}\nyaw = {:?}\nhealth = {:?}\nfed = {}\ngrowth = {:?}\ntamed = {}\ntame_fed = {}\ntame_need = {}\nsaddled = {}\nbelly = {:?}",
                def.name,
                m.pos.face() as u8,
                m.pos.u(),
                m.pos.y(),
                m.pos.v(),
                m.yaw,
                m.health,
                m.fed,
                m.growth,
                m.tamed,
                m.tame_fed,
                m.tame_need,
                m.cargo.is_some(),
                m.belly.max(0.0)
            );
            if let Some(cargo) = &m.cargo {
                for (i, st) in cargo.iter().enumerate() {
                    if let Some(st) = st {
                        let _ = writeln!(
                            out,
                            "[[mob.pack]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}\narcane_id = {}",
                            self.reg.item(st.item).name,
                            st.count,
                            st.durability,
                            st.arcane_id
                        );
                    }
                }
            }
            let _ = writeln!(out);
        }
        let path = self.mobs_path();
        report.record(
            "animals",
            path.clone(),
            super::persistence::replace_or_remove(
                &path,
                (!out.is_empty()).then_some(out.as_bytes()),
            ),
        );
        // Face-aware regional ledgers. Each record is
        // (face, region-u, region-v, reserved, value).
        let mut rb = Vec::with_capacity(4 + self.regional_ire.len() * 8);
        rb.extend_from_slice(b"WFR1");
        for (&cell, &v) in &self.regional_ire {
            rb.extend_from_slice(&[cell.face as u8, cell.u, cell.v, 0]);
            rb.extend_from_slice(&v.to_le_bytes());
        }
        let path = self.save_dir.join("rire");
        report.record(
            "regional ire",
            path.clone(),
            super::persistence::replace_or_remove(
                &path,
                (!self.regional_ire.is_empty()).then_some(rb.as_slice()),
            ),
        );
        let mut bb = Vec::with_capacity(4 + self.bloom.len() * 8);
        bb.extend_from_slice(b"WFB1");
        for (&cell, &v) in &self.bloom {
            bb.extend_from_slice(&[cell.face as u8, cell.u, cell.v, 0]);
            bb.extend_from_slice(&v.to_le_bytes());
        }
        let path = self.save_dir.join("bloom");
        report.record(
            "bloom ledger",
            path.clone(),
            super::persistence::replace_or_remove(
                &path,
                (!self.bloom.is_empty()).then_some(bb.as_slice()),
            ),
        );
        let path = self.save_dir.join("longwinter");
        report.record(
            "long winter",
            path.clone(),
            super::persistence::atomic_replace(&path, if self.long_winter { b"1" } else { b"0" }),
        );
        // The ground's spent willingness to bloom.
        let mut sb = Vec::with_capacity(4 + self.bloom_spent.len() * 8);
        sb.extend_from_slice(b"WFS1");
        for (&cell, &v) in &self.bloom_spent {
            sb.extend_from_slice(&[cell.face as u8, cell.u, cell.v, 0]);
            sb.extend_from_slice(&v.to_le_bytes());
        }
        let path = self.save_dir.join("bspent");
        report.record(
            "bloom exhaustion",
            path.clone(),
            super::persistence::replace_or_remove(
                &path,
                (!self.bloom_spent.is_empty()).then_some(sb.as_slice()),
            ),
        );
        // Planetary hearts: province face/grid address, canonical block
        // site, stage, strain, rooting, graft, drift, and cutting timer.
        let mut hb = Vec::with_capacity(4 + self.hearts.len() * 28);
        hb.extend_from_slice(b"WFH4");
        for (&key, h) in &self.hearts {
            hb.extend_from_slice(&[key.face as u8, key.u, key.v, 0]);
            hb.push(h.pos.face() as u8);
            hb.extend_from_slice(&h.pos.u().to_le_bytes());
            hb.push(h.pos.y());
            hb.extend_from_slice(&h.pos.v().to_le_bytes());
            hb.push(h.stage);
            hb.extend_from_slice(&h.strain.to_le_bytes());
            hb.extend_from_slice(&h.rooting.to_le_bytes());
            hb.push(h.graft.map(|b| b as u8 + 1).unwrap_or(0));
            hb.extend_from_slice(&h.drift.to_le_bytes());
            hb.extend_from_slice(&h.regrow.to_le_bytes());
        }
        let path = self.save_dir.join("hearts");
        report.record(
            "hearts",
            path.clone(),
            super::persistence::replace_or_remove(
                &path,
                (!self.hearts.is_empty()).then_some(hb.as_slice()),
            ),
        );
        // Planetary seeded-chunk marks: magic followed by face/u/v records.
        let mut buf = Vec::with_capacity(4 + self.mob_seeded.len() * 5);
        buf.extend_from_slice(b"WFA1");
        for pos in &self.mob_seeded {
            buf.push(pos.face() as u8);
            buf.extend_from_slice(&pos.u().to_le_bytes());
            buf.extend_from_slice(&pos.v().to_le_bytes());
        }
        let path = self.save_dir.join("aseeded");
        report.record(
            "animal seed marks",
            path.clone(),
            super::persistence::atomic_replace(&path, &buf),
        );
        // Player-touched chunk marks: same planetary shape.
        let mut pt = Vec::with_capacity(4 + self.player_touched.len() * 5);
        pt.extend_from_slice(b"WFP1");
        for pos in &self.player_touched {
            pt.push(pos.face() as u8);
            pt.extend_from_slice(&pos.u().to_le_bytes());
            pt.extend_from_slice(&pos.v().to_le_bytes());
        }
        let path = self.save_dir.join("ptouched");
        report.record(
            "player-touched marks",
            path.clone(),
            super::persistence::atomic_replace(&path, &pt),
        );
        let mut structures = Vec::with_capacity(4 + self.structure_chunks.len() * 5);
        structures.extend_from_slice(b"WFS1");
        for pos in &self.structure_chunks {
            structures.push(pos.face() as u8);
            structures.extend_from_slice(&pos.u().to_le_bytes());
            structures.extend_from_slice(&pos.v().to_le_bytes());
        }
        let path = self.save_dir.join("structured");
        report.record(
            "structure chunk marks",
            path.clone(),
            super::persistence::atomic_replace(&path, &structures),
        );
        // Flag-gated feature positions (spec 2.5): each record is the 6-byte
        // block position (face/u/y/v) followed by the 2-byte gate index.
        let mut gates = Vec::with_capacity(4 + self.gated.len() * 8);
        gates.extend_from_slice(b"WFG1");
        for (pos, gate) in &self.gated {
            gates.push(pos.face() as u8);
            gates.extend_from_slice(&pos.u().to_le_bytes());
            gates.push(pos.y());
            gates.extend_from_slice(&pos.v().to_le_bytes());
            gates.extend_from_slice(&(*gate as u16).to_le_bytes());
        }
        let path = self.save_dir.join("gated");
        report.record(
            "flag-gated feature marks",
            path.clone(),
            super::persistence::atomic_replace(&path, &gates),
        );
        // Nest spawn-gates (capability E9): same 8-byte record shape as the
        // gates — 6-byte block position + 2-byte nest index.
        let mut nests = Vec::with_capacity(4 + self.nests.len() * 8);
        nests.extend_from_slice(b"WFN1");
        for (pos, nest) in &self.nests {
            nests.push(pos.face() as u8);
            nests.extend_from_slice(&pos.u().to_le_bytes());
            nests.push(pos.y());
            nests.extend_from_slice(&pos.v().to_le_bytes());
            nests.extend_from_slice(&(*nest as u16).to_le_bytes());
        }
        let path = self.save_dir.join("nests");
        report.record(
            "nest spawn-gate marks",
            path.clone(),
            super::persistence::atomic_replace(&path, &nests),
        );
        // Settlement hidden cells (spec 3.4): each record is the 6-byte block
        // position (face/u/y/v), the 2-byte settlement index, and the 1-byte
        // tier.
        let mut settlements = Vec::with_capacity(4 + self.hidden.len() * 9);
        settlements.extend_from_slice(b"WFST1");
        for (pos, key) in &self.hidden {
            settlements.push(pos.face() as u8);
            settlements.extend_from_slice(&pos.u().to_le_bytes());
            settlements.push(pos.y());
            settlements.extend_from_slice(&pos.v().to_le_bytes());
            settlements.extend_from_slice(&(key.settlement as u16).to_le_bytes());
            settlements.push(key.tier as u8);
        }
        let path = self.save_dir.join("settlements");
        report.record(
            "settlement hidden marks",
            path.clone(),
            super::persistence::atomic_replace(&path, &settlements),
        );
        report.failures
    }

    pub(super) fn load_mobs(&mut self) {
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
        self.long_winter = fs::read(self.save_dir.join("longwinter"))
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
                    self.mob_seeded.insert(pos);
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

    pub(crate) fn chunk_loader(&self) -> ChunkLoader {
        ChunkLoader {
            store: self.region_store.clone(),
            palette: self.palette.snapshot(&self.reg),
            reg: Arc::clone(&self.reg),
        }
    }

    pub(super) fn try_load_chunk(&self, pos: ChunkPos) -> std::io::Result<ChunkRead> {
        self.chunk_loader().load(pos)
    }
}

impl World {
    /// Planetary WFC8 block/metadata/water-salt/soil-salt RLE and HU reservoir
    /// residuals for disk. Derived light is deliberately omitted from saves.
    #[cfg(test)]
    pub fn chunk_rle(&self, pos: ChunkPos) -> Option<Vec<u8>> {
        let chunk = self.chunks.get(&pos)?;
        Some(encode_chunk(chunk))
    }




    pub(super) fn save_chunk(&self, pos: ChunkPos) -> std::io::Result<()> {
        // Deep chunks (capability E10) never persist: dungeon runs are
        // ephemeral by construction, so every entry regenerates fresh.
        if pos.face().is_deep() {
            return Ok(());
        }
        #[cfg(test)]
        if self.save_fail_chunks.contains(&pos) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "injected chunk save failure",
            ));
        }
        let chunk = self.chunks.get(&pos).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("chunk {pos:?} is not resident"),
            )
        })?;
        let palette = self.palette.publish(&self.reg)?;
        let buf = encoder::encode_saved_chunk(chunk, &palette)?;
        self.region_store.write(pos, &buf)
    }

    /// Persist a single departing chunk (unload path): only its own
    /// file, only if edited. With the autosave timer gone this is how
    /// most of the world reaches disk: a chunk is written once, as it
    /// leaves the view, instead of the whole world on a clock.
    pub fn save_chunk_if_modified(&self, pos: ChunkPos) -> std::io::Result<bool> {
        if self.chunks.get(&pos).is_some_and(|chunk| chunk.modified) {
            self.save_chunk(pos)?;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn save_modified(&mut self) -> SaveReport {
        let mut report = SaveReport::default();
        report.record(
            "save directory",
            self.save_dir.clone(),
            fs::create_dir_all(&self.save_dir),
        );
        let meta_path = self.save_dir.join("world.toml");
        report.record(
            "world metadata",
            meta_path,
            write_world_meta_full(
                &self.save_dir,
                self.seed,
                &self.mode,
                self.ire,
                self.day,
                &self.camera,
            ),
        );
        if let (Some(atlas), Some(weather)) = (&self.planet_atlas, &self.planetary_weather) {
            report.record(
                "planetary weather",
                crate::planet_atlas::PlanetAtlas::planet_dir(&self.save_dir).join("dynamic.wfd"),
                atlas
                    .save_dynamic_snapshot(&self.save_dir, &weather.cells, &weather.water)
                    .map_err(std::io::Error::other),
            );
        }
        if let Some(ledger) = &self.material_ledger {
            report.record(
                "finite-material ledger",
                self.save_dir.join("materials.wfm"),
                ledger.save(),
            );
        }
        if let Some(ledger) = &self.arcane_ledger {
            report.record(
                "finite-Current ledger",
                self.save_dir.join("arcane.wfc"),
                ledger.save().map_err(std::io::Error::other),
            );
        }
        if let Some(geography) = &mut self.arcane_geography {
            report.record(
                "planetary arcane geography",
                crate::planet_atlas::PlanetAtlas::planet_dir(&self.save_dir)
                    .join("arcane-geography.wad"),
                geography
                    .save_dynamic(&self.save_dir)
                    .map_err(std::io::Error::other),
            );
        }
        // One owner publishes extensions for both full saves and direct chunk
        // writes. Stored names are never renumbered when content changes.
        let palette_ready = report.record(
            "block palette",
            self.save_dir.join("palette"),
            self.palette.publish(&self.reg).map(|_| ()),
        );
        let path = self.entities_path();
        report.record("block entities", path, self.save_entities());
        let path = self.save_dir.join("discovery.toml");
        let discovery_result = self
            .discovery_state
            .as_mut()
            .map_or(Ok(()), |state| state.save().map_err(std::io::Error::other));
        report.record("discovery state", path, discovery_result);
        let path = self.save_dir.join(crate::implements::IMPLEMENTS_FILE);
        let implements_result = self
            .implements_state
            .as_ref()
            .map_or(Ok(()), |state| state.save().map_err(std::io::Error::other));
        report.record("implement state", path, implements_result);
        let path = self.save_dir.join(crate::workings::WORKINGS_FILE);
        let workings_result = self
            .workings_state
            .as_ref()
            .map_or(Ok(()), |state| state.save().map_err(std::io::Error::other));
        report.record("working transaction state", path, workings_result);
        let path = self.save_dir.join(crate::alchemy::ALCHEMY_FILE);
        let alchemy_result = self
            .alchemy_state
            .as_ref()
            .map_or(Ok(()), |state| state.save().map_err(std::io::Error::other));
        report.record("alchemy state", path, alchemy_result);
        let path = self.save_dir.join(crate::workings::WATER_CARRIERS_FILE);
        let carrier_result = self
            .water_carriers
            .as_ref()
            .map_or(Ok(()), |state| state.save().map_err(std::io::Error::other));
        report.record("detailed water carriers", path, carrier_result);
        report.record(
            "host-owned loose items",
            self.save_dir.join("loose-items.toml"),
            self.save_loose_items(),
        );
        report.extend(self.save_mobs());
        let path = self.save_dir.join("stamps");
        report.record("random-tick stamps", path, self.save_stamps());
        report.record(
            "template library",
            self.save_dir.join("templates.toml"),
            self.save_templates(),
        );
        report.record(
            "local structures",
            self.save_dir.join("local_structures.toml"),
            self.save_local_structures(),
        );
        let dirty: Vec<ChunkPos> = self
            .chunks
            .iter()
            .filter(|(_, chunk)| chunk.modified)
            .map(|(pos, _)| *pos)
            .collect();
        for pos in dirty {
            // New stored IDs cannot reach disk before their names. Existing
            // IDs keep their meaning even when unloaded chunks are untouched.
            if !palette_ready {
                continue;
            }
            // Clear only on a write that landed: a chunk whose file could
            // not be written stays queued for the next save.
            match self.save_chunk(pos) {
                Ok(()) => {
                    report.chunks_saved += 1;
                    if let Some(chunk) = self.chunks.get_mut(&pos) {
                        chunk.modified = false;
                    }
                }
                Err(error) => report.failures.push(SaveFailure::new(
                    format!("chunk {pos:?}"),
                    super::region::region_path(&self.save_dir, pos),
                    error,
                )),
            }
        }
        report
    }

    /// Remap all in-memory chunks from an old registry to the current one
    /// (used by hot reload). Unknown blocks become the placeholder.
    pub fn remap_from(&mut self, old: &Registry) {
        self.chunks.remap_from(old, &self.reg);
        // Re-resolve gated positions (spec 2.5) against the new registry's
        // gate list by their sealed block; a gate whose def was removed (or
        // whose sealed block changed) stops gating rather than softlocking
        // the world with a permanent unbreakable wall.
        let mut gates: HashMap<_, _> = HashMap::with_capacity(self.gated.len());
        for pos in std::mem::take(&mut self.gated).into_keys() {
            if let Some(gate) = self.reg.gate_for_block(self.get_block_at(pos)) {
                gates.insert(pos, gate);
            }
        }
        self.gated = gates;
        // Re-resolve hidden cells (spec 3.4): a record whose settlement def
        // was removed, or whose tier is no longer declared, is dropped and
        // treated as revealed (the placed block simply becomes solid/visible).
        self.hidden.retain(|_, key| {
            self.reg
                .settlements
                .get(key.settlement)
                .is_some_and(|def| def.tiers.iter().any(|t| t.tier == key.tier))
        });
    }

    // ---------------- lighting ----------------
}
