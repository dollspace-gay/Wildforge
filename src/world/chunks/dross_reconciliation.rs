//! Dross reconciliation chunks transaction coordination.

use crate::chunk::CHUNK_X;
use crate::chunk::CHUNK_Y;
use crate::chunk::CHUNK_Z;
use crate::chunk::ChunkPos;
use crate::planet::BlockPos;
use crate::world::World;

impl World {
    pub(super) fn reconcile_dross_scars_chunk(&mut self, chunk: ChunkPos) {
        let Some(atlas) = self.planet_atlas.as_ref() else {
            return;
        };
        let Some(geography) = self.arcane_geography.as_ref() else {
            return;
        };
        let candidates = geography
            .dynamic
            .dross_state
            .scars
            .values()
            .filter(|site| site.resolved_step.is_none())
            .filter_map(|site| {
                let center = site.region.center(atlas.side());
                let surface = crate::planet::SurfacePos::new(
                    center.face,
                    center
                        .u
                        .floor()
                        .clamp(0.0, f64::from(crate::planet::FACE_BLOCKS - 1))
                        as u16,
                    center
                        .v
                        .floor()
                        .clamp(0.0, f64::from(crate::planet::FACE_BLOCKS - 1))
                        as u16,
                )
                .ok()?;
                const SLOT_OFFSETS: [(u16, u16); crate::dross::MAX_ACTIVE_SCARS_PER_REGION] = [
                    (0, 0),
                    (4, 0),
                    (12, 0),
                    (0, 4),
                    (0, 12),
                    (4, 4),
                    (12, 12),
                    (4, 12),
                ];
                let origin = ChunkPos::from_surface(surface).block_origin();
                let (offset_u, offset_v) = SLOT_OFFSETS[usize::from(site.site_slot)];
                let local_u = (surface.u() % CHUNK_X as u16 + offset_u) % CHUNK_X as u16;
                let local_v = (surface.v() % CHUNK_Z as u16 + offset_v) % CHUNK_Z as u16;
                let surface = crate::planet::SurfacePos::new(
                    origin.face(),
                    origin.u().saturating_add(local_u),
                    origin.v().saturating_add(local_v),
                )
                .ok()?;
                let owned_chunk = site
                    .materialized_at
                    .map_or_else(|| ChunkPos::from_surface(surface), BlockPos::chunk);
                (owned_chunk == chunk).then_some((
                    site.id,
                    site.kind,
                    site.content_id.clone(),
                    surface,
                    site.materialized_at,
                ))
            })
            .collect::<Vec<_>>();
        for (scar_id, kind, content_id, surface, saved) in candidates {
            if saved.is_none() && self.player_touched.contains(&chunk) {
                // A persisted scar selected before the chunk was authored may
                // still be waiting to materialize. The later construction
                // record wins; loading the chunk must not bypass that history.
                continue;
            }
            let Some((block, handler)) = self
                .reg
                .resolve_dross_scar(&content_id, kind)
                .map(|definition| (definition.block, definition.handler))
            else {
                continue;
            };
            let at = if let Some(saved) = saved {
                saved
            } else {
                const LOCAL_OFFSETS: [(i32, i32); 13] = [
                    (0, 0),
                    (1, 0),
                    (0, 1),
                    (-1, 0),
                    (0, -1),
                    (1, 1),
                    (-1, 1),
                    (-1, -1),
                    (1, -1),
                    (2, 0),
                    (0, 2),
                    (-2, 0),
                    (0, -2),
                ];
                let mut selected = None;
                for (du, dv) in LOCAL_OFFSETS {
                    let u = i32::from(surface.u()) + du;
                    let v = i32::from(surface.v()) + dv;
                    if !(0..i32::from(crate::planet::FACE_BLOCKS)).contains(&u)
                        || !(0..i32::from(crate::planet::FACE_BLOCKS)).contains(&v)
                    {
                        continue;
                    }
                    let candidate =
                        crate::planet::SurfacePos::new(surface.face(), u as u16, v as u16)
                            .expect("bounded scar candidate is canonical");
                    if ChunkPos::from_surface(candidate) != chunk {
                        continue;
                    }
                    let surface_y = self.surface_height_at(candidate).saturating_add(1);
                    let y = if handler == crate::dross::ScarHandler::WaterMarginFilm {
                        (1..CHUNK_Y as i32 - 1)
                            .rev()
                            .find(|height| {
                                let at = BlockPos::new(
                                    candidate.face(),
                                    candidate.u(),
                                    *height as u8,
                                    candidate.v(),
                                )
                                .expect("bounded water-margin candidate is canonical");
                                at.offset(0, -1, 0).is_some_and(|below| {
                                    self.reg.is_water(self.get_block_at(below))
                                        && matches!(self.get_block_at(at), crate::registry::AIR)
                                })
                            })
                            .unwrap_or(surface_y)
                    } else {
                        surface_y
                    };
                    if !(1..CHUNK_Y as i32 - 1).contains(&y) {
                        continue;
                    }
                    let candidate =
                        BlockPos::new(candidate.face(), candidate.u(), y as u8, candidate.v())
                            .expect("scar surface candidate is canonical");
                    let present = self.get_block_at(candidate);
                    if (present != crate::registry::AIR && present != block)
                        || self.arcane_geography.as_ref().is_some_and(|geography| {
                            geography
                                .dynamic
                                .dross_state
                                .materialized
                                .get(&candidate)
                                .is_some_and(|owner| *owner != scar_id)
                        })
                    {
                        continue;
                    }
                    let Some(below) = candidate.offset(0, -1, 0) else {
                        continue;
                    };
                    let support = self.get_block_at(below);
                    let support_definition = self.reg.block(support);
                    if !self.reg.is_solid(support)
                        && !self.reg.is_water(support)
                        && !support_definition.cross
                    {
                        continue;
                    }
                    let water_margin = self.reg.is_water(support)
                        || [(1, 0), (-1, 0), (0, 1), (0, -1)]
                            .into_iter()
                            .filter_map(|(du, dv)| below.offset(du, 0, dv))
                            .any(|neighbor| self.reg.is_water(self.get_block_at(neighbor)));
                    let organic = support_definition.cross
                        || support_definition.burns != 0
                        || ["grass", "dirt", "leaves", "log", "moss"]
                            .iter()
                            .any(|part| support_definition.name.contains(part));
                    let mineral = self.reg.is_solid(support)
                        && !support_definition.cross
                        && support_definition.burns == 0;
                    let score = match handler {
                        crate::dross::ScarHandler::WaterMarginFilm => u8::from(water_margin) * 3,
                        crate::dross::ScarHandler::FilamentGrowth => u8::from(organic) * 3,
                        crate::dross::ScarHandler::MineralCrust => u8::from(mineral) * 3,
                        crate::dross::ScarHandler::SurfaceOverlay => 1,
                    };
                    if selected
                        .as_ref()
                        .is_none_or(|(_, best_score)| score > *best_score)
                    {
                        selected = Some((candidate, score));
                    }
                }
                let Some((selected, _)) = selected else {
                    continue;
                };
                selected
            };
            let present = self.get_block_at(at);
            if present != crate::registry::AIR && present != block {
                // A structural, inventory, fluid, plant, or other authored
                // block always wins. Scars occupy space; they never replace it.
                continue;
            }
            if present != block {
                self.set_block_at(at, block);
            }
            if let Some(geography) = self.arcane_geography.as_mut()
                && let Some(site) = geography.dynamic.dross_state.scars.get_mut(&scar_id)
            {
                if let Some(previous) = site.materialized_at
                    && previous != at
                {
                    geography.dynamic.dross_state.materialized.remove(&previous);
                }
                site.materialized_at = Some(at);
                site.last_changed_step = geography.dynamic.dross_state.completed_steps;
                geography
                    .dynamic
                    .dross_state
                    .materialized
                    .insert(at, scar_id);
            }
        }
    }

    pub(in crate::world) fn refresh_loaded_dross_scars(&mut self) {
        let chunks = self.chunks.keys().copied().collect::<Vec<_>>();
        for chunk in chunks {
            self.reconcile_dross_scars_chunk(chunk);
        }
    }
}
