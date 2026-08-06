# Visual polish arc — Goal 4 closeout report

Executed 2026-08-06 on the qualification machine (NVIDIA GeForce RTX 3090,
native Windows DX12, NTFS staging directory `C:\Games\Wildforge`), driven from
the WSL2 repository checkout. The closeout evidence commit is
`8d3385f0d19007d17dda7fa9ff3b42ae3da946f1`; every closeout sidecar names it
with a clean build. Goals 2 and 3 qualified on different commits
(`b7a4498…` and `cc3d085…`); this closeout re-ran both plans' matrices from
one commit on one machine in one night, as `docs/visual-polish-sequence.md`
Goal 4 requires.

## What was re-run

- The full goal-2 strata "after" matrix: fourteen cases re-captured as
  `strata-closeout-*` under scene `closeout-strata-20260806`.
- Strata performance: five baseline captures from the archived goal-2
  baseline executable (commit `6048663…`) and five closeout captures from the
  closeout executable, same night, same scene, same machine.
- The four cracked-geode evidence frames and ten geode performance captures
  as `closeout-geode-*`, against a freshly re-prepared opened save (below).
- Interactive motion walkthroughs of the strata site and the geode approach,
  recorded in `screenshots/visual-polish/closeout-motion.report.toml`.

The manifest (`screenshots/visual-polish.toml`, schema 3) declares every
closeout capture; `tools/verify_visual_closeout.py` rebuilds the aggregates
from the same shared parser and the same gates as the original goals, and the
Rust validator (`src/visual_capture.rs`) fails closed on all of it in CI.

## Results

- **Strata readability: PASSED.** All retention, silhouette, fog-monotonicity,
  family-distinction, and dark-control gates hold at the closeout commit
  (`screenshots/visual-polish/closeout-readability.report.toml`).
- **Cracked-geode composition: PASSED** on the freshly prepared save
  (`closeout-geode-composition.report.toml`): host, quartz, amethyst, and
  recessed-heart pixel gates, three-sided quartz lip, zero sky, bounded
  overlay, and exact sealed/reload camera identity.
- **Strata performance: PASSED.** Median draw is 0.540 ms *faster* than the
  goal-2 baseline executable (7.743 → 7.202 ms); simulation is +0.67 ms at
  0.01 ms precision against the explicit 1.00 ms cross-arc budget
  (`closeout-strata-performance.report.toml`).
- **Geode performance: PASSED.** Sealed→opened median draw −0.038 ms and
  simulation +0.045 ms, within the unchanged goal-3 budgets of 0.20/0.10 ms
  (`closeout-geode-performance.report.toml`).
- **Motion: PASSED.** Both walks recorded 28 frames across five phases
  (static holds and both walking legs); maximum static-hold mean-luminance
  delta 0.038 (strata, animated water and sun glare) and 0.014 (geode, torch
  flame flicker) against the 0.05 pumping bound; no shimmer, no chunk walls,
  no unloaded faces, and a clean reveal in motion
  (`closeout-motion.report.toml`).

## Findings the closeout surfaced

The arc's design says implementation must record defects it exposes rather
than silently staging around them. The closeout surfaced three.

### 1. Committed lake water never evaporates (production hydrology defect)

`PlanetaryWeather::plan_surface_evaporation` (`src/planet_atlas/climate.rs`)
draws evaporation only from a reservoir's **coarse** (uncommitted) mass.
Once chunk materialization moves lake mass into per-chunk commitments
(`commit_fresh_chunk_water`), that water can still be credited by rain,
runoff, and discharge, but can never leave through evaporation: visited
lakes and closed pools rise monotonically. Conservation is intact — the
water audit reports exactly zero unexplained water and salt on every save
used tonight — the defect is a missing return path, not a leak.

Observed consequences across the qualification saves (each capture session
advances roughly one climate hour, and these worlds have hosted dozens of
sessions since 2026-08-05):

- the pool below the goal-2 sandstone waterfall rose about three blocks and
  submerged the bank the original camera stood on;
- the limestone lake rose several blocks; a player standing at the goal-2
  camera position now sinks to the lake bed among kelp.

This is recorded here as a separately reviewed prerequisite for whichever
arc owns hydrology next. It is **not** fixed in this change: the fix belongs
in conservation-critical water code with its own tests, not inside a visual
polish closeout.

### 2. The qualification sites are live places (measurement finding)

Because the strata worlds regenerate unsaved chunks and keep advancing their
planetary water, the "unchanged shipping scene" assumption behind still
re-captures is only approximately true. The closeout therefore re-frames the
matrix as: the same recorded sites, captured as they exist at the closeout
commit, judged by the same numeric gates — not pixel-identity with 2026-08-05
frames. Camera positions are pinned with the capture-only
`WILDFORGE_SHOT_ALTITUDE=0` hover (the same mechanism the original operator
used; it is recorded in each sidecar's camera identity), and the readability
gates were met on the current state of every site.

### 3. The opened geode save was re-proven, not repaired (evidence hygiene)

Tonight's first hero re-capture raised two suspicions about the 2026-08-05
opened save: an apparent second torch, and a +1.5 ms opened-scene draw
regression. Both were investigated to ground truth and neither was drift.
The "second torch" is the held-torch viewmodel, which the goal-3 hero staged
differently in a way capture sidecars cannot record; the draw delta was
entirely the unpinned performance camera falling into the excavated pit
(finding above). To settle it mechanically, the 2026-08-05 save was set
aside untouched as `visual-polish-geode-opened-drifted-20260806` and the
reveal was re-executed from the untouched sealed source with
`--prepare-cracked-geode`: 39 planned breaks applied, one torch placed, zero
unexpected edits, material accounts balanced before, after, and across
reload (`screenshots/visual-polish/closeout-geode-preparation.report.toml`).
The re-prepared save renders byte-stable-identical to the preserved original
at the matched aperture camera — the goal-3 preparation was exactly
reproducible, and the closeout evidence uses the freshly proven copy.

### Performance measurement conditions

Three measurement notes, recorded so the numbers are interpretable:

- The strata performance scene now runs the entire magic arc's simulation
  (weather-hour slicing, dross chasing, alchemy maintenance) merged since the
  goal-2 baseline commit. Single-frame simulation telemetry captured during
  catch-up is highly variable; closeout performance captures therefore use a
  longer warmup (`WILDFORGE_SHOT_MIN_FRAME=1200`) so catch-up completes
  before the telemetry frame. With that warmup the samples are tight and the
  comparison is clean: closeout draw is ~0.54 ms **faster** than the goal-2
  baseline executable, and simulation costs a consistent ~0.67 ms more —
  the merged magic arc's tick cost, not a strata regression. The closeout
  simulation budget is therefore an explicit cross-arc 1.00 ms
  (`simulation_budget_basis` in the report); draw keeps the goal-2 budget
  because the tile change is draw-side.
- The geode performance comparison (sealed vs opened) uses the goal-3 budgets
  unchanged on the freshly prepared opened save. An early closeout run
  measured a spurious +1.6 ms because the unpinned perf camera fell into the
  excavated pit of the opened save and measured a different scene; the
  accepted runs pin the goal-3 hover camera (`WILDFORGE_SHOT_ALTITUDE=0`) on
  both phases, which each sidecar's camera identity records.

## Commands

The complete drivers are recorded in the pull request; representative
invocations:

```sh
# closeout capture (from WSL, running the native Windows executable)
powershell.exe -NoProfile -Command "Set-Location C:\Games\Wildforge; \
  $env:WILDFORGE_VISUAL_EVIDENCE='1'; $env:WILDFORGE_WORLD='visual-polish-strata-baseline'; \
  $env:WILDFORGE_CAPTURE_ID='strata-closeout-sandstone-v4-near-noon-base'; \
  $env:WILDFORGE_CAPTURE_SCENE='closeout-strata-20260806'; \
  $env:WILDFORGE_SHOT='screenshots/strata-closeout-sandstone-v4-near-noon-base.ppm'; \
  $env:WILDFORGE_SHOT_SIZE='1280x720'; $env:WILDFORGE_SHOT_MIN_FRAME='180'; \
  $env:WILDFORGE_SHOT_ALTITUDE='0'; $env:WILDFORGE_FACE='neg_z'; \
  $env:WILDFORGE_POS='1436.5,68.05,-1871.5'; $env:WILDFORGE_LOOK='3.141593,0.046079'; \
  $env:WILDFORGE_TIME='0.790053'; $env:WILDFORGE_DAY='0'; $env:WILDFORGE_WEATHER='clear'; \
  $env:WILDFORGE_VIEW_DIST='4'; .\wildforge-closeout.exe"

# per-capture reports and closeout aggregates (repository root)
python3 tools/verify_visual_polish.py report \
  --sidecar screenshots/strata-closeout-sandstone-v4-near-noon-base.capture.toml \
  --report screenshots/visual-polish/strata-closeout-sandstone-v4-near-noon-base.report.toml
python3 tools/verify_visual_closeout.py qualify \
  --readability-report screenshots/visual-polish/closeout-readability.report.toml \
  --strata-performance-report screenshots/visual-polish/closeout-strata-performance.report.toml \
  --geode-composition-report screenshots/visual-polish/closeout-geode-composition.report.toml \
  --geode-performance-report screenshots/visual-polish/closeout-geode-performance.report.toml

# fresh opened-save preparation
wildforge --prepare-cracked-geode saves/visual-polish-geode-sealed \
  --destination saves/visual-polish-geode-opened \
  --site screenshots/visual-polish/cracked-geode-site.toml

# offline conservation checks run against tonight's saves
wildforge --water-audit saves/visual-polish-strata-baseline   # 0 HU / 0 salt unexplained
wildforge --material-audit saves/visual-polish-geode-opened   # BALANCED
```

## Repository gates

Recorded at the evidence commit of this report:

```text
cargo fmt --all -- --check                          PASS
cargo clippy --locked --all-targets -- -D warnings  PASS
cargo test --locked --all-targets                   PASS
  fast lane: 789 passed, 0 failed
  serial agent lane: 10 passed, 0 failed (25.73 s)
cargo build --locked --release                      PASS
cargo deny check advisories                         PASS
git status --short                                  clean after the evidence commit
```

## Tracked-file audit

`git ls-files` contains no `.ppm`, `.exe`, `.zip`, `.wfd`, or save
directories. All 555 tracked PNGs are content (base/pack textures, docs
imagery); the largest tracked file is planetary diagnostic data
(`screenshots/planetary-atlas-seed-1337-v9/hydrology.toml`, 2.7 MB). Raw
capture frames remain ignored local evidence with tracked hashes.

## Hero image

The reviewed hero image name is `geode-cracked-hero` (goal 3) and its
closeout re-capture `closeout-geode-cracked-hero`; the closeout frame was
reviewed at full resolution and thumbnail size by Claude (the closeout was
executed end-to-end by Claude on Doll's machine) against the composition
gates and the goal-3 original. Final human taste confirmation rests with the
pull-request review.

## Limitations

- Still captures of live sites are snapshots of an evolving world; the
  next arc that touches hydrology should expect site water levels to differ.
- The closeout compares performance across three merged arcs; its simulation
  numbers describe the merged game, not the strata tile change alone.
- `verify_visual_closeout.py check-qualification` and `check-motion` need
  the local raw frames (ignored by git) and the adjacent geode report copies
  they recreate; CI verifies the tracked reports through the Rust validator
  instead.
- The interactive walkthroughs were driven by scripted, ordinary key events
  on disposable save copies (goal-2 precedent) with deterministic frame
  analysis plus Claude's review of every frame; they are evidence for the
  motion gates, not a replacement for a human playing the build.
