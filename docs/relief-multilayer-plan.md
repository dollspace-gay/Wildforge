# Relief lighting & multilayer translucent surfaces — design plan

Drafted and **IMPLEMENTED** 2026-07-20. Follow-on to the parallax slice
(single-layer POM via the material atlas). Adds two general capabilities on top
of that atlas — both opt-in per texture, both demoed on a block:

1. **Relief lighting** — the parallaxed surface now also *catches light*.
2. **Multilayer translucent interior** — a block can be see-through to an
   internal layer that parallaxes deeper than its surface.

Neither is ice-specific: the mechanisms live in the shader + material atlas, and
any block opts in with data. Ice and rock are just the first customers.

## Relief lighting

Parallax alone only shifted the albedo — the grooves *looked* displaced but the
lighting stayed flat. Now `parallax_surface` also reads the height gradient at
the displaced point (central differences on the material R channel) and returns
a **perturbed world normal**. `world_light` takes it as a second normal used for
the *directional* terms (sun + point-light N·L) while the stylized per-face
shade and ambient keep the flat geometric normal — so relief adds self-shading
without disturbing the base Minecraft look. Recesses darken, ridges catch the
sun/torch.

**Luminance height for free.** Authoring a height map for every rock type is
tedious, so `derive_luminance_height` builds one from a tile's own albedo
luminance (bright = raised, dark = recessed). Stone and cobblestone use it, so
cave walls and masonry read as 3D with zero authoring. It's derived from the
*final* atlas (after pack layering), so it automatically tracks whatever tile a
pack is showing — no clear-on-override needed.

Applies to leaves, bark, brick, or any block later — this is the general
"surfaces respond to light" layer, and the integration point (`world_light`'s
detail normal) is where authored normal maps will plug in next.

## Multilayer translucent interior

The material atlas's **G channel** is an *interior mask*: a second stratum that
sits below the surface and parallaxes deeper, composited as a **real translucent
veil** — `final = mix(interior, surface, VEIL)` — so the interior shows through
the surface *everywhere*, not as a stencil painted on top. Ice uses it: a smooth
surface (flat R) over a continuous cloudy lattice of frost fractures (G), which
slides beneath the surface as the eye moves — you look *into* the ice.

Getting the interior to read took three fixes, each from a real observation:
- **Per-tile clamp → wrap.** Clamping the deep sample to the tile made cracks
  *pop* at block edges ("something in front interrupting them"). Because the
  crack noise is periodic (`vnoise`/`fbm` use `rem_euclid`) and the atlas is
  nearest-filtered, we **wrap** the sample within the tile instead: it scrolls
  seamlessly and — since every block shares the [0,1] UV mapping — tiles into
  one continuous layer across block boundaries, no pop.
- **Stencil → translucency.** A sparse crack mask blended as an opaque tint read
  as marks *on* the surface. Making the interior a **continuous** field (cloud +
  veins, present everywhere via a G floor > 0) blended through a partial veil is
  what makes it read as depth *seen through* the surface.
- **Gated** so a block with no interior (G ≈ 0) is untouched.

## Material atlas channels (current)

| Channel | Meaning |
|---|---|
| R | parallax **height** (255 = surface, 0 = deepest); luminance-derived for rock |
| G | **interior mask** for the translucent layer (0 = none) |
| B / A | reserved (scalar surface data — roughness / AO / …) |

Surface **normals** deliberately do *not* live here — they'll get their own
standard tangent-space RGB atlas so a stock/downloaded/AI normal map drops in
with no channel surgery (derived-from-height is the free default; an authored
normal overrides it).

## Deliberately deferred

- **Authored normal atlas** (the drop-in convention above) — its own slice; the
  `world_light` hook is already there.
- **normal → height** (Poisson integration) so a normal-only tile gets parallax
  free — the one genuinely hard, approximate, global-solve quadrant.
- **De-tiling.** The interior repeats per block, exactly as every surface texture
  in a voxel game already does — not a regression, so no bespoke fix. The general
  answer, if ever wanted, is the standard voxel move: random per-block
  rotations/flips with tiles authored to work under any combination. Explicitly
  NOT world-space multi-octave hacks (that would be ice-specific gold-plating).

## Tests

`material_atlas_authors_ice_and_pack_override_clears_it` covers: plain tiles flat
with no interior; ice smooth-R with a continuous G interior; rock carrying
luminance relief in R; and a pack repaint flattening ice's material. WGSL still
validates via `wgsl_shaders_validate`; full suite green.
