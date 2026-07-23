//! World metadata, palettes, save paths, and persistence codecs.

use super::*;

impl World {
    /// Load a world from disk (reads seed + palette) or create a fresh one.
    pub fn load_or_create(save_dir: PathBuf, reg: Arc<Registry>) -> World {
        let (seed, mode, ire, day, weather) = read_world_meta_full(&save_dir);
        let seed = seed.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as u32)
                .unwrap_or(1337)
        });
        write_world_meta_full(&save_dir, seed, &mode, ire, day, weather);
        let mut w = World::new(seed, save_dir, reg);
        w.mode = mode;
        w.ire = ire;
        w.day = day;
        w.weather = weather;
        w.clock = day as f64 * crate::server::DAY_LENGTH as f64;
        w.load_remap = w.read_palette_remap();
        w.load_entities();
        w.load_mobs();
        w.load_stamps();
        w
    }

    /// Per-chunk random-tick stamps: compact (x, z, time) triples.
    pub(super) fn load_stamps(&mut self) {
        let Ok(buf) = fs::read(self.save_dir.join("stamps")) else {
            return;
        };
        for rec in buf.chunks_exact(16) {
            let x = i32::from_le_bytes(rec[0..4].try_into().unwrap());
            let z = i32::from_le_bytes(rec[4..8].try_into().unwrap());
            let t = f64::from_le_bytes(rec[8..16].try_into().unwrap());
            self.last_random.insert((x, z), t);
        }
    }

    pub(super) fn save_stamps(&self) {
        let mut buf = Vec::with_capacity(self.last_random.len() * 16);
        for ((x, z), t) in &self.last_random {
            buf.extend_from_slice(&x.to_le_bytes());
            buf.extend_from_slice(&z.to_le_bytes());
            buf.extend_from_slice(&t.to_le_bytes());
        }
        let _ = fs::write(self.save_dir.join("stamps"), buf);
    }

    /// Map every stored numeric id to a current runtime id via string names.
    pub(super) fn read_palette_remap(&self) -> Vec<BlockId> {
        let Ok(text) = fs::read_to_string(self.save_dir.join("palette")) else {
            return Vec::new();
        };
        let mut remap = Vec::new();
        for line in text.lines() {
            let Some((num, name)) = line.split_once(' ') else {
                continue;
            };
            let Ok(num) = num.parse::<usize>() else {
                continue;
            };
            if remap.len() <= num {
                remap.resize(num + 1, self.reg.unknown_block);
            }
            remap[num] = self
                .reg
                .block_id(name.trim())
                .unwrap_or(self.reg.unknown_block);
        }
        remap
    }

    /// Write the current registry as this world's palette (runtime ids are
    /// stored ids from now on).
    pub(super) fn write_palette(&self) {
        let mut out = String::new();
        for (i, b) in self.reg.blocks.iter().enumerate() {
            out.push_str(&format!("{i} {}\n", b.name));
        }
        let _ = fs::write(self.save_dir.join("palette"), out);
    }

    pub(super) fn chunk_file(&self, pos: ChunkPos) -> PathBuf {
        self.save_dir.join(format!("c.{}.{}.wfc", pos.x, pos.z))
    }
}
