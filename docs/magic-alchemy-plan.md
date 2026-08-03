# Alchemy — preparations bind power into matter

> **Status: implemented and production-qualified 2026-08-03.**
>
> This is goal 7 of `docs/magic-sequence.md`. It requires goals 1–6 and the
> qualified food, spoilage, glassworks, water, fire, crop, health, and
> container systems.
>
> Qualification covers the eight-preparation base roster, four embodied
> apparatus paths, exact batch/dose/material/water/Current custody, named
> process failures, spoilage and disposal, authoritative status effects,
> multiplayer and agent operation, closed mod validation, bounded population
> budgets, and GPU-rendered UI/audio/visual feedback. The locked all-targets
> suite passed 737 tests with no failures; formatting, Clippy, advisories,
> native and Windows release builds, and the generated-artifact audit passed.

## Purpose

Preparations make magical effects portable, divisible, tradable, and
consumable. They combine the real biochemical behavior of plants/minerals with
bound charge through grinding, extraction, fermentation, infusion,
distillation, settling, and filtration.

Alchemy is not a shapeless crafting-grid recipe that turns one flower and one
water item into a supernatural bottle. The vessel, solvent, heat, time,
ingredients, residues, dose, charge, and failure state all remain physical and
accounted.

## Preparation model

Every batch declares:

```text
solvent/carrier identity and exact volume
active ingredients and physical quantities
catalyst/stabilizer
charge amount and resonance
temperature/time/agitation process
batch volume and number of doses
ordinary residues/byproducts
dross disposition
shelf life and storage requirements
effect, dose, duration, and bodily/ecological costs
```

Output volume cannot exceed liquid input except for declared dissolved
material displacement. Filling bottles transfers batch volume; it does not
copy the batch.

Bound charge moves from ingredients/vessels/Ambient Current into the batch.
Drinking or applying a dose transfers it into an active effect, then returns
or disorders it according to the effect definition.

## Apparatus

### Mortar and grinding slab

- Reduces plants/minerals to measured mash or powder.
- Consumes durability and may lose a declared ordinary fraction as dust.
- Releases or retains charge according to material; careless grinding of a
  saturated ingredient can produce dross.

### Infusion basin

- Holds a finite solvent batch and mounted ingredients.
- Draws charge through an adjacent vessel/conductor.
- Requires a controlled interval and permits sampling.
- Has an actual liquid/container state, not three abstract slots and an arrow.

### Alembic/still

- Uses existing heat sources and glass/metal components.
- Separates volatile fractions and returns condensate through real vessels.
- Conserves water/solvent, salt/nonvolatile matter, charge, and residues.
- Can leak vapor/charge/dross if cracked, overheated, or poorly cooled.

### Filter stand

- Passes liquid/air samples through Ashlace, charcoal, cloth, still salt, or
  later media.
- Transfers contaminants into a finite filter burden.
- A spent filter remains hazardous and must be treated or stored.

Apparatus forms a recognizable laboratory. Breaking, draining, repairing,
overheating, and dismantling use existing embodied-station rules.

## Solvents and vessels

Initial carriers:

- fresh water,
- brine where the process calls for it,
- fermented alcohol introduced through an ordinary food process,
- plant oil/fat where available.

Salt water is not fresh water with a label removed. Distillation consumes
heat, returns salt residue, and moves exact water mass. Rain or Draw cannot
fill apparatus without debiting the same reservoirs as any vessel.

Glass bottles/jars are reusable:

- filling transfers exact volume and batch state,
- drinking returns an empty vessel,
- throwing/breaking credits liquid, charge, dross, and glass disposition to
  the local world through declared paths,
- partial containers either use bounded exact fill levels or are disallowed
  by a clear minimum-dose rule,
- stackability requires identical batch and remaining-life state.

## Process control

Recipes are visible. Correct execution still depends on:

- temperature band,
- order of operations,
- time,
- charge rate and resonance,
- vessel cleanliness,
- ingredient condition,
- stabilization and filtration.

Leaving the valid band produces a deterministic named result:

```text
weak extraction
scorched mash
broken emulsion
spent liquor
fouled batch
overcharged batch
```

Failure never rolls an unrelated random potion. It preserves all material,
water, charge, and dross in declared outputs.

The tuning lens and ordinary visual/smell/sound cues make process state
readable. Automation may control heat, stirring, and transfer through normal
machines, but does not improve chemistry beyond what it physically controls.

## Initial preparation roster

Names and exact balance remain subject to implementation evidence.

### Clear-eye tincture

Echo cap, Rainbell distillate, a small Echo charge, and fresh solvent:

- improves low-light adaptation and weak trace visibility,
- never reveals ores, inventories, exact hidden entities, or global state,
- does not replace a tuning lens for measurement,
- ends by returning/disordering active charge.

### Hearth tonic

Root ingredients, ordinary nutritious stock, and measured charge:

- accelerates natural health recovery over time,
- consumes hunger/nutrient reserve while healing,
- cannot heal a starving body beyond a small stabilization floor,
- does not regrow equipment, replace food, or grant permanent maximum health,
- excessive dosing provides no stacked effect and causes a bounded sickness.

### Root wash

Rainbell water, Ember poppy/Root material, mineral nutrient, and Root charge:

- is applied to one plot/plant or rooting-bed batch,
- supplies only its declared water and nutrients,
- improves uptake or enables efficient Rootwake,
- does not itself set crop age or create seed,
- can burn/salt plants if overconcentrated.

### Settling draught

Hushwood and Frostlace preparation:

- reduces the user's bodily strain while sustaining a working,
- does not reduce apparatus dross below its actual transaction result,
- lowers maximum throughput slightly,
- does not stack and has a recovery interval.

### Ashlace wash

A filter preparation for tools, vessels, soil samples, or small surfaces:

- transfers mobile dross into a recoverable sludge/filter burden,
- never converts dross directly into nothing or Ambient Current,
- has strictly bounded capacity per dose,
- leaves physical wastewater/residue with the correct water and dross state.

### Frostlace suspension

A coating/storage bath for one fragile seed or charged botanical specimen:

- slows decay and leakage during a journey,
- has finite duration and temperature tolerance,
- does not reset age or preserve bulk food economically,
- leaves reusable vessel plus spent carrier/residue.

### Storm cordial

A volatile Stormvine preparation:

- temporarily increases safe personal wand throughput,
- increases reservoir drain and makes overdraw escalate faster,
- gives clear pulse/vision/audio warnings,
- cannot stack and carries a meaningful recovery cost.

### Scouring antidote

A preparation against bodily effects of dross exposure:

- reduces ongoing harm and moves the dose's contained burden into a declared
  excretion/filter/active-to-dross path,
- does not clean the region or erase scar exposure,
- needs Ashlace-derived or equivalent sequestration material,
- cannot make polluted laboratories safe to inhabit indefinitely.

## Body and status accounting

Preparation effects are authoritative timed statuses with:

```text
source batch id
dose
remaining duration
active charge
stack/exclusion group
physiological cost/recovery
completion/interruption disposition
```

Rules:

- duplicate effects refresh only through declared bounded logic,
- incompatible preparations may refuse or produce a documented interaction,
- logout/save/death settles or persists state consistently,
- health restoration uses existing server damage/healing gates,
- hunger/nutrients are debited as healing occurs, not only promised,
- liquid consumption follows the planetary potable-water return model rather
  than deleting water,
- milk/unrelated common items do not universally clear magical effects.

There is no random addiction system in this arc. Tolerance, if later needed,
requires its own design; current recovery intervals prevent spam.

## Spoilage and storage

Organic preparations inherit the existing perishable-item clock:

- temperature and storage affect remaining life,
- distilled/mineral preparations may be stable,
- spoiled batches become named residue/weak or harmful preparations rather
  than vanish,
- combining stacks uses the established age rule and cannot refresh old stock,
- Holdfast slows but does not reset the clock,
- refrigeration/cellars remain useful.

This creates an apothecary trade without universal building upkeep.

## Waste and laboratory responsibility

Every batch names:

- plant/mineral residue,
- wastewater or recovered solvent,
- spent filters,
- fumes if any,
- dross generated,
- safe disposal or reuse paths.

Pouring a batch out transfers its contents to local water/soil/air; it never
invokes a delete action. Sewer, river, and groundwater movement retain dross
and chemical loads supported by the qualified models. Goal 8 makes chronic
mismanagement manifest.

An efficient laboratory is valuable because it captures solvent, reuses
glass, controls heat, and contains dross—not because an upgrade multiplies
yield.

## Content and mod schema

Preparations and processes are data-driven:

```toml
[[preparation]]
id = "example:clear_eye"
process = "distill"
solvent = { tag = "fresh_water", units = 4 }
ingredients = ["example:echo_cap", "example:rainbell"]
charge = { units = 18, resonance = "echo" }
output = { doses = 4, effect = "trace_sight" }
residue = ["base:spent_mash"]
dross = 2
```

Approved effect handlers are bounded like workings. Mods cannot use a potion
effect to mutate arbitrary blocks, grant items, reveal hidden state, erase
dross/Ire, or transmute material. Validation checks batch mass/volume, water,
charge, outputs, residues, duration, stacking, and destruction disposition.

## Multiplayer and agents

- Apparatus operations and liquid transfers are transactional host requests.
- Batch and container states use stable ids and reliable deltas.
- Concurrent filling/decanting cannot duplicate volume or charge.
- Timed effects live on authoritative player profiles/entities.
- Remote visuals show drinking, throwing, apparatus operation, leaks, and
  dangerous overcharge.
- Agents can grind, load, heat, sample, decant, drink/apply, and clean through
  ordinary station actions.
- Host moderation/audit attributes harmful dumping to stable player and
  installation ids where evidence is available.

## Required tests

### Batch conservation

- Every base recipe balances ordinary material, water/solvent, salt, charge,
  doses, residues, and dross.
- Grinding, infusing, distilling, filtering, filling, partial use, pouring,
  breaking, spoilage, and cleanup preserve all ledgers.
- Fixed-point fill and proportional solute/dross remainders survive repeated
  splits.
- A batch cannot fill more doses than its exact volume.

### Effects

- Healing pays hunger/nutrients as health is restored.
- Root wash supplies but does not conjure growth inputs.
- Ashlace wash transfers rather than deletes dross.
- Preservation never reverses age.
- Vision effects reveal only allowed state.
- stacking, recovery intervals, logout, death, and save/load are deterministic.

### Process and failure

- Valid heat/time/order/charge produces the declared batch.
- Each invalid band produces its named balanced failure output.
- Apparatus leaks/failure/dismantling balance and remain legible.
- Ordinary automation controls the same process without special yield.

### Authority, mods, and budgets

- Concurrent host/guest actions commit each transfer once.
- Hostile clients cannot forge batch ids, volume, effect, charge, or time.
- Valid fixture preparations use approved handlers.
- Free-volume, free-healing, hidden-state, unbalanced, and raw-mutation
  fixtures fail validation.
- Large active laboratories/status populations remain within measured server,
  save, and wire budgets.

## Completion criteria

This goal is complete when:

- physical apparatus supports extraction, infusion, distillation, and
  filtration,
- the initial preparation roster is useful without bypassing food, health,
  farming, preservation, observation, or remediation systems,
- every batch, dose, effect, vessel, residue, failure, and disposal path
  reconciles water, material, and Current,
- spoilage, storage, automation, multiplayer, agents, mods, UI, audio, and
  visuals integrate,
- conservation, process, effect, abuse, performance, and repository gates
  pass.

Do not mark this complete because ingredients combine into colored bottles.
Alchemy is complete when a settlement can operate an apothecary whose supply,
quality, waste, and consequences all exist in the world.
