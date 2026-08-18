# Task Brief: Mod Test Harness (capability E0)

**Repo:** WildForge, branch `feat/capability-ramp`
**Primary files:** `src/mod_lint.rs`, `src/lib.rs` (CLI arm),
`src/tests/registry.rs` (content-graph test refactor), `screenshots/visual-polish.toml`
**Related design doc:** `belt-quest/docs/port-plan.md`, capability E0.

## Goal

Give a pure data + script mod repo (belt-quest) a single command to run in
CI against WildForge that proves the mod is loadable, balanced, reachable,
and scripted — without the mod owning any Rust. The harness is the shared
qualification battery: base content and mod content must both pass it, and
the engine's own content-graph suite delegates to it.

## What shipped

- `src/mod_lint.rs`:
  - `qualify_mods(mods_dir) -> ModLintReport` — loads the registry from any
    mods directory and runs: per-mod load errors, `material_errors`,
    `arcane_errors`, missing textures, the structural + obtainability
    battery (`content_errors`), and script compile via `ScriptHost`.
  - `obtainable_items(reg) -> HashSet<u16>` — the survival content-graph
    closure (world sources → recipes/smelts/steelworks/worked/implements/
    alchemy/kiln/separator/bucket/smoker/compost/heart code paths),
    extracted verbatim from the old inline test logic so there is one
    source of truth.
  - `ModLintReport::is_qualified()` / `render()` (materials.rs audit
    pattern).
- CLI arm in `lib.rs` `run()`:
  `wildforge --mod-qualification <mods_dir>` — prints the report, exit 1 on
  any failure. Mirrors `--material-audit` / `--magic-qualification`.
- `src/tests/registry.rs::content_graph_is_complete_and_obtainable`
  refactored to delegate structural/biome/obtainability/fuel/breed-food
  checks to the shared harness; the test keeps only its base-specific
  tuning-lens assertions.
- 5 unit tests in `src/mod_lint.rs` covering: base passes, missing texture
  fails, unobtainable item fails, broken script fails, and a clean mod with
  a crafting chain qualifies (each on a unique temp tree for parallelism).

## Verification

- `cargo clippy --lib --tests -- -D warnings` clean.
- `cargo test --lib` green (suite is now 919 passing).
- Manual: `wildforge --mod-qualification mods` (ships base + gems, which has
  a `main.rhai`) → PASS; a mod with a bad `world_api`, missing texture, or
  broken `main.rhai` → FAIL with the specific error.
- Both qualification hashes in `screenshots/visual-polish.toml` refreshed
  because `src/lib.rs` is a `QUALIFICATION_SOURCES` file.

## Out of scope (later capabilities)

Ruleset/mode system (E1), third-person camera (E2), and every downstream
capability. The harness is the prerequisite: each subsequent capability's
data-only sample mod is proven reachable from mod-land by running it through
`--mod-qualification`.

## belt-quest integration

belt-quest CI (once it has content) runs:

```
wildforge --mod-qualification <checkout-of-wildforge>/mods
```

with belt-quest installed into that mods tree, and gates on exit code 0.
