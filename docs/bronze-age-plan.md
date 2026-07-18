# Bronze Age — Native Metals, Furnace & Block Entities

Decisions (agreed 2026-07-18):

1. **Metal-age content goes native** in `base/`: copper moves in from the
   mod, tin and bronze are added alongside. A registry **alias table**
   migrates existing worlds losslessly (`copper:ore` → `base:copper_ore`).
   `mods/copper` retires; a new small example mod keeps dogfooding the
   mod system.
2. **Real smelting**: a furnace block with fuel, progress, and time —
   built on a new **block entity** system (per-block persistent state),
   which is the same infrastructure chests need later.
3. **Tool tier gating** lands now: tools carry a tier, blocks may require
   a minimum tier to drop.

## 1. Block entities (engine)

Per-block state for interactive machines.

```rust
// world.rs
pub enum BlockEntity {
    Furnace(FurnaceState),
    // future: Chest(ChestState), ...
}
pub struct FurnaceState {
    pub input: Option<ItemStack>,
    pub fuel: Option<ItemStack>,
    pub output: Option<ItemStack>,
    pub progress: f32,   // seconds into current smelt
    pub burn_left: f32,  // seconds of fuel remaining
    pub burn_total: f32, // for the flame indicator
}
```

- Storage: `World.block_entities: HashMap<(i32,i32,i32), BlockEntity>`.
- Lifecycle: created on placing a block whose def declares an entity;
  on break, contents drop as item entities and the entry is removed.
  `set_block` to a different block clears any stale entity at that pos.
- Ticking: `World::tick_entities(dt, reg)` every frame (the map is tiny);
  furnace logic: if `burn_left > 0` and input is smeltable, `progress +=
  dt`; at the recipe's time, consume 1 input, emit output (merge-checked);
  when `burn_left` hits 0 and smelting should continue, consume 1 fuel
  item (`burn_left = its burn value`), else progress decays.
- Persistence: `saves/<world>/entities.toml` — coords + kind + slots
  (items serialized by string id/count/durability, so they survive mod
  and registry changes by name). Saved in `save_modified`, loaded in
  `load_or_create`.
- Hot reload: item stacks inside entities remap by name like inventories.

## 2. Furnace (content + UI)

- `base:furnace`: cobblestone-family block (pickaxe, hardness 6.5,
  requires_tool), crafted from 8 cobblestone in a ring (3x3).
- Block interaction becomes data: `interaction = "crafting" | "furnace"`
  on BlockDef (replaces the `interactive` bool; crafting table sets
  `"crafting"`). Right-click opens `Screen::Furnace { pos }`.
- Furnace screen: input slot (top-left), fuel slot (below), output slot
  (right), flame icon scaled by `burn_left/burn_total`, progress arrow by
  `progress/time`. Input/fuel use normal click_stack rules; output is
  take-only. Closing returns nothing (state lives in the entity).
- While the screen is open the entity keeps ticking; the UI just renders
  the live state. If the block is broken while open, the screen closes.

### Data formats

```toml
# recipes.toml
[[smelt]]
input = "base:raw_copper"
output = "base:copper_ingot"
time = 8.0

[[fuel]]
item = "#base:logs"      # tags allowed
burn = 15.0
```

Registry: `smelts: Vec<SmeltDef>`, `fuels: Vec<(Ingredient, f32)>`.
Base fuels: logs 15s, planks 7.5s, sticks 2.5s, charcoal 40s.
Base smelts: raw copper → copper ingot, raw tin → tin ingot,
bronze blend → bronze ingot, any log (tag) → charcoal 6s.

## 3. Metals & tools

| Tier | Tool set | Speed | Durability | Notes |
|---|---|---|---|---|
| 1 wood | pick/axe/shovel | 4x | 59 | existing |
| 2 stone | pick/axe/shovel | 8x | 131 | existing |
| 2 copper | pick/axe/shovel | 9x | 160 | slight stone upgrade, cheap |
| 3 bronze | pick/axe/shovel | 12x | 225 | the tier that matters |

- **Copper** (moved native): `base:copper_ore` (y 8–72, vein 7, 8/chunk,
  requires pickaxe) → drops `base:raw_copper`; smelt → `base:copper_ingot`;
  `base:copper_block` (2x2 ingots); full tool set (ingots, not raw, as
  material — recipe change from the mod's raw-copper pickaxe).
- **Tin**: `base:tin_ore` (y 8–56, vein 5, 5/chunk — rarer than copper,
  silvery texture) → `base:raw_tin` → `base:tin_ingot`. No tin tools
  (too soft — flavor is the point).
- **Bronze**: craft 3 copper ingots + 1 tin ingot → 4 `base:bronze_blend`;
  smelt blend → `base:bronze_ingot`; `base:bronze_block`; full tool set.
- Textures: procedural — copper warm orange (reuse/adapt mod art),
  tin pale silver-grey, bronze deep gold-brown; ore tiles = stone base +
  colored nuggets; ingot icons drawn like the existing pixel-art items.

### Tier gating

- `ItemDef.tool: Option<(ToolKind, f32 speed, u8 tier)>`.
- `BlockDef.min_tier: u8` (default 0); TOML `min_tier = 3`.
- `drops_for` requires kind match AND `tool.tier >= block.min_tier`.
- Current content: all existing blocks stay tier 0/1 (any pickaxe);
  future iron ore will set `min_tier = 3` (bronze+).
- Speed unaffected by tier (only drops) — matches classic feel.

## 4. Alias table (save migration)

```toml
# base/aliases.toml
[[alias]]
old = "copper:ore"
new = "copper_ore"     # qualified against the declaring mod ("base")
# ... raw, block, pickaxe, ingot equivalents
```

- Registry: `aliases: HashMap<String, String>` applied as a fallback in
  `block_id()` / `item_id()` lookups (one hop, resolved at build time
  into the name maps so lookups stay O(1)).
- Covers world palettes (block names) and furnace/entity item names.
  Player inventories aren't persisted, so nothing else needs mapping.

## 5. Example mod replacing `mods/copper`

`mods/gems`: ruby ore (deep, rare, min_tier 2), cut ruby item, ruby
block, a script that counts rubies mined and toasts milestones — small,
clearly optional, exercises features (ore worldgen, tags, script KV,
min_tier from a mod).

## Tests

- Block entities: place furnace → entity exists; break → contents drop,
  entity gone; save/load round-trips state by item name.
- Smelting: fuel consumption order, progress timing, output merging,
  charcoal from any log via tag fuel, no-fuel decay.
- Tiers: wood pick fails on a `min_tier = 2` fixture block, stone pick
  succeeds; tin ore mineable with wood pick.
- Metals: ores generate in expected bands; full recipe chain
  raw → ingot → blend → bronze → tools resolves; alias lookups map old
  copper names to new ids (fixture world palette with `copper:ore`
  loads as `base:copper_ore`, not placeholder).
- Regression: example mod loads, its min_tier applies.

## Implementation order

1. Registry: tool tiers, `min_tier`, `interaction` string, aliases,
   smelt/fuel data.
2. Block entity storage + persistence + furnace tick logic (headless,
   fully testable before UI).
3. Furnace UI screen + interaction routing.
4. Native metals content (blocks/items/recipes/features/textures) +
   copper migration + `mods/gems`.
5. Tuning, tests, screenshots, README.
