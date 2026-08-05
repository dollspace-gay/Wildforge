# Implements — charms, wands, charge, and embodied apparatus

> **Status: implemented and production-qualified (2026-08-03).**
>
> This is goal 5 of `docs/magic-sequence.md`. It requires goals 1–4 and the
> existing crafting, stations, durability, salvage, container, rendering, and
> multiplayer systems.

## Implementation record — 2026-08-03

Goal 5 is live in ordinary play, local and dedicated hosting, remote
presentation, agent actions, persistence, and the arcane and material audits.
The implementation includes:

- declarative base/mod component and charm definitions with one versioned,
  role-aware, order-independent, bounded resolver. Every base combination is
  enumerated in qualification, exposes its costs in the frame preview, and no
  combination simultaneously wins capacity, throughput, stability,
  containment, and dross;
- four-part stable-identity wands assembled in a physical binding frame. The
  same host action path mounts parts, inspects, calibrates, assembles, swaps a
  focus, transfers or safely discharges charge, repairs, disassembles, and
  binds charms. Actual adjacent mounts, vessel, conductor network,
  containment, tools, revisions, and inventory custody are validated before
  commitment;
- finite charge vessels, Current sources, local conductor networks, and
  containment. Transfer rate, loss, retained dross, saturation, damage,
  leakage, unloaded boundaries, network size, confluence/well extraction,
  overfill, breakage, and failure are bounded server simulation and ledger
  transactions. Conductors cannot name or scan an arbitrary nearby item
  account;
- charged craft lifecycles for Quiet, Bark, and Slow Hunger charms. A real
  reagent and blank become a stable instance at the frame; each shipped effect
  remains at its old cap, receives no benefit without a successful debit,
  becomes dormant exactly at depletion, and recharges only by debiting a
  vessel or another declared finite source;
- exact construction bills and linked material deltas for assembly, focus
  replacement, repair, safe recovery, and destruction. Frame breaks route
  charge to an adjacent vessel, Ambient Current, or environmental dross;
  catastrophic heat/strain failure leaves one same-identity recoverable
  fragment bundle rather than deleting matter or Current;
- custody reconciliation through inventories, transactional cursors, chests,
  cargo, loose items, hosted death, save/load, crash recovery, fire, lava,
  despawn, and content-pack removal. Legacy charms migrate once against the
  planetary reserve, partial funding stays partial, and every migration is
  idempotent and audited;
- component-derived first-person and remote-held models, focus silhouettes,
  restrained charge emitters, qualitative tooltips, exact tuning-lens
  readings, apparatus light/strain cues, and distinct use, transfer, strain,
  empty, and failure sound envelopes. Authored descendant art ships with
  procedural/missing-content fallbacks;
- reliable optimistic-revision host requests, interest-managed public state,
  concurrent serialization, hostile-request rejection, explicit
  creative/admin marking, and the same frame actions and inspection text for
  headless agents. Clients never submit parts, charge, dross, resolved stats,
  or finished outputs.

The base game has no separate generic item-frame or formal land-claim system.
Its physical item mount is the binding-frame cradle, whose save, break, spill,
and reconstruction lifecycle is qualified. This goal does not invent either
excluded subsystem: place conductors source only real local geography, while
item transfers require the actor's selected authoritative custody.

Qualification covers twenty focused Implements integration tests plus the
shared arcane, material, survival, multiplayer, registry, rendering, and agent
suites. On 2026-08-03 the repository gates passed with 660 tests and 22
intentional operator probes ignored, strict Clippy, optimized release builds,
and a clean dependency-advisory audit. A native Windows DX12 capture on an
RTX 3090 held 60 FPS with 138/138 resident chunks uploaded, no dirty chunks,
2.59 ms simulation time, and 11.02 ms draw time. The captured save's schema-2
Implements audit found three instances, zero missing accounts, zero content
mismatches, zero over-capacity instances, balanced parent arcane and material
audits, and bounded save/wire metadata.

## Purpose

This goal lets players reproduce the kind of craft implied by ruin charms. It
adds component-built wands, charge storage and routing, binding apparatus, and
craftable charged charms.

It does not yet add the broad working roster. Implements can be charged,
inspected, assembled, safely discharged, worn, repaired, and exercised through
bounded test operations. Goal 6 gives wands and ritual structures their useful
world verbs.

## Design rule

An implement shapes or carries power; it does not own an infinite ability.

Every implement has some combination of:

```text
charge capacity
conductivity / maximum safe transfer rate
stability
resonance affinity
dross retention or leakage
physical durability
```

No scalar "magic tier" determines which one is best. A high-throughput tool is
not also the safest, largest, most precise, and easiest to repair.

## Charge state

Charge-bearing items use the stable instance state from goal 1:

```text
current charge
charge capacity
dross burden
resonance mixture
component identities
wear/strain
provenance/version
```

Tooltips use qualitative bands by default and better readings when examined
with a tuning lens. Charge is not a second player hunger/mana bar.

Items do not recharge merely because they exist in inventory. Charge enters
through:

- a binding frame drawing from measured Ambient Current,
- transfer from a wellglass shard or vessel,
- an attached conductor at a confluence/well,
- a declared biological or warden material process,
- a later working that transfers rather than creates charge.

Every route uses an arcane transaction and produces the declared dross.

## Wand construction

A wand is a four-part instrument:

### Body

Controls handling, broad stability, and resonance. Initial bodies:

- Hushwood: stable, slow, low dross, modest throughput.
- Nightglass stem/wood: retains charge well, sensitive to heat.
- Stormvine braid: high throughput, poor stability when saturated.
- ordinary seasoned wood: accessible, inefficient, repairable baseline.

### Reservoir

Holds a working buffer:

- wellglass seed/shard,
- charged biological nodule,
- small refillable vessel,
- no reservoir for a place-bound ritual rod.

Capacity brings risk: a large unstable reservoir has a more serious failure.

### Focus

Physically encodes one family of working behavior:

- Echo slate for reading/revealing,
- Choirstone geometry for controlled movement,
- Wake iron for fast ignition/change,
- living/root structures for cultivation and binding,
- later mod-defined foci through the same registry.

A wand carries one installed focus in the base game. Changing it requires the
binding frame. This keeps the held object legible and avoids a spell-wheel
Swiss army gun.

### Binding/ferrules

Metal and fiber fittings control leakage, wear, and component compatibility.
Bronze is forgiving, silver conducts precisely, lead/still-salt assemblies
contain but burden the tool, and steel survives strain without improving
magical precision by itself.

## Component behavior

Resolved behavior derives from declared component properties through one
documented formula. Requirements:

- order-independent where physical order is irrelevant,
- explicit where body/focus/reservoir roles differ,
- bounded against zero-cost or infinite-capacity combinations,
- visible in the assembly preview before commitment,
- validated identically for base and mod components,
- stored as component ids plus format version, not only baked opaque stats.

There is no procedurally rolled rarity or affix quality. Materials and
craftsmanship decisions explain the result.

## Binding frame

The central workshop is an in-world multiblock **binding frame**, assembled
from:

- a work surface,
- conductor arms or posts,
- a sample/focus mount,
- a charge vessel position,
- a containment boundary,
- access to ordinary tools and heat where a recipe needs them.

It validates actual surrounding blocks. The screen, if used, represents
physical mounts and readings; it is not a three-slot arrow machine.

Operations:

- assemble/disassemble a wand,
- install/remove a focus,
- bind a charm,
- transfer charge between local reservoirs,
- discharge an implement safely into the environment or a vessel,
- inspect strain and component compatibility,
- repair with matching materials,
- run a low-power calibration pulse.

Breaking the frame spills ordinary components. Stored charge transfers to the
vessel, Ambient Current, or Dross according to which part was broken and how.
It cannot vanish or duplicate.

## Vessels and conductors

### Charge vessel

A stationary glass/metal vessel safely holds moderate charge or contained
dross. It:

- has a finite capacity and pressure/strain state,
- must be physically adjacent to a frame or conductor endpoint,
- leaks according to construction and damage,
- displays state through light, sound, and instrument reading,
- can be dismantled safely with the right tool,
- fails visibly if overfilled.

### Conductor

Choirstone/metal conductor pieces move Current between adjacent apparatus.
They do not transmit across unloaded arbitrary distance or form a free global
power grid.

- Transfer has capacity, loss/dross, direction, and update bounds.
- Networks are local, physically built, and server simulated.
- Every connected component has a size/budget cap or incremental solver.
- Breaking one segment changes connectivity deterministically.
- A conductor cannot extract through protected construction or from an
  unowned item.

Long-distance Current transport uses charged cargo and the existing road,
boat, rail, and trade systems.

### Containment

Still salt, lead, Hushwood, and physical spacing reduce leakage/failure in
different ways. Containment is an arrangement to build and inspect, not a
single perfect magic-proof block.

## Existing charms become a craft

The one dedicated charm slot remains. Charms are modest passive implements,
not permanent free buffs.

### Charm of Quiet

- Reduces a warden's attention radius as currently shipped.
- Debits a small charge only when it actually changes an attention result or
  sustains an active concealment interval.
- Does not lower Ire or make the wearer morally invisible.

### Charm of Bark

- Retains the current small armor benefit.
- Debits charge when it prevents damage.
- Cannot reduce a hit below the established minimum or replace armor repair.

### Charm of Slow Hunger

- Retains the current modest hunger reduction.
- Debits charge in fixed authoritative intervals while it saves hunger.
- It never creates food or prevents starvation after depletion.

All three receive:

- visible charge/depleted behavior,
- a binding recipe using ordinary body materials plus appropriate magical
  plant/mineral components,
- safe recharge at a frame,
- disassembly/salvage behavior,
- tuning-lens readings,
- original descendant art related to but distinct from ruin examples.

Ruin charms remain valuable because they are well-made, historically
interesting, and may have favorable capacity/stability—not because their
recipe is secret.

### Migration

Pre-goal charged-looking charms in valid planetary saves receive charge by an
explicit one-time migration that debits the regional Ambient/Deep reserve.
If insufficient, the charm begins partially charged. The migration is
idempotent and appears in the audit. No item is filled from nowhere.

## Physical lifecycle

- Wand use creates strain and ordinary wear according to throughput.
- Repair consumes matching body/fitting material.
- Safe disassembly recovers reusable components at declared yields.
- Catastrophic failure produces recoverable fragments, released charge, and
  dross; it does not delete the expensive wand.
- Fire, lava, despawn, death, containers, item frames, cargo, and mod removal
  use explicit charge dispositions.
- A charged implement remains an item with mass and obeys ordinary transport.

Creative/admin items can be infinite only in explicitly non-survival modes and
their use is excluded/marked in audits.

## Interface and feel

- Held wands have component-derived silhouettes/material accents.
- Focus identity is readable without relying only on color.
- Charge is communicated by a restrained emitter whose brightness reflects
  actual stored state.
- Use, transfer, strain, empty, and failure have distinct sound envelopes.
- First-person animation shows aiming/settling, not automatic gun recoil.
- Inventory tooltip gives purpose, installed parts, condition, and qualitative
  charge; the tuning lens supplies detail.
- Remote players see the correct held model and transfer/use events.

## Content and mod schema

Provisionally:

```toml
[wand_component]
role = "body"
capacity = 20
conductivity = 12
stability = 85
resonance = { root = 2, echo = 1 }
repair_material = "base:hushwood"

[charm]
effect = "quiet"
charge_per_trigger = 2
capacity = 120
```

The exact schema may differ. Validation requires:

- recognized role and effect handler,
- bounded properties,
- declared charge/dross behavior,
- physical recipe balance,
- one authoritative resolver for base/mod components,
- no script-defined raw passive stat mutation,
- no component combination that bypasses the transaction API.

Server content sync includes definitions and art but not scripts, consistent
with existing multiplayer policy.

## Multiplayer and agents

- Assembly, transfer, focus swap, recharge, repair, and disassembly are
  reliable host-authoritative requests.
- Containers use transactional cursor semantics and stable item instances.
- Concurrent use of one vessel/frame serializes without duplication.
- Interest-managed deltas send implement state only where legitimately
  visible/owned.
- Held focus/model and activation events replicate.
- Agents manipulate mounts and inventory through the same actions and receive
  the same tooltip/lens information.
- No client chooses its resolved wand stats or charm effect.

## Required tests

### Components and lifecycle

- Every base component combination resolves deterministically.
- Property bounds prevent negative cost, overflow, and infinite capacity.
- Assemble/disassemble/repair/fail/despawn/fire/lava/save all conserve charge
  and tracked materials.
- Stable item ids and charge survive inventory, container, cargo, death,
  save/load, and mod removal.
- Stack merge/split restrictions prevent laundering state.

### Charms

- Each existing effect remains within its current gameplay cap.
- Benefits occur only when sufficient charge is debited.
- Depleted charms provide no hidden benefit.
- Recharge debits a source and produces declared dross.
- Migration is idempotent and ledger-balanced.
- One charm slot and non-stacking behavior remain.

### Apparatus and authority

- Binding-frame multiblock validates, breaks, and reconstructs correctly.
- Vessel overfill, leakage, safe discharge, and catastrophic failure balance.
- Conductors obey adjacency, throughput, unloaded-boundary, and network-size
  rules.
- Concurrent remote operations commit once.
- A hostile client cannot forge parts, stats, charge, or frame results.
- Agents can complete the same bounded assembly sequence.

### Mods, visuals, and budgets

- Valid fixture components/charms use the shared resolver.
- Invalid raw effects and unbounded combinations fail content validation.
- Every base implement has procedural fallback and content-pack art.
- Held/local/remote models and state cues match authority.
- Network solve, save size, item metadata, and transfer costs meet measured
  budgets.

## Completion criteria

This goal is complete when:

- the existing three charms have craftable, charged, conserved lifecycles,
- component-built wands offer meaningful tradeoffs without a best tier,
- frames, vessels, conductors, containment, repair, and salvage are embodied
  and functional,
- no implement recharges, activates, fails, or disappears outside the ledger,
- UI, art, audio, multiplayer, agents, mods, and migration work,
- all lifecycle, concurrency, authority, balance, performance, and repository
  gates pass.

Do not mark this complete because a wand item holds a charge number. An
implement is complete when its physical construction explains what it can
safely do and every state transition survives the finite world.
