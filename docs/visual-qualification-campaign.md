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
heavy work out of their measurement interval. Timings are the game's recorded
draw and simulation observations, not GPU timestamp measurements.

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
