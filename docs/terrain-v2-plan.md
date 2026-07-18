# Terrain v2 — "Caves & Cliffs"-Style Generation

Design for replacing the 2D-heightmap generator with a modern
density-function pipeline: 3D terrain with overhangs, spline-shaped
geography, multi-noise biomes, layered noise caves, and slope-aware
surface rules.

## Status: IMPLEMENTED (2026-07-17) — all five systems + tests + tuning.

## Decisions (agreed 2026-07-17)

- **World height: 256** (sea level stays 64; build/terrain ceiling 256,
  bedrock floor at 0). Roughly: oceans/valleys 40–64, lowlands 64–90,
  hills 90–130, mountains 130–200, extreme peaks to ~230.
- **No save compatibility.** Nobody but us is playing: chunk format bumps
  to `WFC3` (same RLE, 65 536 cells), old saves are ignored/regenerated,
  and the v1 legacy loader is deleted. `saves/` gets wiped once.
- Everything remains **deterministic per seed** and chunk-local (no
  cross-chunk writes during generation) so parallel generation stays
  possible later.
- Mods are unaffected: registry, ore `y_range`, and features keep their
  semantics (copper's range should be retuned upward once mountains exist).

## Architecture overview

`Generator::generate(chunk)` becomes a five-stage pipeline:

```
1. shape:    density field  -> stone/air/water        (3D, lattice-sampled)
2. carve:    cheese + spaghetti caves                 (3D noise)
3. surface:  per-column top-down scan                 (biome + slope rules)
4. features: ores (registry), trees/cacti (biome)     (unchanged pass)
5. bedrock:  floor seal                               (y = 0)
```

A per-chunk `heightmap[16][16]` (highest solid y) is produced by stage 3
and cached for stages 4, spawn search, and the fog/HUD — `height(x, z)`
as a pure function disappears; the exact surface is emergent.

---

## 1. 3D density terrain

For each point: solid ⇔ `density(x, y, z) > 0`.

```
density(x,y,z) = base_3d(x,y,z) * amplitude          // the "wiggle"
               + bias(y, offset, factor)             // the height gradient
```

- `base_3d`: 3 octaves of Perlin sampled at (x/171, y/128, z/171)-ish
  frequencies, amplitude ~1.0. This term creates overhangs: wherever it
  locally beats the bias gradient, stone protrudes or hollows.
- `bias(y) = (offset - y) * factor_above` for `y > offset`, and
  `(offset - y) * factor_below` for `y < offset` — pulls terrain toward
  the spline-provided `offset` height. `factor` is the vertical "squish":
  small factor ⇒ wild vertical variation (cliffs, shelves), large factor
  ⇒ flat terrain hugging `offset`.
- Water fill: after shaping, any air cell with `y <= 64` becomes water
  (arctic surface layer becomes ice as today).

**Lattice interpolation (the perf trick).** Density is evaluated only on
a coarse lattice — every 4 blocks horizontally, 8 vertically:
5 × 33 × 5 = 825 evaluations per chunk (vs 65 536 naive) — then
trilinearly interpolated per block. This is both the classic Notch
optimization and the source of the pleasant "smoothed diagonal" look.
Spline/biome inputs (2D) are evaluated at the 5 × 5 lattice columns and
shared down the column.

**Testing:** determinism; an overhang exists near a known seed/coord
(air cell with solid above it in a natural chunk); water fills to 64;
no floating single blocks epidemic (sanity count under threshold).

---

## 2. Spline-shaped geography

Three new low-frequency 2D noise fields (seeded offsets of the world
seed, frequency ~1/600–1/1200 blocks):

- **continentalness** `C` — ocean ↔ inland axis
- **erosion** `E` — how worn-down / flat the land is
- **ridges** `R` — folded `1 - |2*|r|-1|` "peaks & valleys" shaping

Two hand-authored splines (piecewise-linear, monotone control points —
a `Spline(Vec<(f32, f32)>)` helper with binary-search eval):

```
offset(C, E, R):    deep ocean floor (~40) → coast (~64) → lowlands (~70)
                    → hills (~95) → mountains (E low: up to ~190,
                    boosted by R toward ridge lines)
factor(E):          high erosion → 8.0 (flat plains/plateaus)
                    low erosion  → 1.6 (dramatic vertical freedom)
```

Starting control tables live in code next to the spline type and are the
main tuning surface; expect iteration against screenshots. `R` near its
folded zero-line subtracts from `offset` in low-erosion areas — that's
the classic ridge/valley carving and a future river hook.

**Testing:** spline eval unit tests (endpoints, midpoints, monotonic
segments); statistical: sampling 10k columns, extreme-low-E regions
average significantly higher max-height than high-E regions; oceans
exist (some region with offset < 60).

---

## 3. Multi-noise biomes

Biome selection becomes nearest-centroid matching in a 5D parameter
space — `(temperature, humidity, C, E, R)` — replacing the 2D threshold
tree. Each biome declares a centroid; the closest one (weighted
euclidean; C weighted ~1.5×) wins. Same inputs drive terrain shape and
biome choice, so placement is automatically coherent (peaks land where
the offset spline made mountains).

| Biome | T | H | C | E | notes |
|---|---|---|---|---|---|
| Ocean* | – | – | very low | – | emergent: offset < sea level |
| Plains | 0.1 | -0.2 | 0.3 | 0.6 | flat, sparse trees |
| Forest | 0.1 | 0.4 | 0.3 | 0.2 | current look |
| Jungle | 0.7 | 0.7 | 0.3 | 0.3 | dense giant canopy |
| Desert | 0.8 | -0.7 | 0.3 | 0.4 | sand + cacti |
| Scrubland | 0.5 | -0.3 | 0.2 | 0.5 | patchy sand/grass |
| Taiga | -0.5 | 0.2 | 0.3 | 0.3 | conifers |
| Arctic | -0.8 | 0.0 | 0.3 | 0.4 | snow + frozen ocean |
| **Mountains** (new) | -0.1 | 0.0 | 0.5 | **-0.7** | exposed stone, snow caps |

*Ocean is not a centroid: columns whose offset is below sea level render
as today's water world regardless of climate (frozen in arctic).

Additions: a **Mountains** biome (stone surface, no trees, altitude snow)
and altitude rules that override any biome (see §5). The `Biome` enum,
`biome(x, z)` API, HUD title, and tests keep working — classification
internals change, the seven existing biomes remain.

**Testing:** all biomes reachable within a search radius across a few
seeds; mountains correlate with height (mean heightmap under Mountains >
under Plains); determinism.

---

## 4. Layered noise caves

Two independent systems, both carving stage-2 stone (never through
bedrock; flooded below y 64 → water? no — carved cells below sea level
stay stone unless already open to water, to avoid instant ocean floods;
carving is capped at `density > small_margin` cells):

- **Cheese** (exists today, retuned): large 3D noise `> 0.6` voids,
  biased to grow with depth (`threshold - depth*k`) so big chambers live
  deep and rarely breach the surface.
- **Spaghetti** (new): carve where **two** independent 3D noises are both
  near zero: `|n1| < w && |n2| < w`, with width `w ≈ 0.06` widening
  slightly with depth. The intersection of two implicit surfaces is a
  1D curve — long, winding, branching tunnels. Frequencies ~1/60 blocks.
- **Entrances:** spaghetti width tapers toward the surface instead of
  hard-stopping, so some tunnels daylight naturally on cliffsides.

Out of scope: aquifers (local water tables), lava (no lava block yet —
candidate for a future fluid + light-source milestone), ravine carvers.

**Testing:** caves exist below y 50 in a sampled region; a spaghetti
tunnel is traversable-ish (connected air run of ≥ 12 blocks along its
curve); no cave carves bedrock; determinism.

---

## 5. Slope-aware surface rules

After carving, a per-column top-down scan applies surface sets:

```
depth 0 (top solid):        biome top    (grass / sand / snow)
depth 1..=3:                biome under  (dirt / sand / dirt)
deeper:                     stone
underwater floor (top solid below sea level): sand shallow, gravel deep
```

Two modifiers:

- **Steepness:** estimate slope from the cached heightmap gradient
  (|Δh| ≥ 3 across a block) or exposed side faces at the column top —
  steep columns skip the topsoil set entirely and show **bare stone**.
  This is what makes 3D cliffs read as rock faces instead of
  grass-wrapped blobs.
- **Altitude:** above y ≈ 170 any biome's top becomes snow (snow caps);
  above y ≈ 150 in Mountains, tops are stone/snow mix via detail noise.
  Trees refuse to plant on stone/snow surfaces (existing behavior —
  they only plant on grass).

Because the surface pass runs after carving, cave ceilings/floors stay
stone naturally, and overhang undersides are stone (no upside-down
grass).

**Testing:** steep-face columns expose stone; high-altitude tops are
snow; underwater floors are sand/gravel; cave interiors have no dirt
crowns.

---

## Save format & migration

- `CHUNK_Y = 256`, cells per chunk 65 536, magic `WFC3`.
- Loader accepts only `WFC3`; anything else → regenerate chunk. Delete
  the v1 fixed-palette table and the v2 branch (dead code once saves are
  wiped). The per-world string palette mechanism is unchanged.
- One-time: `rm -rf saves/*` locally; note in README.

## Performance budget

- Density: 825 lattice evals × ~6 noise octaves ≈ 5k noise calls/chunk —
  well under a millisecond. Spaghetti/cheese add 2–3 3D noises per
  *block* in stone; if profiling shows pain, sample caves on the same
  lattice at 4×4×4 and interpolate the carve fields too.
- Chunk memory doubles (128 KB blocks); meshing scans double — both fine
  at view distance 12. GEN_BUDGET may drop from 10 to 6 chunks/frame if
  hitching appears.

## Implementation order

1. `WFC3` + `CHUNK_Y = 256` + delete legacy loaders (mechanical).
2. Splines + C/E/R noises + density shaping (flat `factor` first, then
   tune splines against screenshots).
3. Surface-rule pass + heightmap cache (replaces `height()` callers:
   spawn search, trees, HUD).
4. Cave retune + spaghetti + entrances.
5. Multi-noise biome selection + Mountains biome + altitude snow.
6. Tune pass: seeds, spline tables, cave widths; screenshot gallery.

Each step compiles and runs on its own; tests land with their step.
