//! RGB block light, skylight, and cross-chunk relight cascades.

use super::*;

impl World {
    /// (block-light intensity, sky light) at a world position. Unloaded chunks
    /// read as open sky so the world's edge doesn't render black.
    pub fn light_at(&self, x: i32, y: i32, z: i32) -> (u8, u8) {
        if y < 0 {
            return (0, 0);
        }
        if y >= CHUNK_Y as i32 {
            return (0, 15);
        }
        match self.chunks.get(&ChunkPos::of_world(x, z)) {
            Some(c) => c.light_intensity(
                x.rem_euclid(CHUNK_X as i32) as usize,
                y as usize,
                z.rem_euclid(CHUNK_Z as i32) as usize,
            ),
            None => (0, 15),
        }
    }

    /// (block-light r,g,b, sky light) at a world position — the full colored
    /// signal the mesher bakes into vertices.
    pub fn light_rgb_at(&self, x: i32, y: i32, z: i32) -> ([u8; 3], u8) {
        if y < 0 {
            return ([0; 3], 0);
        }
        if y >= CHUNK_Y as i32 {
            return ([0; 3], 15);
        }
        match self.chunks.get(&ChunkPos::of_world(x, z)) {
            Some(c) => c.light(
                x.rem_euclid(CHUNK_X as i32) as usize,
                y as usize,
                z.rem_euclid(CHUNK_Z as i32) as usize,
            ),
            None => ([0; 3], 15),
        }
    }

    /// Recompute both light channels for one chunk from scratch: sky column
    /// scan, then BFS from emitters and lit cells, seeded across chunk
    /// borders from loaded neighbors. Returns true if any value changed.
    pub(super) fn relight_chunk(&mut self, pos: ChunkPos) -> bool {
        const NX: usize = CHUNK_X;
        const NY: usize = CHUNK_Y;
        const NZ: usize = CHUNK_Z;
        let idx = |x: usize, y: usize, z: usize| (x * NZ + z) * NY + y;
        let reg = self.reg.clone();
        let Some(chunk) = self.chunks.get(&pos) else {
            return false;
        };

        // Per-cell properties, resolved once.
        #[derive(Clone, Copy)]
        struct Cell {
            opaque: bool,
            cost: u8,          // propagation cost: 1, or 2 through water
            emit: [u8; 3],     // per-channel emission
            filter: [bool; 3], // stained glass gates channels
        }
        let mut cells = vec![
            Cell {
                opaque: false,
                cost: 1,
                emit: [0; 3],
                filter: [true; 3],
            };
            NX * NY * NZ
        ];
        for x in 0..NX {
            for z in 0..NZ {
                for y in 0..NY {
                    let d = reg.block(chunk.get(x, y, z));
                    cells[idx(x, y, z)] = Cell {
                        opaque: d.opaque,
                        cost: if d.water_level.is_some() { 2 } else { 1 },
                        emit: d.light_rgb,
                        filter: d.light_filter,
                    };
                }
            }
        }

        let mut ls = vec![0u8; NX * NY * NZ];
        let mut sky_q: VecDeque<(usize, usize, usize)> = VecDeque::new();

        // Sky columns: full light straight down to the first opaque block,
        // dimming through water.
        for x in 0..NX {
            for z in 0..NZ {
                let mut v = 15u8;
                for y in (0..NY).rev() {
                    let c = cells[idx(x, y, z)];
                    if c.opaque {
                        break; // rest of the column stays 0
                    }
                    if c.cost > 1 {
                        v = v.saturating_sub(1);
                    }
                    ls[idx(x, y, z)] = v;
                    if v >= 2 {
                        sky_q.push_back((x, y, z));
                    }
                    if v == 0 {
                        break;
                    }
                }
            }
        }
        // Border seeds from loaded neighbors (light crosses chunk seams).
        // `chan` selects the channel: None = sky, Some(c) = block channel c.
        let seed = |grid: &mut [u8],
                    q: &mut VecDeque<(usize, usize, usize)>,
                    cells: &[Cell],
                    chan: Option<usize>| {
            for (dx, dz, edge_x, edge_z) in [
                (-1i32, 0i32, 0usize, usize::MAX),
                (1, 0, NX - 1, usize::MAX),
                (0, -1, usize::MAX, 0usize),
                (0, 1, usize::MAX, NZ - 1),
            ] {
                let npos = ChunkPos {
                    x: pos.x + dx,
                    z: pos.z + dz,
                };
                let Some(nc) = self.chunks.get(&npos) else {
                    continue;
                };
                // The neighbor's cell touching our edge cell.
                let (nb_x, nb_z) = (
                    if dx == -1 { NX - 1 } else { 0 },
                    if dz == -1 { NZ - 1 } else { 0 },
                );
                for t in 0..(if edge_x == usize::MAX { NX } else { NZ }) {
                    for y in 0..NY {
                        let (ox, oz, nx, nz) = if edge_x != usize::MAX {
                            (edge_x, t, nb_x, t)
                        } else {
                            (t, edge_z, t, nb_z)
                        };
                        let (nlb, nls) = nc.light(nx, y, nz);
                        let v = match chan {
                            None => nls,
                            Some(c) => nlb[c],
                        };
                        if v < 2 {
                            continue;
                        }
                        let c = cells[idx(ox, y, oz)];
                        if c.opaque || chan.is_some_and(|ch| !c.filter[ch]) {
                            continue;
                        }
                        let nv = v.saturating_sub(c.cost);
                        if nv > grid[idx(ox, y, oz)] {
                            grid[idx(ox, y, oz)] = nv;
                            q.push_back((ox, y, oz));
                        }
                    }
                }
            }
        };

        // BFS relax (single channel; run once per light channel).
        let bfs = |grid: &mut [u8],
                   q: &mut VecDeque<(usize, usize, usize)>,
                   cells: &[Cell],
                   chan: Option<usize>| {
            while let Some((x, y, z)) = q.pop_front() {
                let v = grid[idx(x, y, z)];
                if v < 2 {
                    continue;
                }
                let mut relax = |nx: usize, ny: usize, nz: usize| {
                    let c = cells[idx(nx, ny, nz)];
                    // Stained glass is opaque to the channels it blocks.
                    if c.opaque || chan.is_some_and(|ch| !c.filter[ch]) {
                        return;
                    }
                    let nv = v.saturating_sub(c.cost);
                    if nv > grid[idx(nx, ny, nz)] {
                        grid[idx(nx, ny, nz)] = nv;
                        q.push_back((nx, ny, nz));
                    }
                };
                if x > 0 {
                    relax(x - 1, y, z);
                }
                if x < NX - 1 {
                    relax(x + 1, y, z);
                }
                if y > 0 {
                    relax(x, y - 1, z);
                }
                if y < NY - 1 {
                    relax(x, y + 1, z);
                }
                if z > 0 {
                    relax(x, y, z - 1);
                }
                if z < NZ - 1 {
                    relax(x, y, z + 1);
                }
            }
        };

        // Sky: one channel.
        seed(&mut ls, &mut sky_q, &cells, None);
        bfs(&mut ls, &mut sky_q, &cells, None);

        // Block light: independent flood per color channel, packed into rgb.
        let mut lb = vec![[0u8; 3]; NX * NY * NZ];
        // `ch` selects a lane of per-cell arrays and parameterizes seed();
        // there is no slice to enumerate here.
        #[allow(clippy::needless_range_loop)]
        for ch in 0..3 {
            let mut grid = vec![0u8; NX * NY * NZ];
            let mut q: VecDeque<(usize, usize, usize)> = VecDeque::new();
            for i in 0..cells.len() {
                let e = cells[i].emit[ch];
                if e > 0 {
                    grid[i] = e;
                    let y = i % NY;
                    let xz = i / NY;
                    q.push_back((xz / NZ, y, xz % NZ));
                }
            }
            seed(&mut grid, &mut q, &cells, Some(ch));
            bfs(&mut grid, &mut q, &cells, Some(ch));
            for i in 0..grid.len() {
                lb[i][ch] = grid[i];
            }
        }

        let chunk = self.chunks.get_mut(&pos).unwrap();
        let (old_b, old_s) = chunk.light_raw();
        if old_b == lb.as_slice() && old_s == ls.as_slice() {
            return false;
        }
        let (dst_b, dst_s) = chunk.light_raw_mut();
        dst_b.copy_from_slice(&lb);
        dst_s.copy_from_slice(&ls);
        chunk.dirty = true;
        true
    }

    /// Relight a chunk and let changes ripple to loaded neighbors until the
    /// light field settles; every changed chunk is marked for remesh.
    ///
    /// Removing a bright light is the demanding case. `relight_chunk` rebuilds
    /// a chunk from scratch but seeds across seams from its neighbors' *stored*
    /// light — which, right after a removal, still holds the dead light's glow.
    /// So the just-cleared chunk gets re-seeded from that stale glow and the
    /// cascade has to peel it back one level at a time, each pass draining the
    /// seam by the propagation cost. A level-`L` light therefore needs up to
    /// `L` visits of a chunk to fully drain, so the cap must clear the 0..15
    /// light range — the old cap of 4 stranded a faint residual bar near the
    /// seam (visible as a lingering red patch when a torch was mined). Each
    /// pass is monotone — values only fall during a removal — so this
    /// converges; the cap is a safety net a couple of levels above the max.
    pub fn relight_and_cascade(&mut self, start: ChunkPos) {
        const MAX_VISITS: u32 = 18; // > the 15-level light range, with headroom
        let mut queue = VecDeque::from([start]);
        let mut visits: HashMap<(i32, i32), u32> = HashMap::new();
        while let Some(p) = queue.pop_front() {
            let v = visits.entry((p.x, p.z)).or_insert(0);
            if *v >= MAX_VISITS {
                continue; // safety cap; converges below this
            }
            *v += 1;
            if !self.chunks.contains_key(&p) {
                continue;
            }
            if self.relight_chunk(p) {
                for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let n = ChunkPos {
                        x: p.x + dx,
                        z: p.z + dz,
                    };
                    if self.chunks.contains_key(&n) {
                        queue.push_back(n);
                    }
                }
            }
        }
    }
}
