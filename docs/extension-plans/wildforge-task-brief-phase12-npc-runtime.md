# Task Brief: NPC Runtime, Dialogue Screen, and Quest Journal

**Repo:** WildForge, new branch `feat/npc-runtime` (off `main`, after
`feat/npcs-dialogue-quests` lands)
**Primary files:** new `src/npc/` module (NPC instance + behavior);
touches `src/script.rs` (`Cmd::SpawnNpc`, `quest_progress` host fn +
`Cmd::QuestProgress`), `src/game/mod.rs` (`Screen::Dialog` /
`Screen::Journal`), `src/game/actions.rs` (mob-interact → talk pathway,
assembly-marker consumption), `src/game/ui.rs` (dialog + journal panels),
`src/world/mod.rs` + `src/world/chunks.rs` (NPC spawn/persist), `src/world/
pieces.rs` (marker kind surface), `src/game/content.rs` (reload rule).
**Related design doc:** `wildforge-engine-extensions-spec.md` Part 3.1–3.3.
**Data-layer prerequisite (landed):** `base/npcs.toml` +
`dialogue.toml` + `quests.toml` load into the registry as `NpcDef`,
`DialogueDef`, `QuestDef`; NPCs already synthesize a companion `AnimalDef`
(`AnimalDef.npc: Option<usize>`, `is_npc_species`), so every mob pipeline
already treats them as ordinary species.

## Status Going In

The data layer of the spec's Part 3.1–3.3 is committed and green on
`feat/npcs-dialogue-quests`:

- `NpcDef { name, label, dialogue, species, talk_radius, patrol, pause,
  sound_pitch }` — a companion species is synthesized at load from the
  NPC's tex/model; the companion is non-hostile, never flees, never tames,
  has no drops/belly/prey, and empty `biomes` (never wildlife-spawned).
- `DialogueDef { id, npc, root, nodes }` where each node has an optional
  `condition` `ScriptHook`, `text`, and `choices` each with optional
  `condition`/`callback` hooks and a `next` node id (`None` closes).
- `QuestDef { id, title, description, giver, prereq, objectives, rewards }`
  with `QuestReward::Give(ItemId, count)` / `SetFlag(flag, value)`.
- Runtime lookups: `npc_id`, `dialogue_id`, `quest_id`, `is_npc_species`.
- `ScriptHook { mod_id, fn_name }` (`""` mod = any mod) already exists and
  parses `"mod:fn"` / bare `"fn"`.

The runtime layer is entirely untouched (verified): no `Cmd::SpawnNpc`, no
`npc_cap`, no `Screen::Dialog`/`Journal`, no `quest_progress`, no
`spawn:npc:` marker consumer, no talk pathway, no NPC persistence.

## Goal

Turn the committed definitions into live game systems:

1. **3.1** NPCs exist in the world — spawned deterministically from
   authored defs + the Phase 10 `AssemblyMarker` seam, walking `patrol`
   loops or standing fixed, never acting as wildlife, persisted with the
   world, and **talked to** by right-clicking the mob.
2. **3.2** Right-clicking an NPC opens `Screen::Dialog`: data-driven
   branching text with `condition` pruning and `callback` execution, both
   dispatched through the existing `ScriptHost::dispatch` (KV as the flag
   store). ESC / click-away ends; terminal nodes close.
3. **3.3** Quest state lives in the existing per-mod KV
   (`storage_get`/`storage_set`); `quest_progress` host fn queues
   increments applied on the game loop; completion pays `rewards` through
   `give`; `Screen::Journal` lists accepted quests with `x / count`
   progress.

## In Scope

### 12a — NPC spawn, movement, persistence (3.1 runtime)

- **`Cmd::SpawnNpc(String, EntityPos)`** mirroring `Cmd::SpawnAnimal`, gated
  by a new `npc_cap` (constant in `world/mod.rs`, default small — authored
  NPCs are rare). Apply in `apply_script_cmds`: look up `reg.npc_id`, spawn
  the companion species, and attach the `NpcInstance`.
- **New `src/npc/` module.** A lightweight `NpcInstance` (not a `MobState`
  extension — fauna fields don't apply): holds `species`, `npc` def id,
  fixed `EntityPos` or a `patrol` walker (waypoint index, phase timing,
  `pause` countdown), `talk_radius`, and a `dialogue` id. NPCs idle in
  place or walk the patrol loop; they never flee/hunt/tame/graze; they take
  no damage and cannot be killed this pass.
- **Deterministic spawn.** Authored NPCs spawn from a `[npc]`-bearing mod's
  `on_world_start` via a new `spawn_npc` host fn (mirror `spawn_animal`).
  **Phase 10 seam — the first real consumer of `AssemblyMarker`:** the
  piece walk already returns `Vec<AssemblyMarker> { kind, at }`, but
  `place_assembly`'s return is currently dropped at `chunks.rs:930`. Reserve
  the tag convention **`spawn:npc:<npc_id>`** (a piece's `markers` entry
  lists it); when chunkgen places an assembly, markers with that prefix
  spawn the named NPC at the resolved world `at` (clamped to the surface if
  `at` is not walkable). Document the convention in `pieces.rs` marker docs.
  Determinism: `place_assembly` is already seeded by the per-chunk hash, so
  same seed → same NPC placement, and `structure_chunks` protection already
  makes it survive regen.
- **Persistence.** Spawned NPCs persist with the world the same way mobs do
  (companion species rides the existing mob save path). Spawn *sources*
  (markers / `on_world_start`) stay deterministic — on load, the world's
  NPCs are the saved set, not re-rolled.
- **Talk interaction (actions.rs:1337 mob branch).** When the crosshair mob
  is an NPC species (`reg.is_npc_species`), right-click opens
  `Screen::Dialog` instead of feed/tame/cargo paths; NPCs are excluded from
  drop/flee/tame/hunt logic everywhere.

### 12b — Dialogue screen (3.2 runtime)

- **`Screen::Dialog { npc: usize, node_id: String, choice_sel: usize }`**.
  Rendering mirrors the stall/menu pattern (ui.rs:1910 `Screen::Stall`):
  framed panel, node text, choices as selectable rows. ESC / click-away
  closes; up/down + Enter pick; `next` advances nodes; `None` closes.
- **Dispatch signature** (extend `dispatch` args): `(npc_id, node_id,
  player_uid)` as strings + a KV-namespace token so scripts read/write
  per-player flags. `condition` returning `false` prunes that node/choice;
  `callback` runs on selection (rewards, flag sets). Node text may
  interpolate returned script values — document the convention (recommended:
  a `node_text` callback fn per node whose return string replaces the
  authored text; absent hook → authored text verbatim).
- **Flow control:** opening reads `reg.dialogue_id` from the NPC def; each
  frame re-evaluates visible conditions (cheap: only the current node's
  choices); selecting a choice runs its callback, then moves to `next` or
  closes.

### 12c — Quest journal + progression (3.3 runtime)

- **`quest_progress(quest_id, objective_key, n)`** host fn (mirror
  `hud_message`): queues `Cmd::QuestProgress(String, String, u32)` — never
  writes KV from inside the script. On the game loop:
  - `accepted` state: `quest_<id>` = `accepted` per player under a
    `player_<uid>` KV namespace (the current mod's KV, already persisted).
  - Increment `progress/<obj>`, detect completion (counts from the def),
    set `quest_<id>` = `done`, and apply `rewards` through the existing
    `give`/inventory path (`Give` → inventory, `SetFlag` → KV).
  - A `quest_accept` host fn / callback opens the quest; `prereq` must be
    done before accept.
- **`Screen::Journal`** (hotkey + dialog-fallback): lists accepted quests,
  per-objective `x / count`, and a compact recent-completion log. Read-only;
  looks up defs + KV state. No beacon tracing or route markers.
- **Hot-reload rule (11c):** because accepted state lives in KV and defs in
  TOML, reload must keep accepted-but-redefined quests valid — track
  `quest_<id>` accepted state independently of the def so re-authored
  objectives don't orphan progress. Enforce in `content.rs::reload_mods`
  with the same refuse-to-reload style used for material identity changes.

## Out of Scope

- **3.4–3.7** (settlement reputation/tiered growth, blueprint-gated
  recipes, enemy archetypes, ire generalization) — later spec items.
- **NPC combat, damage, deaths** — non-hostile; cannot be killed this pass.
- **Scheduling beyond fixed/patrol** (sleep, work assignments).
- **Voice/audio dialogue** beyond existing `play_sound`.
- **Journal beyond read-only** — no quest beacons, no route markers, no
  in-game quest editor, no shared/leaderboard completion.
- **Relationship economy** — `relationship` stays a read/write flag.

## Verification

- **Spawn determinism:** an assembly with a `spawn:npc:base:elder` marker
  places the NPC at the same `at` for the same seed, survives regen, and
  never spawns via the wildlife pass (`is_npc_species` guards).
- **Talk flow:** right-clicking the NPC opens `Screen::Dialog` with the
  root node; a `condition` script returning `false` hides that choice; a
  `callback`'s `storage_set` lands in the mod KV and survives save→load.
- **Quest lifecycle:** accept → `quest_progress` → complete → rewards via
  `give`; journal `x / count` matches KV state; `prereq` gate enforced.
- **Reuse check:** NPCs don't flee/tame/drop/hunt/spawn-wild; `MOB_CAP`
  and `npc_cap` respected.
- **Hot reload:** editing a dialogue node / quest def mid-session re-reads
  without orphaning accepted progress.
- Existing suite stays green: `cargo test --locked --all-targets` and
  `cargo clippy --locked --all-targets -- -D warnings` (per README).

## Suggested PR Description Framing

> Implements the runtime half of spec Part 3.1–3.3 on the committed data
> layer: deterministic NPC spawning (authored `spawn_npc` + the Phase 10
> `spawn:npc:<id>` assembly-marker consumer), a fixed/patrol NPC instance
> that never behaves as wildlife and persists with the world, a data-driven
> `Screen::Dialog` whose condition/callback hooks run through the existing
> script API, and quest tracking whose state lives entirely in the existing
> per-mod KV, surfaced by a read-only `Screen::Journal`. **Out of scope:**
> 3.4–3.7, NPC combat/scheduling, and any journal feature beyond the
> read-only log.

## Roadmap After This

- **3.4 settlement reputation** reads the per-NPC-per-player relationship
  flags seeded this pass.
- **3.5 blueprint-gated recipes** plugs into the quest `reward` "unlock"
  slot (a `quest_done` KV flag gating a recipe's `unlock`).
- **2.4/2.5 skinning:** quest-gated features consume the same KV-flag check
  the dialogue `condition` path already uses.
