# Task Brief: Skill Tree System (capability E5)

**Repo:** WildForge, branch `feat/capability-ramp`
**Primary files:** `src/skills.rs` (new — data model, content schema,
XP/level/allocation engine), `src/game/skills.rs` (new — Game glue:
mode gating, XP granting, allocation/respec, effects fold),
`src/registry.rs` (RawMod `skills` field + `skills.toml` parsing,
`reg.skills: SkillTree` resolution), `src/ruleset.rs` (new `skills`
mode toggle), `src/game/mod.rs` (`Game.skills: SkillState`,
`Screen::Skills`), `src/game/session.rs` (skill persistence in the
player profile), `src/game/actions.rs`/`containers.rs`/`frame.rs` (XP
hooks), `src/game/stats.rs` (skills merged into `stat_block()`),
`src/game/ui.rs` + `keymap.rs` + `menus.rs` (skill screen, HUD XP bar,
`K` toggle), `mods/gems/skills.toml` + `mods/gems/modes.toml` (sample),
`mods/README.md` (`skills.toml` docs).
**Related design doc:** `belt-quest/docs/port-plan.md`, capability E5
and C6. **Explicitly overrides WildForge's "no skill trees" non-goal.**
`Screen::Skills` is a temporary hardcoded screen; E11 generalizes the
screens layer for mods.

## Goal

Data-driven skill trees: mods declare `skills.toml` with branches, nodes,
point cost, and tier gates; XP accrues from canonical engine sources with
per-source diminishing returns; leveling (soft-capped) pays skill points;
allocated nodes grant `StatModifier`s into the E4 `StatBlock`. The whole
capability is mode-gated (E1), so built-in Survival/Creative are unchanged.

## What shipped

- **Content schema** (`skills.toml`, schema_version 1): `[tree]` knobs
  (`max_level` 80, `points_per_level` 1, `xp_base` 100, `xp_exponent`
  1.5), `[[xp_source]]` entries `{ id, base, decay }` for the closed
  engine source set (`mine`, `build`, `craft`, `smelt`, `kill`, `fish`,
  `harvest`), and `[[branch]]` → `[[branch.node]]` entries
  `{ id, name, tier (1..=4), cost (default tier), description, stats }`
  where `stats` are E4 `StatModifier`s. Branch/node ids qualify to the
  mod; XP source ids stay engine-global. Parsed per mod in
  `parse_mod_dir` and merged into `reg.skills` in `build()`; bad packs
  surface on the mods screen. 12 unit tests.
- **Engine model** (`src/skills.rs`): `SkillTree`/`SkillBranch`/
  `SkillNodeDef`/`SkillXpSource`/`SkillState` plus pure `parse_skills`,
  `resolve`, `xp_for_level` (`base * level^exponent`), `grant_xp`
  (diminishing `base / (1 + decay * k)`, level-ups pay `points_per_level`,
  capped at `max_level`), `unlockable` (affordable + same-branch
  tier-gate: `tier_gate` allocated nodes of the previous tier, default 3),
  `allocate`, `respec` (refunds all costs), `stats_for` (node effects
  fold into a `StatBlock`).
- **Mode gating** (E1): new `skills` ruleset toggle, `false` in both
  built-in rulesets. `Game::skills_enabled()` = not creative + mode has
  `skills = true` + the pack ships content.
- **Game glue** (`src/game/skills.rs`): `skills_enabled`, `grant_xp`,
  `skills_stats` (merged into `stat_block()`), `allocate_skill` (toasts
  the learned skill, player-facing errors), `respec_skills`,
  `skills_xp_to_next`, plus the skill-screen geometry helpers.
- **XP hooks**: mine (successful block break), build (placed block),
  craft (result taken), smelt (furnace output taken), kill
  (`SimEvent::MobDied`), fish (caught), harvest (harvested). Every hook
  no-ops unless `skills_enabled()`.
- **Persistence**: the player profile gains `level`, `xp`,
  `skill_points`, `allocated`, `respecs`, and `[[skill_xp]]` per-source
  grant counts, all `#[serde(default)]` for backward compatibility.
- **UI**: `Screen::Skills`, `K` toggles it in-world; the screen shows
  level/points/respecs, an XP bar toward the next level, branch tabs,
  tiered node boxes (green = learned, grey = affordable, dark = gated),
  per-node cost + description, and a respec button; the HUD shows an XP
  bar under the carry burden meter when skills are live.
- **Sample mod**: `mods/gems/skills.toml` (2 branches × 6 nodes,
  all 7 XP sources) and `skills = true` on the `gems:cozy` mode — a cozy
  world is the in-game demo path. `mods/README.md` documents the schema.
- **Tests**: 12 `skills` unit tests + `skills_toggle_is_opt_in_and_off_by_default`
  (and `overrides_layer_onto_survival` asserting the new toggle) in
  `src/ruleset.rs`. Suite green.

## Verification

- `cargo clippy --lib --tests -- -D warnings` clean.
- `cargo test --lib` green.
- `wildforge --mod-qualification mods` PASS.
- Visual + geode qualification hashes refreshed (src/game/mod.rs,
  src/game/ui.rs, src/game/frame.rs, src/game/keymap.rs, src/ruleset.rs,
  src/registry.rs, src/stats.rs are qualification sources).

## Content notes

- Base ships no `skills.toml` and both built-in modes keep `skills =
  false`, so stock gameplay is numerically identical.
- Respec is engine-free for now; a content-gated respec item is left to
  belt-quest (C6) / future content.

## Out of scope (later capabilities)

- E11 mod-extensible screens: the hardcoded `Screen::Skills` becomes a
  generic mod screen layer.
- E14 save/load completeness: skill allocation persistence landed here;
  remaining belt/dungeon/equipment state is deferred.
- C6 belt-quest skill tree data (7 branches, ~120 nodes, 65 points).