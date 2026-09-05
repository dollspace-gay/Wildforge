# Proven gameplay bugs: depots and dungeon resets

The branch contains executed reproductions and fixes for item duplication, item
loss, blocked food deliveries, incorrect guest inventory debits, and overworld
dens that stop spawning after a dungeon visit.

The production baseline is `3253489`. Commit `52acfa9` adds the reproduction
harness and fixture content without changing production behavior. Its gameplay
regressions deliberately fail; the following fix commit makes them pass.

## Observed effects

All counts below came from execution. The depot client cases create a real
`Game`, initialize its renderer, position the crosshair over an actual depot,
and invoke the same `Game::interact` method used by the frame loop. The recorded
adapter was **NVIDIA GeForce RTX 5070 Ti Laptop GPU, Vulkan, DiscreteGpu**.
These are automated interaction proofs, not a manual playthrough or a claim
that rendered frames were visually inspected.

| Reproduction | Before the fix | After the fix |
| --- | --- | --- |
| Offer 32 clay to an empty depot | Hand 31, depot 32: 31 new items created | Hand 0, depot 32 |
| Offer 16 clay when only 4 are wanted | Hand 15, depot 76: stock exceeds the 64-unit appetite | Hand 12, depot 64 |
| Offer 16 unwanted cobblestone | Hand 16, depot 16 despite rejection | Hand 16, depot 0 |
| Offer 8 requested bread | Hand 8, depot 0, standing 0 | Hand 0, depot 8, standing 16 |
| QUIC guest offers 16 clay with 60 staged and 7 in another slot | Other slot 3, selected slot 16, depot 76; only 4 credited | Other slot 7, selected slot 12, depot 64; 4 credited, inventory echoed to guest |
| Belt sends 2 clay with 63 staged | Depot 64, loose items 0, belt cargo 0: one item lost | Depot 64, loose items 1, belt cargo 0 |
| Visit, leave, and reset a dungeon sharing an overworld den's numeric chunk coordinates | Intact den spawned once before the visit, zero times after | Intact den spawns once before and once after |

Client standing is checked alongside the physical stacks. The guest proof uses
an actual loopback QUIC client and host, including the delivery acknowledgement
and updated inventory message. The belt proof advances the real entity tick and
counts remaining cargo and collectible drops. The dungeon proof observes an
actual wolf spawn, enters and exits a generated dungeon, lets it reset, then
tries the intact overworld den again.

The self-contained `test-fixtures/gameplay` mod supplies settlement, depot, den,
and dungeon definitions through the normal content loader. This proves effects
in the engine's supported content systems without relying on an untracked
`mods/belt_quest` checkout. It does not prove that these fixture structures occur
in every default generated world.

Captured logs: [client before](evidence/gameplay/before-client.txt),
[guest before](evidence/gameplay/before-guest.txt),
[dungeon before](evidence/gameplay/before-dungeon.txt),
[belt before](evidence/gameplay/before-belt.txt),
[client after](evidence/gameplay/after-client.txt),
[simulation and QUIC after](evidence/gameplay/after-sim.txt).
The original before logs were captured during discovery, before final test
formatting and extra assertions. The belt before log was captured after the
depot and dungeon fixes but before the belt fix; the baseline belt also discards
the remainder after a partial acceptance. The reproduction commit preserves
all failing scenarios against unchanged baseline production code.

## Fixes and scope

- **Depot accounting: Category A, root-cause fix.** Demand validation belongs at
  the depot mutation boundary. `depot_deposit` now limits acceptance to demand
  and available stack capacity before mutating storage. Both solo and guest
  delivery use `deliver_to_depot`, which debits exactly the accepted quantity
  from the selected physical stack. Rejected goods change neither inventory.
  The host refreshes held state and sends the authoritative inventory back to
  the guest immediately. This replaces the two divergent debit implementations.
- **Food routing: Category B, local correctness fix.** The general food gate
  prevented the depot branch from running at all. A depot under the crosshair
  now receives food interaction as well as non-food goods.
- **Belt surplus: Category B, local correctness fix.** The handoff treated any
  positive acceptance as consumption of the whole stack. It now passes the
  remaining quantity through the existing machine/loose-item path, preserving
  item metadata and physical quantity.
- **Dungeon nest reset: Category B, local correctness fix.** Slot membership
  compared only numeric coordinates, so Deep cleanup also deleted overworld
  nest registrations. Membership now includes the face. Nest registrations and
  their spawn cooldowns are removed only inside the expired dungeon slot.

Call-site and sibling inspection used:

```sh
rg -n 'depot_accept|depot_deposit|deliver_to_depot|chunk_in_slot' \
  --glob '*.rs' --glob '!target/**' .
```

The production delivery callers are the solo interaction handler and QUIC host;
the belt calls `depot_accept`. The remaining deposit callers are tests and the
shared transfer implementation. Dungeon slot checks are local to its reset and
run-membership code. Existing public signatures, network protocol, dependencies,
MSRV, and unsafe code are unchanged. No lint suppression was introduced. No
cross-crate coordination is required in this single-package workspace.

The client test declaration lives beside the interaction handler in
`src/game/actions.rs`. An initial declaration in `src/game/mod.rs` invalidated
the existing visual-evidence source digest; moving only that test declaration
restored the original digest and all eight visual-evidence validator tests
passed. No visual evidence, source-hash gate, or acceptance threshold was
changed.

## Replay

Run the current passing proofs from the repository root:

```sh
RUSTC_WRAPPER= cargo test --locked --lib gameplay -- --nocapture --test-threads=1
python3 tools/run_gameplay_proofs.py
```

The first command runs the portable regressions (plus existing tests whose names
contain `gameplay`). The real client test is explicitly ignored in ordinary CI
because it requires Linux, X11, and a working graphics adapter. The second
command builds and runs that test with a temporary working directory, isolated
save and identity, the tracked fixture mod, and muted audio. It removes
`WILDFORGE_*` startup overrides from the child environment. It does not touch the
user's saves. Renderer initialization errors fail the run.

To replay the failing versions without disturbing the fixed checkout:

```sh
git worktree add --detach /tmp/wildforge-before 52acfa9
cd /tmp/wildforge-before
RUSTC_WRAPPER= cargo test --locked --lib tests::gameplay:: -- --nocapture --test-threads=1
RUSTC_WRAPPER= cargo test --locked --lib gameplay_guest_depot -- --nocapture --test-threads=1
python3 tools/run_gameplay_proofs.py
```

Each test invocation above is expected to fail at the reproduction commit.
The local Cargo config names `sccache`, which is absent on the test machine;
`RUSTC_WRAPPER=` disables that unavailable wrapper for these commands. The
Python runner applies that workaround only when `sccache` cannot be found.

## Verification

This section records the initial verification at `7e79e15`. The subsequent
[CI repair](ci-repair.md) supplies the missing content, fixes formatting, and
restores the required runner checks.

Executed on Linux with Rust 1.96.0, using the wrapper override described above.
Command output summaries are preserved in
[validation.txt](evidence/gameplay/validation.txt).

| Command/check | Result |
| --- | --- |
| `cargo build --locked --workspace` | Passed |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | Passed |
| `cargo test --locked --lib gameplay -- --nocapture --test-threads=1` | 7 passed, 1 explicitly ignored client test |
| `python3 tools/run_gameplay_proofs.py` | 1 passed, all four real-client cases checked on NVIDIA Vulkan |
| `cargo test --locked --lib visual_capture::tests:: -- --test-threads=1` | 8 passed |
| `cargo test --locked --all-targets -- --skip tests::agent::` | 1,003 passed, 8 failed, 24 ignored, 10 filtered |
| `cargo test --locked tests::agent:: -- --test-threads=1` | 10 passed |
| `cargo test --locked --doc` | Passed, 0 doctests defined |
| `cargo fmt --all -- --check` | Existing failures in the same 62 files; 290 diff sections versus 293 on the baseline |
| New standalone Rust files, Python syntax/platform guard, `git diff --check` | Passed |

The full suite is **not green**: all eight failures are the same tests that
failed on the untouched baseline because the external `mods/belt_quest` content
is missing. That baseline had 999 passing tests, eight failures, and 23 ignored
tests. This branch adds four passing portable regressions and one explicitly
run client test. No failing test was removed or skipped to obtain this result.
The tracked fixture makes these new reproductions independent of that missing
external content; it does not substitute for the existing content tests.

The existing formatting debt is outside this gameplay fix; the new standalone
Rust files pass rustfmt and no new file was added to the formatter's failure
set. Miri was not run because no unsafe code changed. Release, MSRV, and
cross-platform graphics runs were not performed; the recorded workspace build
uses the development profile.
