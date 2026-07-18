# Animals — Wildlife, Hunting & Weapons

Decisions (2026-07-18): **passive animals only** (hostiles are a later
milestone); **biome-native wildlife** rather than universal livestock
(the trees/crops philosophy applied to fauna); **hunt-only** in v1
(breeding/taming/penning is a follow-up); **real weapons** — a sword
tier set alongside the existing tools. This milestone wakes the dormant
**Protein** nutrition track.

## 1. Mob engine (the core of the milestone)

New `src/mobs.rs`; mobs live in `World` (like block entities).

```rust
pub struct Mob {
    pub species: usize,        // index into reg.animals
    pub pos: Vec3, pub vel: Vec3, pub yaw: f32,
    pub health: f32,
    pub state: MobState,       // Idle | Wander | Flee
    pub state_timer: f32,
    pub threat: Option<Vec3>,  // what we're fleeing from
    pub anim_phase: f32,       // leg swing
    pub hurt_flash: f32,
}
```

- **Movement**: light AABB mover sharing the collision helpers —
  gravity, ground friction, and the classic auto-jump when a 1-block
  step blocks the way. No pathfinding: steer yaw toward a target point
  and walk. Avoid walking into water deeper than 1 block (probe ahead).
- **AI states** (ticked with the world):
  - *Idle*: stand, occasional head turn; short random duration.
  - *Wander*: pick a point 4–10 blocks away, walk to it, back to idle.
  - *Flee*: triggered by taking damage, or by the player closing within
    the species' `flee_range` (skittishness varies — deer bolt early,
    boars barely care). Run directly away at `speed * 1.6` for 4–6 s,
    then calm down. Fleeing mobs jump ledges and ignore water rules.
- **Ticking scope**: only mobs in loaded chunks tick; others are frozen
  in storage. Fixed cap on loaded mobs (~64) for perf sanity.
- **Persistence**: `saves/<world>/animals.toml` — species by string id,
  pos, health (mod-safe by name, like entities.toml). Saved in
  `save_modified`, loaded in `load_or_create`; unknown species skip.

## 2. Spawning

- **Worldgen**: during chunk feature pass (same place trees/wild crops
  spawn), roll each species whose `biomes` matches: an eligible chunk
  spawns a group of `group = [min, max]` on surface grass/snow/sand.
  Density tuned so wildlife feels sparse-but-present (~1 group per
  4–8 chunks in a species' home biome).
- **Repopulation**: a slow random tick — if the loaded area is under
  its wildlife cap, a matching-biome chunk more than 24 blocks from the
  player may spawn a small group. Wildlife recovers if overhunted, but
  slowly; no despawning (the wildlife is persistent, not scenery).

## 3. Data-driven species (`base/animals.toml`, mods can add more)

```toml
[[animal]]
id = "deer"
label = "Deer"
biomes = ["forest"]
health = 10
speed = 2.4
flee_range = 10          # blocks; skittishness
group = [1, 3]
tex = "@deer"            # body tile; face tile derived (@deer_face)
drops = [
  { item = "base:raw_venison", min = 1, max = 2 },
  { item = "base:hide", min = 0, max = 1 },
]
sound_pitch = 1.0        # scales the procedural hurt/call sounds
[animal.model]           # boxy model, sizes in px (16 = 1 block)
body = { size = [6, 7, 12], at = [0, 9, 0] }
head = { size = [4, 4, 5], at = [0, 14, -7] }
leg = { size = [2, 9, 2], at = [2, 0, 4] }   # mirrored x4 automatically
```

- Boxes are textured from the species' atlas tiles (procedural fur/hide
  patterns like block art; **texture packs and mods re-skin animals for
  free** since they're just named tiles).
- Animation by convention: `leg` boxes swing with `anim_phase` while
  moving (opposite pairs out of phase), `head` gets idle turns and a
  small bob. That's the whole animation system — alpha-MC energy.
- Renderer: mob boxes are appended to a per-frame dynamic vertex buffer
  in world space, drawn with the existing atlas pipeline (the item-drop
  path scaled up). Hurt flash tints the model red briefly.

### Species roster (8 biomes, 5 models — hares share one)

| Species | Biomes | Drops |
|---|---|---|
| Deer | Forest | raw venison, hide |
| Boar | Jungle | raw boar meat, hide |
| Mountain goat | Mountains | raw chevon, hide |
| Grouse | Taiga | raw fowl, feather |
| Rabbit | Plains | raw rabbit |
| Desert hare | Desert, Scrubland | raw rabbit |
| Snow hare | Arctic | raw rabbit |

## 4. Weapons & combat

- **Swords** per existing tier: wood (dmg 4, dur 59), stone (5, 131),
  copper (6, 160), bronze (8, 225) — damage in half-hearts. Recipe:
  2 material over a stick (planks tag / cobblestone / ingots). Data:
  a `damage` stat on ItemDef (`damage = 8`); tools get modest implicit
  damage (axe 3, pick/shovel/hoe 2, empty hand 1) so hunting without a
  sword works, just badly.
- **Attacking**: left click raycasts mob AABBs before blocks (nearest
  wins). Hit = damage + knockback impulse + red flash + flee trigger +
  hurt sound (existing `burst` synth, pitched per species) + weapon
  durability −1 + a hunger exhaustion tick (hunting costs energy).
- **Death**: drops roll and spawn as item entities; a short shrink-out
  instead of a particle system. Creative mode: normal damage rules
  (no invulnerable-swings special case needed — mobs are passive).
- Script hooks: `on_animal_killed(species, x, y, z)` event and a
  `spawn_animal(species, x, y, z)` host fn for mods.

## 5. Food: Protein wakes up

- New foods: raw/cooked versions of venison, boar, chevon, fowl, rabbit
  (furnace smelts, cooked strictly better — matches the existing
  raw→cooked pattern). All carry `protein` nutrition; cooked portions
  are the efficient path.
- **Hearty stew**: any meat (`#base:meats` tag) + potato + mushroom —
  the protein counterpart of forest stew, three categories at once.
- Protein track activates: drop the "SOON" label in the nutrition
  panel; the ≥40 threshold now grants its +2 max health like the other
  tracks (ceiling rises accordingly). No other tuning changes.
- **Hide → leather** (furnace tan, like log → charcoal), tagged
  `#base:hides`/`#base:leather` as material for the future armor
  milestone; feathers likewise bank for future arrows.

## Tests

- Species TOML parses; a mod-added species registers and spawns.
- Mob mover: gravity settles on ground; 1-block step auto-jumps;
   2-block wall stops.
- AI: damage triggers Flee away from the threat point (distance grows
  over ticks); flee decays back to Idle; flee_range proximity triggers
  for skittish species and not for bold ones at the same distance.
- Worldgen spawning: forest chunks produce deer groups within group
  bounds, deserts produce no deer; loaded-mob cap respected;
  repopulation only under cap and beyond 24 blocks of the player.
- Combat: mob raycast beats the block behind it; sword damage stat
  applies; durability decrements; death rolls drops within min/max and
  spawns item entities.
- Persistence: save/load round-trips species/pos/health by name;
  unknown species entry skips cleanly.
- Food: smelt chain for each meat resolves; protein nutrition applies
  on eating; max health rises when protein ≥ 40; hearty stew crafts
  via the meats tag.
- Weapons: all four sword recipes resolve; hand/tool implicit damage.

## Implementation order

1. Registry: species defs, `damage` stat, meat/hide/feather items,
   sword recipes, smelts, tags (headless-testable).
2. Mob struct + AABB mover + AI states + persistence (headless).
3. Worldgen spawning + repopulation + caps.
4. Rendering: box models, leg/head animation, hurt flash.
5. Combat: mob raycast, damage/knockback/death/drops, sounds,
   exhaustion.
6. Protein activation + stew + nutrition panel label.
7. Species art (procedural tiles), balance pass, screenshots, README.
