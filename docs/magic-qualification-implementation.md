# Magic qualification implementation record

Status: implemented and production-qualified 2026-08-04.

This record is the row-level final evidence referenced by
`docs/magic-qualification-matrix.csv`. The matrix has 1,203 requirement rows
from the master plan and goals 1–8. `tools/magic_qualification_matrix.py`
regenerates it from the source documents and refuses to run if its named
behavior tests disappear. A matrix row is not treated as proven merely because
the implementation text exists: the evidence below records the assertions that
were inspected and the runtime gates that were executed.

## Constitutional result

Wildforge now has one integrated, finite magical practice:

- Current is a conserved fixed-point planetary resource. It moves among Deep,
  Ambient, Bound, Active, Dross, and Scar custody; ordinary play cannot create
  an external adjustment.
- Ire is relational and Dross is physical waste. They interact only when an
  attributed act actually harms or tends life, and all four high/low states
  persist independently.
- Magic may observe, illuminate, ignite existing fuel, apply bounded physical
  impulses, advance viable growth, move real water, repair with matching
  matter, slow change, route charge, and protect a supplied boundary. It may
  not conjure, transmute, teleport, reveal private/global state, or silently
  delete a cost or residue.
- Recipes are public. Ruin charms and maker records are useful historical
  shortcuts, not unique progression gates.
- Construction remains construction. Dross stalls life, manifests in bounded
  nonstructural sites, and imposes status/effect pressure; it does not rewrite
  player structures or inventories.

The production command is:

```sh
wildforge --magic-qualification <world> [--mods <directory>] [--output <report.txt>]
```

It exits nonzero on invalid content, a corrupt/missing sidecar, any unexplained
Current/water/salt/material delta, orphan custody, unqualified subsystem, or a
nonzero Current operator adjustment. This is an offline operator command. It
is not exposed to survival clients or agents.

## Fresh survival progression

The final route is a composite end-to-end gate because each step uses the real
subsystem transaction while the route test verifies there is no hidden item
gate. `fresh_survival_magic_route_is_public_solo_and_has_no_unique_gate`
inspects every required ordinary and magical output recipe, all three
craftable charm blanks, twelve workings, eight preparations, renewable
wellglass, and the four finite resonant minerals. It runs alongside
`content_graph_is_complete_and_obtainable`, which recursively proves the base
content graph has an obtainable production path.

The actual step behavior is covered by:

- `homeland_census_installs_three_redundant_observational_sites_once`,
  `the_agent_earns_and_reads_a_host_signed_magic_observation`, and
  `signed_records_copy_with_optional_location_and_survive_reload` for ruin and
  no-ruin discovery, lens calibration, records, redaction, copying, and folios;
- `every_natural_site_obeys_ordinary_and_magical_habitat`,
  `crystal_harvest_is_exact_seed_preserving_and_single_shot`,
  `simultaneous_wellglass_harvest_resolves_once_and_both_custodies_balance`,
  and `resonant_mineral_break_reconciles_physical_and_arcane_ledgers_together`
  for renewable and finite resources;
- `embodied_frame_calibrates_assembles_saves_disassembles_and_dismantles_conservatively`,
  `every_charm_binds_from_physical_reagents_and_recharges_from_a_vessel`, and
  `wand_strain_failure_and_fire_lava_despawn_dispositions_are_conservative`
  for every implement lifecycle;
- `base_roster_is_eight_wand_workings_and_four_rituals` plus the per-handler
  effect tests in `src/tests/workings.rs` for every working and rite;
- `every_base_preparation_completes_through_its_real_apparatus_sequence` and
  the per-preparation tests in `src/tests/alchemy.rs` for the complete physical
  laboratory; and
- `scenario_careful_homestead_stays_below_seep_for_two_years` and
  `ashlace_wash_moves_bounded_dross_into_one_recoverable_sludge` for routine
  safe practice and physical waste capture.

No step requires an admin command, debug atlas, class, unique drop, second
player, or undisclosed recipe. Manual presentation was checked in the native
GPU capture described below.

## Four-identity society

`four_identity_magic_society_shares_one_unprivileged_host` connects SURVEYOR,
CULTIVATOR, BINDER, and APOTHECARY simultaneously to one dedicated host. It
asserts four distinct durable PlayerIds/principals and an ordinary shared chat
channel. Role actions then use the same host boundary in the discovery,
implements, workings, alchemy, and remediation agent scenarios:

- signed observations and optional location redaction remain physical records;
- stable item ids keep charge cargo physical through inventory, containers,
  cargo, death, save/load, and reconnect;
- stale/concurrent frame, working, alchemy, dumping, and cleanup requests
  serialize once;
- Ward Boundary requires an intact closed supplied perimeter, charges Ire for
  resisted attributed pressure, and stops when broken or empty;
- public clients receive qualitative local cues but no global audit or exact
  private provenance;
- moderation and working/alchemy/Dross history use the authoritative PlayerId,
  actor label, and installation id; and
- cleanup remains available after the source actor disappears because custody
  belongs to the world, carriers, apparatus, and recoverable physical items.

This proves technical support for specialization, trade, conflict, evidence,
and recovery. It does not claim that unobserved players will form a polity.

## Careless-use and recovery campaign

The campaign is deliberately assembled from deterministic real transaction
tests so each failure can identify the ledger it touched:

- mismatched parts and forced overdraw:
  `strain_is_deterministic_monotonic_and_visibly_banded` and
  `forced_wand_draw_crosses_the_safe_floor_visibly_and_is_audited`;
- poor/damaged containment:
  `visible_apparatus_damage_increases_strain_and_eventually_refuses`,
  `damaged_apparatus_requires_matching_matter_and_hammer_to_repair`, and
  `bounded_maintenance_is_round_robin_and_damaged_apparatus_leaks_legibly`;
- air, water, and soil release:
  `provenance_transport_conserves_units_and_degrades_evidence`,
  `water_routes_move_the_exact_proportion_without_moving_water_mass`, and
  `scenario_watershed_dispute_moves_pollution_and_degrades_a_useful_signature`;
- warnings, scar, and controlled breach:
  `scenario_careless_laboratory_warns_through_every_band_before_breach`, which
  asserts Trace, Strained, Seep, Scar, Breach Risk, forecast, then breach;
- permanence and custody:
  `scar_manifestation_preserves_structure_persists_once_and_excavates_exactly`,
  `scar_burden_stalls_cultivation_without_deleting_block_or_metadata`, and the
  integrated audit;
- treatment and recovery:
  `ashlace_wash_moves_bounded_dross_into_one_recoverable_sludge`,
  `scenario_dead_heart_recovery_remains_possible_and_reawakening_accelerates_it`,
  and `scenario_planetary_abuse_is_regional_not_self_replicating_and_recovers_when_stopped`.

The abuse fixture funds 54,000 Current units into three source cells, reaches
regional damage in 22 simulated hours, stops every source, and returns every
cell below Scar by hour 5,000 without a new breach clock. The careful fixture
uses two units per day for two years and never reaches Seep. The final century
gate funds 1,200 monthly practice sessions and stays below Seep, while the
stopped three-source continuation remains exact through 876,000 hours. This is
the measured prevention/recovery comparison: stable matched use needs no scar
labor; abuse needs source shutdown, containment, physical excavation/filter
media, and a long regional recovery continuation. Habitat loss—not raw Dross
arithmetic—is the only route from this campaign to Ire.

## Cross-ledger production audit

Seed/save: `saves/magic-dross-smoke`, production atlas side 256 per face,
393,216 coarse cells, production content set plus `mods/gems`.

The 2026-08-04 final command completed in 49.51 seconds at 770,508 KiB peak RSS and
produced the report later checked into
`docs/magic-qualification-report.txt`:

| Ledger | Expected/genesis | Accounted | Unexplained |
|---|---:|---:|---:|
| Current | 1,610,612,736 | 1,610,612,736 | 0 |
| Water | 4,716,702,572,299 HU | 4,716,702,572,299 HU | 0 |
| Salt | 916,212,404,381,888 | 916,212,404,381,888 | 0 |
| Tracked materials | per-material manifest | manifest + named external inheritance | 0 for every material |

Current operator adjustment was zero. Geography held exactly 1,409,286,144
units and matched its ledger account. The ecology audit found 676 sites, seven
required roles, nine physical habitat expressions, and 12,931 units of living
custody. Discovery, implements, workings, alchemy, Dross, water/salt, and
materials all reported qualified. Dross contained 576 known units in three
archaeological owners and had no unexplained delta.

The combined audit covers parent-ledger closure while arc-specific tests cover
every mutation class: growth/harvest/fire, wardens, implements and loss,
working cancel/crash/replay, Draw/rooting water, alchemy lifecycle, Dross
transport/scars/treatment, entity/container destruction, content removal and
retrogen, and atomic save recovery.

## Long simulation and determinism

- Water/climate: seed 6006, side 2, 200 years/1,752,000 hours in 24.28 seconds.
  Water remained 1,034,039,254,496 HU and salt 227,481,264,750,976 with both
  unexplained deltas zero at years 1, 10, 100, and 200. Surface storage rose
  and fell seasonally, aquifers and reservoirs remained nonzero, redistribution
  stayed under 0.1%, and drift did not accelerate.
- Magic: seed `0xd2055008`, side 2, 100 years/876,000 hours for untouched,
  careful, and stopped-abuse paths. Exact combined Ambient/Dross custody stayed
  4,320,000 units. Careful use stayed below Seep; abuse remained regional and
  converged after shutdown.
- Geography: seed `0x105eed`, 1,000 randomized full transport/deep-exchange
  steps retained scalar and every resonance total.
- Ecology: unloaded succession retained exact Current; the production ecology
  day reconciled all 676 sites.
- Dross: ten years of dense recovery retained exact custody, cleared the source
  cell, and kept provenance/events within hard bounds.
- Slicing/order: whole/sliced geography, whole/sliced water, Dross reload
  checkpoint, chunk generation order, ecology slices, and adopted worker chunk
  tests compare complete state or authoritative hashes, not approximate totals.

## Spatial matrix

`cargo test --locked seam -- --nocapture --test-threads=1` passed 29 tests in
9.25 seconds. `cargo test --locked corner -- --nocapture --test-threads=1`
passed 5 tests in 1.54 seconds. Together with every-directed-edge tests they
cover canonical topology, Current/Dross transport, air/water routes, climate,
hydrology, ecology geography, mineral strata, heart association and bearings,
machinery/conductors, movement/raycasting, multiplayer streaming, light/fire/
fluids, and rendered shared corners. The assertions compare reciprocal edge
geometry, conserved totals, canonical coordinates, state on both sides, and
complete corner resolution; no test merely checks that a call returned.

## Abuse, balance, authority, saves, and mods

The abuse matrix is closed by the inspected tests in the owning modules:

- implements: split/merge identity, stale ids, recharge, overfill, network
  cycles, simultaneous requests, fire/lava/despawn/death, removed content;
- workings: active disconnect/death/crash, Draw extremes, Rootwake missing
  inputs, Fieldmend wrong matter, Kindle unsupported targets, ward break/unload,
  target range/occlusion, and exactly-once replay;
- alchemy: exact fractional splits, overfill, spoilage/refresh, incompatible
  effects, block break, status death/reopen, and client-authored chemistry;
- Dross: concurrent dump/cleanup, concurrent excavation/treatment, save-boundary
  transport, bounded provenance, carrier partitions, and scar persistence;
- content: unknown/raw handlers and spawn, transmute, delete, scan, teleport,
  free sink/growth, duplicate harvest, overflow, and live identity changes all
  fail registration or one atomic transaction without partial mutation.

`every_magical_route_keeps_a_bounded_niche_below_bulk_technology` measures the
ten comparison axes in the qualification plan. Technology wins reliable bulk
light, ignition, transport, irrigation, repair, preservation, protection, and
production. Magic retains portable/local precision and hostile-condition
niches. Component census tests prove no linear best set; site census proves
basic magic on all major continents with regional specialities but redundant
progression.

Protocol 40 keeps the dedicated host authoritative. Join/reconnect, local
interest, private records, stable ids, and transactional concurrent actions
are covered by multiplayer and arcane-interest tests. Agents use perceived
state and ordinary verbs for discovery, binding, workings, alchemy, and Dross;
they receive no global atlas, exact hidden burden, private provenance, or
privileged target endpoint.

The qualified formats are Arcane 1/algorithm 1; Geography 1/algorithm 2,
dynamic 1, journal 1; Ecology 1; Discovery 1; Implements 2/resolver 1;
Workings 1/definition 1; Alchemy 1/definition 1; Dross 1; and protocol 40.
Migration tests cover pre-ecology geography, pre-Dross and pre-alchemy owner
discriminants, legacy charm funding, removed resonance/content, mod removal,
retrogen, backup recovery, and unknown placeholders without invented Current.

Valid fixture mods cover a resonance/site, plant, finite mineral, growing
crystal, wand component/charm, approved working, preparation/residue chain, and
scar lifecycle. Invalid fixtures cover free/unbounded growth, duplicate
harvest, raw effects, unsafe structures, missing physical residues, forbidden
handlers, overflow, raw Current credit, and unaccounted transformation.

## Performance budgets

Baseline: Intel Core i7-14700K under Fedora WSL2 for authoritative tests; RTX
3090/DX12 for native rendering. Constrained target: CPU affinity restricted to
one logical CPU (`taskset -c 0`) and the production 4,096-cell work slice.

| Operation | Measured | Production gate |
|---|---:|---:|
| Arcane ledger audit | 80.431 us | < 1 ms |
| Arcane transaction | 7.285 us | < 100 us |
| Geography loaded state | 30 MiB | < 64 MiB |
| Geography full pass | 134.805 ms baseline; 128.942 ms constrained | < 20 s |
| Geography worst 4,096-cell slice | 7.354 ms baseline; 5.941 ms constrained | < 25 ms |
| Ecology day, 676 sites | 36.311 ms baseline; 33.854 ms constrained | < 20 s |
| Ecology worst 512-site slice | 0.353 ms baseline; 0.351 ms constrained | < 25 ms |
| Idle server tick, 25 chunks | 4.988 ms | < 16.67 ms |
| Unified offline audit | 49.51 s, 770,508 KiB RSS | < 60 s, < 1 GiB |
| Max projected Dross save | 81 MiB | < 96 MiB |
| Dense Dross + routes | 39 MiB | < 64 MiB |
| Dross client cue | 21 bytes | bounded reliable message |
| Dross server slice | 0.124 ms | < 1 s test ceiling |
| Native integrated frame | 59 FPS; 7.27 ms sim; 6.76 ms draw | >= 30 FPS; sim/draw each < 16.67 ms |

The production qualification save is 266 MiB including atomic backups; the
largest immutable genesis file is 131,727,392 bytes. Dynamic magic uses bounded
coarse arrays and indexed owner/site maps. Ordinary no-magic ticks do not scan
all chunks or charged objects, clients receive only interest-managed local
cues, histories have hard caps, and overloaded work is sliced or sheds visual
density before authoritative state.

## Presentation, accessibility, and originality

Native capture command environment: Windows release binary, world
`magic-discovery-final`, `WILDFORGE_DEMO_ALCHEMY=1`,
`WILDFORGE_TIME=0.22`, `WILDFORGE_HELD=base:glass_bottle`, and
`WILDFORGE_SHOT_MIN_FRAME=180`. Artifact:
`C:\Games\Wildforge\screenshots\magic-qualification-alchemy-final.png`.
The RTX 3090/DX12 frame settled after 16 clean frames with 138 resident and 138
GPU chunks. Manual inspection confirmed the physical mortar, infusion basin,
alembic, filter stand, adjacent conductors, heat, cooling stock, bottle, held
item text, and ordinary landscape. Earlier goal records retain the implements,
workings, discovery, geography, ecology, and Dross captures and their exact
settings.

A second native run added `WILDFORGE_JUICE=0` and
`WILDFORGE_SHOT_TOASTS=1`. Its artifact,
`C:\Games\Wildforge\screenshots\magic-qualification-reduced-effects.png`,
settled at 59 FPS with a 6.68 ms simulation and 7.64 ms draw. Manual inspection
confirmed the apparatus and qualitative state text remained legible without
cosmetic particles. The standard capture was also converted to
`magic-qualification-monochrome-check.png`; grayscale inspection retained the
apparatus silhouettes, held-item label, terrain separation, and state text.

Content tests assert real textures and drawable labels for every shipped base
and mod entry, component-derived local/remote models, qualitative tooltips,
accessible text for every Dross forecast/breach, non-color state cues, bounded
particles, current ambience, working sounds, reduced-effects behavior, and
local/remote replication. Nine habitat expressions cover more than the six
required climate-distinct ecologies; site, plant, mineral, growth, depletion,
working, ritual, preparation, and scar presentation is driven from the same
authoritative state tested above.

The final vocabulary and whole-system review found no borrowed proper names,
copied prose, copied interface, or one-to-one recreation of another game's
research/taint expression. Current, Dross, Wellglass, Heartshadow, Wake,
Ashlace, Rainbell, Hushwood, Stormvine, Cairnbloom, Pilgrim Root, Lantern Reed,
Nightglass, Frostlace, Tidekelp, Echo Cap, and the preparation/working names are
Wildforge's own combination. Generic wand, charm, ritual, alchemy, resonance,
and scar vocabulary remains where it describes a generic fantasy object. This
is an internal originality review, not a legal opinion.

The result reads as Wildforge: subdued supernatural signs embedded in finite
geology and ecology, real workshops and cargo, consequences that travel through
water and society, recoverable landscapes, and a Wild that remembers actual
conduct.

## Repository evidence

The final branch must retain green results for:

```sh
python3 tools/magic_qualification_matrix.py --output docs/magic-qualification-matrix.csv
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
cargo build --locked --release --target x86_64-pc-windows-gnu
cargo deny check advisories
```

Goal-specific executed commands, seeds, durations, hashes, values, and artifact
paths are recorded in the sections above. The final CI check URLs and merge-base
commit belong in the pull request, because they are external state rather than
stable repository documentation.
