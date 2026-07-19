# Making Wildforge mods

Drop a folder in `mods/` and it loads on the next launch — or within a
second if the game is already running (hot reload; F5 forces it).
Vanilla content is itself a mod: everything under `base/*.toml` goes
through the exact same pipeline documented here, so the base files are
a complete worked reference.

This document is executable: the `meadow` example mod below is
extracted verbatim by the test suite
(`mods_readme_example_mod_loads_and_works` in `src/tests.rs`), loaded,
and every behavior it claims is asserted. If the docs drift from the
code, CI fails.

## A mod is a folder

```text
mods/<your_mod>/
  mod.toml          required: id, and optionally name/version/depends
  blocks.toml       [[block]] entries
  items.toml        [[item]] entries
  recipes.toml      [[recipe]], [[smelt]], [[fuel]] entries
  tags.toml         [[tag]] item groups for recipes
  features.toml     [[feature]] worldgen (ore veins)
  animals.toml      [[animal]] creatures, with box models
  structures.toml   [[structure]] templates + [[loot]] tables
  aliases.toml      [[alias]] lossless renames for old saves
  main.rhai         optional script with event handlers
  textures/         PNG tiles referenced by the TOML files
```

Every file is optional except `mod.toml`. Mods load in alphabetical
folder order, re-sorted so dependencies come first (`base` always
loads first). A mod that fails to parse is skipped and its error shows
on the MODS screen — the rest of the game keeps working.

```toml
# mods/meadow/mod.toml
id = "meadow"
name = "Meadow"
version = "1.0.0"
depends = ["base"]
```

## Names, and how things refer to each other

- Everything you register gets qualified with your mod id:
  `id = "sunstone"` in mod `meadow` becomes **`meadow:sunstone`**.
- Within your own files, bare names auto-qualify (`drops =
  "sun_shard"` means `meadow:sun_shard`). To reference another mod's
  content, use the full name: `base:wheat`.
- **`@name`** in a texture field references a built-in tile
  (`@bread`, `@stone`, `@torch`, ... — every name in
  `builtin_slots()` in `src/atlas.rs`). Anything else is a PNG path
  relative to your `textures/` folder, extension included.
- **`#name`** in a recipe key or smelt/fuel input references a tag
  (item group). `#shiny` means your own `meadow:shiny`;
  `#base:planks` is the shared planks group.
- Crops register hidden stage variants named `meadow:foo/stage1`,
  `/stage2`, ... — names containing `/` never appear in the item
  browser.

## blocks.toml

```toml
# mods/meadow/blocks.toml
[[block]]
id = "sunstone"
name = "Sunstone"
texture = "sunstone.png"
hardness = 6.5
tool = "pickaxe"
requires_tool = true
light = 9
light_color = [1.0, 0.85, 0.4]

[[block]]
id = "sunstone_ore"
name = "Sunstone Ore"
texture = "sunstone_ore.png"
hardness = 8.0
tool = "pickaxe"
requires_tool = true
min_tier = 1
drops = "sun_shard"
```

Every field, with defaults:

| field | default | meaning |
|---|---|---|
| `id` | required | qualified to `modid:id` |
| `name` | the id | display label |
| `texture` | required | one tile for all faces, or `{ top = "...", side = "...", bottom = "..." }` (`bottom` falls back to `top`) |
| `hardness` | `1.0` | seconds to mine bare-handed (a matching tool divides this by its speed) |
| `unbreakable` | `false` | cannot be mined at all — even creative respects it |
| `tool` | none | `"pickaxe"` \| `"axe"` \| `"shovel"` \| `"hoe"` — which tool class speeds this up |
| `requires_tool` | `false` | without the right tool class, breaking drops nothing |
| `min_tier` | `0` | minimum tool tier for drops (wood/stone 1, bronze 2, ...) |
| `drops` | itself | `"none"`, an item name, or omit for the block's own item |
| `drop_count` | `1` | how many of `drops` |
| `solid` | `true` | player/mob collision and raycast solidity |
| `opaque` | `true` | set `false` for see-through blocks (leaves): neighbors render behind them and light passes |
| `cross` | `false` | render as two crossed quads (plants) instead of a cube |
| `light` | `0` | emitted light 0–15 (torch is 14). Emitting blocks also become **shadow-casting point lights** when the player is near — no extra data; the same `light`/`light_color` drive both the flood-fill and the hard light |
| `light_color` | white | `[r, g, b]` 0–1 tint for the glow — hue-normalized so the brightest channel still reaches the full `light` level (torches burn warm; a modded block can smoulder any color) |
| `height` | full cube | render height 0–1 for thin slabs (snow layers are `0.125`); pair with `solid = false` to walk through, and unsupported slabs pop off like torches |
| `falls` | `false` | gravity block: detaches and falls when unsupported (sand, gravel) |
| `glass` | `false` | glazing: renders translucent (the blended pipeline), passes sky light, and a glass roof grows winter crops at 0.75× |
| `light_filter` | all pass | `[r, g, b]` as 0/1 — stained light: which block-light channels pass through (red glass is `[1, 0, 0]`) |
| `water` | none | fluid level: `0` = source (registers flow levels automatically) |
| `interaction` | none | right-click opens: `"crafting"` \| `"furnace"` \| `"chest"` \| `"offering"` \| `"bloomery"` \| `"kiln"` \| `"anvil"` \| `"quern"` |
| `crop` | none | `{ stages = N, next_chance = 0.3, stage_textures = [...], any_soil = false }` — advances on random ticks; `any_soil` grows off farmland too |
| `harvest` | none | `{ item = "...", count = 2, becomes = "..." }` — right-click yield without breaking |
| `sapling` | none | `{ tree = "oak" }` — grows into that tree species on random ticks (`oak`/`birch`/`spruce`/`jungle`/`acacia`; unknown names grow oak) |
| `bonus_drop` | none | `{ item = "...", chance = 0.1 }` — extra roll on break |
| `brush` | none | `{ table = "loot_id", becomes = "..." }` — archaeology: brushing rolls the loot table, block transmutes |
| `item` | `true` | set `false` to register no placeable item form (fluids, crop stages) |
| `icon` | block texture | item-form icon override |

## items.toml

```toml
# mods/meadow/items.toml
[[item]]
id = "sun_shard"
name = "Sun Shard"
texture = "sun_shard.png"

[[item]]
id = "honey_bread"
name = "Honey Bread"
texture = "@bread"
food = { hunger = 7, nutrition = { grain = 30 } }
```

| field | default | meaning |
|---|---|---|
| `id`, `name`, `texture` | — | as for blocks; `texture` is the inventory icon |
| `max_stack` | `64` (tools `1`) | stack size |
| `tool` | none | `"pickaxe"`/`"axe"`/`"shovel"`/`"hoe"` |
| `tool_speed` | `4.0` | hardness divisor on matching blocks |
| `tool_tier` | `1` | gates `min_tier` drops |
| `durability` | `59` for tools, else `0` | uses before breaking |
| `damage` | tools get a modest implicit value | attack damage in half-hearts |
| `food` | none | `{ hunger, eat_time = 1.5, nutrition = { grain/vegetable/fruit/fungi/protein = 0..100 } }` |
| `places` | none | placing this item puts down that block (seeds → crops) |
| `bow` | none | `{ damage, speed = 24.0 }` — hold right-click to draw |
| `ammo` | none | ammo class name (`"arrow"`); bows consume it |
| `armor` | none | `{ slot = "head"/"chest"/"legs"/"feet", points = N }` — each point blocks 4% wild damage |
| `charm` | none | `"quiet"` (wardens notice you less) \| `"bark"` (+1 armor point) \| `"hunger"` (slower drain) — one charm slot |
| `bedroll` | `false` | right-click to camp to dawn and set spawn |
| `shears` | `false` | leaves break into leaf blocks |
| `tablet` | `false` | right-click reads lore |
| `brush_tool` | `false` | channels on brushable blocks |
| `throw` | none | `{ speed = 18.0 }` — right-click throws the item as a projectile (snowballs); zero-damage throws still knock back |
| `hammer` | `false` | works `[[worked]]` inputs on an anvil (a 2 s channel per strike) |
| `glow` | none | `[r, g, b]` color × intensity — the item sheds a carried light while held (items that place a light-emitting block glow automatically; this is for the rest, e.g. a raw ember) |

## recipes.toml

```toml
# mods/meadow/recipes.toml
[[recipe]]
pattern = ["ss", "ss"]
keys = { s = "sun_shard" }
output = "sunstone"

[[recipe]]
pattern = ["w", "w"]
keys = { w = "base:wheat" }
output = "honey_bread"
count = 2

[[recipe]]
pattern = ["p", "g"]
keys = { p = "#base:planks", g = "#shiny" }
output = "sun_shard"
count = 4

[[smelt]]
input = "sunstone"
output = "sun_shard"
time = 6.0

[[fuel]]
item = "sun_shard"
burn = 20.0
speed = 1.5
```

- `pattern` rows are strings, one character per grid cell, space =
  empty. Up to 3×3 (2×2 fits the inventory grid; 3×3 needs a crafting
  table). Recipes match anywhere in the grid and mirrored.
- `keys` maps each character to an item name or a `#tag`.
- `count` defaults to 1. `[[smelt]]` `time` defaults to 8 s.
- `[[fuel]]` `burn` is seconds of furnace heat; `speed` (default 1.0)
  multiplies smelt rate while that fuel burns — base's `ember` smelts
  at 2×.
- `[[bloomery]]` `{ charge, fuel, bloom }` declares a bloomery firing
  chain: a lit stack converts 2 charge + 2 fuel per bloom (+2 bonus
  blooms on a full 8+8 batch) over half an in-game day.
- `[[worked]]` `{ input, output, strikes = 3, station = "anvil",
  tool = "hammer", count = 1 }` declares station work. The anvil wants
  a `hammer = true` item; `station = "quern"` with `tool = "none"`
  grinds bare-handed (minerals into pigment). `count` is the output
  stack.
- `[[kiln]]` `{ powder, glass }` maps a pigment to its colored glass;
  the `[kiln_base]` table `{ sand, fuel, clear }` declares the kiln's
  staples. One powder colors a whole batch; no powder fires clear.

## tags.toml

```toml
# mods/meadow/tags.toml
[[tag]]
id = "shiny"
items = ["sun_shard", "base:copper_ingot"]
```

Tags merge by qualified name: declaring `id = "base:planks"` with
your own items **extends** the shared planks group, so your wood
works in every existing plank recipe. Unknown items in a tag are
skipped silently.

## features.toml

```toml
# mods/meadow/features.toml
[[feature]]
type = "ore"
block = "sunstone_ore"
replaces = "base:stone"
vein_size = 5
per_chunk = 3
y_range = [10, 40]
```

The only `type` today is `"ore"`: random-walk veins of `block`
replacing `replaces` (default `base:stone`). Defaults: `vein_size` 5
(1–32), `per_chunk` 6 (0–64), `y_range` [4, 60].

## animals.toml

```toml
# mods/meadow/animals.toml
[[animal]]
id = "meadow_hen"
name = "Meadow Hen"
biomes = ["plains", "forest"]
health = 6
speed = 2.2
flee_range = 5
group = [2, 3]
rarity = 5
tex = "hen.png"
head_tex = "hen_face.png"
drops = [{ item = "base:raw_fowl", min = 1, max = 2 }]
breed_food = "base:wheat"

[animal.model.body]
size = [6, 5, 8]
at = [0, 3, 0]

[animal.model.head]
size = [4, 4, 4]
at = [0, 7, -5]

[animal.model.leg]
size = [1.5, 3, 1.5]
at = [2, 0, 2]
```

- Models are boxes in **pixels, 16 px = 1 block**. `size` is
  [width, height, depth]; `at` is [center x, bottom y, center z], with
  **−Z as the model's forward**. A box named `leg` mirrors into four
  at (±x, y, ±z) and swings while walking. Boxes whose name starts
  with `head` bob and show `head_tex` on their front face. Any box can
  set `tex = "..."` for its own texture (antlers, saddles). Omitting
  `model` entirely gets you a default quadruped.
- Collision size derives from the model automatically.
- Defaults: `health` 8, `speed` 2, `flee_range` 6 (0 = bold, flees
  only when hurt), `group` [1, 2], `rarity` 6 (1-in-N eligible chunks
  spawn a group), `sound_pitch` 1.0.
- Wildlife with `breed_food` can be fed and bred; two fed adults near
  each other bear young.
- Hostile creatures (wardens) set `hostile = true` plus `attack`
  (half-hearts, default 3), `aggro_range` (12), `ire_min` (world ire
  before they may spawn), `spawn_light_max` (3), and optionally
  `movement = "float"`, `emissive = true`, and `projectile =
  { tex, damage, speed = 14, cooldown = 2 }`. Hostiles spawn from
  darkness pressure, never persist, and dissolve in daylight.
- `glow = [r, g, b]` (color × intensity) gives a creature a real
  shadow-casting light the player sees coming — the two nearest
  glowing creatures cast (emberkin's firelight, rimewisp's shimmer).
  Warm glows flicker like flame; cool ones hold steady.

## structures.toml

```toml
# mods/meadow/structures.toml
[[structure]]
id = "sun_shrine"
biomes = ["plains"]
rarity = 40
palette = { s = "base:cobblestone", g = "sunstone" }
layers = [
  ["sss", "sCs", "sss"],
  ["s.s", ".g.", "s.s"],
]
loot = "shrine_loot"

[[loot]]
id = "shrine_loot"
entries = [
  { item = "sun_shard", weight = 3, count = [1, 3] },
  { item = "base:old_coin", weight = 1 },
  { item = "base:copper_pickaxe", weight = 1, durability = 0.4 },
]
```

- `layers` stack bottom-up; each layer is a list of rows, one
  character per block. `.` leaves the terrain untouched, `~` forces
  air, `C` places a loot chest (rolled from `loot`, owned by the wild
  — first opening costs a little ire). Everything else must be in
  `palette`.
- `rarity` is 1-in-N eligible chunks (at most one structure per
  chunk, placed once at first generation, deterministic per seed).
- `placement = "buried"` sinks the structure `depth = [min, max]`
  blocks (default [5, 15]) under the surface with a rubble hint on
  top; otherwise it sits on the surface.
- `[[loot]]` tables are shared by chests and `brush` blocks. `weight`
  (default 1) is the relative roll chance, `count` (default [1, 1]) a
  uniform range, `durability` a fraction of max wear for tools that
  should surface already-used.

## aliases.toml

```toml
[[alias]]
old = "meadow:sunstone_brick"
new = "sunstone"
```

Renamed something? Map the old qualified name to the new one and
existing worlds keep loading losslessly (saves store blocks by name
palette). Base uses this for its own renames — see
`base/aliases.toml`.

## textures/

PNG files, any size (nearest-neighbor scaled into the atlas; base
tiles are 32×32). Referenced by filename from the TOML. The atlas has
1024 slots (a 32×32 grid) and built-ins use the first ~216, leaving
**~800 tiles for all installed mods together**. Texture packs can override mod tiles by
shipping `tiles/<mod_id>/<file stem>.png`.

## main.rhai — scripts

```rhai
// mods/meadow/main.rhai
fn on_world_start(world) {
    hud_message("The meadow hums with light.");
}

fn on_block_break(x, y, z, block) {
    if block == "meadow:sunstone_ore" {
        let n = storage_get("mined");
        let count = if n == "" { 1 } else { n.parse_int() + 1 };
        storage_set("mined", count.to_string());
        hud_message("sunstone mined: " + count);
        play_sound("craft");
    }
    true
}
```

Scripts are [Rhai](https://rhai.rs). Define any of these functions
and they are called when the event happens (block names are always
fully qualified):

| event | args | notes |
|---|---|---|
| `on_world_start` | `(world_name)` | after a world loads |
| `on_tick` | `(dt)` | ~10 Hz while playing |
| `on_block_break` | `(x, y, z, block)` | return `false` to cancel |
| `on_block_place` | `(x, y, z, block)` | return `false` to cancel |
| `on_interact` | `(x, y, z, block)` | right-click on a block; return `false` to cancel |
| `on_craft` | `(item)` | after a craft is taken |
| `on_animal_killed` | `(species, x, y, z)` | adult wildlife/warden death |
| `on_player_respawn` | `()` | after the respawn button |
| `on_mode_change` | `(mode)` | `"survival"`/`"creative"` toggle |

API callable from any handler:

- `get_block(x, y, z) -> name`, `set_block(x, y, z, name)`,
  `surface_height(x, z) -> y`
- `give(item, count)` — into the player's inventory (overflow drops)
- `hud_message(text)` — toast
- `play_sound(name)` — `"click"`, `"place"`, `"pickup"`, `"hurt"`,
  `"craft"`, `"splash"`
- `spawn_animal(species, x, y, z)`
- `storage_get(key) -> string` / `storage_set(key, value)` — per-mod
  key-value store, saved with the world
- `log(text)` — to the terminal, prefixed with your mod id

Sandbox: scripts have no filesystem or network access, and each event
call is capped (200k operations, call depth 32) so an accidental
infinite loop errors instead of hanging the game. A script that fails
to compile keeps its previous working version running and shows the
error on the MODS screen.

## Hot reload

Any change under `mods/` (or `packs/`) is picked up within a second
while the game runs — registry, atlas, and scripts rebuild, and the
live world remaps to the new ids by name. F5 forces an immediate
reload. Script `storage` survives reloads.

## Save safety

Worlds save a name palette, so block ids can shuffle freely between
sessions. Removing a mod turns its placed blocks into harmless
placeholder blocks instead of corrupting the save; reinstalling the
mod brings them back.

## Multiplayer

When a guest joins a host with different content, the host streams its
entire mods folder (data + textures) and the guest plays with it —
nothing to install. **Scripts are the exception: `.rhai` files never
leave the host** and run host-side only, so data defines what exists
and scripts stay private to the world that runs them.
