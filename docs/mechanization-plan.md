# Mechanization — the tech tree's first turning wheel

Drafted 2026-07-24. Decisions settled with dollspace: this is the
tech tree the minerals update was "precursor" to. Power stays LOCAL
in rung one (a wheel powers the building it's part of — no shafts,
no gearboxes, no power networks yet); the rare-earth thread runs
straight to magnets, which is what those ores are for; uranium
stays a far-future note.

The economy rule extends up the tree unchanged: **the primitive
path always exists; the powered path out-produces it.** A hand
quern still grinds. A mill town simply grinds more, while its
miller sleeps — which is what makes a mill worth building, and a
miller worth paying.

## Power sources (rung one)

Two, both site-bound — geography keeps mattering:

- **The water wheel**: a wheel block placed against falling water.
  "Live water" has a real definition in our sim: a water cell whose
  drop rule can fire — water adjacent to air with room below (a
  weir lip, a spring channel, a player-built race). Standing pools
  don't turn wheels. The beautiful accident: our terraced rivers
  put a rock weir at every reach step, so **every natural waterfall
  lip is a mill site** — dams are real estate now. Validation
  rechecks the water each firing (a diverted race stops the mill).
- **The windmill**: a sail block needing open sky and altitude
  (y ≥ ~90) with a swept 5×5×2 clearance. Wind strength derives
  from weather: Clear 0.6, Rain 1.0, Storm 1.4 (a storm is the
  ONLY weather that helps a machine — mills love what kilns fear).
  Highlands without rivers get their own power identity.

A powered station validates like every workshop: the station block
within 3 of its wheel/sail, structure intact, source live. Power is
a boolean with a rate multiplier — no rotation simulation, no
energy storage. Batches per firing scale with the source rate.

## Powered stations (rung one)

Each is the capital sibling of an existing hand process:

- **Millstone** (quern's sibling): bulk grinding. Charge up to 16
  grindables (grain, ores to powders, rare-earth concentrate); a
  firing processes the lot unattended. Hand quern: one at a time,
  standing there.
- **Sawmill** (the axe's sibling): logs → planks at 6 per log
  (hand: 4), plus a slow log→beam cut for a future building-blocks
  pass. First powered station most players build.
- **Helve hammer** (the smith's arm): auto-works anvil jobs —
  blooms → bars without standing and hammering, at half player
  speed but zero attention. The forge owner's throughput edge
  compounds; the smith's hour is freed for finishing work (tools),
  which is exactly the specialization the economy wants.

## The rare-earth thread (rung two)

The minerals update shipped monazite and bastnasite grinding to ONE
mixed powder, separation "deferred to the tech tree." This is the
tech: rare earths are the magnet metals.

- **The separator**: a kiln-pattern multiblock (firebrick stack +
  chimney) that roasts mixed rare-earth powder in quartz crucibles:
  batch in, `neodymium_powder` + `cerium_powder` out (the crucible
  is consumed — cupellation's rule). Cerium wants a use so it isn't
  slag: glass polish (a kiln additive that upgrades glass batch
  yield) keeps it honest.
- **The magnet**: neodymium powder + iron ingots, forged (the forge
  gets its first exclusive recipe — the primitive path exception,
  noted and deliberate: magnets NEED the controlled heat; there is
  no campfire path to a magnet, the way there is no hand path to a
  perfect lens. The guard we keep: everything a magnet unlocks has
  a non-electric alternative path, slower).
- **The generator**: magnets + copper ingots (coils) + a powered
  wheel = electricity for the building it sits in. First loads:
  **lamps** (the experimental colored lamps become craftable and
  real — light without torches, colored light as a luxury good),
  and the **electric quern** (grinds without water or wind — power
  where geography gave you none, at generator prices).
- Wires, batteries, machines beyond lighting: rung three, not
  planned here. **Uranium**: stays glowglass and menace until rung
  three earns it.

## Why this compounds the economy

Every rung deepens all three legs: mill sites are REGIONAL (leg 1 —
weir lips and high hills are now claims worth surveying, and the
prospecting pick learns to note "strong fall here" for free);
mills are CAPITAL that out-produce hands (leg 2); and machines burn
nothing but replace labor, freeing hours that go somewhere — while
magnets create brand-new demand for copper, iron, and the
rare-earth trade the geology already prices (leg 3).

## Touchpoints

- New blocks: water wheel, windmill sail, millstone, sawmill, helve
  hammer, separator mouth (kiln-pattern), generator, real lamps.
- machines.rs: live-water check (the drop-rule predicate, read-only),
  wind check, per-station validation on the has_chimney model.
- machine_tick.rs: firing loops on the forge model (batch at
  completion, rate from source strength).
- Registry: grindable/sawable tables; forge-exclusive recipe flag.
- Prospect reading gains a "strong fall" line (pure, cheap).
- No protocol changes expected (stations ride container kinds like
  the forge did).

## Tests

- A wheel beside a weir lip validates; the same wheel beside still
  water refuses; damming the race mid-firing stops the mill.
- Windmill rate follows weather; storm beats clear.
- Millstone/sawmill/hammer batch outputs and out-produce their hand
  siblings per unit of player attention (assert the multiple).
- Separator splits the mixed powder; magnet needs the forge;
  generator lights a lamp only while its wheel turns.
- Content-graph stays complete (cerium has a sink).

## Stages

1. Live-water + wind predicates, the water wheel and sail.
2. Millstone + sawmill (the everyday mills).
3. Helve hammer (the smith's arm).
4. Separator + cerium polish (rare earths split).
5. Magnet + generator + lamps + electric quern (rung two closes).

Order matters: 1-2 are a self-contained "mills update" worth
shipping alone; 3 completes rung one; 4-5 are their own beat with
their own reveal (the first colored electric light in a window is
the screenshot).
