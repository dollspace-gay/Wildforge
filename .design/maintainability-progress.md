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
- Initial local branch: `refactor/maintainable-engine`.
- Published work branch: `refactor/engine-maintainability` (draft review; full migration remains active).

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
| AC-3 terrain jobs | Shared owned `TerrainJobs`, observable errors, saved revision checks, and owned homeland trials | Content/palette context invalidation; caller and runtime qualification |
| AC-4 common client | Duplicate admission/remapping inspected | Shared session and protocol/runtime proofs |
| AC-5 player operations | Stall-sale clone inspected | Shared domain operations and adapter parity |
| AC-6 world domains | Domain fields/methods inventoried | Encapsulation, explicit transaction coordination |
| AC-7 generation stages | Existing pure `Generator` boundary | Stage extraction and deterministic order tests |
| AC-8 app/content/render/tools | Existing module tree and clone report | Cohesive owners and affected runtime/codec checks |
| AC-9 source size | Advisory 400/500 report and baseline | Reduce migrated debt; explain any real exceptions |
| AC-10 clone analyzer | 18 tests passed in planning; complete JSON snapshot | Maintain analyzer coverage as tooling evolves |
| AC-11 dependency boundaries | Direction specified in plan | Dependency checker with violating fixtures |
| AC-12 tests/performance | Baseline suite passed; extraction focused gates passed | Full slice gates and reproducible measurements for affected paths |
| AC-13 shutdown | Terrain, mesh, encoding, creation, entry, and homeland work have joined owners and failure tests | Dedicated shutdown; graphical close/capture and save-failure runtime proof |
| AC-14 migration record | This file and separate baseline commits | Add implementation/result entries after each slice |
| Folder guidance | 53 maintained directories documented; coverage CI and hash regressions pass | Recheck when adding directories |

## Compatibility observations requiring explicit treatment

`net::content_hash` and `collect_mod_files` currently include non-script
documentation files. Adding guides inside a mod directory therefore affects
the transport content hash. Before adding those guides, characterize this
behavior and separate documentation from runtime content without altering
registry identities or rewriting immutable saved-world manifests. Record the
compatibility outcome and regression evidence as its own corrective slice.

## First implementation slice (historical)

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
| `src/game/mod.rs` | 1079 | App composition still owns the remaining client fields | Phase 7 session/UI/presentation owners |
| `src/game/session.rs` | 793 | Entry adapters still coordinate the old client state | Phases 3 and 7 client session extraction |
| `src/mesher.rs` | 903 | Snapshot registry identity query accompanies the existing CPU mesh implementation | Phase 7 mesh/input ownership |
| `src/lib.rs` | 674 | Legacy CLI dispatch remains alongside the facade | Phase 7 app command extraction |
| `src/tests/mod.rs` | 518 | Shared fixtures remain alongside test registration | Phase 7 scenario fixtures and test organization |
| `src/net/transport.rs` | 1006 | Content inventory is extracted; QUIC lifecycle/discovery remain | Phases 3 and 8 session/transport boundaries |
| `src/planet_atlas/climate.rs` | 2440 | Live-audit correction reuses the existing total helper; stage ownership remains debt | Phase 6 climate stage extraction |
| `src/world/mod.rs` | 4258 | Composes region and palette owners; remaining world domains still share fields | Phase 5 domain ownership |
| `src/world/chunks.rs` | 1192 | Adoption keeps its existing side-effect sequence; retrogen delegates palette publication | Phase 5 chunk/residency domain |
| `src/world/storage.rs` | 1144 | Delegates codec, palette, and region I/O; sidecars, save orchestration, and remapping remain | Phase 5 persistence ownership |
| `src/world/spawn.rs` | 1592 | Fingerprints use coordinated reads; trial/qualification orchestration remains | Phases 5 and 6 entry/genesis ownership |

The host streaming adapter is now 371 lines after extracting its private job
owner; it no longer needs a size exception. World-domain files above remain
explicit migration debt, with the added revision boundary reviewed in Phase 2.

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

## Terrain read and write errors (in verification)

The decoder distinguishes missing terrain, stored WFC6-WFC8 terrain, and the
named valid legacy-placeholder repair. Bounded byte/RLE parsing rejects
truncation, zero/overflow runs, and trailing data. Region I/O distinguishes
absence from corruption/I/O errors, bounds offsets and allocations, refuses to
rewrite damaged headers, and aborts compaction rather than dropping unreadable
payloads. Region code and scenario tests now live with storage primitives.

Worker failures retain position, cause, and session identity. Failed requested
positions are suppressed while interest persists. The graphical adapter pauses
and reports a read failure; the host refuses affected entry/view requests.
Synchronous adoption also refuses failed reads. Saved bytes are not regenerated
on ordinary corruption or I/O failure; the previously supported valid
all-placeholder repair remains explicit and tested.

Focused results: 3 saved-version/decoder tests, 9 region scenarios, and 3 actual
worker/save-retry/QUIC admission scenarios pass. The 12 terrain-worker and 25 multiplayer scenarios also pass, along with
strict all-feature Clippy. A failed region write retains dirty edits and can be retried after repair.
The read contract and region-write behavior are corrective changes, separate
from the prior immutable-reader extraction. Full Rust/runtime gates remain pending.

Code inspection also identified a concurrency requirement for the next immediate
step: cold readers and the authoritative writer must share region coordination,
so a read cannot observe a partially published header/index. Prepared terrain
also needs persisted-revision validation after a player edit is saved/unloaded.
These checks are required before accepting the streaming slice; they are not
resolved by the existing resident-chunk edit regression alone.

## Coordinated region I/O and prepared revision validation

`568e18a` separates the host's private chunk-job/cache owner from guest interest
and delivery scheduling without changing budgets. Its compile, format, directory
coverage, and whitespace checks passed before the following corrective slice.

Category A root-cause correction: World now owns a RegionStore, cloned into
immutable loaders, that coordinates raw reads/writes/compaction per region.
Spawn fingerprints use that owner too. Reads retain opaque revision watches;
attempted writes invalidate watches for that chunk before touching disk. Both
streaming adapters reject stale prepared results without another file read,
even after an edited chunk was saved and unloaded. No disk/wire format changes
or global I/O mutex were introduced. Weak region/watch indexes retain only
active work plus expired slots pruned on the next access.

Call-site evidence: `rg -n 'region::(read|write)_chunk|adopt_prepared\\(' src
--glob '*.rs'` found raw production reads/writes only inside RegionStore and
the unchecked adoption method only behind the revision-checked World boundary;
that unchecked method is now private. The scope is one World session: independent
processes or separately opened Worlds writing the same directory are not
coordinated. The existing on-disk index is not a crash-atomic transaction; its
documentation now states that limit rather than promising an intact old slot.

Eight storage tests, 13 terrain-job tests, and three real worker/save/QUIC
scenarios pass, as does strict all-feature Clippy. Scenarios cover independent
region progress, concurrent complete payload snapshots, watch lifetimes, failed
write invalidation, missing/saved provenance, edits saved/unloaded before late
adoption under both worker policies, and unchanged exactly-once water/material
accounting. The saved fixture initially did not dirty its generated chunk, so
no persisted input existed; explicitly editing it corrected the fixture.
Format, strict all-feature Clippy, MSRV 1.95, 15 serial agent scenarios,
doctests (zero defined), and release build pass. The full subsystem run recorded
1,041 passes, six failures, and 24 ignored. Five failures are the previously
identified stale visual-source fingerprints. The sixth was a save-error fixture
that made its save directory unreadable before asking to generate its dirty
chunk. Under the corrected read contract it never had a chunk to save. The
fixture now creates and edits terrain first, then injects the same filesystem
write failure; all its component-reporting and retry assertions are retained.
That focused regression and strict Clippy pass after the fixture correction.
The whole subsystem suite was not rerun after this test-only change; no full
green-suite claim is made.

Logs, exact broad-gate commands/results, and source fingerprints (including
untracked new modules) are under
`target/maintainability/region-coordination-gates/`. The advisory comparison is
`target/maintainability/region-coordination-delta.json`: 220 source files,
119 above 400 lines, 102 above 500, and 71 exact-token clone candidate groups.
All 54 maintained directories retain both guides. This is not full streaming
or migration acceptance. No new native GPU/runtime qualification was performed.

Next Phase 2/13 work remains: typed startup/live worker failures, owned mesh
and encoding lifetimes, caller-level world-switch/disconnect proof, and measured
native runtime travel/entry qualification. Inspection of `game/content.rs`
also confirms hot reload remaps World/generator but retains old terrain-job
context; rebuild/invalidate that context with a regression before acceptance.
Phases 3–8 and the five stale visual-qualification manifests remain outstanding.

## Observable failures and owned snapshot workers (in verification)

The previous goal turn was progress: `f033c97` committed coordinated region I/O
and saved-revision validation with focused and broad-gate evidence. This turn
extends the Phase 2/13 lifetime boundary rather than declaring the phase done.

Category A root-cause correction: native terrain startup returns `io::Result`
and joins already-started workers on partial failure. A supervised panic or
poisoned queue stops work and retains its cause. The graphical client receives
one notification; the host retains fatal state and refuses affected current or
future arrivals through the existing server-error protocol.

`background/` now owns reusable native spawn/panic boundaries and bounded FIFO
snapshot processing. Terrain retains its entry-priority queue. CPU mesh jobs
and wire encoding use the shared snapshot owner: stop, cancel queued inputs,
join active work, discard output. Host shutdown stops both preparation and
encoding before joining either. The new mesh adapter owns deduplication and
uses explicit requests/results; live streaming keeps GPU upload and acceptance.
Startup errors reach local/guest entry handling instead of detaching threads.

Intentional behavior change: wire encoding now permits at most 32 total queued,
running, and ready jobs. Rejected requests retain ordinary interest retry;
cancelled queued snapshots release their deduplication keys. The mesh limit
remains two. Terrain policy, adoption budgets, codec bytes, and GPU dispatch are
unchanged. `streaming_errors.rs` owns failure-to-refusal mapping and brings the
host streaming adapter back below 400 lines. Every new code file is below 400;
55 maintained directories have both local guides.

Focused evidence: 16 terrain tests, six snapshot lifecycle scenarios, one real
mesh buffer/emitter parity scenario, four worker/save/QUIC scenarios (including
an actual worker panic before admission), and 25 multiplayer scenarios pass.
Strict all-target/all-feature Clippy, format, folder coverage, and advisory
analysis pass. Logs are under `target/maintainability/worker-*`,
`snapshot-owner-*`, and `mesh-owner-*`. The clean source checkpoint is `932d1f8`;
its broad gates and native smoke evidence are recorded below.

Hardware execution map: immutable meshing still runs through
`mesher::mesh_chunk_input` on CPU; `renderer/resources.rs::upload_chunk` creates
wgpu buffers and the existing indexed draws in `renderer/frame.rs` run on GPU.
No shader/kernel or device fallback was added. The available native adapter is
an NVIDIA RTX 5070 Ti Laptop GPU, driver 610.57.04, with 12,227 MiB. CPU worker
tests alone cannot establish rendering; the native capture below exercises it.

Remaining Phase 2/13 work includes hot-reload context invalidation, caller-level
world-switch/disconnect/save-failure proof, entry/creation task ownership, and
dedicated/graphical graceful shutdown. The auto-capture path still calls
`process::exit`, so a capture can prove drawing but not owner Drop execution.
Controlled cold/warm entry and travel performance, the five visual evidence
requalifications, and all remaining Phase 3–8 ownership work stay in scope.

## Clean worker-owner gates and native smoke evidence

On `932d1f8`, format, strict all-feature Clippy, Rust 1.95 MSRV, all 15 serial
agent scenarios, doctests (zero defined), and release build pass. The complete
subsystem run records **1,053 passes, 24 ignored, and five failures**, all the
known stale visual-source fingerprints. The corrected save-error fixture passes
in this complete run. Exact commands, durations, and exit codes are in
`target/maintainability/worker-owner-gates/results.json`. An earlier gate attempt
was interrupted before the clean checkpoint; it is separately retained under
`worker-owner-gates-interrupted` and is not this revision's acceptance evidence.

Two native captures ran on a disposable copy of the playtest world, each exiting
with code zero after 15.65 seconds. The first wrote absolute artifact references,
which the existing converter rejects. The second used relative names and passed
both `verify_visual_polish.py report` and `check-report`; no raw pixels, checksum,
scene identity, or historical qualification manifest was changed to pass.
The first requested view distance three was clamped to the actual minimum four;
the second invocation explicitly requests four.

Validated metadata identifies the clean `932d1f8` build, NVIDIA discrete Vulkan
adapter, 1280x720 output, seed 20260904, generation version 10, and a settled
frame after 180 eligible frames (115 settled). It records 65 resident chunks,
63 uploaded opaque meshes, and 30 water meshes. Visual inspection of the PNG
shows the textured woodland, terrain, shadows, and in-world HUD. The invocation,
executable hash, raw PPM/WFD, metadata, verified report, and PNG are retained in
`target/maintainability/worker-owner-native/`. The accepted raw capture is under
its `runtime/` directory and is referenced by `invocation-relative.json`.

All 59 files (294,610,196 bytes) in the user's original playtest world remain
byte-for-byte unchanged, verified with before/after SHA-256 inventories. Both
capture processes are terminal and absent from `/proc`. This proves one native
mesh-to-GPU path; it is not a paired performance baseline, a travel benchmark,
graceful Drop-path shutdown proof, or replacement for the five full visual
requalification suites. Those requirements and the remaining phases stay open.

## Owned world loading and cancellable homeland preparation

`game::WorldLoading` now exclusively owns either creation or entry. UI state
holds progress text only; request preparation, worker execution, and presentation
are separate modules. `background::Operation` bounds progress to one latest value,
retains the native handle, preserves terminal I/O/panic errors, and joins before
completion or Drop. Cancellation cannot overwrite an active operation or admit
a world that finished just before cancellation. A successfully published world
remains listed when its subsequent entry is cancelled.

Prepared entry records the requested registry separately from the loaded World's
registry, which may contain restored placeholder definitions. If content changes
before adoption, the completed private world is discarded and entry restarts with
the current registry. This protects loading only; active terrain/mesh/host context
invalidation during content or palette replacement remains outstanding.

Cancellation now propagates through missing-world creation and common homeland
preparation. The old homeland trial threads could detach on early error; trials
now use `SnapshotJobs` with supervised per-worker context initialization. Existing
worker counts, private generator caches, and sorted adoption order remain intact.
Cancellation discards speculative trials and joins their running chunks. Once
homeland adoption or discovery retrogen persistence starts, it finishes that save
sequence before entry acknowledges cancellation. Atlas file reads, candidate
analysis, running chunk generation, and durable writes remain cooperative steps,
not preemptible operations; shutdown latency is not yet a qualified performance
claim.

A separate residency correction adds `try_ensure_chunk`: entry preserves the real
read error and checks residency instead of treating failed preparation as a ready
doorstep. Existing boolean simulation/adoption callers retain their interface and
failure reporting. Cancellation classification uses typed errors, so an unrelated
read failure containing the word "cancel" is not hidden. No codec, generation
algorithm, dependency, public exported API, or MSRV changes were made.

Focused checks so far: all 11 previously-run background lifecycle scenarios,
6 loading-owner scenarios, and 4 actual preparation/read-error scenarios pass;
strict all-feature Clippy passes after the complete source change. The additional
per-worker initialization scenario and full clean gates will be recorded below.
New production/test modules are below 400 lines. `game/session.rs` shrank from
1,078 to 794 lines and remains documented migration debt. Folder coverage finds
55 maintained directories and zero missing guides. The advisory scanner still
reports 119 files above 400, 102 above 500, and 71 clone groups; no existing debt
has been suppressed. Native entry/cancellation and broad qualification remain
pending, including the five historical visual-source freshness failures.

## Clean loading-owner gates and native cancellation proof

The clean `b2d1901` checkpoint passes format, strict all-target/all-feature Clippy,
Rust 1.95 MSRV, all 15 serial agent scenarios, doctests (zero defined), release
build, 28 Python scenarios, folder guidance, advisory analysis, and whitespace
checks. The full subsystem run has **1,069 passes, 24 ignored, and five failures**;
the failures are exclusively the existing visual qualification source-fingerprint
checks. This run includes all 12 background owner scenarios, 6 loading-owner
scenarios, and 4 preparation/read-error scenarios. Exact commands, clean revision,
timings, and logs are in `target/maintainability/loading-owner-gates/results.json`.
No stale visual manifest was rewritten to turn a failure into a pass.

Three native scenarios ran from disposable working directories with cwd-local
identities and configuration. The reproducible script, invocations, executable
SHA-256, logs, process IDs, and results are retained under
`target/maintainability/loading-owner-native/`:

- Existing-world entry produced a settled 1280x720 frame on the NVIDIA RTX 5070 Ti
  Laptop GPU using Vulkan. It completed in 15.602 seconds and records 65 resident
  chunks, 63 opaque GPU meshes, 30 water meshes, and 115 settled frames. Raw
  capture conversion and deterministic report validation both pass; the inspected
  PNG contains the woodland, terrain, shadows, and in-world HUD.
- Escape during missing-world entry cancelled atlas preparation. All observed
  `world-loading`/homeland threads were gone after 3.054 seconds, the process was
  still alive, and no destination world was published. A graceful window-quit
  request then produced exit code zero.
- A graceful window-quit request while `world-loading` was still present produced
  exit code zero with no published world. The entire startup/close scenario took
  1.983 seconds. Automatic screenshot exit was disabled for both cancellation
  scenarios, so they exercise the normal window/owner Drop path.

All three processes exited and are absent from `/proc`. The original playtest
save's 59 files (294,610,196 bytes) have identical before/after SHA-256 inventories.
These timings are individual native lifecycle observations, not paired cold/warm
or travel benchmarks. The screenshot path itself still uses `process::exit` and
therefore does not establish graceful capture shutdown. Dedicated shutdown,
in-world save-failure/close and session-switch scenarios, content/palette worker
invalidation, controlled performance evidence, five complete visual
requalifications, and the remaining Phase 3–8 ownership migrations stay open.

## Registry and palette context ownership (in verification)

Prepared terrain now retains its decoding registry and shared palette snapshot
alongside the saved-region revision. The authoritative adoption boundary rejects
obsolete context without save reads. Loader clones share the remap allocation;
every existing palette refresh replaces it. Terrain context construction derives
generation's registry from the loader, preventing divergent generation/decode IDs.

The graphical terrain owner compares seed/atlas/registry/palette/session context
before scheduling or adopting work. A replacement joins the old pool and starts
one new generation; failed startup is retained against that requested context.
The host's `HostChunkState` owns its pool or startup failure in one place and
retires encoders, pending revisions, and cached payloads with their context.
Matching failures cannot cause an endless per-frame or per-pump startup loop.
Mesh completion also retains the input registry in addition to the variant
signature, releasing and discarding stale results before the existing live-chunk
and GPU-upload checks. Worker counts and adoption budgets are unchanged.

Focused verification passes: 20 terrain queue/worker/context scenarios, 3 host
context/cache/failure scenarios, 2 actual CPU mesh-buffer scenarios, and 4 saved
I/O/real QUIC refusal scenarios. Strict all-target/all-feature Clippy passes.
An initial new fixture expected a saved chunk without modifying it; it was
corrected to persist an actual block edit before eviction, preserving the
regression's saved-byte and stale-repair assertions. No disk/wire codec,
dependency, exported API, MSRV, shader, or GPU upload implementation changed.
Meshing remains on CPU and the existing wgpu renderer performs the actual GPU
work; native reload/capture verification and the broad clean gates follow.

The source-size/clone scan still reports 119 files above 400 lines, 102 above
500, and 71 exact-token clone groups. New context test files and affected worker
owners are below 400. All 55 maintained directories retain their guides.

Remaining correctness boundaries are explicit: session admission must negotiate
changed live content for guests, and persistence palette publication still needs
its own domain owner. The current palette writer replaces the global numeric-ID
table before rewriting all unresident chunks; worker snapshot invalidation cannot
by itself make a reordered saved palette safe. These remain active migration
work, alongside caller-level switch/disconnect/save-failure runtime scenarios,
controlled performance evidence, five full visual requalifications, and the
remaining Phase 3–8 domains and tooling. No full migration acceptance is claimed.


## Stable stored palette owner (in verification)

`storage::PaletteStore` now owns each session's immutable block-ID mappings and
pending palette publication. Stored numeric IDs keep their original names;
new content appends entries, and removed names and vacant IDs remain reserved.
Runtime registry reordering therefore does not require rewriting cold chunks.
The saved reader uses the owner's immutable mapping and leaves decoded chunks
unmodified until a real edit. Fresh worlds have an identity mapping before their
first save, so an earlier loader no longer fabricates all-placeholder repair.
Successful first publication still retires older prepared-context snapshots.

Every authoritative chunk write now passes through the same palette publication
boundary, including unload, homeland, material transactions, and retrogen. Failed
publication retains the pending table and stops chunk publication; a later save
can retry. Invalid runtime block IDs cannot replace an existing chunk. Initial
palette read/parse failures remain explicit through loading and terrain workers;
a repaired file requires reopening the session. Entry validates a saved palette
before creating or updating world sidecars. Missing/empty historical palettes
retain the existing identity fallback; malformed tables are refused. Parsing is
bounded to 16 MiB and 65,536 numeric slots, and extension exhaustion cannot
partially alter the table.

`storage::encoder` owns the shared WFC planes. Disk encoding maps runtime blocks
to stored IDs while WFC9 keeps runtime IDs; metadata, light policy, salt, and HU
records retain their byte layouts. Production no longer exposes a runtime-ID
WFC8 helper; legacy codec test fixtures retain it under `cfg(test)`. No exported
crate API, dependency, shader, MSRV, or wire protocol changes were made.

Focused checks pass: strict all-target/all-feature Clippy; 18 storage scenarios
(including ten new palette/encoding regressions); 20 terrain lifecycle/context
scenarios; and four saved-I/O/real-QUIC refusal scenarios. The regressions cover
reordered and added content with unchanged cold bytes, removed/reinstalled names,
identity-codec byte equivalence, malformed/bounded input, full-table rollback,
and failed direct-save retry. Full clean gates and native entry/reload/save
qualification follow; the five existing visual-evidence failures are not bypassed.

All four new modules are below 400 lines. `world/persistence.rs` is now 351 lines
and no longer needs a size exception; the storage adapter is 1,144 lines and still
requires sidecar/domain extraction. The advisory scan reports 240 source files,
118 above 400 lines, 102 above 500, and 71 exact-token clone groups. All 55
maintained directories retain their README and AGENTS guidance. This addresses
the palette-publication defect recorded above; guest content negotiation,
independent authority/replica owners, the remaining Phase 3–8 migrations,
controlled performance work, and full visual qualification remain open.


## Clean palette-owner gates and native reload/save proof

The clean `b43f4af` checkpoint passes format, strict all-target/all-feature Clippy,
Rust 1.95 MSRV, all 15 serial agent scenarios, doctests (zero defined), release
build, 28 Python scenarios, folder coverage, advisory analysis, and whitespace.
The subsystem run has **1,087 passes, 24 ignored, and five failures**. Those five
remain the historical visual-qualification source-fingerprint checks. Exact
commands, revision, durations, and logs are in
`target/maintainability/palette-owner-gates/results.json`.

Native verification used the release executable from this clean checkpoint and
an isolated copy of the existing playtest world. Three successful scenarios are
recorded under `target/maintainability/palette-owner-native/attempt-2/` and
`target/maintainability/palette-owner-native/reopen/`, with scripts, explicit
environment, executable SHA-256, process IDs, UI commands, logs, and results:

- Focused X11 F5 input reloaded content and replaced all eight terrain thread IDs
  while retaining both mesh worker IDs. The settled 1280x720 capture records
  NVIDIA RTX 5070 Ti Laptop GPU hardware using Vulkan, 65 resident chunks,
  63 opaque GPU meshes, 30 water meshes, and 534 settled frames. Capture
  conversion and deterministic report checking pass, and the PNG was inspected.
- A separate run disabled automatic capture, reloaded content, and exited zero
  through the normal window-quit path. Its durable palette appended 26 names to
  the existing 287 bindings; a full ID/name comparison found zero changed old
  bindings and confirmed that every new numeric ID follows the old range.
- Reopening that saved world, reloading content again, and capturing another
  settled hardware-rendered frame passed. Its palette stayed byte-identical.
  Conversion/report checks pass and the reopened-world PNG was inspected.

Observed F5-to-replacement intervals were 0.102, 0.082, and 0.080 seconds. These
are individual lifecycle observations, not controlled latency or travel
benchmarks. The screenshot path still exits with `process::exit`; only the
separate normal-quit scenario supports the shutdown assertion. Two initial
harness attempts sent unfocused synthetic F5 events without observing a reload;
they remain recorded as failures, including the second attempt's explicit
SIGTERM cleanup. The successful harness focuses the exact game window and uses
normal key injection. No game code or assertion was weakened to pass input.

All launched game processes have exited. The original world's 59 files have
identical before/after SHA-256 inventories. No full migration acceptance is
claimed: guest renegotiation, shared client and authority owners, remaining
world/app/content/generation decomposition, dependency/complexity tools,
controlled performance comparisons, and complete visual requalification remain.
