# Feature: Maintainable engine ownership and source quality checks

## Summary

Refactor Wildforge around explicit owners for terrain work, client sessions,
authoritative player operations, and world domains. Keep ordinary source files
around 400 lines, use 500 lines as a soft review limit, and continuously report
copy/paste candidates. Deliver small, tested slices that preserve the game.

Status: implementation in progress on `refactor/maintainable-engine`.
The user subsequently authorized the full migration and directory guidance.
Track completed slices and remaining criteria in [the progress record](maintainability-progress.md).
Originally prepared on 2026-09-05 from the working tree based on `8c1ec40`,
including the then-pending agent/wildlife repairs.
This follows the completed July work in [the modularization plan](../docs/modularization-plan.md).
That document's historical sizes and protocol versions are not this baseline.

## Requirements

- REQ-1: Preserve gameplay, tick order, deterministic genesis, existing saves,
  content identities, host authority, and wire compatibility in structural slices.
- REQ-2: Make authoritative world state and guest replicas distinct owners with
  explicit read/query surfaces and controlled mutation operations.
- REQ-3: Share terrain loading/generation job machinery between solo/windowed
  hosting and dedicated hosting, retaining one authoritative adoption boundary.
- REQ-4: Share protocol admission, remapping, and replica updates between the
  graphical guest and agent; keep their presentation and control policies explicit.
- REQ-5: Route local and network player actions through the same authoritative
  domain operations, including inventory and station transactions.
- REQ-6: Give world domains ownership of their state and invariants, with explicit
  coordination for operations that affect multiple domains.
- REQ-7: Expose world generation as deterministic stages with named inputs and
  outputs, independent of job scheduling, persistence, networking, and rendering.
- REQ-8: Divide content loading, client UI/actions, simulation orchestration,
  rendering, and tooling into cohesive modules with narrow interfaces.
- REQ-9: Apply a soft 400/500 physical-line policy to first-party source,
  including tests and shaders; record justified exceptions rather than game the metric.
- REQ-10: Report significant exact-token copies and review semantic repetition;
  never treat all similar code as an instruction to create a shared abstraction.
- REQ-11: Document and mechanically check the allowed dependency direction as
  boundaries are established, including dependencies hidden by broad imports.
- REQ-12: Protect each migrated boundary with focused behavioral tests and
  repeatable performance observations appropriate to that boundary.
- REQ-13: Make background work and session shutdown owned, observable, and
  testable; stop accepting work, cancel/drain, join workers, and surface save failures.
- REQ-14: Keep an incremental migration record identifying completed slices,
  compatibility evidence, remaining debt, and any separately delivered bug fixes.

## Current architecture and evidence

The crate already has useful foundations: `Server::advance` owns fixed-step
simulation, `Generator::generate` prepares terrain, and `World::adopt_prepared`
commits prepared chunks. Preserve and strengthen these boundaries.

| Current location | Observed maintenance cost | Intended owner |
|---|---|---|
| `src/game/streaming.rs`, `src/multiplayer/host/streaming.rs` | Separate worker pools, queues, completion handling, and lifetime policies | Terrain job service, with caller-specific policies and adapters |
| `src/game/remote.rs`, `src/agent/mod.rs` | Repeated admission flags, ID maps, snapshot assembly/application, and replica setup | Shared client session |
| `src/game/containers.rs`, `src/multiplayer/host.rs` | Repeated stall-sale checks and item/till mutation | Authoritative trade operation |
| `src/world/mod.rs` | One object holds storage, generator, ledgers, weather, ecology, machines, and remote-only fields | World composition root and bounded domain owners |
| `src/worldgen.rs`, `src/planet_atlas/` | Large generation/stage implementations make local terrain changes hard to isolate | Generator stages and immutable generation context |
| `src/registry.rs` | Definitions, raw content loading, linking, validation, and lookup coexist | Domain definitions, loader/linker, validated registry |
| `src/game/mod.rs`, `src/game/actions.rs`, `src/game/ui.rs`, `src/game/frame.rs` | Child modules retain broad access to the same `Game` state | Session, interaction, UI, and presentation owners |
| `src/world/workings.rs`, `src/world/alchemy.rs`, `src/world/implements.rs` | Large world adapters coordinate several ledgers and physical effects | Domain transactions with explicit dependencies |
| `src/lib.rs`, `src/dedicated.rs`, `src/agent/mcp.rs` | Startup/lifetime responsibilities and process I/O need clearer homes | App dispatch and runtime/session owners |

Initial size survey: 98 Rust files exceed 500 lines, counting separate test
modules and inline tests. The largest are `registry.rs` (7,455),
`world/workings.rs` (5,419), `world/alchemy.rs` (5,221), `arcane.rs` (5,127), and
`multiplayer/host.rs` (5,087). This is an inventory, not a ranking by importance.
The complete advisory snapshot is [maintainability-baseline.json](maintainability-baseline.json).
Counts there also include Python, WGSL, Rhai, and shell source.

The initial clone scan identifies repeated inventory UI drawing, stall
transactions, shader sections, and visual-verification helpers. These are
lexical candidates. For example, the stall candidate was inspected and does
duplicate the same goods/till rules in local and network paths. The divergent
terrain worker implementations also need consolidation despite not appearing
as one large exact-token clone. A detector alone cannot inventory semantic debt.

## Architecture

Proposed module names below describe destinations; implementation may refine
names when the corresponding owner and its callers are extracted.

```text
platform / CLI
  -> graphical app -> UI, input, presentation -> renderer/audio
  -> agent app     -> perception, navigation, work policies
                    \ shared client session -> network transport
                                             -> client world replica

solo / windowed host / dedicated runtime
  -> authoritative player operations + fixed-step simulation
  -> authoritative world composition
       -> terrain store, ecology, machines, environment, magic domains
       -> authoritative chunk adoption and persistence

terrain job service -> immutable generation context + save reader
                    -> prepared results for authoritative adoption
```

World queries should expose the data physics, pathfinding, meshing, and
perception actually need. Mutation belongs to explicit commands/transactions.
Use concrete types by default. A read trait is justified where authoritative
terrain and a client replica are real independent implementations. Avoid a
universal service container or an event bus that makes call order implicit.

Stay within the existing Cargo package initially. Establish ownership and
dependency rules before considering crate boundaries. A later crate extraction
needs evidence from dependency separation, compile measurements, or public-API
constraints; smaller files alone are not sufficient justification.

### Phase 0 — establish a reproducible migration baseline

1. Preserve the current agent/wildlife repair as a separate reviewable change
   before landing architectural moves. Do not mix it into a structural diff.
2. Record current public APIs, Rust MSRV 1.95, protocol 46, disk WFC8 and wire
   WFC9 codecs, atlas/content versions, test inventories, and representative saves.
3. Rerun format, strict Clippy, MSRV, subsystem/agent tests, and release build
   on the actual baseline revision. Earlier playtest results are useful evidence,
   but do not substitute for the revision being refactored.
4. Record reproducible cold/warm entry, travel, and shutdown scenarios with
   seed, route, view distance, hardware, build, cache state, and content identity.

Deliverable: baseline revision and evidence manifest. Use disposable copies
for migration tests; retain the user's original playtest save and identities.

### Phase 1 — advisory checks and a reviewed debt inventory

The first implementation is `tools/check_maintainability.py`, supported by
small modules in `tools/maintainability/` and tests in `tools/tests/`.

- Count physical lines in Git-tracked and unignored new source files.
- Report exact-token clone groups with paths, line spans, and stable content IDs.
- Produce text and complete JSON reports; run in an independent lightweight CI job.
- Keep findings advisory. Invalid configuration, scanner failures, or failing
  scanner tests fail the check so a broken analyzer cannot look like a clean scan.
- Maintain a reviewed inventory of semantic duplicates, ownership problems,
  and large files as migration slices are accepted.

The checked-in JSON is a dated observation, not an automatic exemption list
or a permanently frozen acceptance threshold. PR-delta reporting is implemented
with `--base <git-revision>`: it resolves a pinned commit and distinguishes new,
grown, reduced, and removed findings, including Git-detected file renames.
Introducing a merge-blocking debt policy remains a separate decision; the
requested size limit stays soft.

### Phase 2 — shared terrain jobs (first architecture slice)

Extract `terrain_jobs/{queue,workers,result,policy}.rs` from `GenPool` and the
generation portion of `HostChunkJobs`. Use the existing `ChunkLoader` and
`Generator`; do not create another terrain algorithm or decoder.

The service owns queued/running/completed jobs, deduplication, worker handles,
session generation IDs, cancellation, and completion errors. Callers provide
interest/priority and explicit limits. GPU meshing and wire encoding consume
prepared world data through their own adapters.

Land in reviewable steps:

1. Characterize each current queue/budget policy and extract shared preparation
   mechanics while preserving those policies.
2. Migrate solo/windowed hosting, then dedicated hosting, to the shared owner.
3. Add explicit session shutdown and stale-result rejection as isolated
   corrective changes, with regressions for world switch/disconnect.
4. Surface missing, invalid, and unreadable saves as distinct results. The
   current `ChunkLoader::load -> Option<Chunk>` collapses these cases; changing
   regeneration behavior needs its own reviewed compatibility decision/test.

Prepared results carry provenance (loaded/generated), world generation ID,
and errors. Only the authoritative thread adopts them and commits side effects.
Test that late results cannot overwrite player edits or a new session, and
that finite material/water accounting occurs exactly once.

### Phase 3 — one client session for graphical guests and agents

Extract `client_session/{admission,palette,snapshots,replica,events}.rs`.
Replace repeated boolean combinations with explicit admission states and
transitions. Share host/local ID mapping, snapshot assembly, entry manifests,
replica application, disconnect handling, and outgoing requests.

The session exposes state and bounded events. Graphical interpolation, audio,
UI toasts, and mesh readiness remain explicit presentation consumers; agent
breadcrumbs, pathfinding, perception, and MCP macros remain agent policies.
Preserve the graphical entry requirement to prepare its initial visible mesh.

Migrate remapping and snapshot helpers first, admission second, then the full
replica owner. Test both consumers against the same recorded protocol sequences:
partial/out-of-order snapshots, reconnect, mod remapping, entry failures, and
block updates. Guests must never generate or save authoritative terrain.

### Phase 4 — shared authoritative player operations

Extract domain operations called by both local play and `HostSession` request
handlers. Start with the inspected stall transaction, then inventory/stations,
crafting, placement/mining, combat, and equipment according to duplicate review.

Each operation receives an actor context and the state it needs, validates
before committing, and returns a typed outcome. Network authorization/rate
limits stay at the request boundary; gameplay eligibility and transaction rules
have one owner. Preserve signed provenance and item/Current conservation.

Parity tests submit the same action through local and network adapters and
compare authoritative outcomes. Include rejected actions, full inventory/till,
depleted stock, and partial-capacity cases before deleting the duplicate paths.

### Phase 5 — world domains and explicit authority

Extract owners in dependency order: terrain store/read views, persistence
coordination, ecology/entities, machines/structures, environment/calendar, then
magic integration. Existing ledger types remain the owners of their conserved
quantities; avoid duplicating accounting inside new wrappers.

Move behavior with the state whose invariant it enforces. A domain module
receives a narrow context instead of unrestricted `&mut World`. Cross-domain
transactions retain one coordinator and the existing side-effect order.
Document the fan-out for block edits, death/drops, chunk adoption, and unload.

Replace `World::remote` plus optional authority fields with distinct authoritative
and replica owners over shared spatial primitives. Migrate read consumers
incrementally so this does not require one enormous rewrite of every call site.
Persistence encodes owned domain state through existing format adapters.

### Phase 6 — generation and content modules

Split `worldgen.rs` along actual stages: terrain/strata, surface materials,
water, vegetation, structures, and finishing. Give stages immutable context,
explicit outputs, stable seed derivation, and documented execution order.
Do the same inside the large atlas climate/geology/hydrology implementations.

Split `registry.rs` into domain definitions, raw deserialization, name linking,
validation, and runtime lookup. Keep stable content names and existing runtime
ID assignment behavior. Keep startup/hot-reload publication atomic: callers
receive a validated registry, with actionable errors for rejected content.

These slices establish seams for later worldgen polish. Terrain outputs and
atlas hashes should remain identical on pinned fixtures during extraction.

### Phase 7 — client, renderer, diagnostics, and test organization

Move methods from broad `impl Game` access to input, interaction, UI, session,
and presentation owners. Extract the repeated inventory-panel drawing only
where the panels have the same behavior and lifetime. Keep the frame pipeline
explicit and preserve simulation/presentation RNG separation.

Split the large action dispatchers by domain. Move CLI command dispatch out of
`lib.rs` into app command modules. Consolidate repeated Python verification
helpers around real shared inputs/outputs. Split tests by scenario family and
build reusable fixture builders where setup encodes the same contract.

For `src/shader.wgsl`, choose coherent shader units and a deterministic assembly
step if shader composition requires it. Validate the assembled WGSL and compare
render captures/timings on the actual GPU. A generated aggregate can be an
explicit exception; keep the authored shader units within the source policy.

### Phase 8 — prevent architecture drift

Maintain a dependency allowlist for established module boundaries. Check that
simulation/generation/session-core modules do not import winit, wgpu, UI, audio,
or app state, and that renderer-facing code does not mutate authoritative state.
Check both qualified paths and imported aliases; replace `use crate::*` and
`use super::*` at boundaries with explicit imports as those modules migrate.

Introduce function-length and complexity reports using syntax-aware Rust
analysis or supported Clippy lints after recording their noise on this tree.
Do not estimate control-flow complexity by counting braces/keywords. The initial
linter does not claim these checks. Tighten visibility when the owning API is
stable, and ensure modules remain independently testable.

## File-size and DRY policy

| Size | Default response |
|---|---|
| Up to 400 physical lines | Normal target; still review cohesion and complexity |
| 401–500 | Advisory warning; inspect the next natural responsibility boundary |
| Over 500 | Strong review signal; identify a split or document an exception |

Count comments, blank lines, embedded strings, and inline tests. This makes
measurement predictable; it is not an instruction to remove documentation,
compress formatting, move code into macros, or build numbered file fragments.
Each extracted module needs a coherent responsibility and an explicit owner.
Tests, Python tools, shaders, and authored mod scripts follow the same guidance.

Existing oversized files are migration debt. Touching a small bug does not
require rewriting an entire file. Architectural slices should reduce relevant
debt and keep their newly authored modules around the target.

An exception record must identify the exact file/finding, reason, responsible
domain, and next review point. Generated output must name its generator and
authored inputs. No blanket directory exclusions for tests, registry data, or
shaders. The initial linter reports everything in scope and has no suppression
mechanism; accepted exceptions can be documented alongside the migration record.

Exact clone reports ignore formatting/comments but preserve identifiers and
literals. Defaults are at least 100 tokens and 12 distinct token-bearing lines
per occurrence. Rust, Python, and WGSL receive clone checks; Rhai and shell
currently receive only size checks. Nested/overlapping clone groups may exist;
do not sum their lengths into a claimed duplication percentage.

Review a candidate by asking whether both sites implement the same rule and
should change together. Extract that rule into its owning domain and test both
callers. Similar shapes with independent semantics may remain separate. Keep
renamed/rewritten copies on the manual inventory: this scanner cannot establish
semantic equivalence, expand macros, or discover every DRY violation. It caps
extremely common fingerprint buckets at 64 occurrences and reports skipped
bucket counts explicitly.

## Acceptance Criteria

- [ ] AC-1 (REQ-1): Each structural slice records unchanged save/codec fixtures,
  deterministic output checks, public API/MSRV status, and applicable gameplay gates.
- [ ] AC-2 (REQ-2): Authoritative and replica owners are distinct; read consumers
  compile against bounded queries and replicas lack authoritative persistence/generation APIs.
- [ ] AC-3 (REQ-3): Both hosting paths use one terrain job owner; tests cover
  deduplication, priority, cancellation, stale completion, edits, and exactly-once adoption.
- [ ] AC-4 (REQ-4): Graphical guest and agent use one admission/replication owner;
  shared protocol scenarios plus real graphical/agent smoke checks pass.
- [ ] AC-5 (REQ-5): Migrated local and network operations produce equivalent
  authoritative results in success and rejection cases; their copied rule bodies are removed.
- [ ] AC-6 (REQ-6): Extracted world domains own private state and behavior;
  cross-domain transaction tests preserve accounting and side-effect order.
- [ ] AC-7 (REQ-7): Named generation stages preserve pinned chunk/atlas output
  across worker counts and request orders and can be exercised independently.
- [ ] AC-8 (REQ-8): Migrated content/UI/render/tool modules expose narrow APIs;
  registry remap/hot-reload, real UI actions, and affected render checks pass.
- [ ] AC-9 (REQ-9): Every migrated/new oversized source file has a split or a
  specific exception record; reports retain the 400/500 soft thresholds.
- [x] AC-10 (REQ-10): The linter emits deterministic clone locations and JSON;
  tests cover comments, raw strings, literals, overlap, and advisory exit behavior.
- [ ] AC-11 (REQ-11): A documented import/dependency contract and a check with
  intentionally violating fixtures cover each established architectural boundary.
- [ ] AC-12 (REQ-12): Every slice records focused tests and applicable Rust gates;
  controlled entry/travel tests report frame-time percentiles, queue depth, peak
  memory, and entry latency, with reproducible regressions resolved before acceptance.
- [ ] AC-13 (REQ-13): Shutdown/world-switch tests leave no owned worker alive,
  reject old-session results, and report persistence failures without losing dirty state.
- [ ] AC-14 (REQ-14): A migration record ties each completed slice to its diff,
  test evidence, debt changes, and separately reviewed behavior corrections.

Rust gates remain formatting, strict Clippy, MSRV, subsystem and serial agent
tests, doctests, and release build. Run focused tests during development; run
the complete applicable gates before accepting a slice. Use Miri for changed
unsafe boundaries. Runtime/streaming/render slices additionally need cold/warm
entry, movement, save/reload, disconnect/shutdown, and actual GPU evidence.

Do not claim a performance improvement from fewer lines or a passing test
suite. Preserve budgets during extraction and measure tuning as separate work.

## Open Questions

No blocking questions for this plan or the advisory tooling. The soft limit,
single-package starting point, and first terrain-service slice follow the
discussion. Renamed-clone analysis, syntax-aware complexity tooling, and later
merge enforcement remain explicit future evaluations, not implemented features.

## Scope boundaries

The original planning change excluded execution and local runtime work. The
subsequent implementation goal authorizes the migration, local checkpoints,
and controlled verification. Publishing and unrelated gameplay changes remain
outside this work.

- Changing terrain appearance, ecology balance, or player-facing mechanics.
- Replacing the engine, introducing an ECS, or forcing async simulation.
- Reformatting or splitting the entire repository to satisfy a line counter.
- Automatically deduplicating code or changing save/wire versions.
- Publishing changes or launching delegated agents without authorization.

## Tooling delivery and verification

```sh
python3 tools/check_maintainability.py
python3 tools/check_maintainability.py --format json
python3 tools/check_maintainability.py --json-output /tmp/wildforge-maintainability.json
python3 -m unittest discover -s tools/tests -v
```

The tool uses the Python standard library and Git; no new Rust dependencies or
Node toolchain are required. It scans tracked and unignored new `.rs`, `.py`,
`.wgsl`, `.rhai`, and `.sh` regular files, skipping symlinks. Ignored saves,
installed test mods, and build products are excluded by Git's existing rules.
Finding debt exits successfully; scanner/configuration failures exit nonzero.
The CI job tests the analyzer and prints its advisory report independently of
the existing Rust gates. The delivery results below describe the original
planning checkpoint; current implementation evidence is in the progress record.

Delivery checks: 18 analyzer tests passed on Python 3.12.13 and 3.14.7;
both full-tree scans found 103 files over 500 lines and 71 clone candidate
groups, with no saturated fingerprints skipped. The recorded JSON uses
Python 3.12.13. `actionlint .github/workflows/ci.yml` and `git diff --check`
passed. Design validation covers 14 requirements and 14 matching acceptance
criteria. Rust build/Clippy/MSRV/test/format gates were not rerun for this
documentation/Python-tooling change; no Rust implementation was changed here.
The workflow has been checked locally, not executed on hosted CI in this task.
