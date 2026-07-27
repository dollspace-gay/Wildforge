//! World metadata, palettes, save paths, and persistence codecs.

use super::*;

/// Stamps are only meaningful against the DAY_LENGTH they were written
/// under. Bump this whenever that changes and old stamps are discarded
/// rather than misread as an absence.
const STAMPS_MAGIC: &[u8] = b"WFS2-1200";

impl World {
    /// Load a world from disk (reads seed + palette) or create a fresh one.
    pub fn load_or_create(save_dir: PathBuf, reg: Arc<Registry>) -> World {
        let (seed, mode, ire, day, weather) = read_world_meta_full(&save_dir);
        let seed = seed.unwrap_or_else(|| {
            if let Some(seed) = std::env::var("WILDFORGE_SEED")
                .ok()
                .and_then(|value| value.parse().ok())
            {
                return seed;
            }
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
        w.palette_stale = !w.palette_matches_registry();
        w.load_entities();
        w.load_mobs();
        w.load_stamps();
        w
    }

    /// Per-chunk random-tick stamps: compact (x, z, time) triples.
    ///
    /// The times are clock seconds, and the clock's unit is DAY_LENGTH.
    /// When that changed, every stamp from an older save started
    /// reading as days of absence — so the whole explored world would
    /// have fired its catch-up burst (crops jumping, snow phasing) the
    /// first time it loaded. The header says whose clock these are;
    /// anything older is dropped, which costs only the catch-up itself.
    pub(super) fn load_stamps(&mut self) {
        let Ok(buf) = fs::read(self.save_dir.join("stamps")) else {
            return;
        };
        let Some(body) = buf.strip_prefix(STAMPS_MAGIC) else {
            return;
        };
        for rec in body.chunks_exact(16) {
            let x = i32::from_le_bytes(rec[0..4].try_into().unwrap());
            let z = i32::from_le_bytes(rec[4..8].try_into().unwrap());
            let t = f64::from_le_bytes(rec[8..16].try_into().unwrap());
            self.last_random.insert((x, z), t);
        }
    }

    pub(super) fn save_stamps(&self) {
        let mut buf = Vec::with_capacity(STAMPS_MAGIC.len() + self.last_random.len() * 16);
        buf.extend_from_slice(STAMPS_MAGIC);
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
            // WFC3 saves from before named palettes stored the then-current
            // registry ids directly. Treating a missing palette as an empty
            // remap turns every cell, including air, into base:unknown and
            // produces solid chunk-height magenta pillars.
            return self.identity_palette_remap();
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
        if remap.is_empty() {
            self.identity_palette_remap()
        } else {
            remap
        }
    }

    fn identity_palette_remap(&self) -> Vec<BlockId> {
        (0..self.reg.blocks.len())
            .map(|id| BlockId(id as u16))
            .collect()
    }

    /// The palette this registry would write: one `id name` line per block.
    fn palette_text(&self) -> String {
        let mut out = String::new();
        for (i, b) in self.reg.blocks.iter().enumerate() {
            out.push_str(&format!("{i} {}\n", b.name));
        }
        out
    }

    /// Does the saved palette already describe this registry? When it does,
    /// every chunk file on disk is written in ids we still understand, so a
    /// chunk that loads unedited does not need saving again. When it does
    /// NOT (a mod arrived, the game updated), chunk files carry stale ids
    /// and every one we touch has to be rewritten under the new palette.
    pub(super) fn palette_matches_registry(&self) -> bool {
        fs::read_to_string(self.save_dir.join("palette")).is_ok_and(|on_disk| {
            // A fresh world has no chunks yet, so a missing palette is not
            // a mismatch — but an unreadable one we treat as stale.
            on_disk == self.palette_text()
        })
    }

    /// Write the current registry as this world's palette (runtime ids are
    /// stored ids from now on).
    pub(super) fn write_palette(&self) {
        let _ = fs::write(self.save_dir.join("palette"), self.palette_text());
    }

    pub(super) fn chunk_file(&self, pos: ChunkPos) -> PathBuf {
        self.save_dir.join(format!("c.{}.{}.wfc", pos.x, pos.z))
    }
}
