//! Mob/chunk persistence, WFC4 streaming, saves, and registry remapping.

use super::*;

impl World {
    pub(super) fn mobs_path(&self) -> PathBuf {
        self.save_dir.join("animals.toml")
    }

    pub(super) fn save_mobs(&self) -> Vec<SaveFailure> {
        use std::fmt::Write as _;
        let mut report = SaveReport::default();
        let mut out = String::new();
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
                "[[mob]]\nspecies = \"{}\"\npos = [{:?}, {:?}, {:?}]\nyaw = {:?}\nhealth = {:?}\nfed = {}\ngrowth = {:?}\ntamed = {}\ntame_fed = {}\ntame_need = {}\nsaddled = {}\nbelly = {:?}",
                def.name,
                m.pos.x,
                m.pos.y,
                m.pos.z,
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
                            "[[mob.pack]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}",
                            self.reg.item(st.item).name,
                            st.count,
                            st.durability
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
        // The regional ledger: compact (cell x, cell z, standing).
        let mut rb = Vec::with_capacity(self.regional_ire.len() * 12);
        for (&(x, z), &v) in &self.regional_ire {
            rb.extend_from_slice(&x.to_le_bytes());
            rb.extend_from_slice(&z.to_le_bytes());
            rb.extend_from_slice(&v.to_le_bytes());
        }
        let path = self.save_dir.join("rire");
        report.record(
            "regional ire",
            path.clone(),
            super::persistence::replace_or_remove(&path, (!rb.is_empty()).then_some(rb.as_slice())),
        );
        // The bloom ledger, same shape as rire.
        let mut bb = Vec::with_capacity(self.bloom.len() * 12);
        for (&(x, z), &v) in &self.bloom {
            bb.extend_from_slice(&x.to_le_bytes());
            bb.extend_from_slice(&z.to_le_bytes());
            bb.extend_from_slice(&v.to_le_bytes());
        }
        let path = self.save_dir.join("bloom");
        report.record(
            "bloom ledger",
            path.clone(),
            super::persistence::replace_or_remove(&path, (!bb.is_empty()).then_some(bb.as_slice())),
        );
        let path = self.save_dir.join("longwinter");
        report.record(
            "long winter",
            path.clone(),
            super::persistence::atomic_replace(&path, if self.long_winter { b"1" } else { b"0" }),
        );
        // The ground's spent willingness to bloom.
        let mut sb = Vec::with_capacity(self.bloom_spent.len() * 12);
        for (&(x, z), &v) in &self.bloom_spent {
            sb.extend_from_slice(&x.to_le_bytes());
            sb.extend_from_slice(&z.to_le_bytes());
            sb.extend_from_slice(&v.to_le_bytes());
        }
        let path = self.save_dir.join("bspent");
        report.record(
            "bloom exhaustion",
            path.clone(),
            super::persistence::replace_or_remove(&path, (!sb.is_empty()).then_some(sb.as_slice())),
        );
        // The hearts: (province x, z, site x, y, z, stage, strain,
        // rooting, graft, drift, regrow). The magic distinguishes this
        // from the older headerless 34-byte layout, which is otherwise
        // ambiguous — a save with 19 hearts is 646 bytes and divides
        // evenly by both record sizes.
        let mut hb = Vec::with_capacity(4 + self.hearts.len() * 38);
        hb.extend_from_slice(b"WFH2");
        for (&(kx, kz), h) in &self.hearts {
            hb.extend_from_slice(&kx.to_le_bytes());
            hb.extend_from_slice(&kz.to_le_bytes());
            hb.extend_from_slice(&h.pos.0.to_le_bytes());
            hb.extend_from_slice(&h.pos.1.to_le_bytes());
            hb.extend_from_slice(&h.pos.2.to_le_bytes());
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
        // Seeded-chunk marks: compact binary pairs.
        let mut buf = Vec::with_capacity(self.mob_seeded.len() * 8);
        for (x, z) in &self.mob_seeded {
            buf.extend_from_slice(&x.to_le_bytes());
            buf.extend_from_slice(&z.to_le_bytes());
        }
        let path = self.save_dir.join("aseeded");
        report.record(
            "animal seed marks",
            path.clone(),
            super::persistence::atomic_replace(&path, &buf),
        );
        // Player-touched chunk marks: same shape.
        let mut pt = Vec::with_capacity(self.player_touched.len() * 8);
        for (x, z) in &self.player_touched {
            pt.extend_from_slice(&x.to_le_bytes());
            pt.extend_from_slice(&z.to_le_bytes());
        }
        let path = self.save_dir.join("ptouched");
        report.record(
            "player-touched marks",
            path.clone(),
            super::persistence::atomic_replace(&path, &pt),
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
        }
        #[derive(Deserialize)]
        struct MobT {
            species: String,
            pos: [f32; 3],
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
            #[serde(default)]
            mob: Vec<MobT>,
        }
        if let Ok(text) = fs::read_to_string(self.mobs_path())
            && let Ok(f) = toml::from_str::<FileT>(&text)
        {
            for t in f.mob {
                // Unknown species (mod removed) skip cleanly.
                let Some(si) = self.reg.animal_id(&t.species) else {
                    continue;
                };
                let mut m = Mob::new(si, glam::Vec3::new(t.pos[0], t.pos[1], t.pos[2]), t.yaw);
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
                            });
                        }
                    }
                    m.cargo = Some(cargo);
                }
                self.mobs.push(m);
            }
        }
        if let Ok(data) = fs::read(self.save_dir.join("rire")) {
            for p in data.chunks_exact(12) {
                let x = i32::from_le_bytes([p[0], p[1], p[2], p[3]]);
                let z = i32::from_le_bytes([p[4], p[5], p[6], p[7]]);
                let v = f32::from_le_bytes([p[8], p[9], p[10], p[11]]);
                self.regional_ire.insert((x, z), v.clamp(-20.0, 20.0));
            }
        }
        if let Ok(data) = fs::read(self.save_dir.join("bloom")) {
            for p in data.chunks_exact(12) {
                let x = i32::from_le_bytes([p[0], p[1], p[2], p[3]]);
                let z = i32::from_le_bytes([p[4], p[5], p[6], p[7]]);
                let v = f32::from_le_bytes([p[8], p[9], p[10], p[11]]);
                self.bloom.insert((x, z), v.clamp(0.0, 9.0));
            }
        }
        self.long_winter = fs::read(self.save_dir.join("longwinter"))
            .map(|d| d.first() == Some(&b'1'))
            .unwrap_or(false);
        if let Ok(data) = fs::read(self.save_dir.join("bspent")) {
            for p in data.chunks_exact(12) {
                let x = i32::from_le_bytes([p[0], p[1], p[2], p[3]]);
                let z = i32::from_le_bytes([p[4], p[5], p[6], p[7]]);
                let v = f32::from_le_bytes([p[8], p[9], p[10], p[11]]);
                self.bloom_spent.insert((x, z), v.max(0.0));
            }
        }
        if let Ok(data) = fs::read(self.save_dir.join("hearts")) {
            // WFH2 carries the cutting timer; a headerless file is the
            // older layout and its hearts are simply ready to give.
            let versioned = data.starts_with(b"WFH2");
            let (body, size) = if versioned {
                (&data[4..], 38)
            } else {
                (&data[..], 34)
            };
            for p in body.chunks_exact(size) {
                let i32_at = |o: usize| i32::from_le_bytes([p[o], p[o + 1], p[o + 2], p[o + 3]]);
                let f32_at = |o: usize| f32::from_le_bytes([p[o], p[o + 1], p[o + 2], p[o + 3]]);
                self.hearts.insert(
                    (i32_at(0), i32_at(4)),
                    Heart {
                        pos: (i32_at(8), i32_at(12), i32_at(16)),
                        stage: p[20],
                        strain: f32_at(21),
                        rooting: f32_at(25),
                        graft: crate::worldgen::Biome::from_index(p[29]),
                        drift: f32_at(30),
                        regrow: if versioned { f32_at(34) } else { 0.0 },
                    },
                );
            }
        }
        if let Ok(data) = fs::read(self.save_dir.join("aseeded")) {
            for p in data.chunks_exact(8) {
                let x = i32::from_le_bytes([p[0], p[1], p[2], p[3]]);
                let z = i32::from_le_bytes([p[4], p[5], p[6], p[7]]);
                self.mob_seeded.insert((x, z));
            }
        }
        if let Ok(data) = fs::read(self.save_dir.join("ptouched")) {
            for p in data.chunks_exact(8) {
                let x = i32::from_le_bytes([p[0], p[1], p[2], p[3]]);
                let z = i32::from_le_bytes([p[4], p[5], p[6], p[7]]);
                self.player_touched.insert((x, z));
            }
        }
    }

    pub(super) fn try_load_chunk(&self, pos: ChunkPos) -> Option<Chunk> {
        let data = super::region::read_chunk(&self.save_dir, pos)?;
        let mut chunk = Chunk::new();
        let is_v4 = data.starts_with(b"WFC4");
        if !is_v4 && !data.starts_with(b"WFC3") {
            return None; // pre-256-height save: regenerate
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
                pos.x, pos.z
            );
            return None;
        }
        if is_v4 {
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
        }
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

    /// WFC4 block and metadata RLE, also used for multiplayer chunk streaming.
    /// WFC3 remains readable with an all-zero metadata plane.
    pub fn chunk_rle(&self, pos: ChunkPos) -> Option<Vec<u8>> {
        let chunk = self.chunks.get(&pos)?;
        let mut buf: Vec<u8> = Vec::with_capacity(4096);
        buf.extend_from_slice(b"WFC4");
        // Runs come straight off the plane, so a uniform plane is one step
        // rather than a scan of every cell. The u16 length field still caps
        // a wire run, so long runs are split to fit it.
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
        Some(buf)
    }

    /// Insert a network-streamed chunk, remapping host block ids to
    /// local ones. Relights and marks for remesh.
    pub fn insert_remote_chunk(&mut self, pos: ChunkPos, rle: &[u8], remap: &[BlockId]) {
        self.insert_remote_chunks([(pos, rle)], remap);
    }

    /// Insert a group received in one network poll and settle their shared
    /// borders through one lighting cascade.
    pub fn insert_remote_chunks<'a>(
        &mut self,
        chunks: impl IntoIterator<Item = (ChunkPos, &'a [u8])>,
        remap: &[BlockId],
    ) {
        let mut inserted = Vec::new();
        for (pos, rle) in chunks {
            if self.insert_remote_chunk_unlit(pos, rle, remap) {
                inserted.push(pos);
            }
        }
        self.relight_chunks_and_cascade(inserted);
    }

    fn insert_remote_chunk_unlit(&mut self, pos: ChunkPos, rle: &[u8], remap: &[BlockId]) -> bool {
        let is_v4 = rle.starts_with(b"WFC4");
        if !is_v4 && !rle.starts_with(b"WFC3") {
            return false;
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
        if is_v4 {
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
        }
        chunk.dirty = true;
        chunk.compact();
        self.chunks.insert(pos, chunk);
        // Neighbors need remeshing for the new border faces.
        for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let n = ChunkPos {
                x: pos.x + dx,
                z: pos.z + dz,
            };
            if let Some(c) = self.chunks.get_mut(&n) {
                c.dirty = true;
            }
        }
        true
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
                format!("chunk {},{} is not resident", pos.x, pos.z),
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
            write_world_meta_full(
                &self.save_dir,
                self.seed,
                &self.mode,
                self.ire,
                self.day,
                self.weather,
            ),
        );
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
            }
            ready
        } else {
            true
        };
        let path = self.entities_path();
        report.record("block entities", path, self.save_entities());
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
                    format!("chunk {},{}", pos.x, pos.z),
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
