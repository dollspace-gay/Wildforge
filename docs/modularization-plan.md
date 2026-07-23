# Engine Boundaries & God-File Modularization — Design Plan

Drafted 2026-07-22. This is a structural plan, not an engine rewrite.
Wildforge already has a custom engine: world/chunk storage, authoritative
simulation, rendering, physics, audio, UI, content loading, scripting, and
networking. The work here makes those boundaries legible and maintainable
without replacing them with a third-party framework or inventing a generic
engine product.

The refactor has two deliberately different passes:

1. **Mechanical separation** — move existing code into coherent modules while
   preserving its ownership, call order, and behavior.
2. **Responsibility extraction** — replace the largest bags of state and
   cross-system mutation with smaller owners and explicit APIs.

Pass 1 must land and settle before Pass 2 begins. Mixing the two would make
review difficult: a moved method and a redesigned method should never be in
the same diff unless the move is required for compilation.

## Why now

The project has good subsystem seams already (`Server`, `World`, `Renderer`,
`Registry`, `HostSession`), but several source files have become convergence
points:

| File | Current size | What has accumulated there |
|---|---:|---|
| `src/main.rs` | 8,234 lines | startup, `Game` state, local/remote sessions, input, survival, interaction, UI, frame construction, and dedicated hosting |
| `src/tests.rs` | 6,499 lines | every domain's tests and shared fixtures |
| `src/world.rs` | 3,426 lines | chunks, saves, lighting, fluids, machines, ecology, weather, and persistence |
| `src/atlas.rs` | 3,018 lines | atlas/pack management plus procedural art generation |
| `src/renderer.rs` | 2,069 lines | GPU setup, pipelines, resources, shadows, frame passes, and post-processing |

Line count is only a symptom. The real cost is that unrelated work collides in
the same files, ownership is hard to see, and a method can reach across many
systems simply because all their fields are on `Game` or `World`.

The existing sim/client split is the boundary to protect. `Server` remains the
single authoritative simulation for local play, a windowed host, and the
headless server. Presentation stays downstream of simulation and never feeds
cosmetic randomness or GPU state back into world behavior.

## Decisions and non-goals

- **Keep the custom engine.** Do not migrate to Bevy, Godot, an ECS, or another
  game framework as part of this work.
- **One repository and one Cargo package.** A `lib.rs` gives the codebase an
  internal API; it does not create a separately versioned engine crate.
- **No gameplay changes in either pass.** If a discovered behavior deserves a
  fix, record it and land it separately so the refactor remains reviewable.
- **No save or wire changes introduced by this refactor.** The current `WFC4`
  metadata format (with `WFC3` read compatibility), palettes, player/entity
  files, mod storage, protocol 10, message cadence, and host/guest trust
  semantics remain byte- and behavior-compatible with the refactor baseline.
- **No speculative abstractions.** A trait needs more than one real
  implementation or a demonstrated testing seam. A new state object must own
  an invariant, not merely shorten a field list.
- **No forced async conversion.** Tokio stays inside networking. The sim and
  client frame remain synchronous.
- **Preserve frame and tick order.** In particular: host requests apply before
  `Server::advance`; remote guests do not run the local server; sim randomness
  stays separate from presentation randomness; chunk generation/meshing
  budgets retain their current order and limits.

## Target dependency direction

The end state should read in one direction:

```text
executable / platform (args, winit)
                |
                v
client orchestration (input, session, UI, presentation)
       |              |               |
       v              v               v
authoritative sim   renderer       multiplayer bridge
       |              |               |
       +-------> world/content <-------+
```

More concretely:

- `app` may know winit and the client, but not world rules.
- `client` may coordinate sim, renderer, audio, UI, and network sessions.
- `server`/sim may know `World` and simulation inputs/events, but never winit,
  wgpu, UI, or audio.
- `world` may know registry definitions and domain entities, but never client
  screens or rendering resources.
- `renderer` consumes frame data and world meshes; it does not mutate the sim.
- `net` owns transport and wire DTOs; `mp` remains the host-side adapter that
  translates requests into authoritative operations.
- `content` (`registry`, `crafting`, `script`, `atlas` inputs) is addressed by
  stable string ids at persistence and synchronization boundaries.

---

## Pass 0 — establish the refactor baseline

This is a short prerequisite, not an architecture pass.

1. Run and record the existing gates: currently 138 tests pass, one diagnostic
   test is ignored, and clippy passes with warnings denied.
2. Land a formatting-only change. `cargo fmt --check` currently reports drift
   in `mesher.rs` and `renderer.rs`; that noise must not contaminate move-only
   diffs.
3. Record a small manual smoke checklist:
   - title screen and settings open;
   - an existing world loads, plays, saves, and reloads;
   - a new world generates and meshes;
   - inventory and one machine screen work;
   - windowed host accepts a loopback guest;
   - `--server <world>` starts and autosaves.
4. Keep the worktree free of unrelated feature work during each move. If a
   feature must land concurrently, rebase the mechanical slice after it rather
   than resolving a mixed architectural diff.

Every subsequent slice must pass:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

The manual checklist runs at the end of each pass and after any slice touching
`Game::update`, world loading/saving, or the winit event bridge.

---

## Pass 1 — Mechanical Separation

### Goal

Make file and module boundaries match the responsibilities that already exist.
Pass 1 changes where code lives, not who owns it. `Game` and `World` may remain
large structs during this pass; their existing methods may be distributed
across child modules with multiple `impl Game` / `impl World` blocks.

Allowed changes:

- moving types, functions, tests, and `impl` blocks;
- adding `mod`, `pub(crate)`, `pub(super)`, and re-exports needed by the new
  module tree;
- renaming imports made stale by a move;
- extracting a thin `run` entry point so the binary can call the library;
- tiny compile-only adjustments whose behavior is demonstrably identical.

Not allowed:

- grouping fields into new state objects;
- changing method signatures for design reasons;
- reordering update phases, draw passes, or simulation calls;
- replacing direct field access with a new policy-bearing API;
- changing serialization, packet layouts, constants, tuning, or content.

### 1. Introduce the library boundary and thin executable

Create `src/lib.rs` as the package's internal root and reduce `src/main.rs` to
argument handoff and process startup. The library exposes one narrow entry
surface for the binary, for example `wildforge::run(args)`, while keeping game
internals crate-private.

Move the current winit `ApplicationHandler` into `app.rs` and the dedicated
host loop into `dedicated.rs`. Both continue to construct the same `Server`
and use the same multiplayer host session.

Target:

```text
src/
  main.rs              # thin executable entry
  lib.rs               # module map + run entry
  app.rs               # winit ApplicationHandler / event bridge
  dedicated.rs         # --server loop
```

Acceptance:

- `main.rs` contains no game rules or subsystem state;
- windowed and headless invocation accept the same arguments as before;
- `#[cfg(test)] mod tests` belongs to the library so tests exercise the same
  modules the executable uses;
- no public API is exposed merely to make the binary reach internals—the
  library should expose the runner, not every subsystem.

### 2. Split `Game` by existing method clusters

Create a `game/` module. Define `Game`, `Screen`, `Remote`, and small local
support types in `game/mod.rs`; move current methods without redesigning them.

Proposed first-pass layout:

```text
src/game/
  mod.rs                # Game/Screen/Remote state + constructor
  session.rs            # world lifecycle, player save/load, packs/mod reload
  remote.rs             # joining, snapshot pump, chunk streaming
  interaction.rs        # mining, placing, combat, bows, stations, scripts
  survival.rs           # damage, hunger, air, death, respawn, item pickup
  frame.rs              # existing update/frame assembly in its current order
  input.rs              # keyboard, pointer capture, event-facing actions
  ui/
    mod.rs              # shared layout/helpers and build_ui entry
    menus.rs            # title/pause/settings/mods/packs/appearance/join
    inventory.rs        # slots, crafting, armor, containers
    browser.rs          # item browser drawing and navigation
```

The exact file names may adjust when the moves expose a better seam, but each
file must describe one recognizable concern. Do not split solely to satisfy a
line target.

`frame.rs` is intentionally allowed to contain the still-large `update`
method. Breaking its phases apart changes control flow and therefore belongs
to Pass 2. Pass 1 should make its current order easier to inspect, not silently
redesign it.

Suggested review slices:

1. types + constructor;
2. session and remote methods;
3. interaction and survival methods;
4. UI layout/drawing/click handling;
5. input bridge;
6. the unchanged frame/update method last.

### 3. Split `World` by existing domains

Convert `world.rs` into `world/mod.rs` plus child modules. `World` remains the
owner of its current fields. Child modules may implement methods on it and use
parent-private state; no new world service objects are introduced yet.

```text
src/world/
  mod.rs                # World and shared domain types, core block access
  calendar.rs           # weather/season/ire/offering rules
  chunks.rs             # ensure/load chunks, structures, remote chunk insert
  persistence.rs        # metadata, palettes, WFC4/WFC3, entities, mobs, stamps
  lighting.rs           # RGB block light, skylight, relight cascades
  fluids.rs             # finite-water queue and flow
  machines.rs           # furnaces, bloomery, kiln, clamp, anvil, falling blocks
  ecology.rs            # wildlife seed/spawn/tick, projectiles, random ticks
```

Some methods cross domains (`set_block` wakes water, lighting, gravity, and
block entities). Keep those coordinating methods in `world/mod.rs` during
Pass 1. Their dependency policy is Pass 2 work.

Persistence moves require special care: keep file names, textual field names,
defaulting behavior, palette fallback, and RLE bytes unchanged. Existing
round-trip and migration tests are the gate; add a characterization fixture if
a format path lacks one before moving it.

### 4. Separate atlas management from procedural tile generation

The atlas has a natural seam that does not require ownership changes:

```text
src/atlas/
  mod.rs                # public atlas API and stable slot table
  packs.rs              # discovery, scan, override, export, PNG loading
  procedural.rs         # generated built-in tile art
  season.rs             # foliage/player tint transforms
```

Slot numbers and built-in names are persistence-like compatibility surfaces:
move them without reordering. The atlas layout stability, embedded-pack, pack
override, and export round-trip tests must remain unchanged.

### 5. Split renderer construction from frame passes

Do this only after `Game` has moved, to avoid two high-conflict refactors at
once. Keep `Renderer` as one owner in Pass 1.

```text
src/render/
  mod.rs                # Renderer, FrameInput, public frame API
  resources.rs          # device/surface/buffers/textures and resize
  pipelines.rs          # pipeline/layout creation
  shadows.rs            # sun and point-shadow cube passes/cache
  post.rs               # HDR targets, bloom, composite
  frame.rs              # render-pass ordering
  mesher.rs             # existing chunk mesher module
  lights.rs             # existing point-light director
```

WGSL entry points, bind-group layouts, render-pass ordering, target formats,
and light cache keys must not change. Shader validation and existing light
director tests are mandatory gates; a visual smoke run covers what headless
tests cannot.

This renderer split is the last Pass 1 source move. It may be deferred if the
earlier changes sufficiently reduce collision pressure; `main.rs`, `world.rs`,
and `tests.rs` are the priority.

### 6. Split the test suite along the same boundaries

Move shared fixtures to `tests/mod.rs`, then group tests by the subsystem they
characterize:

```text
src/tests/
  mod.rs                # shared registry/world/temp-dir helpers
  registry.rs           # content graph, recipes, mods, scripts
  worldgen.rs           # terrain, biomes, structures, ore generation
  world.rs              # chunks, saves, ticks, fluids, light, weather
  machines.rs           # containers, furnaces, steelworks, glassworks
  mobs.rs               # wildlife, wardens, breeding, projectiles
  multiplayer.rs        # protocol and loopback host/guest behavior
  rendering.rs          # atlas, light director, models, WGSL validation
  player.rs             # inventory, physics, survival, bows/armor
```

This remains an in-crate unit-test tree for access to internals. Moving to
external integration tests would force a broad public API and is not part of
this plan.

### Pass 1 completion criteria

- All baseline automated and manual checks pass.
- The executable entry contains startup only.
- `Game`, `World`, atlas, and tests have coherent module maps with no semantic
  redesign mixed into their history.
- No save fixture, encoded protocol message, registry id, atlas slot, shader
  output contract, tick frequency, or update order changed.
- Each module has a short module-level comment stating its responsibility.
- Temporary visibility widening is reviewed: prefer `pub(super)` or
  `pub(crate)`; do not leave internals globally `pub` by accident.
- The Pass 1 branch is allowed to settle before ownership extraction begins.

---

## Pass 2 — Responsibility Extraction

### Goal

Turn the module map from Pass 1 into real ownership boundaries. The root
objects should orchestrate a small number of cohesive subsystems instead of
holding every field and allowing every method to mutate everything.

Pass 2 is not measured by the number of structs created. It succeeds when:

- invariants have clear owners;
- update phases state their inputs and outputs;
- simulation, presentation, UI, and transport cannot accidentally reach into
  each other;
- direct mutable access is replaced by small domain operations where policy
  matters;
- future features have an obvious home.

### 1. Characterize order-sensitive behavior

Before extracting a responsibility, protect its current behavior. Add focused
tests where the existing suite is too broad or indirect:

- host messages are applied before the fixed sim tick;
- pausing stops local simulation but does not stop a windowed host serving
  guests;
- guest mode never generates or saves authoritative chunks;
- sim and presentation RNG streams remain independent;
- a block edit triggers the same water, gravity, relight, remesh, entity, and
  network-log side effects in the same observable frame/tick;
- save/load and hot registry remap preserve named content and unknown
  placeholders;
- container transactions remain identical between local and remote paths.

Prefer observable assertions over tests of private call order. Golden bytes
are appropriate for the protocol and WFC4 codec; state snapshots are better
for simulation behavior.

### 2. Extract cohesive client state

Start with fields that already move and reset together. Candidate owners:

- `InputState` — keys, held buttons, pointer capture, cursor/scroll state, and
  input cooldowns;
- `UiState` — current screen, UI cursor/focus/search/browser state, held stack,
  screen animation, and menu-specific selections;
- `SurvivalState` — health, hunger, nutrition, air, damage/fall timers, armor,
  and spawn point;
- `InteractionState` — mining, bow draw, brushing, station channels, crafting
  grid, selected hotbar slot, and action cooldowns;
- `PresentationState` — particles, cosmetic RNG, audio/footstep timers,
  weather/lightning presentation, toasts, pickup animations, and camera nudge;
- `MultiplayerState` — host/remote/discovery state, sleep/chat/join state, and
  snapshot interpolation clocks;
- `ContentRuntime` — registry, script host, mod/pack stamps, active pack data,
  and hot-reload diagnostics.

These names are proposals, not a mandate. Extraction rules:

1. Move one cohesive field group at a time.
2. Give it methods that enforce a real invariant (for example,
   `SurvivalState::apply_wild_damage`), not generic getters for every field.
3. Keep coordination on `Game`; do not let subsystems call one another through
   back-pointers.
4. Avoid a `GameContext` containing `&mut` references to everything—that would
   recreate the god object as a parameter and make borrowing harder.
5. When two systems need to communicate, prefer a small command/event/result
   value over cross-mutation.

Renderer, `Server`, `Player`, and `Camera` may remain directly owned by the
client root. They are already coherent owners and do not need wrapper structs
for symmetry.

### 3. Make the client frame an explicit pipeline

After state extraction, reduce `Game::update` to orchestration. Preserve its
current ordering while naming the phases:

```text
clock + input timers
presentation timers
remote/host network pump
authoritative Server::advance + SimEvent collection
player input/physics/survival
interaction commands
world streaming + generation + meshing budgets
presentation from sim/network events
frame data construction
UI construction
Renderer::render
```

Each phase should take the narrowest useful inputs and return events or frame
data. `Game::update` remains the one place that orders them; the goal is not a
callback graph.

Use existing event types where they fit (`SimEvent`, `HostFx`) and introduce
small client-side events only when they remove a real dependency. Do not build
a universal event bus.

Useful end-state signatures might resemble:

```rust
fn advance_session(&mut self, dt: f32) -> Vec<SessionEvent>;
fn update_player(&mut self, dt: f32, input: PlayerInput) -> Vec<ClientEvent>;
fn build_frame(&mut self, dt: f32) -> FrameData<'_>;
```

The exact borrowing model should follow what Rust makes simple. If a proposed
split requires pervasive `RefCell`, `Rc`, or unsafe pointers, the boundary is
wrong and should be reconsidered.

### 4. Encapsulate world domains behind operations

`World` is both the aggregate state and the spatial authority; it should
remain the root, but its domain internals should stop being public mutation
surfaces.

Priorities:

1. **Persistence boundary.** Keep codecs and filesystem I/O in
   `world::persistence`. Expose `load`, `save_modified`, and explicit snapshot
   helpers; simulation code should not construct file paths.
2. **Block mutation boundary.** Make `set_block` (and a small number of
   specialized operations) the owner of edit side effects. External code must
   not mutate chunk arrays, light queues, block entities, or the edit log
   independently.
3. **Machine store.** Encapsulate `block_entities` and `pending_drops` behind
   container/station operations and read-only iteration needed for rendering.
4. **Ecology state.** Group mob/projectile collections, stable-id allocation,
   seeded-chunk markers, and spawn clocks. Expose queries, damage/feed
   commands, snapshots, and tick results rather than raw mutable vectors.
5. **Fluid state.** Own the water queue and dedup set together. World block
   mutations wake it through one method; callers cannot enqueue inconsistent
   work.
6. **Lighting state.** Keep derived light data rebuildable and unsaved. Block
   changes invalidate/cascade through explicit operations; gameplay reads a
   scalar/RGB query API.

Do not force every world domain into an independently borrowed struct at once.
Spatial systems legitimately share block access. A child module with private
functions is better than a state object that fights the borrow checker or
duplicates world access.

### 5. Clarify commands, events, and snapshots

There are three different kinds of cross-boundary value and they should stay
distinct:

- **Commands/requests** ask an authority to do something (`C2S`, script
  commands, local interaction intent).
- **Events** report that the authority did something (`SimEvent`, `HostFx`,
  presentation cues).
- **Snapshots/frame data** are read models for networking or rendering and
  contain no authority.

Local and remote interactions should converge on domain operations, not on
duplicated rules. For example, mining validation/drop/ire/edit logic should
have one authoritative operation that the local client and `HostSession` can
both invoke with the appropriate actor context. This extraction may expose
behavioral mismatches; preserve current behavior during the refactor, then
fix mismatches in clearly labeled follow-up changes.

Keep render snapshots presentation-oriented. Do not hand `Renderer` or UI
unrestricted mutable access to `World` merely to avoid building a small read
model.

### 6. Tighten visibility and document the internal engine surface

Once callers use explicit operations:

- make subsystem fields private;
- reduce temporary `pub(crate)` items to `pub(super)` or private where
  possible;
- keep the library's external surface intentionally small (`run` plus any
  genuinely useful tooling entry points);
- add a short architecture section to the README linking this plan and
  describing the sim/client/world/render/content/network boundaries;
- update stale module comments encountered during moves, including the chunk
  height comment, in a documentation-only slice.

A separate reusable engine crate becomes worth discussing only when there is
a second real consumer (editor, server tool, another game) or a demonstrated
compile-time/dependency benefit. Directory names are not a reason to publish
an abstraction.

### Pass 2 completion criteria

- `Game::update` is a readable orchestration method whose phases are visible
  without scrolling through their implementations.
- `Game` directly owns a small set of coherent subsystem states rather than
  dozens of unrelated timers, flags, and collections.
- `World` remains the spatial aggregate but its chunks, entity store, ecology,
  fluid queues, lighting invalidation, and persistence paths are not freely
  mutated by clients.
- Local play, windowed hosting, guest play, and headless hosting still use the
  same authoritative `Server` rules where they do today.
- Renderer/UI/audio code cannot enter the simulation dependency graph.
- No universal ECS, service locator, event bus, or everything-context was
  introduced.
- Automated gates and the full smoke checklist pass with unchanged save and
  wire compatibility.
- As a review guardrail—not the objective—ordinary implementation modules
  should generally stay below roughly 1,200–1,500 lines. A larger cohesive
  module is preferable to an arbitrary split, but a file above that range
  deserves an explicit reason.

---

## Recommended implementation sequence

Each numbered item should be independently reviewable and leave the game
runnable:

1. Formatting-only baseline and smoke notes.
2. `lib.rs`, thin `main.rs`, `app.rs`, and `dedicated.rs`.
3. Move `Game` lifecycle/session/remote methods.
4. Move interaction and survival methods.
5. Move UI and input methods.
6. Move the existing frame/update method; finish the `game/` module map.
7. Split `world.rs`, persistence first and coordinating mutation last.
8. Split `tests.rs` to mirror the source domains.
9. Optionally split atlas and renderer if collision pressure still warrants
   it; close Pass 1 and run the full smoke checklist.
10. Add order-sensitive characterization tests for Pass 2.
11. Extract client state owners one at a time.
12. Turn `Game::update` into the named frame pipeline.
13. Encapsulate world persistence and block mutation.
14. Encapsulate machines, ecology, fluids, and lighting where their invariants
    justify state owners.
15. Converge local/remote commands on authoritative domain operations.
16. Tighten visibility, update architecture docs, run compatibility and smoke
    verification, and close Pass 2.

## Stop conditions

Pause and reassess a slice when any of these occurs:

- it requires a save, registry-id, atlas-slot, or protocol migration;
- deterministic generation or sim tests change without an understood reason;
- the frame or tick order becomes implicit;
- borrowing pressure leads toward pervasive interior mutability;
- a proposed abstraction has only one caller and no invariant;
- a move-only diff begins accumulating gameplay fixes;
- headless and windowed hosting start taking different simulation paths.

The safe response is to make the slice smaller, add a characterization test,
or defer the design change to a separate proposal. The purpose of this work is
to make Wildforge easier to change without sanding away the specific engine it
has already become.

---

## Implementation record — completed 2026-07-22

Both passes were completed in place without adding a third-party engine, a
second Cargo package, an ECS, a service locator, a universal event bus, or
interior-mutability workarounds.

### Pass 1 result

- `main.rs` is a three-line handoff to the library's sole external entry point,
  `wildforge::run`.
- Winit lifecycle and headless hosting live in `game/app.rs` and
  `dedicated.rs`.
- The former `main.rs`, `world.rs`, `tests.rs`, `atlas.rs`, and `renderer.rs`
  convergence points are coherent module trees under `game/`, `world/`,
  `tests/`, `atlas/`, and `renderer/`.
- Every new module states its responsibility in a module-level comment. The
  2,325-line `atlas/procedural.rs` is the one deliberate size exception: it is
  a single ordered deterministic pixel recipe, and the reason is documented at
  the top of that file.
- Save formats, protocol values, atlas slots, shader contracts, tick rates, and
  frame ordering were retained.

### Pass 2 result

- `Game` now coordinates cohesive `InputState`, `UiState`, `SurvivalState`,
  `InteractionState`, `PresentationState`, `MultiplayerState`, and
  `ContentRuntime` owners alongside the already-cohesive renderer, server,
  player, and camera.
- `Game::update` is a visible six-phase orchestration pipeline: begin frame,
  feedback, session authority, content/toasts, player, then frame build/render.
  The extracted helpers preserve the original ordering.
- Presentation randomness owns its stream separately from the simulation seed,
  and survival state owns its attackability invariant.
- World chunks, block entities, ecology collections, gravity state, edit log,
  and pending give/drop queues are private. Callers use read views and targeted
  operations; test-only raw views remain behind `cfg(test)`.
- `World::break_block` and `World::place_block` are the shared authoritative
  operations used by local interaction and host-side guest requests.
- Renderer GPU chunk storage is private, and the normal library API exposes
  only `run`.
- The README now records the client/simulation/world/render/network/content
  dependency direction and links back to this plan.

### Verification record

The baseline before structural work was 138 passing tests and one ignored
diagnostic. After reconciling the completed refactor with the current engine,
the tree has 158 tests: 157 pass and the same diagnostic remains ignored. New
characterization coverage protects pause/host behavior,
guest-world authority, simulation/presentation RNG separation, and the complete
fan-out of an authoritative block edit.

Final automated gates:

```text
cargo fmt --all -- --check                         PASS
cargo clippy --all-targets --all-features -- -D warnings  PASS
cargo test                                         PASS (157 passed, 1 ignored)
git diff --check                                   PASS
```

Runtime smoke evidence:

- title and settings screens initialized and rendered to captures;
- a fresh world generated and meshed through frame 120 in an isolated save
  root, with the in-world inventory rendered in the same run;
- the dedicated `--server smoke` path bound port 27431 and created its world
  metadata in an isolated save root;
- protocol and windowed-host loopback join/stream/edit behavior passed the
  in-process networking test;
- WFC4/WFC3 palette/remap, metadata, block entities and machines, mobs, and registry
  hot-remap save/reload paths passed their round-trip tests.

The smoke runs used disposable directories and did not modify repository save
data. WSLg cannot synthesize a normal window-manager close reliably, so the
close-triggered save portion uses the round-trip suite as its verification
evidence; an attempted synthetic X11 close was discarded rather than counted
as a game result.
