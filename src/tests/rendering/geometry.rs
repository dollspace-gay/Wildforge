//! Geometry scenarios.

use super::*;

#[test]
fn curved_chunk_meshes_join_and_cull_across_a_cube_face() {
    use std::collections::HashSet;

    use crate::planet::{BlockPos, PLANET_RADIUS};

    let reg = base_reg();
    let stone = b(&reg, "base:stone");
    for (index, seam) in directed_planet_seams().into_iter().enumerate() {
        let mut world = World::new(
            47,
            tmp_dir(&format!("planet-mesh-seam-{index}")),
            reg.clone(),
        );
        let source =
            BlockPos::new(seam.source.face(), seam.source.u(), 100, seam.source.v()).unwrap();
        let across =
            BlockPos::new(seam.across.face(), seam.across.u(), 100, seam.across.v()).unwrap();
        world.insert_empty_chunks_for_test([source.chunk(), across.chunk()]);
        for chunk in [source.chunk(), across.chunk()] {
            let chunk = world.chunks_mut().get_mut(&chunk).unwrap();
            for x in 0..crate::chunk::CHUNK_X {
                for z in 0..crate::chunk::CHUNK_Z {
                    chunk.set(x, 0, z, crate::registry::AIR);
                }
            }
        }
        world.set_block_at(source, stone);
        world.set_block_at(across, stone);

        let a = crate::mesher::mesh_chunk(&world, source.chunk(), &Default::default());
        let bmesh = crate::mesher::mesh_chunk(&world, across.chunk(), &Default::default());
        assert_eq!(
            a.opaque_idx.len() + bmesh.opaque_idx.len(),
            10 * 6,
            "directed seam {index}: shared block sides are culled"
        );
        let positions = |mesh: &crate::mesher::ChunkMesh| {
            mesh.opaque_verts
                .iter()
                .map(|vertex| vertex.pos.map(f32::to_bits))
                .collect::<HashSet<_>>()
        };
        let a_positions = positions(&a);
        let b_positions = positions(&bmesh);
        assert!(
            a_positions.intersection(&b_positions).count() >= 4,
            "directed seam {index}: all four shared-face corners must be bit-identical"
        );
        for vertex in a.opaque_verts.iter().chain(&bmesh.opaque_verts) {
            let p = Vec3::from(vertex.pos);
            assert!(
                (PLANET_RADIUS as f32..=PLANET_RADIUS as f32 + 256.0).contains(&p.length()),
                "mesh vertex is embedded radially: {:?}",
                vertex.pos
            );
            let normal = Vec3::from(vertex.normal);
            assert!((normal.length() - 1.0).abs() < 1.0e-4);
        }
        for mesh in [&a, &bmesh] {
            for triangle in mesh.opaque_idx.chunks_exact(3) {
                let p0 = Vec3::from(mesh.opaque_verts[triangle[0] as usize].pos);
                let p1 = Vec3::from(mesh.opaque_verts[triangle[1] as usize].pos);
                let p2 = Vec3::from(mesh.opaque_verts[triangle[2] as usize].pos);
                let normal = Vec3::from(mesh.opaque_verts[triangle[0] as usize].normal);
                assert!(
                    (p1 - p0).cross(p2 - p0).dot(normal) > 0.0,
                    "directed seam {index}: rendered triangle winding must face its geometric normal"
                );
            }
        }
    }
}

#[test]
fn fluid_surfaces_stitch_at_shared_corners() {
    // Two adjacent water cells of different volumes must meet: the
    // thin cell's top corners on the shared edge rise to the full
    // cell's surface, so the water reads as one connected sheet
    // instead of disconnected tiles with gaps between their rims.
    let reg = base_reg();
    let mut w = test_world_with("stitch", reg.clone());
    let stone = b(&reg, "base:stone");
    let y = 200;
    for x in 0..8 {
        for z in 0..8 {
            w.set_block(x, y, z, stone);
        }
    }
    let full = reg.water_block(0); // volume 8, surface 8/9
    let thin = reg.water_block(5); // volume 3, surface 3/9
    w.set_block(2, y + 1, 2, full);
    w.set_block(3, y + 1, 2, thin);
    let mesh = crate::mesher::mesh_chunk(&w, tchunk(0, 0), &Default::default());
    let has = |x: i32, py: f32, z: i32| {
        let point = crate::planet::SurfacePoint {
            face: crate::planet::Face::PosZ,
            u: (x + crate::planet::FACE_BLOCKS as i32 / 2) as f64,
            v: (z + crate::planet::FACE_BLOCKS as i32 / 2) as f64,
        };
        let expected = crate::planet::block_to_render(point, py as f64).as_vec3();
        mesh.water_verts
            .iter()
            .any(|v| (Vec3::from_array(v.pos) - expected).length() < 1e-3)
    };
    let ys = (y + 1) as f32;
    // Shared edge corners sit at the full cell's height...
    assert!(has(3, ys + 8.0 / 9.0, 2), "near shared corner stitched");
    assert!(has(3, ys + 8.0 / 9.0, 3), "far shared corner stitched");
    // ...while the thin cell's outer edge keeps its own height.
    assert!(has(4, ys + 3.0 / 9.0, 2), "outer corner keeps thin height");
    // No thin-cell rim hangs at full height on the outer edge.
    assert!(
        !has(4, ys + 8.0 / 9.0, 2),
        "no floating rim on the thin side"
    );
}
