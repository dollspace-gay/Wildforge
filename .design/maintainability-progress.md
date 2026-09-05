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
| AC-4 common client | Shared admission, mappings, entity reconstruction, terrain ordering, roster, content publication, and domain updates implemented; earlier source has focused/QUIC evidence | Finish request and replica ownership; final transfer/reconnect/native qualification |
| AC-5 player operations | Shared stall, craft/repair, conservation effects, and armor exchange implemented | Remaining action domains; final rejection/conservation/adapter parity tests |
| AC-6 world domains | Domain fields/methods inventoried | Encapsulation, explicit transaction coordination |
| AC-7 generation stages | Explicit chunk stages, intermediate maps, independent queries, and atlas climate/weather/geology/hydrology modules implemented | Final pinned output and diagnostic artifact checks and worker-order checks |
| AC-8 app/content/render/tools | Registry definitions, schemas, linker, graph validation, runtime publication gate, and script preparation implemented | App/UI/render/tool owners; final publication/runtime/codec checks |
| AC-9 source size | Advisory 400/500 report and baseline | Reduce migrated debt; explain any real exceptions |
| AC-10 clone analyzer | 18 tests passed in planning; complete JSON snapshot | Maintain analyzer coverage as tooling evolves |
| AC-11 dependency boundaries | Written import/alias/glob allowlist report and Clippy function-report implementation added, unverified | Violating fixtures, resolution/noise review, complete compiler reports, and remaining owner restrictions |
| AC-12 tests/performance | Baseline suite passed; extraction focused gates passed | Full slice gates and reproducible measurements for affected paths |
| AC-13 shutdown | Terrain, mesh, encoding, creation, entry, and homeland work have joined owners and failure tests | Dedicated shutdown; graphical close/capture and save-failure runtime proof |
| AC-14 migration record | This file and separate baseline commits | Add implementation/result entries after each slice |
| Folder guidance | 64 directories previously verified; new player_ops, worldgen, registry/schema, registry/linking, script, planet_atlas/climate, planet_atlas/geology, and planet_atlas/hydrology, planet_atlas/codec, and planet_atlas/storage, and planet_atlas/water_cycle, and planet_atlas/biomes, and diagnostic/catalog/site directories include guides | Final coverage and content identity checks after coding |

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

## Full visual campaign rebuild (in progress)

The latest GitHub run at `6f1a442` finished with 1,087 subsystem passes and the
same five visual-source freshness failures; the six other jobs passed. The
failures reproduce locally. A full native GPU campaign is being rebuilt against
the clean `main` baseline `8c1ec40`, using an isolated seed-20260802 world and
separate saved inputs for each process. The previous August evidence stays in
place. No qualification is accepted at this checkpoint.

Category A (root cause): the validator now selects a separate complete campaign
directory and fingerprints newly introduced source modules as well as existing
ones. Schema 4 makes the campaign date explicit. The existing numeric gates and
92-capture matrix remain required, together with both motion walkthroughs.
Category B (local correctness): metric builders accept the actual baseline and
candidate revisions instead of fixing every later report to August's binaries.
The new driver records clean executable identities, isolated fixture inventories,
raw artifact hashes, native GPU identity, and failed attempts. It refuses to
publish incomplete or stale qualification.

Development validation: three new Rust provenance tests pass, including Rust /
Python fingerprint agreement; all 32 Python tests pass. Format, strict Clippy,
Rust 1.95 checking, folder guidance (57 directories), and advisory analysis pass.
The complete subsystem, agent, doctest, and release checks follow the accepted
capture campaign. Work is isolated from the unfinished client-session slice.

Campaign harness corrections (Category B): use the declared geode report
directory, observe the native initial-upload settlement event, and isolate X11
input from desktop shortcuts. Diagnostic motion probes confirmed discrete
NVIDIA Vulkan rendering, actual F2/WASD delivery, and successful normal shutdown
for both scenes. Failed and superseded probes remain in ignored working storage;
they are not accepted qualification. Input tracing remains opt-in, and numeric
gates and ordinary gameplay behavior are unchanged. The final full campaign must
use the clean revision containing these corrections.

The `e949183` campaign completed all 92 captures and both 28-frame motion walks;
every numeric qualification passed. Rust validation then exposed the remaining
hard-coded RTX 3090/DX12 and old content identity. Its generated publication was
retained in ignored working storage instead of being committed as a passing
result. Category A: native identity now belongs to the campaign foundation and
is required consistently across baseline, candidate, geode, and closeout captures;
the located site's content identity must agree too. Discrete hardware and the
existing numeric budgets remain mandatory. A new clean source campaign follows
validation of this correction against the complete recorded dataset.

The same full validation also identified six rain scenes captured as clear:
the diagnostic category `precipitation` must be translated to the game's `rain`
input. The driver now checks actual world, render, and weather axes immediately
after capture. Six fresh diagnostic rain captures pass both readability gates,
and the entire Rust manifest validator passes against that completed diagnostic
dataset and its original source snapshot. Two native-identity regression tests
and all 35 Python tests pass. The temporary external-dataset diagnostic hook was
removed before committing; its logs and captured inputs remain in working storage.

## Native GPU campaign qualified — 2026-09-05

The complete replacement campaign passes against clean candidate `74fec843`
and clean `main` baseline `8c1ec408`. Its 92 native captures comprise 52 visual
scenes and 40 timing samples, with no retries in this final run. All captures
record the same NVIDIA GeForce RTX 5070 Ti Laptop GPU, discrete Vulkan backend,
and atlas content identity `adbd53f4372ff5f0`. All six rain scenes record actual
precipitation. Both independently opened geode fixtures retain balanced material
accounting, successful save/reload, and unchanged source inputs.

The foundation repeatability comparison, both strata readability reports, both
geode composition reports, all four performance reports, and motion qualification
pass their existing budgets. Recorded median differences in milliseconds are:

| Comparison | Draw difference / budget | Simulation difference / budget |
| --- | --- | --- |
| Strata candidate vs baseline | +0.080128 / 0.30 | -0.10 / 0.10 |
| Closeout strata vs baseline | -0.014620 / 0.30 | +0.12 / 1.00 |
| Opened vs sealed geode | +0.005883 / 0.20 | -0.032965 / 0.10 |
| Closeout opened vs sealed geode | -0.031134 / 0.20 | -0.022040 / 0.10 |

Strata simulation differences use the verifier's existing 0.01 ms precision.
These are recorded game draw/simulation observations, not GPU timestamps or a
general streaming benchmark. No builds or other campaign analysis ran during
the timing interval.

Both native WASD walks completed normal shutdown and supplied 28 F2 frames each.
All 56 frame hashes were checked and all frames were inspected in ordered contact
sheets, with representative approach/return frames also inspected at full scene
size. The reviewed samples show no shimmer, unloaded chunk faces, or abrupt
reveals. Maximum static-hold luminance differences are 0.000197405 for strata
and 0.003714961 for the geode, below the unchanged 0.050 budget. Water and torch
animation remain visible. This is sampled motion evidence, not continuous video
or an unbounded travel test.

The selected [campaign records](../screenshots/qualification-20260905/README.md)
include 205 small metadata, report, execution, and guidance files. Original
August evidence and failed/superseded attempts remain intact. Frozen binaries,
fixture inventories, raw images, input traces, and process results remain under
`target/maintainability/visual-campaign-20260905/campaign-5/`. The captured source
revision is retained in branch ancestry. All eight visual tests now pass,
including the five previously failing GitHub checks and the stale/incomplete
evidence rejection test.

Final local validation passes: format, strict all-target/all-feature Clippy,
Rust 1.95 MSRV, release build, all 35 Python tests, all 1,097 subsystem tests
(24 ignored, 15 agent scenarios filtered), all 15 serial agent scenarios, and
doctests (zero defined). The subsystem suite completed in 156.47 seconds. This
includes the three source-provenance and two native-identity regressions.
Folder guidance covers all 60 maintained directories with zero problems;
advisory maintainability analysis and whitespace checks pass. Publishing the
evidence and this record leaves the captured source fingerprint unchanged.

Broader migration work remains open: guest negotiation, shared client/authority
ownership, remaining domain and UI decomposition, dependency/complexity tools,
remaining shutdown cases, and controlled streaming/travel performance coverage.

## Shared guest content and snapshot ownership (in progress)

Phase 3 resumes from the qualified `a73df389` checkpoint. The parked four-file
client-session draft was restored after verifying its saved SHA-256 inventory;
the preserved copy remains in ignored working storage. This is new migration
work, so the preceding GPU campaign remains evidence for its recorded source,
not qualification of this changed source.

Category A (root cause): `client_session::ContentMap` owns the host's block/item
names, resolved local IDs, and the registry that defines them. Both graphical
and agent adapters use it for block updates, inventory/cursor/armor slots,
container snapshots, held-item presentation, and loose items. Host-owned IDs,
quantities, instance identities, and the existing loose-item durability clamp
are preserved. The unused reverse block map and guest remapping helpers in the
authoritative host module are removed. Local graphical content reload now
rebinds the retained host names to the new registry; previously its host ID
tables remained bound to the old local definitions.

`client_session::Snapshots` owns all five independent incoming stream receivers.
Both adapters replace them on Welcome; presentation interpolation and agent
navigation/trails remain in their respective consumers. Snapshot reconstruction
moves out of wire DTO definitions, preserving the transport/session dependency
direction. The existing packet representation and batching remain unchanged.

The mapping checkpoint passed five focused regressions, strict all-target/
all-feature Clippy, all 25 multiplayer scenarios, and all 15 serial agent
scenarios. Logs are under `target/maintainability/client-session-palette-*`.
Call-site inspection with
`rg -n '\b(block_remap|item_remap|block_map|item_map|host_block|local_item)\b' src --glob '*.rs'`
finds no remaining copied adapter mappings or host remap helpers. The new
module folder has both guides; coverage checks report 61 maintained directories
and zero documentation problems.

Inspection of the shared receiver exposed stale-single-packet, duplicate,
fragment-layout, and sequence-wrap defects. Six new regressions all failed
against the original receiver after its structural move; their exact results
are retained in `client-session-snapshot-regressions-before.log`. The ordering
correction follows as a separately reviewable change. No public crate API,
dependency, MSRV, save/wire format, or generation algorithm changes are part of
this extraction. Full admission/replica ownership, protocol failure handling,
new native guest evidence, and final full Rust/GPU gates remain outstanding.

## Snapshot generation correction

Category B (local correctness): the shared receiver now represents collecting
and applied generations explicitly. Single-packet and fragmented snapshots obey
one freshness rule. Old or duplicated generations cannot roll replica state
backward, a duplicated fragment cannot overwrite the first copy, and conflicting
fragment counts cannot mix layouts. Invalid part indices or zero part counts
are rejected before replacing valid pending work. Sequence comparisons accept
forward progress across the host's wrapping `u32` counter and reject an ambiguous
half-range jump. Only one incomplete generation and at most 255 fragment slots
are retained; the common single-packet path adds no assembly allocation.

All six regressions that failed on the original implementation now pass, along
with the five content-map scenarios and the independent-stream/reconnect test:
12 focused passes. Strict all-target/all-feature Clippy, Rust 1.95 checking,
format, all 25 multiplayer scenarios, all 15 serial agent scenarios, and advisory
analysis pass. Logs use `target/maintainability/client-session-snapshots-*`.
The new authored Rust modules range from 10 to 158 lines. This changes acceptance
of stale, duplicate, or malformed incoming data, while retaining the existing
protocol, batching, latest-wins policy, and valid snapshot payloads.

Call-site inspection with `rg -n 'SnapshotAssembler|snapshots\.' src --glob '*.rs'`
shows both guest consumers routed through `client_session::Snapshots`, plus
focused and existing serialization/reassembly tests. `net` no longer depends on
or exports the client receiver. Full session admission and replica extraction,
content transfer failure handling, broad Rust gates, and new native runtime/GPU
qualification remain open; the full migration is not accepted at this checkpoint.

## Shared admission and decoded terrain readiness

Both guest adapters now use `client_session::Admission` for Welcome, manifest
validation, decoded terrain readiness, the one-time EntryReady acknowledgement,
host acceptance, idle timeout, and closure. The graphical policy still requires
its entry mesh upload; the agent requires terrain only. Existing decode budgets
(eight graphical chunks and two agent chunks per pump), wire messages, and the
15-second idle threshold are preserved. Repeated matching manifests are
deduplicated and recheck residency; conflicting, empty, wrong-spawn, and
out-of-phase manifests close admission.

Category A (root cause): readiness now depends on actual replica residency after
decoding, not receipt of a chunk message. Both adapters previously removed a
required chunk even when its decoder rejected the payload. New Welcome clears
the agent's queued terrain, and disconnection during preparation becomes an
immediate refusal instead of waiting for the connection timeout. `thiserror`
2.0.19 is now a direct dependency for typed admission errors; this exact version
was already in Cargo.lock, and no resolved package version changed.

All 21 focused session tests pass. Three new real QUIC regressions verify that
malformed entry terrain sends no acknowledgement, a new Welcome cannot reuse
the previous world's readiness, and preparation disconnect is reported once.
They reuse the owned stage fixture in a documented `src/tests/agent/` folder and
use ordered message barriers to establish delivery. Strict all-target/all-feature
Clippy, Rust 1.95 checking, format, 25 multiplayer tests, all 18 serial agent
tests, advisory analysis, and folder guidance checks pass. New admission and
wire-test modules remain below 400 lines. Logs are under
`target/maintainability/client-session-admission-*`.

This is a local checkpoint. Full session/replica ownership, shared content
transfer failure handling, full subsystem/release gates, and new native guest
and GPU evidence remain outstanding. The campaign recorded above still
qualifies only its own source revision.

## Guest session lifetime and paced terrain owner

`GuestSession` now owns content mapping, snapshot reconstruction, admission, and
the terrain inbox for both adapters. Welcome replaces all four together. Closure
discards queued mutations; pre-Welcome or closed admission cannot queue terrain.
The inbox retains host IDs until application so pending block updates use the
current registry after a reload. Graphical request bookkeeping clears on Welcome.
The former unpaced single-chunk storage wrapper is now test-only; both production
adapters decode through the shared batch owner, at their existing budgets.

Category A (root cause): block updates collected earlier in the same poll could
previously outlive Welcome and mutate the replacement world. Keeping them in the
session closes that lifetime gap alongside queued chunks and snapshot generations.
The agent's duplicated Chunk/BlockSet dispatch was removed. Graphical player-state
application receives only its content map instead of the entire remote adapter.
Interpolation, navigation, input cadence, and mesh upload remain consumers.

All 27 focused session tests, 18 serial agent tests, 25 multiplayer tests, strict
all-target/all-feature Clippy, Rust 1.95 checking, format, advisory analysis, and
the 62-directory guide check pass. Six new tests exercise coordinated reset,
paced decoding under both presentation policies, chunk/edit ordering within an
adopted batch, pending updates across content reload, closure, and rejected
payloads through real replica storage. Logs use
`target/maintainability/client-session-owner-*`; the new owner, inbox, and test
modules are under 400 lines.

This consolidates protocol lifetime and terrain application, not the complete
replica domain. Entity replication, shared outgoing requests/events, content
transfer failure handling, explicit authority types, and new native runtime/GPU
qualification remain open. Full phase acceptance is not claimed.

## Terrain snapshot and edit ordering correction

Three new regressions failed against `421cf63`: an edit following a chunk beyond
the decode budget disappeared, an older edit replayed after a newer snapshot,
and alternating snapshot/edit sequences changed outcome with the decode budget.
The failing output is retained in `client-session-terrain-order-before.log`.

Category A (root cause): the terrain inbox now keeps a single ordered mutation
queue. A paced chunk retains the edits behind it until it can decode; a newer
snapshot follows earlier edits. Consecutive chunks and their following edits
still use batched lighting. Chunk budgets remain two for agents and eight for
graphics. This corrects update ordering without changing payloads, decoding,
authority, or registry identity. Edits behind a queued snapshot can now wait for
that snapshot's paced adoption, which needs runtime latency qualification.

All three formerly failing regressions pass, together with the remaining
session tests (30 total), 18 serial agent tests, 25 multiplayer tests, strict
Clippy, Rust 1.95 checking, format, advisory analysis, and folder guidance.
Logs use `target/maintainability/client-session-terrain-order-*`. Full subsystem,
release, and new native guest/GPU evidence remain outstanding for this source.

## Agent mob snapshot field correction

Category B (local correctness): the agent's mob reconstruction now preserves the
host's health and hurt fields, matching the graphical guest. The real QUIC
regression failed first with local health `0.0` for host health `7.25`; the
constructor default had never been replaced. Its failing log is
`client-session-entity-fields-before.log`. All 19 serial agent scenarios and
strict all-target/all-feature Clippy pass after the two-field correction
(`client-session-entity-fields-{agent,clippy}.log`). This fix is separate from
the forthcoming entity receiver/conversion extraction; broader source and GPU
qualification remain outstanding.

## Shared entity receiver and conversion owner

`client_session::EntitySnapshots` replaces the raw receiver holder with shared
reconstruction of mobs, projectiles, loose items, and falling blocks. Both
adapters consume the same local values. The session exposes typed stream methods
instead of mutable receiver access, and streams cannot apply before Welcome or
after closure. Unknown species/items are omitted, unknown blocks use the bound
registry's placeholder, and host instance IDs and payload fields are preserved.
Replica projectiles retain zero damage and no drop/payload authority.

Graphical interpolation remains in its consumer. `Mob::present_replica_at` keeps
the presentation yaw, initial facing, and animation phase consistent with the
former construction path without exposing private mob fields. Agent navigation
and its existing selection of observed entity kinds are unchanged. The prior
health/hurt correction remains separately recorded in `ff6ccca`.

All 34 focused session tests, 19 serial agent scenarios, 25 multiplayer tests,
23 mob tests, and 14 archetype tests pass. Strict all-target/all-feature Clippy,
Rust 1.95 checking, format, advisory analysis, and folder guidance also pass.
Logs use `target/maintainability/client-session-replica-*`. New replica and test
modules contain 107 and 247 lines; the session owner contains 179 lines.
World-domain authority separation, shared events/outgoing requests, content
transfer failures, and new native guest/full GPU qualification remain open.

## Shared host fixture and native guest proof (in verification)

The deterministic QUIC stage moved from the oversized agent test module into
`src/tests/fixtures/host.rs`, with explicit ownership of its simulation thread
and transport. Existing agent tests and native graphical tests share it. The
stage's terrain, seed, spawn, wildlife/weather policy, and simulation cadence
are unchanged. The two new maintained directories include local guides.

The hardware gameplay harness now includes graphical guest admission, a real
uploaded entry mesh, a second ordinary agent, shared roster/chat, movement
through the graphical frame update, receipt of movement by the host, a renderer
screenshot, and normal guest disconnect/worker release. The runner's optional
`--output` preserves the scenario report, screenshot, exact binary SHA-256, and
exit status in a new directory. It refuses an existing destination. Only these
proof artifacts are copied; disposable identities and saves are not exported.

Preflight passes: strict all-target/all-feature Clippy, Rust 1.95 checking,
19 serial agent scenarios, all 35 Python tool tests, format, advisory analysis,
and 64-directory guidance coverage. Logs use
`target/maintainability/client-session-native-*`. The native scenario has not
yet run at this checkpoint; it follows from a clean committed source. This
functional stage proof does not replace production GPU/performance qualification.

Native attempt 1 at `d794ba7` used the NVIDIA RTX 5070 Ti / Vulkan adapter and
passed all four depot cases plus graphical entry/mesh checks. It then failed
because the fixture's synthetic agent name contained an underscore, rejected by
the existing display-name validator. The fixture name is corrected to letters
only; production admission rules are unchanged. The failed attempt, exit status,
and binary hash remain in `target/maintainability/client-session-native/attempt-1`
and `client-session-native-attempt-1.log`.


## Guest transport ownership and ordinary disconnect

The guest endpoint, authentication flow, and stream tasks moved to
`src/net/client.rs` in `f86f776`, preserving the transport facade. The following
correction is separate from that move.

Native attempt 2 at `0a94502` passed the four depot cases, graphical entry and
mesh upload, shared roster/chat with the ordinary agent, movement, host receipt,
and screenshot capture on NVIDIA/Vulkan. It failed ordinary disconnect: the
host still retained the viewer after five seconds. Artifacts and exit status
remain in `target/maintainability/client-session-native/attempt-2` and
`client-session-native-attempt-2.log`; this attempt is not a passing proof.

Category A (root cause): dropping the client queued Bye then immediately
stopped the runtime, racing delivery of both queued messages and QUIC closure.
The client now owns its endpoint and three stream handles, queues a writer
finish after outstanding messages, drains QUIC with the runtime alive, then
cancels/joins remaining stream tasks. Each network wait has a two-second
deadline; failures are reported. Protocol and authentication are unchanged.
The reconnect scenario now exercises ordinary Drop instead of explicitly
sending Bye while keeping the client alive. A regression checks ordered chat,
Bye, and host departure through the real transport.

Before the revised verification schedule below, the new drop scenario, all
25 multiplayer and 20 serial agent scenarios, net tests, strict Clippy, Rust
1.95 checking, formatting, advisory analysis, and directory guidance passed.
Logs use `target/maintainability/client-session-close-*`. Native attempt 3 and
full source/runtime qualification remain pending.

## Implementation and final verification schedule

The user requested that all remaining refactor coding be completed before
running tests. Subsequent implementation checkpoints defer test execution,
format/lint/build gates, native proof, and GPU qualification to the final
verification stage. A targeted check may be needed to unblock implementation;
any such exception will be recorded. Existing evidence retains its original
source scope. Unverified implementation is not an accepted phase or passing
campaign, and outstanding acceptance criteria stay open.


## Remaining guest state consolidation (implementation, unverified)

The shared session now owns presence records and synchronous time, weather,
arcane, sign, and switch replication. Both adapters retain their presentation
policies while using the same mutation bodies. Welcome clears roster state,
render interpolation/held models, and agent breadcrumbs together with terrain
and stream generations; departure retires all graphical player maps.

Category A corrections: transferred mod files previously ignored write/load
failures and accepted absolute path escapes. Both consumers now use one staged
content publication operation with portable relative path and duplicate checks,
host-script rejection, registry validation, an exclusive writer workspace,
rollback, and observable recovery failures. Cache paths and legacy hash inputs
are preserved. Invalid transfer or out-of-phase transfer closes admission, and
the agent stops processing that poll rather than accepting a later Welcome.
Partial content is never assigned to the live registry. Stale workspace cleanup
and concurrent readers of the legacy shared agent cache remain review items.

No new checks have run for this checkpoint under the requested coding-first
schedule. Final verification must cover valid/invalid transfer, rollback and
path remapping, entry failure, repeated Welcome, roster departure, both replica
consumers, and the full applicable Rust/native/GPU gates. This checkpoint does
not close Phase 3 or its acceptance criterion.


## Shared stall transaction (implementation, unverified)

`player_ops::trade::purchase` now owns the stock/payment/till rule previously
copied in local and network purchase paths. It validates before mutation,
transfers one good, removes the full payment, inserts the purchase into the
buyer inventory, and returns overflow for each adapter's existing drop path.
Reach, structure checks, network seller/moderation checks, presentation, and
state echoes retain their existing order and policy. The new directory has
local guides and explicit imports. This extraction preserves the old payment
matching behavior; identity-bearing barter and malformed stack handling need
separate correction and final verification. No checks run for this checkpoint.


## Barter preserves physical payment instances (implementation, unverified)

Category A: the extracted legacy purchase copied the seller's price template
into the till while deleting buyer items matched only by definition. That loses
buyer durability/Current identities and can duplicate the template identity.
The transaction now stages the buyer and six till slots, transfers actual buyer
stacks, rejects malformed/non-singular durable stock, and commits only when the
whole price fits. Normal currency retains first-fit placement; different durable
instances need separate till slots. Insufficient funds/capacity leave all live
state unchanged. Sale overflow remains the adapter's ordinary delivery path.
This correction is separate from the preceding shared-body extraction. Identity,
conservation, capacity, and local/network parity tests are deferred to the final
verification stage as requested; no acceptance claim is made here.


## Shared crafting, repair, and equipment rules (implementation, unverified)

Local and network crafting now call `player_ops::craft` for repair matching,
blueprint eligibility, cursor capacity, grid consumption, and a single owned
sequence of charged-input retirement, recipe-loss accounting, secondary output,
and overflow burial. The outcome is consumed when effects finish. Local tech
unlock checks, XP, script callbacks, guest prediction policy, and network echoes
remain adapter responsibilities. Existing local/network overflow reason strings
are retained. Both equipment adapters use one armor/charm eligibility and cursor
exchange operation; the local loadout still returns components first.

No tests or build/lint gates run for this checkpoint. Final verification must
exercise repair identity, recipe/blueprint rejection, cursor capacity, Current
retirement, material loss/byproducts, full inventory, armor/charm eligibility,
and local/network parity before acceptance. Other player-operation domains and
world authority separation remain open.


## Deterministic generation stages (implementation, unverified)

The 3,186-line `worldgen.rs` is now an explicit stage coordinator and generation
context/value declarations. Stage modules own shape/strata preparation, caves,
surface rules, mineral deposition, vegetation, province structures, and finite
water/salt finalization. Named ShapeColumns and SurfaceColumns carry the same
intermediate arrays between stages. Deep generation still returns an empty
chunk, and the original stage, iteration, seed-salt, and final bedrock order is
retained. Supporting climate/province/relief/hydrology/density/strata/prospecting
queries moved behind explicit imports and generation-scoped helper visibility.
Pure heart form/height lookup moved out of authoritative world code; its old
world facade remains a re-export. Generation no longer calls world simulation.

New authored stage/query modules are below 400 lines before final formatting;
the parent is around 415 lines, retaining cohesive context and public value
types within the soft 500-line review limit. Every new directory has guides.
No checks run under the coding-first schedule. Pinned chunk/atlas byte and hash
comparisons, worker-order scenarios, full Rust gates, and actual GPU/runtime
qualification remain required. Atlas internals and registry decomposition are
still outstanding; Phase 6 is not accepted by this implementation checkpoint.


## Registry definitions and graph owners (implementation, unverified)

Registry domain definitions moved into cohesive block, item, material/magic,
fauna, narrative, recipe, and structure modules. Existing type names remain
available through the registry facade. Material fixed-point inference/balance,
derived salvage, arcane transformation validation, and ecology/scar validation
now have separate owners with explicit imports and the same call order.
Runtime policy queries and saved-placeholder reconstruction are separate from
loading, and the runtime lookup module no longer uses a broad parent import.

New modules remain below 400 lines before final formatting. This is a structural
checkpoint with no tests/build/lint execution under the requested schedule.
Raw TOML schemas, loader/linker stages, atomic registry publication, and their
final remap/hot-reload/determinism verification remain open; the registry and
full refactor are not accepted yet.


## Raw registry schemas and loader (implementation, unverified)

Raw deserialization types and provider bundles now live in `registry/schema/`,
organized by content domain with separate file envelopes. Schema fields and
helpers are visible within the registry boundary; runtime definition names stay
on the existing facade. `loading.rs` owns file parsing, embedded base documents,
provider dependency ordering, and dispatch into linking. Its include paths were
adjusted for the source move to retain the same base files. Raw field names,
serde defaults, compatibility alternatives, load order, and WORLD_API_VERSION
are unchanged by this extraction. New schema directory guidance is included.

No tests/build/lint execution at this checkpoint. Linking passes and atomic
publication remain implementation work; final tests must cover every schema
family, bad providers, dependency order, content hashes, runtime ID ordering,
and hot reload/remap. The requirement audit above now distinguishes these
unverified implementations from the older validated source evidence.


## Typed content interpretation (implementation, unverified)

Material classification/salvage, observation/discovery, and declarative magic
interpretation moved into registry-scoped modules. They consume the extracted
raw schemas, retain the existing qualified-name rules and validation errors,
and return runtime definitions without game-state mutation. The new linking
directory has guides. Remaining registration passes still live in the parent;
no checks run and final schema/registry/runtime qualification remains pending.


## Registry registration owner (implementation, unverified)

Per-provider registration now has one owner for texture allocation and deferred
references. It registers blocks (including their held forms), then authored
items, then remaining references/appearance inputs in the original provider
order. Texture paths and names accumulate privately and join the registry before
later name resolution. Named pending animal/NPC records replace long internal
tuples; the pending-content output carries the unresolved references explicitly.
The original built-in air registration and unknown-block insertion stay on either
side of this pass, preserving runtime ID assignment order.

The new registration modules remain below 400 lines before final formatting.
No tests or other gates run. Final verification must compare complete block/item
and texture ID order, provider errors, embedded texture behavior, hot reload, and
content hashes. Remaining name-resolution passes, publication, and full phase
acceptance are still open.


## Ordered registry linker (implementation, unverified)

The remaining registry build body now has an explicit coordinator in
`registry/linking/mod.rs`. Each domain consumes its own deferred declarations;
qualified lookup rules, engine sentinels, structure/settlement wiring, station
and shaped recipes, species/NPCs, narrative rewards, features, modes, and optional
capabilities have separate modules. Registration IDs, resolver sequence, and
diagnostic append order are retained. The facade contains runtime definitions
and re-exports. Existing inline schema/NPC tests moved unchanged into adjacent
files while retaining their original test module paths.

No validation gates run under the requested coding-first schedule. This
structural checkpoint intentionally retains existing duplicate-species prey
linking and skill-error lifetime behavior for a separately reviewable correction.
Final checks must cover those regressions as well as complete registry IDs,
diagnostics, hash/remap behavior, and startup/hot-reload publication.


## Rejected content keeps correct ownership and diagnostics (unverified)

Duplicate wildlife declarations now skip before queuing prey links. Previously
the discarded definition retained the next species index, so its prey could
attach to a later species or index beyond the final roster. Accepted species
still receive their original IDs and prey-resolution order. Skill resolution
errors now survive material-graph reconciliation through a deferred error
vector, so malformed skills remain actionable pack failures.

These corrections are separate from the preceding structural extraction. Tests
are deferred: final cases must include duplicate hunters at the end/middle of a
roster, later NPC companions, first-definition retention, and skill failures
through both content inspection and validated startup/reload.


## Complete content publication gate (implementation, unverified)

`registry/publication.rs` now owns aggregate provider/material/arcane diagnostics
and live material/accepted-quest compatibility checks. Runtime loading returns
a validated candidate or an error; content inspection retains rejected records
for the menu. World opening and creation validate before storage/generation,
and guest entry/content transfer, dedicated startup, and content-dependent CLI
operations use the shared gate. Missing optional files remain allowed; unreadable
files or directory scans now produce rejected-content diagnostics instead of
silently omitting content.

Reload compiles its whole script set against the existing sandbox before changing
the registry, atlas, inventory, or world. A failed candidate leaves the running
ASTs and content intact. Successful installation retains KV and queued commands.
The existing per-mod previous-AST fallback API remains for its other callers.
Local world entry checks startup script errors while allowing menu inspection.
The new `script/` directory includes local guides. Ledger persistence failures
after an accepted live remap remain reported by the existing save path.

No tests, builds, formatting, or lint gates run for this checkpoint. Final cases
must cover every error family, failed optional-file/directory reads, no save or
atlas generation on rejected startup, invalid reload preserving all live state,
script compilation/missing-file failure, retained KV/command order, successful
remap, and both guest entry paths. Implementation does not establish acceptance.


## Atlas climate and weather ownership (implementation, unverified)

The climate facade now retains constants, public exports, and PlanetaryWeather's
private grids/cursor/checkpoint/lifecycle. Astronomy, circulation, conservative
transport, seasonal moisture relaxation, and immutable normals have separate
modules. Weather cell preparation moved into a method that reads the prior grid;
the explicit slice coordinator preserves cell order and publishes through one
completed-hour operation. Inbound/surface transfers, grid swaps, salt settlement,
daily groundwater, basin levels, and the final conservation gate keep their
original sequence. Ecology, detailed/portable custody, industry, basins, and
sampling moved with their existing operations and explicit imports.

No new tests or other validation gates run. The extraction retains arithmetic,
seed salts, physical constants, solver tolerance, and failed-hour latching by
construction, but byte/hash and behavioral equivalence remain unverified. Final
cases must compare pinned atlas output, varied slice budgets, failed-hour rollback,
external transfers during a partial hour, water/salt conservation, and seam-aware
weather observations. Geology, hydrology, and remaining atlas infrastructure are
still outstanding. The new climate directory has local guides.


## Atlas geological genesis modules (implementation, unverified)

Geology now separates stable classifications/records, spherical site math,
plate/craton motion, tectonic fields, volcanism, relief, strata, provinces,
finite deposits, validation, and sampling. The parent retains the persisted
model and bounded deterministic attempt selection. One attempt still executes
the same stages and acceptance constraints; the original compact-fixture policy
and cancellation points remain intact. Decoder and validation visibility retains
the atlas boundary, and the existing rotation probe keeps its public test path.

No validation gates run under the coding-first schedule. Final evidence must
compare atlas bytes/hashes, rejected-attempt histories, site IDs and finite
budgets, read-only samples, rotated/seam boundaries, and codec compatibility.
The new geology directory includes guides. Hydrology and the remaining atlas
infrastructure still need implementation; this checkpoint is not acceptance.


## Static hydrology domains (coding checkpoint; unverified)

Hydrology now separates deterministic priority flooding and flow accumulation,
climate runoff, erosion, supported basin shaping, oceans, lake candidates and
outlet resolution, watersheds, river records, channels, model validation,
read-only sampling, and finite placer routing. The parent retains the ordered
generator and final dense-cell water accounting. Public records, enum layouts,
seed salts, tolerances, loop/tie-breaking order, and floating-point expressions
are preserved. New folder guidance describes graph and conservation boundaries.

This is a local coding checkpoint. Formatting, compilation, all tests, pinned
atlas/codec comparisons, and GPU/runtime qualification remain deferred until
coding is finished, following the current requested validation schedule. No old
CI or qualification result is evidence for this source.


## Atlas infrastructure owners (coding checkpoint; unverified)

The atlas facade now separates canonical addressing/dense grids, immutable
cells, dynamic atmosphere storage, sparse history, generation configuration,
identity, explicit generation stages, sampling, and validation. Compatibility
rules live with the manifest schema. Codec modules own checked primitives,
fixed-width layers, containers, sparse TOML encoding, and fingerprints. Storage
modules own new bundles, mutable checkpoints, paired backup recovery, and
arcane checkpoint integration. Public names, persisted field order, versions,
operation order, and stage evidence expressions are retained. Remaining domain
parents now use explicit imports. New codec/storage folders include guides.

Atlas-scoped scratch-buffer access remains an internal migration seam; complete
world-domain encapsulation is still outstanding. Existing mutable checkpoint
and recovery behavior is retained here, not claimed to provide stronger crash
atomicity than before. Known bounded-stencil traversal needs a separate bounds
correction: queued cells can exceed the requested output limit. Validation and
new regression scenarios remain deferred until the coding phase is complete.


## Bounded stencil output correction (coding checkpoint; unverified)

`bounded_stencil` now stops consuming queued neighbors when its output reaches
`max_cells`. Previously the limit stopped expansion but allowed the remaining
queue to append extra cells. Breadth-first neighbor order and radius semantics
are retained; capped queries return the intended prefix. This is separate from
the atlas move. Final regressions must cover zero/one limits, queued overflow,
face seams, and unchanged output when the cap does not bind. Pinned generation
and runtime checks remain part of the deferred final validation phase.


## Private atlas storage and owned weather buffers (coding checkpoint; unverified)

`AtlasGrid` now keeps its backing vector private. Weather's atmosphere/water
scratch buffers are complete grids, allocated or cloned with their validated
shape. Completed-hour publication and rollback swap these owners in constant
time; cell mutation uses fixed-length slices. No caller can resize a grid's
storage independently of its dimensions. Weather's arithmetic, transaction
sequence, and buffer allocation count are retained. Full world/weather state
encapsulation remains a later boundary; this change closes only the dense-grid
storage seam. Rollback, slice partition, conservation, and performance checks
are deferred to the final validation phase along with the other coding changes.


## Water-cycle accounting and checkpoint domains (coding checkpoint; unverified)

Exact water/salt mass operations, record schemas, detailed custody transfers,
audits/validation, finite genesis reservoirs, and checkpoint encoding now have
separate water-cycle modules. The existing ledger remains the sole owner of
conserved totals; no wrapper duplicates its accounting. Public records and
operations, units, salt remainder rules, debit order, and persisted bytes are
preserved. New folder guidance records the custody and conservation contracts.

This is structural implementation only. Parcel/rejection/conservation tests,
codec fixtures, backup recovery, strict Rust gates, and fresh runtime/GPU
qualification remain deferred until all coding is finished.


## Biome and country generation domains (coding checkpoint; unverified)

Biome classification, soil/groundwater genesis, water-backed habitat overlays,
country seed selection, traversal/border costs, heart scoring, persisted country
records, validation, and read-only sampling now have cohesive modules. The
parent preserves the ordered generation coordinator and identifiers. Existing
production/fixture policies, seeds, tie-breaking, IDs, numerical expressions,
and habitat compatibility rules are retained. Folder guidance describes their
ownership and physical constraints. Pinned atlas/codec, country coverage and
route/heart checks, Rust gates, and fresh runtime/GPU evidence remain deferred
until the coding phase is finished.


## Atlas diagnostic domains (coding checkpoint; unverified)

Diagnostics now separates ordered dense-cell census collection, sparse model
summaries, report/CSV schemas, map value lookup, shared image encoding, climate
and river transects, isolated weather examples, route export, and memory
estimates. Map definitions are grouped by physical domain with one ordered
registry. Qualification selectors share coordinate serialization and preserve
the existing geology, climate, habitat, country, river/lake, and weather order.
Every new diagnostics/catalog/sites directory has local guidance. No export,
map, site selection, or campaign has been run for this checkpoint.

Selection predicates/ties, artifact names, report fields, colors, layer order,
and dense aggregation order are retained. Final comparison of diagnostic
artifacts, census totals, map registration, qualification coordinates, pinned
atlas output, and fresh GPU evidence remains deferred with the full test phase.


## Generation dependency direction (coding checkpoint; unverified)

Mod file identity/inventory now lives in a transport-independent root module;
network callers retain the same public facade exports, and genesis hashes call
the shared owner directly. The existing characterization tests moved with that
owner, without new cases or execution. Topology's serialized identity now lives
with planet geometry, with the world facade re-exporting its prior name. Atlas
generation and manifest compatibility no longer depend on authoritative world
or network modules for these constants/helpers. Paths, bytes, exclusions,
sorting, and topology values are unchanged. Final identity/API/codec checks
remain deferred with the rest of validation.


## Process command ownership (coding checkpoint; unverified)

The library facade now delegates process dispatch to `app/`. Command precedence
is explicit in one ordered selector, followed by domain adapters for audits,
geography, creation, common-entry preparation, qualification, and runtime launch.
Existing argument interpretation, usage messages, exit statuses, defaults, and
operation order are retained. Window/audio/input imports moved to the graphical
client, whose root no longer imports the whole crate. New app folder guidance
records the boundary. Client child imports and narrower state ownership remain
open. No validation commands ran; CLI contracts, Rust gates, native evidence,
and fresh GPU qualification remain deferred until coding is complete.


## Input, navigation, and cosmetic owners (coding checkpoint; unverified)

Pointer capture/warp state is private to InputState. Its operations receive the
window and camera directly, and both screen changes and focus loss share the
existing held-key/button reset. The navigation owner holds screens and UI state;
its explicit coordinator preserves container-close, cursor/craft return, focus,
and capture order. Cosmetic variation, particles, and gait execute on
PresentationState without access to simulation state. Client construction and
platform event-loop startup now live with their respective adapters. Content
watching and browser/world-name helpers moved to their actual callers' domains.

Field access within other presentation/UI adapters remains an implementation
seam, not full client encapsulation. No validation gates ran. The final phase
must cover input capture/focus/repeat behavior, screen inventory conservation,
cosmetic RNG isolation, startup and hot-reload behavior, native guest play, and
fresh hardware GPU qualification before accepting this checkpoint.


## Text input and platform bridge (coding checkpoint; unverified)

UI text editing now operates on UiState and returns explicit app actions for
world creation and sign submission. Startup fields remain before join/chat;
sign, discovery-label, and search editing remain after chat and before repeated
game-key filtering. Character policies, length limits, release handling, Escape
behavior, and key action order are retained. The keyboard adapter handles the
cross-domain effects, and the platform bridge uses explicit imports.

No new tests or validation commands ran. Final cases must cover each text field,
repeated text versus game keys, submit/cancel paths, focus changes, and native
input/session evidence. The full refactor remains in progress.


## Shared inventory presentation (coding checkpoint; unverified)

Buttons, item slots, and cursor-held stacks now use read-only widget functions.
One inventory layout value supplies hotbar/storage, card/avatar/tabs, and craft
geometry to drawing, clicks, tooltips, and portrait placement. Ordinary inventory
and station restocking use one borrowed storage-grid renderer. The existing
loadout grid keeps its distinct geometry/selection policy; browser placement
and station controls remain explicit at each screen. Paint order, coordinates,
colors, stack counts, and durability bars are preserved by the extraction.

No checks ran. Final vertex/layout characterization, click/hover scenarios,
full Rust gates, and fresh native GPU captures/timings remain required. Larger
screen/action dispatchers and remaining ownership boundaries are still open.


## Transferred content reader lifetime correction (coding checkpoint; unverified)

The exclusive transfer writer protected publication but left existing readers
pointing at the replaceable cache. A later agent/guest transfer could therefore
change assets beneath an older registry. Transfers now prepare a private asset
copy before cache publication; accepted registries and their clones share its
storage owner. The last reader releases that exclusively created directory.
Failed copies/publications release only their own storage, and collisions or
crash leftovers are never automatically reclaimed. Files are copied rather than
linked, so ordinary cache edits cannot change a retained snapshot.

The ordinary cache path and its legacy content hash remain unchanged. This is
a separate ownership correction, with no test/build/lint execution. Final cases
must cover simultaneous readers, cloned registries/jobs, independent transfer
contents, cache edits, failed copy/validation/publication, final-reader cleanup,
and unchanged content hashes. The extra per-transfer disk copy is deliberate;
entry timing and native integration remain in the deferred validation phase.


## Deterministic world shader composition (coding checkpoint; unverified)

The world WGSL source is split at complete declarations into binding, vertex,
shadow, lighting, sky/fog, material, terrain-fragment, diagnostic, line, and UI
units. One compile-time concatenation feeds the renderer and existing Naga/
fog fixtures. No runtime file loading, generated checked-in aggregate, pipeline
change, or CPU rendering path is introduced. Units retain their original bytes
and order by construction; source references and folder guidance are updated.

Only existing test source references changed; no tests or validation commands
ran. Byte equivalence, Naga validation, uniform/binding contracts, and fresh
hardware captures/timings remain required in the final phase. Other renderer
setup/frame/post ownership and remaining client domains are still open.


## Device setup and dedicated shadow sources (coding checkpoint; unverified)

Surface-compatible adapter ranking, hardware-only selection, device limits,
backend diagnostics, and surface configuration now have an explicit constructor.
Renderer setup receives its configured GPU context. Dedicated point-light and
cascade shader sources moved out of Rust literals into authored WGSL files;
their separate module/uniform ABIs and exact source strings are retained.

No build, test, or GPU command ran. Final verification must compare shader
sources, validate all shader modules, check the hardware admission policy, and
produce fresh native frame/capture/timing evidence. Resource/pipeline ownership
continues as implementation work.


## Post-processing resource owner (coding checkpoint; unverified)

PostProcess now owns private pipelines, parameter bindings, samplers/layouts,
and HDR/bloom targets. Setup creates a complete owner, resize replaces its target
set, and frame orchestration calls its bloom/composite operations explicitly.
Only the HDR scene view crosses into world/viewmodel encoding. Exposure/white
point inputs, night grade, half-resolution targets, bloom-off clear, pass order,
and capture replay remain unchanged. The new setup directory has guides.

No validation commands ran. Final checks must cover resize and target lifetime,
bloom off/on, exposure overrides, UI/capture ordering, Naga/pipeline contracts,
and fresh native GPU captures and timings. Other renderer and client/world
ownership work remains in progress.


## Shared qualification file and scalar helpers (coding checkpoint; unverified)

Geode and closeout verification now use the existing shared visual-evidence
converter for byte hashing, TOML reads, atomic report replacement, and scalar
serialization. Typed error classes and each verifier's non-finite diagnostic
remain explicit parameters; nine-decimal floats, ASCII quoting, bool/integer
precedence, recursive lists, read exception causes, and report ordering are
retained. The shared implementation stays in the already-recorded converter
source so the existing converter hash continues to cover it. Different flat,
family/stratum, and nested campaign report layouts remain separate policies.

No Python tests, byte comparisons, reports, or campaigns ran. Final coverage
must compare serialized fixtures, nested scalar failures, read/write failures,
source freshness, and the full newly captured qualification campaign. Larger
tool decomposition and architecture checks remain implementation work.


## Explicit graphical module dependencies (coding checkpoint; unverified)

Production graphical modules no longer import the full parent namespace. Each
names the game state, domain modules, constants, and platform types it uses.
The facade drops aliases used only by those children, and native proof source
imports its dependencies directly. Method bodies and proof cases are unchanged.
This makes dependency checks meaningful without claiming that broad Game state
access has already been replaced by narrow domain interfaces.

No formatting, build, lint, test, or runtime validation ran. Import resolution
and unused imports remain part of the final Rust gates; session, interaction,
world ownership, large dispatchers, and architecture checks are still open.


## Architecture and compiler-backed reports (coding checkpoint; unverified)

An explicit dependency allowlist now covers established worker, terrain,
generation, content, guest, operation, simulation, and renderer boundaries.
A written-token scanner follows nested imports, aliases, re-exports, and globs,
records all resolved edges, and identifies unrestricted parent imports. It is
conservative about cfg and block scopes, reports resolution cycles, and does not
claim macro expansion, type inference, or mutation-effect analysis. Guest World
access remains a documented migration seam. Empty selectors and bad contracts
are errors. CI findings are advisory; strict policy exit is an explicit option.

A separate report uses Clippy's syntax-aware function-line and cognitive-
complexity diagnostics, with 100/25 advisory thresholds. It records toolchain,
command, configuration, compiler completion, locations, and errors, without
keyword approximations. Failed/incomplete compiler runs fail the report. The
ordinary strict Clippy gate remains unchanged. Commands and limitations are
added to the relevant guides; no new directories or runtime dependencies were
introduced.

No analyzer, Python test, format, build, Clippy, workflow, or runtime command ran.
Final validation must add alias/re-export/glob/qualified-path/inline-module and
violating boundary fixtures, malformed-input and empty-selector cases, compiler
failure/timeout/diagnostic fixtures, and a real reviewed noise inventory. These
checks remain unverified and AC-11 is not accepted. World/replica separation,
remaining domain operations and state ownership, large dispatchers, and final
Rust/native/GPU qualification remain open under the complete refactor goal.


## Resident terrain and read contracts (coding checkpoint; unverified)

TerrainRead now bounds resident voxel, metadata, hidden-cell, light, and surface
queries; SceneRead adds local structures for selection. Physics, dropped-item
motion, raycasts, camera collision, and mesh snapshots use those read contracts.
World keeps compatibility methods forwarding shared query behavior. Missing
chunks still read as air/open sky, and hidden cells retain their prior collision
and mesh treatment. No mutable world or generation/save operation is exposed by
the read contracts.

TerrainStore owns the resident map, mesh dirtiness, light solve/cascade, and
WFC6-WFC9 wire reconstruction. Lighting and decoding were moved with their exact
plane order, remapping, legacy fallback, neighbor dirtiness, and visit bound.
World coordinates adoption/persistence through explicit store methods; direct
map access remains test-only. The new terrain folder includes both guides.

No format, compiler, tests, analyzers, or runtime checks ran. Final cases must
cover read parity, hidden cells, physics and targeting, missing chunks, complete
wire fixtures and invalid payloads, light removal across seams, and fresh native
mesh/render evidence. This establishes a shared spatial primitive; separate
replica ownership and the remaining world domains are still being implemented.


## Agent replica and shared spatial observations (coding checkpoint; unverified)

The agent now constructs ReplicaWorld with resident terrain, replicated entities,
and bounded host observations. It no longer constructs World or names a world
save/cache path. ReplicaWorld has no generator, persistence operation, identity
allocator, simulation queues, or conservation ledger. Perception consumes host
arcane observations directly, removing its unreachable authoritative survey path.
GuestSession uses a bounded replication target and TerrainRead for admission;
the graphical World adapter remains temporary, so AC-2/AC-4 remain unfinished.

The immutable temperature field and seasonal formula are shared with generation
and atlas-free authority readings, preserving seed salts, seam sampling, and
fallback temperature. ReplicaObservations owns weather and arcane snapshot rules.
Resident plane writes, edge dirtiness, and support classification moved into the
terrain owner. Replica updates preserve recursive plant/torch/layer removal,
material-change invalidation, and batched light, without minting temporary drops
or scheduling authority simulation. The host still coordinates its existing
logging, carrier retirement, gravity, drops, nests, and machine revalidation.

No validation command ran. Final checks must compare authority/replica reads,
legacy and current wire planes/light, support updates, weather fallback, entity
identity/remapping, admission and reconnect, and native agent gameplay. Compiler
visibility/import checks and existing agent fixture adaptation remain deferred.

## Shared guest transport lifetime (coding checkpoint; unverified)

GuestSession now owns the connected QUIC client and outgoing reliable/datagram
surface. Graphical request adapters and agent work send through that owner;
neither retains a separate client field. Polling and connection state use the
same owner, while admission closure and transport drop timing are unchanged.
Dropping the session invokes the existing reliable drain/endpoint close/task
join. Protocol-only fixtures retain the disconnected constructor.

No validation ran. Final native teardown, queued reliable ordering, movement
datagrams, admission failure, reconnect, and ordinary UI/agent request scenarios
must exercise this lifetime. Graphical replica and remaining ownership work are
still implementation, not accepted phase completion.

## Immutable geography ownership (coding checkpoint; unverified)

Geography now owns the seeded climate fields, relief splines, optional immutable
atlas, and derived province-label cache with their climate/province/relief query
methods. Generator retains its established public query API through explicit
forwarding, and generation stages borrow the same detail fields. ReplicaWorld
uses this query owner without terrain/content bindings or a generate operation;
its observed biome preserves the former atlas-free, ungrafted guest policy.
Open-water classification is shared through TerrainRead.

Source moves retain seed salts, spline points, topology packing, field formulas,
province cache/iteration behavior, and atlas overrides. The new geography folder
has both guides. No validation ran. Final fixtures must compare legacy queries,
chunk/atlas outputs, seams, ties, cache/request order, and worker counts. This
supplies the biome observation needed for graphical replica migration; graphical
runtime ownership and the final campaign remain open.

## Calendar and machine read capabilities (coding checkpoint; unverified)

CalendarView holds copied astronomical inputs and shares sun, daylight, seasons,
moon, and progress formulas; World forwards these observations. Ire bands and
seasonal wants retain one pure implementation. This establishes the same reads
for the graphical replica without transferring calendar mutation to consumers.

BlockRead now separates shape/stat/capability and machine-recognition inputs
from BlockStore mutation, ledger, and output operations. World and local
structures implement the split contracts with unchanged method bodies. Named
machine, glassworks, and stall checks have shared read-only entry points, so
replica UI need not implement physical mutation just to inspect a shape.

No validation ran. Final Rust/API checks must update applicable trait imports
and test fixtures, and verify calendar boundaries, machine recognition/rotation,
structure/world parity, and unchanged firing/accounting. Graphical ownership
migration continues; these source checkpoints do not accept its criteria.

## Guest eviction must not settle physical blocks (corrective checkpoint; unverified)

Source inspection for the replica migration found that World::evict_chunks ran
settle_falling even in guest mode. That calls the physical falling tick and can
plant blocks locally while merely unloading distant terrain. Eviction now settles
falling blocks only for authority; guests retain the host's falling snapshots.
This behavior correction is committed separately from graphical ownership moves.

No validation ran. The final regression must evict guest chunks with a falling
snapshot and prove no local landing/block edit occurs, while authoritative
save/unload still settles physical falling blocks before persistence.

## Furnace cursor identity parity (corrective checkpoint; unverified)

The network furnace-output path merged by item definition alone, while the local
path required ItemStack::can_merge. That could merge stacks with different
instance/wear identity into the retained cursor identity. The host now uses the
same identity-aware eligibility check as local play. This correction precedes
extraction of the shared container transaction and is committed separately.

No validation ran. Final parity cases must cover charged/instance-bearing and
worn output rejection, ordinary compatible merges, full cursors, and unchanged
output/cursor state on rejection.


## Graphical scene and shared container ownership (coding checkpoint; unverified)

The graphical runtime now selects a local authoritative Server or an independent
ReplicaWorld after Welcome. Guest entry no longer constructs a remote World with
a generator/save path. Snapshot application, chunk eviction, registry remapping,
mesh bookkeeping, clock observations, and cosmetic mob state route to the replica.
WorldView exposes explicit terrain, scene, calendar, geography, machine, item,
and limited laboratory observations; no mutable authority accessor is exposed.
Physics, camera, selection, bounce lighting, rendering, UI, and script reads use
these contracts. Immutable geography now shares geological prospecting queries.

Container transactions share chest/offering/stall, furnace, bloomery/forge, and
kiln rules across the local/host adapters. The earlier furnace identity correction
remains a separate commit. Crafting shares byproduct prediction without a guest
ledger. Standing-position rescue keeps search order and permits missing-column
preparation only for authority. Player knowledge/KV retain the existing local
sidecar location through the graphical runtime, outside ReplicaWorld.

This is an implementation checkpoint. The remaining guest action routes still
need source-level migration, including legacy actions without a network request,
station state prediction, script command authority, and read-only soil/heart
queries. The temporary World replication adapter remains until its final fixture
migration. No format, compiler, tests, analyzer, native proof, or GPU campaign ran;
all current-source validation remains at the end of the implementation phase.


## Guest mutation routing correction (coding checkpoint; unverified)

Source review found guest actions that spent inventory or changed local world
state without a corresponding C2S operation: boat launch, fishing payout,
harvesting, fertilizer/fire/tilling, heart/compost/smoker/hand-fed machine/firebox
operations, and station crafting. They now reject before their mutation and item
cost with an explicit multiplayer-unavailable message. This preserves the wire
schema and prevents temporary client-only outcomes from masquerading as host
state; implementing new host operations is a separate feature. Local play is
unchanged. Furnace opening now sends the existing OpenContainer request so the
host admits the cursor/container session before clicks. Physical script commands
also cannot mutate a guest. Cosmetic drops carry no fabricated host identity and
are excluded from identity-targeted workings until a host snapshot replaces them.

These are explicit correctness changes, separate from the preceding ownership
move. Final regression work must prove no inventory cost on rejection, local
operation preservation, actual furnace admission/click echo, script authority,
and no guest generation, save, stable-ID allocation, or physical simulation.
No tests or validation ran; the implementation-first schedule remains active.


## Unicode observation correction (coding checkpoint; unverified)

The shared receiver previously truncated ecology text at byte 240 unconditionally,
which panics when that byte lies inside a UTF-8 code point. The same bounded
240-byte policy now rounds down to a character boundary. ASCII and shorter
messages are unchanged. Final receiver regressions must include multibyte text
crossing the limit, short Unicode, and empty text for both guest adapters.
No tests ran during this implementation checkpoint.


## Remaining scene observation routing (coding checkpoint; unverified)

Country seed bearings/heart reports now share pure read algorithms over immutable
geography and explicit heart observations. Soil warnings share their priority
rules over salinity and optional atlas habitat/moisture. The graphical guest
retains the former empty private heart map and atlas-free moisture baseline.
Container screens retain their expected entity-type guard before prediction while
sending the existing host click first; shared slot rules stay centralized.
These complete the identified unconditional country/soil reads in the graphical
action adapter. Full Rust/API/scene parity checks remain deferred to the end.


## Authority-only World (coding checkpoint; unverified)

Removed World::remote, mode-switching APIs, incoming replication implementation,
network chunk insertion wrappers, and ReplicaObservations storage from World.
Generation, persistence, gravity/support, machine revalidation, footprints, and
item/atmospheric queries now have one authoritative meaning. ReplicaWorld alone
implements the shared incoming replication contract. Optional finite ledgers on
World remain for atlas-free fixtures and existing world initialization, not as a
guest representation; conserved quantities still belong to their existing books.

This is the planned owner/API transition. Save/wire schemas are unchanged. The
legacy remote-mode fixtures in world, climate, hydrology, implements, multiplayer,
and client_session tests still require conversion to ReplicaWorld in the final
test phase, as does the native guest proof's old field access. Those old fixtures
will not compile until migrated; no test success is claimed. Final checks must
prove receiver parity, no replica save/generation APIs, host edit side-effect
order, and corrected guest eviction without physical landing or persistence.


## Authoritative population ownership (coding checkpoint; unverified)

Population now owns private live mob/NPC, projectile, and loose-item collections
plus their allocation cursors. Stable insertion/restoration, lazy mob identity,
paired NPC creation/loading, removals, and member lookup/edit operations moved
with their state. Mutable collection views are member slices; insertion and
removal pass through explicit operations. World still coordinates arcane custody
before mob publication, physical AI/collision events, loss/harvest settlement,
and save adapters in their existing order. Collection restore during a tick does
not rewind previously allocated IDs; explicit snapshot replacement retains its
previous cursor behavior. Allocation/exhaustion policy was not changed.

The former ecology monolith is now focused habitat, wildlife placement, ordered
mob stepping, hostile/nest admission, death/harvest, projectile, and loose-item
coordinators with explicit imports and local guidance. The ordered mob step
remains a large cross-domain transaction and needs final function/size review;
this extraction does not claim to eliminate all coordinator complexity.

No tests/compiler/analyzers ran. Final tests must adapt the legacy Vec-mutating
fixtures to named collection operations and cover IDs, NPC links, restoration,
cargo/Current/material effects, drops, projectile vessels, unloaded residency,
deterministic seeding, and predator food eligibility. All accepted results remain
pending the end-of-implementation verification phase.


## Population cadence and seeded residency (coding checkpoint; unverified)

Wildlife seeded-chunk history and hostile/nest/repopulation timers now live with
Population. Named mark insertion/removal operations preserve adoption and blessing
order; distinct cadence operations preserve their thresholds, reset policy, and
seasonal multipliers. Persistence still writes/reads the WFA1 sidecar with the
same layout and iteration policy. Final seed-once, save restoration, rejected
spawn budget, and seasonal cadence checks remain pending. No tests ran.


## Authoritative calendar state (coding checkpoint; unverified)

CalendarState owns the persisted day, absolute simulation clock, Long Winter
transition, and independent reciprocity rollover fraction. Server natural dawn,
sleep, save restoration, and development overrides use named transitions; the
existing tick order and wrapping day policy are retained. Reciprocity still
subtracts one day per call, preserving accumulated remainder semantics. World
readers and host snapshots query the owner, while ReplicaWorld retains received
calendar observations and pure CalendarView supplies common astronomy.

The old direct-field test fixtures must move to explicit calendar setters during
final test work. Final checks include dawn/sleep and overflow behavior, calendar
save/load, opposite hemispheres, moon/season timing, Long Winter transitions,
offering/reseed order, and actual capture sky/weather consistency. No tests or
validation commands ran in this implementation checkpoint.


## Live weather lifetime and update admission (coding checkpoint; unverified)

WeatherState owns the optional live PlanetaryWeather and atlas-free override.
Climate-hour admission retains the dross-route dependency and prior target-hour
comparison; failed updates abort their owned hour before returning the error.
Development forcing retains the exact vapor/cloud redistribution and cannot mint
water. PlanetaryWeather remains the conserved-water owner. World supplies atlas,
calendar, and local-ire inputs, then applies loaded inboxes and the ordered
spring/shore edit batch after a completed hour. Water, alchemy, machinery,
persistence, and entry diagnostics borrow the same live state through explicit
accessors. Initialization cloning and save formats remain unchanged.

No tests/format/build/analyzers ran. Final climate/dross routing, failure-abort,
water conservation, machine/alchemy transfers, save/reload, and hardware weather
captures remain required before this checkpoint is accepted.


## Development-scene mutation adapter completion (coding checkpoint; unverified)

The graphical runtime migration now passes mutable authority to the development
scene macros that author blocks, metadata, entities, loose drops, workstation
inputs, regional ire/bloom, and fire. Read-only height/block/standing macros retain
read access. This completes an expansion-specific part of the source migration;
scene geometry, operation order, and capture content are unchanged. No macro
compilation, native staging, or GPU capture validation has run yet.


## Installation storage and work state (coding checkpoint; unverified)

Installations owns the private on-grid block-entity map, station work banks,
wheel momentum, spoilage cadence, and industrial-ire cadence. Lookups, insertion,
removal, and shared container transactions now go through this owner. Work
accumulation/consumption/reset and wheel decay are explicit operations; each
caller retains its existing comparison and carry/discard policy. The spoilage
sweep subtracts one period while industrial ire takes the accumulated interval,
matching their different pre-extraction behavior. Shared machine BlockRead and
BlockStore adapters borrow the owned entries; LocalStructure remains independent.

Block edit/custody, machinery transfers, discovery/implements/workings, and save
code retain their operation order and original codecs. No tests, compiler,
format, analyzer, or runtime checks ran. Final station timing, powered/idle
transitions, wheel inertia, container identity, save/load, block-removal drops,
and all conservation regressions remain mandatory.


## Construction membership and ghost progress (coding checkpoint; unverified)

Construction owns the template library, pending ghost fills, independent local
structures, and their allocation cursor. Named operations retain duplicate-name
admission, same-anchor replacement, exact-name ghost completion, structure order,
and loaded cursor advancement. Rail stepping takes/returns structures at their
original indices while World coordinates rail queries and machine work. Existing
TOML adapters and in-memory publication before fallible save remain unchanged.

No tests/compiler/format/analyzers ran. Final template capture/stamp, ghost
completion/cancellation, structure identity/load, rail order and machine cargo
checks remain pending the end-of-implementation verification phase.


## World facade and transaction coordinators (coding checkpoint; unverified)

The 4,000-line world root now declares owners and compatibility exports. Save
results, block-entity schemas, header/browser adapters, atomic world creation,
owner initialization, resident access, feature visibility, installation access,
event publication, spawn rescue, and lunar observations are separate modules.
Mining, placement, raw edits, material journals, and charged-item custody retain
one coordinator per existing cross-domain transaction. Public paths, schema
fields, operation ordering, and error text are preserved by the extraction.
Private helpers shared between coordinators remain scoped to the world module.

All extracted files are below 500 physical lines before final formatting; this
is not a final linter result. The long mining transaction still needs the final
function/exception review. No format/compiler/tests/analyzers ran. Final source
integration, codec and deterministic fixtures, transaction fault paths, and
native/GPU/runner qualification remain pending after implementation finishes.


## Explicit arcane observation and custody contexts (coding checkpoint; unverified)

ArcaneEnvironment borrows only immutable geography and the existing Current
ledger. It produces the same bands/survey/item observations and typed ward
pressure snapshots from supplied atlas/workings inputs. World applies each
controller's wake before dross after transport completion. Linked ecology
destruction now takes explicit mutable geography/parent-ledger references plus
registry/save location, keeping the old snapshot rollback and commit-before-voxel
ordering. Existing ledger types remain the sole owners of conserved quantities.

No checks ran. Final tests must cover absent fixture owners, exact pressure
bounds/order, missing accounts, linked-write failure restoration, reload after
commit before voxel removal, and all Current/ecology conservation suites.


## Guest feeding prediction routing (corrective checkpoint; unverified)

Source review found that graphical feeding sent FeedMob and then borrowed local
authority to update visible tame/fed/calm state. With the independent guest owner,
that branch would panic. The update now uses a named runtime prediction operation
on the matching resident mob; host requests and inventory/accounting order are
unchanged. Local feeding takes the same operation on its authoritative member.
Final native guest feeding/host echo and tame/breed regressions remain required.
No test, compiler, or analyzer ran for this source-identified correction.
