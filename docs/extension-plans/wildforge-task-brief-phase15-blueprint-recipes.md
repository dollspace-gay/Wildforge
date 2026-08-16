# Task Brief: Blueprint-Gated Recipes (spec 3.5)

**Repo:** WildForge, new branch `feat/blueprint-recipes` (off
`feat/settlement-growth`)
**Primary files:** `src/registry.rs` (`RecipeToml`/`RecipeDef` gate fields +
qualification + material-balance), `src/game/containers.rs` (single-player
craft gate), `src/multiplayer/host.rs` (guest craft gate),
`src/game/inventory_ui.rs` + `src/game/tooltip.rs` (locked preview),
`src/game/browser.rs` (locked annotation), `src/game/dialogue.rs`
(`learn_recipe` quest reward), `src/registry/runtime.rs` (gate-aware recipe
queries), `base/recipes.toml` (example gated recipe), `base/quests.toml`
(reward that unlocks it), `mods/README.md` + spec note.
**Related design doc:** `wildforge-engine-extensions-spec.md` Part 3.5.

## Status Going In

- Shaped recipes live in `RecipeToml` (`src/registry.rs:2052`) → `RecipeDef`
  (`src/registry.rs:699`); qualified in `registry.rs:4597-4658`, balanced in
  `reconcile_material_definitions` (`src/registry.rs:4950`) and checked in
  `validate_material_graph` (`src/registry.rs:5474`), whose `material_errors`
  hard-gates world creation.
- `match_recipe` (`src/crafting.rs:91`) is stateless w.r.t. the player: the
  only recipe gate today is `station.is_some()` — such recipes are excluded
  from the grid but still browsable (`src/crafting.rs:113`).
- Crafting executes in exactly two places: single-player
  `Game::result_click` (`src/game/containers.rs:42-156`) and the multiplayer
  host's `C2S::CraftResult` (`src/multiplayer/host.rs:3718-3823`). Neither
  checks player state. The result preview (before clicking) calls
  `match_recipe` directly in `src/game/inventory_ui.rs:614-632` and
  `src/game/tooltip.rs:327-341`.
- Per-player unlock state already exists and is mature: the KV store
  (`content.scripts.kv`, `player_<uid>` namespace) written via
  `write_player_kv`/`read_player_kv` (`src/game/dialogue.rs:135-155`),
  consumed by gates (`src/game/actions.rs:1187-1189, 1791-1793`) and quest
  rewards. `QuestReward` (`src/registry.rs:634-638`) currently has `Give`,
  `SetFlag`, `Reputation`; quest rewards parse at `registry.rs:4499-4527`
  and dispatch in `quest_progress_apply` (`src/game/dialogue.rs:193-246`).
  The `apply_reputation_reward` seam (`src/game/dialogue.rs:256-284`) is the
  template for a recipe-unlock reward.
- Inventory consume primitives exist: `count_of`, `can_afford`, `try_consume`
  (`src/inventory.rs:211-255`); `stamp_instant` is the existing consumable-cost
  consumer (`src/world/template.rs:304-362`).

Everything below is untouched (verified): no tech/blueprint field on any
recipe, no recipe-gate check at either craft site, no `learn_recipe` reward,
no gate-aware recipe browsing.

## Goal

Implement spec 3.5: `recipes.toml` recipes may declare an optional **tech
gate** (a per-player KV flag that must read truthy) and/or an optional
**blueprint-item gate** (a consumable item that must be present and is
consumed on craft) — either, both, or neither. A locked recipe is refused
at both craft sites and annotated as locked in the UI; a quest reward
(`learn_recipe`) unlocks a tech-gated recipe as a distinct reward type from
the quest `SetFlag`/reputation rewards.

1. **Recipe gates** — `RecipeDef` gains `tech: Option<String>` (KV key,
   default `learned:<recipe_id>`) and `blueprint: Option<ItemId>`. A craft
   succeeds only when the player's KV flag reads truthy (tech) and the
   blueprint item is present (consumed 1:1 on a successful craft).
2. **Craft-site gating** — refuse at `containers.rs` (single-player) and
   `host.rs` (multiplayer guest): the client refuses locked tech recipes
   before they reach the wire; the host refuses blueprints it can see in the
   guest's inventory.
3. **UI** — the result preview shows a locked recipe (dimmed / "requires…"
   note) instead of a craftable result; the recipe browser annotates gated
   recipes with their gate.
4. **Reward** — `QuestReward::LearnRecipe(String)` (authored
   `{ learn_recipe = "base:forged_key" }`) validates the recipe exists at
   load and, on completion, writes the recipe's tech KV flag truthy.
5. **Base content** — one authored example: a gated recipe for a tool or
   material unlocked by a small quest reward, demonstrating both gate kinds.

## In Scope

### 15a — gate fields on recipes + qualification

- `RecipeToml` (`registry.rs:2052`) gains:
  - `tech: Option<String>` — a KV key that must read truthy to craft.
    Omitted on the toml means *no tech gate*; the runtime default
    `learned:<recipe_id>` applies **only** to recipes unlocked by a
    `learn_recipe` reward (see 15d). Explicit `tech = "..."` overrides the
    default and lets a non-quest flag (e.g. a `SetFlag` reward or a gate
    flag) unlock the recipe.
  - `blueprint: Option<String>` — an item name (qualified then bare, same as
    every other cross-reference) that must be in the player's inventory to
    craft; one is consumed per craft.
- `RecipeDef` (`registry.rs:699`) gains `tech: Option<String>` and
  `blueprint: Option<ItemId>`.
- Qualification (`registry.rs:4597`): resolve `blueprint` via `lookup_item`;
  an unresolvable blueprint **fails the recipe with a load error** naming the
  recipe (collected into a local `recipe_errors` vec, extended into
  `material_errors` after `validate_material_graph` — the pattern from
  `gate_errors` at `registry.rs:4874` and the Phase 14 settlement/quest
  errors). A `tech` key is a free string (no resolution).
- The material graph must stay balanced: a blueprint is **extra input**, so
  `reconcile_material_definitions` (`registry.rs:4950`) and
  `validate_material_graph` (`registry.rs:5474`) account `blueprint.materials
  * count` on the input side (the recipe's pattern input + blueprint
  materials must equal output × count + loss + byproducts). A blueprint item
  with no declared materials (e.g. a plain non-material item) balances trivially.

### 15b — craft-site gating

- Single-player `Game::result_click` (`containers.rs:76-106`): after
  `match_recipe` returns a recipe, before putting output on the cursor:
  - `tech` gate: `read_player_kv(&recipe.tech)` must parse truthy
    (non-empty and != `"false"`); otherwise refuse (no craft, no consume).
  - `blueprint` gate: `inventory.can_afford(&[(recipe.blueprint, 1)])`
    must hold; on success `inventory.try_consume(&[(recipe.blueprint, 1)])`
    before `crafting::consume` (blueprint is consumed from inventory, not
    the grid).
- Multiplayer host `C2S::CraftResult` (`host.rs:3751`): the host sees the
  guest's `craft_grid` and `inventory`. It enforces the **blueprint** gate
  against `guest.inventory` (authoritative). The **tech** gate is enforced
  client-side (`result_click` refuses before sending the request) — the KV
  lives on the client, matching the existing trust model where quest/dialogue
  state is client-side and the host has no `read_player_kv`. A guest that
  never sent the request cannot craft; a hostile client bypassing its own
  gate is out of scope (same trust boundary as quest rewards).
- The `match_recipe` helper stays stateless; gates are applied at the two
  call sites (and the preview, below), never inside the matcher.

### 15c — locked preview + browser annotation

- Result preview (`inventory_ui.rs:614-632`, `tooltip.rs:327-341`): when
  `match_recipe` matches a recipe whose gates are unmet, do not show a
  craftable result — show the locked presentation (dimmed result slot with a
  `"locked"` hint, or no preview at all, matching whichever reads better in
  the existing UI). Unmet means: tech flag not truthy, or blueprint absent
  (`count_of`).
- Recipe browser (`browser.rs:116-204`): a gated recipe is shown with an
  annotation of its gate (`"requires <blueprint>"`, `"locked"` for tech)
  rather than hidden — authors can see what they authored, players see what
  they don't yet have. `recipes_for`/`uses_of`
  (`src/registry/runtime.rs:208-230`) gain gate metadata in their returned
  entries (or the browser reads `RecipeDef` directly), without changing the
  material-graph consumers that also use these queries.

### 15d — `learn_recipe` quest reward

- `QuestReward` (`registry.rs:634`) gains `LearnRecipe(String)`; `QuestRewardToml`
  (`registry.rs:2497`) gains `learn_recipe: Option<String>`.
- Reward parsing (`registry.rs:4499`): validate the recipe id exists
  (qualified) like the settlement check at 4508-4513; unknown recipe → local
  `quest_errors` (extended with the Phase 14 pattern) and the reward is
  dropped. Rewards are additive with the existing three.
- Dispatch (`quest_progress_apply`, `dialogue.rs:227` arm): a standalone
  `apply_recipe_unlock_reward(kv, namespace, reg, recipe_id)` mirroring
  `apply_reputation_reward` — writes KV `learned:<recipe_id>` = `"true"`
  (the runtime default tech key). A recipe with an explicit `tech` override
  is **not** affected (its own flag governs it).
- `game/mod.rs` re-exports the seam `#[cfg(test)]` like
  `apply_reputation_reward`.

### 15e — base content + docs

- `base/recipes.toml`: one gated recipe demonstrating both kinds, e.g. a
  serviceable tool or material:
  ```
  [[recipe]]
  pattern = ["ib", " s"]       # whatever shape fits
  keys = { i = "base:iron_ingot", b = "base:??", s = "base:stick" }
  output = "base:??"
  tech = "learned:base:??"     # or omit + unlock via reward default
  blueprint = "base:??_blueprint"
  ```
  Keep it material-balanced (15a) and reuse existing blocks/items (no new
  textures; a blueprint item may be a new item id only if its material vector
  is trivially empty or balanced by an explicit recipe/material entry).
- `base/quests.toml`: a small quest whose reward is
  `{ learn_recipe = "base:??" }`, chained after an existing quest, so the
  unlock path is visible in a fresh world.
- `mods/README.md`: document `tech`/`blueprint` on recipes, the KV-key
  convention (`learned:<recipe_id>` default), the locked UI, and the
  `learn_recipe` reward. `registry.rs` load-error text mirrors the
  quest/settlement loaders.

### 15f — tests

- Registry: a recipe with `tech`/`blueprint` parses; unknown blueprint fails
  load naming the recipe; a `learn_recipe` reward resolves a known recipe and
  an unknown one fails load; a gated recipe's material balance accounts the
  blueprint input (a deliberately imbalanced gated recipe fails
  `material_errors`).
- Single-player craft: a locked (tech) recipe refuses at `result_click`
  without consuming the grid; writing the KV flag unlocks it; a blueprint
  recipe without the item refuses, with the item present it crafts and
  consumes exactly one.
- Multiplayer: the host's `C2S::CraftResult` refuses a blueprint-gated recipe
  when the guest's inventory lacks the item, and consumes it on success.
- Preview: `match_recipe` on a locked recipe yields the locked presentation
  in the preview path (assert via the same seam the UI uses).
- Reward: `apply_recipe_unlock_reward` writes `learned:<id>` truthy and does
  not touch an explicit-`tech` recipe's flag.
- All tests follow the existing single-threaded `tests/*.rs` style.

## Out of Scope

- **Tech-tree / research UI** — this phase adds per-player recipe gating and
  one unlock reward, not a research screen or a tree of nodes. "Distinct from
  the tech-tree unlock path" means the reward exists; the tree itself is
  future content.
- **Recipe secrecy** — locked recipes are annotated, not hidden from the
  browser (see 15c); hiding discovered recipes behind fog-of-war is not this
  phase.
- **Blueprint durability/consumption fraction** — exactly one blueprint is
  consumed per craft; partial consumption, blueprint stacks as a count
  resource, or blueprint recycling are out of scope.
- **Gating the other recipe families** (`[[smelt]]`, `[[worked]]`, kiln,
  bloomery, preparations, workings) — this phase gates shaped
  `[[recipe]]` only; the pattern is a template for later.
- **Host-authoritative tech gates** — the multiplayer trust boundary stays as
  today (client KV); moving KV to the host is a separate networking change.
- **New block types / textures** for a bespoke workbench — reuse existing
  stations and blocks.

## Verification

- `cargo check` / `cargo clippy --all-targets -- -D warnings` clean.
- `cargo test --lib` green including the new recipe-gate/quest tests; the
  `visual_capture` suite remains green (refresh qualification hashes if any
  `QUALIFICATION_SOURCES` file touched the sources).
- Manual smoke: a gated recipe shows locked in the grid preview and refuses
  to craft; completing the unlock quest (or setting the KV flag) makes it
  craftable; a blueprint recipe consumes exactly one blueprint per craft;
  save/reload keeps the unlocked flag.

## Roadmap After This

- **3.6 enemy behavior archetype library** (next task) — independent content
  layer; the `learn_recipe` reward is the first of the "dungeon-exclusive
  unlock as a reward type" family, which 3.6's dungeon-style encounters will
  pair with.
- **3.7 generalized industrial response** — unrelated to recipes; can be
  planned independently.
- If later content wants **gated `[[smelt]]`/`[[worked]]`**, the gate fields
  extend to those defs using the same KV/blueprint primitives.