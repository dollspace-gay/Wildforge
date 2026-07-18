# Hostiles — The Wild Answers

Decided 2026-07-18: lore direction **"the Wild answers"** (underground
threats are the same wild, older); **territorial lurkers**, not
base-hunters; **reciprocity ships in v1 as the core difficulty system**;
tone: **elemental & vegetal** — wardens of wood, stone, ember and frost,
hostile dryads, and carnivorous walking trees.

## 0. The lore (the "why")

The world is alive, and it was never given to you. It tolerates small
takers — animals graze, fires burn out, everything returns. The forge
is different: it turns forests into charcoal, empties the veins, cuts
what was growing into what is built. The Wild keeps its own places —
the night, the deep woods, the dark under the ground — and out of them
it sends **wardens**: not evil, not cursed, just the world's answer.
They are expressions, not creatures; at dawn, or when the Wild calms,
they return to it. The more you take, the more it sends.

One page of this, no more. No hell, nothing broken. The world is whole
— that's the problem.

## 1. Ire — reciprocity as the difficulty system (core, v1)

A per-world scalar `ire: 0..100`, persisted in world.toml.

**Gains** (tuned so a normal industrious day nudges it, a clearcut
spikes it):
- Breaking natural blocks: logs +0.3, ore +0.4, stone +0.05,
  leaves/dirt +0.02. Player-placed blocks are free to break (track via
  `modified`? No — simpler: breaking ANY block of those classes counts;
  the wild doesn't audit receipts).
- Killing wildlife: +2 per animal.
- Furnace fuel: +0.1 per item burned.

**Decay**: −4 per in-game day (the wild forgives, slowly). Floor 0.

**Giving back** (active reduction, v1): the wild notices things growing
where you walk — planting a crop −0.2, a crop reaching its final stage
−0.5, capped at −8 per day from planting (mending must stay slower
than taking; a clearcut can't be laundered with a seed drawer).
Deliberate asymmetry: spikes are fast, amends are slow. Note that high
ire is not purely bad — warden drops are exclusive materials, so
Wrathful nights are also a harvest. A future **stewardship milestone**
owns the full loop: saplings + tree regrowth (replanted forests
actually return, big payback when a planted tree matures) and an
offering stone the wild accepts overnight.

**Tiers** drive the spawn budget and roster:

| Ire | Tier | Night feel |
|---|---|---|
| 0–20 | Calm | rare, weak spawns — first nights are gentle by design |
| 20–50 | Uneasy | normal pressure |
| 50–80 | Provoked | more, stronger, casters appear |
| 80–100 | Wrathful | elites walk; the woods move |

**UI**: a small vine-ring meter in the inventory screen with the tier
word (CALM/UNEASY/PROVOKED/WRATHFUL); toast on tier change ("The wild
stirs against you." / "The wild settles."). No raw number shown.

Ire is the difficulty curve — there is no difficulty setting.

## 2. The wardens (biome-native, all data-driven)

Extends `animals.toml` — hostiles are species with extra fields, same
box models, per-box textures, same registry. Mods can add wardens.

| Warden | Element | Where | AI | Notes |
|---|---|---|---|---|
| **Thornling** | wood | plains/forest/jungle nights | ground melee | the bread-and-butter chaser: a knee-high carnivorous shrub with a snapping maw |
| **Dryad** | wood | forest/jungle nights, ire ≥ 50 | ground caster | bark-skinned, antlered; lobs thorn bolts, keeps distance |
| **Emberkin** | ember | desert/scrubland nights | floater caster | drifting cinder-wisp (blaze-like), lobs embers; **emissive** — glows in the dark |
| **Rimewisp** | frost | taiga/arctic nights | floater caster | pale cold wisp, frost bolts; emissive faint blue |
| **Gravelurk** | stone | underground (light < 4, any hour) | ground melee, heavy | eyeless hunched boulder-thing; slow, hits hard, the deep's landlord |
| **Wrathwood** | wood | any forest, **ire ≥ 80 only** | ground melee, elite | a walking carnivorous tree, 3 blocks tall; rare, a night to remember |

Three AI archetypes carry all six: ground-melee (thornling, gravelurk,
wrathwood), ground-caster (dryad), floater-caster (emberkin, rimewisp).

### Data (animals.toml additions)

```toml
[[animal]]
id = "thornling"
hostile = true
biomes = ["plains", "forest", "jungle"]
attack = 3            # contact damage, half-hearts
aggro_range = 12
ire_min = 0           # tier gate
movement = "ground"   # or "float"
emissive = false
spawn_light_max = 3   # only in real darkness
# casters add:
projectile = { tex = "@thorn_bolt", damage = 3, speed = 14, cooldown = 2.0 }
```

- **Hostiles are never persisted** — they are expressions of the wild,
  not individuals. Surface wardens dissolve at dawn (despawn when their
  cell turns bright); all despawn beyond 80 blocks or on save/load.
  This keeps animals.toml persistence untouched.
- Wildlife AI additions: `Hunt` state (steer at player, melee on
  contact with a 1 s swing cooldown + knockback) and `Cast` (hold
  6–10 blocks range, lob projectile on cooldown). Floaters use a
  no-gravity mover: hover 1–2 blocks up, bob, drift toward the player.
- Emissive species render at full block-light (they're their own
  lantern) — per-box `tex` gives emberkin a glowing core inside a
  charred shell.

## 3. Spawning (territorial lurkers)

- Runtime spawning, not worldgen: every ~4 s, if the hostile count near
  the player is under the ire budget, roll a spot in a ring 24–56
  blocks out. Eligible: effective light < `spawn_light_max` (effective
  = max(block, sky × current daylight) — so the surface is only
  eligible at night, caves always), biome matches, tier gate passes,
  solid ground (or air pocket for floaters).
- Budget by tier: Calm 2 · Uneasy 6 · Provoked 10 · Wrathful 14 (+1
  wrathwood, rolled separately, max one alive).
- They **lurk**: wander the dark like wildlife until the player enters
  `aggro_range`, then Hunt/Cast. No pathfinding to bases, no digging;
  walls and light genuinely work. Losing sight for ~8 s drops aggro.
- Never within 16 blocks of the player's spawn point.

## 4. Combat additions

- **Mob → player damage**: contact melee with swing cooldown applies
  `attack` via the existing damage path (armor slots in a later
  milestone will hook here); knockback on the player.
- **Projectiles**: a light entity (pos, vel, slight gravity, sprite
  tile) — hits player → damage + small knockback; hits a solid block →
  despawns with a puff sound. This is deliberately also the groundwork
  for **player bows** later.
- Player attacks work on hostiles exactly as on wildlife (same mob
  raycast, knockback, hurt flash, swords matter).
- Sounds: synth growl/crackle/chime per element (pitched bursts).

### Drops — the wild's materials (banked for future crafts)

| Warden | Drop | Future use |
|---|---|---|
| Thornling | plant fiber ×1–2 | **bowstring** (bow milestone), rope |
| Dryad | living wood ×1–2 | bow limbs, wands? |
| Emberkin | ember ×1 | premium fuel (2× charcoal), fire arrows later |
| Rimewisp | frost shard ×1 | preservation/later |
| Gravelurk | stone + 10% raw ore | mining shortcut |
| Wrathwood | heartwood ×2–4 | rare tier of future crafts |

All new items get procedural + gemini art.

## 5. UI & feedback

- Ire vine-ring + tier word in the inventory panel; tier-change toasts.
- Hostile hit flash/sounds reuse the wildlife systems.
- Death screen unchanged (dying to a warden says
  "Reclaimed by the wild" as the subtitle — one string, worth it).

## Tests

- Ire: gains per action class, per-day decay, tier thresholds,
  world.toml round-trip; breaking N logs raises tier from Calm.
- Spawning: dark-only (lit cell rejected; torch ring suppresses),
  night-only on the surface but any-hour underground, biome match,
  budget cap per tier, ire gate (no dryads at Calm), spawn-point
  exclusion radius, dawn despawn.
- AI: Hunt aggros within range and steers at the player; contact
  damage respects the swing cooldown; aggro drops after losing sight;
  Cast holds distance and fires on cooldown.
- Projectiles: hit player → damage; hit wall → gone; gravity arc.
- Floaters hover (no ground contact) and never sink or fall.
- Hostiles never write to animals.toml; despawn on reload.
- Drops roll from the table; fiber/ember/etc. items craftable-ready.
- Wildlife regression: passive mobs unaffected (flee, not fight).

## Implementation order

1. Ire scalar + gains/decay/tiers + world.toml persistence + tests
   (headless, no mobs needed).
2. Data: hostile fields on AnimalToml/AnimalDef; parse + validate.
3. AI: Hunt + Cast states, float mover, contact damage, aggro rules.
4. Projectile entity (tick, collide, render, sounds).
5. Runtime spawner (ring rolls, light/biome/tier gates, budgets,
   dawn dissolve) + despawn rules.
6. Content: six wardens (models with per-box tex, procedural + gemini
   art, sounds, drops + new items), ire UI, death subtitle.
7. Balance pass (a Calm first night must be survivable naked; a
   Wrathful forest night should be genuinely scary), screenshots,
   README lore section, push.
