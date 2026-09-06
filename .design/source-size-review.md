# Source size review

The 400/500 physical-line guidance remains advisory. The analyzer still reports
all maintained source, including these reviewed exceptions; there are no size or
clone suppressions. Final formatting and reports will supply the accepted counts.
This record supersedes the early migration's temporary-exception table.

## Migrated transaction coordinators

| Exact file | Domain and reason retained | Next review point |
|---|---|---|
| `src/world/workings/settlement.rs` | Working completion/cancellation/interruption keeps Current/material journaling, PendingApply recovery and profile/world-save publication visible in one coordinator. Reservation, validation and effect helpers are already separate. | Next settlement or recovery protocol change; split only with an explicit typed commit/replay plan. |
| `src/world/alchemy/preparation_use.rs` | One stable dose transaction coordinates spoilage, target preflight, dense-dross linked rollback, reusable vessel custody and post-commit water/soil effects. Shared quantity/status rules and other apparatus operations are separate. | Next preparation target or custody-staging change. |
| `src/server.rs` | The authoritative fixed-step coordinator exposes simulation order and owned subsystem cadence together; transport, terrain jobs and presentation are separate. | Next simulation scheduling change; retain one visible phase order. |

## Existing algorithms and compatibility surfaces

These received focused read-contract, identity or owner-access changes. Their
existing algorithms are outside the structural slices specified in this plan;
rewriting them solely to satisfy an advisory count would widen behavior risk.
Each row identifies its owning domain and the next concrete review trigger.

| Exact file | Domain and scope of change | Next review point |
|---|---|---|
| `src/agent/mod.rs` | Agent composition retains its public facade while using shared GuestSession and independent ReplicaWorld. | Next agent lifecycle or MCP surface change. |
| `src/agent/work.rs` | Agent task policies send network actions and read the replica; they are distinct from authoritative player transaction rules. | Next task/navigation workflow change; separate action families with native parity evidence. |
| `src/mesher.rs` | CPU voxel mesh emission consumes TerrainRead; vertex ordering, face/material rules and lighting interpolation remain the existing algorithm. | Next meshing performance or material-emission change, with pinned buffers and actual GPU captures. |
| `src/mobs.rs` | Existing movement/combat/presentation model uses bounded scene reads where applicable; AI and locomotion policy is preserved. | Next AI/pathfinding or creature-presentation change; separate model/tick/emission responsibilities then. |
| `src/planet.rs` | Canonical coordinate and seam transforms remain the shared topology owner; new consumers use these stable primitives. | Next topology/coordinate API change, with seam and codec fixtures. |
| `src/net/protocol.rs` | Wire DTOs and message enum ordering remain together for compatibility review; no protocol version change accompanies the refactor. | Next protocol version, with pinned discriminants and exact round trips. |
| `src/net/transport.rs` | QUIC endpoint/stream ownership and orderly client shutdown remain one transport implementation under GuestSession. | Next transport lifecycle change; preserve bounded shutdown and live connection proof. |
| `src/world/fluids.rs` | Existing finite-water/lava scheduling and mass transfer now access explicit owners; flow policy and conserved water remain unchanged. | Next fluid scheduling or water-polish change, with conservation and seam tests. |
| `src/world/hearts.rs` | Country-heart lifecycle and rooting keep the existing state/ire/ledger interaction; observation and calendar inputs use new owners. | Next heart/rooting policy change, with state-transition and Current fixtures. |
| `src/world/ticks.rs` | Random sampling and offline crop/snow/rain reconciliation preserve their RNG/cadence and now use calendar/population owners. | Next growth or offline simulation change, with pinned request-order and catch-up evidence. |
| `src/world/local_structure.rs` | Independent structure state and physical BlockStore/sidecar representation remain distinct from the new construction membership owner. | Next independent-structure state or sidecar version change. |
| `src/world/template.rs` | Template capture/stamp commands use construction membership; existing capture codec and physical stamp order are preserved. | Next template command or stamp semantics change. |
| `src/world/pieces.rs` | Authored modular-assembly placement retains its deterministic RNG and placement order against the new world facade. | Next assembly/worldgen content change, with pinned structure output. |
| `tools/verify_visual_polish.py` | Campaign-specific visual interpretation retains its own thresholds and layout semantics; common file/scalar helpers are shared. | Next campaign schema/metric change; split semantic metric families without weakening evidence. |
| `tools/verify_visual_closeout.py` | Closeout-specific grouping and acceptance remain distinct from ordinary visual metrics; common IO is shared. | Next closeout manifest change, with malformed and incomplete evidence fixtures. |

## Scenario organization

World, multiplayer, agent, ecology, rendering, workings, implements, alchemy,
machines, mobs, hydrology, dross, water-cycle, climate and interior scenarios now
have operation-focused modules. Shared setup stays in each parent; reusable
geography selection lives in tests/fixtures. Tooltip, spawn and host identity
cases are separate modules. Visual qualification schemas and validation stages
have distinct files under visual_capture without changing acceptance thresholds.

| Exact file | Domain and reason retained | Next review point |
|---|---|---|
| `src/tests/multiplayer/loopback_join_stream_and_edit.rs` | A single real QUIC scenario follows admission, registry/chunk/lighting receipt, authoritative edits and guest feedback with one pair of endpoints. Shared loopback setup is already outside it. Keeping this stateful compatibility sequence intact makes the wire ordering visible. | Next wire/admission version; introduce named phase helpers with a typed scenario context if the sequence grows. |
