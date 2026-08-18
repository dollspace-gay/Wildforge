# Task Brief: Third-Person Camera (capability E2)

**Repo:** WildForge, branch `feat/capability-ramp`
**Primary files:** `src/camera.rs` (modes, chase + orbit placement),
`src/game/keymap.rs` (Tab cycle), `src/game/app.rs` (orbit wheel zoom),
`src/game/frame.rs` (placement, local body, held-light anchor),
`src/game/session.rs` (per-world camera on entry), `src/world/mod.rs` +
`persistence.rs` + `storage.rs` (world.toml `camera` line)
**Related design doc:** `belt-quest/docs/port-plan.md`, capability E2.

## Goal

Over-shoulder chase cam with wall push-out and mouse aim; Tab toggles an
orbit "factory" cam; the chosen view persists per world (`camera` line in
`world.toml`).

## What shipped

- **Camera modes** (`CameraMode::First | Third | Orbit`), the default
  remaining first person. `Camera::turn`, `forward`, and `tangent_forward`
  are mode-aware; orbit drag spins the camera and never moves the aim.
- **Chase placement** (`Camera::place_chase`): the eye floats behind the aim,
  over the right shoulder, with a wall push-out that runs the topology-aware
  DDA in the player's chart and, on a hit, parks the eye in the last free
  cell pulled a hair toward the player (near plane clears the wall).
- **Orbit placement** (`Camera::place_orbit`): spherical camera around the
  player looking back at them; drag to spin, wheel to zoom (2..20 blocks,
  gated in `app.rs` so the hotbar wheel is untouched in first/third).
- **Local avatar**: in chase/orbit the local player gets a body
  (`emit_humanoid_interpolated`, own style, held item / implement in hand,
  gait from movement) and the first-person hand model is suppressed; the
  held-item light anchors to the body's hand rather than the camera.
- **Persistence**: `world.toml` gains a `camera = "first"|"third"|"orbit"`
  line (default `first`, backward compatible). `World::set_camera` persists
  on toggle; the view is restored on world entry. Guests keep first-person
  (Tab is the roster in multiplayer, so they could never toggle back).
- **Tests**: 5 camera tests (orbit framing + drag independence, mode-key
  round-trip, chase wall push-out in a real world + no-wall float) and a
  world-meta camera round-trip; both visual-polish qualification hashes
  refreshed (`app.rs`, `frame.rs`, `world/mod.rs` are qualification sources).

## Verification

- `cargo clippy --lib --tests -- -D warnings` clean.
- `cargo test --lib` green (suite now 929 passing).
- `wildforge --mod-qualification mods` PASS.

## Keybind notes

- Single-player **Tab**: first → chase → orbit → first (with a click).
- Multiplayer **Tab**: roster (unchanged); camera mode is per world but
  guests do not inherit the host's view.

## Out of scope (later capabilities)

- E3 combat depth (stamina/dodge/block) and floating damage / health bars.
- The orbit camera is a plain spherical view; E11 UI screens may add an
  overlay HUD for it.
