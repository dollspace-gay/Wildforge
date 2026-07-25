# Mechanization — from the water wheel to the machine-tool age

Drafted 2026-07-24, rewritten 2026-07-24 after design review with
dollspace. Decisions settled: **realistic tech progression** — the
ladder runs water power → millwork → the lathe → boring → steam →
electricity, in that order because that is the order in which each
made the next possible; **machine tools are the heart of the tree**
(the IC2/GregTech lineage is the anti-model for its magic boxes, the
model for its earned chains); **few, honest parts** — five machined
components, each gating something physically true, never component
bloat; **multiblocks look like the machine**, via shape meshes and
validation dressing, not controller blocks; and the standing economy
rule holds on every rung: **the primitive path always exists; the
powered path out-produces it.**

The design spine is the true story of the industrial revolution:
**precision begets precision.** A machine tool is the only kind of
tool that can make a better copy of itself. Watt's steam engine was
unbuildable until Wilkinson's mill could bore a true cylinder; every
precise thing on Earth descends from a handful of hand-scraped
surface plates and one good leadscrew. That loop — crude machine,
better part, better machine — is the progression mechanic. No
arbitrary tiers; your shop gets better because you can see the chain
of parts that made it better.

## Rung 0 — power sources (unchanged from the first draft)

Two, both site-bound; geography keeps mattering:

- **The water wheel**: turns only on LIVE water — a cell where the
  drop rule can fire (a weir lip, a spring race, a player-built
  channel). Standing pools turn nothing. Terraced rivers put a rock
  weir at every reach step, so every natural fall is a mill site and
  dams are real estate. Validation rechecks the water each firing.
- **The windmill**: sails wanting altitude (y ≥ ~90) and open sky,
  with wind strength from weather — Clear 0.6, Rain 1.0, Storm 1.4.
  The only machine that loves what the kiln fears. Highlands without
  rivers get their own power identity.

Power is a boolean with a rate, never a number with a cable. What a
wheel drives is decided by SHAFTS (rung 1), not adjacency lists.

## Rung 1 — millwork (power learns to travel)

The first machined... no — the first CARPENTERED parts: wooden shaft
and gear, cut at the sawmill. Millwork is what a millwright did for
two thousand years before anyone owned a lathe.

- **Shaft blocks**: render as actual axles (shape mesh), run
  horizontally or vertically up to ~12 blocks from a wheel, turn
  visibly when live. **Gear blocks** turn a corner or cross a
  vertical run. Wooden millwork is lossy: past ~12 blocks the run
  refuses (friction is real; bearings fix this later).
- **Powered stations** hang off the shaft line, each the capital
  sibling of a hand process:
  - **Millstone** (quern's sibling): bulk grinding, 16 a firing,
    unattended.
  - **Sawmill** (the axe's sibling): logs → planks at 6 per log
    (hand: 4), and the log → **beam** cut for future building work —
    and for shafts and gears themselves (the bootstrap: the first
    sawmill's parts are hand-whittled at the crafting grid, slower
    and wasteful; its own products build the second one cheaply).
  - **Helve hammer** (the smith's arm): auto-works anvil jobs at
    half player speed and zero attention. The forge compounds; the
    smith's hour moves to finishing work — exactly the
    specialization the economy wants.
- **Trip hammer, drop forge, and the like are NOT separate
  machines** — they are what the helve hammer reads as. One station,
  honest name.

## Rung 2 — the machining age (the heart of the tree)

### The crude lathe

Wooden bed, forged tool rest, shaft-driven. It turns wood and soft
metal (copper, bronze, tin) at SLOPPY tolerance — and that is
enough, because the first thing you cut is a **leadscrew**.

### The precision loop

The leadscrew is the mechanic: a lathe WITH a leadscrew cuts true
threads — better screws than any screw in the lathe itself. That
unlocks the **iron lathe** (iron bed, leadscrew feed), which turns
iron and steel at TRUE tolerance. Two machines, one loop, and the
player has lived the Maudslay story instead of reading a tier
number. Tolerance is a property of the MACHINE (crude/true), not a
stat on parts — parts made on the wrong machine simply can't be
made, the recipe wants the better lathe.

### The boring mill

The iron lathe's sibling for holes instead of spindles: it bores the
**cylinder**, the one part that gates everything pressurized — the
pump (water up a mine shaft: deep mining's drainage problem becomes
real and solvable), and the steam engine. Wilkinson before Watt,
always.

### The five parts (the whole roster, pinned)

Component bloat is the failure mode of the genre; five parts, each
gating something physically true, is the guard. Mods can add more;
base never does without a fight:

| part | made on | gates |
|---|---|---|
| **screw** | crude lathe | the vice, the press, the leadscrew itself |
| **shaft (iron)** | crude→iron lathe | bearings' partner; steam and generator internals |
| **bearing** | iron lathe | long shaft runs (millwork loses its 12-block limit), fast machines |
| **gear (iron)** | iron lathe | speed conversion under real load; clockwork later, maybe |
| **bored cylinder** | boring mill | pumps, steam engines |

Plus **plate** (helve hammer, not a machine tool — hammered, like
real plate) for boilers and tanks.

## Rung 3 — steam (power leaves the river)

The payoff of the whole machining age: cylinder + plate boiler +
machined shaft = the **steam engine**, a multiblock with a firebox
that eats coal and drives a shaft line like a wheel does — but sits
ANYWHERE. Water and wind stay the cheap path (fuel-free, site-bound);
steam is capital-intensive, coal-hungry, and free of geography. Coal
country becomes power country — a new regional identity for the
trade map, and the first time the economy's commons (coal by depth)
becomes a trade good by USE rather than rarity. Boilers want water
(the pump earns its keep feeding them).

## Rung 4 — electricity (finally honest)

The rare-earth thread lands where history put it — after machining,
because a generator IS a machined object: lathe-turned shaft,
bearings, wound coils. The separator (kiln-pattern) splits the mixed
rare-earth powder into neodymium (magnets, forged) and cerium (glass
polish — the sink that keeps it honest); magnets + machined parts +
copper windings = the **generator**, driven by wheel, wind, or
steam; first loads are **lamps** (the experimental colored lamps
become real — light without torches, colored light as luxury) and
the **electric quern** (power where geography and coal both said
no). Wires-as-infrastructure, batteries, and uranium stay a future
rung; they will want their own design pass.

## Multiblocks that look like machines

Two mechanisms, both already proven in the codebase:

- **Shape meshes** (the obelisk/rack system): machine structural
  blocks get real silhouettes — the lathe bed is a long bed with
  ways, the flywheel is a WHEEL, shafts are axles that visually
  connect, the steam engine's beam rocks over its cylinder. No
  generic-cube assemblages.
- **Validation dressing** (the bloomery-lit system): mouth-block
  swaps on state already exist; machines extend this — when a
  multiblock validates, its parts swap to fused/dressed variants,
  and when it runs, to running variants (the wheel turns, the
  hammer lifts). An unassembled pile reads as parts; a valid machine
  SNAPS TOGETHER visually. Screenshot gates apply to every one of
  these: a machine that reads as a pile fails review (the smoker
  precedent — it got legs).

## What stays out (the anti-model, recorded)

- No machine GUIs with slots and progress arrows: stations stay
  hand-loaded and readable (anvil, smoker, rack precedent).
- No energy units, cable loss, or storage blocks in this arc.
- No machine tiers by color or material reskin; capability changes
  by MECHANISM (crude vs true lathe), never by palette.
- No ore-doubling boxes; yield advantages stay physical (the
  glassworks' draft, the forge's batch) and modest.
- Nothing that makes solo play require another player; every machine
  is one settler's patient project, faster with friends.

## Why this compounds the economy

Every rung deepens all three legs: mill sites and coal country are
REGIONAL (leg 1 — and the prospector's pick learns "strong fall
here" / "coal country" lines for free); machines are CAPITAL that
out-produce hands and now out-produce each other up the precision
loop (leg 2); machines consume — fuel, wear, replacement parts —
and the five components are themselves trade goods a machinist
sells to people who own no lathe (leg 3). The stall's price
template already handles "three bearings for an iron ingot."

## Touchpoints

- New blocks: wheel, sail, shaft, gear, millstone, sawmill, helve
  hammer, lathe bed (crude/iron), boring mill, steam firebox/boiler,
  separator mouth, generator, lamps — all with shape meshes and
  dressed variants.
- machines.rs: live-water/wind predicates; shaft-run tracing
  (bounded walk, no power graph); per-machine validation on the
  has_chimney model; dressing swaps via swap_block_keep_entity.
- machine_tick.rs: firing loops on the forge model; steam eats coal
  per firing; pumps move water via the finite-water rails.
- Registry: parts as plain items; recipes gate on station kind
  (crude vs iron lathe as different interaction ids).
- Worldgen: nothing new — coal, iron, copper already live where
  they live. The pick gains two reading lines.
- No protocol changes expected (stations ride existing container
  kinds or hand-load like the smoker).

## Tests

- A wheel on still water refuses; a shaft run of 12 works wooden,
  13 refuses until a bearing joins it; gears turn corners.
- The crude lathe refuses iron; leadscrew unlocks the iron lathe
  recipe; the iron lathe cuts all five parts; the boring mill alone
  cuts cylinders.
- Steam runs anywhere with coal and water, stops starved of either.
- Powered stations out-produce their hand siblings per unit of
  attention (assert the multiple).
- Generator lights a lamp only while its shaft turns.
- Content-graph completeness: every part reachable, cerium sinks.
- Screenshot gates: every machine, assembled and running, judged as
  a machine and not a pile.

## Stages

1. Millwork: wheel + sail + shafts/gears + millstone, sawmill,
   helve hammer (a self-contained "mills update," shippable alone).
2. The crude lathe + screw + the vice/press it unlocks.
3. The precision loop: leadscrew → iron lathe → shaft, bearing,
   gear; bearings free the shaft runs.
4. The boring mill + pump (mine drainage as the demo problem).
5. Steam: firebox, boiler, engine; coal country wakes up.
6. Electricity: separator, magnets, generator, lamps, electric
   quern (the rare-earth thread, finally honest).

Each stage lands independently and improves solo play by itself;
the loop from 2-3 is the arc's soul and should feel like the
telling of a true story, because it is one.
