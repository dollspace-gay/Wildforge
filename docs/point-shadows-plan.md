# Sharp per-light shadows — design plan

Follow-up to the shaders/lighting stack (directional sun, RGB flood-fill).
Goal: **line-of-sight** point lights so a torch around a corner leaves you in
darkness (down to the ambient floor) but blares on the wall you face — and so
colored point lights cast *hard* colored shadows (red shadow / blue shadow /
purple mix, with real edges), instead of the flood-fill's soft wrap.

## Why the flood-fill can't do this

Block light floods *around* occluders (a level-14 torch wraps ~14 blocks
around any corner). There is no tuning that yields "dark just past the
doorway" — that needs real occlusion. Shadow maps give it, and as a bonus
they're geometry-agnostic: once sub-voxel meshes exist, a fan blade rendered
into a light's shadow pass rakes its silhouette across the wall for free.

## Shading model

Per shadow-casting light, the **direct** term is:

    color · atten(d) · max(N·L, 0) · shadow          shadow ∈ {0,1}

- `atten`: range-based, punchy — `saturate(1 - d/range)²` (blares up close,
  falls to zero at `range`; unlike the flood-fill's flat −1/block).
- `shadow`: 0/1 from an omnidirectional cube shadow map (below).
- Lights sum, so overlaps mix in color and shadows compose per channel.

Total = `ambient + soft-fill (flood-fill, indirect) + Σ direct(light) + sun`.
Occluded direct → 0 → the surface falls to **ambient**, which is a knob:
stark (`~0.03`) to accessible (whatever dollspace prefers). Exposed via
config; `WILDFORGE_AMBIENT=r,g,b` overrides it for tests.

Keeping the flood-fill as a *soft fill* under the hard direct means shadows
aren't cartoon-black unless ambient is set that way — real light bounces.
Lights promoted to shadow-casters are excluded from the flood-fill so they
aren't counted twice.

## Cube shadow maps

- Each point light → a **distance cube map**: 6 perspective (90° FOV) depth
  renders; the fragment writes `length(worldPos − lightPos)` to an R32Float
  cube face. Sampling in the main shader: `dist = cube(dir, light)`, then
  `shadow = (|dir| − bias) ≤ dist ? 1 : 0`. Distance-in-world avoids
  per-face depth reconstruction.
- Stored as an **R32Float cube array** (`6·N` layers), sampled nearest
  (hard voxel shadows don't need PCF; can add later).
- Resolution 512²/face to start — voxel silhouettes are coarse.

## Budget & caching (what makes it affordable)

- Only the **N nearest/brightest** lights (start N=4–8) cast shadows; the
  rest keep the flood-fill.
- Shadow casters are **range-culled**: a light only re-renders chunks within
  its range.
- **Static-light caching** is the key: a wall torch never moves, so render
  its cube map once and reuse it; invalidate only when a block within its
  range changes (`set_block` already marks dirty regions). Dynamic lights (a
  carried torch, an entity glow) re-render per frame. This turns "a room full
  of torches" from N·6 renders/frame into near-zero for a static scene.

## Milestones

1. **Shading + point-light plumbing** — lights uniform, range attenuation,
   colored accumulation, ambient knob. `shadow = 1` stub. Prove the stark
   colored-pool look. *(this milestone)*
2. **Cube shadow maps** — 6-face distance renders + cube-array sampling +
   hard shadow gating. Prove "blares/black around the corner" and hard
   colored shadows.
3. **Caching + N-nearest culling** — static cube-map cache with block-edit
   invalidation; range-cull casters; promote/demote lights by proximity.
4. **Integration** — exclude shadow-cast lights from the flood-fill (no
   double count); wire real torches (not just demo lights); expose ambient +
   caster-count in config.

## Non-goals (future)

Screen-space GI using the flood-fill as a diffusion stand-in; bloom (wants
HDR + normal/displacement maps first); PCF/soft point shadows.
