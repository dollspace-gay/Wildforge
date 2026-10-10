//! Shoreline behavior and exact water/salt custody fixtures.

use std::path::Path;
use std::sync::Arc;
use std::sync::OnceLock;

use crate::chunk::{CHUNK_X, CHUNK_Y, CHUNK_Z, Chunk};
use crate::planet::{BlockPos, ChunkPos, SurfacePos};
use crate::planet_atlas::{
    AtlasPos, HYDRO_UNITS_PER_BLOCK, PlanetAtlas, ReservoirMass, SurfaceReservoirKind,
    SurfaceReservoirState, surface_reservoir_id,
};
use crate::registry::{AIR, Registry};
use crate::world::{RevealKey, World};

struct ShoreFixture {
    world: World,
    chunk: ChunkPos,
    reservoir: u64,
}

impl ShoreFixture {
    fn new(at_face_edge: bool) -> Self {
        static ATLAS: OnceLock<Arc<PlanetAtlas>> = OnceLock::new();
        static REGISTRY: OnceLock<Arc<Registry>> = OnceLock::new();
        let atlas = ATLAS
            .get_or_init(|| Arc::new(PlanetAtlas::fixture(6_015, 8).unwrap()))
            .clone();
        let reg = REGISTRY
            .get_or_init(|| Arc::new(crate::registry::load(Path::new("/nonexistent-mods-dir"))))
            .clone();
        let atlas_pos = atlas
            .genesis
            .hydrology
            .values()
            .iter()
            .enumerate()
            .filter(|(_, hydro)| hydro.ocean_basin_id != 0)
            .map(|(index, _)| AtlasPos::from_index(index, atlas.side()).unwrap())
            .find(|pos| !at_face_edge || pos.u == 0)
            .expect("ocean fixture includes a face-edge cell");
        let center = atlas_pos.center(atlas.side());
        let surface = SurfacePos::new(
            center.face,
            if at_face_edge {
                0
            } else {
                center.u.floor() as u16
            },
            center.v.floor() as u16,
        )
        .unwrap();
        let chunk_pos = ChunkPos::from_surface(surface);
        let hydro = atlas.hydrology_sample(surface.center());
        let reservoir =
            surface_reservoir_id(SurfaceReservoirKind::Ocean, u32::from(hydro.ocean_basin_id));
        assert_ne!(hydro.ocean_basin_id, 0);
        let mut world = World::new_with_optional_atlas(
            6_015,
            std::env::temp_dir().join("wildforge-shore-read-only-fixture"),
            reg.clone(),
            Some(atlas.clone()),
            false,
        );
        let mut chunk = Chunk::new();
        let stone = reg.block_id("base:stone").unwrap();
        for lx in 0..CHUNK_X {
            for lz in 0..CHUNK_Z {
                let here = SurfacePos::new(
                    chunk_pos.face(),
                    chunk_pos.u() * CHUNK_X as u16 + lx as u16,
                    chunk_pos.v() * CHUNK_Z as u16 + lz as u16,
                )
                .unwrap();
                assert_eq!(
                    atlas.hydrology_sample(here.center()).ocean_basin_id,
                    hydro.ocean_basin_id,
                    "fixture's loaded columns share one ocean"
                );
                chunk.set(lx, 10, lz, stone);
            }
        }
        world.chunks.insert(chunk_pos, chunk);
        let mut fixture = Self {
            world,
            chunk: chunk_pos,
            reservoir,
        };
        fixture.set_level(14);
        fixture
    }

    fn at(&self, lx: usize, y: usize, lz: usize) -> BlockPos {
        BlockPos::new(
            self.chunk.face(),
            self.chunk.u() * CHUNK_X as u16 + lx as u16,
            y as u8,
            self.chunk.v() * CHUNK_Z as u16 + lz as u16,
        )
        .unwrap()
    }

    fn set_level(&mut self, height: i32) {
        self.world
            .weather_state
            .live_mut()
            .unwrap()
            .water
            .reservoir_mut(self.reservoir)
            .unwrap()
            .level_milliblocks = height * 1_000;
    }

    fn reservoir(&self) -> SurfaceReservoirState {
        self.world
            .weather_state
            .live()
            .unwrap()
            .water
            .reservoirs
            .iter()
            .find(|reservoir| reservoir.id == self.reservoir)
            .unwrap()
            .clone()
    }

    fn reconcile(&mut self) {
        self.world
            .edit_batch(|world| world.reconcile_loaded_shores());
    }

    fn seed_water(&mut self, top: usize) {
        for lx in 0..CHUNK_X {
            for lz in 0..CHUNK_Z {
                for y in 11..=top {
                    let parcel = self
                        .world
                        .weather_state
                        .live_mut()
                        .unwrap()
                        .materialize_surface_water(self.reservoir, HYDRO_UNITS_PER_BLOCK);
                    assert_eq!(parcel.water_hu, HYDRO_UNITS_PER_BLOCK);
                    let water = self.world.reg.water_for_volume(8);
                    let chunk = self.world.chunks.get_mut(&self.chunk).unwrap();
                    chunk.set(lx, y, lz, water);
                    chunk.set_meta(lx, y, lz, parcel.salinity());
                    chunk.set_water_salt(lx, y, lz, u16::try_from(parcel.salt_mass).unwrap());
                }
            }
        }
    }

    fn visible_mass(&self) -> ReservoirMass {
        let mut mass = ReservoirMass::default();
        for lx in 0..CHUNK_X {
            for lz in 0..CHUNK_Z {
                for y in 0..CHUNK_Y {
                    let at = self.at(lx, y, lz);
                    if let Some(parcel) = self.world.water_mass_at(at) {
                        assert_eq!(self.world.get_meta_at(at), parcel.salinity());
                        mass.water_hu += parcel.water_hu;
                        mass.salt_mass += parcel.salt_mass;
                    }
                }
            }
        }
        mass
    }

    fn assert_water_heights(&self, heights: &[usize]) {
        let stone = self.world.reg.block_id("base:stone").unwrap();
        for lx in 0..CHUNK_X {
            for lz in 0..CHUNK_Z {
                for y in 0..CHUNK_Y {
                    let block = self.world.get_block_at(self.at(lx, y, lz));
                    if heights.contains(&y) {
                        assert!(
                            self.world.reg.is_water(block),
                            "expected water at {lx},{y},{lz}"
                        );
                    } else {
                        assert_eq!(block, if y == 10 { stone } else { AIR });
                    }
                }
            }
        }
    }
}

fn assert_custody(before: &SurfaceReservoirState, after: &SurfaceReservoirState) {
    assert_eq!(
        before.coarse.water_hu + before.committed.water_hu,
        after.coarse.water_hu + after.committed.water_hu
    );
    assert_eq!(
        before.coarse.salt_mass + before.committed.salt_mass,
        after.coarse.salt_mass + after.committed.salt_mass
    );
}

#[test]
fn rising_shore_moves_exact_water_and_salt_up_two_blocks_per_column() {
    let mut fixture = ShoreFixture::new(false);
    let before = fixture.reservoir();
    fixture.reconcile();
    fixture.assert_water_heights(&[11, 12]);
    let visible = fixture.visible_mass();
    let after = fixture.reservoir();
    assert_eq!(
        visible.water_hu,
        (CHUNK_X * CHUNK_Z * 2) as u64 * HYDRO_UNITS_PER_BLOCK
    );
    assert!(visible.salt_mass > 0);
    assert_eq!(
        before.coarse.water_hu - after.coarse.water_hu,
        visible.water_hu
    );
    assert_eq!(
        before.coarse.salt_mass - after.coarse.salt_mass,
        visible.salt_mass
    );
    assert_eq!(
        after.committed.water_hu - before.committed.water_hu,
        visible.water_hu
    );
    assert_eq!(
        after.committed.salt_mass - before.committed.salt_mass,
        visible.salt_mass
    );
    assert_custody(&before, &after);
}

#[test]
fn falling_shore_returns_exact_water_and_salt_and_preserves_higher_water() {
    let mut fixture = ShoreFixture::new(false);
    fixture.seed_water(14);
    fixture.set_level(10);
    let before = fixture.reservoir();
    let visible_before = fixture.visible_mass();
    fixture.reconcile();
    fixture.assert_water_heights(&[13, 14]);
    let visible_after = fixture.visible_mass();
    let after = fixture.reservoir();
    let returned_water = visible_before.water_hu - visible_after.water_hu;
    let returned_salt = visible_before.salt_mass - visible_after.salt_mass;
    assert_eq!(
        returned_water,
        (CHUNK_X * CHUNK_Z * 2) as u64 * HYDRO_UNITS_PER_BLOCK
    );
    assert_eq!(
        after.coarse.water_hu - before.coarse.water_hu,
        returned_water
    );
    assert_eq!(
        after.coarse.salt_mass - before.coarse.salt_mass,
        returned_salt
    );
    assert_eq!(
        before.committed.water_hu - after.committed.water_hu,
        returned_water
    );
    assert_eq!(
        before.committed.salt_mass - after.committed.salt_mass,
        returned_salt
    );
    assert_custody(&before, &after);
}

#[test]
fn no_water_fallback_ignores_hidden_solidity() {
    let mut fixture = ShoreFixture::new(false);
    let hidden = fixture.at(0, 13, 0);
    let stone = fixture.world.reg.block_id("base:stone").unwrap();
    fixture
        .world
        .chunks
        .get_mut(&fixture.chunk)
        .unwrap()
        .set(0, 13, 0, stone);
    fixture.world.hide_at(
        hidden,
        RevealKey {
            settlement: 0,
            tier: 2,
        },
    );
    let before = fixture.reservoir();
    fixture.reconcile();
    for y in [11, 12] {
        assert!(
            fixture
                .world
                .reg
                .is_water(fixture.world.get_block_at(fixture.at(0, y, 0)))
        );
    }
    assert_eq!(fixture.world.get_block_at(hidden), stone);
    assert!(fixture.world.is_hidden(hidden));
    assert_eq!(fixture.world.get_block_at(fixture.at(0, 14, 0)), AIR);
    assert_custody(&before, &fixture.reservoir());
}

#[test]
fn player_touched_chunk_is_excluded_from_shore_reconciliation() {
    let mut fixture = ShoreFixture::new(false);
    fixture.world.player_touched.insert(fixture.chunk);
    let before = fixture.reservoir();
    fixture.reconcile();
    fixture.assert_water_heights(&[]);
    assert_eq!(fixture.visible_mass(), ReservoirMass::default());
    assert_eq!(fixture.reservoir(), before);
    assert!(fixture.world.water_queue.is_empty());
}

#[test]
fn ocean_column_at_face_edge_uses_its_canonical_resident_chunk() {
    let mut fixture = ShoreFixture::new(true);
    assert_eq!(fixture.chunk.u(), 0);
    assert_eq!(fixture.at(0, 11, 0).u(), 0);
    let before = fixture.reservoir();
    fixture.reconcile();
    fixture.assert_water_heights(&[11, 12]);
    let visible = fixture.visible_mass();
    let after = fixture.reservoir();
    assert_eq!(
        before.coarse.water_hu - after.coarse.water_hu,
        visible.water_hu
    );
    assert_eq!(
        before.coarse.salt_mass - after.coarse.salt_mass,
        visible.salt_mass
    );
    assert_custody(&before, &after);
}
