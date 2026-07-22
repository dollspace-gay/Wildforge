# The authored normal atlas — design plan

Drafted and **IMPLEMENTED** 2026-07-22. Third slice on the material-atlas
plumbing, and the one the previous two named as next: *"surface normals will get
their own standard tangent-space RGB atlas so a stock/downloaded/AI normal map
drops in with no channel surgery."* That is what this is.

The capability: **a texture pack can author how a surface catches light**, rather
than accepting what the engine infers from the albedo. Rock is the first
customer; the mechanism is per-texture and general.

## The normal atlas

A third texture beside the color and material atlases — same tile grid, same
side length, linear `Rgba8`, bound at `@group(1) @binding(3)`. It holds nothing
but tangent-space normals in the plain standard encoding, which is the whole
point: no packing, no borrowed channels, so an off-the-shelf map is a file copy.

Default fill is flat `(128, 128, 255)`, so an unauthored tile is a no-op.

### Which convention, and why the shader negates green

Encoding is **OpenGL / +Y ("green up")** — green brightens where the surface
tilts toward the *top* of the tile image. It is the more common of the two
conventions, so it is the one that makes "drop the file in" true most often.

Wildforge stores tiles top-row-first, so its bitangent runs *down* the image:
`v` increases as you descend. The shader therefore **negates green on decode**.
That single sign is the entire difference between the OpenGL and DirectX
conventions, and getting it backwards is the classic silent failure — nothing
crashes, nothing looks obviously broken, every ridge just lights from the wrong
side.

So it was verified rather than reasoned about. Synthesize a normal map that is
the *exact* OpenGL encoding of a tile's own height map, and render it against
the same tile with no normal map (which takes the engine's height-gradient
path). If the sign is right the two images must agree:

| render, over the reproducible region | mean \|diff\| | p99 | px > 8 |
|---|---|---|---|
| same pack twice (control) | 0.03 | 0.33 | 0.00% |
| **OpenGL-encoded normal vs height-derived** | **0.30** | 6.0 | 0.43% |
| green-flipped normal vs height-derived | 2.11 | 28.3 | 4.28% |

The OpenGL residual is 8-bit quantization of the encoded normal. The flipped
version is seven times further off. (The comparison is masked to the pixels two
runs of the same pack agree on — sky, clouds and distant chunk streaming are not
frame-deterministic, and at first swamped the signal entirely.)

## Where "is there a normal here?" lives

In the **material atlas's B channel**, not in the normal atlas's own alpha.

`parallax_surface` opens with a single material fetch and an early-out for plain
tiles — which is almost the entire world, so that fetch is the hot path. Putting
the flag in the material sample keeps the test at one texture read; a flag in
the normal atlas would mean sampling a second texture on every fragment just to
discover there was nothing to sample. B doubles as a **strength**, so partial
blends are free.

Material channels are now: R height, G interior mask, **B authored-normal
strength**, A still reserved.

## Selection, not blending

An authored normal **replaces** the height-derived one; it does not average with
it. Two reasons. It carries detail the height field never had — a chisel bevel
inside one flat-toned face — and blending would dilute exactly that. And an
author who ships a normal map has stated what they want; averaging it with our
inference silently overrides them.

Height-derived stays the free default for everything else, so nothing regresses.

## Companion maps

`scan_pack` now recognizes `<tile>_n.png` / `<tile>_normal.png` and
`<tile>_h.png` / `<tile>_height.png` beside `<tile>.png`. A real tile name
always wins the match, so a registry that genuinely contains a tile called
`foo_n` keeps it (tested).

Ordering matters: an albedo override still resets its material slot to flat (the
no-deface guarantee from the parallax slice), so companion maps are applied
**after** all albedo layering — otherwise a pack's own height map would be wiped
by its own albedo. Authored height also suppresses the luminance-derived height
for that slot, since guessing is strictly worse than being told.

## Resampling

Pack tiles finer than the atlas are now **box-averaged** instead of point
sampled. Downscaling a 128px tile into a 32px atlas kept 1 texel in 16; for an
albedo that is merely blurry, but for a normal map it is fatal — the discarded
neighbours are precisely what define the surface, and the surviving noise
sparkles under a moving light. Upscaling stays nearest: that chunk is the look.

## The reference texture

`tools/split_material_sheet.py` imports the albedo/height/normal contact sheet
an image model produces. Three things those sheets get wrong, all handled:

- **Panels don't land on clean fractions.** Seams are found from
  column-difference peaks. (On the reference sheet the true split is 342/683,
  not the 341/682 arithmetic predicts.)
- **A panel is usually a grid of separate tiles, not one texture.** The
  reference is 2x6 cells of ~171px. The grid origin is found by sliding the cut
  lines to where the image changes most — which for masonry is the mortar, and
  cutting *through* the mortar is what makes a cell tile against itself
  invisibly even though its opposite edges don't match numerically.
- **The normal map is not a unit field.** Models hallucinate normals rather than
  deriving them: on the reference, `|n|` averages 0.87 and 50 texels of
  `stone_n` face backwards outright. The importer renormalizes, folds z back to
  the front hemisphere, and *prints what it had to repair*.

Handedness is **reported, never acted on**. Correlating green against the height
gradient says which convention a map looks like, but on generated art that
correlation is weak (0.32 here) — inverting an axis on that evidence is how you
get a pipeline tuned to one image that quietly mangles the next. The engine's
contract is plain OpenGL/+Y; a map passes through untouched unless you pass
`--flip-green` yourself. (The reference is OpenGL, so it needed no flip.)

**Sanitation lives at import, not in the shader.** The engine assumes a
well-formed map: the only decode-time guard is against a degenerate zero-length
texel, which would normalize to NaN. An earlier draft clamped z to the front
hemisphere in the shader too — but that was the one line in the engine that
existed because of this specific image, it is dead code for anything the
importer has touched (min decoded z on the shipped tiles is 0.059 and 0.302,
both above the old 0.05 threshold), and a malformed map should *look* wrong
rather than be silently repaired on every fragment forever.

The height and normal panels are only loosely consistent with each other
(corr 0.47 / 0.32) — they were generated, not derived. That is fine here: each
is independently plausible, and parallax reads height while lighting reads the
normal.

`packs/hewn` is the result: `stone` and `cobblestone`, each with both companion
maps.

## Demo

```
WILDFORGE_PACK=hewn WILDFORGE_WORLD=rock WILDFORGE_DEMO_ROCK=1 \
  WILDFORGE_TILE_PX=128 WILDFORGE_TIME=0.30 cargo run --release
```

A stone court, a wall, and a pillar. The wall sits at −z and you look back at
it, because the sun always leans +z — a wall on the far side shows only its
shaded face and the relief never reads at all.

Against the same pack stripped of its companion maps (so stone falls back to
luminance-derived height), 42% of pixels differ by more than 8/255.

Note the honest scope of that number: for *this* texture the albedo luminance is
already a decent height proxy (corr 0.72), so authored data is an improvement
rather than a transformation. The larger win is that relief is no longer limited
to the two slots `derive_luminance_height` hardcodes — any tile in any pack can
now opt in.

## Deliberately deferred

- **A detail atlas at higher resolution than the albedo.** The normal atlas is
  px-matched to the color atlas, so seeing fine authored relief means running
  the whole atlas at `WILDFORGE_TILE_PX=128`. Decoupling them — chunky 32px
  color with crisp per-texel lighting — is the natural next slice, and needs a
  second sampler with linear filtering plus mip generation (nothing in the
  renderer has mips today).
- **normal → height** (Poisson integration), so a normal-only tile gets parallax
  for free. Still the one genuinely hard, approximate, global-solve quadrant.
- **Per-face and per-block tile variants.** The reference sheet yields 12
  distinct rock tiles and we use 2. Spending the rest — different cells on
  different cube faces, or hashed per block position — would kill the repeating
  texture look, but it is a mesher/registry feature, not a lighting one.

## Tests

- `pack_companion_maps_author_normals_and_height` — the flat default, where
  authored data lands, the material-B flag, authored height beating the derived
  one, untouched neighbours, and the real-tile-name precedence rule.
- `finer_pack_tiles_are_averaged_down_not_point_sampled` — a checkerboard whose
  mean and point samples differ maximally.
- `material_atlas_authors_ice_and_pack_override_clears_it` still green: the
  no-deface guarantee survives.
- WGSL still validates via `wgsl_shaders_validate`; full suite green.
