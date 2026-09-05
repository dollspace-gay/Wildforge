//! Link loot, templates, pieces, pools, and assemblies in dependency order.

use std::collections::HashMap;
use crate::registry::{Registry, LootEntry, StructureDef, PieceConnector, PieceMarker, PieceChest, PieceDef, PoolEntry, PoolDef, AssemblyDef, TerrainAdaptation, DungeonDef, qualify};
use crate::registry::schema::{LootToml, StructureToml, PieceToml, PoolToml, AssemblyToml};
use super::lookups::{lookup_item, lookup_block, lookup_piece, parse_direction4};

pub(super) fn loot(reg: &mut Registry, pending_loots: Vec<(String, LootToml)>) {
    for (modid, lt) in pending_loots {
        let entries: Vec<LootEntry> = lt
            .entries
            .iter()
            .filter_map(|e| {
                lookup_item(reg, &modid, &e.item).map(|item| LootEntry {
                    item,
                    weight: e.weight.max(1),
                    count: e.count.map(|c| (c[0], c[1])).unwrap_or((1, 1)),
                    durability_frac: e.durability,
                })
            })
            .collect();
        if !entries.is_empty() {
            reg.loots.insert(qualify(&modid, &lt.id), entries);
        }
    }
}

pub(super) fn templates(reg: &mut Registry, pending_structs: Vec<(String, StructureToml)>) {
    for (modid, st) in pending_structs {
        let mut palette = HashMap::new();
        let mut ok = true;
        for (ch, block) in &st.palette {
            let Some(c) = ch.chars().next() else { continue };
            match lookup_block(reg, &modid, block) {
                Some(b) => {
                    palette.insert(c, b);
                }
                None => ok = false,
            }
        }
        if !ok {
            continue;
        }
        reg.structures.push(StructureDef {
            name: qualify(&modid, &st.id),
            biomes: st.biomes.iter().map(|b| b.to_lowercase()).collect(),
            rarity: st.rarity.max(1),
            buried: if st.placement.as_deref() == Some("buried") {
                let d = st.depth.unwrap_or([5, 15]);
                Some((d[0], d[1].max(d[0])))
            } else {
                None
            },
            palette,
            layers: st.layers,
            loot: st.loot.as_ref().map(|l| qualify(&modid, l)),
        });
    }
}

pub(super) fn pieces(reg: &mut Registry, pending_pieces: Vec<(String, PieceToml)>) {
    // Pieces reference only block *names* (resolved at stamp time), so cells
    // pass through verbatim. Connector facings are validated against the four
    // cardinal directions at load, keeping the walk free of parse errors.
    for (modid, p) in pending_pieces {
        let mut connectors = Vec::new();
        let mut ok = true;
        for c in p.connectors {
            let Some(facing) = parse_direction4(&c.facing) else {
                ok = false;
                break;
            };
            connectors.push(PieceConnector {
                du: c.du,
                dy: c.dy,
                dv: c.dv,
                kind: qualify(&modid, &c.kind),
                facing,
            });
        }
        if !ok {
            continue;
        }
        let cells = p
            .cells
            .into_iter()
            .map(|c| crate::world::template::TemplateCell {
                du: c.du,
                dy: c.dy,
                dv: c.dv,
                block: c.block,
            })
            .collect();
        let markers = p
            .markers
            .into_iter()
            .map(|m| PieceMarker {
                du: m.du,
                dy: m.dy,
                dv: m.dv,
                kind: qualify(&modid, &m.kind),
            })
            .collect();
        let chests = p
            .chests
            .into_iter()
            .filter_map(|c| {
                reg.loots
                    .contains_key(&qualify(&modid, &c.loot))
                    .then(|| PieceChest {
                        du: c.du,
                        dy: c.dy,
                        dv: c.dv,
                        loot: qualify(&modid, &c.loot),
                    })
            })
            .collect();
        reg.pieces.push(PieceDef {
            name: qualify(&modid, &p.id),
            cells,
            connectors,
            markers,
            chests,
            settlement_tier: p.settlement_tier.max(1),
        });
    }
}

pub(super) fn pools(reg: &mut Registry, pending_pools: Vec<(String, PoolToml)>) {
    for (modid, pool) in pending_pools {
        let entries: Vec<PoolEntry> = pool
            .entries
            .into_iter()
            .filter_map(|e| {
                lookup_piece(reg, &modid, &e.piece).map(|_| PoolEntry {
                    piece: qualify(&modid, &e.piece),
                    weight: e.weight.max(1),
                })
            })
            .collect();
        if !entries.is_empty() {
            reg.pools.push(PoolDef {
                id: qualify(&modid, &pool.id),
                entries,
            });
        }
    }
}

pub(super) fn assemblies(reg: &mut Registry, pending_assemblies: Vec<(String, AssemblyToml)>) {
    for (modid, a) in pending_assemblies {
        let Some(entry_piece) = lookup_piece(reg, &modid, &a.entry) else {
            continue;
        };
        let terrain = match a.terrain.as_deref() {
            Some("bury") => TerrainAdaptation::Bury,
            Some("encapsulate") => TerrainAdaptation::Encapsulate,
            _ => TerrainAdaptation::None,
        };
        let pools = a
            .pools
            .into_iter()
            .filter_map(|(kind, pool)| {
                let kind = qualify(&modid, &kind);
                let pool = qualify(&modid, &pool);
                reg.pools
                    .iter()
                    .any(|p| p.id == pool)
                    .then_some((kind, pool))
            })
            .collect();
        reg.assemblies.push(AssemblyDef {
            name: qualify(&modid, &a.id),
            biomes: a.biomes.iter().map(|b| b.to_lowercase()).collect(),
            rarity: a.rarity.max(1),
            entry_piece,
            pools,
            max_depth: a.max_depth.max(1),
            max_pieces: a.max_pieces.max(1),
            terrain,
            settlement: a.settlement.as_ref().map(|s| qualify(&modid, s)),
            dungeon: a.dungeon.as_ref().map(|d| DungeonDef {
                reset: d.reset.unwrap_or(60.0).max(1.0),
            }),
        });
    }
}
