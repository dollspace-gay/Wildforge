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
| Folder guidance | Directory guides and tested coverage checker added | 53 maintained directories pass coverage; verify hash opt-out and enable CI check |

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
