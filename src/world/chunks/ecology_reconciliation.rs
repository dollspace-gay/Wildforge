//! Ecology reconciliation chunks transaction coordination.

use crate::chunk::CHUNK_Y;
use crate::chunk::ChunkPos;
use crate::world::World;

impl World {
    /// Materialize the one sparse representative block for each persistent
    /// ecology site in this chunk. A saved site wins over generation order;
    /// player-touched terrain wins over retrogen/materialization.
    pub(super) fn reconcile_arcane_ecology_chunk(&mut self, chunk: ChunkPos) {
        let Some(geography) = self.arcane_geography.as_ref() else {
            return;
        };
        let candidates = geography
            .dynamic
            .ecology
            .sites
            .iter()
            .filter_map(|site| {
                let surface = site.surface()?;
                (ChunkPos::from_surface(surface) == chunk).then(|| {
                    (
                        site.id,
                        site.content_id.clone(),
                        surface,
                        site.materialized_y,
                        site.stage,
                        site.ownership,
                        site.charge_total(),
                    )
                })
            })
            .collect::<Vec<_>>();
        for (site_id, content_id, surface, saved_y, stage, ownership, charge) in candidates {
            let Some(block) = self.reg.block_id(&content_id) else {
                continue;
            };
            let state_variant = (content_id == "base:lantern_reed")
                .then(|| self.reg.block_id("base:lantern_reed_dim"))
                .flatten();
            let display_block = if charge == 0 {
                state_variant.unwrap_or(block)
            } else {
                block
            };
            let y = if saved_y != 0 {
                i32::from(saved_y)
            } else {
                if ownership != crate::arcane_ecology::EcologyOwnership::Cultivated
                    && self.player_touched.contains(&chunk)
                {
                    continue;
                }
                let definition = self.reg.block(block).arcane_ecology.as_ref();
                let cave = definition.is_some_and(|definition| {
                    definition
                        .habitat
                        .iter()
                        .any(|tag| matches!(tag.as_str(), "cave" | "moist_cave" | "subsurface"))
                });
                let top = self.surface_height_at(surface).clamp(2, CHUNK_Y as i32 - 2);
                if cave {
                    (2..top)
                        .rev()
                        .find(|height| {
                            let at = crate::planet::BlockPos::new(
                                surface.face(),
                                surface.u(),
                                *height as u8,
                                surface.v(),
                            )
                            .expect("ecology cave candidate is inside shell");
                            at.offset(0, -1, 0).is_some_and(|below| {
                                self.get_block_at(at) == crate::registry::AIR
                                    && self.reg.is_solid(self.get_block_at(below))
                            })
                        })
                        .unwrap_or(top + 1)
                } else {
                    top + 1
                }
            };
            if !(1..CHUNK_Y as i32 - 1).contains(&y) {
                continue;
            }
            let at =
                crate::planet::BlockPos::new(surface.face(), surface.u(), y as u8, surface.v())
                    .expect("ecology materialization position is canonical");
            let present = self.get_block_at(at);
            let absent = matches!(
                stage,
                crate::arcane_ecology::EcologyStage::Collapsed
                    | crate::arcane_ecology::EcologyStage::Harvested
                    | crate::arcane_ecology::EcologyStage::Dormant
            );
            if absent {
                if present == block || state_variant == Some(present) {
                    self.set_block_at(at, crate::registry::AIR);
                }
                continue;
            }
            if present != display_block {
                // Ecological representatives never overwrite water, ordinary
                // plants, ruins, or player blocks. A skipped natural site can
                // recover after habitat repair; it does not reroll elsewhere.
                if present != crate::registry::AIR
                    && present != block
                    && state_variant != Some(present)
                {
                    continue;
                }
                self.set_block_at(at, display_block);
            }
            if let Some(site) = self.arcane_geography.as_mut().and_then(|geography| {
                geography
                    .dynamic
                    .ecology
                    .sites
                    .iter_mut()
                    .find(|site| site.id == site_id)
            }) {
                site.materialized_y = y as u8;
            }
        }
    }

    pub(in crate::world) fn refresh_loaded_arcane_ecology(&mut self) {
        let chunks = self.chunks.keys().copied().collect::<Vec<_>>();
        for chunk in chunks {
            self.reconcile_arcane_ecology_chunk(chunk);
        }
    }
}
