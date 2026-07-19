# Game Feel — juice the verbs, not the screen

Drafted and **IMPLEMENTED** 2026-07-19, all six stages plus the
stretch night bed. Notes vs. this spec: warden hit particles sample
the *mob's own body tile* instead of a species table (any mob —
including mod mobs — sheds its own colors for free, same trick as
block debris); the sim-purity guarantee is structural rather than a
hash test — every juice field lives on the client-only `Game`
struct, the particle pool and new sfx are unreachable from
`server::Server`/`World`, and the one world-touching feature
(snow footprints) is deliberately *not* juice: it's sim, host-owned,
covered by its own persistence/melt test, and runs identically with
the layer killed; item drop-pop easing is pure age-driven geometry
so it stays on under `WILDFORGE_JUICE=0` (it cannot diverge); the
press-dip rides a `UiBatch` flag so every button gets it without
threading state; and guests hear the pickup ramp through the `Give`
path since their drops never become local item entities.

Drafted from the research pass on the science of fun and
pleasing interfaces. The deep layers of Wildforge are already sound:
Self-Determination Theory is practically the game's thesis (autonomy
in the sandbox, competence in the tier processes, relatedness in the
camp), the ire system is a rare *autonomy-preserving* difficulty
dial, and native Rust clears Swink's sub-100ms control bar without
trying. What's thin is the **feedback skin** — the layer the
juiciness literature says carries perceived quality: animation
(~74% of juice in shipped games), particles (~21%), audio, and
persistence. This plan adds that skin.

**The governing constraint, from the research itself**: juiciness is
an inverted U — zero measurably hurts, and so does excess. Wildforge
has a quiet register; the wild is watchful, not a slot machine. So:
every effect answers a *player verb* (step, break, take, strike,
hit, get hit), effects never fire on their own, and screenshake is
all but banned (the corpus shows good games use it ~1% of the time;
we reserve it for taking damage, one nudge, two pixels). Rule of
thumb throughout: **the world confirms what you did, then shuts up.**

A second constraint: **presentation never touches the sim.** Every
system here is client-side; a headless server and a guest must run
byte-identical simulation with all of it stripped. Tests enforce
this where they can.

## The one new engine piece: a tiny particle pool

Everything below that isn't audio or UI rides one small system:

- `Game.particles: Vec<Particle { pos, vel, tile, uv_jitter, size,
  age, ttl, gravity, lum }>` — client-side only, capped at 512
  (oldest culled), ticked in `update` (velocity, gravity, ttl),
  emitted as camera-facing quads in the existing entity batch, the
  same way precipitation already works. No new renderer work.
- **Sub-tile UVs** are the trick that makes debris read as *the
  block*: a debris quad samples a random quarter-region of the
  broken block's own tile (classic Minecraft debris), so dirt
  crumbles brown and cobalt ore sprays blue — for free, from any
  block, including mod blocks and texture packs.
- Guests spawn the same particles locally from the events they
  already receive (`BlockSet`, their own swings, `Hit`); nothing new
  crosses the wire.

## Stage 1 — the ground speaks (footsteps)

Walking is the game's most common verb and it is currently silent.

- **Per-material footsteps**: track distance walked while
  `on_ground`; every ~2.2 blocks, play a step. Material comes from
  the block underfoot through the existing `break_mat` mapping
  (Stone / Wood / Soft / Leafy) plus two special cases: snow and
  snow layers (a muffled crunch), sand/gravel (a looser scuff).
  All synthesized in the existing rodio burst engine — no assets.
- **The variation rule** (applies to every repeated sfx in this
  plan): ±10% random pitch and slight length jitter per instance.
  Repetition without variation is how feedback becomes noise.
- **Landing dust**: falls of 2+ blocks puff 4–6 ground-colored
  particles at the feet (sub-tile UVs from the landing block) and
  play a heavier step. Falls that deal damage add a low thud.
- Remote players and mobs get footsteps too, distance-attenuated —
  hearing a deer before seeing it is informative audio (the
  research's strongest claim for sound: perceiving the world before
  seeing it), and hearing your friend walk up behind you is People
  Fun.

## Stage 2 — breaking and taking (the second verb)

- **Block-break burst**: 8–12 debris particles with the block's own
  sub-tile UVs, radial velocities, gravity, ~0.5s ttl. Mining
  progress gets 1–2 chips per crack stage so the channel feels like
  it's *doing* something before the payoff.
- **Drop pop**: item entities spawn at 0.6× scale and ease to full
  over ~150ms (ease-out back — a single overshoot). The same
  squash-and-stretch on bounce when they land.
- **Fly-to-hotbar**: on pickup, a UI-space ghost of the item icon
  animates from the item's screen-projected position to its
  inventory slot over ~220ms (ease-in quad), then the receiving
  slot **pulses** (brightness, one cycle, ~180ms). This is the
  research's competence/effectance loop in miniature: action →
  confirmation → location. Doherty is comfortably met (~220ms sits
  in the 100–400ms command band).
- **Pickup pitch ramp**: consecutive pickups within 1.5s raise the
  pickup chime's pitch a semitone-ish step, capping at +7 and
  resetting on the gap. Harvesting a wheat field becomes a small
  ascending melody — the classic collection ramp, and the cheapest
  competence signal in games.

## Stage 3 — UI motion and the HUD pass

The UI is immediate-mode, so motion is time-based, not retained:

- **Panel ease-in**: `set_screen` stamps an `open_age`; screens
  scale from 0.96 and fade from 0.85 over ~140ms, ease-out quad.
  One number, every screen, instantly less mechanical. (Perceived
  responsiveness research: animation this short *reads as faster*
  than an instant snap, not slower.)
- **Buttons**: hovered buttons grow 4% and brighten (exists partly);
  pressing dips them 2% for one frame — down-up is what makes a
  click feel mechanical.
- **Hotbar**: selection change bounces the selected slot's scale
  (1.0 → 1.12 → 1.0, ~120ms); the selected item name already
  toasts.
- **HUD readability pass** (aesthetic-usability effect: the pretty
  version literally *feels* easier to use):
  - one spacing grid (8px multiples) for hearts/hunger/hotbar/panel
    text; today's offsets are hand-tuned per element;
  - hearts wobble individually when health ≤ 3 hearts (danger
    reads peripherally, no numbers needed);
  - damage feedback becomes a brief red **vignette** (edges only)
    instead of the full-screen flash — readable, less blinding;
  - stat text in the inventory panel gets a soft dark backing strip
    (readability first — parse fast or nothing else matters);
  - the calendar/ire/max-health lines get one consistent type scale.

## Stage 4 — combat impact

- **Hand hitch, not hit-stop**: real hit-stop pauses the sim — a
  non-starter with an authoritative multiplayer tick. The viewmodel
  is client-side, so instead the swing **holds at its peak for
  ~60ms** when a melee hit connects, then completes. Same punch,
  zero sim contact, works identically for guests.
- **Per-warden hit particles**: thornlings shed bark chips, dryads
  leaves, emberkin sparks, gravelurks grit, wrathwood splinters —
  one table: `(species → tile, count)`, using existing tiles. Wildlife
  puffs a few soft tufts. Kills burst slightly bigger.
- **Taking damage**: the red vignette (stage 3) plus the plan's one
  and only camera shake — a 2px directional nudge *away from the
  attacker*, ~80ms. It doubles as information: the nudge points
  where the hit came from.
- **Anvil and quern get their promised sparks**: orange spark
  particles per hammer strike (the doc promised them; only the
  clang shipped), stone-dust puffs per quern turn, and the quern's
  top face renders as a **rotating overlay quad** during the grind
  channel — the persistence category's cheapest win.

## Stage 5 — the wild is audible (informative audio)

Sound that carries information the eyes don't have yet:

- **Wind before rain**: entering Overcast starts a soft wind bed in
  the existing ambience sink. Since every rain and storm passes
  through Overcast, wind *is* the forecast — the sky becomes
  legible by ear, no UI.
- **Warden presence**: a hostile within ~20 blocks that hasn't
  attacked yet emits a species-pitched rustle/creak every 6–10s
  (their `sound_pitch` already exists). The research framing:
  players who hear threats before seeing them report higher
  competence, not higher fear.
- **Hunger**: below 5 hunger, a quiet stomach note every ~20s.
  Below 2, slightly more insistent. No new UI.
- **Stretch — the night has a mood**: a sparse night bed (synth
  crickets: filtered impulse train) whose brightness falls as ire
  tier rises — a CALM night chirps, a WRATHFUL night goes quiet and
  low. This is the ire meter made diegetic; it ships last and cuts
  clean if the synth fights back.

## Stage 6 — marks left on the world (persistence)

The most underused juice category, and the one that suits a game
about consequence:

- **Footprints in snow**: walking through a `snow_layer` cell swaps
  it to `snow_layer_trod` — same block in every way, one new tile
  with pressed prints. It's a real block edit: edit-logged, guests
  see your trail, it persists across sessions, and it melts like
  any layer. A camp in winter accumulates paths — history written
  in the ground.
- Landing dust and quern rotation live in stages 1/4; they're
  persistence-adjacent and listed there.

## What this plan deliberately does not do

No screenshake beyond the damage nudge. No slow-motion, no zoom
punches, no floating damage numbers, no XP chimes — wrong register
for this game. No sim-side changes of any kind: a fired arrow, a
mined block, and a warden fight resolve identically with the
presentation layer deleted.

## Engine work checklist

1. Particle pool (struct, tick, cap, camera-facing emit with
   sub-tile UVs) + guest-side spawning from received events.
2. Footstep tracker (distance + material map + variation helper) —
   the pitch/length jitter helper is shared by every stage.
3. Sfx additions to the synth: step ×4 materials, snow crunch,
   thud, spark ring, quern grind, wind bed, stomach note, night bed
   (stretch). All procedural, no assets.
4. Item-entity spawn/land easing + UI ghost-fly list + slot pulse
   timers + pickup ramp state.
5. `open_age` screen easing + button press dip + hotbar bounce +
   HUD grid/vignette/wobble/backing strips.
6. Viewmodel hitch timer; hit-particle table; damage nudge.
7. Anvil/quern spark hooks + quern spin overlay.
8. `snow_layer_trod` block + tile + walk-through swap (+ gemini
   prompt; melt rules inherited by data).
9. Dev hooks: `WILDFORGE_JUICE=0` kills the whole layer (also the
   proof it's presentation-only), `WILDFORGE_DEMO_JUICE` stages a
   scene for screenshots.

## Tests

- Sim purity: a scripted sequence (mine, fight, pickup) produces an
  identical world hash with `WILDFORGE_JUICE=0` and 1 — presentation
  provably changes nothing.
- Particle pool: cap holds under a spawn flood; ttl culls; sub-tile
  UV math lands inside the source tile for every atlas slot.
- Footstep material mapping: each base surface resolves to the
  intended sfx class; snow layers report the crunch.
- Pickup ramp: pitch steps and caps and resets on the timing gap.
- Snow trod: swap happens on walk-through, persists through
  save/load, still melts by light and season, drops the same
  snowball (content-graph unaffected).
- Screenshot passes: HUD grid before/after, vignette at low health,
  a debris burst mid-frame, the winter trail.
- All existing suites green — especially the loopback, since guests
  now spawn local particles from the same messages.

## Sequencing

Stages are independent and each is a small commit; 1–2 first (the
two most common verbs), then 3 (UI), 4 (combat), 5 (audio), 6
(snow). The stretch night-bed ships last or not at all.
