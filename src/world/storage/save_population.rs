//! Save population storage transaction coordination.

use std::path::PathBuf;
use crate::world::SaveFailure;
use crate::world::SaveReport;
use crate::world::World;

impl World {
    pub(in crate::world) fn mobs_path(&self) -> PathBuf {
        self.save_dir.join("animals.toml")
    }

    pub(in crate::world) fn save_mobs(&self) -> Vec<SaveFailure> {
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
            crate::world::persistence::replace_or_remove(
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
            crate::world::persistence::replace_or_remove(
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
            crate::world::persistence::replace_or_remove(
                &path,
                (!self.bloom.is_empty()).then_some(bb.as_slice()),
            ),
        );
        let path = self.save_dir.join("longwinter");
        report.record(
            "long winter",
            path.clone(),
            crate::world::persistence::atomic_replace(&path, if self.calendar_state.long_winter() { b"1" } else { b"0" }),
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
            crate::world::persistence::replace_or_remove(
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
            crate::world::persistence::replace_or_remove(
                &path,
                (!self.hearts.is_empty()).then_some(hb.as_slice()),
            ),
        );
        // Planetary seeded-chunk marks: magic followed by face/u/v records.
        let mut buf = Vec::with_capacity(4 + self.population.seeded_chunks().len() * 5);
        buf.extend_from_slice(b"WFA1");
        for pos in self.population.seeded_chunks() {
            buf.push(pos.face() as u8);
            buf.extend_from_slice(&pos.u().to_le_bytes());
            buf.extend_from_slice(&pos.v().to_le_bytes());
        }
        let path = self.save_dir.join("aseeded");
        report.record(
            "animal seed marks",
            path.clone(),
            crate::world::persistence::atomic_replace(&path, &buf),
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
            crate::world::persistence::atomic_replace(&path, &pt),
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
            crate::world::persistence::atomic_replace(&path, &structures),
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
            crate::world::persistence::atomic_replace(&path, &gates),
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
            crate::world::persistence::atomic_replace(&path, &nests),
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
            crate::world::persistence::atomic_replace(&path, &settlements),
        );
        report.failures
    }
}
