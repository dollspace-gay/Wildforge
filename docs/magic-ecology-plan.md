# Magical ecology — living things participate in the Current

> **Status: implementation design complete, not implemented.**
>
> This is goal 3 of `docs/magic-sequence.md`. It requires goals 1–2 and the
> qualified planetary biome, water-cycle, ecology, and finite-material systems.

## Purpose

This goal turns magical geography into organisms, minerals, crystal growth,
habitats, and visible landscape. Magical content is not decorative loot with
arbitrary ingredient values. Every species or formation performs a legible
role in the Current cycle.

The signature result is not one universal "magic biome." It is a family of
confluence ecologies: supernatural versions of real planetary habitats whose
water, temperature, soil, geology, season, and Current all agree.

This goal provides harvestable materials but does not yet let players research
or craft magic.

## Ecological role vocabulary

Every magical organism or formation declares at least one role:

| Role | System behavior |
|---|---|
| Gatherer | Draws diffuse Ambient Current over time |
| Reservoir | Holds substantial stable charge |
| Conductor | Moves Current readily but stores little |
| Transformer | Changes resonance or binds dross through a real process |
| Indicator | Visibly responds to Current, resonance, Ire, or dross |
| Parasite | Draws charge from another organism, heart, or reservoir |
| Stabilizer | Reduces working instability while lowering throughput |
| Catalyst | Increases rate or output while increasing strain or dross risk |

No base plant exists only to fill an ingredient slot. Its world behavior must
demonstrate why practitioners later use it.

## Magical richness is an overlay

Add continuous habitat fields:

```text
magical richness
usable Current
stability
dominant resonance
dross tolerance
heart influence
```

Species placement and succession require both ordinary habitat and magical
conditions. A rich location therefore retains its base identity:

- tropical mangrove confluence,
- temperate old-growth confluence,
- boreal or alpine glass heath,
- volcanic ember garden,
- desert night oasis,
- polar auroral lichen field,
- freshwater spring marsh,
- marine current reef,
- deep cave echo garden.

The rare, iconic high-richness woodland may receive an in-world proper name
after content review, but it is one expression among these, not the template
for all magic.

Confluence ecology may cross an ordinary biome boundary gradually when the
underlying water and temperature allow it. It never places rainforest plants
in an arctic desert merely because Current is high.

## Base plant roster

Names are working original names and receive a final linguistic/art review in
goal 9. The minimum base roster is:

| Organism | Ordinary habitat | Role and visible behavior | Principal use later |
|---|---|---|---|
| Rainbell | wetlands, springs, wet oasis margins | Gatherer; charged flowers hold luminous dew and close during local depletion | Tide preparations and liquid measurement |
| Hushwood | old temperate forest and taiga | Stabilizer; branch motion and sound diminish as it damps rapid Current changes | Safe wand bodies and containment |
| Stormvine | warm wet forest, stormward cliffs | Reservoir/catalyst; charges during real storms and visibly arcs between tendrils when saturated | High-output foci and volatile infusions |
| Cairnbloom | alpine scree, exposed badlands | Indicator; flower faces follow local Current drift rather than sunlight | Early surveying and Gale/Stone reagents |
| Ashlace | caves, old scars, contaminated margins | Transformer/sequester; binds mobile dross into pale tissue but does not erase it | Filters and remediation feedstock |
| Pilgrim root | healthy heartshadows and old routes | Indicator/conductor; new runners favor living hearts and withdraw from dead ones | Heart work and long-term field records |
| Lantern reed | freshwater margins and swamps | Reservoir; spends stored charge as low light and dims before exhaustion | Portable light, Echo/Tide preparations |
| Nightglass | arid spring and fog-oasis margins | Reservoir; takes up Current during cold nights and closes in daytime heat | Desert wand grips and preservation |
| Frostlace | tundra rock and permanent cold | Stabilizer; locks charge into a slow Stone/Gale mixture until warmed | Cooling, storage, and winter preparations |
| Tidekelp | nutrient-rich coasts and marine confluences | Gatherer/conductor; fronds align with both water and Current flow | Marine survey, salts, and circulation work |
| Echo cap | moist caves, ruins, rotting heartwood | Indicator/reservoir; growth rings retain coarse resonance of nearby events | Memory and perception preparations |
| Ember poppy | fire-disturbed fertile ground | Transformer/catalyst; existing plant gains a real Ember-cycle role after fire | Ignition, heat control, and volatile brewing |

Not every species must be abundant. Every accepted default planet must contain
multiple renewable populations of progression-critical roles. Optional
specialties may remain genuinely regional trade goods.

### Growth accounting

Magical plants are still plants:

- biomass growth consumes water, soil nutrients, light/season, and time,
- reproduction uses species-specific seed/spore/runner rules,
- charge uptake debits Ambient Current,
- passive discharge, decay, composting, fire, eating, and brewing credit
  declared reservoirs,
- dross stored in tissue remains dross after harvest,
- a dead heart may remove supernatural recovery bonuses but cannot delete
  player cultivation,
- irrigation moves real water and can salinize poorly drained soil.

Accelerated magical growth arrives only in goal 6 and pays the same inputs
faster.

## Magical mineral classes

### Finite resonant geology

Some ordinary geological materials have exceptional conductivity, stability,
or memory. Their physical mass is finite and belongs to the planetary deposit
manifest. Initial base examples:

- **Choirstone:** a conductive metamorphic mineral used for channels and
  resonant apparatus.
- **Still salt:** an evaporite that absorbs and immobilizes Current/dross;
  useful for containment, ecologically harmful when dumped.
- **Wake iron:** an uncommon iron-bearing phase that releases charge quickly
  and is difficult to stabilize.
- **Echo slate:** records coarse resonance changes and is used in observation
  plates and library apparatus.

Mining transfers ordinary material exactly through the finite-material
ledger. Any charge present is a simultaneous arcane transfer. Smelting or
grinding cannot duplicate either.

### Regenerative exceptional crystals

**Wellglass** is the working name for naturally grown charge-bearing crystal.
It is exceptional magical matter governed by the arcane ledger rather than an
ordinary ore that respawns.

A growth site requires:

- a persistent seed/bud,
- compatible host rock or mineral solution,
- minimum Ambient Current and suitable resonance,
- local stability within a species band,
- free attachment space,
- a growth interval.

Each growth stage atomically debits Ambient Current and credits the crystal's
bound charge. A placed block represents a sparse cluster, not a cubic metre of
free conventional mineral.

Harvest:

- transfers declared shards and their exact charge,
- leaves a seed when harvested with the correct tool,
- destroys or damages the seed under careless extraction,
- may release dross if overcharged or unstable,
- never produces both intact block charge and item charge.

Different resonance mixtures change structure and behavior, not merely color
and ingredient labels. Base content avoids a one-crystal-per-aspect rainbow.

## Unloaded ecology and deterministic sites

The finite planet must not regrow resources only where players stand.

Magical populations use deterministic feature-site ids tied to atlas cells:

- genesis reserves candidate sites and carrying capacity,
- materialization creates the exact current state once,
- harvesting records site state/tombstone and charge transfer,
- unloaded succession advances coarse population/charge state,
- rematerialization reconciles the site rather than rerolling it,
- player-placed cultivation has distinct ownership/state and never gets
  overwritten by genesis.

Crystal growth sites use the same identity rule. Chunk unload, regeneration,
retrogen, and worker order cannot duplicate a bud or its charge.

## Harvest, cultivation, collapse, and restoration

Each species declares:

```text
habitat tolerances
reproduction mode and season
water/nutrient demand
Current uptake/release
charge capacity
dross tolerance and disposition
harvest parts and regrowth
ire/tending classification
coarse carrying capacity
```

Overharvesting can:

- remove seed sources,
- lower local charge storage,
- expose soil or alter water,
- reduce natural dross recovery,
- collapse a specialized confluence community into its ordinary base habitat.

Restoration remains possible through habitat repair, replanting, clean water,
reintroduced seed, Current recovery, and time. It does not require a unique
lost item. Proper pruning, coppicing, fruit collection, spore collection, and
seed-preserving crystal harvest provide lower-impact practices.

Taking from wild magical ecology uses the existing regional Ire rules by
actual damage and renewal class. The mere transfer of Current does not change
Ire. Tending and successful establishment may use existing capped reciprocity
hooks.

## Hearts, wardens, wildlife, fire, and water

### Hearts

- Healthy hearts bias local resonance and improve dross-reordering ecology.
- They may support exceptional growth by transferring charge from their
  accounted reserve.
- A dead heart stops those gifts but does not change geology or erase
  cultivated plants.
- Grafted hearts can shift compatible magical species slowly within physical
  climate limits.

### Wardens

- Manifestation charge and charged drops follow goal 1.
- Warden passage may leave temporary wakes that indicator species read.
- Warden materials become ingredients later without being the sole route into
  basic magic.
- Magical plants are not wardens and do not automatically become hostile at
  high Ire.

### Wildlife

Magical habitat may change forage, cover, attraction, avoidance, or temporary
charge. This goal does not add a separate bestiary merely to advertise the
biome. Existing wildlife receives only behaviors justified by its ecology.

### Fire

- Ember poppy and fire-following species respond to real fire history.
- Burning charged biomass transfers charge and dross explicitly.
- Natural, husbandry, and arson provenance remains intact.
- Magical fire does not gain permission to rewrite player blocks beyond the
  existing player-caused fire rules.

### Water

- Rainbell, Tidekelp, springs, and aquatic richness read actual water state.
- Charge or dross dissolved/suspended in water is metadata/reservoir state,
  not replacement water.
- Harvesting luminous dew debits both the plant/Current and the actual liquid
  volume used.
- Magical ecology cannot create an oasis without groundwater or surface
  supply.

## Visual and audio direction

Avoid a uniform neon-purple signifier. The Current is visible through:

- polarized or oil-film color at grazing angles,
- rhythmic posture, alignment, rings, and branching,
- restrained internal light where charge is actually being spent,
- harmonic, damped, or phase-shifting ambient sound,
- particles tied to transfer events rather than constant glitter,
- climate-specific materials and silhouettes.

Dross/scarring receives its own grounded palette in goal 8. Magical richness,
Ire, and dross must not be communicated by recoloring the same effect.

All base textures retain Wildforge's packable data-driven content path and
have procedural fallback art. Texture packs and synced server content retain
parity.

## Content and mod schemas

Extend feature, block, and item definitions with declarative magical ecology:

```toml
[arcane_ecology]
roles = ["gatherer", "indicator"]
charge_capacity = 80
uptake_per_day = 3
resonance = { tide = 3, root = 1 }
dross_tolerance = 12
habitat = ["freshwater_margin", "warm_enough"]
```

Exact schema is implementation-owned. Requirements:

- every uptake has a source and every destruction/decay path a disposition,
- magical habitat composes with ordinary tags,
- finite minerals declare material identities/deposit rules,
- regenerative crystals declare seed and exact charge stages,
- scripts may react to events but cannot award growth or harvest directly,
- content validation detects unreachable base progression and unbalanced
  lifecycle definitions.

Post-creation content follows finite-world retrogen policies and never
overwrites touched terrain. Adding a species may introduce seed through a
declared world event or deterministic untouched habitat; it does not pretend
unvisited infinity exists.

## Player information

Before goal 4, ecology teaches through observation:

- Rainbells close as a marsh is depleted.
- Cairnblooms lean with drift.
- Lantern reeds dim as stored charge falls.
- Ashlace pales or darkens with dross burden.
- Wellglass growth halts before a region becomes dangerously overdrawn.

Tooltips identify harvested items and basic physical properties, not exact
hidden numbers. The tuning lens later quantifies qualitative bands.

## Required tests

### Habitat and worldgen

- Every species satisfies climate, water, soil, geology, and Current
  predicates.
- Cross-climate confluence variants preserve their base biome causality.
- No cube seam changes density or species selection.
- Same seed/load order produces identical site ids and initial states.
- Seed census proves every core role is redundantly accessible.

### Conservation

- Plant uptake/growth/harvest/decay/fire/composting balances Current, water,
  nutrients where tracked, and declared physical material.
- Every crystal stage debits exactly the charge it stores.
- Correct harvest, destructive harvest, explosion, fire, item loss, and
  rematerialization cannot duplicate crystal charge.
- Finite magical mineral extraction reconciles both ledgers.
- A long unloaded/loaded succession run has zero unexplained Current.

### Ecology and recovery

- Overharvest lowers seed bank/carrying function visibly.
- Protected harvest retains renewal.
- A collapsed population can recover from non-unique seed and repaired
  habitat.
- Dead hearts withdraw bonuses without deleting cultivation.
- Dross sequestration moves dross into tissue rather than erasing it.
- Ire changes follow actual taking/tending actions, not Current arithmetic.

### Multiplayer, mods, and budgets

- Simultaneous harvest resolves once on the host.
- Remote clients receive authoritative site/block/item state.
- Valid fixture species/minerals/crystals run through the same lifecycle.
- Invalid free-growth and duplicate-harvest fixtures are rejected.
- Coarse unloaded ecology stays within measured CPU, memory, save, and network
  budgets.

## Completion criteria

This goal is complete when:

- magical richness is a causal overlay across ordinary planetary habitats,
- the base role roster exists with climate-correct placement and lifecycle,
- finite resonant minerals and regenerative wellglass use the correct ledgers,
- loaded and unloaded growth is deterministic and duplication-safe,
- overharvest, cultivation, collapse, and restoration all function,
- hearts, wardens, Ire, water, soil, fire, mods, and multiplayer integrate,
- visual and audio language is distinct and environmentally legible,
- conservation, habitat, seed-census, persistence, performance, and
  repository gates pass.

Do not mark this complete because a glowing tree and crystal ore generate.
The ecology is complete only when their existence, growth, harvest, loss, and
recovery all have causes.
