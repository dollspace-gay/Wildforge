# Rebuilding visual qualification

The active evidence directory is selected by
`screenshots/current-campaign.toml`. Historical campaigns remain intact. The
Rust visual tests validate the selected campaign against the current source,
including newly added modules, shipped content, and verification tools.

## Inputs and prerequisites

Use a clean source checkout and two release executables from recorded clean
commits: a baseline and the candidate. The candidate commit must remain in the
ancestry of the commit that adds its evidence. Commit the source before building;
do not rewrite capture identities after rebasing or changing code.

Provide an isolated seed-20260802 production world with the expected atlas and
content configuration. The driver locates the natural geode, clones its sealed
source, and independently prepares both opened fixtures through the game's real
save and material-accounting operations. Every capture receives a separate copy
of an immutable fixture. User saves are not campaign inputs by default.

Still captures require an X11 session and a discrete Vulkan GPU. Motion also
requires `Xvfb`, `xdotool`, and `libX11`; its isolated display prevents desktop
shortcuts from intercepting screenshot keys. The native game must still identify
the hardware Vulkan adapter. Software rendering is rejected.

## Execute and review

Set the variables below to the prepared inputs and a new, absolute, ignored
working directory. `baseline_revision` is the full commit recorded in the
baseline executable, and `campaign_date` uses `YYYY-MM-DD`.

```sh
python3 tools/run_visual_campaign.py --work-dir "$campaign_work" prepare \
  --source-world "$source_world" \
  --baseline "$baseline_binary" --baseline-revision "$baseline_revision" \
  --candidate "$candidate_binary" --date "$campaign_date"
python3 tools/run_visual_campaign.py --work-dir "$campaign_work" motion
python3 tools/run_visual_campaign.py --work-dir "$campaign_work" capture
```

The matrix requires 92 native captures: 52 visual scenes and 40 performance
samples. Baseline and candidate scenes use matched cameras and render settings.
Performance samples run last, in paired repetitions. Keep builds and other
heavy work out of their measurement interval. Strata timing reports retain the
game's recorded draw and simulation observations. Geode performance uses the
versioned rendering-work protocol below; legacy draw observations remain in
those reports as presentation-pacing diagnostics.

The six marble view-distance-12/14 dawn stills use a declared 6000-frame warmup.
The preceding campaign's extended baseline view12 capture and view14 captures
settled successfully at that minimum. The first replacement run omitted the
view12 exception and failed at 3000 eligible frames with 32 dirty chunks and zero
settled frames. Its complete failed attempt remains preserved. Apply the same
still-only warmup to baseline, candidate and closeout before the fresh replacement
run, without changing settlement criteria or any performance sample's
1200-frame minimum.

## Geode rendering-work protocol v1

Geode performance qualification schema 2 measures rendering work separately
from presentation pacing. `telemetry.draw_ms` keeps its original meaning: an
exponential moving average of host time around the entire renderer, including
swapchain acquisition and presentation. It is neither a GPU timestamp nor an
isolated measure of CPU rendering work. Do not reinterpret older reports.

This migration follows the preserved 2026-10-09 failed qualification and
original-build controls. On the same hardware, the original build also exceeded
the 0.20 ms wall-time budget. Four prescribed original/candidate diagnostic
captures measured opened-minus-sealed GPU command medians of 0.191152 ms and
0.176320 ms, respectively; their renderer wall deltas were 0.640256 ms and
1.7896135 ms. These diagnostics established that wall time does not substitute
for GPU command cost. They did not qualify either build or separate CPU work
from presentation waits. Their raw receipts and the failed campaigns remain in
`target/lag-control-20261009/`.

The new protocol is fixed before the replacement campaign:

- Enable `WILDFORGE_RENDER_WORK=1` and hardware GPU timestamps for every geode
  timing case. Keep cameras, settings, five repetitions per phase, the 1200-frame
  minimum, normal presentation mode, and native settlement checks unchanged.
- Retain the latest 120 consecutive, fully CPU/GPU-completed, settled frames.
  Join each host sample to its exact renderer frame ID; reject screenshot frames,
  gaps, invalid timings, errors, and readback-slot skips within that window. A
  reset discards the interrupted window. The asynchronous tail is bounded by
  the four readback slots. Anchor the latest renderer ID to the enclosing next
  capture frame and verify at least 1200 eligible frames. Do not select an older,
  more favorable window.
- Measure host render wall time from before frame uploads through presentation.
  Independently time swapchain acquisition and presentation. CPU submission
  time is wall time minus those two spans; it includes uploads, command encoding,
  queue submission, and profiling bookkeeping. GPU command timestamps cover
  shadow, world, postprocessing, composite, and UI commands. They exclude
  pre-encoding uploads, query resolve/readback, and presentation waits; host
  upload cost remains covered by the CPU measurement.
- Serialize all 120 joined sample tuples, their medians, actual timestamp period
  and capabilities, frame boundaries, and coverage counters in the native
  sidecar. Python and Rust independently check coverage and recompute medians.
  Missing hardware timestamps or an incomplete window cannot qualify.
  Timing cameras and render settings match the group's sealed-context capture,
  except for the declared 1280x720 timing dimensions. Controlled day/time/season/
  weather/precipitation match that capture; full environment identity matches
  across all ten timing cases, without anchoring evolving weather scalars to
  the differently warmed primary still.
- Across the five captures per phase, compute CPU and GPU median deltas. The
  rendering budget remains **0.20 ms total**:
  `max(0, CPU delta) + max(0, GPU delta) <= 0.20 ms`. An improvement in one
  component cannot hide increased work in the other. The simulation budget
  remains **0.10 ms**. Legacy draw deltas are diagnostic fields in schema 2.

The replacement must be a complete fresh 92-capture and 56-motion-frame campaign
from frozen clean builds. A measurement-only baseline migration must preserve
the original gameplay implementation. Changing this protocol or relevant source
after freezing invalidates the campaign; do not alter an existing receipt.

Each motion walk records 28 F2 frames through native WASD input. Inspect every
frame in order, verify the recorded hashes, and fill `motion-review.json` with
the reviewer, actual review method, and observations. Record shimmer, unloaded
chunk faces, and awkward reveals honestly. Static-hold luminance analysis
supplements this review. Sampled steps do not establish continuous-video or
unbounded-travel behavior.

```sh
python3 tools/run_visual_campaign.py --work-dir "$campaign_work" qualify
python3 tools/run_visual_campaign.py --work-dir "$campaign_work" publish
cargo test --locked visual_capture::tests::
```

Qualification regenerates and checks the per-capture reports, applies the
existing readability, composition, repeatability, performance, and motion gates,
and rechecks source and fixture inventories. Publication requires a complete,
accepted campaign and refuses to overwrite an existing evidence directory.
Select a new destination in `screenshots/current-campaign.toml` before publishing.

The work directory retains frozen binaries, worlds, raw frames, environment and
input records, and failed attempts. Only small provenance and qualification
records are published. `capture` resumes already verified captures; `capture
--only ID` and `qualify --only ID` support diagnosis. A changed source fingerprint
requires a new clean build and campaign. Finish the repository's full test and
build gates after publication, then verify CI on the published commit.
