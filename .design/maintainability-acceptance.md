# Maintainability acceptance evidence

The architecture implementation and all local qualification gates are complete.
The latest-head PR checks provide the external acceptance result. This
record consolidates the historical checkpoints in
[maintainability-progress.md](maintainability-progress.md), without replacing
their original failures or verification limits.

The final executable source is `70de48ac1e455db2ff38abea674a9d0622ae2fcf`.
Later documentation and evidence commits do not change its qualification source
fingerprint, `ec379a2553b2189159113f6e03c7cac9305fc6351f63bc8a7379c59d882c9a03`.
The comparison baseline is main `8c1ec4082931d4414ef6e2daca2f1a3dbd5d1bb3`.

## Requirement mapping

| Criterion | Implementation and evidence |
|---|---|
| AC-1: compatibility | Save, region, palette, protocol, registry and gameplay scenarios exercise the final implementation. MSRV remains 1.95; the protocol and existing disk/wire formats retain their versions. The independent generation corpus compares main and the final source. Corrective changes have separate commits and historical explanations. |
| AC-2: authority | `World` and `ReplicaWorld` are separate owners. `TerrainRead`, `SceneRead` and `WorldView` provide bounded queries. Guest/replica dependency fixtures reject authority access; the native proof verifies a graphical guest has no generation pool. |
| AC-3: terrain work | Both hosts use `terrain_jobs`. Queue, context and lifecycle scenarios cover priority, deduplication, cancellation, invalidated palettes, saved edits, old sessions, worker panics, joined shutdown and exactly-once authoritative adoption. |
| AC-4: common client | Graphical guests and agents use `GuestSession`. The 34 client-session scenarios cover admission, remapping, assembly and ordered/paced replica updates; 20 serial real-QUIC scenarios and the actual graphical proof cover entry, movement, chat and reconnect. |
| AC-5: player operations | `player_ops` owns shared trade, container, craft, equipment, nutrition, feeding, combat and terrain rules. Success/rejection, identity, overflow and rollback scenarios complement existing multiplayer/operation suites. Four actual depot mouse interactions and unsupported guest boat use exercise the graphical adapter. |
| AC-6: world domains | Terrain, population, calendar, weather, installations and construction own private state. Named World coordinators preserve ledger order. Material/water/Current conservation, interrupted operations, replay and persistence scenarios verify cross-domain transactions. |
| AC-7: generation | Named chunk/atlas stages use immutable inputs and stable ordering. [generation-compatibility.json](generation-compatibility.json) records identical stage/atlas checksums and 54 chunk hashes across main/candidate, three seeds, all faces, center/edge/corner, serial/parallel atlas generation, 1/2/4 workers and forward/reverse requests. |
| AC-8: application boundaries | Registry linking/publication, client input/actions/UI, renderer setup/passes and diagnostics have named owners. Registry/remap/reload and shader scenarios, native interactions and the hardware campaign exercise these boundaries. |
| AC-9: size policy | [maintainability-final.json](maintainability-final.json) reports 1,023 source files, 84 above 400 lines and 46 above 500. [source-size-review.md](source-size-review.md) records an exact exception for every migrated/new source above 500. Findings remain advisory and visible. |
| AC-10: clones | All 53 Python tooling scenarios pass, including scanner literals/comments/raw strings, overlaps, deterministic output and advisory exit behavior. [clone-review.md](clone-review.md) reviews all 61 retained candidates; copied domain rules with shared semantics were extracted. |
| AC-11: dependencies | The strict architecture report covers 486 protected source files with no findings. Intentionally violating fixtures exercise aliases, globs and exclusions. The compiler-backed function report completes with 379 advisory findings; it is not a suppression or a complete effect/type-system proof. |
| AC-12: verification | The command record below covers final Rust/Python gates. [native-runtime-proof.json](native-runtime-proof.json) reports actual entry latency, update percentiles, pending initial work and peak process memory. Hardware qualification preserves its original timing limits and failed trials. |
| AC-13: lifetime | Background/terrain/session tests verify joined workers, old-session rejection and retained failures. Save lifecycle scenarios inject real directory/chunk write failures and verify dirty data is retried. The native guest proof performs full disconnect/re-entry/disconnect; the two hardware motion walks request normal window closure and exit successfully. |
| AC-14: record | The progress record and separate commits map extraction, corrective changes and checks. Baseline observations are preserved; final size, clone, native, generation and visual records state their scope. Every maintained directory has local README/AGENTS guidance. |

## Reproducible final checks

Logs live under `target/maintainability/final-refactor-validation/`. The ignored
raw storage is retained locally; the portable summaries and deterministic
qualification reports are checked in.

| Command / experiment | Result artifact |
|---|---|
| `cargo fmt --all -- --check` | `format-2.log`: pass |
| `cargo clippy --locked --all-targets -- -D warnings` | `final-clippy-2.log`: pass |
| `cargo +1.95.0 check --locked --all-targets` | `msrv-2.log`: pass |
| `cargo build --locked --release` | `release-1.log`: pass |
| `cargo test --locked --all-targets -- --skip tests::agent::` | `subsystems-3.log`: 1,144 pass, zero failures, 24 ignored; 20 agent scenarios run separately |
| `cargo test --locked tests::agent:: -- --test-threads=1` | `agent-tests-1.log`: 20 pass |
| `cargo test --locked --doc` | `doctests-2.log`: pass, zero doctests defined |
| `python3 -m unittest discover -s tools/tests -v` | `python-6.log`: 53 pass |
| `MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri test --locked --lib script::context_tests:: -- --test-threads=1` | `miri-2.log`: three real scoped WorldView/ReplicaWorld tests pass; nightly 1.99.0 (2026-08-08) |
| `python3 tools/check_architecture.py --strict` | `architecture-6.json`: 486 protected files, zero findings |
| `python3 tools/report_rust_functions.py` | `functions-2.json`: complete, 379 advisory findings |
| Native graphical lifecycle | `native-4.log`, raw artifacts in `target/maintainability/client-session-native/attempt-4`: pass, clean source, discrete Vulkan |
| Independent main/candidate generation probe | [Full probe and invocations](generation-compatibility.md): both pass with identical corpus |

The selected [GPU bundle](../screenshots/qualification-20260906/README.md)
passes every aggregate gate and retains the initial failed timing trial plus
one complete passing repeat under the original limits. Directory coverage
(`guides-4.log`) is 131 maintained directories with zero documentation problems.

## Interpretation and retained debt

The native performance observations use test-profile `Game::update` timings,
excluding harness sleeps. Fresh entry starts a new guest session with initialized
content; re-entry joins the same host after full disconnect. Peak RSS covers the
process lifetime, and queue depth counts initial terrain/mesh work. These are
reproducible functional observations, not a production speedup claim or a cold
operating-system-cache comparison. The release hardware campaign separately
measures the existing visual timing budgets. The generation comparison is a
finite fixture corpus, not an all-seeds proof.

Large existing algorithms, compatibility schemas and selected stateful scenarios
retain specific size exceptions. Clone and function findings remain advisory.
The boundary scanner checks the documented written dependency contract and its
fixtures; it does not claim exhaustive semantic analysis of every Rust effect.

The draft PR's external acceptance requires all seven checks on its latest
immutable head. Historical failures remain part of the record; a passing later
run does not change their outcomes.
