# Wildforge

A Minecraft-alpha-style voxel game written in Rust with a custom engine —
no game framework, just **wgpu** for rendering, **winit** for windowing,
**glam** for math, and **noise** for terrain. Physics is hand-rolled AABB
collision (a voxel world doesn't need a general-purpose physics engine).

![screenshot](docs/screenshot.png)

## Run

```sh
cargo run --release
```

### WSL2 / WSLg note

WSLg cannot truly capture the mouse: the host Windows cursor can neither be
hidden nor warped from inside Linux ([wslg#1361](https://github.com/microsoft/wslg/issues/1361),
[wslg#240](https://github.com/microsoft/wslg/issues/240)), so under WSLg the
game falls back to stable position-delta look — the cursor stays visible and
look stops at the window edge. For proper capture, run the **native Windows
build** instead (from the repo root, so the save folder is shared):

```sh
rustup target add x86_64-pc-windows-gnu   # once; needs mingw64-gcc installed
cargo build --release --target x86_64-pc-windows-gnu
./target/x86_64-pc-windows-gnu/release/wildforge.exe   # launches on Windows
```

Sensitivity can be scaled with `WILDFORGE_SENS` (default `1.0`).

The world saves automatically to `saves/world1/` (modified chunks only,
RLE-compressed) and reloads on next launch. Delete that folder for a fresh
world with a new seed.

## Controls

| Input | Action |
|---|---|
| Mouse | Look |
| WASD / arrows | Move |
| Space | Jump / swim up |
| Ctrl | Sprint |
| Hold left click | Mine block (per-block hardness; bedrock unbreakable) |
| Right click | Place selected block (consumes from inventory) |
| Middle click | Select targeted block if in hotbar |
| 1–9 / scroll | Select hotbar slot |
| E | Open/close inventory (click to move stacks, right-click half/one) |
| Esc | Pause menu (resume / save and quit) |
| F2 | Screenshot (`screenshot-<ts>.ppm`) |
| F11 | Fullscreen |

## Modding (native, hot-reloadable)

Wildforge has a built-in mod system — vanilla content itself is the `base`
mod, registered through the same TOML pipeline external mods use
(see `base/*.toml` for the reference). Design doc: `docs/modding-plan.md`.

- **Data mods** (no code): drop a folder in `mods/` with `mod.toml`
  (id/name/version/depends), `blocks.toml`, `items.toml`, `recipes.toml`,
  `features.toml` (ore veins), `tags.toml` (item groups — recipes accept
  `"#base:planks"`-style tag ingredients, and mods can extend shared tags
  so e.g. a new wood's planks work in every plank recipe), and PNG tiles in
  `textures/` (packed into the atlas at load; `@name` references built-in
  procedural tiles)
- **Script mods**: add `main.rhai` with event handlers —
  `on_world_start`, `on_block_break/place` (return `false` to cancel),
  `on_interact`, `on_craft`, `on_player_respawn`, `on_tick`.
  Host API: `get_block`/`set_block`, `give`, `hud_message`, `play_sound`,
  `surface_height`, `log`, and `storage_get`/`storage_set` — a per-mod KV
  store that survives hot reloads and is saved with the world.
  Scripts are sandboxed (no filesystem/network, per-event op limits).
- **Hot reload**: edit anything under `mods/` while playing — the game
  repacks the registry/atlas, remaps the live world, and recompiles scripts
  within a second (F5 forces it). A script error keeps the previous version
  running and shows the error on the MODS screen.
- **Save safety**: worlds store an id palette; removing a mod turns its
  blocks into placeholders instead of corrupting the world, and pre-mod
  (v1) worlds migrate automatically.
- Ships with `mods/copper` — a worked example adding copper ore worldgen,
  items, tools, recipes, and a scripted mining counter.

## Menus, worlds & settings

- **Title screen**: list of worlds under `saves/` with their seeds — play any,
  create a **new world with a random seed**, or delete one (with confirmation)
- **Settings** (from title or pause menu): master **volume**, mouse
  sensitivity, render distance, and FOV — adjusted with sliders, applied live,
  persisted to `config.txt`
- **Pause menu**: resume, settings, save & quit to title
- **Sound**: procedurally synthesized effects (no audio files) — per-material
  block breaking, placing, item pickup, crafting, damage, splashes, UI clicks
- Dev/headless: `WILDFORGE_WORLD=name` skips the title screen

## Survival

- **Mining**: hold to break with per-block times and a growing crack overlay;
  blocks drop item entities (grass → dirt, stone → cobblestone, leaves → nothing)
  that bob, spin, and magnetize into your inventory
- **Inventory**: 9 hotbar + 27 storage slots, 64-per-stack, full drag-and-drop
  inventory screen (E); placing consumes items
- **Health**: 10 hearts, fall damage (beyond 3 blocks), drowning with air
  bubbles, slow regeneration when out of danger
- **Death**: your inventory scatters as drops; respawn at the world spawn
- **HUD**: hotbar with icons/counts, hearts, air bubbles, item name popup,
  damage vignette — all drawn with a procedural 5×7 pixel font (zero assets)

## Tools & Crafting

- **Items**: blocks, sticks, and wood/stone pickaxes, axes, and shovels with
  Minecraft-alpha durability (59/131 uses, shown as a colored bar); tools
  don't stack
- **Tool rules**: matching tools mine 4× (wood) / 8× (stone) faster;
  stone and cobblestone drop nothing without a pickaxe
- **Crafting**: 2×2 grid in the inventory (E); craft a **crafting table**
  (2×2 planks) and right-click it placed in the world for the 3×3 grid.
  Shaped recipes match at any grid offset and mirrored:
  - log → 4 planks; 2 planks (stacked) → 4 sticks; 2×2 planks → crafting table
  - pickaxe: 3 material across the top + 2 sticks down the middle (3×3)
  - axe: 2×3 head-and-shaft shape, either chirality (3×3)
  - shovel: 1 material over 2 sticks (3×3)
  - materials: planks → wood tier, cobblestone → stone tier

The natural progression: punch a tree → planks → sticks + crafting table →
wood pickaxe → mine stone → cobblestone → stone tools.

Dev cheat: `WILDFORGE_GIVE=1` starts with some items for testing.

## Features

- Infinite procedural **3D terrain** (Caves & Cliffs style): a
  lattice-interpolated density field with spline-shaped geography —
  continentalness/erosion/ridges noises drive ocean basins, plains,
  plateaus, and mountain ranges up to y≈230 with real overhangs and cliff
  lips (16×16×256 chunks, sea level 64, bedrock floor); frustum-culled
  rendering; design in `docs/terrain-v2-plan.md`
- Layered noise caves: big "cheese" caverns deep down plus winding
  "spaghetti" tunnels whose entrances taper near the surface
- Slope- and altitude-aware surfacing: steep faces expose bare stone,
  peaks above y≈170 carry snow caps, underwater floors are sand/gravel
- **Eight biomes** by nearest-centroid matching in 5D climate space
  (temperature, humidity, continentalness, erosion, ridges) — forest,
  plains, desert (sand + cacti), jungle (dense giant canopies), scrubland
  (patchy sand/grass + shrubs), taiga (conifers), arctic (snow cover,
  frozen ocean ice), and mountains (bare stone, snow caps) — each with its
  own surfaces, vegetation shapes, and densities; **five wood families**
  (oak, birch with flecked white bark, dark spruce, vivid jungle, olive
  acacia) grow per biome with distinct bark/leaf/ring textures, forests
  mix oak and birch, and every log crafts into its own colored planks —
  all plank types are interchangeable (and mixable) in recipes via
  ingredient tags; biome placement
  correlates with terrain shape because both read the same noise fields;
  the current biome shows in the window title
- Chunk streaming with per-frame generation/meshing budgets, nearest-first
- Face-culled chunk meshing with per-vertex ambient occlusion and
  Minecraft-style directional face shading (with anisotropy-fixing quad flips)
- Procedurally generated texture atlas (32×32 tiles by default,
  `WILDFORGE_TILE_PX=16|32|64|128`): tileable multi-octave value noise,
  voronoi cobblestone/gravel, board-and-nail planks, growth-ring logs,
  turf overhangs — zero asset files
- **Texture packs**: drop a square `assets/atlas.png` (side a multiple of 16)
  and it replaces the procedural atlas — no recompile. Export the procedural
  atlas as a starting template with `WILDFORGE_EXPORT_ATLAS=atlas.png`
- **Flowing water** (Minecraft-style): sources spread up to 7 blocks with
  decreasing levels and rendered heights, fall over ledges as waterfalls,
  cascade downhill, stay one block deep, and recede when cut off (5 Hz fluid
  ticks); jump while swimming against a ledge to hop out of water
- Translucent water with level-based surface heights, underwater tint,
  swimming physics
- Day/night cycle (10 min) with sky, fog, and light dimming
- AABB player physics: gravity, jumping, sprinting, axis-resolved collision
- DDA voxel raycast for block targeting with wireframe outline + crosshair
- World persistence via RLE-encoded chunk files

## Architecture

| Module | Role |
|---|---|
| `main.rs` | winit event loop, input, chunk streaming, day/night, HUD title |
| `world.rs` | chunk map, block get/set, dirty tracking, save/load |
| `worldgen.rs` | Perlin heightmap, caves, trees |
| `chunk.rs` | 16×128×16 block storage |
| `mesher.rs` | visible-face extraction, AO, opaque + water meshes |
| `renderer.rs` | wgpu device/surface, pipelines, per-chunk GPU buffers |
| `shader.wgsl` | chunk/water/line shaders, fog, daylight |
| `physics.rs` | player AABB movement & collision |
| `raycast.rs` | Amanatides–Woo voxel traversal |
| `atlas.rs` | procedural block textures |
| `camera.rs` | first-person camera |

Tests (`cargo test`) cover worldgen determinism, save/load round-trips,
raycast targeting, and physics (landing, walls, jump height).
