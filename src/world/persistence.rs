//! World metadata, palettes, save paths, and persistence codecs.

use super::*;

/// Stamps are only meaningful against the DAY_LENGTH they were written
/// under. Bump this whenever that changes and old stamps are discarded
/// rather than misread as an absence.
const STAMPS_MAGIC: &[u8] = b"WFS3-PLANET-1200";

impl World {
    /// Load a world from disk (reads seed + palette) or create a fresh one.
    pub fn load_or_create(save_dir: PathBuf, reg: Arc<Registry>) -> std::io::Result<World> {
        let existing = load_world_meta(&save_dir)?;
        let seed = existing.as_ref().map(|meta| meta.seed).unwrap_or_else(|| {
            std::env::var("WILDFORGE_SEED")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or_else(|| {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as u32)
                        .unwrap_or(1337)
                })
        });
        let (mode, ire, day, weather) = existing
            .map(|meta| (meta.mode, meta.ire, meta.day, meta.weather))
            .unwrap_or_else(|| ("survival".to_string(), 0.0, 0, Weather::Clear));
        write_world_meta_full(&save_dir, seed, &mode, ire, day, weather)?;
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
        Ok(w)
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
        for rec in body.chunks_exact(13) {
            let Some(face) = crate::planet::Face::from_u8(rec[0]) else {
                continue;
            };
            let u = u16::from_le_bytes(rec[1..3].try_into().unwrap());
            let v = u16::from_le_bytes(rec[3..5].try_into().unwrap());
            let Ok(pos) = ChunkPos::new(face, u, v) else {
                continue;
            };
            let t = f64::from_le_bytes(rec[5..13].try_into().unwrap());
            self.last_random.insert(pos, t);
        }
    }

    pub(super) fn save_stamps(&self) -> std::io::Result<()> {
        let mut buf = Vec::with_capacity(STAMPS_MAGIC.len() + self.last_random.len() * 13);
        buf.extend_from_slice(STAMPS_MAGIC);
        for (pos, t) in &self.last_random {
            buf.push(pos.face() as u8);
            buf.extend_from_slice(&pos.u().to_le_bytes());
            buf.extend_from_slice(&pos.v().to_le_bytes());
            buf.extend_from_slice(&t.to_le_bytes());
        }
        atomic_replace(&self.save_dir.join("stamps"), &buf)
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
    pub(super) fn write_palette(&self) -> std::io::Result<()> {
        atomic_replace(
            &self.save_dir.join("palette"),
            self.palette_text().as_bytes(),
        )
    }
}

pub(super) fn atomic_replace(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    crate::identity::atomic_write(path, bytes, false)
}

pub(super) fn remove_if_exists(path: &std::path::Path) -> std::io::Result<()> {
    crate::persist::remove_if_exists(path)
}

pub(super) fn replace_or_remove(
    path: &std::path::Path,
    bytes: Option<&[u8]>,
) -> std::io::Result<()> {
    match bytes {
        Some(bytes) => atomic_replace(path, bytes),
        None => remove_if_exists(path),
    }
}
