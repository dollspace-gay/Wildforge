# Task Brief: Friendly NPCs, Dialogue, and Quest Tracking

**Repo:** WildForge, new branch `feat/npcs-dialogue-quests`
(off `main`, post-merge of `feat/multiblock-recognition`)
**Primary files:** a new `src/npc/` (or `src/game/npc.rs`) module; touches
`src/script.rs` (event dispatch + a `Cmd::SpawnNpc`), `src/registry.rs`
(new `npcs.toml` / `dialogue.toml` / `quests.toml` read + `Npc`/`Dialogue`/
`Quest` defs + resolve), `src/game/mod.rs` (new `Screen::Dialog` /
`Screen::Journal` variants), `src/game/actions.rs` (mob-interact → NPC
talk pathway), `src/game/ui.rs` (dialog + journal rendering),
`src/world/mod.rs` (NPC persistence), `base/npcs.toml` /
`base/dialogue.toml` / `base/quests.toml` (new base content files).
**Related design doc:** `wildforge-engine-extensions-spec.md` — this task
implements Part 3.1 ("Friendly NPC Primitive"), 3.2 ("Dialogue System"),
and 3.3 ("Quest Tracking"). No other Part 3 items.

## Status Going In

Work begins by branching `feat/npcs-dialogue-quests` off `main`
(post-merge of the Phase 10 branch), committing incrementally there, and
merging back when green. Phase 10 (2.3 piece-based procedural structures)
is merged into that base truth:

- Pieces/assemblies stamp into the ordinary chunk grid; the walk returns
  **`Vec<AssemblyMarker> { kind, at }`** per placed piece — the spec's
  2.4/2.5 seam, currently *returned but not consumed*. Phase 10's brief
  explicitly deferred consumption to "friendly NPC primitive (spec 3.1)".
- The script runtime already dispatches by function-name convention from
  each mod's `main.rhai`: `on_world_start(world_name)` (session.rs:288),
  `on_block_break(...)`, `on_interact(face, u, y, v, block)` running on the
  crosshair block, returning `false` to cancel. Host functions: `give`,
  `hud_message`, `play_sound`, `spawn_animal`, `set_block`/`get_block`,
  `storage_get`/`storage_set` (per-mod KV persisted to `modstore.toml`).
- Scripts defer world mutations through a `Cmd` queue drained by
  `apply_script_cmds` (actions.rs:2812); `spawn_animal` uses
  `Cmd::SpawnAnimal(name, EntityPos)` and gated by `mob_count`/`MOB_CAP`.
- Mobs are defined in `animals.toml` (hostile/fauna-centric fields:
  flee_range, hunt, graze, taming, cargo). `Mob` state machine has
  `Idle/Wander/Flee/Hunt/Graze/Stalk` — no friendly/idle dialogue state,
  no per-NPC-per-player state.
- Registry loading is a fixed list of `include_str!`ped base TOMLs +
  a per-mod `parse_mod_dir` read (`*File` shim per file). `Screen` enum
  (game/mod.rs:44) has no dialogue/journal screens yet.

## Goal

Ship the content-layer trio from spec Part 3 in one coherent pass,
all **TOML-authored like the rest of the content layer**:

1. **3.1** A friendly NPC entity distinct from `animals.toml`: fixed
   position or a patrol path, a talk interaction, per-NPC-per-player
   authored + runtime state (quest stage, relationship) through the engine
   KV store.
2. **3.2** A data-driven branching dialogue format with condition checks
   against quest flags and choice-selected callbacks — the data-vs-code
   split matching `recipes.toml` + `on_craft`.
3. **3.3** Quest definitions (objectives, rewards) and a journal/log UI;
   completion state lives in the existing per-mod
   `storage_get`/`storage_set` KV so it is hot-reload- and save-safe by
   construction.

The three compose: an NPC's dialogue nodes reference quest flags; choosing
a node advances/sets quest flags; the quest completion loop pays rewards
through the existing script `give`.

## In Scope

### 11a — NPC primitive (3.1)

A new content type **`npcs.toml`**, intentionally *not* reusing
`animals.toml` fields:

- `[[npc]]` def: `id`, `name`, `tex` (+ `head_tex`), voxel `model`
  (mirror `animals.toml`'s box model: body/head/limb boxes — reuse the
  same `TexSpec` resolution path, registry.rs:2731), `fixed` position (a
  face/u/y/v `EntityPos`) **or** a `patrol` path (list of waypoints +
  `pause` seconds per node), `talk_radius` (interaction range; default the
  client's crosshair REACH), `sound_pitch`, and optional `script_mod`/
  `dialogue` id the NPC enters when talked to.
- A new `MobState`-adjacent behavior: NPCs are non-hostile, never flee,
  never hunt; they idle at `fixed` or walk the `patrol` loop. Prefer a new
  tiny NPC state machine in `src/npc/` over extending `mobs.rs` states —
  the spec's whole point is that fauna fields (flee/tame/cargo/hunt) don't
  apply.
- Talk interaction: extend the right-click mob pathway
  (actions.rs:1337 mob branch) — when the crosshair mob is an NPC
  (`reg.npcs`), open `Screen::Dialog` with the NPC def/dialogue root
  instead of feeding/taming. Keep NPCs out of drop/flee/tame logic.
- Spawn: `Cmd::SpawnNpc` (mirror `Cmd::SpawnAnimal`), gated by a new
  `npc_cap`; authored NPCs spawn at world start via a `[npc]`-bearing mod's
  `on_world_start` (or a `spawn_npc` host fn). **Consume the Phase 10
  `AssemblyMarker`s:** pieces whose markers carry a tag the marker
  convention reserves for NPCs (e.g. `spawn:npc:<npc_id>`) should place an
  NPC — decide the tag rule and document it (recommended:
  `spawn:npc:<id>`; the walk already returns the resolved `kind`+`at`
  pairs, this becomes the first real consumer).
- **Per-NPC-per-player state:** a `(npc_id, player_uid) -> state`
  namespace in the engine KV (the mod's own KV: `storage_get/set`), seeded
  by scripting (see 11c) — quest stage, relationship tier, met/unmet.

### 11b — Dialogue system (3.2)

TOML-authored branching dialogue reusing the script-event split:

- `[[dialogue]]` def in **`dialogue.toml`**: `id`, `npc`, `root` node id,
  and `[[dialogue.node]]`: `id`, `text`, optional `condition` (a mod +
  script fn name, or a KV flag expression), `choices`
  (`[[node.choice]]`: `label`, optional `condition`, and a `callback`
  naming a mod + fn), `next` node id.
- **Evaluation hooks stay in scripts** (the existing Rhai host): a chosen
  `condition`/`callback` dispatches through the same `dispatch`/`wants`
  machinery (`ScriptHost::dispatch`), with `storage_get`/`storage_set` as
  the flag store. Dialogue itself is pure data; scripts decide flags and
  rewards. This is exactly the spec's "exposing just evaluation hooks
  through the existing script API" and mirrors `recipes.toml` + `on_craft`.
- Rendering: add `Screen::Dialog` (holding the active node + choice
  indices). Draw text into a framed panel and render choices as selectable
  rows (reuse the stall/menu selection pattern in ui.rs + input.rs). ESC /
  click away ends the dialogue; choosing a `next` advances nodes; terminal
  nodes close the panel.
- **Script dispatch signature:** pass `(npc_id, node_id, player_id)` and a
  KV-namespace token so a script can read/write flags per player; returning
  `false` from a `condition` fn prunes that branch (dialogue-level
  "no-op" = hide the choice). Node text may interpolate returned script
  values (document the interpolation convention).

### 11c — Quest tracking (3.3)

- **`quests.toml`** def: `[[quest]]`: `id`, `title`, `description`,
  `giver` (npc id, optional), `objectives` (`[[quest.objective]]`: `key`,
  `description`, `count`), `rewards` (list of { `type` =
  `give`|`set_flag`|`unlock`, `item`, `count`, `flag`, `value` }), optional
  `prereq` quest id.
- **State model: the KV store is the source of truth** — no new
  persistence. `quest_<id>` = `accepted`/`progress/<obj>`/`done` per player,
  keyed under a `player_<uid>` namespace in the current mod's KV
  (which `modstore.toml` already saves). Scripts advance objectives with
  `storage_set("quest_.../progress", n)`; the definitions give counts and
  labels only. Spec's requirement is met by reusing storage — the new work
  is authoring format + the UI surfacing it.
- **Objective hooks:** script events (`on_block_break`, `on_craft`) are
  where objective increments naturally happen; provide a host fn
  `quest_progress(quest_id, objective_key, n)` that appends to the queue
  like `hud_message` so completion detection + rewards run on the game
  loop (not in the script). Completion → `quest_done` flag → apply
  `rewards` via the existing `give` path.
- **Journal UI:** `Screen::Journal` (keyed from dialog-fallback or hotkey)
  listing accepted quests, objective progress
  (`x / count`), and a compact log of recent completions. Read-only;
  authored by looking the quest def up + reading its KV state.
- **Hot-reload safety:** because flags live in the KV and definitions in
  TOML, reloading data mid-world (content.rs:59 `reload_mods`) must keep
  accepted-but-redefined quests valid (document the redefinition rule;
  recommended: track `quest_<id>` accepted state independently of the def
  so re-authored objectives don't orphan progress).

### 11d — Registry & base content

- Add `npcs.toml`, `dialogue.toml`, `quests.toml` to the fixed loader list
  (`parse_mod_dir` reads, `base_mod()` `include_str!` consts, `RawMod`
  fields, `Registry` field + resolve pass mirroring how `animals`/`pieces`
  are handled). No loader restructure — three files in the existing list.
- **Base content decision (resolved):** the spec frames 3.1–3.3 as the
  *proof of the content layer*, so author a minimal **base** demo set in
  `base/npcs.toml` + `dialogue.toml` + `quests.toml` (one NPC with a short
  branching line and a 1–2 objective starter quest touching existing base
  items) so the feature is testable without a mod. Mods then override/add
  via their own files (same override semantics as `animals.toml`).

## Out of Scope

- **3.4 settlement reputation, 3.5 blueprint-gated recipes, 3.6 enemy
  archetypes, 3.7 ire-generalization** — later spec items, deliberately not
  touched. `relationship` is a *read/write flag*, not a reputation economy.
- **Runtime NPC scheduling beyond `fixed`/`patrol`** (sleep schedules,
  work assignments). Later NPC-work systems build on the primitive.
- **Voice/audio-driven dialogue** beyond the existing `play_sound`; no
  TTS or subtitle system.
- **Journal quest tracking via a tracker UI beyond the read-only
  journal** — no clickable quest-beacon tracing, no route markers.
- **Editing/authoring quests at runtime** — quests are shipped data,
  like recipes. No in-game quest editor.
- **Persistent dialogue history** beyond the completion log (no full
  transcript replay).
- **Multiplayer per-player truth** beyond the KV namespaces — each player
  gets `player_<uid>` scoped flags; a shared world "quest done"
  leaderboard or shared completion is not in this pass.
- **NPC combat, damage, or deaths** — non-hostile only; NPCs do not enter
  `Hunt`, take no damage, and cannot be killed this pass.

## Verification

- **Authoring round-trip:** an `npcs.toml`/`dialogue.toml`/`quests.toml`
  load resolves all referenced ids (npc→dialogue, node→next, quest→giver,
  reward→item) with the registry's usual collected-error style; bad refs
  are reported, not panics.
- **Talk flow (headless or script test harness):** ray-hitting an NPC at
  the crosshair opens `Screen::Dialog` with the root node; choosing a
  conditional branch whose `condition` script returns `false` hides/marks
  that choice; a `callback` script's `storage_set` lands in the mod KV and
  survives a save→load.
- **Quest lifecycle:** accept → progress via `quest_progress` → complete →
  rewards given through the existing `give`/inventory path; objective
  counts displayed in `Screen::Journal` match KV state.
- **Reuse check:** NPCs do not flee, tame, take drops, hunt, or get
  spawned by the wildlife pass; MOB_CAP/`npc_cap` respected.
- **Phase 10 seam:** a piece assembly whose piece carries a reserved
  `spawn:npc:<id>` marker places that NPC at the resolved world `at`,
  deterministically (same seed → same NPC placement) and survives
  regen (the assembly is `structure_chunks`-protected already).
- **Hot reload:** editing a dialogue node/quest def mid-session re-reads
  data without orphaning accepted quest progress (KV accepted-state rule
  from 11c holds).
- Existing suite stays green:
  `cargo test --locked --all-targets` and
  `cargo clippy --locked --all-targets -- -D warnings` (per README).

## Suggested PR Description Framing

> Adds spec Part 3.1–3.3 in one TOML-authored content pass: a friendly NPC
> primitive distinct from animals (fixed/patrol movement, non-hostile,
> non-killable, own voxel model + spawn capability, first consumer of the
> Phase 10 `AssemblyMarker` spawn tags), a data-driven branching dialogue
> system whose condition/callback hooks run through the existing script
> API (data-vs-code split like `recipes.toml` + `on_craft`), and quest
> tracking whose objective/completion state lives entirely in the existing
> per-mod KV store (`storage_get`/`storage_set`), surfaced by a new
> journal UI. Add `npcs.toml`/`dialogue.toml`/`quests.toml` to the loader
> list with base demo content; **out of scope:** 3.4–3.7 and any NPC
> combat/scheduling work not described in the spec.

## Roadmap After This

- **3.4 settlement reputation & tiered growth** can read the
  per-NPC-per-player relationship flags this pass seeds.
- **3.5 blueprint-gated recipes** plugs into the quest `reward` "unlock"
  slot (a `quest_done` flag gating a recipe's `unlock` field is a later
  content-layer read of the same KV).
- **2.4-adjacent skinning:** once NPCs consume spawn markers, quest-gated
  features (2.5's locked-door/flag-gated pieces) become the same
  KV-flag-check consumed by the dialogue `condition` evaluation path.
- **NPC economy/summons:** work-assignment NPCs and any "recruit" reward
  type build directly on the `Npc` primitive + quest reward loop.