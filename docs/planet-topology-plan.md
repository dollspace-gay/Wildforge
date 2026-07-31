# Planet topology — one finite world, closed under travel

> **Status: implemented and qualified (2026-07-31).**
>
> This is goal 1 of the planetary-world sequence in
> `docs/planetary-world-sequence.md`. It replaces the infinite planar world
> with one finite cube-sphere. Complete this document before beginning any
> later planetary goal.

## Purpose

Wildforge's world is currently an unbounded `(x, y, z)` lattice. That is the
wrong physical foundation for a game about a society sharing one land:
damage can always be abandoned, rare material can always be found by walking
farther, maps can never be completed, and nobody can circumnavigate home.

The replacement is one finite, closed planet:

- Six finite square surface charts form a cube-sphere.
- Traveling off one face enters its neighbor; the world has no edge.
- The surface visibly curves and local up points away from the planet.
- Blocks remain an ordinary local voxel lattice for simulation and building.
- Chunks remain `16 × 16 × 256`; they are generated lazily.
- The complete address space is finite even though most chunks begin
  unmaterialized.

This goal changes topology, coordinates, persistence, physics, rendering,
network positions, and every neighbor walk. It deliberately does not replace
terrain, climate, rivers, or biomes yet. The existing generator may produce
temporary planetary terrain until later goals replace its inputs.

## Fixed decisions

### Planet dimensions

The first and only supported size is:

```text
FACE_BLOCKS  = 8192
FACE_CHUNKS  = 512
CHUNK_X/Z    = 16
CHUNK_Y      = 256
SURFACE_FACES = 6
```

An equatorial circuit crosses four faces: **32,768 surface blocks**. At the
current 4.4-block/s walk speed that is a little over two uninterrupted hours;
mature transport can reduce it without making the planet feel like a toy.

The nominal render radius is:

```text
PLANET_RADIUS = FACE_BLOCKS / (π / 2) ≈ 5215.19 blocks
```

One face therefore spans ninety degrees and one cell near a face center is
approximately one world unit along the surface. Cube-sphere distortion is
bounded and measured; logical block distance remains the simulation truth.

The logical world contains exactly:

```text
6 × 8192 × 8192 × 256 = 103,079,215,104 block cells
```

Most are air or ungenerated. This number is a limit, not an allocation.

### One topology, one simulation path

There is no permanent flat-world compatibility mode. Nobody is playing a
durable public world yet, and two topologies would double every physics,
fluid, lighting, AI, persistence, scripting, and networking path.

- Bump the world and chunk formats.
- Refuse old flat saves with a clear message.
- Never silently reinterpret flat coordinates as planetary coordinates.
- Development saves may be deleted after the format bump.
- Back up an old save before refusing it if an automated migration command
  touches the save directory.

### Locally orthogonal, globally curved

Simulation cells remain boxes in a face-local `(u, y, v)` chart. Rendering
maps their corners to a curved shell. A block is therefore an imperceptibly
tapered radial cell in embedded space, not a perfect Euclidean cube.

Collision, construction, machines, and voxel fluids use logical coordinates.
This is intentional. At a radius above five thousand blocks, the difference
across a player's body is far below a pixel and is not worth curved-space
collision geometry.

## Coordinate model

Introduce canonical coordinate types. Raw tuples and ambiguous `Vec3`
positions must not cross subsystem boundaries after this goal.

```rust
#[repr(u8)]
enum Face {
    PosX,
    NegX,
    PosY,
    NegY,
    PosZ,
    NegZ,
}

struct SurfacePos {
    face: Face,
    u: u16, // 0..FACE_BLOCKS
    v: u16, // 0..FACE_BLOCKS
}

struct BlockPos {
    surface: SurfacePos,
    y: u8, // 0..CHUNK_Y
}

struct ChunkPos {
    face: Face,
    u: u16, // 0..FACE_CHUNKS
    v: u16, // 0..FACE_CHUNKS
}

struct EntityPos {
    face: Face,
    u: f32,
    y: f32,
    v: f32,
}
```

Exact Rust layout may differ, but the semantics and bounded ranges do not.
Constructors validate ranges. Deserialization rejects invalid faces,
coordinates, and heights.

`EntityPos` is canonicalized whenever it crosses a face edge. Velocity,
facing, camera yaw, and any directional state rotate through the same edge
transform.

### Face bases

One table defines, for every face:

- outward normal,
- increasing-`u` tangent,
- increasing-`v` tangent,
- the adjacent face reached at each of four edges,
- coordinate reversal or exchange on that edge,
- rotation between the two local tangent frames.

There must not be scattered face-specific match statements. Topology bugs are
most dangerous when fluids, light, movement, and networking disagree.

### Required topology operations

One module owns:

```text
canonicalize(EntityPos) -> EntityPos + frame rotation
step4(SurfacePos, direction) -> SurfacePos + rotated direction
step6(BlockPos, direction) -> BlockPos + rotated direction
neighbors4(SurfacePos)
neighbors8(SurfacePos)
neighbors6(BlockPos)
surface_to_unit(SurfacePoint) -> normalized Vec3
block_to_render(BlockPoint) -> embedded Vec3
local_frame(SurfacePoint) -> east/up/north basis
geodesic_distance(a, b)
great_circle_bearing(a, b)
```

Every world system uses these operations. Direct `x ± 1` or `z ± 1` is
allowed only inside a function that has proved it cannot reach a face edge.

### Corners

A quadrilateral sphere necessarily has eight extraordinary vertices where
three face grids meet. They are valid topology, not missing terrain.

- Cell centers remain unambiguous.
- An exact diagonal move through a corner resolves as two ordered edge
  crossings using a documented canonical order.
- Four- and six-neighbor simulation never needs a diagonal corner shortcut.
- Meshing shares the canonical embedded corner position, preventing cracks.
- Tests exercise all eight corners in every entering direction.

No invisible wall, ocean mask, or lore object is required to conceal them.
If later art makes them landmarks, that is content rather than a topology
repair.

## Curved embedding

Map a face-local point into a cube direction, apply a low-distortion
spherified-cube mapping, and normalize. The same pure function is used by
terrain diagnostics, rendering, sun position, climate, and tests.

For a block vertex:

```text
direction = cube_sphere(face, u, v)
radius    = PLANET_RADIUS + y
position  = direction × radius
```

The implementation must:

- produce bit-stable results for generation queries,
- produce identical positions for either side of a face seam,
- keep adjacent top-face vertices shared,
- preserve face winding,
- expose distortion metrics for atlas diagnostics.

Generation never hashes embedded floating-point coordinates. It hashes the
canonical integer surface address or samples stable continuous fields from a
documented direction vector.

## Chunks and world access

A chunk remains one full vertical column, `16 × 16 × 256`.

- A chunk key gains `face`.
- Local indexing stays unchanged.
- Chunk halos fetch through topology-aware neighbors.
- Region files group `32 × 32` chunks on one face.
- Region paths include the face name or stable face number.
- No region crosses a face seam.

All block access becomes `get_block(BlockPos)`, `set_block(BlockPos, ...)`,
and typed equivalents. Temporary compatibility wrappers may exist within a
single porting commit, but completion requires ordinary implementation code
to stop passing raw `(i32, i32, i32)` world tuples.

The following systems must be ported before this goal is complete:

- chunks and region storage,
- block entities and edit logs,
- finite water and lava queues,
- lighting and relighting walks,
- fire,
- falling blocks and item drops,
- machine and multiblock neighbor checks,
- power shafts,
- ecology and mob positions,
- hearts, regional ire, touched ledgers, and bloom,
- structures and world features,
- raycasts and block interaction,
- agent perception and actions,
- server events and snapshots,
- waystones, signs, stalls, player spawn, and bedroll spawn.

### Temporary planetary terrain

Goal 1 must remain playable before the scientific atlas exists, but the old
planar generator cannot simply run once per face: that would stamp six square
worlds together with climate and terrain seams.

Use a deliberately temporary seam-safe generator:

- Sample low-frequency 3D noise at `unit_direction × frequency` for broad
  land/ocean height.
- Sample additional 3D fields for erosion, ridge, temperature, and moisture
  placeholders.
- Reuse the existing fine 3D density, cave, surface, feature, structure, and
  content passes after replacing their large-scale inputs.
- Hash canonical `BlockPos` for discrete details.
- Ensure every progression-critical base item remains obtainable for
  development.
- Mark the implementation and tests `temporary_planet_gen`; goal 3 removes
  its land/geology inputs and goal 7 removes its biome inputs.

This temporary path is allowed only between planetary goals. The final
qualification goal searches for and removes it. A face-local planar adapter
with visible seams is not an acceptable intermediate.

## Physics and camera

### Gravity

Simulation gravity is always negative local `y`. The camera's rendered up
vector is the local radial direction. Walking across ordinary chunks changes
the embedded up vector smoothly; crossing a cube-face edge also canonicalizes
the logical frame.

### Collision

Retain axis-resolved local AABB collision in `(u, y, v)`:

- Existing step-up, sweep, swimming, and fall-damage semantics remain.
- Movement may cross a face edge during a sweep; canonicalize at every hop.
- Collision queries crossing an edge rotate into the neighboring chart.
- Falling below `y = 0` is impossible because bedrock seals the shell.
- The old planar void-respawn rule is removed.

### Look and movement

Yaw is relative to the local tangent frame; pitch is relative to local
horizontal. At an edge crossing, preserve the embedded look direction and
derive the new local yaw. The player must not experience a ninety-degree
camera snap.

Creative flight uses local tangent and radial axes. It may rise above
`CHUNK_Y` only up to a bounded presentation ceiling and may not address
blocks outside the radial voxel range.

### Raycasting

Replace global Cartesian DDA with topology-aware local DDA:

- Traverse logical voxel cells.
- Canonicalize and rotate the step state at face edges.
- Retain reach and server validation.
- Pin seam and corner hits with deterministic tests.

## Rendering

Chunk meshes emit curved embedded positions or enough canonical logical data
for the vertex stage to do so. The selected design is CPU-side embedded
positions for the first implementation because:

- seam equality is directly testable,
- existing shadows and entity rendering consume world positions,
- one transform path serves block and non-block geometry.

Use a camera-relative floating origin at the player's embedded position.
Even this small planet should not feed five-thousand-unit absolute values
through every depth calculation when zero is available.

Required renderer changes:

- curved block vertices and correct normals,
- seam-safe chunk halos and face culling,
- local radial sky/sun basis,
- camera-relative entity and particle transforms,
- curved water surfaces,
- block targeting outline,
- point and sun shadows,
- fog following geodesic rather than planar distance,
- horizon culling that never draws through the planet,
- remote player and held-item transforms across faces.

The player standing at sea level should see a close horizon. From a mountain,
the visible surface distance should increase according to sphere geometry.

## Networking and persistence

All authoritative wire positions gain face plus bounded local coordinates.

- Protocol version bumps once for all planetary position changes.
- Guests reject invalid or noncanonical positions.
- Interpolation never lerps directly between different face charts; convert
  snapshots into the receiver's local embedded frame first.
- Interest management uses geodesic/chunk-neighbor distance.
- A player near a seam receives chunks and entities from both faces.

World metadata stores:

```text
topology = "cube_sphere_v1"
face_blocks = 8192
world_height = 256
planet_radius = derived versioned constant
generator_version
```

Player positions, bedroll spawns, waystone attunements, signs, heart records,
mob saves, block entities, ledgers, and audit events all use canonical
planetary positions.

## Bearings, distance, and maps

Anything presented to a player uses planetary geometry:

- Waystones and cairns report great-circle bearing and surface distance.
- Directions remain readable local octants rather than raw face coordinates.
- The shortest route may cross a face seam.
- Antipodal bearings may be ambiguous; report distance and say the bearing is
  uncertain rather than choosing an unstable direction.
- Debug screens may show face/`u`/`v`; ordinary UI does not.

## Script and mod API break

This is a major API version:

- Replace ambiguous `x, y, z` world arguments with a planetary position
  value or `(face, u, y, v)`.
- Add topology-aware neighbor, distance, and bearing calls.
- Reject mods targeting the previous world API with a precise version error.
- Update the shipped gems mod and executable modding guide.
- Do not emulate infinite signed planar coordinates.

## Performance constraints

- Topology lookup and four-neighbor steps are table-driven and allocation-free.
- Block access away from seams must remain as cheap as the current path after
  inlining.
- Embedded transforms occur at mesh construction, not per simulation tick.
- Interest walks operate over chunk topology, never over all 1.5 million
  possible chunks.
- Existing generation and meshing budgets remain bounded.

## Required tests

### Topology

- Every face edge is reciprocal for every coordinate along the edge.
- Four steps around an elementary loop return to the same cell and direction.
- All eight corners canonicalize deterministically.
- No valid step leaves the finite address space.
- A four-face equatorial walk of 32,768 steps returns to its origin.
- Great-circle distance is symmetric and zero only for identical positions.
- Antipodal handling is stable.

### Geometry

- Both faces produce bit-close embedded seam vertices.
- Adjacent chunks have crack-free shared vertices.
- Surface normals point outward.
- A one-block logical step has bounded physical distortion everywhere.
- Horizon distance increases with camera altitude.

### Simulation

- Player walking, sprinting, swimming, boating, and flying crosses every seam.
- Collision, stepping, falling blocks, fluids, fire, light, and power cross
  every seam.
- A multiblock straddling a seam validates through rotated directions.
- A mob pursues a player across a seam.
- Raycasts select blocks on the neighboring face.

### Persistence and multiplayer

- Every planetary position round-trips through all save and wire formats.
- Region files on different faces cannot collide by path or key.
- A host and guest cross a seam together without a snap or missing chunks.
- Interest streaming includes the neighboring face.
- Invalid coordinates are rejected before allocation or world access.

### Visual qualification

Capture:

- sea-level horizon,
- the same view from a mountain,
- a building spanning a face seam,
- water crossing a seam,
- three players straddling a seam,
- one of the eight extraordinary corners.

No visible crack, camera roll, lighting discontinuity, or shadow discontinuity
passes review.

## Completion criteria

This goal is complete only when:

- the game creates and saves only finite cube-sphere worlds,
- all authoritative positions use the planetary coordinate model,
- all listed simulation systems cross seams correctly,
- a player can circumnavigate and return to the exact starting cell,
- multiplayer streaming and interpolation work across seams,
- the renderer presents continuous curvature and a real horizon,
- old flat saves are rejected clearly rather than corrupted,
- the modding guide and shipped mod use the new API,
- all existing tests are ported or intentionally replaced,
- all new topology, seam, persistence, multiplayer, and visual gates pass.

Do not mark this document implemented because a curved terrain demo renders.
The topology is not real until all world mutations and authoritative actors
are closed under it.
