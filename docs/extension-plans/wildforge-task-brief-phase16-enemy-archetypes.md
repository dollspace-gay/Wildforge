# Task Brief: Enemy Behavior Archetype Library (spec 3.6)

**Repo:** WildForge, new branch `feat/enemy-archetypes` (off
`feat/blueprint-recipes`)
**Primary files:** `src/registry.rs` (`AnimalToml`/`AnimalDef` archetype +
attack + resistance fields, qualification), `src/mobs.rs` (`Mob` archetype
state, `MobEvent` typed attacks, `Mob::tick` dispatch, `Mob::hurt`
resistance application), `src/game/actions.rs` (player-attack damage type,
hack-vs-destroy interaction arm), `src/game/survival.rs` (typed player
damage), `src/world/ecology.rs` (archetype spawn/tick hooks, arcane retire),
`src/world/pieces.rs` + `src/world/template.rs` (builder archetype uses
`place_assembly`/`stamp_instant`), `src/npc/mod.rs` (pattern reference),
`src/script.rs` (`on_hurt`/`on_attack`/`on_enemy_destroyed` hooks if kept),
`base/animals.toml` (archetype example), `mods/README.md` + spec note.
**Related design doc:** `wildforge-engine-extensions-spec.md` Part 3.6.

## Status Going In

- Enemies and wildlife share one pipeline: `AnimalToml`
  (`src/registry.rs:1960-2028`) → `AnimalDef` (`src/registry.rs:433-507`) →
  `Mob` (`src/mobs.rs:202-272`). AI runs in exactly one place:
  `Mob::tick` (`src/mobs.rs:417-942`), a hardcoded `match` over `MobState`
  (`src/mobs.rs:31-43`: `Idle|Wander|Flee|Hunt|Graze|Stalk`).
- There is **no damage type, resistance, or vulnerability concept**: damage
  is a plain `f32` everywhere — `ItemDef.damage` (`registry.rs:357`),
  `ProjectileDef.damage` (`registry.rs:427`), `AnimalDef.attack`
  (`registry.rs:465`). `Mob::hurt` (`src/mobs.rs:375-399`) is
  `health -= dmg` + flash + knockback + retaliate. Player damage
  (`hurt_player_from_wild`, `src/game/survival.rs:29-80`) reduces by armor
  points only.
- There is **one melee attack** per hostile: range `half_w + 0.9`, cooldown
  hardcoded `1.0 s` (`mobs.rs:840-847`), damage `def.attack`. Ranged uses
  `ProjectileDef` + `cast_cd` (`mobs.rs:802-836`). `Mob` carries exactly
  `attack_cd` and `cast_cd` — no per-attack list.
- **No boss / weak-point / phase / hackable / builder concept exists.** The
  closest stateful mobs: `watcher` warning state (`mobs.rs:248-252,
  552-569`), `masterless` wardens, `ire_min`-gated hostile spawning
  (`src/world/ecology.rs:1612-1781`).
- The established precedent for layering behavior onto a `Mob` is the NPC:
  `NpcDef` synthesizes a companion `AnimalDef` (`registry.rs:4346-4459`) and
  `NpcInstance` drives that `Mob` from outside (`src/npc/mod.rs:72-106`),
  reusing all mob physics. There is no generic FSM/archetype framework.
- Real-time construction primitives already exist and are player/furnace-
  facing: `stamp_instant` (`src/world/template.rs:304-362`),
  `spawn_structure` (`src/world/local_structure.rs:637-656`),
  `place_assembly` (`src/world/pieces.rs:144`).
- Script hooks today: only `spawn_animal`/`spawn_npc` and
  `on_animal_killed` (`mods/README.md:666-676`). The `ScriptHook`/`Cmd`
  dispatch (`src/script.rs:23-44, 393`) can carry new lifecycle events.

Everything below is untouched (verified): no archetype field, no per-attack
list, no damage types, no resistances, no hackable/builder behavior.

## Goal

Introduce the codebase's first **data-driven behavior archetype** for
enemies: `animals.toml` may name a `behavior` archetype (implemented in
code), replacing the "hostile + attack scalar" approximation with a small
archetype library. Substrate first: **damage types + resistances** and a
**multi-attack list** with per-attack damage/cooldown/range. Then a set of
named archetypes: a **multi-attack brute**, a **hackable-vs-destroy
construct**, and a **builder** that constructs pieces in real time — each a
code state machine, wired so future archetypes (bosses with weak-point
sequences, etc.) are data + one dispatch arm, not one-off per-creature code.

1. **Damage types + resistances** — a damage type travels with every hit
   (weapon, projectile, animal attack); `AnimalDef` may carry
   resistances/vulnerabilities by type (a multiplier); `Mob::hurt` applies
   them before health is reduced.
2. **Multi-attack defs** — `AnimalToml.attacks` lists melee/charge/projectile
   attacks with distinct damage/cooldown/range; `Mob` runs a per-attack
   cooldown wheel and picks by range/state. `attack` stays as the default
   single melee for back-compat.
3. **Behavior archetypes** — `AnimalToml.behavior` names a code archetype;
   `Mob::tick` dispatches. Ships `standard` (current behavior, unchanged),
   `brute` (multi-attack + resistances), `construct` (hackable-vs-destroy
   branching), `builder` (raises pieces at runtime).
4. **Base content** — one authored example per new archetype, reusing the
   existing palette (no new textures).

## In Scope

### 16a — damage types + resistances

- A `DamageType` is a registry-level string id (no enum of hardcoded names):
  `"blunt"`, `"pierce"`, `"slash"`, `"arcane"`, `"fire"`, etc. are authored
  data, not code constants. Define `DamageTypeId = String` (or a small
  newtype) in `registry.rs`.
- `ItemDef` (`registry.rs:357`) gains `damage_type: Option<String>` (the
  weapon's type; default none = untyped). `ProjectileDef` (`registry.rs:427`)
  and the new attack defs (16b) gain `damage_type` too.
- `AnimalToml` gains `resistances: Vec<{ type, mult }>` and
  `vulnerabilities: Vec<{ type, mult }>`; `AnimalDef` resolves to a
  `HashMap<DamageTypeId, f32>` (multiplier, default 1.0; `vulnerabilities`
  are multiplier < 1, `resistances` > 1 — authorial convenience, both just
  set the multiplier; a single explicit list is fine if the toml prefers one
  field `resist = [{ type, mult }]`).
- `Mob::hurt` (`src/mobs.rs:375`): the incoming hit carries its damage type;
  before `health -= dmg`, apply `dmg * def.damage_multiplier_for(&type)`.
  Untyped damage is unaffected (multiplier 1.0).
- Player → mob: `actions.rs:1055` (`dmg = held item damage`) resolves the
  weapon's `damage_type`; the swing passes the type into `mob.hurt`. Host
  `C2S::AttackMob` (`host.rs:2287`) mirrors it (weapon type from the item the
  guest holds).
- Mob → player (typed attacks, 16b): `MobEvent::HitPlayer` gains the attack's
  damage type; `hurt_player_from_wild` (`survival.rs:29`) keeps today's armor
  reduction (per-type **player** armor is out of scope) but logs the type for
  the mob's own future type-gated behavior (e.g. a brute that enrages on
  `"fire"`).

### 16b — multi-attack defs

- `AnimalToml` (`registry.rs:1960`) gains `attacks: Vec<AttackToml>` where
  `AttackToml { name, kind, damage, cooldown, range, damage_type?,
  projectile? }` and `kind` is `"melee" | "charge" | "projectile"`. The
  existing `attack` scalar (default 3) synthesizes the implicit first melee
  attack when `attacks` is empty — full back-compat for base's wardens.
- `AnimalDef` resolves `attacks: Vec<AttackDef>` (`registry.rs:433-507`);
  `projectile` attacks reuse the existing `ProjectileDef`.
- `Mob` (`mobs.rs:202`) replaces the single `attack_cd` with a per-attack
  cooldown wheel (a small `Vec<(attack_index, timer)>` or a `[f32; N]`
  indexed by attack). `Mob::tick`'s `Hunt` arm (`mobs.rs:777-851`) picks the
  ready attack by range (melee first when close, projectile when in hold
  range, charge as a gap-closer with a wind-up) and emits
  `MobEvent::HitPlayer`/`Cast` naming the attack. A `charge` attack is a
  straight-line dash with a brief wind-up where the mob does not turn; it
  ends on collision or range cap.
- The three new events keep `MobEvent` (`mobs.rs:46-64`) additive: the
  existing `HitPlayer(usize, f32, EntityPos)` gains the damage type + attack
  name (or a new `Attack` event if the signature is too overloaded).

### 16c — behavior archetype dispatch

- `AnimalToml` gains `behavior: Option<String>`; `AnimalDef` resolves to a
  `BehaviorArchetype` enum in code: `Standard` (default), `Brute`,
  `Construct`, `Builder`.
- `Mob::tick` (`mobs.rs:417-942`) gains a dispatch: the current hardcoded
  `MobState` machine remains `Standard`'s implementation; other archetypes
  own a subset of states (they may still wander/hunt via the shared physics)
  and add archetype-specific behavior. Concretely: the big `match` becomes a
  call through `self.archetype.tick_override(self, world, ctx)` where the
  archetype can override `Hunt`/`Idle` or inject a new bespoke state — the
  smallest seam that keeps `Standard` byte-for-byte behavior.
- `Mob` carries archetype state (`archetype: BehaviorArchetype`, plus a
  small per-instance blob: `build_target`, `hackable_hp`, etc.) so behavior
  is per-instance, not per-species-global.

### 16d — archetypes: `brute` + `construct` (hackable-vs-destroy)

- **`brute`** — uses the 16a/16b substrate: authored resistances (e.g.
  shrugs `"blunt"`, flinches at `"fire"`) and 2–3 attacks (heavy melee +
  charge). No new state machine beyond `Standard`'s `Hunt` extended to the
  attack wheel + a `wind_up` sub-state for the charge.
- **`construct`** — the **hackable-vs-destroy** branching: a machine enemy
  with two kill paths. Destroying it (reduce `health` to 0) drops scrap; a
  new player **interaction arm** (right-click on the mob in `actions.rs`
  next to the NPC dialogue arm) attempts a "hack": if the player holds the
  right tool (a `hack = true` item tag, mirroring how `hammer = true` gates
  anvil work per `mods/README.md`), the construct is disabled instead of
  destroyed — it freezes, becomes passable/non-aggressive, drops its "core"
  intact, and (optionally) spawns a friendly companion via the NPC seam. The
  construct's `Mob` uses a bespoke `Hackable` state where damage and
  interaction are both live choices.
- Both paths must be authored per construct (the toml declares the tool
  tag + the two drop tables) so "hack vs destroy" is data, not hardcoded.

### 16e — archetype: `builder`

- A hostile that **constructs things in real time**: while aggro'd (or on a
  timer near its anchor), it drives the existing construction primitives —
  `stamp_instant` (`template.rs:304`) or `place_assembly` (`pieces.rs:144`) —
  to raise walls/burrows/nests around itself, then fights from behind them.
- `Mob` builder state: `build_target: Option<BlockPos>`, `build_cd`,
  `built_count` (a cap so it cannot build forever). The spawned cells go
  through the ordinary block path (save with the world; the wilderness
  reclamation already handles hostile-made cell cleanup where relevant —
  verify `wild_falls_at`, `calendar.rs:688`).
- `place_assembly` is currently a worldgen-facing call; expose the smallest
  runtime entry (a `place_assembly_at` that a `Mob` can drive with a chosen
  assembly id from the registry) or reuse `stamp_instant` with a template
  authored for the builder. Choose whichever keeps the block path + retrogen
  guarantees intact; do not bypass the chunk/structure bookkeeping.

### 16f — base content + docs + script hooks (optional)

- `base/animals.toml`: one authored example per new archetype, reusing
  existing textures/models — e.g. a `brute` variant of an existing warden
  (new `behavior`, `attacks`, `resistances`; same tex), a `construct` that
  is hackable with the hammer tag, and a `builder` that raises a small wall
  (`base/pieces.toml` or `template.toml` supplies the built shape). No new
  block/texture art.
- Script lifecycle hooks: add `on_hurt` and `on_enemy_destroyed` (and, if
  the hack path wants scripting, `on_hack`) to `src/script.rs` dispatch,
  documented in `mods/README.md` — kept only if they stay cheap (one extra
  dispatch per hit is fine; guard behind `has_hook` like the existing
  `wants` check).
- `mods/README.md`: document `behavior`, `attacks`, `resistances`,
  `damage_type`, and the two kill paths for `construct`.

### 16g — tests

- Registry: `attacks`/`behavior`/`resistances` parse and resolve; an unknown
  `behavior` id fails load naming the animal; an attack whose projectile
  resolves like today; `attack` scalar synthesizes the implicit melee.
- Damage types: `Mob::hurt` applies the resistance multiplier (a mob with
  `resist = { type = "fire", mult = 0.5 }` takes half fire damage, full
  blunt); untyped damage is unchanged; the weapon's `damage_type` reaches the
  hit.
- Multi-attack: a two-attack mob uses each attack by range and obeys each
  cooldown (a mob just after a projectile cast cannot instantly melee, etc.);
  `charge` winds up then dashes and stops on collision.
- Archetypes: `brute` enrages on its vulnerability type (if authored);
  `construct` is destroyed for scrap OR hacked (right-click with the tool
  tag) into a disabled state — both asserted; `builder` stamps its assembly
  once per `build_cd` up to the cap, cells land in the block path and save/
  reload correctly.
- Spawn/save: archetype mobs spawn through the existing
  `tick_hostile_spawns` path and persist (where hostiles persist) without
  regressing the ire/dissolve rules.
- All tests follow the existing single-threaded `tests/*.rs` style.

## Out of Scope

- **Spatial weak-point hit zones** — the mob is a single AABB; per-box hit
  detection for "shoot the eye" weak points is deferred (a weak-point
  *sequence* — telegraphed stages / vulnerability windows — is the 16d shape,
  and bosses that use it are a later archetype on this seam).
- **Per-type player armor** — `hurt_player_from_wild` keeps today's flat
  armor-point reduction; per-damage-type player resistances are out of scope.
- **Authoring AI in scripts** — archetypes are code (Rust); a data/script
  DSL that composes states is a larger, separate project. `behavior` names a
  code archetype, nothing more.
- **New model/texture/block art** — reuse the existing palette.
- **Fully generic FSM authoring surface** — the dispatch is an enum of named
  code archetypes, not a turing-complete editable state graph.
- **Reputation/quest rewards interacting with enemy types** — 3.5's
  `learn_recipe` and 3.4's reputation stay as-is; dungeon-exclusive enemy
  drops are future content.

## Verification

- `cargo check` / `cargo clippy --all-targets -- -D warnings` clean.
- `cargo test --lib` green including the new archetype/damage tests; the
  `visual_capture` suite stays green (refresh qualification hashes if any
  `QUALIFICATION_SOURCES` file is touched).
- Manual smoke: a `brute` in the world uses its attack wheel and takes
  reduced fire damage; a `construct` dies to scrap or is disabled by a hack
  with the right tool; a `builder` raises its wall and fights behind it;
  save/reload keeps builder-built cells and construct state.

## Roadmap After This

- **3.7 generalized industrial response** — unrelated to enemies; can be
  planned independently once the archetype seam (which may want a
  ire/factory-driven `builder` or spawn gradient) is in.
- **Boss archetype** — a `boss` on this dispatch seam with a
  weak-point-sequence / phase machine, once the content wants it.
- **Dungeon content pairing** — 3.5's `learn_recipe` reward becomes the
  payoff for clearing a `construct`/`boss` encounter.