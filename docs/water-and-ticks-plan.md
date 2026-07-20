# Living water, living world — finite fluids and the tick budget

Drafted 2026-07-19. Decisions settled with dollspace: **everything
finite — the oceans too** (no infinite water anywhere), **water
cycle this pass** (evaporation + rain refill), **reconcile on load**
(the world lives while you're away), **bucket this pass**.

The state this plan fixes: water is Minecraft-classic — eternal
source blocks project flow levels 1–7 through `desired_flow`, so
volume is never conserved and a breached pond never empties. Random
ticks sample 8 blocks per loaded chunk per half-second, so cost
scales with view distance and player spread, and unloaded chunks are
frozen — leave for three days and return to an unfrozen lake in
midwinter. What's already right and stays: the event-driven
`water_queue` + budget architecture (settled water costs zero), the
fixed 30 Hz sim, `set_block` waking neighbors, worldgen being the
only source of water today (there is no bucket yet, so no legacy
duplication behavior to preserve).

## Finite water: volume, not projection

Keep the existing 8-block water chain and the wake-queue exactly as
they are; change what a level *means*. Level `l` (0–7) now holds
**volume `v = 8 − l`** — level 0 is a full cell (8 units), level 7
a thin film (1 unit). Worldgen already places level 0, so it needs
no change; `water_height` already maps levels to render heights.

Rules on wake (replacing `desired_flow`):

1. **Fall**: move as much volume as fits into the cell below (air or
   partial water). Falling is greedy and first.
2. **Equalize**: among strictly-lower horizontal neighbors (air
   counts as volume 0 over a solid floor), move units toward the
   lowest until within **2 units** of it — the hysteresis kills
   oscillation and guarantees the queue quiesces. Moved-to and
   moved-from cells wake their neighbors.
3. Volume moves; it is never created or destroyed. A big lake drains
   layer by layer as a creeping front from the breach, bounded by
   the existing 512-cells-per-200 ms budget — big bodies take real
   time to drain, which reads as right.

**No pressure in v1** (recorded limitation): a sealed U-tube will
not push water up the far side; equalization travels along the
surface only. Future-work line, not a blocker.

## The ocean is finite too

No special sea block, no infinite sources, no migration — the ocean
is just a very large amount of ordinary water. Scoop a bucket from
the sea and the hole refills from neighbors because the sea is
*big*, not because it cheats; the total actually dropped by one
bucket. Breach the seafloor into a cave and the cave genuinely
fills from the ocean's volume, the drain front creeping outward
under the queue budget until the cave is full — a permanent breach
saturates the budget until equilibrium, which is the throttle
working as designed, not a leak.

Two honest boundary conditions, recorded:

- **Generation is the horizon.** Newly generated ocean chunks
  arrive with worldgen sea. The ocean is finite *within generated
  terrain*; an infinite world cannot promise more, and draining
  "the ocean" means draining what exists. This is the accepted
  fiction and it costs nothing.
- **Flow defers at ungenerated borders.** `set_block` into an
  ungenerated chunk silently drops writes (known gotcha), so a cell
  may only fall/equalize into *loaded* chunks — border cells go
  back to sleep un-flowed, and chunk load wakes water along its
  incoming faces so deferred flow resumes. Water never vanishes
  into the void off the edge of the generated world.

## Random ticks: constant cost, one mechanism

Give each chunk a persisted `last_random` stamp (absolute sim-time:
`day as f64 * DAY_LENGTH + time_of_day * DAY_LENGTH`, passed into
the tick — the Server owns the clock). Each random pass visits a
**fixed K chunks** (cursor over the loaded set, oldest stamp first)
and gives each a burst of `clamp(elapsed / 0.5 × 8, 8, 256)`
samples — each sample is exactly one classical random tick, so no
rule changes anywhere. Cost per pass is O(K) regardless of view
distance; a chunk that waited longer gets a proportionally bigger
burst — the same code path serves "far corner of the loaded set"
and "just loaded after an hour away". K tuned so the expected
per-block rate matches today's (~K=64 at 0.5 s cadence).

Stamps persist in a sidecar (`stamps.bin`, postcard map of
`ChunkPos → f64`, written by `save_modified`); chunk files stay
untouched and old saves read as stamp-missing (= no elapsed time,
clean first load).

## Reconcile on load: the world lived while you were gone

Bursts are statistically honest up to minutes; for longer absences a
chunk-load reconcile applies **phase rules wholesale** in one sweep:

- It's winter and this exposed cold-climate water is full → it's
  frozen *now*. Spring in a warm climate → ice is water again.
- Snow layers melt if the elapsed span crossed into warmth.
- Crops jump stages by `Poisson(E)` where E integrates the per-block
  tick rate × `crop_chance` × seasonal multiplier over the elapsed
  days, per season crossed. Light-gated cases (greenhouse) evaluate
  against current state — an accepted approximation.
- Evaporation/rain are *not* reconciled (the cycle nets roughly
  zero over a season); recorded as intentional.

The sweep is one pass over the chunk, capped, and runs where chunks
already pay worldgen/disk costs — no load hitch.

## The water cycle: the world breathes

- **Evaporation** (random-tick rule): an exposed **shallow** water
  cell — depth ≤ 2, i.e. solid within two cells below — under open
  sky, warm climate, summer sun loses one unit at low probability.
  The depth guard keeps the ocean and deep lakes off the stove:
  they lose only rim shallows, not their bodies, and it spares the
  wake-queue an eternally churning sea surface. A pond bed never
  dries below a 1-unit film when it sits in a depression (≥3 of 4
  horizontal neighbors solid at its y) — marshy film, never
  nothing, so rains can find it again. Open spills on flat ground
  evaporate fully.
- **Rain refill**: the existing precipitation sprinkle (the
  snow-settle rider in `server.rs`) also serves rain columns: if
  the column surface is a partial water cell or film, add one unit
  and wake it.
- Winter freeze/spring thaw already exist; the freeze rule updates
  to "full cells freeze" (partial rims stay liquid; melt returns a
  full water cell).

Ponds shrink through a dry summer, come back with autumn rain,
freeze over in winter.

## The bucket: finite water you can carry

Items `base:bucket` (craft: iron, the existing recipe shapes) and
`base:bucket_water`. Use on water with the empty bucket scoops the
whole cell (cell → AIR, neighbors wake, item swaps to the full
bucket); use the full bucket pours a full finite cell and swaps
back. Both resolve through the existing block-edit + inventory
paths, so multiplayer rides the current edit protocol — **no
protocol bump**. `held:u16` already shows the carried bucket on
remote models.

## Touchpoints

- `world.rs`: tick_water rules, wake unchanged plus border-face
  wake on chunk load; random_tick signature gains `now`/K-cursor;
  reconcile fn on chunk load; stamps sidecar in save/load.
- `registry.rs`: bucket items + recipe (no new water blocks).
- `worldgen.rs`: unchanged (sea stays `base:water` level 0).
- `server.rs`: passes `now` into random_tick; rain-refill arm in
  the sprinkle block.
- `main.rs`: bucket use paths (scoop/pour) in the interact code.
- `mesher.rs`/`physics.rs`: already read `water_height`/`is_water`;
  verify partials render and swim sensibly, adjust thresholds only
  if needed.

## Tests

- Conservation: seal a basin, pour N units, tick to quiescence —
  total volume exactly N; queue empties (no oscillation).
- Equalization: two joined columns level out within the hysteresis
  band.
- Breach: a pond drains only by what actually left; level drops;
  an ocean-fed breach fills the cavity and stops at equilibrium
  with total volume conserved.
- Borders: water at the edge of generated terrain defers (no volume
  lost to ungenerated space); generating the neighbor chunk wakes
  the border and flow resumes.
- Ticks: cost per pass is O(K) not O(chunks) (instrumented count);
  burst scales with elapsed; stamp roundtrips through the sidecar.
- Reconcile: a chunk stamped last season loads frozen in winter /
  thawed in spring; crops advance ~E stages over a simulated
  absence.
- Cycle: summer sun shrinks a shallow pond to a film but not past
  it; a deep column does not evaporate below the depth guard; rain
  adds volume back; a flat-ground spill dries fully.
- Bucket: scoop/pour roundtrip conserves; scooping the sea refills
  the hole from neighbors while total volume drops by exactly one
  cell; loopback MP sees both edits.
- Screenshots: half-drained pond behind a breached bank; a winter
  lake frozen on arrival; bucket in hand.

## Stages

1. **Finite cells** — volume semantics + fall/equalize rules,
   border-deferral + border wake on load, freeze-rule update,
   conservation tests.
2. **Constant-cost random ticks** — K-cursor, stamps, elapsed
   bursts, sidecar persistence.
3. **Reconcile on load** — wholesale phase pass + Poisson crops.
4. **The water cycle** — evaporation with the depth guard + rain
   refill.
5. **The bucket** — items, recipe, scoop/pour, MP loopback.
6. **Docs & screenshots** — README water note, demo shots, full
   gate green.
