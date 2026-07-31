# Magic qualification — prove the whole practice

> **Status: implementation design complete, not implemented.**
>
> This is goal 9 and the final gate of `docs/magic-sequence.md`. It requires
> goals 1–8 to claim implementation with evidence. It implements missing
> integration work discovered by the audit; it is not permission to waive a
> failed requirement.

## Purpose

Individual subsystems can compile while the complete promise remains false:
a crystal may duplicate on unload, a preparation may delete water, an easy
wand loop may obsolete pumps, a guest may forge charge, or scarring may never
actually recover.

This goal audits every requirement in the master and goals 1–8, closes gaps,
runs long and adversarial scenarios, records measured budgets, finalizes
content language and presentation, and updates all player/modder/operator
documentation.

Completion means that a fresh group can move from an old charm to a mature
magical society on one finite planet, and every benefit and consequence
survives technical scrutiny.

## Completion audit

Build a requirement matrix from:

- every numbered or bulleted requirement in `docs/magic-sequence.md`,
- every named type, layer, reservoir, item, plant, mineral, apparatus,
  working, ritual, preparation, scar stage, command, artifact, and scenario in
  goals 1–8,
- every stated invariant, forbidden behavior, test, budget, migration,
  multiplayer/mod/agent/UI/documentation requirement, and completion
  criterion,
- every prerequisite planetary requirement that magic touches.

For each row record:

```text
requirement
owning goal
implementation location
authoritative test/diagnostic/runtime evidence
result: proven / contradicted / missing / insufficient
remediation commit/change
final evidence
```

Search results and compilation are not evidence for behavior. A test counts
only after its assertions are inspected and shown to cover the requirement's
scope.

Any contradicted, missing, or insufficient row is implementation work in this
goal. The matrix ships with the implementation record.

## Fresh-world progression scenario

Automate as much as possible and manually verify the presentation:

1. Create a qualified finite planet with the production content set.
2. Spawn through the normal survival path with no admin grants.
3. Find an overland ruin clue and an old charm; also verify the alternate
   no-ruin construction path.
4. Produce required ordinary glass, metals, writing materials, and tools.
5. Construct and calibrate a tuning lens.
6. Survey at least one ordinary site, confluence, still, heartshadow, and
   charged organism/mineral.
7. Record, copy, omit a sensitive location, and share observations through a
   settlement folio.
8. Harvest/cultivate renewable magical material and extract one finite
   resonant mineral.
9. Grow and harvest wellglass with exact Current accounting.
10. Build a binding frame, vessel, conductor, wand, and each existing charm.
11. Safely charge, use, deplete, recharge, repair, disassemble, and rebuild an
    implement.
12. Perform every base wand working and ritual with its mundane inputs.
13. Build an alchemy laboratory and produce/use every base preparation.
14. Cause trace dross under ordinary use, capture it, and demonstrate routine
    safe operation.

The scenario succeeds in solo without requiring another player, a unique
drop, undisclosed recipe, operator command, or debug atlas.

## Society scenario

Run a multi-client/dedicated-host scenario with at least four identities:

- one surveys and publishes selected records,
- one cultivates magical plants/crystals,
- one operates a wand/ritual workshop,
- one brews and handles waste.

Verify:

- records and location redaction move only through physical sharing,
- regional materials encourage travel/trade without hard-locking a player,
- charge cargo remains physical,
- shared apparatus transactions serialize correctly,
- a ward protects only while supplied,
- an operator can inspect contributions without players receiving the admin
  audit,
- malicious dumping can be investigated with uncertain in-world evidence,
- moderation/identity logs attribute authoritative actions,
- recovery remains possible after the guilty player disconnects.

This is a technical scenario, not a claim that unobserved future players will
form a government. Its purpose is to ensure the mechanics support the story.

## Careless-use and recovery campaign

Execute the complete goal-8 careless laboratory:

1. Establish clean baseline maps, census, and audit.
2. Use mismatched components and forced overdraw.
3. Run poorly contained rituals and alchemy.
4. Release dross into air, soil, and a headwater stream.
5. Observe every warning band through world cues and instruments.
6. Reach a local scar and one controlled breach.
7. Verify structures/inventories remain intact and physical ledgers close.
8. Stop sources, isolate carriers, excavate manifestations, filter/sequester
   mobile dross, and restore habitat.
9. Compare dead-heart versus reawakened-heart recovery.
10. Continue until the region returns below trace with all contained dross or
    reordered Current located.

Record the elapsed player/simulation time, materials, labor, Current movement,
ecological damage, Ire changes, and final unexplained deltas.

Prevention must be materially cheaper than the campaign. Recovery must be
substantial but not an endless chore or irreversible soft-lock.

## Cross-ledger conservation

Run a combined audit after every major scenario:

```text
Current = exact genesis total + explicit operator adjustments
water   = exact qualified planetary total
salt    = exact qualified planetary total
tracked materials = manifest + explicit external additions/loss
```

Required coverage:

- crystal and plant growth/harvest/decay/fire,
- warden spawn/dissolution/death/drop,
- charm and wand charge/use/failure/loss,
- active/cancelled/crashed workings,
- water Draw and rooting-bed irrigation,
- alchemy extraction/distillation/dosing/pouring/spoilage,
- dross carrier movement, scarring, excavation, sequestration, reordering,
- item/entity despawn, lava/fire, death, cargo, container destruction,
- mod removal/reinstall and retrogen,
- save/load and crash replay.

Every unexplained delta is zero. An operator adjustment is acceptable only
when the test intentionally invokes one and the report names it.

## Long simulation

Run at least:

- a century-equivalent untouched magical planet,
- a century-equivalent careful-use planet,
- a sustained industrial-abuse planet,
- a stopped-source recovery continuation,
- repeated loaded/unloaded and varying-worker-count variants.

Prove:

- exact global Current conservation,
- no equilibrium oscillation or rounding drift,
- climate/water/ecology remain within their qualified bounds,
- magical populations do not explode or vanish from bookkeeping,
- wells debit/recover from real reserves,
- normal use does not require constant pollution chores,
- abuse can produce connected regional consequences but not an unbounded
  automatic global cascade,
- stopped-source recovery converges under viable conditions,
- chunk-load order and worker count do not change authoritative hashes.

## Seam, topology, and spatial matrix

Exercise every directed face edge and all eight cube corners for:

- Ambient Current and dross diffusion,
- directional conductivity and drift,
- air/water dross transport,
- confluence ecology/site materialization,
- crystal growth and harvest,
- lens direction/uncertainty,
- wand targeting and working traces,
- conductor/ritual boundaries where legal,
- wards and manifestations,
- country/heart/Ire association,
- map/record coordinates,
- multiplayer interest and interpolation.

No effect reflects, duplicates, stalls, rotates incorrectly, or gains privilege
at a seam.

## Abuse and exploit matrix

Attempt:

- recharge/discharge loops,
- crystal harvest with unload/reload or simultaneous tools,
- stack split/merge and stale item ids,
- active-working disconnect/death/crash,
- alchemy fractional fill and spoilage refresh,
- Draw between full/empty/saline/dross-bearing reservoirs,
- Rootwake without water/nutrients/space,
- Fieldmend with wrong or insufficient material,
- Kindle on unsupported targets,
- ward segment unload/break/rebuild,
- vessel/conductor network cycles,
- dross dumping during save boundaries,
- scar excavation while another player treats the same site,
- forged client readings/records/charge/effects/attribution,
- mod handlers that spawn, transmute, delete, scan, teleport, or create free
  sinks,
- content hot reload changing capacity or mass under live instances.

Each attempt either performs one balanced legal transaction or fails without
partial mutation.

## Balance qualification

Measure mundane and magical routes under comparable conditions:

```text
light-years per material/Current unit
ignitions per effort
item mass-distance throughput
crop biomass per water/nutrient/time/Current
water volume-distance throughput
repair recovery/material efficiency
preserved item-days per resource
protected area-pressure-time per charge
preparation effect per ingredient/labor
dross produced and recovery labor
```

Required outcomes:

- technology wins reliable bulk throughput,
- magic wins flexibility, precision, portability, or unusual conditions,
- one component set is not best for all workings,
- one magical habitat or resonance is not universally optimal,
- old ruin charms are desirable but not mandatory,
- basic magic remains viable on every major continent,
- regional specialties still support travel and trade,
- no loop makes ordinary mining, farming, logistics, food preservation,
  medicine, metallurgy, or mechanization irrelevant.

Balance constants may change here only with the measurements recorded in each
affected implementation record.

## Multiplayer, agents, saves, and mods

### Multiplayer

- Dedicated host remains the only authority.
- Join-in-progress receives sufficient local atlas/effect/apparatus state.
- Reconnect preserves item/status/working ownership safely.
- Content sync covers data and art with existing script boundaries.
- Interest management does not reveal global maps or private records.
- All concurrent actions use stable ids and transactional resolution.

### Agents

- An agent can follow the same fresh progression using perceived state and
  ordinary verbs.
- It receives no direct global Current, exact hidden dross, recipe unlock, or
  privileged target endpoint.
- Its harmful/tending actions receive the same Ire and provenance treatment.

### Saves

- Test migration from the last qualified pre-magic planet.
- Test every arcane schema/protocol/content version introduced by goals 1–8.
- Unknown content remains safe and accounted.
- Atomic backups/recovery report corruption without inventing Current.

### Mods

Provide fixture mods for:

- one valid resonance-aware plant,
- one finite resonant mineral,
- one growing exceptional crystal,
- one wand component/charm,
- one approved-handler working,
- one preparation and residue chain,
- one scar growth and treatment.

Provide invalid fixtures for every forbidden capability/accounting class and
prove actionable rejection. Update `mods/README.md` with complete finite-world
examples and migration/retrogen guidance.

## Performance qualification

Measure on the documented baseline and at least one constrained target:

- arcane atlas memory and save size,
- whole-planet coarse tick/catch-up,
- audit/export time,
- magical ecology unloaded update,
- charged item/entity indexes,
- maximum legal conductor/ritual solve,
- active workings and statuses,
- scar sites and provenance aggregation,
- host networking and client rendering,
- world creation time added by magical geography.

Set and record production budgets based on evidence. Required invariants:

- ordinary no-magic play has negligible new tick overhead,
- no operation scans all chunks or all charged objects,
- no client receives global dynamic layers for ordinary play,
- saves and histories remain bounded,
- overload degrades work queues/visual density before simulation correctness.

## Presentation and originality review

Produce captures/audio checks for:

- at least six climate-distinct confluence ecologies,
- confluence, well, still, echo, heartshadow, and wake,
- every base magical plant and mineral,
- wellglass growth/harvest/depletion,
- tuning lens and shared folio,
- each wand component family and charm,
- every working/ritual/preparation action,
- each dross warning/scar stage in several carriers/climates,
- containment and a recovered landscape,
- local and remote player views,
- color-vision-deficient and reduced-effects settings.

Review final names, prose, silhouettes, palettes, particles, audio, interfaces,
progression, and mechanical combinations as a whole. Remove temporary terms
or content that remain recognizably derivative of a specific prior game's
protected expression. Generic fantasy vocabulary alone is not the review, and
this internal review is not a legal opinion.

The final direction must read as Wildforge: grounded material processes,
planetary ecology, restrained supernatural signs, physical workshops,
conservation, the Wild, and social consequence.

## Documentation

Update:

- README player overview and controls,
- item browser descriptions and uses,
- mod author documentation and schemas,
- dedicated-host/operator commands,
- agent MCP operations,
- save/migration/version notes,
- architecture/module ownership,
- planetary atlas/material/water cross-references,
- historical plan supersession notes.

Documentation must state plainly:

- Ire is not dross,
- the Current is finite,
- recipes are public,
- magic cannot conjure/transmute/teleport,
- water/material costs remain real,
- scarring warns and is recoverable,
- exact audit commands and permissions,
- how post-creation magic content enters a finite world.

## Repository gates

Run:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
cargo deny check advisories
```

Also run all goal-specific:

- arcane audit and atlas validators,
- seed census,
- long simulation,
- seam/corner suite,
- fresh progression and society scenarios,
- careless-use/recovery campaign,
- cross-ledger audit,
- multiplayer/agent/mod/crash/abuse matrices,
- performance benchmarks,
- visual and accessibility capture checks.

Commands, seeds, environment, durations, hashes, outputs, and artifact paths
belong in the implementation record.

## Completion criteria

The magic sequence is complete only when:

- every requirement from goals 1–8 is proven in the audit matrix,
- fresh survival progression reaches complete magic without a unique drop,
  secret recipe, debug state, class, or another player,
- planetary magical geography/ecology are causal, finite, and seam-safe,
- charms, wands, workings, rituals, and preparations are useful and fully
  accounted,
- no magic creates/transmutes ordinary matter, duplicates resources,
  teleports, or replaces logistics/industry,
- Current, water, salt, and tracked material audits have zero unexplained
  delta after every integrated scenario,
- careful practice is sustainable, abuse is consequential, warnings are
  legible, and recovery is proven,
- Ire, dross, and environmental state remain distinct but interact through
  actual actions,
- construction permanence holds,
- dedicated multiplayer, agents, mods, saves, crashes, and hostile clients are
  qualified,
- performance budgets and long simulations pass,
- final art, audio, language, UI, docs, and originality review pass,
- no placeholder/debug/unaccounted shipping path remains.

Do not mark the sequence implemented because each document has an
implementation note. Goal 9 is complete only when the integrated evidence
proves the practice that the documents describe.
