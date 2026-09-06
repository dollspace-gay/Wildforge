//! Outline stage of GPU frame encoding.

use crate::renderer::{LineVertex, Renderer};

impl Renderer {
    pub(in crate::renderer) fn upload_outline(
        &self,
        outline: Option<crate::planet::BlockPos>,
        outline_color: [f32; 3],
    ) {
        if let Some(block) = outline {
            let e = 0.003f32;
            let c = outline_color;
            let p = |du: f64, dy: f64, dv: f64| {
                let surface = crate::planet::SurfacePoint {
                    face: block.face(),
                    u: f64::from(block.u()) + du,
                    v: f64::from(block.v()) + dv,
                };
                let mut pos =
                    crate::planet::block_to_render(surface, f64::from(block.y()) + dy).as_vec3();
                pos += pos.normalize_or_zero() * e;
                LineVertex {
                    pos: pos.to_array(),
                    color: c,
                }
            };
            let verts = [
                // bottom
                p(0.0, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(1.0, 0.0, 1.0),
                p(1.0, 0.0, 1.0),
                p(0.0, 0.0, 1.0),
                p(0.0, 0.0, 1.0),
                p(0.0, 0.0, 0.0),
                // top
                p(0.0, 1.0, 0.0),
                p(1.0, 1.0, 0.0),
                p(1.0, 1.0, 0.0),
                p(1.0, 1.0, 1.0),
                p(1.0, 1.0, 1.0),
                p(0.0, 1.0, 1.0),
                p(0.0, 1.0, 1.0),
                p(0.0, 1.0, 0.0),
                // pillars
                p(0.0, 0.0, 0.0),
                p(0.0, 1.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(1.0, 1.0, 0.0),
                p(1.0, 0.0, 1.0),
                p(1.0, 1.0, 1.0),
                p(0.0, 0.0, 1.0),
                p(0.0, 1.0, 1.0),
            ];
            self.queue
                .write_buffer(&self.outline_buf, 0, bytemuck::cast_slice(&verts));
        }
    }
}
