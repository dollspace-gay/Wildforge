# Maintainability implementation record

## Goal and baseline

Implement every coding phase and acceptance criterion in
`maintainability-refactor.md`, and provide useful README/AGENTS guidance in
every maintained repository directory. Include logical new folders as owners
are extracted. Runtime data, build output, Git internals, and ignored third-party
installations are outside the maintained source-directory inventory.

The previous planning turn made progress: it delivered the plan, advisory
analyzer, CI job, and verified analyzer tests. This is the first implementation
goal turn. The full objective is active; no architecture phase is claimed done.

Local baseline checkpoints:

- `f41df43`: existing agent navigation and predator repairs, separately recorded.
- `c6d9589`: design, baseline measurements, and advisory tooling.
- Work branch: `refactor/maintainable-engine` (not published).

Baseline verification completed at `c6d9589`: format, strict all-feature
Clippy, Rust 1.95 MSRV, 1,017 subsystem tests (24 ignored), 15 serial agent
tests, doctests (none defined), and release build all passed. Exact commands,
exit codes, timings, and logs are in `target/maintainability/baseline/`. The
baseline release executable is preserved there as `wildforge` for controlled
runtime comparisons. Cold/warm runtime measurements remain outstanding.

## Requirement audit

| Criterion | Current evidence | Remaining work |
|---|---|---|
| AC-1 compatibility | Existing repairs and planning separated into commits | Baseline gates; per-slice save/codec/genesis/API checks |
| AC-2 authority/replica | Existing `World::remote` guard | Distinct owners and bounded read APIs |
| AC-3 terrain jobs | Shared `TerrainJobs`; both adapters migrated; eight queue/worker tests pass | Owned shutdown, errors, session rejection, runtime qualification |
| AC-4 common client | Duplicate admission/remapping inspected | Shared session and protocol/runtime proofs |
| AC-5 player operations | Stall-sale clone inspected | Shared domain operations and adapter parity |
| AC-6 world domains | Domain fields/methods inventoried | Encapsulation, explicit transaction coordination |
| AC-7 generation stages | Existing pure `Generator` boundary | Stage extraction and deterministic order tests |
| AC-8 app/content/render/tools | Existing module tree and clone report | Cohesive owners and affected runtime/codec checks |
| AC-9 source size | Advisory 400/500 report and baseline | Reduce migrated debt; explain any real exceptions |
| AC-10 clone analyzer | 18 tests passed in planning; complete JSON snapshot | Maintain analyzer coverage as tooling evolves |
| AC-11 dependency boundaries | Direction specified in plan | Dependency checker with violating fixtures |
| AC-12 tests/performance | Baseline suite passed; extraction focused gates passed | Full slice gates and reproducible measurements for affected paths |
| AC-13 shutdown | Existing detached terrain/mesh workers inspected | Owned shutdown, join/cancel, persistence failure tests |
| AC-14 migration record | This file and separate baseline commits | Add implementation/result entries after each slice |
| Folder guidance | 53 maintained directories documented; coverage CI and hash regressions pass | Recheck when adding directories |

## Compatibility observations requiring explicit treatment

`net::content_hash` and `collect_mod_files` currently include non-script
documentation files. Adding guides inside a mod directory therefore affects
the transport content hash. Before adding those guides, characterize this
behavior and separate documentation from runtime content without altering
registry identities or rewriting immutable saved-world manifests. Record the
compatibility outcome and regression evidence as its own corrective slice.

## Next implementation slice

Shared terrain jobs: preserve existing client/host worker counts and adoption
budgets; centralize queue ownership, priority promotion/preemption, completion
delivery, and deduplication. Follow with isolated shutdown/stale-result and
load-error corrections. `World::adopt_prepared` remains the authoritative commit
point while later domain extraction strengthens that boundary.

## Terrain preparation extraction (in verification)

`src/terrain_jobs` now owns preparation queues, worker policy, request
deduplication, cancellation, and loaded/generated provenance for both hosting
paths. Existing worker counts, priority preemption, graphical capacity, and
host adoption budgets are preserved. Queue/policy/real-worker tests pass (8);
the existing worker/worldgen regression, multiplayer tests, 15 serial agent
tests, and strict all-feature Clippy also pass. Format and MSRV also passed. The full subsystem run produced 1,020 passes
and five visual-evidence freshness failures: the declared qualification source
set includes `src/lib.rs` and `src/game/mod.rs`, changed by this extraction.
The old capture manifest remains untouched and must be requalified through
actual GPU evidence before this slice is accepted. Doctests passed (none
defined); release verification is recorded in the same log directory.
Logs: `target/maintainability/terrain-extraction/`.

This is a structural checkpoint, not completed Phase 2. Detached worker
shutdown and collapsed save-read errors remain deliberately separate repairs.
Prepared chunks still commit only through the authoritative world adoption
operation. No public API, MSRV, dependency, codec, or generation algorithm
changes were introduced. Actual GPU/runtime qualification remains pending.

## Folder guidance and content identity

All 53 maintained repository directories now have local README/AGENTS guides.
`tools/check_folder_guides.py` inventories tracked/unignored source ancestors,
requires both guides, rejects heading-only stubs, and ignores private runtime
and generated trees through Git's existing ignore rules. CI runs the checker.
Human review still determines whether the guidance is accurate and useful.

A separate compatibility correction extracts mod inventory/hash code into
`src/net/content.rs`. Only new `README.md`/`AGENTS.md` files beginning with
`<!-- wildforge:guide -->` are omitted from content hashes and transfer.
Historical unmarked files, including `mods/README.md`, retain their bytes and
hash contribution. Scripts retain the existing host-only exclusion. Runtime
TOML/images cannot use the guide marker to opt out.

Evidence: 21 Python tests, 2 content/hash regressions, the genesis-content
regression, 51 registry scenarios, 45 rendering/atlas tests, strict all-feature
Clippy, actionlint, folder coverage, and whitespace checks pass. The installed
`mods` path hashes to `4d96ccb51157d021` both before and after adding guides.
An initial registry filter matched zero tests; it was corrected to
`tests::registry_tests::` in commands and new guides, then actually exercised.
Logs are in `target/maintainability/folder-guides/`. Full post-correction Rust
and runtime gates remain pending; the previous five stale visual evidence
checks have not been bypassed or relabeled as passing.

## Temporary size exceptions during migration

These are explicit review points for existing owners, not linter exclusions.
New terrain/content/tooling modules remain below 400 lines. The scan continues
to report every oversized file and these entries must be revisited at the
named phase rather than treated as permanent exemptions.

| File | Current lines | Domain and reason | Next review |
|---|---:|---|---|
| `src/game/mod.rs` | 1112 | App composition still owns the remaining client fields | Phase 7 session/UI/presentation owners |
| `src/game/session.rs` | 1059 | Entry adapters still coordinate the old client state | Phases 3 and 7 client session extraction |
| `src/lib.rs` | 674 | Legacy CLI dispatch remains alongside the facade | Phase 7 app command extraction |
| `src/tests/mod.rs` | 518 | Shared fixtures remain alongside test registration | Phase 7 scenario fixtures and test organization |
| `src/net/transport.rs` | 1006 | Content inventory is extracted; QUIC lifecycle/discovery remain | Phases 3 and 8 session/transport boundaries |
| `src/planet_atlas/climate.rs` | 2440 | Live-audit correction reuses the existing total helper; stage ownership remains debt | Phase 6 climate stage extraction |
| `src/multiplayer/host/streaming.rs` | 462 | Within review ceiling; encoding/snapshot delivery still share the adapter | Phases 2 and 3 encoding/session ownership |

The baseline check commands, counts, compatibility versions, executable hash,
and source hashes for public entrypoint modules are retained in
`maintainability-verification-baseline.json`. Source hashes pin the original
implementation; they are not an assertion that future internal source bytes
must remain unchanged. Public signatures and behavior require review per slice.

## Live water audit correction

The new exactly-once adoption regression exposed an existing disagreement:
`PlanetaryWeather::water_audit` counted vapor but omitted cloud water, while
atlas and climate-step audits used `dynamic_water_total`. Seed 42 / side 8
reported a false deficit of 3,548,558 HU. The live audit now uses that same
helper. This corrects reporting only: no reservoir, ledger, climate state,
terrain output, save format, or wire behavior changes.

A separate regression requires nonzero cloud water and equality between live
and atlas audits. That test and all 17 climate scenarios pass. With the
correction, both terrain worker policies match synchronous material/water
accounting, remain balanced, and reject duplicate adoption without changing
those totals. The correction is kept separate from worker lifetime changes.

## Terrain lifetime correction (in verification)

The terrain owner now retains worker handles. Explicit shutdown and Drop stop
accepting requests, clear queued work, wake waiters, join every running worker,
and discard ready results. Running preparation finishes cooperatively before
shutdown returns. Publication checks the stopped state under the same lock.
A retained generation identity rejects completions from an old session before
they can release a current request's budget. Worker panic/poison failures are
reported after joins; Drop emits an actionable error if shutdown was implicit.

All 12 terrain tests pass, including real saved/generated terrain, player-edit
protection, old-session rejection, explicit and implicit joins, panic reporting,
and exactly-once material/water accounting against synchronous adoption. The
25 multiplayer scenarios, 15 serial agent scenarios, and strict all-feature
Clippy also pass. Logs are in
`target/maintainability/terrain-lifecycle/`; further gates are recorded there.

Still pending in Phase 2: typed missing/invalid/unreadable save outcomes,
live worker/startup error propagation, owned mesh/encoding workers, caller-level
world-switch/disconnect tests, and controlled runtime/GPU requalification.
This lifetime checkpoint does not close the full shutdown acceptance criterion.

## Checkpoint results at `4b9582a`

Format, strict all-feature Clippy, MSRV 1.95, doctests (none defined), release
build, and 15 serial agent tests pass. The full subsystem run has **1,027
passes, 24 ignored, and five failures**. All five are the previously identified
visual qualification source fingerprints; no new test failures remain. This
is not a green full-suite claim. `remaining-gates.json` records exact commands,
revision, exit codes, and timings under `target/maintainability/terrain-lifecycle/`.
No unsafe boundary, public signature, dependency, MSRV, save codec, or protocol
version changed. GPU/runtime qualification has not yet been performed on this
checkpoint; the user playtest world and identities remain untouched.

Local change sequence: `cc474a1` shared preparation; `db595e3` folder guidance
and content identity; `65643ea` live water audit correction; `4b9582a` terrain
shutdown/session rejection. None has been published.

## Advisory baseline comparison

`--base` now scans one immutable Git commit and the working tree with identical
analyzer settings, without checking out or running base source. It reports
new/grown/reduced/removed size findings, preserves Git-detected file renames,
and compares exact clone-family IDs and occurrence locations. CI uses a pinned
PR base SHA and keeps all findings advisory. Unknown/broken bases fail clearly.

All 28 Python tests pass on Python 3.14.7 and 3.12.13. Tests include source
renames, overwritten destinations, symlink exclusion, committed-versus-working
bytes, clone moves/growth, mismatched thresholds, and invalid base scans.
Actionlint, directory coverage, and whitespace checks pass. The full-tree
comparison against `c6d9589` reports 102 files over 500 lines (baseline 103),
71 clone candidate groups, three grown and three reduced size findings, and
three moved clone families. These measurements do not imply a performance
improvement or semantic elimination of every clone. Logs/reports are under
`target/maintainability/delta-*`; the original baseline JSON is unchanged.

## Immutable chunk reader extraction

The previous goal turn made concrete progress: committed worker ownership,
folder guidance, audit correction, and baseline-delta tooling with their tests.
The next slice extracts `ChunkLoader` unchanged into
`src/world/storage/reader.rs`, with local storage-domain guides. All 12 terrain
worker tests pass after the move; codec bytes, palette rules, and the existing
legacy-placeholder repair are unchanged. This structural checkpoint precedes
the separate missing/corrupt/unreadable read correction. Directory coverage is
now 54 maintained directories. Full slice/runtime acceptance remains pending.
