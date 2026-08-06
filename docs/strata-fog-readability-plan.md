# Strata and atmospheric readability

Drafted 2026-08-04. **IMPLEMENTED AND QUALIFIED 2026-08-05.**

## Implementation and qualification record

The accepted pass changed the eight deterministic base rock albedos and left
the atmosphere shader and fog range unchanged: native evidence proved the
existing directional-sky fade was already monotonic and exact at its endpoint.
The generator now uses stable SHA-256-derived seeds, produces byte-identical
periodic tiles, and gives sedimentary, igneous, and metamorphic families
different large-scale signals. Pack inheritance remains per material layer;
Hewn's sandstone and limestone overrides still win while its other rocks fall
back to base content.

The version-2 visual manifest records eight production-world sites, fourteen
baseline and fourteen after cases, and five matched performance captures per
phase. All frames were rendered at 1280×720 on an NVIDIA GeForce RTX 3090
through native Windows DX12 from clean executables. Raw frames remain ignored;
their hashes, capture identities, per-stratum measurements, and aggregate
results are tracked in `screenshots/visual-polish.toml` and
`screenshots/visual-polish/*.report.toml`.

All acceptance gates passed. Four-pixel/16-pixel pre-fog retention was
98.9%/61.7% for sandstone, 542.6%/274.7% for limestone, 106.7%/112.8% for
marble, and 103.4%/123.6% for quartzite. Clear-noon, clear-dawn, and
overcast-dawn silhouette magnitudes were 0.802, 0.721, and 0.732 against
minimums of 0.08/0.06/0.06. Basalt retained 91.36% of baseline 16-pixel shadow
detail with no additional display-black clipping. Fog-band contrast declined
to the directional-sky endpoint without a halo or exposed streaming wall.

The matched five-capture medians were 6.998 ms draw / 10.723 ms simulation for
the baseline and 6.805 ms / 10.467 ms after the change. This is a 0.193 ms draw
improvement and 0.256 ms simulation improvement; the largest after draw was
7.015 ms against a 14.212 ms single-frame ceiling. The aggregate reports and
Rust validator fail closed if their source, tool, commit, matrix, sidecar, or
metrics become stale.

The 2026-08-06 arc closeout re-ran this plan's after-matrix and performance
captures from one commit alongside the geode set and re-verified every gate on
the sites as they now exist; see `docs/visual-polish-closeout-report.md`.

The final motion check used the same native executable on a disposable copy of
the production save. Under both clear and overcast weather, injected ordinary
W/S key events moved the live player about nine blocks toward the sandstone
overhang and back to within roughly one block of the start. F2 renderer frames
showed the changed weather, readable rock, and stable streaming in motion; the
client remained responsive. The temporary save was deleted afterward and the
operator's original Gemini runtime configuration was restored.

This plan closes the visual defect recorded by the planetary biome pass:
pale strata can disappear into distance haze even when the generated column is
solid and continuous. It is a readability problem, not a license to change the
planet. Limestone, marble, quartzite, and sandstone must remain recognizable as
different rocks from arm's length through the useful part of the view, and the
last part of the horizon must still dissolve cleanly enough to hide the finite
streaming boundary.

The old reports describe "white fog," a seven-chunk view, and primitive
lighting. Those reports are valuable evidence, but they are no longer an exact
description of the renderer. The shipping default is twelve chunks; the WGSL
fog starts at 90% of its weather-adjusted range and blends terrain toward the
sky radiance in that view direction. Wildforge also has directional sun,
spherical-harmonic ambient light, point lights, relief maps, HDR, and bloom.
Implementation therefore begins with a current native-GPU baseline. It does
not begin by assuming that yesterday's cause survived yesterday's fixes.

## North star

A player looking across a limestone valley at dawn should read three things at
once: the valley is made of rock, the visible bands are geology rather than a
rendering seam, and the land continues into weather. Haze may take away detail
with distance; it must not make a solid pale cliff resemble unloaded sky.

The result should feel atmospheric rather than outlined. Near rock has texture
and grazing-light relief, middle-distance rock keeps its large bedding and
silhouette, and only the final tenth relinquishes itself to the sky. Dark rock
must not become a black cutout as the pale rocks improve.

## Scope and invariants

This pass owns:

- the procedural base albedo of `base:sandstone`, `base:limestone`,
  `base:shale`, `base:granite`, `base:marble`, `base:slate`,
  `base:quartzite`, and `base:basalt` in `tools/gen_base_tiles.py`;
- optional height and normal companions for those same natural rocks through
  the existing atlas material layers;
- the above-water atmosphere blend in `src/shader.wgsl` and the fog range
  supplied by `src/game/frame.rs`, if the baseline proves either is at fault;
- deterministic visual diagnostics and the tracked metadata for native-GPU
  captures.

The pass does **not** own or change:

- atlas geology, terrain height, cave frequency, strata placement, surface
  cover, biomes, water, chunk streaming radius, or the planet's dimensions;
- block ids, hardness, drops, recipes, save format, network protocol, or the
  finite-material ledger;
- the minimum or default view distance as a way to conceal bad art;
- emissive ordinary rock, cel-shaded outlines, a global contrast filter, or
  permanent debug coloration;
- a fix that exists only in Gemini. The geological tiles are base content;
  Gemini and Dusk currently inherit them when they do not override them.

No implementation may expose the loaded-world edge. Fog is the contract that
hides that edge, not an optional beauty effect.

## Establish the current failure

### Deterministic sites

Add a read-only ignored qualification test that searches the production atlas
for natural, exposed cuts containing each of the eight rock families. Prefer a
single multi-strata cliff for sedimentary rocks and separate intrusion,
metamorphic, and volcanic sites where necessary. A candidate is accepted only
when production worldgen, not a capture fixture, places the visible blocks.

For every selected site, write a stable record containing:

- world seed, generator and atlas versions;
- face and surface coordinates, camera height, yaw, and pitch;
- visible block families and their near/middle/far sample coordinates;
- time, weather, pack, view distance, output resolution, and executable commit;
- the actual fog start and end distances after weather gloom is applied.

Commit the records in `screenshots/visual-polish.toml`. PNG and PPM frames stay
ignored local evidence, as the existing planetary captures do. A selection
change is reviewable data, not an unrecorded camera nudge.

### Capture matrix

Capture the same geometry under this minimum matrix:

| Axis | Required cases |
|---|---|
| Material | limestone, marble, quartzite, sandstone; one dark control |
| Distance | near `< 0.35`, middle `0.55-0.75`, pre-fog `0.80-0.89`, fog `0.90-1.00` of effective range |
| Light | clear noon, dawn grazing light, overcast/rain |
| Pack | procedural base (`WILDFORGE_PACK=`), Gemini, Dusk |
| View | minimum 4 and default 12 chunks |

The minimum-distance case is a boundary-safety check, not the artistic target.
The default Gemini, default-distance dawn capture is the player-facing target.
Hewn is a supplementary override check: it already authors sandstone and
limestone albedo/height/normal tiles, so it must remain coherent after the base
change but cannot substitute for a base-content fix.

Use the shipping GPU renderer. CPU adapters are already rejected and WSLg is
not performance evidence. Qualification is a native Windows DX12 run from an
NTFS directory, with the game built from the commit named in the manifest.

### Diagnostic attachments

For qualification builds only, add a second capture attachment that writes a
block-family/depth mask using the same visible fragments as the color frame.
It may be a dedicated debug render target or a deterministic replay of the
resolved depth buffer. It must not infer rock identity from screenshot color.
This makes the measurement robust when the art changes and prevents a sky
pixel from being mistaken for pale limestone.

The debug attachment is never presented as game art and is excluded from
release pipelines when unused.

## Measure readability instead of voting on it

All calculations use linear-light relative luminance, not byte-space RGB.
`tools/verify_visual_polish.py` consumes the capture record, color frame, and
diagnostic attachment and emits a tracked TOML or JSON report.

For each rock and distance band it records:

- median luminance and the 10th/90th percentile span inside the rock mask;
- RMS local contrast at 1-, 4-, and 16-pixel scales;
- contrast between the rock silhouette and its immediately adjacent sky;
- visible-mask coverage, connected components, and one-pixel fringe count;
- the expected atmospheric blend factor at each sampled depth;
- a greyscale thumbnail score so hue alone cannot carry the distinction.

The first accepted baseline freezes numerical thresholds from real images.
The implementation must then meet these gates:

1. Before fog begins, every pale rock retains at least 70% of its near-view
   4-pixel local contrast and at least 50% of its near-view 16-pixel bedding
   contrast. The exact baseline values and their commit are recorded.
2. At 85% of the effective range, a pale-rock silhouette has a Weber contrast
   magnitude of at least 0.08 against adjacent sky in clear noon and at least
   0.06 in dawn and overcast cases.
3. Across the 90-100% fog band, contrast and color distance decline
   monotonically within a two-code-value tolerance; the fix creates no bright
   rim, dark halo, or late pop.
4. At the fog end, terrain is within the existing output tolerance of the
   directional sky target. No loaded-world wall becomes visible.
5. Each pale family remains mechanically distinguishable from at least two of
   the other pale families in near and middle bands by either 16-pixel
   structure or median chroma. Marble cannot become "brighter limestone" and
   quartzite cannot become "slightly greyer marble."
6. The dark control does not lose more than 10% of its baseline shadow detail
   and does not clip more than 1% additional masked pixels to display black.

These gates are intentionally about the useful field of view. The last fogged
pixels are allowed to become sky; that is their job.

## Implementation order

### 1. Fix the material signal first

The current `rock()` generator mainly uses high-frequency value noise and a
single horizontal sine for bedding. Pale palettes are close in both luminance
and structure. Give each family a geologically legible low-frequency identity:

- sandstone: warm, broad sediment packets with occasional iron-dark laminae;
- limestone: cooler, irregular bedding with restrained fossil/silt flecks;
- shale: thin, close cleavage bands and a low-gloss dark body;
- granite: coarse interlocking light/dark grains, no bedding;
- marble: flowing veins on a quieter crystalline field;
- slate: planar cleavage finer and more directional than shale;
- quartzite: hard granular sparkle and broken remnant bedding;
- basalt: fine, nearly uniform dark mass with sparse mineral specks.

Keep the tiles seamless and deterministic. Refactor the generator so its seed
is stable across Python processes; Python's randomized `hash()` must not decide
shipped pixels. A fixed digest of the tile name is the seed. Regeneration of a
named tile must produce byte-identical PNGs on two clean invocations.

Add companion height and derived normal tiles only when they make a measured
difference. They use the already-shipped material/normal atlases and the
existing `_h`/`_n` naming contract, so they require no new bind group, atlas
slot family, or draw call. Bedding relief stays shallow enough that a pale rock
does not look carved or wet. The albedo alone must still pass the middle-scale
test with relief disabled.

Extend `tools/audit_tiles.py` with an opaque-rock family instead of judging the
new tiles by eye alone. It checks seamless edges, luminance span, clipping,
deterministic dimensions/format, and excessive pairwise similarity. Contact
sheets include full color and greyscale.

Run and commit only intended geological outputs:

```sh
python3 tools/gen_base_tiles.py sandstone limestone shale granite marble slate quartzite basalt
python3 tools/audit_tiles.py --sheets /tmp/wildforge-strata-audit
```

### 2. Re-measure before touching fog

Repeat the matrix after the material pass. If the pre-fog and 85% gates pass
and the fade is monotonic, the shader is correct and remains unchanged. This
is the preferred result.

If the material signal passes near/middle checks but still fails only as the
fog blend approaches, change the atmospheric transfer—not the fog range.
Preserve these laws:

- transmittance is a smooth monotonic function of geodesic-plus-radial
  distance;
- the start and end remain 0.90 and 1.00 of the effective range unless a
  separate streaming proof permits otherwise;
- the far color is `sky_radiance(rd)` above water and the existing water fog
  below it;
- weather may shorten the range but cannot change topology or privilege a
  cube face;
- the endpoint is exact sky, without a contrast-preserving outline at 100%.

The acceptable shader adjustment is a physically plausible, bounded
transmittance curve or atmosphere-aware luminance treatment inside the fade.
A screen-space sharpen, depth edge outline, per-block-name branch, or sky-color
subtraction is rejected. Mirror the chosen scalar fog math in a small Rust
test so endpoint and monotonicity failures do not depend on a screenshot.

### 3. Validate pack inheritance

Build the atlas with no pack, Gemini, Dusk, and Hewn. For each, assert that all
eight base rocks resolve an albedo and that missing pack-specific strata tiles
fall back to the same base bytes. Hewn's existing sandstone and limestone
albedo/height/normal overrides must win deliberately; its other strata still
fall back. Companion maps inherit independently through the existing
material-layer contract. No pack warning, slot collision, or interior-layer id
may appear.

## Automated gates

Add focused tests with names that state the contract:

- `generated_strata_tiles_are_reproducible_and_seamless`;
- `pale_strata_keep_distinct_large_scale_signals`;
- `strata_companion_maps_survive_pack_inheritance`;
- `above_water_fog_is_monotonic_and_reaches_directional_sky`;
- `fog_distance_is_cube_face_invariant`;
- `visual_polish_manifest_is_complete`;
- `strata_capture_metrics_meet_readability_budget`.

The manifest test fails closed when a required field, case, attachment, report,
or commit id is missing. The metric test reads small committed reports, not
ignored screenshots, so CI remains deterministic and does not pretend its
software or virtual display is visual qualification.

The full repository gates remain:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
cargo deny check advisories
```

## GPU qualification and performance budget

The operator stages the chosen save under native NTFS and runs captures with
the manifest's exact values. A representative invocation is:

```powershell
$env:WILDFORGE_WORLD='visual-polish'
$env:WILDFORGE_SHOT='screenshots/strata-gemini-dawn.ppm'
$env:WILDFORGE_SHOT_SIZE='1920x1080' # capture-only override added by Goal 1
$env:WILDFORGE_PACK='gemini'
$env:WILDFORGE_VIEW_DIST='12'
$env:WILDFORGE_POS='<u>,<y>,<v>'
$env:WILDFORGE_LOOK='<yaw>,<pitch>'
$env:WILDFORGE_TIME='<manifest time>'
$env:WILDFORGE_WEATHER='<manifest weather>'
.\wildforge.exe
```

Use a real hardware adapter and record adapter/backend, resolution, frame,
resident/GPU chunk counts, FPS, simulation milliseconds, and draw milliseconds
from capture telemetry. Compare before and after at identical coordinates and
settings after both have settled.

Acceptance permits no new draw call or bind group and no new independently
allocated atlas. Median draw time may regress by at most 5% or 0.30 ms,
whichever is larger, over five settled captures; simulation time may not
regress by more than 0.10 ms. A single captured frame must not exceed twice the
baseline draw time. Tile changes should be free at runtime; any shader work
must earn its cost.

## Completion evidence

This plan is complete only when all of the following are in one reviewable
change:

- current before/after native-GPU captures for the full matrix;
- tracked `screenshots/visual-polish.toml` entries and metric reports;
- deterministic base tiles and any justified companion maps;
- passing focused and repository-wide tests;
- a performance comparison from the same hardware and scene;
- no visible streaming wall, halos, clipped dark strata, or pack warnings;
- closure notes in `docs/planetary-biomes-plan.md` and the older geology,
  hydrology, and water-cycle reports distinguishing the historical capture
  limitation from the current result.

The captures remain evidence, not a substitute for walking the site. Before
merge, use the native client to approach and leave the cliff on foot, change
the weather, and verify that the measured result also reads naturally in
motion.
