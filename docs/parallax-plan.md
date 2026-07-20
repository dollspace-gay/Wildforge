# Parallax mapping & the material atlas — design plan

Drafted and **IMPLEMENTED** 2026-07-20. The capability is **parallax occlusion
mapping**: per-texel surface relief that shifts with the viewpoint, so recessed
detail reads as real depth in motion. Ice is the first use case (its cracks
become grooves), but the mechanism is general — leaves, bark, brick, or any mod
tile can opt in.

## The material atlas (the load-bearing piece)

Parallax needs per-texel depth, which the color atlas doesn't carry. So a
**second atlas** rides alongside it: same tile layout, linear `Rgba8` (never
sampled as sRGB), bound at `@group(1) @binding(2)`.

Channel reservations — only R is consumed today; the rest are deliberately left
flat so later features slot in without another atlas or binding:

| Channel | Meaning | Status |
|---|---|---|
| **R** | parallax **height** (1.0 = surface, 0.0 = deepest) | used now |
| **G** | tangent-space **normal.x** (0.5 = flat) | reserved (normal mapping) |
| **B** | tangent-space **normal.y** (0.5 = flat) | reserved (normal mapping) |
| **A** | **displacement / roughness / AO** — TBD | reserved |

The default fill is `R=255` (flat), so **a tile with no material data is a
parallax no-op**: the opt-in is *per texture*. Built-in tiles author their
material procedurally in `atlas.rs` (currently ice); pack authors will later
drop a companion map to give their own tiles relief.

**Pack-aware, so we never deface hand-drawn art:** any slot a pack (or a mod
tile) repaints has its material reset to flat in `build_atlas`. Our procedural
ice grooves only apply where the *procedural* ice albedo is also showing — never
under a mismatched pack tile. (A test enforces this.)

## The shader (single-layer POM)

`parallax_uv` in `shader.wgsl`, called at the top of `fs_chunk`:

1. **Tangent frame per fragment** from the screen-space derivatives of world
   position and uv (the cotangent-frame trick) — no per-vertex tangents, and it
   just works for our axis-aligned faces.
2. **Early-out** when the sampled height is ~1.0 (flat), so non-parallax tiles —
   i.e. almost the whole world — pay only one texture read.
3. **Steep march** of the tangent-space view ray through the height field,
   returning the uv where it first dips below the surface. The march is
   **clamped to the tile's own atlas cell** so it can never bleed into a
   neighbour tile's texels.

Units matter: the offset is a **displacement depth in blocks** (`PARALLAX_DEPTH`,
0.08) scaled into a single tile's uv span, and the grazing-angle term is clamped
— otherwise the offset blows up to several tiles wide at oblique views and slams
into the tile clamp, washing the texture out. (That was the first bug; the diff
of same-viewpoint on/off now traces cleanly along the crack veins only.)

## Demo

`WILDFORGE_DEMO_ICE` lays a flat ice rink + a kerb and stands you at a grazing
angle. Run on the procedural pack (`WILDFORGE_PACK=`), whose `@ice` albedo
matches the procedural height:

```
WILDFORGE_PACK= WILDFORGE_WORLD=demo WILDFORGE_DEMO_ICE=1 \
  WILDFORGE_LOOK="1.5708,-0.45" WILDFORGE_SHOT=ice.ppm cargo run
```

## What this deliberately does NOT do (yet)

- **No multi-layer / volumetric depth.** Single-layer POM gives surface relief;
  the dramatic "bubbles and cracks at different depths sliding past each other"
  ice look needs stacked depth layers. That's a follow-on that reuses this
  atlas + tangent plumbing.
- **No normal mapping.** The G/B channels are reserved for it but left flat.
- **No self-shadowing / silhouette (POM+).** Straight steep-parallax for now.
- **No pack companion-map loading yet.** The atlas is structured for it; built-in
  procedural tiles are the only authored source in this slice.

## Tests

- `material_atlas_authors_ice_and_pack_override_clears_it`: ice carries recessed
  height, plain tiles are flat, and a pack that repaints ice flattens its
  material (the no-deface guarantee).
- The full suite stays green — the only sim-facing change is the `build_atlas`
  return tuple (now yields the material atlas too).
