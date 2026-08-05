# Visual polish sequence

Drafted 2026-08-04. **IMPLEMENTATION IN PROGRESS; GOALS 1–3 COMPLETE.**

This is the small post-planetary polish arc left by two explicit qualification
findings. It does not reopen planetary world design:

1. pale natural strata can lose their form against atmospheric fade;
2. the minerals arc still owes a real cracked-geode reveal capture.

The implementation plans are:

- [Strata and atmospheric readability](strata-fog-readability-plan.md)
- [Cracked geode reveal and capture](cracked-geode-capture-plan.md)

There are no hidden feature documents in this arc. If implementation exposes a
worldgen, streaming, conservation, or lighting defect beyond these scopes,
record it as a separately reviewed prerequisite rather than silently expanding
"visual polish."

## North star

Polish must reveal the simulation already present. The player should read real
geological layers through real weather, then crack a real finite geode and see
the structure worldgen and material accounting created. The camera never earns
permission to fabricate the world.

## Order and dependency

```text
current native-GPU baseline
            |
            v
shared visual evidence contract
            |
            v
pale strata / atmosphere fix
            |
            v
real geode selection and reveal
            |
            v
cross-scene GPU qualification and closure notes
```

The strata goal comes first because the geode is pale limestone/marble around
pale quartz. It also builds the shared block/depth attachment and visual
manifest validator that makes the geode proof mechanical. The geode goal may
begin site selection in parallel after the manifest schema is stable, but its
final capture must use the accepted strata/fog result.

## Goal 1 — Shared evidence foundation

Before changing art, implement only the common qualification plumbing:

- `screenshots/visual-polish.toml`, versioned and fail-closed;
- exact capture identity: commit, world/atlas versions, site, camera, clock,
  weather, pack, view distance, resolution, GPU backend, and telemetry;
- a development-only `WILDFORGE_SHOT_SIZE=<width>x<height>` override parsed
  before window/surface creation, with bounded dimensions and no persistence;
- a development-only visible-fragment block-family/depth attachment;
- `tools/verify_visual_polish.py` for deterministic reports;
- native PPM-to-PNG provenance and hashes while full frames remain ignored;
- a validator test that rejects incomplete or stale evidence.

Definition of done: one unchanged shipping scene can be captured twice and
produces the same site/camera identity and equivalent segmentation/metrics;
none of this plumbing runs in ordinary play.

Suggested goal command:

```text
/goal complete all features in Goal 1 of docs/visual-polish-sequence.md
```

## Goal 2 — Strata and atmospheric readability

**Completed and qualified 2026-08-05.** The accepted implementation and
native-GPU measurements are recorded in the linked plan and version-2 visual
manifest. Goal 3 is now the next unfinished document in this arc.

Execute [the strata plan](strata-fog-readability-plan.md) in its stated order:
fresh baseline, material signal, re-measurement, shader work only if still
proved necessary, then pack and native-GPU qualification.

The goal is not complete with prettier tiles alone. It needs monotonic horizon
fade, no exposed streaming boundary, dark-rock control, minimum/default view
checks, base/Gemini/Dusk inheritance, and recorded performance parity.

Suggested goal command:

```text
/goal complete all features in the design document docs/strata-fog-readability-plan.md
```

## Goal 3 — Cracked geode reveal and capture

**Completed and qualified 2026-08-05.** A deterministic finite production
geode was selected, opened through authoritative operations in a copied save,
balanced across reload, and captured in four settled native-GPU frames. The
linked plan records the site, operation ledger, composition metrics, matched
performance run, implementation defects fixed, and evidence commit. Goal 4 is
now the only unfinished work in this arc.

Execute [the geode plan](cracked-geode-capture-plan.md) against the accepted
visual foundation. Locate a finite production geode, prove it sealed, open a
copy through normal player operations, reconcile and reload it, then make the
four evidence captures and final hero frame.

The goal is not complete if quartz or amethyst was hand-authored into the
scene, the opening bypassed authoritative block breaking, the material audit is
unbalanced, the frame timed out while streaming, or the image came from a CPU
renderer.

Suggested goal command:

```text
/goal complete all features in the design document docs/cracked-geode-capture-plan.md
```

## Goal 4 — Arc qualification and closeout

Run both plans' matrices on the same native hardware and accepted commit.
Walk the strata site and geode approach interactively, because still frames do
not catch temporal shimmer, late chunk walls, exposure pumping, or an awkward
reveal in motion. Then:

- run format, Clippy, all tests, release build, and advisory checks;
- compare settled performance at identical before/after settings;
- audit tracked files for accidental saves, PPMs, PNGs, executables, or ZIPs;
- update the historical finding in the planetary documents without erasing
  what those earlier captures actually showed;
- close the deferred line in `docs/minerals-geology-plan.md` with the selected
  site and evidence record;
- record an implementation report with commands, results, hardware, capture
  hashes, limitations, and the reviewed hero image name.

Suggested goal command:

```text
/goal qualify and close the visual polish arc in docs/visual-polish-sequence.md
```

## Global gates

Every goal preserves these invariants:

- no geology, biome, hydrology, material quantity, or finite-planet change;
- no save or network compatibility change unless separately justified;
- no release runtime work from dormant qualification tooling;
- no new draw call or atlas allocation for ordinary strata/geode blocks;
- no binary or untracked capture artifact committed;
- no software-renderer claim presented as performance or visual approval;
- no decrease in default view distance and no visible loaded-world edge.

Repository checks:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
cargo deny check advisories
git status --short
```

## Arc definition of done

The arc is complete when a person can walk a production landscape, identify
pale strata before they vanish naturally into weather, enter an honestly mined
tunnel, and see a conserved worldgen geode through a small torch-lit aperture;
the tests, reports, native-GPU frames, and material ledger all describe that
same world.
