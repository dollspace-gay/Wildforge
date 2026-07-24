# Economy — scarcity, capital, and the reasons to trade

Drafted 2026-07-23, **IMPLEMENTED** 2026-07-24, all six stages.
Notes vs. this spec, where the implementation knew better:

- **Pipe rarity is 1/90,000 chunks, not the drafted 1/6,000** — the
  draft's example number didn't match its own band. Poisson math
  (median nearest = √(ln2·N/π) chunks) says 1/6,000 lands at ~620
  blocks; hitting the 2,000–5,000 treasure band takes ~1/90,000
  (measured median 2,416, p90 3,872). The band was the intent; the
  example was wrong.
- **Batholith provinces**: raising the pluton noise threshold alone
  barely moved the census (the noise frequency keeps blobs
  everywhere). A coarse province gate over the same noise (knee
  0.44, slope 1.8) puts granite country at median 512 / p90 960.
  Tin also thins to chance-gated traces in plain stone (bootstrap,
  never supply) with vein country only along granite.
- **The kiln barn became the glassworks**: there is no ceramics
  chain yet, so the kiln's capital upgrade is the chimney itself —
  same three-course check as the forge, shared code. Rainproof,
  double fuel reach. A future pottery chain can ride the same shell.
- **Survey maps became survey cairns**: item stacks carry no
  arbitrary data, so the knowledge artifact lives in-world — a
  placeable cairn that costs a prospector's strike to raise (pick in
  pack, wears) and reads the land free for anyone thereafter.
  Readings are pure functions of seed and position, so cairns store
  nothing and sync nowhere.
- **Freshness rides the durability field** — `ItemStack::new`
  initializes it from the item def, the wear bar reads as freshness,
  and no save or protocol format changed. Legacy stacks (durability
  0 on a perishable) initialize fresh on first sweep instead of
  rotting.
- Along the way: bloomeries, kilns, and forges became right-click
  openable in singleplayer and for the host — previously only guests
  could reach those screens (via OpenContainer).

Drafted 2026-07-23. Decisions settled with dollspace: **no dedicated
classes** (specialization emerges from infrastructure, never from
locks on the player), **solo play stays fully viable** (a generalist
is valid, if a struggle — the nomad/settler principle scaled up),
**infrastructure-shaped gating is the mechanism** (a forge-shaped
building for metalwork, a kiln for ceramics — the bloomery multiblock
is the working precedent), and the north star: **give and take with
the environment, give and take with other players**.

The benchmark for the whole plan is one question: *would you ever pay
someone diamonds to mine gold?* In Minecraft the answer is never,
because you can go mine all the gold you want. The answer becomes yes
exactly when three things hold, and they are the three legs of this
plan:

1. the gold is genuinely far from you but not from them
   (**comparative advantage** — leg 1),
2. your hour at your forge out-earns your hour in a mine
   (**productive capital** — leg 2),
3. the buyer needs more steel next month because tools wear and food
   rots (**continuous demand** — leg 3).

## What already stands

- **Geologically honest ore hosts** (minerals & geology pass):
  diamonds only in kimberlite pipes, tin in granite, chromite in
  basalt, galena in marble/limestone, rare earths in monazite sand
  and carbonatite. The *mechanism* for regional scarcity exists.
- **The ire system**: taking from the wild has a cost, planting
  refunds it — the environmental give-and-take pillar.
- **Food scarcity** (PR #34): forage patches, hunt-out-able game,
  nomad valid / settler thriving.
- **Multiblock stations**: bloomery (validated structure, fired in
  real time, fears rain), anvils, kiln, quern, single-use cupellation
  crucibles. Steel is already a *process*.
- **No teleportation of any kind** — distance is real, so regional
  advantage is tradeable. This is load-bearing; guard it.
- **ATProto identity** — portable reputation across servers, the
  trust layer player economies die without.

## The census: geology is honest but too even (measured)

`print_resource_census` (ignored dev tool, seed 42, 30 land points on
a ~24 km spiral; distances in blocks to the nearest occurrence):

| resource                    | median | p90  | max  |
|-----------------------------|--------|------|------|
| pluton (tin, pitchblende)   | 64     | 224  | 256  |
| volcano (carbonatite, REE)  | 320    | 576  | 768  |
| kimberlite pipe (any)       | 128    | 208  | 240  |
| kimberlite pipe (breached)  | 176    | 336  | 592  |
| geode                       | 64     | 128  | 128  |
| desert (monazite sand)      | 768    | 1152 | 1152 |

Meanwhile shale (iron, coal), limestone (galena), stone and basalt
(copper, quartz) are **depth bands under every column** — available
everywhere by digging, gated by effort not geography.

Verdict: every settlement has everything within a short walk.
Comparative advantage cannot exist at these distances. The host-rock
*mechanism* is right; the *frequencies* are set for a single-player
showcase, not an economy.

### Target bands (leg 1's tuning goal)

Three tiers, by expected distance from a random settlement:

- **Commons** (copper, iron, coal, quartz, clay, stone): under every
  column, cost is depth and danger. Unchanged — everyone can
  bootstrap alone.
- **Regionals** (tin, galena/marble, volcanic goods, monazite,
  geodes): median 500–1500, p90 ~3000. Some settlements sit on them,
  most don't — the stuff of caravans and border towns.
- **Treasures** (kimberlite pipes, carbonatite, big pure plutons):
  median 2000–5000, *famous* — few enough that individual sites have
  names and reputations.

Concretely: pipes from 1/397 per chunk to ~1/6000 (one per ~40-chunk
radius region becomes one per multi-km journey), geodes 1/89 →
~1/700, pluton noise threshold up so intrusions are half as common
and more distinct, volcano odds thinned outside subduction arcs (arcs
stay rich — volcanic frontiers should feel volcanic). Retune, rerun
the census, commit the numbers into a test that pins the bands the
way the food census pins forage.

## Leg 1 — comparative advantage (geography + knowledge)

- **Retune frequencies** to the target bands above. This invalidates
  no saved chunks (generation-time only, existing chunks keep their
  contents); ongoing worlds simply find the new distribution in new
  land.
- **Prospecting** (the Vintage Story borrow, and the best of it): a
  prospector's pick + a reading. Start simple: strike a rock face and
  read a coarse density verdict for a sampled radius ("traces of
  tin", "rich galena country"). The geology is a pure function of
  seed and position — the data is already there, the tool only
  reveals it. This makes *knowledge itself tradeable*: a surveyor who
  has mapped the pipes of a region has something to sell without
  swinging a pick. Later: a chartable map item, so the knowledge is
  an artifact that changes hands, not a screenshot.
- **Guard**: recipes are never secret and no resource is *absent*
  from any world — only far. Scarcity of inputs, never of knowing.

## Leg 2 — productive capital (workshops out-produce hands)

The rule for every chain: **the primitive path always exists; the
capital path out-throughputs it.** Nobody is locked out; the forge
owner is simply *faster and cheaper at scale*, which is what makes a
smith worth trading with, which is what makes specialization emerge
without classes.

- **The forge** (metalwork): a building, not a block — hearth,
  chimney that must reach open sky, bellows, anvil inside; validated
  like the bloomery. The campfire-and-stone-anvil path still smelts
  and smiths one piece at a time; the forge batches, saves fuel, and
  is the only place for the *big* work (steel tools in quantity,
  later machine parts).
- **The kiln barn** (ceramics/glass): the existing kiln block becomes
  the heart of a validated chamber; batch firing (already the model:
  one colorant, whole batch) scales with the chamber. Pit-firing
  stays for the lone potter.
- **The glassworks**: batch glass from the same pattern (demo scene
  already exists; make it real).
- Throughput knobs, in order of preference: batch size, fuel per
  unit, process time. *Quality tiers only if a chain truly needs
  them* — quantity differences trade well and are easy to reason
  about; quality differences fragment stacks.
- Multiblock validation stays the bloomery way: check the shape at
  the mouth when lit, degrade gracefully (a broken chimney = a cold
  forge, never an explosion).

## Leg 3 — continuous demand (sinks)

- **Food spoilage + preservation** (the big one): raw food carries a
  freshness clock; cellars (cool, dark, stone-enclosed — the climate
  and light systems already know), salting, smoking, and pickling
  extend it; preserved food is the trade good. Solo-feel guard:
  generous timers, cheap preservation — a settled solo player barely
  notices; what dies is the infinite stockpile. Farmers stay
  employed forever, and salt regions and clay regions earn a trade
  identity.
- **Tool wear already exists** — keep repair costing real metal, so
  steel demand never saturates.
- **Fuel**: charcoal already burns; workshops burning fuel per batch
  makes coal country matter (its seams are commons by depth, but
  *surface* coal country still out-competes).
- **Non-sink**: no structure decay, no upkeep tax on buildings.
  Punishing permanence fights the north star. Demand comes from
  doing, never from owning.

## Currency

None minted. Silver from cupellation is scarce, portable, divisible,
and useless enough for tools that it will get picked as money if the
three legs stand — that emergence is the proof the economy works. A
mint (stamped coinage as a convenience) is future work, added only if
players are already trading ingots by weight.

## Non-goals

- No classes, no skill trees, no recipe locks.
- No region-locked knowledge; only region-priced inputs.
- No teleportation, ever (also listed above; it bears repeating).
- No structure upkeep or decay.
- Nothing that makes solo play *require* another player.

## Stages

1. **Geology retune + census pin**: frequencies to the target bands;
   `print_resource_census` numbers land in a real test the way the
   food census pins forage.
2. **The forge**: multiblock validation, batch smelting/smithing,
   fuel economics. The pattern-setter for workshops.
3. **Prospecting pick + readings**: coarse regional survey verdicts
   from real generator data.
4. **Food spoilage + cellars + one preservation chain** (salting
   first — it ties to geology via salt; pickling/smoking follow).
5. **Kiln barn + glassworks** on the forge's validation pattern.
6. **Chartable survey maps** (knowledge as artifact) + observe:
   do players start pricing things in silver on multiplayer servers?
   Only then consider a mint.

Each stage lands independently and improves solo play on its own;
none requires another player to function. Cooperation is always the
*faster* path, never the only one.
