# World entry and cold-streaming remediation

> **Status: implemented and production-qualified (2026-08-01).**
>
> Drafted 2026-08-01 from a live production-planet smoke test. This is a
> discovered prerequisite to `docs/planetary-qualification-plan.md`, not a
> new world-generation feature arc. Complete and qualify it before final
> planetary qualification or the magic sequence.

## Implementation record

The implementation uses world generator version 9, network protocol 24,
spawn-manifest version 1, and spawn-verification contract version 1. The
following contract is current code, rather than illustrative design:

- production world creation writes the atlas and qualified homeland into a
  hidden temporary directory and publishes the world with one directory
  rename only after both are complete;
- the common homeland is a persisted radius-two `5 x 5` region (25 chunks),
  while each solo or network admission requires the exact radius-one `3 x 3`
  safety set around that player's actual position;
- existing solo worlds load and validate both sets on a cancellable background
  task before constructing the player session;
- dedicated guests remain pending connections while their safety chunks are
  loaded or generated, then remain non-actors while the exact manifest is
  transferred and decoded;
- host terrain load/generation uses two to four priority workers with at most
  32 jobs in flight, adoption is capped at two chunks or 4 ms per pump, wire
  encoding uses two workers, and transfer is paced at two chunks per guest per
  pump;
- pending and pre-entry guests are excluded from movement, survival, damage,
  sleep, snapshots, ecology interest, gameplay broadcasts, rendering, and the
  active roster;
- the agent and graphical client do not enter the world on `Welcome`; they
  enter only after the manifest is satisfied and the host sends
  `EntryAccepted`;
- the MCP face schema uses the canonical snake-case names, and idle agents
  apply ordinary swim input instead of sinking unattended.

Network-only WFC9 payloads now include the host's exact RLE block-light and
skylight arrays; saved chunks stay WFC8 and continue to omit derived light.
Graphical guests batch up to eight received chunks into a single adoption,
pace adoption across frames, and mesh two read-only chunks concurrently. The
renderer waits for an in-view neighbor but treats the negotiated outer ring as
air, so a finite circular view neither leaves permanent dirty chunks nor
remeshes its frontier forever. These changes preserve view distance, terrain,
lighting, fluids, and simulation features.

Seed `20260801` has completed a cold production homeland preparation, warm
dedicated restart, and protocol-24 agent admission. A post-fix 142-second live
run admitted `ENTRYSCOUT3`, perceived the qualified doorstep, and walked from
`pos_z 1616,82,3312` to `1620,81,3312`. The measured dedicated-host peak RSS
was 478,024 KiB and the agent at its full ten-chunk view peaked at 159,188 KiB.
A live weather hour exposed separate saturated-transport and sliced-weather
detail-inbox defects. Both are fixed: the evolved production save has accepted
44 hours and one groundwater day with zero water/salt drift, the isolated
12-hour production probe closes exactly, and the cold-streaming server probe
closes ten more hours exactly.

The final remote view-distance-12 capture entered and settled in 30.10 seconds
at 914,340 KiB peak guest RSS, down from a pre-fix run that exceeded 180
seconds. Its settled frame held 518 resident chunks, 498 GPU meshes, 226
water-bearing meshes, and 20 prepared chunks outside the granted view; steady
simulation cost was 1.12 ms and draw cost 115.71 ms. A resident seam fixture
settled in 9.5 seconds. A completely cold natural-oasis view-distance-12
expansion took 72.92 seconds while remaining responsive; this is wider-view
materialization, not admission. The dedicated host peak was 478,024 KiB and
the full-view agent peak was 159,188 KiB. Corrected solo, remote, seam-building,
seam-water, ocean-horizon, and oasis captures are validated by
`screenshots/planetary-qualification.toml`.

## Purpose

Wildforge already creates the finite planet's global atlas before voxel
terrain. It does not yet have a coherent step between "the planet exists" and
"a person can safely enter it."

Solo play synchronously generates a small spawn neighborhood before entering
the session. Dedicated play admits a guest at a fixed planet-face center and
then synchronously generates hundreds of requested chunks from the host's
network pump. These paths disagree about where the world begins, what must be
ready, and which thread may do expensive work.

This goal creates one shared world-entry pipeline for solo players, windowed
hosts, dedicated hosts, graphical guests, and headless agents. It also closes
the two agent-contract defects found by the same smoke test.

The player-facing rule is:

> The planet is planned globally, a viable doorstep is prepared locally, and
> only then does a player enter. The rest of the finite planet materializes
> lazily without stopping the world.

Finite does **not** mean eagerly materializing every voxel. Six `8192 x 8192`
faces contain 1,572,864 surface chunk columns. Building all of them before play
would discard the atlas/lazy-chunk architecture and create an unacceptable
startup, storage, and retrogen burden.

## Live failure record

The 2026-08-01 test used a release build, seed `20260801`, a fresh production
planet, a dedicated server, the built-in MCP agent, and a windowed guest.

The following passed:

- all four agent loopback integration tests,
- all production planet-creation stages,
- save and dedicated-server restart,
- MCP perception, inventory, status, event, and exact block queries,
- land walking, water entry, return-to-shore pathfinding, and reconnect.

The production path exposed these failures:

| Failure | Observed result | Cause at discovery | Implemented resolution |
|---|---|---|---|
| Dedicated fresh spawn is not qualified | The player appeared on a tiny manufactured sand island in open ocean; there was no tree within 48 blocks | `HostSession` asked `safe_spawn_at` to rescue the center of `PosZ` instead of using solo's atlas-aware spawn selection | All modes use the persisted atlas- and voxel-qualified common homeland. |
| Cold join starves the host | The host consumed roughly one CPU core, stopped answering for long intervals, and kept working after a guest disconnected | `stream_chunk` called `World::ensure_chunk` synchronously, eight chunks per guest per pump | Bounded priority workers perform cold reads/generation and encoding; the host only adopts completed work within its pump budget. |
| A normal view request becomes an admission burst | The agent requests radius 10 and a graphical client radius 12, asking for hundreds of cold chunks immediately | Entry readiness and desired horizon were the same operation | Admission uses an exact 3×3 manifest; the wider view starts afterward and expands near-first at two chunks per pump. |
| Graphical join fails without terrain | The host authenticated the guest, but the client returned to the title before receiving a playable neighborhood | There was no preparation/progress/entry-ready protocol state | Protocol 24 has progress, manifest, client acknowledgement, and host acceptance states with explicit failure text. |
| Solo entry looks hung | Loading the same saved planet spent more than four minutes and about 1.1 GiB before producing a frame | `start_world` synchronously generated nine chunks before creating its existing worker pool | World opening and saved-doorstep loading run on a cancellable background task with visible progress. |
| Idle agents sink | An idle agent descended from the water surface to the seabed, although it could path back to shore | Idle input never swam upward, despite the competence layer treating water as a standable state | Idle self-preservation now applies normal upward swim input while unsupported in water. |
| MCP face names contradict their schema | Advertised `PosZ` was rejected while internal `pos_z` worked | Schemas/errors used Rust variant spelling; `Face::from_name` accepted canonical snake case only | Schemas advertise the six canonical names and the parser temporarily accepts legacy aliases. |

The prebuilt agent fixtures did not catch the production failures because their
small terrain rings already existed before the guest connected.

## Non-negotiable invariants

1. Solo, windowed host, and dedicated host use the same spawn selector,
   preparation job, chunk adoption path, and readiness definition.
2. No network/simulation pump synchronously generates terrain, reads a cold
   chunk from disk, or compresses a cold chunk for the wire.
3. A player is not simulated, damageable, hungry, drowning, visible on the
   active roster, or allowed to move until its entry neighborhood is present.
4. Requested view distance is a streaming target, never an admission
   prerequisite.
5. Guests and agents still never generate authoritative terrain.
6. Chunk results are deterministic and independent of request, worker,
   connection, and adoption order.
7. Every committed chunk performs water and finite-material accounting
   exactly once. Rejected spawn candidates perform neither.
8. Prepared terrain never overwrites a saved or player-modified chunk.
9. Normal world creation does not fabricate a rescue island. A naturally
   isolated island may qualify; sterile ocean does not.
10. Cancellation, disconnect, failure, and retry leave no half-admitted player,
    phantom ledger reservation, or corrupt preparation manifest.
11. Reducing default view distance is not the fix. Wide views must be paced and
    responsive too.

## World-entry lifecycle

There are two related readiness gates.

### World readiness

This happens once when a world is created and is cheaply revalidated on boot:

```text
global atlas complete
        |
select qualified spawn from atlas
        |
generate candidate entry region off-thread
        |
verify voxel terrain without committing side effects
        |
adopt + account + persist winning region
        |
publish world as ready
```

A new world does not appear as playable in world selection and a dedicated
host does not advertise/listen for players until this gate succeeds. The UI
and headless console show its named stages and progress.

### Player readiness

This happens for every connection because a returning player may be far from
the common spawn:

```text
transport + identity authenticated
        |
content policy resolved
        |
profile position selected
        |
local safety region loaded/generated by shared scheduler
        |
Welcome metadata + entry chunks transferred
        |
client confirms the required chunks are decoded
        |
player becomes active
```

The common spawn is normally already prepared. A returning location may need a
smaller safety preparation. Pending players are connections, not world actors.
They do not count for sleep, roster presence, simulation interest, snapshots,
or ecology.

## Qualified common spawn

Move the spawn selector out of `game` into a renderer-independent world-entry
module. It consumes the immutable atlas and produces an ordered deterministic
candidate list over the whole planet, not only a bounded walk around the
center of `PosZ`.

### Atlas-level hard requirements

A candidate must be:

- dry ground above normal tide/flood variation with a safety margin,
- outside ocean, lake, permanent wetland, and impassable peak cells,
- on terrain whose estimated local relief permits a safe surface,
- in a habitat with meaningful vegetation potential and fertility,
- within configured travel distance of drinkable water,
- within the existing strategic prospecting distance of both copper and tin,
- outside a heart, mandatory edifice footprint, or other protected genesis
  site,
- connected to enough traversable land to found a settlement.

Cube-face seams are valid geography and must not be penalized or skipped.

### Scoring, not abundance guarantees

Among passing candidates, prefer:

- nearby wood and food habitat,
- reliable fresh water rather than only rainfall,
- moderate slopes and a mix of soil, exposed stone, and buildable ground,
- access to more than one biome or travel corridor,
- a useful but not resource-saturated copper/tin hinterland.

The doorstep is a viable homeland, not a starter chest or a promise that every
ore lies underfoot. Regional scarcity and journeys remain part of the game.

### Voxel-level verification

Generate each candidate's trial region as pure chunk values before adoption.
The winning region must prove:

- a dry two-block-high spawn cell with stable solid support,
- a contiguous walkable patch around it rather than one pillar or overhang,
- no immediate lava, deep fall, suffocation, or unavoidable flood hazard,
- at least one reachable `#base:logs` block within 48 blocks,
- reachable drinkable water within 96 blocks,
- enough local surface diversity to obtain ordinary soil, stone, and plant
  material,
- consistent seams and neighbor borders across the prepared region.

If a candidate fails, discard its pure results and try the next candidate.
Discarding must not seed ruins or animals, reserve ore, commit water, or write
region files.

Candidate attempts are bounded. If a new seed has no qualifying candidate,
planet creation follows the existing deterministic seed-retry/failure policy
and reports the failed requirements. It does not silently raise sand in the
ocean. An operator repairing an old or corrupt save must use an explicit
repair operation whose terrain changes are audited.

## Prepared entry region

The initial fixed contract is a square radius of two chunks around the common
spawn: `5 x 5`, or 25 chunk columns. This leaves a fully neighbored inner
`3 x 3` region for meshing and gives the player roughly forty blocks in every
local direction before reaching the preparation frontier.

`ENTRY_RADIUS_CHUNKS = 2` may change only with recorded measurements and with
the wood/water/walkability qualification still passing. It is deliberately
independent of a client's view-distance setting.

Once a candidate wins:

1. adopt each chunk through the normal `World` path,
2. perform structure, ecology, water, and material commitments once,
3. force-save all entry chunks using the canonical region format even if they
   contain no player edits,
4. write the preparation manifest atomically only after every required chunk
   and ledger component is durable,
5. run water and finite-material audits before publishing readiness.

Persisting this bounded region is intentional. Rebuilding untouched entry
chunks on every restart recreates the cold-start failure and can repeat
creation-time side effects. This is not a general cache of the whole planet.

The world stores a versioned `spawn.toml` (or equivalently versioned world
metadata) containing:

- topology, generator, preparation-format, and atlas identity versions,
- seed and creation-time generation-content identity (block names and data
  that can affect genesis, not texture-pack presentation),
- canonical surface and entity spawn positions,
- prepared radius and exact chunk keys,
- verification results and their contract version,
- a digest over the prepared chunk records,
- completion timestamp for operator diagnostics only.

Version/generation-content disagreement never silently relocates a live
world's spawn or overwrites saved ground. Boot either revalidates compatible
saved chunks or reports that an explicit spawn-repair/retrogen step is
required. A texture-pack-only change does not invalidate preparation.

## Shared asynchronous chunk pipeline

Extract the pure worker machinery currently owned by `game::streaming::GenPool`
into a renderer-independent service usable by solo sessions and headless
hosts. The authoritative `World` remains owned by the simulation thread.

```text
requesters -> bounded priority/dedup queue -> pure generation/load workers
                                                   |
simulation thread <- budgeted adopt/commit queue <-+
        |
resident chunk -> bounded encode queue -> one or more waiting guests
```

### Work classes, highest priority first

1. world-spawn preparation,
2. pending player's collision/safety region,
3. active player's center and immediate movement frontier,
4. explicit repair for a missing client chunk,
5. ordinary near-to-far view expansion,
6. speculative/background horizon work.

One chunk key has one in-flight load/generation job regardless of how many
players want it. Waiters attach to the same result. Priority may be promoted
but work is never duplicated.

### Worker and main-thread responsibilities

Workers may:

- read/decode a saved chunk,
- run deterministic pure terrain generation,
- compute immutable verification summaries,
- encode an immutable resident snapshot for network transfer.

The simulation thread alone may:

- choose whether a saved chunk wins over a generated candidate,
- adopt into `World`,
- seed stateful structures/ecology,
- reserve or transfer water/material ledgers,
- wake seams and simulation state,
- publish the chunk as authoritative.

Adoption is time-sliced during a live session. World creation may drain the
queue faster because simulation has not begun, but UI/network progress must
remain responsive.

### Backpressure and cancellation

- Cap global in-flight generation by measured memory, not CPU count alone.
- Cap pending requests per guest to its granted view and drop superseded far
  requests when it moves.
- A disconnected guest loses its waiter entries; shared work needed by other
  guests continues.
- A cancelled world-preparation job discards unadopted results and removes its
  incomplete temporary manifest.
- A requested chunk is recorded as sent only after its reliable message is
  accepted by the transport queue.
- Resident eviction never removes a chunk with an adoption, encoding, or
  entry-readiness dependency.

The host pump may enqueue, drain already-completed results within a time
budget, and send already-encoded payloads. It may not call
`World::ensure_chunk` as a fallback.

## Network and client state

Protocol 24 implements an explicit pre-entry state. The authoritative message
sequence is:

```rust
S2C::EntryProgress { resident, total } // repeat/heartbeat before Welcome
S2C::Welcome { /* existing metadata */ }
S2C::EntryManifest { spawn, required }
S2C::Chunk { /* one of the exact required chunks */ }
C2S::EntryReady                         // client decoded the exact set
S2C::EntryAccepted                      // host activates the player
```

The authenticated connection stays alive while preparing. The host sends a
progress update or heartbeat at least once per second, so a slow but healthy
preparation is distinguishable from a dead server.

The graphical client remains on a cancellable loading screen after `Welcome`.
It sends `C2S::EntryReady` only when every manifest chunk is decoded and the
center mesh has the four resident horizontal neighbors needed for a valid first
frame. The host then marks the profile active and answers
`S2C::EntryAccepted`; only that message enters `Screen::Playing`. Failure
returns to the join screen with the actual reason; it never silently returns
to the title.

The agent prints preparation progress to stderr and does not expose movement
tools as usable until its world mirror satisfies the entry manifest.

After `EntryAccepted`, clients request their configured wider view near-first.
Changing view distance reprioritizes the bounded queue; it never creates a
synchronous burst.

This protocol change requires the normal protocol-version bump and a clear
incompatibility refusal for old clients. Do not attempt to infer readiness from
timing or from receipt of one center chunk.

## Dedicated, solo, and saved-player behavior

### New solo world

Planetary atlas creation flows directly into the shared common-spawn job. The
creation screen adds `SELECTING HOMELAND`, `PREPARING HOMELAND`, and
`VERIFYING HOMELAND` stages. The local player is constructed only after the
manifest commits.

### Existing solo world

Load and validate the common manifest first. A saved local position uses the
same per-player safety job as a returning network guest. The current synchronous
nine-chunk loop in `game::session::start_world` is removed.

### Dedicated host

Load/validate the common prepared spawn before opening the listening endpoint
or LAN beacon. Headless progress goes to stderr. The host's `fresh_spawn` comes
from the persisted common spawn, never a hard-coded face center.

### Windowed host

Opening an already-running local world to friends reuses its resident scheduler
and prepared common spawn. It does not create a second generation path.

### Returning player

Prefer the valid saved position. Prepare/load a small safety region around it
before admission. If terrain or format changes make it unsafe, try the saved
respawn point and then the common spawn, recording the reason. Do not fabricate
terrain merely to preserve an invalid coordinate.

A saved mid-swim position may remain a swim only after its water neighborhood
is loaded and the client can provide input. Pending players are not advanced by
gravity or survival ticks.

### Respawn

The common spawn is always resident or highest-priority before respawning a
player. Death never exposes the cold-generation path as a black screen or fall
through missing ground.

## Agent contract repairs

### Water-safe competence

`Idle` means no assigned horizontal task, not passive suicide. When an agent is
unsupported in water, its competence layer applies ordinary player swim input
until its head is clear, then maintains the local surface. It receives no
buoyancy or teleport unavailable to players.

Path planning prefers a surface-swimming route unless the requested goal is
explicitly underwater. `stop()` cancels the task but retains self-preservation.
Future explicit diving work may suppress surface seeking while tracking air;
that is not required here.

### Canonical MCP coordinates

The public canonical face names are the values returned by `Face::name()`:

```text
pos_x, neg_x, pos_y, neg_y, pos_z, neg_z
```

Every coordinate tool schema uses a JSON Schema string `enum` containing those
six values. Coordinate fields are `integer` with explicit `minimum` and
`maximum`, not unconstrained `number` values parsed later as integers.

Because the shipped schema advertised Rust-style names, input accepts
`PosX`/`NegX`/`PosY`/`NegY`/`PosZ`/`NegZ` as compatibility aliases for one
protocol generation. Output, documentation, examples, events, and new saved
text remain canonical snake case. Errors report the same accepted values as
the parser.

## Operator visibility and failure handling

Both the loading UI and headless console expose:

- preparation stage and completed/total chunks,
- selected candidate number and rejection reason summary,
- queued/in-flight/ready chunk counts,
- generation, adoption, encoding, and transfer timings,
- resident chunk count and estimated pipeline memory,
- pending versus active player counts.

Normal logs report stage transitions rather than one line per chunk. A verbose
diagnostic mode may emit per-chunk timings.

Failures are explicit:

- no qualified spawn: world creation fails or deterministic retry begins,
- corrupt/incompatible spawn manifest: world is not advertised; operator is
  told how to inspect or explicitly repair it,
- generation worker failure/panic: pending entry fails without admitting the
  player,
- client disconnect/cancel: its pending state and unique work are reclaimed,
- transfer timeout after progress stops: client reports the last stage.

An operator command or headless diagnostic must validate/rebuild preparation
for an existing world without admitting players. Any repair that changes
terrain first creates recoverable backups and records water/material audit
results.

## Performance budgets

Record the review machine, worker count, seed, view distances, wall time, peak
RSS, and percentile method in the implementation header. These initial budgets
are qualification gates, not claims about the current build.

| Operation | Required budget |
|---|---|
| Warm boot of an already prepared spawn, after atlas load | <= 5 s |
| Cold preparation of the 25-chunk entry region, after atlas load | <= 30 s |
| Preparation progress/heartbeat interval | <= 1 s |
| Host pump while cold chunks are requested | p99 <= 10 ms; no sample above 25 ms due to chunk work |
| Resident prepared-spawn join to `EntryReady` on loopback | <= 5 s |
| Additional peak RSS during entry preparation | <= 512 MiB |
| Pending work after the only requester disconnects | reclaimed or cancelled within 2 s |

If production generation cannot meet the entry budget, optimize and measure
the generator/adoption path. Do not weaken readiness, restore synchronous
generation, or hide the wait behind a longer timeout.

## Tests and qualification

### Deterministic spawn tests

- The same seed/content/version yields the same ordered candidates and winner.
- Candidate enumeration crosses every face and seam correctly.
- Ocean, flood-prone, sterile, cliff, protected-site, and disconnected
  candidates fail their named predicates.
- A qualified candidate has reachable wood, fresh water, walkable ground, and
  the copper/tin hinterland contract.
- Reordering worker completion does not change the winner or chunk bytes.
- A seed with no valid candidate fails through the bounded policy without a
  rescue island.

### Preparation and persistence tests

- Exactly 25 common entry chunks are committed and listed in the manifest.
- Manifest publication is atomic and occurs after chunk/ledger durability.
- Cancellation at every stage leaves no completed manifest or committed trial
  side effects.
- A saved modified chunk wins over generated preparation.
- Warm restart loads the same spawn and prepared bytes without regeneration.
- Version/content mismatch follows the explicit refusal/repair path.
- Rejected candidates do not change water, salt, deposit, material, structure,
  wildlife, or history totals.
- Preparation, save, reload, and duplicate requests leave water/material
  audits exact and do not double-reserve resources.

### Scheduler tests

- Two guests requesting one cold chunk cause one worker job and two deliveries.
- Entry work preempts far-view work without starving active movement frontiers.
- Disconnect removes unique waiters and bounded work.
- Queue/in-flight limits hold under malicious repeated `RequestChunk` traffic.
- No host-pump call stack reaches `World::ensure_chunk`, cold disk I/O, terrain
  generation, or synchronous RLE compression.
- Chunk-order tests compare forward, reverse, random, parallel, and
  host-streamed results.

### Protocol/client tests

- Preparation messages round-trip and reset the liveness timeout.
- Pending players never enter active rosters or simulation.
- `Welcome` alone cannot enter play; complete manifest reception,
  `EntryLoaded`, and the host's `EntryReady` activation are required.
- Cancel, refusal, disconnect, and incompatible protocol show a specific join
  status rather than silently returning to the title.
- View distances 4, 10, 12, and the machine-clamped maximum join and expand
  without stalling the host.
- Solo, windowed-host, dedicated-host, graphical-guest, and agent paths all
  exercise the same scheduler and readiness predicates.

### Production smoke scenarios

Run at least the previously failing seed `20260801` on an untouched production
planet:

1. create and prepare the world,
2. assert the common spawn is natural qualified land,
3. restart a dedicated server,
4. join an agent at view distance 10,
5. use MCP perception and walk on land, into water, and back,
6. leave the agent idle in deep water for 60 seconds without drowning or
   sinking to the bed,
7. reconnect the same identity,
8. join a graphical client at view distance 12,
9. walk beyond the prepared region while the wider view streams,
10. disconnect during a second cold request and prove prompt reclamation,
11. save/restart and rerun water/material audits.

Capture the first playable graphical frame and an after-expansion horizon.
Each image records seed, generator/protocol versions, face/coordinates, view
distance, preparation timings, and host peak memory in a sidecar.

Add a production-generator agent integration fixture. The existing hand-built
fixtures remain useful but cannot be the sole cold-entry gate.

### Repository gates

Run and record:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
cargo deny check advisories
```

Resource-constrained runners use serial Rust compilation where required; the
runtime qualification still records production worker concurrency.

## Implementation order

1. **Instrumentation and regression fixtures**: encode the failing production
   seed, cold-join timing, host-pump timing, and conservation baselines.
2. **Shared spawn selection**: extract atlas selection, add voxel verification,
   remove fixed dedicated center spawn, and persist the common manifest.
3. **Shared chunk pipeline**: extract pure workers, add priority/dedup,
   time-sliced adoption, bounded encoding, cancellation, and metrics.
4. **World readiness**: replace solo's synchronous nine-chunk loop and prepare
   dedicated worlds before listening/advertising.
5. **Player readiness protocol**: pending-player state, progress, manifest,
   entry-ready gate, client loading UI, agent progress, and protocol bump.
6. **Streaming integration**: remove synchronous host generation, stage wide
   view requests, and enforce movement/residency frontiers.
7. **Agent repairs**: water-safe idle behavior and one canonical MCP coordinate
   contract with compatibility aliases.
8. **Qualification**: production seed matrix, multiplayer/agent/windowed
   captures, performance measurements, restart tests, and conservation audits.

Each stage keeps the narrow tests green. Do not mark the document implemented
after merely replacing the spawn point or moving `ensure_chunk` to a different
blocking loop.

## Documentation updates on implementation

Update current truth in:

- `docs/multiplayer-plan.md` and operator documentation,
- `docs/agent-mcp-plan.md` and `docs/agent-mcp-operations.md`,
- `docs/planetary-qualification-plan.md` and its evidence matrix,
- README world creation, loading, hosting, joining, and failure behavior.

Record the preparation/protocol/manifest versions, final constants, measured
budgets, deviations, verification commands, capture paths, and honest remaining
limitations in this document's implementation header.

## Out of scope

- eagerly generating or storing every voxel chunk on the planet,
- changing continents, climate, drainage, biomes, deposits, or the water cycle
  except where a spawn qualification reveals a violated existing contract,
- adding starter resources or transmuting/fabricating scarce material,
- redesigning identity or bypassing the explicit first-run local-profile
  consent screen for automation,
- magic, ambient power, taint, or magical ecology,
- MMO-scale concurrency promises.

## Completion criteria

This remediation is complete only when:

- every play/host mode uses one qualified persisted common spawn,
- no normal path creates the open-ocean castaway island,
- no host/network pump performs synchronous cold chunk work,
- pending players receive progress and cannot enter simulation early,
- the fixed entry region is ready independent of requested view distance,
- solo and dedicated cold starts satisfy the measured budgets,
- windowed clients and agents reliably enter untouched production terrain,
- wide views expand without starving network/simulation work,
- disconnect and cancellation reclaim bounded work,
- prepared/rejected/duplicate/reloaded chunks preserve exact water and finite
  material accounting,
- idle agent water behavior and MCP face schemas match their public contracts,
- the previously failing production scenario, full repository gates, and
  required visual evidence all pass.

Until those facts are measured, the finite planet may generate successfully,
but it is not yet safe to invite a society into it.
