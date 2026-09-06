//! Voxel trial for the common spawn contract.

use super::FRESH_WATER_REACH_BLOCKS;
use super::LOCAL_RESOURCE_REACH_BLOCKS;
use super::SPAWN_VERIFICATION_VERSION;
use super::SpawnVerification;
use super::TrialColumn;
use super::TrialQualification;
use super::TrialReject;
use super::entry_chunks;
use crate::chunk::CHUNK_X;
use crate::chunk::CHUNK_Y;
use crate::chunk::CHUNK_Z;
use crate::chunk::Chunk;
use crate::chunk::ChunkPos;
use crate::chunk::SEA_LEVEL;
use crate::planet::SurfacePos;
use crate::planet_atlas::PlanetAtlas;
use crate::registry::Registry;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;

pub(super) fn qualify_trial_region(
    reg: &Registry,
    atlas: &PlanetAtlas,
    center: SurfacePos,
    chunks: &[(ChunkPos, Chunk)],
) -> Result<TrialQualification, TrialReject> {
    if chunks.len() != entry_chunks(center).len()
        || chunks
            .iter()
            .map(|(position, _)| *position)
            .collect::<Vec<_>>()
            != entry_chunks(center)
    {
        return Err(TrialReject {
            reason: "trial chunk set does not match the required entry region".into(),
            fallback: None,
        });
    }
    let log_items = reg.tags.get("base:logs");
    let log_blocks = reg
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| {
            let item = reg.item_id(&block.name)?;
            log_items
                .is_some_and(|logs| logs.contains(&item))
                .then_some(crate::registry::BlockId(index as u16))
        })
        .collect::<HashSet<_>>();
    let is_soil = |name: &str| {
        matches!(
            name,
            "base:grass" | "base:dirt" | "base:mud" | "base:sand" | "base:clay"
        )
    };
    let is_stone = |name: &str| {
        matches!(
            name,
            "base:stone"
                | "base:gravel"
                | "base:sandstone"
                | "base:limestone"
                | "base:shale"
                | "base:granite"
                | "base:marble"
                | "base:slate"
                | "base:quartzite"
                | "base:basalt"
        )
    };
    let mut columns = HashMap::<SurfacePos, TrialColumn>::new();
    for (position, chunk) in chunks {
        let origin = position.block_origin();
        for x in 0..CHUNK_X {
            for z in 0..CHUNK_Z {
                let surface =
                    SurfacePos::new(origin.face(), origin.u() + x as u16, origin.v() + z as u16)
                        .expect("chunk-local surface is canonical");
                let height = (0..CHUNK_Y)
                    .rev()
                    .find(|&y| reg.is_solid(chunk.get(x, y, z)))
                    .unwrap_or(0) as i32;
                let feet = (height + 1).clamp(0, CHUNK_Y as i32 - 1) as usize;
                let head = (height + 2).clamp(0, CHUNK_Y as i32 - 1) as usize;
                let clear = |block| !reg.is_solid(block) && !reg.is_fluid(block);
                let mut column = TrialColumn {
                    height,
                    safe: height > SEA_LEVEL + 1
                        && height + 2 < CHUNK_Y as i32
                        && reg.is_solid(chunk.get(x, height as usize, z))
                        && clear(chunk.get(x, feet, z))
                        && clear(chunk.get(x, head, z)),
                    wood: false,
                    drinkable_water: false,
                    soil: false,
                    stone: false,
                    plant: false,
                };
                let drinkable = atlas.hydrology_sample(surface.center()).salinity < 48;
                for y in 1..CHUNK_Y {
                    let block = chunk.get(x, y, z);
                    let definition = reg.block(block);
                    column.wood |= log_blocks.contains(&block);
                    column.drinkable_water |= drinkable && reg.water_volume(block).is_some();
                    column.soil |= is_soil(&definition.name);
                    column.stone |= is_stone(&definition.name);
                    column.plant |= (definition.cross && definition.burns > 0)
                        || definition.name.ends_with("_leaves")
                        || definition.name == "base:leaves";
                    if reg.is_lava(block) && (y as i32 - height).abs() <= 3 {
                        column.safe = false;
                    }
                }
                columns.insert(surface, column);
            }
        }
    }

    // A safe doorstep must be in the trial's center chunk. This keeps the
    // exact 5x5 prepared set centered on the final spawn even when the atlas
    // candidate itself lies on a chunk boundary.
    let center_chunk = ChunkPos::from_surface(center);
    let mut spawn = None;
    let mut spawn_score = f64::INFINITY;
    for x in 0..CHUNK_X {
        for z in 0..CHUNK_Z {
            let origin = center_chunk.block_origin();
            let surface =
                SurfacePos::new(origin.face(), origin.u() + x as u16, origin.v() + z as u16)
                    .expect("center chunk surface is canonical");
            let Some(column) = columns.get(&surface).copied() else {
                continue;
            };
            if !column.safe {
                continue;
            }
            let maximum_step = crate::planet::neighbors4(surface)
                .into_iter()
                .filter_map(|neighbor| columns.get(&neighbor))
                .map(|neighbor| (column.height - neighbor.height).abs())
                .max()
                .unwrap_or(i32::MAX);
            if maximum_step > 2 {
                continue;
            }
            let score = f64::from(maximum_step) * 100.0
                + crate::planet::geodesic_distance(center.center(), surface.center());
            if score < spawn_score {
                spawn_score = score;
                spawn = Some(surface);
            }
        }
    }
    let Some(spawn) = spawn else {
        return Err(TrialReject {
            reason: "no dry two-block-high standing cell in the center chunk".into(),
            fallback: None,
        });
    };

    let mut queue = VecDeque::from([spawn]);
    let mut visited = HashSet::from([spawn]);
    let mut wood = false;
    let mut water = false;
    let mut soil = false;
    let mut stone = false;
    let mut plant = false;
    let mut walkable_cells = 0;
    while let Some(surface) = queue.pop_front() {
        let column = columns[&surface];
        let distance = crate::planet::geodesic_distance(spawn.center(), surface.center());
        if distance <= LOCAL_RESOURCE_REACH_BLOCKS {
            walkable_cells += 1;
        }
        let nearby = std::iter::once(surface).chain(crate::planet::neighbors4(surface));
        for candidate in nearby {
            if let Some(signal) = columns.get(&candidate) {
                if distance <= LOCAL_RESOURCE_REACH_BLOCKS {
                    wood |= signal.wood;
                    soil |= signal.soil;
                    stone |= signal.stone;
                    plant |= signal.plant;
                }
                if distance <= FRESH_WATER_REACH_BLOCKS {
                    water |= signal.drinkable_water;
                }
            }
        }
        for neighbor in crate::planet::neighbors4(surface) {
            let Some(next) = columns.get(&neighbor) else {
                continue;
            };
            if !next.safe
                || (column.height - next.height).abs() > 1
                || crate::planet::geodesic_distance(spawn.center(), neighbor.center())
                    > FRESH_WATER_REACH_BLOCKS
                || !visited.insert(neighbor)
            {
                continue;
            }
            queue.push_back(neighbor);
        }
    }
    let verification = SpawnVerification {
        contract_version: SPAWN_VERIFICATION_VERSION,
        walkable_cells,
        safe_standing: true,
        reachable_wood: wood,
        reachable_fresh_water: water,
        reachable_soil: soil,
        reachable_stone: stone,
        reachable_plants: plant,
    };
    if verification.is_fully_qualified() {
        Ok(TrialQualification {
            spawn,
            verification,
        })
    } else {
        let reason = format!(
            "walkable={} wood={} fresh_water={} soil={} stone={} plants={}",
            verification.walkable_cells,
            verification.reachable_wood,
            verification.reachable_fresh_water,
            verification.reachable_soil,
            verification.reachable_stone,
            verification.reachable_plants,
        );
        // A safe doorstep with only some resources nearby is still somewhere a
        // player can stand and start — kept as a fallback for the caller.
        Err(TrialReject {
            reason,
            fallback: Some(TrialQualification {
                spawn,
                verification,
            }),
        })
    }
}
