//! Sub-voxel block metadata and angle-of-repose sand relaxation.

use super::*;

impl World {
    #[inline]
    fn sand_octant(&self, sand: BlockId, half_x: i32, half_z: i32, height: i32) -> bool {
        if height < 0 {
            return false;
        }
        let (x, qx) = (half_x.div_euclid(2), half_x.rem_euclid(2) as u32);
        let (z, qz) = (half_z.div_euclid(2), half_z.rem_euclid(2) as u32);
        let (y, oy) = (height.div_euclid(2), height.rem_euclid(2) as u32);
        self.get_block(x, y, z) == sand
            && self.get_meta(x, y, z) & (1 << ((oy << 2) | (qz << 1) | qx)) != 0
    }

    /// Height of the next free octant in one half-column.
    fn sand_surface(&self, sand: BlockId, half_x: i32, half_z: i32, ref_y: i32) -> i32 {
        let (x, z) = (half_x.div_euclid(2), half_z.div_euclid(2));
        for height in ((ref_y - 4) * 2..=(ref_y + 4) * 2 + 1).rev() {
            if self.sand_octant(sand, half_x, half_z, height) {
                return height + 1;
            }
        }
        for y in ((ref_y - 4)..=(ref_y + 4)).rev() {
            if self.reg.is_solid(self.get_block(x, y, z)) {
                return 2 * (y + 1);
            }
        }
        i32::MIN / 2
    }

    /// Update only a sub-voxel mask. Opacity is unchanged, so remeshing and
    /// edit logging are sufficient; no relight is required.
    fn set_mask_fast(&mut self, x: i32, y: i32, z: i32, id: BlockId, mask: u8) {
        let pos = ChunkPos::of_world(x, z);
        let lx = x.rem_euclid(CHUNK_X as i32) as usize;
        let lz = z.rem_euclid(CHUNK_Z as i32) as usize;
        if let Some(chunk) = self.chunks.get_mut(&pos) {
            chunk.set_meta(lx, y as usize, lz, mask);
            chunk.dirty = true;
            chunk.modified = true;
            if self.log_edits {
                self.edit_log.push((x, y, z, id, mask));
            }
        }
        let mut touch = |dx: i32, dz: i32| {
            if let Some(chunk) = self.chunks.get_mut(&ChunkPos {
                x: pos.x + dx,
                z: pos.z + dz,
            }) {
                chunk.dirty = true;
            }
        };
        if lx == 0 {
            touch(-1, 0);
        } else if lx == CHUNK_X - 1 {
            touch(1, 0);
        }
        if lz == 0 {
            touch(0, -1);
        } else if lz == CHUNK_Z - 1 {
            touch(0, 1);
        }
    }

    /// Place block and metadata without relighting or gameplay side effects.
    /// Bulk scene builders must relight the affected region afterward.
    pub fn set_block_quiet(&mut self, x: i32, y: i32, z: i32, block: BlockId, meta: u8) {
        if y < 0 || y >= CHUNK_Y as i32 {
            return;
        }
        let pos = ChunkPos::of_world(x, z);
        let lx = x.rem_euclid(CHUNK_X as i32) as usize;
        let lz = z.rem_euclid(CHUNK_Z as i32) as usize;
        if let Some(chunk) = self.chunks.get_mut(&pos) {
            chunk.set(lx, y as usize, lz, block);
            chunk.set_meta(lx, y as usize, lz, meta);
            chunk.dirty = true;
            chunk.modified = true;
        }
        let mut touch = |dx: i32, dz: i32| {
            if let Some(chunk) = self.chunks.get_mut(&ChunkPos {
                x: pos.x + dx,
                z: pos.z + dz,
            }) {
                chunk.dirty = true;
            }
        };
        if lx == 0 {
            touch(-1, 0);
        } else if lx == CHUNK_X - 1 {
            touch(1, 0);
        }
        if lz == 0 {
            touch(0, -1);
        } else if lz == CHUNK_Z - 1 {
            touch(0, 1);
        }
    }

    /// Move one octant from a higher half-column to a lower neighbor.
    fn flow_octant(
        &mut self,
        sand: BlockId,
        source: (i32, i32),
        target: (i32, i32),
        ref_y: i32,
        avoid: Option<([f32; 3], [f32; 3])>,
    ) -> bool {
        let top = self.sand_surface(sand, source.0, source.1, ref_y) - 1;
        if !self.sand_octant(sand, source.0, source.1, top) {
            return false;
        }
        let target_surface = self.sand_surface(sand, target.0, target.1, ref_y);
        let (tx, tqx) = (target.0.div_euclid(2), target.0.rem_euclid(2) as u32);
        let (tz, tqz) = (target.1.div_euclid(2), target.1.rem_euclid(2) as u32);
        let (ty, toy) = (
            target_surface.div_euclid(2),
            target_surface.rem_euclid(2) as u32,
        );
        if self.reg.is_water(self.get_block(tx, ty, tz)) {
            return false;
        }
        if let Some((low, high)) = avoid {
            let origin = [
                tx as f32 + tqx as f32 * 0.5,
                ty as f32 + toy as f32 * 0.5,
                tz as f32 + tqz as f32 * 0.5,
            ];
            let intersects = origin[0] < high[0]
                && origin[0] + 0.5 > low[0]
                && origin[1] < high[1]
                && origin[1] + 0.5 > low[1]
                && origin[2] < high[2]
                && origin[2] + 0.5 > low[2];
            if intersects {
                return false;
            }
        }

        let (sx, sqx) = (source.0.div_euclid(2), source.0.rem_euclid(2) as u32);
        let (sz, sqz) = (source.1.div_euclid(2), source.1.rem_euclid(2) as u32);
        let (sy, soy) = (top.div_euclid(2), top.rem_euclid(2) as u32);
        let source_bit = 1u8 << ((soy << 2) | (sqz << 1) | sqx);
        let source_mask = self.get_meta(sx, sy, sz) & !source_bit;
        if source_mask == 0 {
            self.set_block(sx, sy, sz, AIR);
        } else {
            self.set_mask_fast(sx, sy, sz, sand, source_mask);
        }

        let target_bit = 1u8 << ((toy << 2) | (tqz << 1) | tqx);
        if self.get_block(tx, ty, tz) == sand {
            let mask = self.get_meta(tx, ty, tz) | target_bit;
            self.set_mask_fast(tx, ty, tz, sand, mask);
        } else {
            self.set_block_meta(tx, ty, tz, sand, target_bit);
        }
        true
    }

    /// Relax half-columns toward the requested angle of repose.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn relax_sand(
        &mut self,
        sand: BlockId,
        center_x: i32,
        center_z: i32,
        ref_y: i32,
        radius: i32,
        repose: i32,
    ) -> bool {
        if self.remote {
            return false;
        }
        let mut moved = false;
        for half_x in (2 * (center_x - radius))..=(2 * (center_x + radius) + 1) {
            for half_z in (2 * (center_z - radius))..=(2 * (center_z + radius) + 1) {
                let surface = self.sand_surface(sand, half_x, half_z, ref_y);
                let mut best = surface;
                let mut target = None;
                for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let candidate = self.sand_surface(sand, half_x + dx, half_z + dz, ref_y);
                    if candidate < best {
                        best = candidate;
                        target = Some((half_x + dx, half_z + dz));
                    }
                }
                if let Some(target) = target
                    && surface - best > repose
                {
                    moved |= self.flow_octant(sand, (half_x, half_z), target, ref_y, None);
                }
            }
        }
        moved
    }

    /// Disturb only sand near cells recently touched by a grounded player.
    pub fn disturb_sand_touched(
        &mut self,
        sand: BlockId,
        feet: Vec3,
        touched: &HashMap<(i32, i32), f32>,
    ) -> bool {
        if self.remote {
            return false;
        }
        let feet_x = feet.x.floor() as i32;
        let feet_z = feet.z.floor() as i32;
        let ref_y = (feet.y - 0.05).floor() as i32;
        if self.get_block(feet_x, ref_y, feet_z) != sand {
            return false;
        }
        let avoid = (
            [feet.x - 0.3, feet.y, feet.z - 0.3],
            [feet.x + 0.3, feet.y + 1.8, feet.z + 0.3],
        );
        let foot_height = (feet.y * 2.0).round() as i32 + 2;
        const RADIUS: f32 = 2.0;
        let x_low = ((feet.x - RADIUS) * 2.0).floor() as i32;
        let x_high = ((feet.x + RADIUS) * 2.0).floor() as i32;
        let z_low = ((feet.z - RADIUS) * 2.0).floor() as i32;
        let z_high = ((feet.z + RADIUS) * 2.0).floor() as i32;
        let mut moved = false;
        for half_x in x_low..=x_high {
            for half_z in z_low..=z_high {
                let (cell_x, cell_z) = (half_x.div_euclid(2), half_z.div_euclid(2));
                let eligible = touched.contains_key(&(cell_x, cell_z))
                    || touched.contains_key(&(cell_x - 1, cell_z))
                    || touched.contains_key(&(cell_x + 1, cell_z))
                    || touched.contains_key(&(cell_x, cell_z - 1))
                    || touched.contains_key(&(cell_x, cell_z + 1));
                if !eligible {
                    continue;
                }
                let surface = self.sand_surface(sand, half_x, half_z, ref_y);
                if surface > foot_height {
                    continue;
                }
                let mut neighbors = [
                    (
                        (half_x - 1, half_z),
                        self.sand_surface(sand, half_x - 1, half_z, ref_y),
                    ),
                    (
                        (half_x + 1, half_z),
                        self.sand_surface(sand, half_x + 1, half_z, ref_y),
                    ),
                    (
                        (half_x, half_z - 1),
                        self.sand_surface(sand, half_x, half_z - 1, ref_y),
                    ),
                    (
                        (half_x, half_z + 1),
                        self.sand_surface(sand, half_x, half_z + 1, ref_y),
                    ),
                ];
                neighbors.sort_by_key(|&(_, height)| height);
                for (target, height) in neighbors {
                    if height >= surface {
                        break;
                    }
                    if self.flow_octant(sand, (half_x, half_z), target, ref_y, Some(avoid)) {
                        moved = true;
                        break;
                    }
                }
            }
        }
        moved
    }
}
