<!-- wildforge:guide -->

# Visual qualification evidence

This complete native GPU bundle is selected by `screenshots/current-campaign.toml`.
It contains 92 selected captures and reports from clean candidate `70de48ac`
and main `8c1ec408`, plus review of 52 visual stills and 56 native motion frames.
All final aggregate gates passed on discrete NVIDIA Vulkan hardware.

The first full trial failed the closeout geode simulation timing limit:
+0.124632 ms versus a +0.100 ms maximum. One complete, predefined repeat of
all ten closeout timing captures used the same frozen executable and immutable
worlds, in the same alternating order. Its simulation delta was -0.0007777 ms
and draw delta -0.03116734 ms; both passed unchanged limits. Every repeat sample
is used, and no further timing repeats were scheduled. The independent ordinary
geode group also passed (+0.018419 ms simulation).

`trials/initial-closeout/` preserves all ten original sidecars, their deterministic
reports, and the failed aggregate. `derivation.json` records each selected
capture's original trial and byte hashes, the predefined repeat protocol, and
the source comparison. `execution.json` records 102 actual native captures in
total, including the ten retained initial timing observations. Earlier campaign
metadata remains unchanged. The failed timing delta did not reproduce; these
measurements do not establish performance beyond the declared scenes.

Run `cargo test --locked visual_capture::tests::` from the repository root.
Raw frames, fixture saves, executable copies and process logs remain under
`target/maintainability/visual-campaign-20260906/`. Published motion frame paths
are repository-relative for portability; frame bytes and hashes are unchanged.
The motion review describes sampled WASD frames, not continuous video.
