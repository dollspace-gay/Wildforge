# Wildforge Codebase Findings and Remediation Brief

This document records the findings from a read-only examination of the
Wildforge repository on 2026-07-30. It is intended to be usable as the source
brief for an implementation goal.

Suggested command:

```text
/goal Fix all actionable issues in CODEBASE_FINDINGS.md, preserve existing behavior and save compatibility, run every listed verification check, and update this document with the completed work and any remaining limitations.
```

## Repository snapshot

- Branch: `main`
- Upstream state at review time: `main` matched `origin/main`
- Reviewed commit: `ae3d394`
- Worktree at the start of review: clean
- Package: `wildforge` `0.1.0`
- Language/toolchain: Rust 2024 edition
- License: MIT
- Tracked files: 731
- Rust source: approximately 65,442 lines
- Total tracked source/configuration/scripts/shaders: approximately 74,907 lines
- Tracked textures: 280 under `base/textures/`; 551 PNG files overall
- Documentation: 800-line main README and approximately 8,688 additional
  lines under `docs/`
- Current local toolchain during review:
  - `cargo 1.96.0`
  - `rustc 1.96.0`
  - `stable-x86_64-unknown-linux-gnu`

The local checkout occupied about 14 GB, almost all of which was the ignored
`target/` directory. Tracked Git data was small, and this was not a repository
bloat problem.

## Architectural assessment

The implemented architecture broadly matches the documentation.

```text
src/main.rs
    |
    v
src/lib.rs::run
    |
    +-- windowed client -> game::Game
    +-- --server        -> dedicated host
    +-- --agent         -> MCP/headless network guest

game::Game / dedicated host
    |
    v
server::Server (fixed-step authority)
    |
    +-- World/content/persistence
    +-- Host networking adapter
    |
    v
renderer and presentation (downstream only)
```

Important architectural strengths:

- The executable has a small entry point and delegates to the library.
- Windowed solo play, windowed hosting, and dedicated hosting share the same
  authoritative `Server` simulation.
- The simulation advances at a fixed 30 Hz and caps accumulated hitch time to
  avoid a spiral.
- Rendering, GPU state, and presentation randomness do not feed back into the
  authoritative simulation.
- Guests use streamed remote worlds and do not generate or save authoritative
  chunks.
- Multiplayer guest requests are translated into host-authoritative world
  operations.
- World block mutation has a central boundary that handles dirty state,
  relighting, neighboring mesh invalidation, and edit logging.
- Stable content names are used at save and network boundaries, allowing
  registry remapping and missing-mod placeholders.
- The data-driven mod system supports dependency ordering, content validation,
  textures, scripts, hot reload, and save remapping.
- Rhai scripts have operation/call/depth limits and no filesystem or network
  API by default.
- The unsafe surface is small and documented. The notable script raw pointer
  is thread-local, guarded for the duration of dispatch, and cleared on guard
  drop.

## Validation results

The following commands were run without modifying source files.

### Formatting

```sh
cargo fmt --all -- --check
```

Result: passed.

### Tests

```sh
cargo test --all-targets
```

Result:

```text
368 passed
0 failed
12 ignored
0 measured
0 filtered out
finished in 724.07s
```

The suite exercises:

- save-format migration and compatibility;
- chunk and region persistence;
- deterministic terrain generation;
- ecology, fluids, fire, lighting, weather, seasons, soil, and machines;
- inventory, crafting, physics, swept player movement, mobs, and projectiles;
- registry validation, data mods, tags, aliases, texture packs, and Rhai;
- WGSL shader validation;
- QUIC loopback networking and snapshot batching;
- authentication, identity, host-key persistence, moderation, and profiles;
- multiplayer authority and remote-world behavior;
- full MCP/headless agent connection, movement, following, crafting, chopping,
  placement, and storage.

The four agent integration tests dominated the tail of the run. All passed,
but the default all-target feedback loop took just over twelve minutes.

### Clippy

```sh
cargo clippy --all-targets -- -D warnings
```

Result: failed with two warnings promoted to errors.

1. `src/config.rs:44`: `clippy::needless_return`
2. `src/world/region.rs:105`: `clippy::suspicious_open_options`

The region-file warning requires an explicit decision about truncation. The
current algorithm opens an existing region for in-place append/index updates,
so the likely intended fix is `.truncate(false)`, not `.truncate(true)`.
Confirm that intent against the region persistence tests.

### Dependency audit

`cargo-audit` was not installed, so an independent advisory scan could not be
run:

```text
error: no such command: audit
```

`docs/dependency-advisories.md` records one accepted advisory:

- `GHSA-q2qq-hmj6-3wpp`
- affected dependency: `hickory-proto 0.24.4`
- path: `jacquard-identity -> hickory-resolver -> hickory-proto`
- upstream fix: `hickory-proto 0.26.1`
- documented assessment: the vulnerable quadratic encoding path is not
  reachable in Wildforge's bounded, single-question stub-client use.

The assessment is technically reasoned and records what upstream change would
permit closure. It should still be rechecked automatically when the Jacquard
dependencies change.

## Actionable findings

### P0: Dirty chunks can be lost after an eviction-time write failure

Relevant code:

- `src/world/storage.rs`, `World::save_chunk_if_modified`
- `src/world/mod.rs`, `World::retain_chunks`
- `src/dedicated.rs`, periodic residency eviction

Current behavior:

1. `retain_chunks` finds chunks outside all residency centers.
2. For every departing chunk, it calls `save_chunk_if_modified`.
3. `save_chunk_if_modified` discards both the directory-creation result and
   the chunk-write result.
4. `retain_chunks` then unconditionally calls `unload_chunk`.

Consequences:

- A full disk, permissions error, transient I/O error, oversized region file,
  or other write failure can discard the only in-memory copy of modified
  terrain.
- The chunk can later reload from an older saved payload, silently reverting
  player edits.
- Dedicated-server logs report only that chunks were released, not that one
  or more failed to save.

This differs from `save_modified`, which clears `chunk.modified` only after a
successful chunk write. The safer behavior already exists in one path but not
the eviction path.

Required remediation:

- Change the single-chunk save/eviction API to return a meaningful
  `std::io::Result`.
- Never unload a modified chunk unless its save succeeded.
- Keep a failed dirty chunk resident and eligible for a later retry.
- Continue allowing unmodified chunks to unload without a write.
- Return or log a residency report that distinguishes:
  - successfully released chunks;
  - retained dirty chunks whose save failed;
  - the underlying errors.
- Avoid retrying a failing chunk in a tight loop; the existing five-second
  residency cadence is acceptable if errors are surfaced and bounded.
- Add deterministic failure-path tests. A practical test can use a save path
  that is a regular file, a read-only directory where reliable on the target,
  or an injected/fake persistence boundary.

Acceptance criteria:

- A modified chunk remains loaded and marked modified after a failed
  eviction-time save.
- The same chunk unloads after a later successful retry.
- An unmodified far chunk still unloads immediately.
- A batch can unload successful chunks while retaining only the failed ones.
- Dedicated-host output makes the partial failure visible.
- Existing region and residency tests continue to pass.

### P0: Save success is reported when significant writes may have failed

Relevant code:

- `src/world/storage.rs`, `World::save_modified` and `World::save_mobs`
- `src/world/entities.rs`, entity persistence
- `src/world/persistence.rs`, metadata/stamp persistence
- `src/dedicated.rs:88-92`, five-minute save loop
- window-close and save-and-quit call sites under `src/game/`

Current behavior:

- Many `fs::write`, `fs::remove_file`, and directory-creation results are
  assigned to `let _` and discarded.
- `save_modified` returns `()`, so callers cannot know whether the save was
  complete, partial, or failed.
- The dedicated server prints `server: world saved` unconditionally.
- Chunk writes preserve dirty state after failure, but most auxiliary world
  state has no comparable retry/error contract.
- Several auxiliary files are overwritten directly rather than through the
  existing atomic-write helper used by identity and profile data.

Consequences:

- Operators and players may believe a save completed when metadata, mobs,
  block entities, ledgers, stamps, or chunks did not land.
- Partial saves can combine state from different points in time.
- Direct overwrites are more vulnerable to interruption than temp-file,
  flush, and rename replacement.

Required remediation:

- Make save functions return `std::io::Result` or a structured `SaveReport`
  containing every failed component.
- Propagate or aggregate failures rather than stopping at the first failure if
  continuing is safe.
- Print “world saved” only when all required components succeed.
- On partial failure, print a concise error summary with component/path
  context and leave retryable in-memory state dirty.
- Use the existing atomic-write strategy, or a world-specific equivalent, for
  small metadata/state files that are fully replaced.
- Preserve intentional deletion semantics for empty optional files, treating
  `NotFound` as success where appropriate.
- Do not claim atomicity across all files unless an actual generation/manifest
  transaction is introduced.
- Ensure game UI/save-and-quit paths surface failures instead of silently
  exiting as if persistence succeeded.

Acceptance criteria:

- Injected failure of every save component is observable by the caller.
- Successful components may complete, but the overall result is partial/error.
- Dirty chunks are cleared only after successful writes.
- Dedicated-server logs distinguish success from partial or total failure.
- Small replace-in-full files cannot be left half-written by an interrupted
  write under the supported atomic replacement model.
- Existing save formats and old-world loading remain compatible.

### P1: No repository CI quality gates

Observed state:

- No tracked `.github/workflows/` configuration was present.
- Formatting and tests pass locally, but nothing in the repository enforces
  them on pushes or pull requests.
- Strict Clippy currently fails.

Required remediation:

- Add a CI workflow for supported host platforms, starting with Linux.
- Run, at minimum:

  ```sh
  cargo fmt --all -- --check
  cargo clippy --all-targets -- -D warnings
  cargo test --all-targets
  ```

- Cache Cargo registry/git data and build artifacts appropriately.
- Consider separating fast and slow tests so Clippy and deterministic unit
  feedback arrive before the twelve-minute agent scenarios.
- Add a dependency-advisory job using `cargo audit` or `cargo deny`.
- Encode the documented Hickory exception narrowly rather than globally
  suppressing advisory failures.

Acceptance criteria:

- CI runs automatically for pull requests and pushes to `main`.
- Formatting and strict Clippy are required and green.
- All non-ignored tests run in at least one CI lane.
- Advisory checking is automatic and fails on new, unacknowledged advisories.
- The existing Hickory advisory has an explicit, documented exception tied to
  its advisory ID and can be removed when upstream permits.

### P1: Toolchain and minimum Rust version are unspecified

Observed state:

- `Cargo.toml` uses edition 2024 but has no `rust-version`.
- No `rust-toolchain.toml` or `rust-toolchain` file is tracked.
- Current stable Clippy reports warnings despite a recent warning-cleanup
  commit, illustrating how lint behavior can drift with toolchain updates.

Required remediation:

- Decide and document the minimum supported Rust version.
- Add `rust-version = "..."` to `Cargo.toml`.
- Add `rust-toolchain.toml` if reproducible developer/CI lint behavior is
  desired.
- Include the components needed by project checks, at least `rustfmt` and
  `clippy`.
- If stable tracking is intentional instead of pinning, document that policy
  and let CI reveal new lints immediately.

Acceptance criteria:

- A fresh checkout selects or clearly documents a supported compiler.
- Cargo rejects unsupported old compilers with a useful version error.
- Local and CI formatting/Clippy behavior is reproducible under the chosen
  policy.

### P1: Strict Clippy is not clean

Required fixes:

1. In `src/config.rs`, remove the unnecessary `return` in the Linux
   `available_memory` branch without changing conditional compilation
   behavior.
2. In `src/world/region.rs`, explicitly declare the intended non-truncating
   behavior on `OpenOptions`.

Acceptance criteria:

```sh
cargo clippy --all-targets -- -D warnings
```

passes on the selected toolchain.

### P1: Default all-target test feedback takes over twelve minutes

Observed behavior:

- The full suite completed successfully in 724.07 seconds.
- Four `src/tests/agent.rs` scenarios remained active long after nearly every
  other test completed.
- The tests are CPU-bound, not simply sleeping or waiting on network timeouts.
- The agent test harness builds and relights large live-world stages while
  several scenarios execute concurrently.

Required remediation:

- Measure individual test duration and CPU cost in serial and parallel modes.
- Determine whether the expensive work is:
  - repeated full relighting during stage construction;
  - unnecessary terrain streaming/generation;
  - contention between concurrently running agent worlds;
  - pathfinding/perception scans;
  - fixed-duration pump loops that could use event-driven completion.
- Preserve end-to-end QUIC coverage and authoritative-world assertions.
- Reuse or batch stage construction so one logical fixture does not trigger a
  full relight for every cell edit.
- Consider an explicit bulk-edit/deferred-relight test helper if it cannot
  affect production behavior.
- Add explicit test time budgets or CI timeouts so a regression cannot run
  forever.
- Mark genuinely slow end-to-end scenarios for a slow lane only if a smaller
  fast test preserves the core behavior in the default lane.

Acceptance criteria:

- No reduction in the behavioral assertions currently covered by the four
  agent tests.
- Routine unit/fast integration feedback has a documented time target.
- Slow scenarios have explicit timeouts and run in CI.
- Test runtime is measured and reported, rather than inferred from occasional
  manual runs.

### P2: Large maintenance hotspots

Largest production source files at review time:

| File | Approximate lines | Responsibility |
|---|---:|---|
| `src/multiplayer/host.rs` | 2,633 | admission, profiles, snapshots, requests, gameplay authorization |
| `src/atlas/procedural.rs` | 2,415 | procedural art generation |
| `src/registry.rs` | 2,344 | content schemas, parsing, dependency resolution, validation |
| `src/worldgen.rs` | 2,064 | terrain, climate, resources, structures |
| `src/game/ui.rs` | 1,987 | client UI rendering |
| `src/game/frame.rs` | 1,916 | frame update/presentation construction |
| `src/game/actions.rs` | 1,899 | player actions and interaction effects |
| `src/game/demos.rs` | 1,866 | capture/demo scene construction |
| `src/mobs.rs` | 1,458 | mob state, movement, AI, projectiles |
| `src/world/mod.rs` | 1,319 | central world state and mutation boundary |

This is not evidence that the current design is wrong. The previous
modularization work substantially improved the repository, and the current
files generally have coherent responsibilities. The risk is that unrelated
changes increasingly meet in the same files, especially
`multiplayer/host.rs`, `registry.rs`, and the client frame/action/UI trio.

Required remediation:

- Refactor only along demonstrated responsibility boundaries; do not create a
  generic engine crate without a real second consumer.
- Prioritize `multiplayer/host.rs`:
  - admission/authentication/profile attachment;
  - request validation and authorization;
  - inventory/container gameplay handlers;
  - snapshot/chunk streaming;
  - moderation.
- Keep host-side authorization centralized even if handlers move.
- Consider splitting registry schema definitions, raw parsing, dependency
  ordering, validation/building, and runtime lookup.
- Keep `World::set_block` and its side effects as one authoritative mutation
  boundary.
- Preserve the current dependency direction and avoid cyclic client/server
  knowledge.

Acceptance criteria:

- Refactoring is behavior-preserving and covered by the existing tests.
- Protocol and save formats do not change unless explicitly migrated and
  versioned.
- Host authorization remains impossible to bypass through a moved handler.
- New modules have clear ownership rather than merely reducing line counts.

### P2: Dependency auditing is manual and only partially reproducible

Observed state:

- `Cargo.lock` is tracked.
- The current direct dependency set is reasonably explicit.
- The known advisory has a strong written reachability assessment.
- The review environment lacked `cargo-audit`.
- Multiple transitive versions of common crates exist. This is normal in a
  graphical/networked Rust application, but should remain visible during
  dependency refreshes.

Required remediation:

- Add automated advisory scanning to CI.
- Retain `docs/dependency-advisories.md` as the human rationale for accepted
  findings.
- Add a narrow machine-readable ignore for the Hickory advisory with a note
  pointing to that document.
- Re-run:

  ```sh
  cargo tree -i hickory-proto
  cargo tree -d
  ```

  during dependency refreshes.
- Remove the exception immediately when Jacquard supports a fixed Hickory
  line.

Acceptance criteria:

- New advisories fail CI unless explicitly assessed.
- Accepted advisories have an ID, reachability analysis, date, dependency
  path, and closure condition.
- No broad or indefinite audit suppression is introduced.

### P2: Runtime/setup documentation lacks a concise prerequisites section

Observed state:

- The README is detailed and explains game systems, controls, WSLg behavior,
  Windows cross-compilation, modding, multiplayer, and dedicated hosting.
- The primary run instruction is simply `cargo run --release`.
- It does not prominently state:
  - minimum/supported Rust version;
  - Linux native build packages such as audio/windowing dependencies;
  - expected graphics API/adapter support;
  - supported/tested operating systems;
  - how to run verification commands;
  - the MIT license near the project summary.

Required remediation:

- Add a short prerequisites/development section near the run instructions.
- Keep platform-specific package names modest and clearly labeled by distro.
- Document the chosen Rust version/toolchain policy.
- Add the standard formatting, Clippy, and test commands.
- Mention that the full agent-inclusive test suite is a slow check if it
  remains so.

Acceptance criteria:

- A new contributor can install prerequisites, build, and run checks without
  searching commit history or plans.
- Documentation matches CI and the actual supported toolchain.

## Non-issues and constraints to preserve

These observations should not be “fixed” by removing deliberate behavior.

### Single Cargo package

The single-package design is intentional. A reusable engine crate is deferred
until a real second consumer exists. Do not split crates solely for aesthetic
purity.

### Shared server simulation

Solo, windowed-host, and dedicated-host behavior must continue using the same
authoritative `Server`. Do not introduce a client-only simulation fork.

### Remote world safety

Guest worlds must not generate or persist authoritative state.

### Save compatibility

The suite proves several historical save migrations and palette behaviors.
Persistence remediation must keep those formats readable.

### Region append algorithm

Region writes intentionally append the new payload before updating its index
slot so an interruption leaves the previous whole payload addressed. The
Clippy fix must preserve this non-truncating behavior.

### Known Hickory advisory

Do not remove DNS handle verification merely to make an unreachable advisory
disappear. Close it through an upstream-compatible dependency update or if
the reachability assumptions change.

### Mod hot reload

A script compile error intentionally retains the previous working AST. Save
and error-handling changes must preserve that live-session behavior.

### Presentation/simulation separation

Do not let rendering state, frame timing, GPU decisions, or cosmetic random
streams affect authoritative outcomes.

## Recommended implementation order

1. Add failure-path tests for eviction and saving.
2. Fix dirty-chunk eviction so failed writes retain chunks.
3. Introduce structured save results and surface partial failures.
4. Make replace-in-full auxiliary state writes atomic where practical.
5. Fix the two Clippy findings.
6. Establish toolchain/MSRV policy.
7. Add CI for formatting, Clippy, tests, and advisories.
8. Profile and shorten/split the slow agent tests.
9. Refactor maintenance hotspots incrementally.
10. Update contributor prerequisites and verification documentation.

Persistence work comes first because later CI should lock in its failure-path
tests, not merely the current happy paths.

## Final verification checklist

Run all of the following after remediation:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
```

Also run the configured dependency audit command, for example:

```sh
cargo audit
```

or:

```sh
cargo deny check advisories
```

Manual/targeted checks:

- Simulate an eviction-time chunk write failure and prove the dirty chunk
  remains resident.
- Restore writability and prove the chunk saves and then unloads.
- Simulate auxiliary save failures and verify UI/server reporting.
- Start a dedicated host and confirm successful and failed saves are logged
  accurately.
- Load at least one existing world and save it without format regression.
- Join that host as an ordinary guest and verify streamed chunks, edits, and
  inventory authority still work.
- Load the shipped `mods/gems` example and exercise mod/script hot reload.
- Validate that a script compile error keeps the previous version running.
- Record total and per-test runtime for the agent scenarios.

## Completion record

Remediation completed in the working tree on 2026-07-30. No commit or pull
request was created because none was requested.

### Implemented outcome

- Eviction now returns a `ResidencyReport`; failed modified chunks remain
  resident and retryable, while successful and unmodified neighbors still
  unload.
- Full saves now return a component/path-aware `SaveReport`. Palette ordering
  prevents chunks from being written under a stale palette, and a chunk's
  modified flag clears only after its write succeeds.
- Replace-in-full configuration, metadata, entities, mob/ledger state, stamps,
  player profiles, script storage, and waystone state use the shared flushed
  temp-file/atomic-replace primitive. Empty optional files use durable
  deletion with `NotFound` treated as success.
- Window close, save-and-quit, camp saves, world creation, mode changes,
  waystone writes, chunk streaming eviction, and dedicated autosaves now
  surface failure instead of claiming success. Save-and-quit and window close
  remain in the world when local persistence is incomplete.
- CI now gates formatting, strict Clippy, the Rust 1.95 MSRV, fast/subsystem
  tests, serial QUIC agent tests, release builds, and dependency advisories.
- Rust 1.95 is declared as the MSRV; `rust-toolchain.toml` pins 1.96 with
  Rustfmt and Clippy for reproducible local and CI behavior.
- The agent fixture now builds a compact authoritative stage, batches fixture
  and streamed-chunk relighting, and negotiates a two-chunk test horizon
  without changing the legacy production default. All protocol and
  authoritative-world assertions remain.
- Host terrain/snapshot delivery now belongs to
  `multiplayer/host/streaming.rs`; request authorization remains in
  `host.rs`. Registry gameplay lookup/query logic now belongs to
  `registry/runtime.rs`, separate from schema parsing and building.
- The README now states supported platforms/backends, Linux packages,
  MSRV/pinned toolchain, verification commands, test budgets, and the MIT
  license.

### Final verification

All required automated checks passed on 2026-07-30:

```text
cargo fmt --all -- --check                         PASS
cargo clippy --locked --all-targets -- -D warnings PASS
cargo +1.95.0 check --locked --all-targets         PASS
cargo test --locked --all-targets                  PASS
  373 passed; 12 ignored; 0 failed; 85.71 seconds
cargo build --locked --release                     PASS
cargo deny check advisories                        PASS
```

Focused results:

- Four serial end-to-end agent scenarios: 4 passed in 12.44 seconds. Before
  remediation, the smallest scenario was still running when stopped at
  206.30 seconds; the original all-target suite took 724.07 seconds.
- Registry tests: 30 passed.
- Multiplayer tests: 18 passed, including failed dirty-chunk retention/retry,
  ordinary guest join/stream/edit, inventory authority, snapshot batching,
  and residency.
- The save-component failure test makes all 13 auxiliary component failures
  observable, proves the palette blocks unsafe chunk writes, restores the
  path, and proves both general and injected chunk failures retry.
- Region compatibility tests: 6 passed. Historical palette, pre-v3,
  cross-region, entity, mob, heart, and stamp save/load tests also passed in
  the complete suite.
- The shipped example mod, script storage, script events, and “compile error
  keeps the previous AST” checks passed.
- A release dedicated server was started from a clean temporary working
  directory, created its world/settings, and listened on its configured port.
  A dedicated logging unit test proves partial reports use “save incomplete”
  and cannot use the success wording.

### Remaining explicit limitations

- Atomic replacement is per file. A world save is not a multi-file
  transaction; `SaveReport` accurately reports partial completion.
- Three audited transitive advisories remain as narrow, machine-readable
  exceptions with dependency paths, reachability analysis, dates, and closure
  conditions in `docs/dependency-advisories.md`: RUSTSEC-2026-0119
  (`hickory-proto` encoding), RUSTSEC-2023-0071 (unused RSA private-key
  operations), and RUSTSEC-2023-0089 (unused/unmaintained
  `atomic-polyfill`). New advisories still fail CI.
- Metal/macOS is not a supported backend. Removing WGPU's unused Metal path
  and Wayland's optional title-font renderer eliminated the avoidable
  `paste` and `ttf-parser` audit findings; Linux Vulkan/GLES and Windows DX12
  remain enabled.

| Finding | Status | Commit/PR | Verification or remaining limitation |
|---|---|---|---|
| Dirty chunks lost after failed eviction save | Completed | Working tree | Deterministic mixed-batch failure and retry test passes. |
| Save failures hidden/false success reporting | Completed | Working tree | Structured aggregate report, atomic small-file writes, caller/UI/operator handling, and failure tests pass. Per-file rather than whole-save atomicity is explicit above. |
| Missing CI quality gates | Completed | Working tree | PR/main workflow covers format, Clippy, MSRV, fast tests, agent tests, release, and advisories. |
| Unspecified Rust version/toolchain | Completed | Working tree | MSRV 1.95 check passes; development/CI toolchain pinned to 1.96. |
| Strict Clippy failures | Completed | Working tree | Strict all-target Clippy passes. |
| Slow default all-target tests | Completed | Working tree | Full suite reduced from 724.07s to 85.71s after rebasing onto current `main`; agent group is 12.44s with a five-minute CI timeout. |
| Large maintenance hotspots | Completed increment | Working tree | Streaming and registry runtime boundaries extracted; authorization and formats unchanged. Further decomposition remains normal incremental maintenance, not a release blocker. |
| Manual dependency auditing | Completed | Working tree | `cargo deny` passes with three documented, narrow exceptions; avoidable all-target paths removed. |
| Missing concise prerequisites documentation | Completed | Working tree | README matches toolchain, CI commands, packages, backends, test budgets, and license. |
