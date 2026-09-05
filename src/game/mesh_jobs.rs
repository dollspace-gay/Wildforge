//! CPU mesh snapshots and deduplication; GPU uploads remain in the game adapter.

use std::collections::HashSet;
use std::io;
use std::sync::Arc;

use crate::atlas::TileVariants;
use crate::background::SnapshotJobs;
use crate::chunk::ChunkPos;
use crate::mesher::{ChunkMesh, ChunkMeshInput, mesh_chunk_input};

type MeshSignature = Vec<(u16, Vec<u16>)>;
type MeshResult = (ChunkPos, ChunkMesh, MeshSignature);

struct MeshJob {
    input: ChunkMeshInput,
    variants: TileVariants,
    signature: MeshSignature,
}

pub(super) struct MeshPool {
    jobs: SnapshotJobs<MeshJob, MeshResult>,
    in_flight: HashSet<ChunkPos>,
    failure_reported: bool,
}

impl MeshPool {
    pub(super) fn new() -> io::Result<Self> {
        let workers = std::thread::available_parallelism()
            .map(|count| (count.get() / 4).clamp(1, 2))
            .unwrap_or(1);
        let jobs = SnapshotJobs::new("chunk-mesh", workers, 2, |job: MeshJob| {
            let position = job.input.position();
            let mesh = mesh_chunk_input(&job.input, &job.variants);
            (position, mesh, job.signature)
        })?;
        Ok(Self {
            jobs,
            in_flight: HashSet::new(),
            failure_reported: false,
        })
    }

    pub(super) fn request(
        &mut self,
        input: ChunkMeshInput,
        variants: TileVariants,
        signature: MeshSignature,
    ) -> bool {
        let position = input.position();
        if self.in_flight.contains(&position) {
            return false;
        }
        if !self.jobs.request(MeshJob {
            input,
            variants,
            signature,
        }) {
            return false;
        }
        self.in_flight.insert(position);
        true
    }

    pub(super) fn try_ready(&mut self) -> Option<MeshResult> {
        let result = self.jobs.try_ready()?;
        self.in_flight.remove(&result.0);
        Some(result)
    }

    pub(super) fn positions(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.in_flight.iter().copied()
    }

    pub(super) fn contains(&self, position: ChunkPos) -> bool {
        self.in_flight.contains(&position)
    }

    pub(super) fn pending_count(&self) -> usize {
        self.jobs.pending_count()
    }

    pub(super) fn take_failure_notification(&mut self) -> Option<Arc<io::Error>> {
        if self.failure_reported {
            return None;
        }
        let failure = self.jobs.failure()?;
        self.failure_reported = true;
        Some(failure)
    }
}

#[cfg(test)]
mod tests {
    use super::{ChunkMeshInput, MeshPool, TileVariants, mesh_chunk_input};
    use crate::chunk::ChunkPos;
    use crate::planet::Face;
    use crate::registry;
    use crate::world::World;
    use std::path::Path;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    #[test]
    fn real_mesh_jobs_preserve_buffers_and_deduplicate_the_same_chunk() {
        let reg = Arc::new(registry::load(Path::new("/nonexistent-mods-dir")));
        let root =
            std::env::temp_dir().join(format!("wildforge-mesh-owner-{}", std::process::id()));
        let mut world = World::new(42, root, reg);
        let position = ChunkPos::new(Face::PosZ, 256, 256).unwrap();
        assert!(world.ensure_chunk(position));
        let input = ChunkMeshInput::capture(&world, position).unwrap();
        let variants = TileVariants::default();
        let expected = mesh_chunk_input(&input, &variants);
        let mut pool = MeshPool::new().unwrap();
        assert!(pool.request(input, variants.clone(), variants.signature()));
        assert!(!pool.request(
            ChunkMeshInput::capture(&world, position).unwrap(),
            variants.clone(),
            variants.signature()
        ));
        assert_eq!(pool.pending_count(), 1);
        let deadline = Instant::now() + Duration::from_secs(10);
        let (at, actual, signature) = loop {
            if let Some(result) = pool.try_ready() {
                break result;
            }
            assert!(
                Instant::now() < deadline,
                "real mesh worker did not complete"
            );
            std::thread::yield_now();
        };
        assert_eq!(at, position);
        assert_eq!(signature, variants.signature());
        assert!(!actual.opaque_idx.is_empty());
        assert_eq!(actual.opaque_idx, expected.opaque_idx);
        assert_eq!(actual.water_idx, expected.water_idx);
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&actual.opaque_verts),
            bytemuck::cast_slice::<_, u8>(&expected.opaque_verts)
        );
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&actual.water_verts),
            bytemuck::cast_slice::<_, u8>(&expected.water_verts)
        );
        let emitters = |mesh: &crate::mesher::ChunkMesh| {
            mesh.emitters
                .iter()
                .map(|emitter| (emitter.pos, emitter.rgb, emitter.emit))
                .collect::<Vec<_>>()
        };
        assert_eq!(emitters(&actual), emitters(&expected));
        assert_eq!(pool.pending_count(), 0);
        assert!(!pool.contains(position));
    }
}
