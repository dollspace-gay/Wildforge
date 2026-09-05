<!-- wildforge:guide -->

Content inspection may retain a rejected pack on the menu. Local world entry
checks its registry and compiled scripts before starting a worker; guest entry
checks its local registry. Reload prepares validated definitions and scripts,
checks live material/quest compatibility, then replaces the active content.
Preparation failure leaves the current registry, scripts, and inventories intact.

# Graphical application

The windowed app coordinates input, UI, sessions, streaming, actions, and presentation. Existing broad Game access is being replaced with cohesive owners.

`mesh_jobs.rs` owns CPU snapshot processing and per-chunk deduplication through
the shared background owner. `streaming.rs` supplies immutable inputs, validates
finished meshes, and uploads them through the existing renderer. Startup/runtime
worker failures reach the entry screen or pause notification; native GPU proof
is required in addition to worker buffer/lifecycle tests.

Terrain streaming compares the live immutable preparation context before using
its worker owner. Mesh completion retains the input registry and variant signature;
the pool discards mismatched results and releases their slots before the adapter
checks the live chunk's dirty state and performs the existing GPU upload.

`world_loading.rs` owns one creation or entry operation until its worker joins.
`world_loading_work.rs` prepares a private world; `world_loading_ui.rs` maps
requests and terminal outcomes to screens and session adoption. UI state holds
only presentation status. Requested registry identity guards entry across content
reload, separately from placeholder definitions restored into the loaded world.

Start with `actions.rs`, `app.rs`, `browser.rs`, `capture.rs`, `combat.rs`, `containers.rs`, `content.rs`, `demos.rs`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Keep simulation order explicit. Guests apply host state, while UI and rendering consume it. Preserve real interaction paths when extracting helpers.

Guest admission, content mapping, and snapshot receiver lifetimes are shared
with the agent through `client_session/`. Supply the first-frame milestone after
the entry mesh is uploaded; keep interpolation and UI responses here. Content
reload rebinds the retained host palette to the new local registry before
subsequent network state is interpreted.

Shared entity reconstruction supplies authoritative mob fields, projectiles,
loose items, and falling blocks. Presentation updates mob yaw and animation
phase from interpolation spans after reconstruction; it must preserve host
identity, canonical position, health, growth, and feeding state.

## Focused checks

```sh
python3 tools/run_gameplay_proofs.py
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.

`startup.rs` constructs the windowed app in content/renderer/session order.
`input.rs` owns held input and private pointer capture/warp state; its pointer
operations receive the window and camera directly. `navigation.rs` owns screens
and UI navigation state, with an explicit coordinator for container closure,
held-item return, focus, and capture. `presentation.rs` owns cosmetic variation,
particle emission, and gait; these operations do not receive simulation state.
`content_watch.rs` observes reload inputs; publication stays in `content.rs`.

`text_input.rs` edits bounded UI fields and returns explicit create/sign actions.
`keyboard_events.rs` preserves startup, join, chat, in-world text, then key-repeat
filtering order. `app.rs` is the platform bridge; text edits have no world access.

`widgets.rs` paints buttons, item slots, and cursor-held stacks from explicit
read-only inputs. `inventory_panel.rs` owns shared inventory/crafting geometry
and the borrowed storage-grid view. Drawing and hit testing use the same layout;
station controls, alternate loadout geometry, and browser placement retain
their separate screen policies.

Production modules import their domain dependencies explicitly. The game facade
contains composition state and selected test entry points; it does not supply a
broad namespace to child modules. Input, navigation, widgets, and presentation
helpers expose their own contracts. Native proof imports name their actual
dependencies even though the proof remains attached to the action adapter.
