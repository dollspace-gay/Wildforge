//! Mob/chunk persistence, planetary chunk streaming, saves, and registry remapping.

use super::*;

impl World {
    pub(super) fn mobs_path(&self) -> PathBuf {
        self.save_dir.join("animals.toml")
    }

    pub(super) fn save_mobs(&self) -> Vec<SaveFailure> {
        use std::fmt::Write as _;
        let mut report = SaveReport::default();
        let mut out = String::from("version = 2\n");
        for m in &self.mobs {
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
                self.mobs.push(m);
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
    }

    pub(crate) fn chunk_loader(&self) -> ChunkLoader {
        ChunkLoader {
            save_dir: self.save_dir.clone(),
            load_remap: self.load_remap.clone(),
            reg: Arc::clone(&self.reg),
            palette_stale: self.palette_stale,
        }
    }

    pub(super) fn try_load_chunk(&self, pos: ChunkPos) -> Option<Chunk> {
        self.chunk_loader().load(pos)
    }
}

/// Immutable save decoder that can be cloned into cold-terrain workers.
/// Disk I/O and WFC decoding therefore never need the simulation-owned World.
#[derive(Clone)]
pub(crate) struct ChunkLoader {
    save_dir: PathBuf,
    load_remap: Vec<crate::registry::BlockId>,
    reg: Arc<Registry>,
    palette_stale: bool,
}

impl ChunkLoader {
    pub(crate) fn load(&self, pos: ChunkPos) -> Option<Chunk> {
        let data = super::region::read_chunk(&self.save_dir, pos)?;
        let mut chunk = Chunk::new();
        let version8 = data.starts_with(b"WFC8");
        let version7 = data.starts_with(b"WFC7");
        if !version8 && !version7 && !data.starts_with(b"WFC6") {
            return None;
        }
        let out = chunk.raw_mut();
        let mut o = 0;
        // (count u16, id u16) pairs, remapped through the palette.
        let mut i = 4;
        while i + 4 <= data.len() && o < out.len() {
            let count = u16::from_le_bytes([data[i], data[i + 1]]) as usize;
            let stored = u16::from_le_bytes([data[i + 2], data[i + 3]]) as usize;
            let id = self
                .load_remap
                .get(stored)
                .copied()
                .unwrap_or(self.reg.unknown_block);
            let end = (o + count).min(out.len());
            out[o..end].fill(id.0);
            o = end;
            i += 4;
        }
        if o != out.len() {
            return None; // corrupt; regenerate
        }
        if out.iter().all(|&block| block == self.reg.unknown_block.0) {
            // The old palette-less-save bug decoded even air as the
            // placeholder and could then persist a solid 16x16x256 magenta
            // tower. No legitimate chunk can contain only unknown blocks;
            // its original contents are already unrecoverable, so regenerate
            // terrain instead of keeping the poisoned chunk forever.
            eprintln!(
                "world: regenerating all-placeholder chunk {},{}",
                pos.u(),
                pos.v()
            );
            return None;
        }
        let meta = chunk.meta_raw_mut();
        let mut offset = 0;
        while i + 3 <= data.len() && offset < meta.len() {
            let count = u16::from_le_bytes([data[i], data[i + 1]]) as usize;
            let value = data[i + 2];
            let end = (offset + count).min(meta.len());
            meta[offset..end].fill(value);
            offset = end;
            i += 3;
        }
        if offset != meta.len() {
            return None;
        }
        if version8 || version7 {
            let salt = chunk.water_salt_raw_mut();
            let mut offset = 0;
            while i + 4 <= data.len() && offset < salt.len() {
                let count = u16::from_le_bytes([data[i], data[i + 1]]) as usize;
                let value = u16::from_le_bytes([data[i + 2], data[i + 3]]);
                let end = (offset + count).min(salt.len());
                salt[offset..end].fill(value);
                offset = end;
                i += 4;
            }
            if offset != salt.len() {
                return None;
            }
        } else {
            for x in 0..CHUNK_X {
                for z in 0..CHUNK_Z {
                    for y in 0..CHUNK_Y {
                        let volume = self.reg.water_volume(chunk.get(x, y, z)).unwrap_or(0);
                        let salt = u16::from(volume)
                            .saturating_mul(32)
                            .saturating_mul(u16::from(chunk.meta(x, y, z)));
                        chunk.set_water_salt(x, y, z, salt);
                    }
                }
            }
        }
        if version8 {
            let salinity = chunk.soil_salinity_raw_mut();
            let mut offset = 0;
            while i + 3 <= data.len() && offset < salinity.len() {
                let count = u16::from_le_bytes([data[i], data[i + 1]]) as usize;
                let value = data[i + 2];
                let end = (offset + count).min(salinity.len());
                salinity[offset..end].fill(value);
                offset = end;
                i += 3;
            }
            if offset != salinity.len() {
                return None;
            }
        }
        if i + 2 > data.len() {
            return None;
        }
        let records = u16::from_le_bytes([data[i], data[i + 1]]) as usize;
        i += 2;
        let record_bytes = if version8 || version7 { 32 } else { 24 };
        if data.len().saturating_sub(i) != records.saturating_mul(record_bytes) {
            return None;
        }
        let mut hydrology = Vec::with_capacity(records);
        for _ in 0..records {
            let reservoir = u64::from_le_bytes(data[i..i + 8].try_into().ok()?);
            let baseline_units = u64::from_le_bytes(data[i + 8..i + 16].try_into().ok()?);
            let residual_units = i64::from_le_bytes(data[i + 16..i + 24].try_into().ok()?);
            let salt_mass = if version8 || version7 {
                u64::from_le_bytes(data[i + 24..i + 32].try_into().ok()?)
            } else {
                0
            };
            hydrology.push(crate::chunk::HydrologyVolumeRecord {
                reservoir,
                baseline_hu: if version8 || version7 {
                    baseline_units
                } else {
                    baseline_units.saturating_mul(32)
                },
                residual_hu: if version8 || version7 {
                    residual_units
                } else {
                    residual_units.saturating_mul(32)
                },
                salt_mass,
            });
            i += record_bytes;
        }
        chunk.set_hydrology_volumes(hydrology);
        chunk.dirty = true;
        // Planes the file turned out uniform in (no block state anywhere,
        // most often) shrink back to a single value.
        chunk.compact();
        // A chunk that came off disk already matches its file, so it only
        // needs saving again once something edits it. The exception is a
        // registry change: the ids in that file are about to be reinterpreted
        // under a new palette, so it has to be rewritten in current ids.
        // Marking every loaded chunk modified unconditionally meant a 20s
        // autosave rewrote the entire explored world, forever.
        chunk.modified = self.palette_stale;
        Some(chunk)
    }
}

impl World {
    /// Planetary WFC8 block/metadata/water-salt/soil-salt RLE and HU reservoir
    /// residuals for disk. Derived light is deliberately omitted from saves.
    pub fn chunk_rle(&self, pos: ChunkPos) -> Option<Vec<u8>> {
        let chunk = self.chunks.get(&pos)?;
        Some(encode_chunk(chunk))
    }

    /// Insert a network-streamed chunk, remapping host block ids to local
    /// ones. Current hosts include their settled derived light; older payloads
    /// remain compatible and are relit locally.
    pub fn insert_remote_chunk(&mut self, pos: ChunkPos, rle: &[u8], remap: &[BlockId]) {
        self.insert_remote_chunks([(pos, rle)], remap);
    }

    /// Insert a group received in one network poll. Current WFC9 payloads carry
    /// the host's settled light field. Legacy WFC6-WFC8 chunks settle their
    /// shared borders through one fallback lighting cascade.
    pub fn insert_remote_chunks<'a>(
        &mut self,
        chunks: impl IntoIterator<Item = (ChunkPos, &'a [u8])>,
        remap: &[BlockId],
    ) {
        let mut needs_relight = Vec::new();
        for (pos, rle) in chunks {
            if self.insert_remote_chunk_unlit(pos, rle, remap) == Some(false) {
                needs_relight.push(pos);
            }
        }
        self.relight_chunks_and_cascade(needs_relight);
    }

    /// `Some(true)` means the payload supplied settled light, `Some(false)`
    /// requests a legacy relight, and `None` rejects an invalid payload.
    fn insert_remote_chunk_unlit(
        &mut self,
        pos: ChunkPos,
        rle: &[u8],
        remap: &[BlockId],
    ) -> Option<bool> {
        let version9 = rle.starts_with(b"WFC9");
        let version8 = rle.starts_with(b"WFC8");
        let version7 = rle.starts_with(b"WFC7");
        if !version9 && !version8 && !version7 && !rle.starts_with(b"WFC6") {
            return None;
        }
        let mut chunk = Chunk::new();
        let out = chunk.raw_mut();
        let mut o = 0;
        let mut i = 4;
        while i + 4 <= rle.len() && o < out.len() {
            let count = u16::from_le_bytes([rle[i], rle[i + 1]]) as usize;
            let stored = u16::from_le_bytes([rle[i + 2], rle[i + 3]]) as usize;
            let id = remap.get(stored).copied().unwrap_or(self.reg.unknown_block);
            let end = (o + count).min(out.len());
            out[o..end].fill(id.0);
            o = end;
            i += 4;
        }
        if o != out.len() {
            return None;
        }
        let meta = chunk.meta_raw_mut();
        let mut offset = 0;
        while i + 3 <= rle.len() && offset < meta.len() {
            let count = u16::from_le_bytes([rle[i], rle[i + 1]]) as usize;
            let value = rle[i + 2];
            let end = (offset + count).min(meta.len());
            meta[offset..end].fill(value);
            offset = end;
            i += 3;
        }
        if offset != meta.len() {
            return None;
        }
        if version9 || version8 || version7 {
            let salt = chunk.water_salt_raw_mut();
            let mut offset = 0;
            while i + 4 <= rle.len() && offset < salt.len() {
                let count = u16::from_le_bytes([rle[i], rle[i + 1]]) as usize;
                let value = u16::from_le_bytes([rle[i + 2], rle[i + 3]]);
                let end = (offset + count).min(salt.len());
                salt[offset..end].fill(value);
                offset = end;
                i += 4;
            }
            if offset != salt.len() {
                return None;
            }
        }
        if version9 || version8 {
            let salinity = chunk.soil_salinity_raw_mut();
            let mut offset = 0;
            while i + 3 <= rle.len() && offset < salinity.len() {
                let count = u16::from_le_bytes([rle[i], rle[i + 1]]) as usize;
                let value = rle[i + 2];
                let end = (offset + count).min(salinity.len());
                salinity[offset..end].fill(value);
                offset = end;
                i += 3;
            }
            if offset != salinity.len() {
                return None;
            }
        }
        if version9 {
            let light = chunk.light_block_raw_mut();
            let mut offset = 0;
            while i + 5 <= rle.len() && offset < light.len() {
                let count = u16::from_le_bytes([rle[i], rle[i + 1]]) as usize;
                let value = [rle[i + 2], rle[i + 3], rle[i + 4]];
                let end = (offset + count).min(light.len());
                light[offset..end].fill(value);
                offset = end;
                i += 5;
            }
            if offset != light.len() {
                return None;
            }
            let sky = chunk.light_sky_raw_mut();
            let mut offset = 0;
            while i + 3 <= rle.len() && offset < sky.len() {
                let count = u16::from_le_bytes([rle[i], rle[i + 1]]) as usize;
                let value = rle[i + 2];
                let end = (offset + count).min(sky.len());
                sky[offset..end].fill(value);
                offset = end;
                i += 3;
            }
            if offset != sky.len() {
                return None;
            }
        }
        if i + 2 > rle.len() {
            return None;
        }
        let records = u16::from_le_bytes([rle[i], rle[i + 1]]) as usize;
        i += 2;
        let detailed_water = version9 || version8 || version7;
        let record_bytes = if detailed_water { 32 } else { 24 };
        if rle.len().saturating_sub(i) != records.saturating_mul(record_bytes) {
            return None;
        }
        let mut hydrology = Vec::with_capacity(records);
        for _ in 0..records {
            let Ok(reservoir) = rle[i..i + 8].try_into().map(u64::from_le_bytes) else {
                return None;
            };
            let Ok(baseline_units) = rle[i + 8..i + 16].try_into().map(u64::from_le_bytes) else {
                return None;
            };
            let Ok(residual_units) = rle[i + 16..i + 24].try_into().map(i64::from_le_bytes) else {
                return None;
            };
            let salt_mass = if detailed_water {
                let Ok(value) = rle[i + 24..i + 32].try_into().map(u64::from_le_bytes) else {
                    return None;
                };
                value
            } else {
                0
            };
            hydrology.push(crate::chunk::HydrologyVolumeRecord {
                reservoir,
                baseline_hu: if detailed_water {
                    baseline_units
                } else {
                    baseline_units.saturating_mul(32)
                },
                residual_hu: if detailed_water {
                    residual_units
                } else {
                    residual_units.saturating_mul(32)
                },
                salt_mass,
            });
            i += record_bytes;
        }
        chunk.set_hydrology_volumes(hydrology);
        chunk.dirty = true;
        chunk.compact();
        self.chunks.insert(pos, chunk);
        // Neighbors need remeshing for the new border faces.
        for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let n = pos.offset(dx, dz);
            if let Some(c) = self.chunks.get_mut(&n) {
                c.dirty = true;
            }
        }
        Some(version9)
    }

    pub(super) fn save_chunk(&self, pos: ChunkPos) -> std::io::Result<()> {
        #[cfg(test)]
        if self.save_fail_chunks.contains(&pos) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "injected chunk save failure",
            ));
        }
        let buf = self.chunk_rle(pos).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("chunk {pos:?} is not resident"),
            )
        })?;
        super::region::write_chunk(&self.save_dir, pos, &buf)
    }

    /// Persist a single departing chunk (unload path): only its own
    /// file, only if edited. With the autosave timer gone this is how
    /// most of the world reaches disk: a chunk is written once, as it
    /// leaves the view, instead of the whole world on a clock.
    pub fn save_chunk_if_modified(&self, pos: ChunkPos) -> std::io::Result<bool> {
        if self.remote {
            return Ok(false);
        }
        if self.chunks.get(&pos).is_some_and(|chunk| chunk.modified) {
            self.save_chunk(pos)?;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn save_modified(&mut self) -> SaveReport {
        let mut report = SaveReport::default();
        if self.remote {
            return report; // the host owns the world
        }
        report.record(
            "save directory",
            self.save_dir.clone(),
            fs::create_dir_all(&self.save_dir),
        );
        let meta_path = self.save_dir.join("world.toml");
        report.record(
            "world metadata",
            meta_path,
            write_world_meta_full(&self.save_dir, self.seed, &self.mode, self.ire, self.day),
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
        // Only when it would actually differ. The palette describes the
        // registry, not the world, so rewriting it on a timer was 4 KB
        // of churn every twenty seconds saying the same thing. It has
        // to land before the chunks below, which are written in the ids
        // it names.
        let palette_ready = if self.palette_stale {
            let path = self.save_dir.join("palette");
            let ready = report.record("block palette", path, self.write_palette());
            if ready {
                self.palette_stale = false;
                self.load_remap = self.read_palette_remap();
            }
            ready
        } else {
            true
        };
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
        report.extend(self.save_mobs());
        let path = self.save_dir.join("stamps");
        report.record("random-tick stamps", path, self.save_stamps());
        let dirty: Vec<ChunkPos> = self
            .chunks
            .iter()
            .filter(|(_, chunk)| chunk.modified)
            .map(|(pos, _)| *pos)
            .collect();
        for pos in dirty {
            // Chunks use runtime numeric ids. If the palette naming those ids
            // did not land, writing them would make the next load reinterpret
            // otherwise healthy terrain under the stale palette.
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
        let map: Vec<BlockId> = old
            .blocks
            .iter()
            .map(|b| self.reg.block_id(&b.name).unwrap_or(self.reg.unknown_block))
            .collect();
        for chunk in self.chunks.values_mut() {
            for cell in chunk.raw_mut() {
                *cell = map
                    .get(*cell as usize)
                    .copied()
                    .unwrap_or(self.reg.unknown_block)
                    .0;
            }
            // A remap can collapse many ids onto one placeholder.
            chunk.compact();
            chunk.dirty = true;
        }
        self.load_remap = self.read_palette_remap();
    }

    // ---------------- lighting ----------------
}

pub(crate) fn encode_chunk(chunk: &Chunk) -> Vec<u8> {
    encode_chunk_state(chunk, false)
}

/// Live network form. Unlike the disk codec, WFC9 includes settled block and
/// sky light so every guest does not recompute the host's identical derived
/// field while a view is streaming in.
pub(crate) fn encode_stream_chunk(chunk: &Chunk) -> Vec<u8> {
    encode_chunk_state(chunk, true)
}

fn encode_chunk_state(chunk: &Chunk, include_light: bool) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    buf.extend_from_slice(if include_light { b"WFC9" } else { b"WFC8" });
    // Runs come straight off the plane, so a uniform plane is one step rather
    // than a scan of every cell. Long runs are split for the u16 wire field.
    for (value, mut run) in chunk.block_runs() {
        while run > 0 {
            let take = run.min(u16::MAX as usize);
            buf.extend_from_slice(&(take as u16).to_le_bytes());
            buf.extend_from_slice(&value.to_le_bytes());
            run -= take;
        }
    }
    for (value, mut run) in chunk.meta_runs() {
        while run > 0 {
            let take = run.min(u16::MAX as usize);
            buf.extend_from_slice(&(take as u16).to_le_bytes());
            buf.push(value);
            run -= take;
        }
    }
    for (value, mut run) in chunk.water_salt_runs() {
        while run > 0 {
            let take = run.min(u16::MAX as usize);
            buf.extend_from_slice(&(take as u16).to_le_bytes());
            buf.extend_from_slice(&value.to_le_bytes());
            run -= take;
        }
    }
    for (value, mut run) in chunk.soil_salinity_runs() {
        while run > 0 {
            let take = run.min(u16::MAX as usize);
            buf.extend_from_slice(&(take as u16).to_le_bytes());
            buf.push(value);
            run -= take;
        }
    }
    if include_light {
        for (value, mut run) in chunk.light_block_runs() {
            while run > 0 {
                let take = run.min(u16::MAX as usize);
                buf.extend_from_slice(&(take as u16).to_le_bytes());
                buf.extend_from_slice(&value);
                run -= take;
            }
        }
        for (value, mut run) in chunk.light_sky_runs() {
            while run > 0 {
                let take = run.min(u16::MAX as usize);
                buf.extend_from_slice(&(take as u16).to_le_bytes());
                buf.push(value);
                run -= take;
            }
        }
    }
    let records = chunk.hydrology_volumes();
    buf.extend_from_slice(&(records.len().min(u16::MAX as usize) as u16).to_le_bytes());
    for record in records.iter().take(u16::MAX as usize) {
        buf.extend_from_slice(&record.reservoir.to_le_bytes());
        buf.extend_from_slice(&record.baseline_hu.to_le_bytes());
        buf.extend_from_slice(&record.residual_hu.to_le_bytes());
        buf.extend_from_slice(&record.salt_mass.to_le_bytes());
    }
    buf
}
