# Task Brief: Mass-Driven Power Draw

**Repo:** WildForge, branch `feat/multiblock-recognition`
**Primary files:** `src/world/power.rs` (existing boolean-rate shaft
power), `src/world/rail.rs` (`tick_rail_motion` consumes `RailState.speed`),
`src/world/local_structure.rs` (the train = a `LocalStructure` riding
rails), `src/registry.rs` (`MaterialVector` on `BlockDef`/`ItemDef`),
plus a new `src/world/power_draw.rs` and a minimal belt module.
**Related design doc:** `wildforge-engine-extensions-spec.md` §2.2
("Power draw"). This is the mass-driven power-draw item explicitly
deferred by Phase 6 and Phase 8's roadmaps.

## Status Going In

- **WildForge's power is a boolean with a rate, never a number with a
  cable** (`src/world/power.rs`). `power_at_pos(pos) -> f32` walks
  shaft/gear blocks (bounded search, `WOODEN_RUN`/`POWER_VISITS`) back
  to a live source and returns a *rate*: water wheel `1.0`, windmill
  sail `0.35..=1.6`, steam `1.4`. Stations accumulate
  `station_work += dt * rate` and strike when work crosses
  `STATION_STRIKE_SECS` (2.0). The rate is a throughput multiplier; a
  millstone draws identically whether it's grinding one grain or
  sixteen. There is **no load concept and no metering anywhere**.
- **Rail motion exists and consumes a plain speed** (`src/world/rail.rs`).
  `RailState.speed` is cells/second; `tick_rail_motion` advances
  `progress` by `speed × dt`, snaps the transform at cell boundaries,
  parks at dead ends. The `speed` field is set by tests today and has a
  doc comment explicitly reserving it for a mass-driven power-draw
  phase. Rail pieces classify via `RailKind` (Straight/Curve/Incline/
  Switch) — the natural source of an incline multiplier.
- **A train is a `LocalStructure`** (Phase 5/6/8): `blocks:
  HashMap<(i32,i32,i32), BlockId>`, `block_entities`, `outbox`. Its
  mass is computable by summing its actual placed blocks — the same
  "fold over real content" shape as `fold_stats` (Phase 2.1).
- **Mass data does not exist yet.** `BlockDef.materials` is a
  `MaterialVector = BTreeMap<String, u64>` but it is declared on only
  **5 of 249** blocks (choirstone, still_salt, wake_iron, echo_slate,
  + copper), and there is **no material→weight table** anywhere. A
  mass formula derived purely from declared `materials` gives nearly
  every block zero mass. This is the single biggest data/design gap in
  the brief and is addressed in 9a.
- **Reference game (`dev/belts/`)**: a full Bevy-ECS wattage network —
  poles with radius + `wire_capacity`, generators, consumers
  (`demand_watts`, `priority`, `allocation_ratio`), batteries,
  spanning-tree flow, priority load-shedding, belt speed ramping toward
  `speed × allocation_ratio`. Notably belts spawn with `demand_watts =
  0` (the network exists but belts draw nothing today), and train fuel
  burn is full physics (rolling + grade + drag forces → fuel power).
  Per spec §2.2 and design intent, **we do not replicate the ECS
  network, spanning-tree, battery, or continuous-wattage machinery** —
  those are the computationally expensive fine details deliberately out
  of scope. We keep only the quantized-load-tier concept.

## Goal

Give rail-riding structures (trains) and belt segments a mass-driven
power **draw** that meets WildForge's cheap boolean-rate model halfway:
mass is quantized into load tiers each tick; each tier maps to a fixed
draw rate; an incline multiplier scales draw on uphill track; and the
delivered shaft rate vs. the required draw decides the structure's
speed tier (with Overloaded a real stall). No continuous wattage
accounting, no wire network — a pure per-tick lookup that stays cheap
over arbitrarily long rail lines.

## In Scope

### 9a — Mass: material weights + a structure/belt mass query

- Add a **material-weight catalog** (registry or data-driven):
  `material → mass per unit`. This is new data; `MaterialVector` today
  has no weights. Anchor values to the existing material names used in
  `blocks.toml`/`items.toml` (copper, tin, iron, wood, stone, ...).
- **Block mass** = Σ over `materials` of `material_mass[name] × count`.
- **Fallback for the 244 undeclared blocks**: a default mass derived
  deterministically (e.g. from `material_class` via
  `inferred_material_class`, or a flat per-block default) so a train
  built from ordinary blocks is never free. Decide the rule explicitly;
  do not silently give un-declared blocks zero mass unless that's the
  intended design.
- **Structure mass** (`LocalStructure::mass()`) = Σ of its placed
  blocks' masses, resolved through `self.reg` against the catalog.
  Lazy/cheap (O(blocks), cached on the structure and invalidated on
  `set_block` if profiling demands — start with per-tick recompute and
  only cache if it shows up).
- **Belt load mass** = mass of the items currently riding that belt
  segment (9d), using the same catalog.

### 9b — Load tiers and draw rate

A new `src/world/power_draw.rs`:

```rust
pub enum LoadTier { Empty, Light, Medium, Heavy, Overloaded }

pub fn load_tier_for_mass(mass: f32) -> LoadTier; // threshold table
pub fn load_tier_rate(tier: LoadTier) -> f32;      // fixed draw per tier
```

- `load_tier_for_mass`: monotone thresholds (data, not code — a const
  table). Thresholds chosen so a bare empty car sits at Empty/Light and
  a full freight train crosses into Heavy/Overloaded.
- `load_tier_rate`: each tier → a fixed rate **in the same units as
  `power_at_pos`** (the boolean-rate f32, ~0.35..=1.6). This keeps the
  comparison `delivered >= draw` meaningful with zero unit conversion.
- **Draw** per the spec: `draw = incline_multiplier(track) ×
  load_tier_rate(tier)`. `incline_multiplier` comes from `RailKind`:
  Straight/Curve = 1.0, Incline = >1.0 (uphill only — downhill is a
  draw *credit* or plain 1.0; decide, and note the ramp is North-only
  today per `rail.rs`). Belt segments use their own base multiplier
  (straight belt = 1.0).
- Belt-ready by construction: the tier machinery is a pure lookup over
  `(mass, incline_multiplier)`; belt and rail call the same functions.

### 9c — Rail power draw

Wire the draw into `tick_rail_motion` (rail.rs), not the station tick:

- Each tick, for each structure with `rail: Some(_)`:
  1. `mass = structure.mass()` → `tier = load_tier_for_mass(mass)`.
  2. `draw = incline_multiplier(current piece) × load_tier_rate(tier)`.
  3. `delivered = self.power_at_pos(structure.anchor)` — the boolean
     rate actually arriving at the train's current cell (a wheel on the
     line, a sail, steam, or a generator field).
  4. **Speed response = tiered steps** (decision): the structure's
     effective speed is a per-tier fraction of `rail.speed`, and a
     shortfall lowers the effective tier:
     `effective_speed = rail.speed × tier_speed_fraction(effective_tier)`,
     where `effective_tier = tier` when `delivered >= draw`, else the
     next-lower tier (empty = 0), and `Overloaded → stall` (`speed = 0`,
     parked like a dead end — reuse the existing park path so behavior
     stays consistent).
- Overloaded as a real failure state: match the reference game's
  stall-and-wait posture — a stalled train resumes next tick if power
  returns; it does not derail or despawn. (Spec: "Overloaded can be a
  real failure state (jam/stall/visible strain) rather than just a cost
  tier." We take the stall half; "visible strain" is a later renderer
  concern.)
- `power_at_pos` is called per moving structure per tick — O(structures)
  × O(search bound). Flag, don't optimize preemptively, at this phase's
  scale; note the option of a per-source rate cache if it ever matters.

### 9d — Minimal belt segments (decision: rail + minimal belts)

A minimal belt so the draw has a second consumer and the mod-facing
shape (belts carrying items at a power-scaled rate) is real:

- A belt block (straight only this phase), recognized like a rail piece
  via `RailKind`-style classification (`BeltKind`).
- Per-belt item transport: a small queue of item stacks riding the
  belt, advanced per tick at `effective_speed × dt`, emitted at the
  far end via the existing loose-item path (`spawn_loose_item` /
  `push_drop_at` — do not invent a new item entity system).
- Belt load = Σ masses of riding items (9a). Belt draw = `1.0 ×
  load_tier_rate(tier)` (no incline in this phase). Delivered power
  resolves the same way a station does (`power_at_pos` / a belt-adjacent
  rule mirroring how stations attach to shaft lines — confirm which,
  see Open Questions). Tiered speed steps apply exactly as 9c.
- Back-pressure: a full exit (loose item cap at the mouth, like the
  reference game's `MAX_LOOSE_ITEMS_AT_END`) stalls the belt and it
  draws nothing while blocked — matching the reference drill/belt
  "set demand 0 when backed up" precedent, and cheap.

## Acceptance Criteria

- Mass: a block with declared materials computes `Σ weight×count`;
  a block with none computes the chosen fallback (never zero by
  accident); `LocalStructure::mass()` sums its blocks correctly
  (verify against a 1-cell and a multi-cell structure).
- Tiers: `load_tier_for_mass` is monotone and crosses each threshold;
  `load_tier_rate` returns the fixed per-tier values; `draw` obeys
  `incline_multiplier × tier_rate` for straight vs. incline rail.
- Rail: an empty car on a powered line moves at full speed; a loaded
  car whose `draw > delivered` runs at the next-lower tier's fraction;
  an Overloaded train stalls (speed 0, no panic, parks on its cell) and
  resumes when power returns. Verify at a few mass/power combinations.
- Belt: items ride a straight belt and are emitted at the end via the
  loose-item path; belt speed follows its tier; a full mouth stalls and
  drops demand to zero (no power consumed while blocked).
- Regression: full `cargo test --lib` (currently 831 passing), clippy
  `-D warnings`, `cargo fmt --check`.
- New unit tests: mass computation; tier thresholds; draw formula;
  rail power-gated speed (under/over-powered, overload stall + resume);
  belt transport + stall/no-draw-while-blocked.

## Non-Goals (do not implement in this task)

- **The reference game's continuous-wattage network**: no poles, wires,
  batteries, generators-as-entities, spanning-tree flow, or per-consumer
  priority load-shedding. The boolean-rate shaft model stays; we only
  add a *draw* against it.
- **Curves/inclines/switch variants for belts** — straight segments
  only; the tier machinery is built to extend, but geometry variety is
  a later belt phase.
- **Block-built car slot loadouts / cargo inventory riding a moving
  structure** — mass comes from the structure's own placed blocks; a
  future cargo system can feed the same `mass()`.
- **Rendering/visual strain** for Overloaded ("visible strain" is
  deferred to the renderer).
- **Fuel burn / energy accounting** (the reference game's
  `COAL_ENERGY`/joules physics) — explicitly rejected per spec §2.2.
- **Station (millstone/forge/etc.) load scaling** — the existing
  station tick keeps its boolean-rate behavior; this phase is trains
  and belts only.

## Open Questions to Resolve Before/During Implementation

- **Material weights**: where does the catalog live (const table in
  `power_draw.rs` vs. data-driven `materials.toml`)? What are concrete
  per-material values? This is new data with no existing anchor — pick
  values that make the Empty..Overloaded thresholds meaningful for
  realistic car sizes.
- **Fallback mass for undeclared blocks**: flat default per block, or
  derived from `material_class`/`inferred_material_class`? A flat
  default is simplest and defensible; class-derived is slightly more
  gameful. Choose deliberately.
- **Belt power connection**: how does a belt segment resolve delivered
  power? Options: (a) `power_at_pos` on the belt block (treat the belt
  like a station in the shaft walk), (b) a belt-adjacent source rule,
  (c) belts are consumer-only and need a generator *nearby* (the
  `generator_near_at`/`ELEC_RADIUS` precedent). (a) is the most
  consistent with existing stations — confirm before wiring.
- **Downhill incline**: does descending track reduce draw (a credit)
  or just cost 1.0? The ramp is North-only today; simplest is 1.0 both
  ways with only the climb costing more — confirm.
- **Belt item queue capacity & emission**: one stack per cell? A capped
  queue with an emitter that spawns loose items only when the mouth is
  clear? The reference `MAX_LOOSE_ITEMS_AT_END` precedent is a good
  starting point — pick a number and state it.

## Suggested PR Description Framing

> Adds mass-driven power draw for rail-riding structures and minimal
> belt segments on top of WildForge's existing boolean-rate shaft
> power, per spec §2.2. Mass is computed by summing a structure's
> actual placed blocks against a new material-weight catalog (with a
> deterministic fallback for blocks that declare no materials), then
> quantized each tick into load tiers (Empty/Light/Medium/Heavy/
> Overloaded) mapped to fixed draw rates. Draw = incline_multiplier ×
> tier_rate; a structure runs at a per-tier speed fraction while
> `delivered (power_at_pos) >= draw`, steps down under a shortfall, and
> stalls on Overload, resuming when power returns. Includes minimal
> straight belt segments carrying items at the same power-scaled rate,
> with back-pressure stalling drawing nothing. Explicitly does not
> introduce a continuous-wattage network (poles/wires/batteries/
> spanning-tree) — the boolean-rate model is unchanged; we only meter
> load against it. [Per design doc link.]

## Roadmap After This

- **Belts v2**: curves/inclines/switches, belt tier materials, and a
  real placement UI.
- **Cargo riding a moving structure** — feeds the existing `mass()`
  hook and makes freight-heavy vs. empty trains matter economically.
- **Block-built car loadouts / slot frames** (spec Part 1.3) — the
  engine slot eventually *drives* `rail.speed` from fuel rather than
  `speed` being set by tests.
- **Renderer strain cues** for Overloaded, once the renderer gets
  per-structure state hooks.
