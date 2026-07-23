//! Mob/chunk persistence, WFC4 streaming, saves, and registry remapping.

use super::*;

impl World {
    pub(super) fn mobs_path(&self) -> PathBuf {
        self.save_dir.join("animals.toml")
    }

    pub(super) fn save_mobs(&self) {
        use std::fmt::Write as _;
        let mut out = String::new();
        for m in &self.mobs {
            let Some(def) = self.reg.animals.get(m.species) else {
                continue;
            };
            if def.hostile {
                continue; // wardens dissolve on save — never persisted
            }
            let _ = writeln!(
                out,
                "[[mob]]\nspecies = \"{}\"\npos = [{}, {}, {}]\nyaw = {}\nhealth = {}\nfed = {}\ngrowth = {}\n",
                def.name, m.pos.x, m.pos.y, m.pos.z, m.yaw, m.health, m.fed, m.growth
            );
        }
        if out.is_empty() {
            let _ = fs::remove_file(self.mobs_path());
        } else {
            let _ = fs::write(self.mobs_path(), out);
        }
        // Seeded-chunk marks: compact binary pairs.
        let mut buf = Vec::with_capacity(self.mob_seeded.len() * 8);
        for (x, z) in &self.mob_seeded {
            buf.extend_from_slice(&x.to_le_bytes());
            buf.extend_from_slice(&z.to_le_bytes());
        }
        let _ = fs::write(self.save_dir.join("aseeded"), buf);
    }

    pub(super) fn load_mobs(&mut self) {
        use serde::Deserialize;
        fn one() -> f32 {
            1.0
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
                self.mobs.push(m);
            }
        }
        if let Ok(data) = fs::read(self.save_dir.join("aseeded")) {
            for p in data.chunks_exact(8) {
                let x = i32::from_le_bytes([p[0], p[1], p[2], p[3]]);
                let z = i32::from_le_bytes([p[4], p[5], p[6], p[7]]);
                self.mob_seeded.insert((x, z));
            }
        }
    }

    pub(super) fn try_load_chunk(&self, pos: ChunkPos) -> Option<Chunk> {
        let data = fs::read(self.chunk_file(pos)).ok()?;
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
        chunk.modified = true;
        Some(chunk)
    }

    /// WFC4 block and metadata RLE, also used for multiplayer chunk streaming.
    /// WFC3 remains readable with an all-zero metadata plane.
    pub fn chunk_rle(&self, pos: ChunkPos) -> Option<Vec<u8>> {
        let chunk = self.chunks.get(&pos)?;
        let raw = chunk.raw();
        let mut buf: Vec<u8> = Vec::with_capacity(4096);
        buf.extend_from_slice(b"WFC4");
        let mut i = 0;
        while i < raw.len() {
            let b = raw[i];
            let mut run = 1usize;
            while i + run < raw.len() && raw[i + run] == b && run < u16::MAX as usize {
                run += 1;
            }
            buf.extend_from_slice(&(run as u16).to_le_bytes());
            buf.extend_from_slice(&b.to_le_bytes());
            i += run;
        }
        let meta = chunk.meta_raw();
        let mut i = 0;
        while i < meta.len() {
            let value = meta[i];
            let mut run = 1usize;
            while i + run < meta.len() && meta[i + run] == value && run < u16::MAX as usize {
                run += 1;
            }
            buf.extend_from_slice(&(run as u16).to_le_bytes());
            buf.push(value);
            i += run;
        }
        Some(buf)
    }

    /// Insert a network-streamed chunk, remapping host block ids to
    /// local ones. Relights and marks for remesh.
    pub fn insert_remote_chunk(&mut self, pos: ChunkPos, rle: &[u8], remap: &[BlockId]) {
        let is_v4 = rle.starts_with(b"WFC4");
        if !is_v4 && !rle.starts_with(b"WFC3") {
            return;
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
        self.chunks.insert(pos, chunk);
        self.relight_and_cascade(pos);
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
    }

    pub(super) fn save_chunk(&self, pos: ChunkPos) -> std::io::Result<()> {
        let buf = self.chunk_rle(pos).unwrap_or_default();
        let mut f = fs::File::create(self.chunk_file(pos))?;
        f.write_all(&buf)
    }

    pub fn save_modified(&self) {
        if self.remote {
            return; // the host owns the world
        }
        let _ = fs::create_dir_all(&self.save_dir);
        write_world_meta_full(
            &self.save_dir,
            self.seed,
            &self.mode,
            self.ire,
            self.day,
            self.weather,
        );
        self.write_palette();
        self.save_entities();
        self.save_mobs();
        self.save_stamps();
        for (pos, chunk) in &self.chunks {
            if chunk.modified {
                let _ = self.save_chunk(*pos);
            }
        }
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
            chunk.dirty = true;
        }
        self.load_remap = self.read_palette_remap();
    }

    // ---------------- lighting ----------------
}
