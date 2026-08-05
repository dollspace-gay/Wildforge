# Workings — magic changes processes, not material truth

> **Status: implemented and production-qualified (2026-08-03).**
>
> This is goal 6 of `docs/magic-sequence.md`. It requires goals 1–5 and the
> qualified physics, water, ecology, material, fire, machine, persistence, and
> multiplayer systems.

## Implementation record — 2026-08-03

Goal 6 is live through one declarative twelve-entry registry: eight wand
workings (Trace, Gleam, Kindle, Nudge, Rootwake, Draw, Fieldmend, and
Holdfast) and four constructed rituals (Settling Rite, Rooting Bed, Ward
Boundary, and Transfer Circle). Each shell selects one closed native handler;
registration rejects unknown handlers, cross-domain capabilities, missing
physical obligations, raw mutation, and forbidden or unbounded effects.

Every operation uses the same host-owned linked transaction. It records stable
actor and apparatus identity, exact versioned targets, Current custody,
physical debits, a closed typed effect, wear, strain, dross, path, timing, and
interruption disposition. Start reserves before mutation, competing source or
target use rejects cleanly, completion is write-ahead and exact-once, and
cancel, interrupt, disconnect, death, unload, save/load, process restart, and
crash replay settle deterministically. Held wand channels interrupt at restart
or when the actor leaves line of sight/range; embodied continue-unloaded
rituals resume from their durable stable id and authoritative schedule.

The base handlers retain their physical truth:

- Trace requires a real fitted tuning lens and reports only bounded local
  drift, leakage, wakes, and recent working evidence. Gleam holds a temporary
  dynamic light and never places a light voxel.
- Kindle delegates ignition, fuel, spread, provenance, and liability to the
  ordinary fire system. Nudge changes only ordinary velocity on one host-owned
  projectile/drop or the persistent draft state of one embodied firebox; it
  neither teleports nor edits terrain.
- Rootwake advances one declared biological interval only after climate,
  season, living-heart, light, space, water, soil, nutrient, and species
  checks, and pays the real water/nutrient budget. Draw moves one exact parcel
  with its salt, thermal load, dross, and fixed-point remainder and refuses
  overflow atomically.
- Fieldmend consumes matching finite material, caps restoration at original
  quality, leaves ordinary residue, and checkpoints the player profile before
  final settlement. Holdfast quarters real age or leakage while continuously
  supplied; it never reverses age, revives an expired heart seed, or replaces
  bulk preservation.
- Transfer Circle conservatively relocates adjacent mounted charge. Rooting
  Bed reserves and advances a bounded physical bed even while unloaded.
  Settling Rite slows one adjacent process and routes rather than deletes its
  dross into a wearing real vessel. Ward Boundary validates a closed player
  construction and spends supply against projectiles, wardens, wakes, and
  dross; pressure and Ire scale cost, broken segments leak, overload is
  deterministic, and neither stored Ire nor ordinary players/animals/actions
  are suppressed.

Local play, windowed hosting, dedicated hosts, guests, and agents all dispatch
through the same authority methods. The wire carries intent and stable ids,
not client-authored costs or mutation state; agents can aim, start, hold,
release, and cancel and perceive ordinary working, projectile, and loose-item
cues. Remote presentation replicates type, source, target/containment path,
completion, and warning band. Restrained source/target/path particles, dynamic
Gleam light, target outlines, qualitative catalogue tooltips, discordant sound
with visual warning equivalents, and no gameplay camera displacement cover
the presentation/accessibility contract.

Qualification covers 41 focused Workings and contract tests plus the shared
arcane, material, water, ecology, fire, machine, persistence, protocol,
multiplayer, renderer, and agent suites. After fetching current `origin/main`
(already an ancestor of this branch), the final repository gate passed 696
non-agent tests with 22 intentional operator probes ignored and seven serial
end-to-end QUIC agent scenarios in 22.01 seconds, including a dedicated-host
aim/start/hold/release/cancel Working lifecycle. Formatting, strict Clippy,
the advisory audit, and optimized Linux and Windows release builds also pass.
A native Windows DX12 capture on an RTX 3090 exercised a physically assembled
wand and active authoritative Gleam at 1280x720: 138/138 resident chunks were
uploaded, the capture reported 53 FPS, 7.64 ms simulation and 9.19 ms draw
time, and no dirty chunks.

## Purpose

This goal gives players useful magic while enforcing the constitutional rule:
magic changes state, rate, location, or relationship; it does not create
ordinary matter or abolish physical obligations.

It adds a deliberately small base roster that proves sensing, ignition,
motion, cultivation, repair, preservation, stabilization, and warding. New
workings—including modded ones—must use the same atomic effect contract.

## Delivery modes

### Wand working

- Fast, local, flexible, and relatively inefficient.
- Draws from the wand buffer and optionally measured local Ambient Current.
- Produces more strain/dross per unit of effect.
- Limited by focus, line of sight, range, safe throughput, and player action.

### Constructed ritual

- A validated arrangement of blocks, vessels, conductors, focus objects, and
  physical targets.
- Slow, visible, interruptible, and more efficient.
- Can affect a larger bounded target only because its material structure and
  charge source support it.
- Remains in the world as infrastructure others can inspect, regulate,
  sabotage, repair, or inherit.

### Passive binding

- Charms and later bound tools react to narrow conditions.
- Effects are modest and debit charge only while doing real work.
- No passive binding grants an unconditional permanent stat.

There is no incantation hotbar, character spell list, or player mana pool.

## Effect transaction

All workings build one server-authoritative linked transaction:

```text
WorkingTransaction
  working/content id and version
  actor and apparatus
  exact targets and expected versions
  Current/charge debits
  Active Current reservation
  material, item, fluid, soil, food, heat, and durability debits
  permitted world/entity state changes
  completion/refund/dross distribution
  attribution and trace
  duration and interruption policy
```

Validation happens before commitment. Multi-tick workings reserve inputs so
another action cannot spend them twice. Completion commits exactly once.
Interruption follows a declared refund/dross rule. Save/load resumes or
settles an active transaction deterministically.

An effect handler receives capabilities only for its declared mutation class.
A sensing handler cannot edit blocks. A plant-growth handler cannot spawn
items. A water-transfer handler must specify equal source debit and destination
credit. Scripts cannot obtain a general `set_block` capability from magic.

## Cost and strain

Each working has:

```text
minimum charge
charge per magnitude/distance/duration
resonance preference
safe throughput
base conversion to dross
physical costs
apparatus/focus wear
range, volume, and target limits
```

Actual dross rises predictably with:

- resonance mismatch,
- poor component stability,
- transfer above safe throughput,
- low local capacity,
- drawing a cell below its safe floor,
- damaged/contaminated apparatus,
- interruption,
- intentionally forced overdraw.

There is no hidden critical-failure die. Instruments, sound, motion, and
visible strain communicate approach to each threshold. Continuing to force an
overdraw is an informed action with deterministic consequences.

## Base wand workings

Names remain subject to final content review.

### Trace

An Echo working that strengthens the tuning lens temporarily:

- reveals weak drift, wakes, charge leakage, and recent working traces,
- improves uncertainty within normal instrument limits,
- does not expose global maps, exact historic actions, inventories, or ore
  through walls,
- reserves charge while active and returns/disorders it at completion.

This is the safest first working.

### Gleam

Maintains a small temporary point light at the wand tip or a visible physical
focus:

- creates no item or permanent light block,
- holds Current in the active effect while lit,
- ends immediately when released/depleted/out of range,
- produces a small dross remainder,
- never outperforms torches for permanent cheap illumination.

### Kindle

Raises an existing combustible target to ignition:

- target must pass the ordinary fire ignition rules,
- Current supplies only the initial heat,
- subsequent flame consumes ordinary fuel and keeps existing provenance,
- wet/noncombustible/unsupported targets refuse,
- player-started fire remains liable under the fire system.

It replaces a spark, not a fuel chain.

### Nudge

Applies a bounded impulse to one visible dropped item, projectile, lever-like
mechanism, or other explicitly supported movable target:

- cost scales with mass, impulse, and distance,
- range is short and line of sight mandatory,
- cannot move terrain blocks, containers, vehicles with occupants, or another
  player's inventory,
- is inefficient for cargo and cannot replace roads, cranes, boats, or belts,
- collision and authority use ordinary physics.

Its primary uses are retrieval, alignment, and emergency control.

### Rootwake

Attempts one accelerated biological growth interval:

- supports only declared plants/crops/saplings,
- checks temperature, season, light, space, soil, water, nutrients, and
  species state,
- atomically debits the same physical inputs ordinary growth would consume,
- advances bounded growth rather than spawning the mature result,
- fails without consuming physical inputs if the habitat cannot support it,
  though careless channeling can still create dross,
- cannot force reproduction past carrying capacity or create seed.

It borrows time, not biomass.

### Draw

Moves a small exact volume of water between visible nearby reservoirs:

- source loses precisely what destination gains,
- salinity, temperature, and dross load travel proportionally with fixed-point
  remainders,
- source must be legitimately scoopable/pumpable,
- no use treats ocean surface or rain as an infinite block,
- range and throughput remain below ordinary pumps and bulk channels,
- destination overflow returns/rejects atomically rather than deleting water.

It is a field expedient and precise laboratory tool, not infrastructure.

### Fieldmend

Reorganizes supplied matching repair material into one damaged portable item:

- requires the damaged item and recipe-declared scrap/part,
- obeys tool-tier and alloy identity,
- cannot change material identity or improve above original quality,
- has worse recovery/material efficiency than the proper workshop,
- produces ordinary residue plus dross,
- cannot repair structures or machines remotely.

It keeps an expedition moving without making a forge obsolete.

### Holdfast

Slows change in one carried or mounted fragile target:

- intended for heart seeds, charged botanical specimens, reactive samples,
  and emergency preservation,
- debits charge continuously,
- slows rather than reverses spoilage/discharge,
- ends on depletion and never resets elapsed age,
- is too expensive and small-scale to replace cellars, salt, smoking,
  pickling, or cold logistics.

## Base rituals

### Settling rite

A frame/vessel/conductor arrangement that:

- lowers throughput and dross production for one adjacent magical process,
- directs the produced dross into a real vessel,
- consumes stabilizer wear and charge,
- cannot cleanse the vessel or increase total output,
- visibly fails if the containment path is broken.

### Rooting bed

A prepared soil plot, water connection, focus posts, and charge source:

- applies Rootwake efficiently over a small bounded bed,
- reserves the bed's actual water and nutrient budget,
- advances unloaded crops through the same authoritative schedule,
- cannot exceed ecological carrying or reproductive rules,
- makes intensive magical horticulture possible but materially demanding.

### Ward boundary

A closed physically built boundary that resists specified supernatural
effects:

- protects only its validated interior and only while supplied,
- pressure from projectiles, wardens, wakes, or dross debits charge,
- leaks/fails at damaged segments and overloads predictably,
- does not lower Ire, stop heart withdrawal, prevent ordinary players or
  animals, or grant invulnerability,
- high Ire can make maintaining it ruinously expensive,
- does not affect the Wild's right to withhold natural recovery.

It creates infrastructure and policy questions, not a permanent monster-off
switch.

### Transfer circle

Moves charge among adjacent mounted items/vessels with lower loss than a wand.
It does not move ordinary items, people, or charge at planetary distance.

## Explicitly forbidden handlers

Base and mod validation reject magical effects whose semantic result is:

- create an ordinary block or item from no material debit,
- convert one material identity into another,
- copy a block/entity/inventory state,
- delete waste, dross, water, salt, or tracked matter,
- teleport an entity/item or open a portal,
- bypass collision with permanent flight or phasing,
- set crop maturity without biological debits,
- refill food/health/durability without declared inputs,
- set weather/rain/water storage rather than conservatively influence a
  supported process,
- reveal hidden server information,
- erase Ire, heart death, ownership history, or attribution.

Changing a block state is allowed only through a named domain operation:
ignite, advance crop, transfer fluid, operate mechanism, apply repair, and
similarly bounded APIs. There is no generic transmutation API.

Large-scale weather working is outside this arc. A later design may influence
fronts only through the conservative atmosphere/water model and must not
become a rain button.

## Technology complement

Qualification compares every working to the mundane alternative:

| Need | Magic | Technology |
|---|---|---|
| Light | Portable temporary Gleam | Cheap permanent torch/lamp |
| Ignition | Precise Kindle | Flint/ember/firebox |
| Motion | Short flexible Nudge | Crane/belt/cart/boat |
| Growth | Fast expensive Rootwake | Efficient farm/irrigation/fertilizer |
| Water | Precise small Draw | Pump/channel/aqueduct |
| Repair | Wasteful field Fieldmend | Efficient forge/workshop |
| Preservation | One fragile Holdfast | Cellar/salt/smoke/cold chain |
| Safety | Supplied ward/settling rite | Walls, distance, procedure, machinery |

If a magical option becomes the cheapest reliable bulk route, balance has
violated the design even if its ledger technically closes.

## Interaction, presentation, and accessibility

- Holding use lets the wand settle/charge; release commits or cancels under
  the working's interruption rule.
- Target outline communicates the exact supported target.
- Source, target, and containment paths have distinct restrained particles.
- Sound rises in discord with strain and has non-audio visual equivalents.
- Dangerous thresholds use persistent world cues, not only transient text.
- Remote players see working type, source object, target path, and completion.
- Camera shake is restrained and presentation-only.

## Mods and content

Workings are declarative shells around approved handlers:

```toml
[[working]]
id = "example:gentle_kindling"
handler = "ignite"
focus = "ember"
charge = 14
dross = 2
range = 5
target = ["combustible"]
```

Mods may configure approved handlers and compose bounded predicates. A new
handler requires native/host-reviewed capability code or a future sandbox
interface that can prove equivalent accounting. Rhai may respond to committed
events; it may not implement authoritative world mutation itself.

Content validation checks:

- source/target/disposition completeness,
- cost and magnitude bounds,
- permitted handler capabilities,
- physical domain prerequisites,
- deterministic interruption,
- no forbidden semantic effect,
- browser/tooltip documentation.

## Multiplayer and agents

- Client requests specify working id, held instance, target, and intent only.
- Host raycasts, validates knowledge-independent recipe/content, range,
  inventory, charge, apparatus, physical inputs, and rate.
- Multi-tick state belongs to the host and replicates by stable id.
- Prediction may animate charging but never apply world state.
- Simultaneous target transactions serialize or reject cleanly.
- Agents use aim/start/hold/release/cancel actions and receive ordinary cues.
- Moderation/audit records use stable player identity for harmful outcomes.

## Required tests

### Constitutional behavior

- Every base working conserves Current.
- Draw conserves water, salt, temperature/dross carriers, and remainder.
- Rootwake pays water/nutrients and never creates seed/biomass.
- Fieldmend pays correct matching material and cannot upgrade/transmute.
- Kindle uses ordinary fire fuel/provenance.
- Holdfast never reverses age.
- Forbidden fixture effects fail registration or transaction validation.

### Transactions and failure

- Start, complete, cancel, interrupt, disconnect, death, chunk unload,
  save/load, and crash replay each settle exactly once.
- Competing actions cannot double-spend a source or target.
- Forced overdraw produces deterministic expected dross/strain/failure.
- Damaged apparatus changes behavior according to visible state.
- No failure rewrites player-built blocks.

### Balance and integration

- Each magical route remains worse than its mundane counterpart at bulk
  throughput while retaining a useful niche.
- Ward pressure scales, overloads, and never changes stored Ire.
- Dead hearts, seasons, climate, and habitat remain authoritative.
- Local/host/dedicated/guest/agent results are identical.
- Valid fixture-mod workings use approved handlers; raw mutation is denied.
- Performance stays bounded under maximum legal active workings and rituals.

## Completion criteria

This goal is complete when:

- the base wand and ritual roster works through atomic linked transactions,
- every effect pays Current and all relevant physical ledgers,
- forced misuse is legible, deterministic, and produces accounted dross,
- no base or mod path can conjure, transmute, teleport, duplicate, or reveal
  forbidden state,
- technology remains the reliable bulk answer,
- persistence, crash, multiplayer, agents, mods, presentation, and
  accessibility work,
- constitutional, balance, abuse, performance, and repository gates pass.

Do not mark this complete because particles fire when a button is pressed.
The proof is that useful magic can change the world without lying about what
changed.
