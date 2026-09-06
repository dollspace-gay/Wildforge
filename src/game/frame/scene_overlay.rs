//! Scene overlay in the graphical frame pipeline.

use super::Geometry;
use crate::atlas;
use crate::entity;
use crate::game::Game;
use crate::mesher;
use crate::world::TerrainRead;

impl Game {
    pub(in crate::game) fn prepare_scene_overlay(&self) -> Geometry {
        let mut overlay_verts = Vec::new();
        let mut overlay_idx = Vec::new();
        if let Some((target, progress)) = self.interaction.breaking {
            let world_pos = match target {
                crate::game::BreakTarget::World(p) => Some(p),
                crate::game::BreakTarget::Structure(id, offset) => self
                    .runtime
                    .view()
                    .local_structure(id)
                    .and_then(|s| s.world_position(offset)),
            };
            if let Some(p) = world_pos {
                entity::emit_crack(p, progress, &mut overlay_verts, &mut overlay_idx);
            }
        }
        // The quern's top face turns while you grind (bare-hand station
        // channels only; hammer stations flash sparks instead).
        if self.presentation.juice
            && self.input.right_held
            && let Some(t) = self.interaction.anvil_pos
            && !self.inventory.slots[self.input.hotbar_sel]
                .is_some_and(|st| self.content.reg.item(st.item).hammer)
        {
            let b = self.runtime.view().get_block_at(t);
            let slot = self.content.reg.block(b).tiles[2];
            let ts = 1.0 / atlas::ATLAS_TILES as f32;
            let (tx, ty) = (
                slot as u32 % atlas::ATLAS_TILES,
                slot as u32 / atlas::ATLAS_TILES,
            );
            let ang = self.interaction.anvil_work * std::f32::consts::PI;
            let (sa, ca) = ang.sin_cos();
            let center = crate::planet::EntityPos::new(
                t.face(),
                f32::from(t.u()) + 0.5,
                f32::from(t.y()) + 1.01,
                f32::from(t.v()) + 0.5,
            )
            .expect("station overlay is inside the shell");
            let c = center.render_pos();
            let local = crate::planet::local_frame(center.surface_point());
            let east = local.east.as_vec3();
            let north = local.north.as_vec3();
            let base = overlay_verts.len() as u32;
            for (lx, lz, u, v) in [
                (-0.5f32, -0.5f32, 0.0f32, 0.0f32),
                (0.5, -0.5, 1.0, 0.0),
                (0.5, 0.5, 1.0, 1.0),
                (-0.5, 0.5, 0.0, 1.0),
            ] {
                let rx = lx * ca - lz * sa;
                let rz = lx * sa + lz * ca;
                overlay_verts.push(mesher::Vertex {
                    pos: (c + east * rx + north * rz).to_array(),
                    uv: [(tx as f32 + u) * ts, (ty as f32 + v) * ts],
                    normal: [0.0, 0.0, 0.0],
                    light: [1.0; 3],
                    sky: 1.0,
                    ao: 1.0,
                });
            }
            overlay_idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        Geometry {
            vertices: overlay_verts,
            indices: overlay_idx,
        }
    }
}
