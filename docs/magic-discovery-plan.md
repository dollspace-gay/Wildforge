# Discovery — someone made the charms

> **Status: implemented and production-qualified (2026-08-02).**
>
> This is goal 4 of `docs/magic-sequence.md`. It requires goals 1–3 and the
> existing ruins, archaeology, survey, browser, multiplayer, and identity
> systems.

## Implementation record — 2026-08-02

Goal 4 is live in world creation and retrogen, archaeology, host-owned
knowledge persistence, ordinary and remote play, dedicated hosting, headless
agents, declarative content/mod validation, audio, rendering, and the
inventory-style record catalogue. It adds no recipe unlock, research points,
class state, global scan, conjuration, or transmutation. The implementation
includes:

- three deterministic, separated observational outposts on the dry walkable
  component of every accepted homeland. Their chests independently cover all
  seven evidence classes. Existing worked homelands receive explicit
  non-destructive remnants, untouched sites receive the structure, and
  persisted site/recovery claims make both paths idempotent;
- original, deterministic maker claims, warnings, boasts, measurements, and
  contradictions on physical artifacts. Charged finds debit real Current,
  sealed ampoules keep it in `ItemDross`, duplicate brushing resolves once on
  the host, and ordinary breaking loses the fragile find;
- a visible industrial route to a fitted tuning lens, including a retained
  bronze/Echo Slate mount and replaceable Wellglass element. A reading requires
  a stable 1.25-second aim, current reach and line of sight, a physical lens,
  and a physical ledger; target outline, needle/progress motion, text, and
  pitched lens sounds expose settling without relying on color alone;
- host-authored qualitative readings for regions, blocks, held samples,
  organisms, minerals, hearts, apparatus, wakes, scars, echoes, and active
  workings. Calibration narrows uncertainty; ambiguity widens it. Exact atlas,
  ore, inventory, operator-ledger, and hidden-player state never cross the
  gameplay API;
- signed, versioned observations with stable `PlayerId`, display attribution,
  planetary or portable-sample provenance, time/season, calibration,
  uncertainty, optional bounded labels, and optional location. Copies preserve
  a signed lineage, reject forgery and repeated-lineage inflation, and can omit
  every location field;
- physical 32-record field ledgers and 128-record survey folios. Folios persist
  as block entities and spill intact. The record catalogue sorts, pages,
  groups phenomena, compares two readings, reports agreement/disagreement and
  obsolete content, and copies ledger-to-ledger/folio only at a reachable real
  writing surface with placed folios adjacent;
- a persistent two-bay comparative apparatus. Capacity, conductivity,
  stability, biological response, and dross response each require an installed
  physical sample and their calibrated reference object, are repeatable and
  non-consuming, and produce a signed qualitative record. Samples and
  references survive save/load, remain installed across trials, and spill on
  break;
- one host validation path for solo, remote guests, and agents. Monotonic
  settle tickets, current equipment, range, line of sight, fixture kind,
  holder ownership, calibration, apparatus custody, writing adjacency, and
  bounded text are rechecked at completion. Agent MCP actions use those same
  packets and receive only the same earned reports;
- declarative observation, discovery-item, fixture, reference, and station
  schemas. Mods can add visible properties and experiment content but cannot
  scan hidden state, forge records, mutate during observation, or award
  profile/recipe progression. Removed content ids remain readable historical
  evidence.

### Formats and measured budgets

| Surface | Qualified version/value |
|---|---:|
| Discovery state schema | 1 |
| Block-entity schema | 6 |
| Multiplayer protocol | 31 |
| Field ledger / survey folio | 32 / 128 records |
| World knowledge ceiling | 16,384 objects / 65,536 records |
| Discovery save ceiling | 32 MiB |
| Fresh seed `20260802` census | 3 sites / 29 objects / all 7 evidence classes |
| Fresh discovery save | 10,931 bytes |
| Native Windows GPU capture | RTX 3090/D3D12; 60 FPS; 530 resident / 514 GPU chunks; 6.58 ms sim / 9.12 ms draw |

Production creation and reopen of seed `20260802` completed with a qualified
discovery audit: zero invalid signatures and dangling references. The Arcane
audit reconciled 1,610,612,736 units exactly and reported five charged item
accounts, five durable references, and zero orphan, invalid, or duplicate
references. The live graphical client entered the fresh planet and produced
`screenshots/magic-discovery-live.png` with the fitted lens rendered in hand;
a second clean-save capture reopened without recovery warnings. Final visual
qualification ran through the native Windows D3D12 build on an RTX 3090; WSL
software-renderer timings are deliberately excluded from performance evidence.

The scope boundary is intentional: this goal supplies observation, evidence,
experimentation, and shared physical knowledge. Craftable charm descendants,
wands, shaped workings, preparations, and regional dross remediation remain
goals 5–8 rather than being smuggled into discovery.

## Purpose

The first proof of player magic remains an old charm found in a ruin. This
goal connects that artifact to a reproducible practice of observation,
experimentation, mapping, record keeping, and shared learning.

Discovery is not an experience bar, skill tree, per-player scan grind, or
secret recipe system. Wildforge's existing economy rules remain absolute:

- recipes are never secret,
- there are no recipe locks or magical character classes,
- no region owns exclusive knowledge,
- solo play is viable,
- knowledge is valuable because it describes this particular finite world.

The item browser can show how to construct an instrument. The hard part is
learning what the valley, material, ruin, or failing apparatus in front of you
is doing.

## What the ruins establish

Existing ruin loot and tablets are extended to show:

- old people observed and shaped the Current,
- they made the three surviving charm families,
- different settlements practiced differently,
- some understood containment and restoration,
- some treated dross as another frontier to dump into,
- heart-cutting and magical abuse occurred in the same history but magic was
  not itself the sin,
- surviving accounts disagree and remain incomplete.

Do not explain the whole past through omniscient exposition. A tablet records
a maker's claim, fear, boast, measurement, warning, or lie. Environmental
evidence may support or contradict it.

New archaeological evidence classes include:

```text
maker's tablet
calibration plate
spent charm fitting
broken focus
sealed dross ampoule
site survey marks
failed containment fragment
```

They are artifacts and clues, not randomized recipe unlock tokens.

## Finite-world discovery guarantees

The site manifest guarantees:

- at least one early observational ruin within overland reach of every
  accepted spawn,
- several independent examples of each foundational clue globally,
- no required artifact exists at one unique site,
- destructive looting cannot remove access to construction recipes,
- every experiment can be repeated with renewable or redundantly finite
  inputs,
- later-added worlds/content run a census rather than relying on lucky loot.

A player may ignore ruins, read the visible recipes, build instruments through
ordinary industry, and rediscover the system experimentally. Ruins make the
path inviting and culturally grounded; they do not hold the only key.

## The tuning lens

The first purpose-built instrument is the **tuning lens**:

- fine glass supplies the optical body,
- bronze/silver fittings provide controlled conduction,
- echo slate or an equivalent resonant plate supplies the reference,
- a replaceable small wellglass element provides charge,
- construction uses existing glassworking and metalworking stations.

It is a held instrument, not a universal scanning gun. The player aims or
places it, waits for a reading to settle, and sees/hears qualitative results.
No click awards points.

### Readings

At first calibration:

```text
Current strength: faint / steady / strong / saturated
stability: settled / strained / breaking
dominant resonance: one or two names
dross: clear / trace / fouled / dangerous
drift: broad direction with an uncertainty cone
```

It can inspect:

- a local atlas region,
- charged plants, minerals, crystals, items, and apparatus,
- hearts and warden wakes from safe ranges,
- water or soil samples placed in a holder,
- echoes and scarring,
- an active working's broad input/output behavior.

It cannot:

- reveal the global atlas,
- locate exact ore or ruins through walls,
- identify another player's hidden inventory,
- provide operator ledger totals,
- replay private chat or exact historical actions,
- perfectly identify an unknown phenomenon on first sight.

Better interchangeable plates and controlled calibration improve precision;
the player does not gain an invisible perception stat.

## Observation records

A settled reading can be recorded into a physical **field ledger** or a
mounted **survey folio**. Each record contains:

```text
phenomenon/content id
planetary position or portable sample provenance
time and season
instrument/calibration class
qualitative readings and uncertainty
observer identity/display attribution
optional player-authored short label
content/schema version
```

The host signs/owns the record data. Clients cannot forge a measurement.
Location can be omitted when copying a record so societies can share a
material fact without revealing a protected site.

Records stack by summary only when identical. Large collections live in a
bounded indexed block entity rather than bloating item metadata.

## Field ledger and settlement library

### Field ledger

A carried ledger stores a bounded working set of observations and known
comparisons. It provides:

- sortable records,
- side-by-side comparison,
- a player-authored site name/note,
- copying to another ledger at a writing surface,
- import/export to a settlement folio.

Its material recipe is visible and uses ordinary writing materials introduced
with this goal. If paper is added, its agricultural and water costs enter
ordinary content accounting; otherwise hide, charcoal pigment, and reusable
slates provide the first path.

### Survey folio

A folio cabinet or library desk is a physical shared block entity:

- accepts and indexes observation copies,
- groups multiple readings of one phenomenon,
- displays trends and disagreement,
- marks records made under obsolete content/calibration versions,
- copies selected public records to a field ledger,
- spills or preserves records through ordinary block-entity rules.

It is not a magic Wi-Fi database. Information moves when a player brings a
record or copies it through an adjacent/used desk. Server operators may back
up the world, but gameplay has no global automatic knowledge sync.

Destroying every copy loses those measurements, not the ability to rediscover
the rules or see recipes.

## Experimentation

Observation becomes understanding through physical experiments. Initial
experiment families:

### Capacity

Charge a sample from a measured source, wait, and compare retained charge.
Reveals reservoir capacity and leakage band.

### Conductivity

Place equal samples in a standard channel and compare transfer time. Reveals
conductivity and preferred resonance.

### Stability

Apply a bounded pulse in containment and measure returned Current versus
dross. Reveals safe throughput.

### Biological response

Grow paired specimens with controlled water, soil, and Current. Reveals
habitat and uptake behavior without granting plant growth.

### Dross response

Expose a tiny sealed sample and measure sequestration, movement, or damage.
All dross remains accounted and the containment can fail if assembled badly.

Experiments produce records and calibrated reference objects. They never emit
abstract research points or permanently modify a player profile.

## Comparative knowledge

The useful knowledge is relational and world-specific:

- this valley refills slowly after a storm,
- this Hushwood stand is unusually stable,
- a focus from this Wake iron vein produces too much dross above a measured
  rate,
- the western well and the town laboratory are conductively connected,
- the old tablet's "bottomless" source is visibly declining,
- a scar expands after spring flood and retreats in dry weather.

This creates work for surveyors, archivists, guides, investigators, and
traders without imposing classes. A new player can borrow the town ledger,
copy a safe design, and contribute immediately.

## Archaeology interaction

Brushing and ruin containers retain their existing mechanics and Ire rules.
Extensions:

- fragile magical artifacts are destroyed by ordinary block breaking,
- charged finds debit a site reserve when first materialized,
- sealed dross finds credit an actual contained reservoir,
- duplicate loot races resolve once on the host,
- tablet phrase generation gains magical subject/stance/evidence tables,
- ruin echo readings connect prose to an observable place without revealing a
  certain canonical interpretation.

Existing pre-magic generated ruin chunks receive no silent replacement.
Eligible untouched/remnant sites use explicit content retrogen; otherwise
players can follow the non-ruin path.

## Interface

The tuning lens uses the world view first:

- the viewed object gains restrained resonance outlines or phase effects,
- drift is directional in local planetary coordinates,
- sound and needle/plate motion communicate settling and instability,
- a compact reading appears only while using the instrument,
- color is never the only channel.

The ledger and folio may use inventory-style screens because they are actual
books/catalogues, not invisible character menus. Keyboard, controller,
remote-client, resolution, and accessibility behavior match existing UI
standards.

## Multiplayer, privacy, and attribution

- Observation requests are range- and line-of-sight validated by the host.
- The host supplies only values the instrument can measure.
- Player-authored labels are sanitized and size bounded.
- Record authorship uses stable `PlayerId`; display names are presentation.
- Servers may control who can edit a shared folio through existing block
  interaction/moderation primitives, not a new land-claim system.
- Copying public records is allowed; gameplay has no DRM or copy protection.
- Dross attribution visible later is evidence with uncertainty, not an
  omniscient accusation delivered by the ledger.

Agents can aim/use the same lens, receive the same qualitative result, and
write/copy records through ordinary action APIs. No direct atlas query is
added.

## Content and mod API

Content may declare:

```toml
[observation]
categories = ["capacity", "conductivity", "biological_response"]
visible_properties = ["charge_band", "dominant_resonance", "stability_band"]
```

Mods can add artifacts, experiment fixtures, reference plates, record
categories, and display text. They cannot:

- run arbitrary scans outside host-validated range,
- return hidden server/operator state,
- grant recipes or profile levels,
- forge measurements,
- mutate the observed object during a read without a separate transaction.

Unknown content records remain readable as historical unknown ids after mod
removal.

## Required tests

### Progression

- Every qualification seed has the guaranteed overland clue path.
- A headless solo player can construct a lens without any unique loot.
- Ruin and non-ruin paths reach the same observational capability.
- Destroying all records never prevents rediscovery.
- No recipe or working is gated by a player unlock flag.

### Observation

- Readings map authoritative values to the correct qualitative bands.
- Uncertainty widens with weak calibration and ambiguous mixtures.
- The lens cannot reveal out-of-range, occluded, inventory, global, or
  operator information.
- Calibration experiments are repeatable and conserve all resources.
- Records retain provenance/version and reject client forgery.

### Sharing and persistence

- Ledger and folio copy/import/export preserve content exactly.
- Omitting a location actually removes it from the copied record.
- Block break/spill/save/load/mod removal preserve bounded records.
- Two players can independently compare and share observations.
- Remote and agent users receive the same earned information as local users.

### Archaeology and budgets

- Charged artifacts and sealed dross debit/credit their site once.
- Concurrent brushing cannot duplicate a find.
- Untouched-site retrogen is idempotent and preserves touched chunks.
- Tablet generation is bounded, deterministic, and original in wording.
- Large libraries remain within measured save, query, and wire budgets.

## Completion criteria

This goal is complete when:

- ruins and existing charms visibly connect to earlier magical makers,
- every player has a reproducible industrial route to the tuning lens,
- observation and experiments report the real authoritative systems,
- knowledge exists as copyable, losable, rediscoverable physical records,
- recipes and participation remain open with no class or unlock state,
- archaeology, identity, multiplayer, agents, mods, persistence, and UI work,
- progression, privacy, conservation, concurrency, and budget tests pass.

Do not mark this complete because blocks can be scanned into a checklist. The
goal is a practice of learning a particular world, not completionism.
