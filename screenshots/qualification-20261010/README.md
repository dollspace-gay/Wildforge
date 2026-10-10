<!-- wildforge:guide -->

# Visual qualification evidence

Native GPU campaign metadata and deterministic qualification reports.

These files belong to the campaign selected by `screenshots/current-campaign.toml`.
Run `cargo test --locked visual_capture::tests::` from the repository root.
Raw frames and fixture saves remain in the campaign's ignored work directory.

This campaign contains 92 new native captures (52 stills and 40 timing cases)
and 56 reviewed native motion frames on NVIDIA RTX 5070 Ti Laptop Vulkan.
Primary and closeout geode rendering increases are 0.078224 ms and 0.058288 ms
against the unchanged 0.20 ms combined CPU/GPU budget. The reports retain the
complete 120-frame rendering-work receipts and the original draw observations.
`execution.json` records the frozen binaries and final source fingerprint.
`verification.json` records final command results; `stills-review.json` and
`motion-review.json` retain actual review observations and frame hashes.
Motion sampling does not establish behavior between samples or in arbitrary
worlds. Raw artifacts and failed prior campaigns remain under the ignored
`target/lag-control-20261009/` laboratory directory.

`shadow-guard-native-diagnostic.json` is a separate four-capture before/after
control, not qualification evidence. Matching old/new color and material files
are byte-identical; the production GPU test also checks 1,188 bit-identical
finite lighting cases and exact shadow invocation counts.

## Reconstructing the instrumented baseline

The baseline was built from a standalone clean snapshot at
`7913ac49e15e65cc4dd2dad793c96f70dbf26916`. Its historical Git metadata is local,
so `baseline-replay.patch.gz` provides the complete source reconstruction from
published ancestor `6383f2c67b54732f549914d5421fcb30eed1e274`. It includes the
original measurement instrumentation, isolated package name, render-work
protocol and declared dawn warmups, while preserving original gameplay and
excluding the candidate security, movement and lighting fixes.

Decompress the patch with `gzip -dc` into ignored working storage. In a new
disposable checkout of that published ancestor, run `git apply --check`
and `git apply` against that patch, then create an actual new clean commit before
building. Do not assign the historical baseline SHA to a rebuilt executable.
The baseline target name is `wildforge-lag-baseline`; use Cargo metadata to
select its executable. A replay requires fresh captures with its actual build
identity. `baseline-provenance.json` records the patch hash, and
`baseline-replay-verification.json` records byte equality for all 2,793 tracked
files and the original baseline source fingerprint. See the
[campaign workflow](../../docs/visual-qualification-campaign.md) for preparation,
capture, review and acceptance commands.

The patch and the two raw GPU logs are gzip archives, preserving their original
bytes, including patch context and terminal blank lines. `replay-artifacts.json`
records compressed/raw hashes and verified byte-exact decompression.
