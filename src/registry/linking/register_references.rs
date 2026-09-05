//! Register references in provider order with owned deferred references.

use super::Registration;
use crate::registry::schema::{RawMod};
use std::collections::HashMap;
use super::super::pending::{PendingAnimal, PendingNpc};

impl Registration {
    pub(super) fn references(&mut self, raw: &RawMod, errs: &mut Vec<String>) {
        for r in &raw.recipes {
            self.pending.recipes.push((raw.info.id.clone(), r.clone()));
        }
        for f in &raw.features {
            self.pending.features.push((raw.info.id.clone(), f.clone()));
        }
        for it in &raw.items {
            if let Some(p) = &it.places {
                self.pending.places.push((raw.info.id.clone(), (it.id.clone(), p.clone())));
            }
        }
        for t in &raw.tags {
            self.pending.tags.push((raw.info.id.clone(), t.clone()));
        }
        for s in &raw.smelts {
            self.pending.smelts.push((raw.info.id.clone(), s.clone()));
        }
        for b in &raw.bloomeries {
            self.pending.bloomeries.push((raw.info.id.clone(), b.clone()));
        }
        for w in &raw.workeds {
            self.pending.workeds.push((raw.info.id.clone(), w.clone()));
        }
        for k in &raw.kilns {
            self.pending.kilns.push((raw.info.id.clone(), k.clone()));
        }
        for k in &raw.kiln_bases {
            self.pending.kiln_bases.push((raw.info.id.clone(), k.clone()));
        }
        for st in &raw.structures {
            self.pending.structs.push((raw.info.id.clone(), st.clone()));
        }
        for p in &raw.pieces {
            self.pending.pieces.push((raw.info.id.clone(), p.clone()));
        }
        for p in &raw.pools {
            self.pending.pools.push((raw.info.id.clone(), p.clone()));
        }
        for a in &raw.assemblies {
            self.pending.assemblies.push((raw.info.id.clone(), a.clone()));
        }
        for s in &raw.settlements {
            self.pending.settlements.push((raw.info.id.clone(), s.clone()));
        }
        for lt in &raw.loots {
            self.pending.loots.push((raw.info.id.clone(), lt.clone()));
        }
        for a in &raw.animals {
            let tile = self.textures.resolve(&a.tex, &raw.info.path, errs);
            let head = a
                .head_tex
                .as_ref()
                .map(|t| self.textures.resolve(t, &raw.info.path, errs))
                .unwrap_or(tile);
            let box_tiles: HashMap<String, u16> = a
                .model
                .iter()
                .filter_map(|(n, b)| {
                    b.tex
                        .as_ref()
                        .map(|t| (n.clone(), self.textures.resolve(t, &raw.info.path, errs)))
                })
                .collect();
            let proj_tile = a
                .projectile
                .as_ref()
                .map(|pr| self.textures.resolve(&pr.tex, &raw.info.path, errs));
            let mut attack_proj_tiles: Vec<Option<u16>> = Vec::new();
            for atk in &a.attacks {
                match atk.kind.as_str() {
                    "melee" | "charge" | "projectile" => {}
                    other => errs.push(format!("animal {}: unknown attack kind {other}", a.id)),
                }
                attack_proj_tiles.push(
                    atk.projectile
                        .as_ref()
                        .map(|pr| self.textures.resolve(&pr.tex, &raw.info.path, errs)),
                );
            }
            match a.behavior.as_deref() {
                None
                | Some(
                    "standard" | "brute" | "construct" | "builder" | "rusher" | "tank" | "sniper"
                    | "support" | "swarm" | "controller" | "phaser" | "shield_bearer",
                ) => {}
                Some(other) => errs.push(format!("animal {}: unknown behavior {other}", a.id)),
            }
            // E9: an archetype that names a companion species must resolve
            // it (checked against base + this mod's roster names here; the
            // full cross-mod resolution happens after the roster exists).
            if a.behavior.as_deref() == Some("swarm") || a.behavior.as_deref() == Some("controller")
            {
                let spawn = if a.behavior.as_deref() == Some("swarm") {
                    a.swarm.as_ref().and_then(|s| s.spawn.clone())
                } else {
                    a.controller.as_ref().and_then(|c| c.spawn.clone())
                };
                if spawn.is_none() {
                    errs.push(format!(
                        "animal {}: behavior {} requires a companion species (`spawn`)",
                        a.id,
                        a.behavior.as_deref().unwrap_or("")
                    ));
                }
            }
            self.pending.animals.push(PendingAnimal {
                modid: raw.info.id.clone(), definition: a.clone(), tile, head_tile: head,
                box_tiles, proj_tile, attack_proj_tiles,
            });
        }
        for n in &raw.npcs {
            let tile = self.textures.resolve(&n.tex, &raw.info.path, errs);
            let head = n
                .head_tex
                .as_ref()
                .map(|t| self.textures.resolve(t, &raw.info.path, errs))
                .unwrap_or(tile);
            let box_tiles: HashMap<String, u16> = n
                .model
                .iter()
                .filter_map(|(name, b)| {
                    b.tex
                        .as_ref()
                        .map(|t| (name.clone(), self.textures.resolve(t, &raw.info.path, errs)))
                })
                .collect();
            self.pending.npcs.push(PendingNpc { modid: raw.info.id.clone(), definition: n.clone(), tile, head, box_tiles });
        }
        for d in &raw.dialogues {
            self.pending.dialogues.push((raw.info.id.clone(), d.clone()));
        }
        for q in &raw.quests {
            self.pending.quests.push((raw.info.id.clone(), q.clone()));
        }
        for f in &raw.fuels {
            self.pending.fuels.push((raw.info.id.clone(), f.clone()));
        }
        for a in &raw.aliases {
            self.pending.aliases.push((raw.info.id.clone(), a.clone()));
        }
    }
}
