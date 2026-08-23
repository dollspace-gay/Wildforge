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
  mod.toml          required: id and world_api; name/version/depends optional
  blocks.toml       [[block]] entries
  items.toml        [[item]] entries
  recipes.toml      [[recipe]], [[smelt]], [[fuel]] entries
  tags.toml         [[tag]] item groups for recipes
  features.toml     [[feature]] worldgen (ore veins)
  modes.toml        [[mode]] named survival rulesets (capability E1)
  skills.toml       [[branch]]/[[branch.node]] skill trees (capability E5)
  items.toml        [[item]] entries — frames and components (capability E6)
  machines.toml     [[machine]] data-driven machine kinds (capability E7)
  screens.toml      [[screen]] data-driven UI screens (capability E11)
  arcane.toml       [[resonance]] and [[arcane_site]] entries
  workings.toml     [[working]] entries using qualified native handlers
  preparations.toml [[preparation]] physical process/effect entries
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
world_api = 2
depends = ["base"]
retrogen = "untouched_host_only"
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

## Finite material accounting

Wildforge worlds are finite planets, so nonrenewable material cannot be
created or silently deleted by a content definition. Blocks and items may
declare an exact vector of integer canonical units:

```toml
material_class = "geologically_finite"
materials = { copper = 1200 }
salvage = { station = "forge", recovery = 0.90 }
```

One ordinary metal ingot is 1,200 canonical units. The number is deliberately
divisible: a recipe can represent wire, fasteners, scale, and other partial
objects without floating-point mass. Material keys are stable identities, not
display labels; qualify a mod-owned family (for example
`materials = { "foundry:iridium" = 1200 }`) so another mod cannot accidentally
claim the same ledger account.

Every definition has one `material_class`:

| value | use |
|---|---|
| `renewable` | ecologically reproducible wood, fibre, food, hides, and similar goods |
| `geologically_finite` | ore, metals, gems, salt, and other bounded virgin deposits |
| `transformative_finite` | abundant but bounded stone, sand, clay, glass, and ceramics |
| `consumptive` | intended sinks such as fuel, flux, and food |
| `exceptional` | Wild or magical matter governed by its own source rules |

Names provide a conservative default classification, but a mod should declare
the class explicitly for economically meaningful content. Material identity
propagates through balanced recipes, smelting, station work, block drops, and
placeable item forms. If a transformation disperses material, declare that
quantity in `loss`; if it produces a physical remainder, declare a
`byproducts` item instead. A bad material graph is rejected when a world is
opened.

Durable tracked items default to 90% forge recovery, and tracked machine
blocks default to 95% clean dismantling recovery. An explicit `salvage` table
can override an item's station and recovery fraction from 0 through 1. Broken
durables retain their material in a damaged object; the generated primitive
path recovers 75%, while a forge collects fractional recovered stock until it
can emit ordinary recipe-usable ingots or powders. Scale, slag, and other
remainders stay in the finite ledger rather than being rounded away.

Material vectors are part of save identity. Hot reload or restart may change
art and behavior, but changing an existing item's or block's vector/class
requires an explicit future migration. Removing a mod preserves named
placeholders and their mass; reinstalling the same definitions restores them.

## Finite magic content

Magic definitions use the same finite-world contract as ordinary materials.
An arcane item declares bounded charge capacity and where its charge goes when
the item is destroyed; an organism also declares physical habitat, water,
nutrient, reproduction, harvest, and carrying-capacity rules. Workings and
preparations select from closed, host-owned handlers. They cannot install a raw
callback, credit Current, spawn charged matter, scan private state, transmute,
teleport, or silently delete water, material, charge, or dross.

`arcane.toml`, `workings.toml`, and `preparations.toml` currently have schema
version 1. A mod-owned resonance and geology-gated site look like this:

```toml
schema_version = 1

[[resonance]]
id = "verdance"
label = "Verdance"

[[arcane_site]]
id = "singing_fault"
requires = ["fault", "carbonate_rock"]
capacity_factor = 1.15
resonance = { "base:echo" = 2, "base:stone" = 1 }
rarity = 0.02
radius_cells = 3
```

The site inherits the mod's `retrogen` policy. `untouched_host_only` may add it
only to eligible, unmodified host cells; it never rewrites explored terrain or
invented historical custody. Resonance ids are stable save identities. Rename
ordinary content through `aliases.toml`; changing or removing a resonance,
charged definition, working, or preparation must preserve the saved qualified
id and its ledger custody or use an explicit future migration.

Blocks and items may carry these inline tables:

```toml
# On a cross-shaped plant block.
arcane = { capacity = 120, conductivity = 600, stability = 600, resonance = { "base:tide" = 3, "base:root" = 1 }, on_destroy = "ambient" }
arcane_ecology = { roles = ["gatherer", "indicator"], habitat = ["wetland"], charge_capacity = 96, uptake_per_day = 3, release_per_day = 1, source = "ambient", resonance = { "base:tide" = 3, "base:root" = 1 }, dross_tolerance = 12, water_per_day_hu = 12, nutrient_per_day = 2, reproduction = "spore", seasons = [true, true, true, true], carrying_capacity = 12, harvest = "spore", regrowth_days = 8, min_stability = 0, max_stability = 1000, min_richness = 0 }

# On a geologically finite ore block with an ordinary [[feature]] seam.
material_class = "geologically_finite"
materials = { songstone = 1200 }
arcane = { capacity = 70, conductivity = 900, stability = 500, resonance = { "base:stone" = 3, "base:echo" = 1 }, on_destroy = "ambient" }
arcane_ecology = { roles = ["conductor", "indicator"], kind = "finite_mineral", habitat = ["sedimentary_host"], charge_capacity = 64, uptake_per_day = 0, release_per_day = 0, source = "ambient", resonance = { "base:stone" = 3, "base:echo" = 1 }, dross_tolerance = 8, water_per_day_hu = 0, nutrient_per_day = 0, reproduction = "none", seasons = [true, true, true, true], carrying_capacity = 1, harvest = "destructive", regrowth_days = 0, min_stability = 0, max_stability = 1000, min_richness = 0 }

# On item definitions.
wand_component = { role = "body", capacity = 160, conductivity = 260, stability = 920, resonance = { "base:root" = 4, "base:echo" = 2 }, repair_material = "base:stick", containment = 180 }
charm = { effect = "quiet", charge_per_trigger = 2, capacity = 4096, stability = 950, dross_per_transfer = 12 }
```

Crystals use `kind = "crystal"`, a physical host habitat,
`reproduction = "bud"`, `harvest = "seed_preserving"`, a positive
`regrowth_days`, `crystal_stages`, and `preserving_tool_tier`. Finite minerals
must use the nonreproducing/destructive combination above and have a finite
material vector plus an ordinary worldgen feature. Validation rejects a
renewable source without physical uptake, duplicate harvest routes, unknown
roles/habitats/resonances, out-of-range permille values, and unbounded growth.

A working is a declarative cost and targeting shell around one of the twelve
native handlers. This complete minimal entry observes only local visible
arcane state:

```toml
schema_version = 1

[[working]]
id = "patient_trace"
label = "Patient Trace"
handler = "trace"
mode = "wand"
focus = "base:echo"
charge = 8
charge_per_second = 1
dross = 1
safe_throughput = 12
range = 8
max_magnitude = 1
max_targets = 1
max_duration_ticks = 1200
target = ["visible_arcane", "self"]
interruption = "end_continuous"
disposition = "split"
wear = 1
ambient = true
description = "A bounded local trace aid."
```

The accepted handler names are `trace`, `gleam`, `ignite`, `nudge`,
`rootwake`, `draw`, `fieldmend`, `holdfast`, `settling_rite`, `rooting_bed`,
`ward_boundary`, and `transfer_circle`. The handler fixes the delivery mode,
targets, physical obligations, and effect kind; a definition may narrow them
and set costs, but cannot substitute arbitrary behavior.

A preparation likewise declares its entire physical batch and residue chain:

```toml
schema_version = 1

[[preparation]]
id = "mire_tonic"
label = "Mire Tonic"
process = "distill"
handler = "trace_sight"
application = "drink"
carrier = "alcohol"
solvent_item = "base:fermented_alcohol"
solvent_units = 256
dissolved_units = 0
ingredients = [
  { item = "base:echo_cap_ring", count = 1, retention_permille = 760 },
  { item = "base:rainbell_dew", count = 1, retention_permille = 880 },
]
charge_units = 12
resonance = "base:echo"
charge_rate = [1, 3]
dross_units = 2
steps = ["grind", "load", "heat", "charge", "distill", "cool", "filter"]
temperature_millic = [60000, 82000]
process_ticks = 1200
agitation = "still"
cleanliness_min = 800
output_item = "mirecraft:mire_tonic"
empty_vessel = "base:glass_bottle"
doses = 4
dose_units = 64
residue_item = "mirecraft:spent_mire"
residue_count = 1
shelf_life_ticks = 604800
storage_temperature_millic = [-10000, 30000]
stack_group = "mire_sight"
effect = { duration_ticks = 1200, recovery_ticks = 600, strength = 80 }
description = "A bounded local trace aid with a physical residue."
```

The output and residue items must exist. Dose volume must exactly partition
the solvent, ingredients must be obtainable physical items, and the selected
native effect handler determines which additional effect debits are required.
For example, `transmute_matter` is not a handler and causes the mod to fail
validation instead of partially loading its preparation.

A nonstructural scar manifestation is a block with no item form and a strict
scar table, for example:

```toml
item = false
solid = false
opaque = false
dross_scar = { kind = "wet_film", handler = "water_margin_film", carriers = ["air", "water", "soil"], min_band = "seep", status = "perception_warp", activity = "arcane_squall", max_sites_per_region = 1 }
```

Scar handlers, carriers, threshold bands, statuses, and activities are also
closed enums. They express bounded removable growth/status pressure; they do
not authorize replacement of player construction or inventory mutation. The
registry fixture tests in `src/tests/registry.rs` load each form above and
also prove representative forbidden definitions fail closed.

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
| `interaction` | none | right-click behavior. Screens: `"crafting"` \| `"furnace"` \| `"chest"` \| `"offering"` \| `"bloomery"` \| `"kiln"` \| `"forge"` \| `"stall"` \| `"sign"` \| `"waystone"` \| `"survey"`. Hand-loaded stations: `"anvil"` \| `"quern"` \| `"smoker"` \| `"firebox"` \| `"separator"`. Powered stations (strikes come from the shaft line): `"millstone"` \| `"sawmill"` \| `"lathe"` \| `"iron_lathe"` \| `"boring"`. Power blocks: `"wheel"` \| `"sail"` \| `"pump"` \| `"generator"` |
| `shape` | cube | custom render silhouette (collision stays the cell): `"obelisk"` \| `"signboard"` \| `"rack"` \| `"axle"` \| `"gearbox"` \| `"wheel"` \| `"sails"` \| `"millstone"` \| `"sawbench"` \| `"helve"` \| `"helve_up"` \| `"lathe"` \| `"lathe_iron"` \| `"vice"` \| `"boring"` \| `"pump"` \| `"boiler"` \| `"engine"` \| `"generator"` — pair with `opaque = false` |
| `crop` | none | `{ stages = N, next_chance = 0.3, stage_textures = [...], any_soil = false }` — advances on random ticks; `any_soil` grows off farmland too |
| `harvest` | none | `{ item = "...", count = 2, becomes = "..." }` — right-click yield without breaking |
| `sapling` | none | `{ tree = "oak" }` — grows into that tree species on random ticks (`oak`/`birch`/`spruce`/`jungle`/`acacia`; unknown names grow oak) |
| `bonus_drop` | none | `{ item = "...", chance = 0.1 }` — extra roll on break |
| `brush` | none | `{ table = "loot_id", becomes = "..." }` — archaeology: brushing rolls the loot table, block transmutes |
| `item` | `true` | set `false` to register no placeable item form (fluids, crop stages) |
| `icon` | block texture | item-form icon override |
| `material_class` | inferred | one of the five finite-material classes above |
| `materials` | `{}` | exact material vector per block; a drop or matching held form may propagate it |

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
| `damage_type` | none | damage class your weapon deals (`"pierce"`, `"blunt"`, `"fire"`, ...) — the wild's resistances key on it; `None`/untyped is always full damage |
| `hack` | `false` | tool tag for the construct hack: right-click a `behavior = "construct"` warden holding it to pop its core instead of grinding it into scrap |
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
| `material_class` | inferred | one of the five finite-material classes above |
| `materials` | `{}` | exact material vector per item; balanced transformations can infer an omitted output vector |
| `salvage` | generated for tracked durables/machines | `{ station = "forge", recovery = 0.90 }`; recovery must be 0–1 |
| `frame` | none | **capability E6**: `{ slots = [ { type = "gem", max = 2 } ] }` — this item is a frame with typed slots; a `max = 0` slot, a duplicate type, or an empty `[frame]` is a load error |
| `component` | none | **capability E6**: a slot-type name (`"gem"`); this item slots into matching frame slots. An item cannot be both a `frame` and a `component`, and a component's slot type must be non-empty |

### Frames, components, and loadouts (capability E6)

A frame is a wearable item (it still uses `armor` for slot + points, and
`durability`). A component is a normal item that grants E4 `[[item.stats]]`
modifiers — but those stats only count while the component is slotted in a
worn frame.

- Open the **loadout screen** with `L` (mirror of the `K` skill screen) in a
  world whose mode opts in — the frame's typed slots sit beside its armor
  box; click a slot to insert the held component, click a filled slot to
  take it back **intact** (components never stack down or deplete).
- Derived stats: `armor.points` always count; a component's `[[item.stats]]`
  count while its frame is worn and intact. Frames only — a frame whose
  `durability` hits 0 is **disabled, not destroyed**: it stops granting its
  armor points and its components' stats, but stays in its slot and can be
  repaired through the crafting repair path (a damaged frame + any item whose
  `materials` are a strict subset of the frame's, e.g. a component).
- **Loadout presets**: the `[[loadout_preset]]` table in the profile saves a
  snapshot of the four worn slots (frame id + per-slot component ids); the
  loadout screen's preset buttons SAVE the current loadout and APPLY a saved
  one from the inventory, returning the previously worn frame to your hand.
- Both `frame` and `component` items are one-per-stack.

## machines.toml

A data-driven machine kind (capability E7). The `MachineKind` the engine
uses is just an index into the machines declared here; each entry binds a
kind to a **closed native handler** and the knobs that handler reads. A
block whose `interaction` names the machine id becomes its mouth (right-
click it in-world).

```toml
# mods/meadow/machines.toml
schema_version = 1

[[machine]]
id = "jewel_bench"       # qualified to your mod: meadow:jewel_bench
label = "Jewel Bench"
handler = "workbench"    # one of the closed handler set
mouth = "meadow:jewel_bench"            # the mouth block (unlit face)
mouth_lit = "meadow:jewel_bench_lit"    # optional lit face (fire handlers)
fire_secs = 150.0        # seconds before a batch completes (fire handlers)
charge_slots = 4         # 0..4
fuel_slots = 4           # 0..4
reagent_slots = 0        # 0 or 1 (the kiln's pigment slot)
items_per_fuel = 1       # items each fuel unit fires (the forge's 2)
min_charge = 2           # charge units needed before lighting
min_fuel = 2             # fuel units needed before lighting
```

- The **closed handler set** is `bloomery`, `forge`, `kiln`, `separator`,
  and `workbench`. A handler owns the machine's shell shape, lighting,
  ticking, click rules, and screen; the entries above tune it. An unknown
  `handler`, a duplicate machine id, or an empty label/mouth fails the pack
  on the MODS screen and fails `--mod-qualification`.
- The four built-in machines are declared the same way in
  `base/machines.toml`; base's machines always come first, so kind 0 (the
  default) is a real machine.
- **Workbench machines**: a `handler = "workbench"` machine is a
  recipe-list station. Its screen (right-click the mouth) lists every
  `[[recipe]]` whose `station` is this machine's id and crafts it from the
  player's inventory. Station recipes are **not** craftable on the free
  grid, so a station machine is the only way to reach them.
- Machines ride their own `S2C::MachineContainer` over multiplayer, keyed
  by machine id — a modded machine works on a guest without touching the
  wire protocol.

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
- A tracked `[[recipe]]` must balance exactly: all input material must equal
  output × `count`, `byproducts = [{ item = "...", count = 1 }]`, plus
  `loss = { material = units }`. Use a byproduct whenever the remainder is a
  gameplay object; `loss` is the explicit dispersed/consumed sink.
- `[[recipe]]` gates (spec 3.5): `tech = "kv_key"` requires the player's
  per-player KV flag `kv_key` to read truthy (non-empty and not `"false"`);
  `blueprint = "item"` requires that item in the player's inventory,
  consuming one per craft. Either, both, or neither may appear. A blueprint
  item is extra input in the material graph, so its material vector must be
  balanced (empty vectors balance trivially). Recipes unlocked by a quest
  `learn_recipe` reward default their tech key to `learned:<recipe_id>` (see
  `quests.toml`); an explicit `tech` overrides that.
- A tracked `[[smelt]]` uses the same rule with optional
  `spit = { item = "...", count = 1 }` and `loss = {...}`. A tracked
  `[[worked]]` may also declare `loss`; its input must equal output × `count`
  plus that loss. `[[kiln]]` must set `consumes = true` if its powder carries
  finite material. Bloomery charge and bloom preserve the same vector.
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
  stack. Powered stations (`"sawmill"`, `"lathe"`, `"iron_lathe"`,
  `"boring"`) take their strikes from the shaft line instead of a
  player and convert their whole resting pile (up to 16) in one
  firing — the millstone reads the quern's table, so a quern recipe
  is automatically a millstone recipe.
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

Every member used through a tag in a tracked recipe must carry the exact same
material vector. This prevents a substitution from turning a low-mass item
into a high-mass output. A mismatched tag makes the material graph invalid.

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

The `"ore"` type is random-walk veins of `block` replacing `replaces`
(default `base:stone`). Defaults: `vein_size` 5 (1–32), `per_chunk` 6
(0–64), `y_range` [4, 60].

```toml
# A flag-gated feature (spec 2.5). `feature:<id>` assembly markers place
# `block` sealed; the player cannot mine or right-click it until their KV
# flag `flag` reads `value`, then it opens (swap to `unlocked_block`, or
# just becomes breakable when `unlocked_block` is omitted).
[[feature]]
type = "gate"
id = "sealed_vault"
block = "base:cracked_masonry"
flag = "vault_key"
value = "true"
unlocked_block = "base:air"
message = "The vault is sealed shut."
# Set false to let the block stay mineable while still flagged.
unbreakable_when_locked = true
```

Gate fields: `id` (referenced by `feature:<id>` markers), `block` (the
sealed block placed at the marker), `flag` + `value` (the per-player KV
flag that unlocks it — quest `set_flag` rewards write the same store),
`unlocked_block` (replaces the seal when opened; omit to just make it
breakable), `message` (toast when locked), `unbreakable_when_locked`
(default true). The gate id and block are `mod:gate`/`mod:block`
qualified.

An ore feature's block/drop vector is its exact finite deposit unit. At planet
creation it receives bounded manifest sites and materializing a chunk reserves
the exact represented mass. If the same resource can occur in basalt,
limestone, or another host, declare a feature for each exact `replaces` block;
the shared resource key receives one quota, so host variants do not multiply
the planet's supply.

Any mod with `[[feature]]` entries must declare one policy in `mod.toml`:

| `retrogen` value | existing-world behavior |
|---|---|
| `untouched_host_only` | deterministically replace only declared host blocks in untouched chunks; never touch player edits, structures, block entities, or existing deposits |
| `secondary_recovery` | add processing for existing slag/tailings; terrain and virgin mass remain unchanged |
| `world_event` | add nothing automatically; an authored event must record its external source |
| `no_retrogen` | content is unavailable in that existing world and the player is told to create a new planet |

Retrogen is atomic, recorded, versioned, and idempotent. The host tells every
client which policy applies without exposing reserve coordinates. The planet
manifest stores the content hash; changing saved material identity is not an
implicit migration.

## modes.toml

A `[[mode]]` entry declares a named survival ruleset a world can be started
with (its `mode` in `world.toml`). This is the modding-out mechanism: a mod
can ship a mode that turns off hunger, disables hearts, or makes a world
PvE-only without changing any engine rule.

```toml
# mods/meadow/modes.toml
[[mode]]
id = "cozy"
base = "survival"      # inherit from "survival" or "creative"
hunger = false
fall_damage = false
pvp = false
```

Every field is optional and inherits from `base`. The full toggle set:

| field | what it controls | base survival |
|---|---|---|
| `creative` | no item costs, no survival pressure (the creative superset) | false |
| `hunger` | hunger drains, food gates sprint, starvation weakens | true |
| `fall_damage` | falling hurts | true |
| `drowning` | underwater exhaustion drowns | true |
| `lava_burn` | lava burns | true |
| `hostile_spawns` | night-time / ire-driven wardens spawn | true |
| `ire` | extraction and kills accrue the moral meter | true |
| `hearts` | the hearts / offerings loop is live | true |
| `weather_extremes` | storms intensify world pressure | true |
| `pvp` | players can hurt each other | true |
| `skills` | the skill tree (E5) is live and XP accrues | false |
| `equipment` | modular equipment (E6): frame/component slots and loadout presets are live | false |
| `industrial_ire` | the industrial response gradient (E12): running machines and industrial buildings feed regional ire, and high ire tiers fray wildlife nerves | true |

A mode's `base` names the built-in `survival` or `creative`, or another
declared mode in the same pack (`base = "cozy"` chains through it). Built-in
`survival` / `creative` ids are reserved. A broken base (undeclared or
cyclic) is reported on the MODS screen and the mode falls back to survival.

## skills.toml

A data-driven skill tree (capability E5): branches of nodes that grant E4
stat modifiers, bought with points earned from XP. The engine is closed —
`skills.toml` declares the tree and the XP economy, never behavior.

```toml
# mods/meadow/skills.toml
schema_version = 1

[tree]
max_level = 80        # the level cap
points_per_level = 1  # points paid per level-up
xp_base = 100.0       # XP needed for level 1 -> 2
xp_exponent = 1.5     # xp_for_level(level) = xp_base * level^exponent

[[xp_source]]
id = "mine"           # the engine hook (one of the closed source set)
base = 6              # XP for the first grant of this source
decay = 0.25          # diminishing: the k-th grant is base / (1 + decay * k)

[[branch]]
id = "survivalist"
name = "Survivalist"
tier_gate = 3         # tier-(N-1) nodes needed to buy a tier-N node

[[branch.node]]
id = "hardy"
name = "Hardy"
tier = 1
cost = 1              # defaults to `tier`
description = "+2 max health"
[[branch.node.stats]]
kind = "health"
flat = 2
```

- `schema_version` must be 1. `[tree]` and every `[[xp_source]]`,
  `[[branch]]`, `[[branch.node]]`, and `[[branch.node.stats]]` table are
  optional; `[tree]` and `[[xp_source]]` entries without a branch/node are
  legal (an XP economy with no spendable tree, or vice versa).
- Branch and node ids are qualified to your mod (`meadow:survivalist`,
  `meadow:hardy`); the engine hooks are the **closed XP source set**
  `mine`, `build`, `craft`, `smelt`, `kill`, `fish`, `harvest`. Declare
  `base`/`decay` for the sources you want — an undeclared source grants
  nothing.
- Nodes may sit in any `tier` 1–4. A tier-N node unlocks when the branch
  has `tier_gate` allocated tier-(N-1) nodes. `cost` defaults to the tier.
- A node's `stats` entries are E4 `StatModifier`s (`kind` is one of
  `health`, `stamina`, `stamina_regen`, `carry`, `build_range`,
  `scan_range`, `move_speed`; `flat` and `mult_permille` as in
  `[[item.stats]]`).
- Skills are **mode-gated** (E1): nothing accrues, allocates, or applies
  unless the world's mode sets `skills = true`, so built-in Survival and
  Creative are numerically identical. Press `K` in-game to open the skill
  screen and spend points.

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
  `movement = "float"`, `emissive = true`. Hostiles spawn from
  darkness pressure, never persist, and dissolve in daylight.
- `glow = [r, g, b]` (color × intensity) gives a creature a real
  shadow-casting light the player sees coming — the two nearest
  glowing creatures cast (emberkin's firelight, rimewisp's shimmer).
  Warm glows flicker like flame; cool ones hold steady.

## Enemy archetypes (spec 3.6)

Wardens can do more than chase and swing. The `behavior` field selects
an archetype; `attacks`, `resist`, `hack`, and `builder` shape it.

```toml
[[animal]]
id = "cragjaw"
name = "Cragjaw"
hostile = true
biomes = ["underground"]
health = 42
speed = 1.7
attack = 6
tex = "@gravelurk"

# Damage-class multipliers: 0.6 blunt = armoured against blunt, 1.5 fire
# = burns hot. Absent classes pass untouched (full damage).
resist = [
  { type = "blunt", mult = 0.6 },
  { type = "fire", mult = 1.5 },
]

# The attack wheel: each entry is one attack with its own cooldown and
# trigger range. The first ready attack whose range trips fires.
#   kind = "melee"     | "charge" | "projectile"
#   range               melee uses its reach, projectiles default 14
attacks = [
  { name = "slam", kind = "melee", damage = 6, cooldown = 1.4, damage_type = "blunt" },
  { name = "ram", kind = "charge", damage = 9, cooldown = 6.0, range = 10, damage_type = "blunt" },
  { name = "spark", kind = "projectile", damage = 3, cooldown = 2.4,
    damage_type = "fire", projectile = { tex = "@ember_bolt", damage = 3, speed = 13, cooldown = 2.4 } },
]

behavior = "brute"
```

Archetypes:

| `behavior` | extras | what it does |
|---|---|---|
| `standard` | — | the classic warden: chase and swing (the default; an empty `attacks` list synthesizes exactly the old single melee) |
| `brute` | `resist` | a hit from a class it is vulnerable to (`mult > 1`) enrages it: it attacks on double time for a few seconds |
| `construct` | `hack = { tool = "hack", drops = [...] }` | right-click it holding a `hack = true` tool pops its `hack.drops` core instantly and freezes it inert — far better than grinding it into its ordinary `drops` scrap |
| `builder` | `builder = { template = "name", cap = N, interval = s }` | stamps the named captured template on an `interval`, up to `cap` stamps, whenever it is not fleeing |

- `attacks` entries: `name` (labels the hit), `kind`, `damage`
  (defaults to the `attack` scalar), `cooldown` (default 1 s), `range`
  (melee = its reach, projectile = 14), `damage_type`, and `projectile`
  for projectile kinds. Charge winds up 0.4 s (rooted, frozen facing),
  then dashes the gap at 3× speed and lands its `damage` if it still
  touches the target where it stops.
- `resist` accepts a single table or a list; each is
  `{ type, mult }` where `mult < 1` shrugs that class off and `mult > 1`
  is a vulnerability.
- A builder's `template` resolves against the world template library —
  capture one in-game under the same name, or the builder idles. The
  stamped cells go through the ordinary block path and save normally.
- The base mod ships one example of each archetype (`stonebrute`,
  `cogmaw`, `tumulus`) plus a `hack = true` tool (`base:crank_rod`).

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

fn on_block_break(face, u, y, v, block) {
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
| `on_block_break` | `(face, u, y, v, block)` | return `false` to cancel |
| `on_block_place` | `(face, u, y, v, block)` | return `false` to cancel |
| `on_interact` | `(face, u, y, v, block)` | right-click on a block; return `false` to cancel |
| `on_craft` | `(item)` | after a craft is taken |
| `on_animal_killed` | `(species, face, u, y, v)` | adult wildlife/warden death |
| `on_enemy_destroyed` | `(species, face, u, y, v)` | a hostile warden died (spec 3.6) |
| `on_hurt` | `(species, damage, damage_type)` | you hit a warden (spec 3.6); `damage_type` is `""` for untyped hits |
| `on_attack` | `(attack_name, damage, damage_type)` | a warden hit you with a named wheel attack (spec 3.6); `"bolt"` for projectiles |
| `on_player_respawn` | `()` | after the respawn button |
| `on_mode_change` | `(mode)` | `"survival"`/`"creative"` toggle |

API callable from any handler:

- `get_block(face, u, y, v) -> name`,
  `set_block(face, u, y, v, name)`,
  `surface_height(face, u, v) -> y`
- `neighbor(face, u, y, v, direction) -> map` — directions are
  `"east"`, `"north"`, `"west"`, `"south"`, `"up"`, and `"down"`;
  the result contains `face`, `u`, `y`, `v`, and the rotated
  `direction`, or is empty at a vertical boundary
- `surface_distance(face_a, u_a, v_a, face_b, u_b, v_b) -> blocks`
- `surface_bearing(face_a, u_a, v_a, face_b, u_b, v_b) -> radians`
  clockwise from local north (`NaN` when no stable bearing exists)
- `give(item, count)` — into the player's inventory (overflow drops)
- `hud_message(text)` — toast
- `play_sound(name)` — `"click"`, `"place"`, `"pickup"`, `"hurt"`,
  `"craft"`, `"splash"`
- `spawn_animal(species, face, u, y, v)`
- `spawn_npc(npc, face, u, y, v)` — spawn a friendly NPC (spec 3.1) at a
  block position; `npc` is the `mod:npc` id from `npcs.toml`
- `quest_accept(quest_id)` — accept a quest (its prereq must be done);
  state lives in the per-player KV, so it survives reloads
- `quest_progress(quest_id, objective, n)` — increment an objective;
  completion pays the quest rewards
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

## Settlements (spec 3.4)

Settlements let a mod author a place that visibly *grows* as a player
invests in it. Every growth tier is placed at worldgen; the higher
tiers stay hidden — non-colliding, unrendered, unbreakable — until the
player's reputation crosses the tier threshold, then the world reveals
them once and for all. No geometry is generated at runtime.

A settlement is authored in `pieces.toml`:

```toml
# The settlement: reputation key and growth tiers. Tiers must be
# ascending with strictly increasing thresholds.
[[settlement]]
id = "elder_haven"
tiers = [
  { tier = 2, threshold = 2 },   # revealed when rep >= 2
  { tier = 3, threshold = 5 },   # revealed when rep >= 5
]

# A growth piece: placed at worldgen but hidden until its tier reveals.
[[piece]]
id = "haven_hall"
settlement_tier = 2        # tier 1 (default) is always visible
connectors = [ ... ]
cells = [ ... ]

# The assembly that places the settlement. `settlement` names the
# `[[settlement]]` id and marks the whole walk as a settlement, so any
# tier>1 piece reachable from its pools is hidden at worldgen.
[[assembly]]
id = "elder_haven"
settlement = "elder_haven"
entry = "watch_platform"
pools = { path = "haven" }
```

- A `[[piece]]` with `settlement_tier > 1` must be reachable from a
  settlement assembly, and its tier must be declared in that
  settlement's `tiers`; otherwise registry validation rejects the mod.
- `rep_key` defaults to `rep_<settlement id>`; the player's reputation
  for a settlement lives in their per-player KV under that key.
- Reputation is currently granted by quest rewards:
  `{ add_reputation = "elder_haven", rep_amount = 3 }` (in
  `quests.toml`). Crossing a threshold reveals that tier's cells.
- `{ learn_recipe = "base:etched_tablet" }` (spec 3.5) unlocks a gated
  recipe: on completion it writes the recipe's tech key truthy. The default
  key is `learned:<recipe_id>` (the recipe id is its output item's qualified
  id); an explicit `tech` on the recipe overrides it, and a recipe with
  neither stays always-craftable. Reward types are additive — `item`,
  `set_flag`, `add_reputation`, and `learn_recipe` may all appear together.
- Reveal is one-way and persisted; removing the mod's settlement leaves
  the placed blocks as ordinary solid terrain.
- The reveal is per-world, not per-player: one player's reputation
  reveals the tier for everyone on the save.

## screens.toml

```toml
# mods/meadow/screens.toml
schema_version = 1

[[screen]]
id = "notice_board"
title = "Notice Board"

[[screen.label]]
text = "Welcome to Meadow."

[[screen.kv_label]]
key = "meadow_standing"
prefix = "Standing: "

[[screen.toggle]]
label = "Track bounties"
key = "meadow_track"

[[screen.button]]
label = "Sign the ledger"
action = "sign"
```

- A block opens a screen with `interaction = "screen:<qualified screen id>"`
  (e.g. `interaction = "screen:meadow:notice_board"`). Opening is pure
  presentation: it works identically solo and as a guest.
- **Widgets** render grouped in declaration order within each kind:
  `[[screen.label]]` static text, `[[screen.kv_label]]` a live readout of
  one per-player KV key, `[[screen.toggle]] { label, key }` flips that key
  between `"1"` and `"0"` client-side, and `[[screen.button]]`
  `{ label, action }` dispatches your script.
- **Buttons** call the `on_screen_click(screen_id, action)` hook in your
  mod's `main.rhai`. React with the ordinary command queue — `give`,
  `set_block`, `hud_message`, `storage_set`, ... — and read state back on
  the screen with a matching `kv_label`.
  ```rhai
  fn on_screen_click(screen, action) {
    if screen == "meadow:notice_board" && action == "sign" {
      let n = parse_int(storage_get("signed") ?? "0") + 1;
      storage_set("signed", `${n}`);
      hud_message("You signed the ledger.");
    }
  }
  ```
- On a guest, a button click rides `C2S::ScreenClick`; the host validates
  both ids against its own registry before dispatching, so only buttons
  your content actually declares can ever fire. Scripts run where they
  always have — on the host.
- An unknown widget field, an empty title/label/action, or a duplicate
  screen id fails the pack on the MODS screen and fails
  `--mod-qualification`. Scripts may also open screens directly with
  `open_screen("<qualified id>")`.

