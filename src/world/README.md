<!-- wildforge:guide -->

# Authoritative world domains

World owns spatial state, persistence, ecology, environment, structures, machines, and magic integration. The migration extracts domain state with its behavior.

Start with `alchemy.rs`, `belt.rs`, `calendar.rs`, `chunks.rs`, `discovery.rs`, `dross.rs`, `dungeon.rs`, `ecology.rs`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Preserve block-edit fan-out, side-effect order, conservation ledgers, and save compatibility. Guests must never acquire authoritative generation/persistence capabilities.

`preparation.rs` runs immutable homeland trials through the shared bounded worker
owner. Worker count, per-worker generator context, and sorted adoption order are
preserved. Cancellable opening/spawn preparation checks between read/generation
steps; once durable homeland adoption starts, it completes before acknowledging
cancellation. `try_ensure_chunk` preserves read errors for entry callers while
ordinary simulation callers retain `ensure_chunk`'s existing boolean interface.

## Focused checks

```sh
cargo test --locked tests::world::
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.

`TerrainRead` exposes only resident voxel/metadata/light queries and the immutable
registry; `SceneRead` adds local structure selection. Physics, item motion,
raycasts, camera collision, and mesh capture consume these contracts. World
retains its existing facade while forwarding common observations to that code.

`terrain.rs` owns the private resident map, dirty-mesh bookkeeping, light solve,
and WFC wire reconstruction. Adoption and persistence still coordinate through
World; the spatial owner has no generator, save writer, or conservation ledger.
This is the shared storage seam used by the separate replica migration.

`ReplicaWorld` is the independent agent world owner. `observations.rs` owns
bounded host weather, arcane, item, and apparatus data. `replication.rs` defines
the shared incoming mutation contract; its World adapter is temporary until
graphical migration finishes. Replica block updates share resident writes and
support classification with authority, without spawning drops or simulation.

`CalendarView` shares astronomical/seasonal observations without simulation
mutation. `block_store.rs` separates read-only machine recognition (`BlockRead`)
from physical mutation (`BlockStore`). Shape/stat/capability queries and machine
validation use the read contract; firing and accounting require the extension.

`view/` exposes bounded scene queries over authority or a streamed replica.
`item_presentation.rs`, `calendar_view.rs`, and terrain queries retain shared
read rules. `standing.rs` preserves position rescue order with explicit column
preparation: authority can load a column, while replica prediction reads only
resident terrain. `terrain/remap.rs` remaps resident block identities by name;
authority separately reconciles its gates, ledgers, and generator bindings.

`country_view.rs` shares seed bearings and heart descriptions from immutable
geography plus an explicit known-dead predicate/visible heart. Soil warning
priority similarly uses observed salinity and optional habitat/moisture inputs.
Guests retain their existing absence of private heart and climate-water books.

World now represents authority only. It has no remote flag, incoming snapshot
adapter, or ReplicaObservations storage. GuestSession applies incoming state only
to ReplicaWorld; world saving, generation, physical support, and machine
revalidation are unconditional authority operations. Tests that formerly switched
World into guest mode must exercise the independent replica instead.

`population.rs` owns live mobs/NPC links, projectiles, loose items, and identity
cursors. Mutation views expose member slices; insertion, extraction, restoration,
and NPC association use named operations. `ecology/` contains the explicit
cross-domain physics/admission/death coordinators with focused habitat, wildlife,
hostile, projectile, and loose-item responsibilities. Save adapters still encode
the same entity fields and establish loaded NPC companion identity immediately.

`calendar_state.rs` owns the authoritative day, absolute clock, Long Winter state,
and separate reciprocity rollover fraction. Server advancement and save loading
use explicit transitions; pure CalendarView remains shared with replicas. Weather
and conservation coordinators sample that state without mutating clock fields.

`weather_state.rs` owns the live PlanetaryWeather lifetime, climate-hour admission,
error abort, and development weather overrides. The existing PlanetaryWeather
owns conserved water. World supplies immutable atlas/calendar/ire inputs and
coordinates completed-hour water inboxes, springs, shores, and batched relighting.

`installations.rs` owns on-grid block entities, transient station work, wheel
momentum, spoilage cadence, and the industrial-ire beat. Container transactions
operate on that owner's entries; machine recognition/ticking use the existing
BlockRead/BlockStore adapter. LocalStructure continues to own its independent
block map and outbox. World coordinates ledger effects and block-edit fan-out.
