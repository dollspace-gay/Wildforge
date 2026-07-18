# Texture Packs — Design Plan

Decisions (2026-07-18): drop-in pack folders mirroring the mod system;
**one active pack** at a time in v1 (stacking/priority is a later
feature); per-tile PNGs addressed **by name**, not by atlas position;
selection persists in `config.txt`; live hot reload while editing.

Existing foundation this builds on: `assets/atlas.png` whole-sheet
override, `WILDFORGE_EXPORT_ATLAS`, `atlas::builtin_slots()`
(name → slot for every built-in tile), `atlas::blit_tile` (arbitrary-
resolution nearest blit), `Registry.tex_files` (mod PNG → slot),
`renderer.set_atlas()` (live swap), `mods_tree_stamp()` + `reload_mods()`
(mtime polling + F5).

## 1. Pack format

```
packs/<pack_id>/
  pack.toml            # optional: name, author, description
  tiles/
    stone.png          # overrides built-in tile "stone"
    grass_top.png
    copper_ore.png
    gems/ruby_ore.png  # overrides mod tile: <mod_id>/<texture file>
```

- Tile filenames are the **same names used everywhere else**: the
  `builtin_slots()` keys that mod TOML references as `@stone` etc.
- Mod tiles are addressable as `<mod_id>/<texture filename>` matching
  the mod's `textures/` entry (e.g. `gems/ruby_ore.png`).
- Any per-tile resolution; nearest-scaled into the atlas tile like mod
  textures today. Non-square or unreadable PNGs are skipped with a
  warning. Filenames matching no known tile warn (stderr + packs
  screen) but don't fail the pack.
- A pack overrides **only the tiles it ships** — everything else falls
  through to the procedural default / `assets/atlas.png` / mod art.

## 2. Atlas layering

Refactor `build_with_mods` into a layered build:

```
build_atlas(tex_files, pack: Option<&Path>, names: &HashMap<String,u16>)
  1. base   = load_or_build()            // procedural or assets/atlas.png
  2. mods   = blit each Registry.tex_files entry (unchanged)
  3. pack   = blit each recognized packs/<active>/tiles/*.png last
```

Pack blits last and therefore **always wins** — an explicit user choice
outranks both vanilla and mod art, but only for tiles the pack targets.

**Name → slot map**: `builtin_slots()` alone can't address mod tiles
(slots are assigned dynamically from `FIRST_FREE_SLOT`). Registry gains
`tex_names: Vec<(String, u16)>` (the `<mod_id>/<file>` key it already
builds internally in `resolve_tex`, made public). The full map passed to
`build_atlas` = builtin ∪ tex_names. Exclude `crack`/`unknown`? No —
they're named tiles too; packs may re-skin break overlays.

## 3. Selection & persistence

- `Config` gains `pack: String` (empty = none), saved/loaded with the
  existing `config.txt` fields (global preference, not per world).
- New `Screen::Packs`, modeled on `Screen::Mods`: button on the title
  screen next to MODS. Lists `NONE (PROCEDURAL)` plus each discovered
  `packs/<dir>` (pack.toml name/description if present, else the dir
  name), active pack highlighted. Click selects, rebuilds the atlas
  immediately via `set_atlas`, persists config. Missing selected pack
  at startup → warn and fall back to none.
- `packs/` directory is created on startup if absent (like `mods/`).

## 4. Hot reload

- Extend `mods_tree_stamp()` to also walk `packs/` (rename to
  `content_tree_stamp`). Editing any pack PNG re-triggers the existing
  1 s poll → `reload_mods()` → `build_atlas` → `set_atlas`: repaint a
  tile while the game runs and the world re-skins within a second.
  F5 force-reload works unchanged.

## 5. Creator workflow: per-tile template export

- `WILDFORGE_EXPORT_TILES=packs/mytheme` dumps **every named tile** as a
  correctly-named PNG (`tiles/stone.png`, …, `tiles/gems/ruby_ore.png`
  for loaded mods) at the current tile resolution, plus a stub
  `pack.toml`. Workflow: export, repaint the tiles you care about,
  delete the rest, done. Round-trip is identity: exporting then
  selecting the pack reproduces the exact same atlas.
- README section documenting the format + workflow.

## Engine work summary

1. `atlas.rs`: `build_atlas` layering refactor + pack scan/blit +
   per-tile export.
2. `registry.rs`: expose `tex_names` from `resolve_tex`.
3. `main.rs`: Config `pack` field, `Screen::Packs` UI, title button,
   stamp walk over `packs/`, pass active pack through `reload_mods`
   and initial startup build.

## Tests

- Pack discovery lists folders; pack.toml metadata parsed; missing
  pack.toml falls back to dir name.
- Override applied at the correct slot: build atlas with a fixture pack
  containing a solid-magenta `stone.png`, assert stone slot pixels are
  magenta and an untargeted slot (e.g. dirt) is unchanged.
- Mod tile override: fixture pack `gems/ruby_ore.png` wins over the mod
  PNG; layering order (pack > mod > base) asserted.
- Unknown tile filename warns, doesn't fail; unreadable PNG skipped.
- Config `pack` round-trips; missing pack at startup falls back to none.
- `content_tree_stamp` changes when a pack file is touched.
- Export → re-import round-trip reproduces identical atlas bytes.

## Implementation order

1. Registry `tex_names` + `build_atlas` layering refactor (headless,
   fully testable).
2. Per-tile export + round-trip test.
3. Config field + startup plumbing + hot-reload stamp.
4. `Screen::Packs` selection UI.
5. Ship a small example pack (`packs/dusk/` — a handful of moody
   recolors) to dogfood the format, README, tests, screenshots.
