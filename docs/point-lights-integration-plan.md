# Point lights in the game — integration plan

Drafted and **IMPLEMENTED** 2026-07-19, all eight stages. Notes vs.
this spec: the transmission pass tints by each pane texel's **hue at
full saturation** (alpha sets strength) rather than raw texture
color — pane interiors are mostly transparent, and raw alpha washed
the tint to white; this also means water gently blues a submerged
beam for free. Rimewisp got a cold-shimmer `glow` alongside emberkin
(cool lights skip the flame flicker by color). Emissive wardens and
the viewmodel hand deliberately keep the flat pre-shaded model. The
promotion/cache director lives in `src/lights.rs`; demo lights ride
it too (`Key::Demo`), so `DEMO_PTLIGHT`/`DEMO_CORNER` now cache like
everything else. `WILDFORGE_DEMO_TORCHROOM` stages the torch-lit
room *plus* the red-glazed alcove (stage 7's proof), built on the
footprint's highest ground with its chunks force-ensured — demo
staging that writes into ungenerated chunks silently loses blocks.

Original plan follows. Decisions settled with dollspace: **held
torches cast real shadows**, the default look is **stark**, and
**emberkin glow ships in this pass**.

## The governing rule: the flood-fill is the sim's truth

Every gameplay read of light — mob spawn checks, snow melt, crop and
greenhouse rules — goes through the flood-fill, and keeps doing so.
Point lights are presentation, same contract as the juice layer: a
headless server and a guest sim identically with the renderer
deleted. Consequences:

- **No sim-side exclusion.** The original M4 idea ("exclude promoted
  lights from the flood-fill") would change `light_at` and therefore
  gameplay. We don't do it.
- **Render-side suppression instead**: for each promoted light the
  chunk shader subtracts an *estimate* of its flood contribution
  from the rendered torch term: `est = max(0, 1 − d/range) · color`,
  clamped so the term never goes negative. In open line of sight the
  estimate cancels the flood glow and the punchy direct term takes
  over. Behind a wall it over-subtracts (the real flood wrapped the
  corner and is weaker than the straight-line estimate), which is
  exactly the goal: the shadow side falls toward ambient, keeping
  only a dim wrap that reads as bounce light. Hard shadows finally
  read in a real torch-lit room — and the sim never notices.
- A held torch does **not** suppress mob spawns. Placed light is
  safety; carried light is sight. (Classic Minecraft rule, and it
  keeps "being followed by something in the dark" possible.)

## The roster (who gets a real light)

No new block data: anything with `light_emit`/`light_color` is a
candidate, so mod blocks join automatically.

1. **Static emitters** — torches, lit bloomeries, lit kilns, burning
   furnaces, smoldering clamps. Promoted by proximity; cube maps
   cached (below).
2. **The held torch** — the flagship. Holding any item that places a
   light-emitting block (or any item with a new optional
   `glow = [r, g, b]` ItemDef field, e.g. a raw ember) carries a
   warm light with **real shadows**: your own body of light sweeping
   a cave as you turn is the point of the whole system. Anchored at
   the camera, offset slightly toward the viewmodel hand so the beam
   direction feels like it comes from the torch.
3. **Emberkin** (and any future `emissive` hostile) — a
   shadow-casting glow at the mob's position. Firelight sweeping
   around a corner announces the creature before you see it — the
   visual twin of the presence audio cue. Wildlife stays unlit.

## Budget, promotion, caching

- `MAX_PT_LIGHTS = 8` total: up to **5 static** + **up to 3
  dynamic** (1 held + 2 nearest glowing mobs).
- **Promotion**: each frame, gather candidate emitters within ~48
  blocks of the camera (from per-chunk emitter lists the mesher
  already walks — collect `(pos, id)` of emitting blocks during
  meshing and store them on the chunk). Score by
  `intensity / (1 + d)`; take the top N. **Hysteresis**: an
  incumbent keeps its slot unless a challenger beats its score by
  25%, so slots don't flicker at range boundaries.
- **Static cube-map cache** (this is what makes it affordable): each
  slot remembers `(light_pos, revision)`. A cached cube re-renders
  only when (a) the light is newly promoted, or (b) a chunk within
  the light's range re-uploads its mesh — chunk re-upload already
  happens for every visible edit, so invalidation needs no new
  plumbing in the world code. Steady-state cost of a torch-lit
  smithy: zero shadow passes per frame.
- **Dynamic lights** re-render their 6 faces when they move or when
  an in-range chunk changes. A held torch standing still is cached;
  walking costs 6 small range-culled passes — measured, not feared
  (the sun's 2048² pass is bigger than all six 512² faces).
- Fewer than 8 active lights = zeroed slots, shader loop already
  handles it.

## Look and feel

- **Stark by default.** Ambient floor low (≈0.04 at night); caves
  and shadowed interiors are genuinely dark, torches genuinely
  matter. The shader's hard `0.03` floor becomes the config value.
- **Settings** (settings screen, persisted in config.txt):
  - `DYNAMIC LIGHTS: OFF / ON / +SHADOWS` — OFF renders no point
    lights (flood-fill only, today's look), ON gives shadowless
    direct light (cheap GPUs), +SHADOWS is the full system
    (default).
  - `DARKNESS: STARK / SOFT` — the ambient floor pair
    (0.04 / 0.12). `WILDFORGE_AMBIENT` still overrides for tests.
- **Flame flicker**: ±8% intensity on flame-colored lights
  (torch/bloomery/kiln/emberkin), slow noise, not strobing; also
  gently jitters the suppression estimate so the pool breathes. Not
  gated by the juice flag — lighting is core rendering like the sun.
- Colors from `light_color` × an intensity per emit level (14 → ~1.8
  so a torch blares at arm's length and dies by ~16 blocks).

## Entities face the light

Entity vertices (items, mobs, players, falling blocks) currently
write zero normals, so `N·L` kills every point light on them — a
deer standing in the beam would render flat-dark against a lit wall.
Fix: the entity emitters already know their face orientation (cube
faces, model boxes) — write real normals. Particles keep zero
normals deliberately (dust motes shouldn't flash). This also gives
entities directional *sun* shading for free, which they've never
had — expect the world to look slightly better everywhere.

## Multiplayer

- Nothing crosses the wire for static lights or your own held light
  (each client promotes and renders locally from its own world copy;
  guests have the blocks already).
- Emberkin glow works on guests from the existing mob snapshots.
- **Remote players' held torches** (in scope): the `Players`
  snapshot carries each player's held item id (protocol bump).
  Guests derive a held light for every remote player by the same
  rules as their own (`places → light_emit`, `glow`). Seeing your
  friend's torchlight bobbing toward you through the trees is
  People Fun; it also hands us the data to later *render* the held
  item on their model.

## Stained glass tints the beam

The cube pass renders opaque geometry only, so glass correctly
doesn't *block* point light — but a torch behind a red pane should
throw a red pool. In scope, via a **transmission cube** per
shadow-casting light:

- A second cube array (Rgba16Float): RGB starts white and glass
  surfaces in range render with **multiplicative blending**
  (`dst × src` of the pane's filter color). Multiplication is
  commutative, so panes need no depth sorting.
- The opaque pass's scratch depth is kept and tested (no write), so
  glass behind a wall never tints.
- Alpha stores the **nearest glass distance** (Min blend). The main
  shader applies the tint only when that distance is closer than the
  fragment (`glass_d ≤ frag_d`), so a pane *beyond* the lit surface
  doesn't stain it. One pane between light and surface — the common
  alcove case — is exact; stacked panes tint as a unit, which voxel
  scenes can live with.
- Shadowless lights (DYNAMIC LIGHTS: ON tier) skip transmission too;
  the tier stays cheap.
- This is also the missing physics for the stage-5 glassworks light:
  the flood-fill already filters per channel through `light_filter`,
  so the two systems will finally agree on what color a window is.

## Explicitly future (not this plan)

- PCF/soft shadows, bloom, screen-space GI (per the original doc).
- Emissive *texture* regions (lava veins etc.).
- Rendering held items on remote player models (the snapshot data
  ships here; the model work is its own feature).

## Stages

1. **Emitter lists + promotion + static cache** — mesher collects
   per-chunk emitters; nearest-N with hysteresis; cube cache with
   chunk-upload invalidation; flood-suppression term in the shader.
   Prove: a room with 12 torches renders 0 shadow passes/frame at
   steady state and the shadows read.
2. **Held light** — camera-anchored shadow-casting light from the
   held item (`places → light_emit`, plus `ItemDef.glow`);
   move/edit-triggered re-render, cached when still.
3. **Emberkin glow** — 2 nearest emissive hostiles as dynamic
   casters; flicker for all flame lights.
4. **Entity normals** — real normals in item/mob/player emitters.
5. **Settings + stark default** — the two settings rows, ambient
   floor rewire, config persistence.
6. **Remote held lights** — protocol bump: held item id rides the
   `Players` snapshot; guest-side held lights for remote players
   (loopback-tested like every other message).
7. **Stained transmission** — the multiplicative transmission cube;
   torch-behind-red-pane demo.
8. **Docs & demos** — `WILDFORGE_DEMO_TORCHROOM` (cached smithy),
   update README lighting blurb; keep `DEMO_PTLIGHT`/`DEMO_CORNER`.

## Tests

- Promotion: nearest-N scoring and hysteresis (pure function, unit
  test with synthetic emitter sets).
- Cache: a static scene renders zero cube passes after warm-up; an
  edit within range invalidates exactly the lights that cover it
  (expose a pass counter for tests).
- Suppression math: estimate never exceeds the flood term's
  possible contribution; term clamps at zero.
- Sim purity: `light_at`/spawn/melt behavior identical with
  DYNAMIC LIGHTS OFF/ON/+SHADOWS (they never read renderer state).
- Screenshots: torch room hard shadows, held-torch cave sweep,
  emberkin glow around a corner, STARK vs SOFT floor comparison,
  red-pane tinted pool (with the shadow still hard).
- Loopback: held item id round-trips in the Players snapshot.
- Settings roundtrip in config.txt; existing suites green.
