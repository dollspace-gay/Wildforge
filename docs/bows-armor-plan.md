# Bows & Armor — Design Plan

Decided 2026-07-18. Ranged weapons built on the warden projectile
engine, and the first armor tiers: **leather** (the hide loop pays off)
and **bronze** (the forge tier does too). Everything data-driven; mods
can add bows, ammo, and armor.

## 1. Bows — two tiers, honoring the banked materials

| Bow | Recipe | Damage (charge-scaled) | Durability | Gate |
|---|---|---|---|---|
| **Hunting bow** | 3 sticks + 2 plant fiber | 3–6 | 96 | fiber drops from thornlings (ire 0 wardens) — reachable from the first nights |
| **Warbow** | 3 living wood + 2 plant fiber | 5–10 | 240 | living wood drops from dryads (Provoked+) — the wild supplies the weapons you turn back on it |

Recipe shape (vertical bow): `.sf / s.f / .sf` — limbs down one column,
fiber string down the other (s = stick or living wood, f = fiber).

**Arrows**: 1 stick + 1 feather + 1 cobblestone → **4 stone-tipped
arrows** (grouse finally justify their feathers). Stack 64. Future:
ember-tipped fire arrows (the material already exists — not v1).

### Mechanics

- **Charge draw**: holding right-click with a bow draws it (needs an
  arrow in the inventory). 0.25 s minimum to fire, full power at 1.0 s;
  damage and velocity scale with charge. Release fires; Esc/switching
  hotbar cancels. Draw progress shown at the crosshair (reuse the eat
  ring) plus a subtle FOV pull at full draw is a stretch goal — skip if
  fiddly.
- **Player projectiles** reuse `mobs::Projectile` with an `owner` flag:
  player arrows collide with **mobs** (nearest AABB along the path)
  instead of the player. Hits apply damage + knockback through the
  existing `hurt` path — drops, hurt flash, sounds, ire (+2 for
  wildlife) all just work.
- **Arrow recovery**: an arrow that hits a block drops as an arrow item
  entity (100% recovery); arrows that hit a mob are consumed. Ammo
  consumed on fire, pulled from anywhere in the inventory.
- Bow wears 1 durability per shot (existing durability system).
- Sounds: draw creak + release twang (synth bursts, pitch by charge).

### Data

```toml
[[item]]
id = "hunting_bow"
max_stack = 1
durability = 96
bow = { damage = 6, speed = 24 }   # damage at full charge
[[item]]
id = "arrow"
ammo = "arrow"                     # bows fire items with ammo = "arrow"
```

`ItemDef.bow: Option<BowDef>`, `ItemDef.ammo: Option<String>` — mods
can add bows and new ammo classes.

## 2. Armor — leather and bronze

Four equipment slots: **head, chest, legs, feet** — a column of slots
on the inventory screen next to the crafting grid. Click-to-equip with
normal click_stack rules; a piece only enters its matching slot.

### Pieces & values (armor points)

| Piece | Leather | Bronze | Recipe shape |
|---|---|---|---|
| Helmet | 1 | 2 | `xxx / x.x` (5) |
| Chestplate | 3 | 4 | `x.x / xxx / xxx` (8) |
| Leggings | 2 | 3 | `xxx / x.x / x.x` (7) |
| Boots | 1 | 2 | `x.x / x.x` (4) |
| **Full set** | **7** | **11** | |

- Materials: leather (tanned hide — the furnace loop) and bronze
  ingots. Leather durability ~70/piece, bronze ~190.
- **Reduction**: each armor point blocks 4% of incoming damage, capped
  at 60%. Full leather = 28%, full bronze = 44%. Applies to warden
  contact damage and bolts (the `hurt_player_from_wild` path) — NOT to
  falls, drowning, or starvation; the wild's answer is what armor is
  for.
- **Wear**: every worn piece loses 1 durability when the player takes
  reduced damage; broken pieces vanish (classic).
- **HUD**: armor pips above the hearts (mirroring the hunger row) when
  any armor is worn; hidden at 0.
- **Persistence**: armor slots join player.toml (by item name,
  mod-safe, like everything else).
- Data: `armor = { slot = "chest", points = 3 }` on any item — mods get
  armor for free. Registry: `ItemDef.armor: Option<(ArmorSlot, u32)>`.

No visual armor on a player model (first person, no model) — the pips
and the surviving are the feedback.

## 3. Integration notes

- Creative mode: bows fire without consuming arrows; armor equips but
  damage is already nulled.
- Item browser: new items appear automatically; bow/armor recipes
  resolve in the recipe view (fiber/living wood show their warden
  sources implicitly by being in USES).
- `WILDFORGE_GIVE` gains a hunting bow + 16 arrows + leather chest for
  headless verification.
- The Dead screen inventory-scatter already handles armor slots via
  drain — extend drain to include them.

## Tests

- Recipes: both bows, arrows (count 4), all 8 armor pieces resolve.
- Bow: draw requires an arrow; firing consumes exactly one and wears
  the bow; charge scales damage/velocity between min and full; no fire
  under the 0.25 s minimum.
- Player arrows hit the nearest mob (not the shooter), apply damage +
  knockback, and are consumed; block hits drop a recoverable arrow
  item; wildlife kills by arrow still add ire.
- Armor: equip only in matching slot; points sum; 4%/point reduction
  capped at 60% applied to warden damage but not fall damage; each
  worn piece wears on hit; broken pieces vanish; armor persists through
  save/load by name; death scatters worn armor.
- Mod-added bow/ammo/armor parse and function.

## Implementation order

1. Registry: `bow`, `ammo`, `armor` item fields + base content
   (bows, arrows, 8 pieces, recipes) + atlas/gemini art.
2. Armor slots: inventory storage + equip rules + UI column + HUD pips
   + player.toml persistence + reduction/wear in the damage path.
3. Bow mechanics: charge state on right-hold, release-to-fire, ammo
   consumption, player-owned projectiles vs mobs, arrow recovery,
   sounds, draw UI.
4. Balance (a full-charge warbow one-shots a rabbit, three-shots a
   thornling; full bronze survives a wrathwood swing meaningfully
   better), tests, screenshots, README, push.
