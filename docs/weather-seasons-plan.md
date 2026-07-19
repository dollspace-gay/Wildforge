# Weather & Seasons — the sky joins the simulation

Drafted 2026-07-19. Weather fronts roll over the world, seasons turn
on a calendar, and snow stops being scenery. Everything is
server-authoritative (multiplayer-correct by construction) and leans
on the new lighting stack — overcast skies dim the directional sun,
storms darken the world, and a lightning flash is one frame of
borrowed noon.

## Why this fits Wildforge

The wild already has a mood (ire). Weather gives that mood a sky:
fronts are mostly random, but **storms bias toward high ire** — a
WRATHFUL camp lives under rolling thunder. Seasons give the homestead
a calendar: plant in spring, dry the harvest in autumn, survive
winter on what you stored. Reciprocity gets a rhythm.

## The clock

- `DAY_LENGTH` stays 600 s. New persistent counter: `world.day`
  (u32, increments when `time_of_day` wraps; sleeping counts the
  skipped night). Saved in `world.toml`.
- **Season = (day / SEASON_DAYS) % 4**, `SEASON_DAYS = 12` (a 48-day
  year). Order: Spring, Summer, Autumn, Winter. Within a season,
  `season_progress` 0..1 for smooth transitions.
- HUD: the inventory panel gains one line — `DAY 23 - EARLY WINTER`
  (early/mid/late by thirds).
- Dev hooks in the existing style: `WILDFORGE_DAY`,
  `WILDFORGE_SEASON`, `WILDFORGE_WEATHER` (clear/overcast/rain/storm).

## Weather machine (server-side)

`Server` owns a `Weather` state + timer, stepped in the fixed tick,
persisted in `world.toml`, rolled from the sim rng:

```
Clear ──> Overcast ──> Precip ──> Clear
              └──────> Storm ───> Overcast
```

- Durations: Clear 0.5–2 days, Overcast 0.2–0.5, Precip 0.2–0.8,
  Storm 0.1–0.3 (uniform rolls).
- Transition weights: Overcast→Precip 60%, →Clear 40%.
  Overcast→Storm replaces Precip with probability
  `0.1 + 0.5 * (ire / 100)` — **the wild's anger leans on the sky**,
  but any weather can happen at any ire (no hard gates, no
  weather-as-punishment; it's a thumb on the scale).
- Sleeping to dawn re-rolls weather (the night passed; so did the
  front).
- **Form follows climate, per column**: one global front, but what
  falls is local — `climate.t` below `SNOW_T = -0.35` (arctic/taiga
  and mountain tops) snows; deserts (t > 0.6, h < -0.5) drop
  precipitation entirely (still overcast); everywhere else rains.
  In **winter**, the snow threshold rises to `-0.05` so taiga and
  cold-temperate snow seasonally; in summer it drops so only arctic
  snows.

### Multiplayer

The 1 Hz `TimeIre` broadcast becomes `WorldState { time, ire, day,
weather }` (protocol bump to 3). Guests render whatever the host
says; guest sim doesn't run the machine. Headless `--server` works
unchanged.

## Rendering (the lighting stack cashes in)

- **Sun dimming**: weather contributes a `sky_factor` uniform —
  Clear 1.0, Overcast 0.6, Precip 0.45, Storm 0.3 — multiplying the
  directional sun term and specular glint (ambient falls less, so
  shadows soften instead of the world going black). Smoothly lerped
  over ~10 s at transitions.
- **Sky + fog**: overcast desaturates the sky color toward gray;
  rain pulls `fog_dist` in ~35%; snowfall fogs white and closer.
- **Precipitation**: no particle system needed — a cylinder of
  camera-centered falling quads (~150) in the existing entity batch,
  streak texture for rain, flake for snow (two new builtin tiles).
  Each quad samples the world's sky-light at its column and skips
  where covered (`light_sky < 15` at head height = under a roof).
  Guests render purely from broadcast weather.
- **Lightning**: during Storm, rare rolls (~every 20–40 s) flash the
  sky: 2 frames of `daylight = 1.0` + a white sky tint, thunder sfx
  delayed 0.5–3 s by distance. No strike damage in v1 (there is
  nothing to burn yet — fire is not in scope).
- **Audio**: rain/storm ambience as procedurally generated noise
  loops through rodio (filtered white noise, thunder rumble), fading
  with weather transitions. First looped ambience in the engine;
  keep the mixer dumb (one ambience sink, crossfade on change).

## Seasons change the world

- **Foliage tint via atlas rebuild**: on season transitions, rebuild
  the atlas (already cheap and hot-reload-tested) applying a hue
  shift to grass/leaf tiles — spring vivid, summer deep, autumn
  amber, winter drab. Texture packs get the same shift applied over
  their tiles. No mesher or shader work.
- **Crops**: `random_tick` growth multiplier by season — spring
  1.25, summer 1.0, autumn 0.75, winter 0. Winter exception: a crop
  that is *not* sky-exposed and has block light ≥ 10 still grows at
  0.5 — **greenhouses emerge from existing rules** (roof + torches).
- **Berry/fruit bushes** fruit only in summer/autumn (stage advance
  gated like crops).
- **Wildlife**: breeding pauses in winter; repopulation rolls double
  in spring (birthing season), halve in winter. Nothing despawns —
  hunting in winter just competes with scarcity.
- **Water freezes**: in winter, sky-exposed still water (source
  blocks with air above) in sub-`SNOW_T`-plus-seasonal columns turns
  to ice on random ticks; ice thaws back to water in spring. Uses
  plain `set_block` — edit-logged, so guests see it. Ice already
  exists and drops nothing; walking on frozen lakes is free
  navigation with a spring deadline.

## Snow becomes a material (the block you can finally touch)

New content, all data-driven where possible:

- **`base:snow_layer`** — a thin ground dusting. New block field
  `height = 0.125` (mesher renders a shortened cube; neighbors don't
  cull against it; collision stays walk-through via `solid = false`).
  Shovel-harvest drops 1–2 **snowballs**; breaking bare-handed
  drops 1.
- **`base:snowball`** — item, stack 16, **throwable** (the
  projectile system already flies arrows): right-click throws, 0
  damage but knockback + hurt-flash — a toy with teeth (snowball
  fights in multiplayer; guests throw via the existing FireArrow
  path generalized to `ThrowItem`).
- **Crafting**: 4 snowballs → 1 `base:snow` block (the existing
  one), 3 snow blocks → 6 snow_layers (decor). Snow block
  shovel-mines into 4 snowballs (drops change from self).
- **Accumulation**: while it snows, the weather tick sprinkles
  `snow_layer` onto random sky-exposed solid surfaces in cold
  columns near players (a handful per second, server-side,
  edit-logged). **Melt**: random ticks remove snow_layers when the
  season warms or when block light ≥ 12 (torches clear paths —
  light as a shovel).
- Arctic/mountain worldgen keeps placing full snow blocks as today;
  accumulation only ever places layers, so builds don't get buried.

## Ire hooks (gentle, no new meters)

- Rain accelerates ire decay by +25% while falling — the wild
  breathes easier when the land drinks.
- Storms add +1 to the warden spawn budget at PROVOKED and above —
  dark skies are cover.
- That's all. Weather never *causes* ire.

## Engine work checklist

1. `world.day` + season fns + persistence (world.toml).
2. `Weather` enum + machine in `Server::step`, rng-rolled,
   persisted; `SimEvent::WeatherChanged` for presentation.
3. Protocol 3: `TimeIre` → `WorldState { time, ire, day, weather }`.
4. Uniform plumbing: `sky_factor` + weather sky/fog tints (lerped
   client-side).
5. Precipitation quads + two builtin tiles (`rain_streak`,
   `snow_flake`) + gemini prompts for the default pack.
6. Lightning flash + thunder + ambience loops (rodio noise synth).
7. Seasonal atlas tint pass (hue-shift grass/leaf slots on rebuild).
8. `random_tick` season gates (crops, bushes, freeze/thaw, snow
   melt) + weather tick accumulation.
9. Block `height` render field (snow_layer) in mesher.
10. `snow_layer`, `snowball` (+ throw), recipes, snow block drops
    change (add alias-safe data edits).
11. HUD calendar line + dev env hooks.

## Tests (headless, deterministic)

- Weather machine: seeded run produces only legal transitions;
  durations within bounds; storm probability rises with ire.
- Day counter increments on wrap and on sleep; season derives and
  persists through save/load.
- Winter: crop random_tick does not advance sky-exposed farmland;
  advances at half rate roofed+lit (greenhouse); resumes in spring.
- Freeze: sky-exposed cold source water becomes ice in winter ticks
  and thaws in spring; covered water never freezes.
- Snowfall tick places layers only on sky-exposed solids in cold
  columns; melt clears them under ≥12 block light.
- Snowball round-trip: craft 4→snow, snow mines to 4; thrown
  snowball knocks a mob back without damage.
- Protocol 3 round-trips; guest applies broadcast weather.
- Atlas seasonal rebuild changes grass tile pixels between summer
  and autumn (and pack tiles get tinted too).
- Screenshot passes: WILDFORGE_WEATHER=rain/storm/snow +
  WILDFORGE_SEASON for each season's foliage.

## Not in scope (v2 candidates)

Wind, leaf particles, lightning strikes/fire, humidity-driven local
fronts, rain barrels/irrigation, temperature survival (freezing
damage), aurora. Fire as an element deserves its own plan before
lightning can set anything alight.
