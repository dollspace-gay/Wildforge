# Task Brief: Enemy AI Archetype Extension + Nests (capability E9)

**Repo:** WildForge, branch `feat/capability-ramp`
**Primary files:** `src/registry.rs` (the `BehaviorArchetype` enum grows
eight new variants, the per-archetype param structs behind `ArchetypeParams`,
and the data-driven `NestDef`/`RawNestToml`/`nests.toml` parsing),
`src/mobs.rs` (Mob fields `support_cd`/`controller_cd`/`phaser_cd`, the
`archetype_tick` dispatch, `hunt_wheel`'s rusher/sniper/phaser branches,
the tank/shield `hurt` hooks, and the `HealPulse`/`SpawnMinions`
`MobEvent`s), `src/world/ecology.rs` (`settle_dead_mobs` swarm brood,
controller/support passes, and the nest-gated `tick_hostile_spawns`),
`src/world/mod.rs` + `src/world/storage.rs` (the persisted `World::nests`
map, maintained by `set_block_state_at`, saved/loaded like `gated`),
`src/world/chunks.rs` (`spawn:nest:<id>` markers), `src/server.rs`
(HealPulse/SpawnMinions application), `src/registry/runtime.rs`
(`nest_index_for_block`/`nest`), `src/mod_lint.rs` (archetype + nest
validation tests), `src/tests/archetypes.rs` + `src/tests/nests.rs`.
**Related design doc:** `belt-quest/docs/port-plan.md`, capability E9
(row 104), dep E3 (damage/backstab interaction).

## Goal

Open the warden behavior catalog from the closed four-archetype enum to a
data-driven set of eight new generic archetypes, and give hostile mobs a
real spawn source in the form of **nests/dens as spawn-gate blocks** —
a nest block is the only source of its bound species nearby, and clearing
it stops those respawns. Both are engine features: the archetypes and the
nest schema ship in WildForge for any mod to use; the species catalog and
the specific nests are belt-quest content, not base's.

## What shipped

- **Eight new archetypes** (each a focused behavioral layer over the
  Standard pipeline, exactly the Brute/Builder pattern):
  - **Rusher** — sprints at a burst multiple of its speed while hunting.
  - **Tank** — takes reduced knockback (holds its ground) on top of its
    authored `resist` data.
  - **Sniper** — holds an authored distance band (retreat when too close,
    close when too far, fire from inside it).
  - **Support** — every `interval` seconds emits a `HealPulse` the server
    applies to hostile allies within `radius`.
  - **Swarm** — releases its brood (a companion species) where the mother
    dies.
  - **Controller** — periodically summons its companion species while it
    lives and fights, capped at `max` living nearby.
  - **Phaser** — on a cooldown, blinks to a valid spot just short of its
    target instead of crossing the ground.
  - **Shield-Bearer** — damage from the direction it faces is reduced (the
    E3 backstab check, inverted); it stays vulnerable from behind.
- **Data-driven knobs**: each archetype reads an optional `Option<…Def>`
  in `AnimalDef.archetype: ArchetypeParams` with engine defaults when the
  mod omits it (`rusher = { rush_mult = 2.4 }`, `sniper = { keep_min = 9,
  keep_max = 16 }`, `swarm = { spawn = "…", count = 3 }`, etc.). The
  `behavior` validation now accepts all twelve strings; swarm/controller
  must name a companion species or the mod fails qualification.
- **Nests/dens**: `nests.toml` (`[[nest]] { id, block, species, radius,
  interval, cap }`) parses into `Registry.nests`; `World::nests:
  HashMap<BlockPos, usize>` is maintained by the authoritative voxel write
  (`set_block_state_at`): placing a nest block records it, breaking or
  replacing it clears the record. `tick_hostile_spawns` spawns a nest's
  species at its surface (light- and ire-gated, capped by `cap` within
  `radius`), and **species bound to a nest are excluded from the ire-ring
  roster** — the nest is their only source, so clearing it stops the
  respawns. Records persist as a `WFN1` sidecar (like `gated`), drop
  cleanly when a mod removes a nest, and self-heal when the marker block
  is gone. `spawn:nest:<id>` worldgen markers place nests through the
  ordinary block path.
- **Server application**: `HealPulse` heals hostile allies (capped at
  their def health); `SpawnMinions` spawns the controller's companions
  (bounded, MOB_CAP-aware). Both are host-authoritative; the sim stays
  MP-safe because guests derive archetypes from the shared species defs and
  the per-mob rhythm fields are transient (like `rage`).
- **Validation + tests**: mod-lint catches unknown behaviors, a
  companion-less swarm/controller, and unresolvable nest blocks/species.
  14 archetype tests (parsing + one behavior test per archetype, all via a
  temp mod) and 2 nest tests (spawn-then-clear, and save/reload round-trip)
  added. Full suite green.

## Verification

- `cargo clippy --lib --tests -- -D warnings` clean.
- `cargo test --lib` green (997 passing, 0 failed, 23 ignored).
- `wildforge --mod-qualification mods` PASS.
- Geode qualification hash refreshed (`src/world/mod.rs` is a
  qualification source; the visual hash is unchanged).

## Content notes

- No base species were added and no base content changed — the archetypes
  and nests are general-purpose engine features, demonstrated by tests.
  The belt-quest mod (C6) authors the species and nests.
- A species with a nest def becomes nest-bound: it no longer spawns from
  the ire ring. This is the contract that makes "clearing the nest stops
  respawns" true.
- Per-mob archetype state is transient (host-side), consistent with the
  existing warden pipeline (wardens dissolve on save); nests themselves
  persist, but the mobs they produce do not.

## Out of scope (later capabilities)

- The belt-quest species catalog, nest layouts, and worldgen placements.
- Player-facing nest UI/notification beyond the existing whisper path.
- Syncing per-mob archetype state to guests (derived from species defs).
