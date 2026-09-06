# Cross-domain transaction order

These sequences describe the current authoritative coordinators. Domain owners
keep their own invariants and conserved quantities; the coordinator defines when
one domain's result may become visible to another. Replica application has its
own resident-write path and performs none of these authority transactions.

## Block edits

`block_edits.rs` keeps raw physical edit fan-out ordered. Admission and custody
for mining/placement occur in their named transaction coordinators first.

| Order | Responsibility |
|---|---|
| 1 | Write voxel/metadata state and publish the existing block edit record. |
| 2 | Retire tracked water carriers when the physical representation changes. |
| 3 | Mark adjoining meshes and wake affected fluid cells. |
| 4 | Detach gravity blocks and remove unsupported attachments with their drops. |
| 5 | Perform or batch relighting according to the existing lighting scope. |
| 6 | Remove invalid block entities, account their contents, and invalidate dependent state. |
| 7 | Maintain nest records, satisfy ghost fills, and revalidate physical multiblocks. |

Mining retains its eligibility checks and staged custody/material operations
before applying the raw edit. Returned drops keep their full ItemStack identity;
material completion, water/seep behavior and heart-struck effects remain after the
physical transaction. A refused mining operation returns without hunger/wear/XP.

## Mob death and drops

`ecology/death.rs::settle_dead_mobs` removes each dead population member once.
It applies hostile-death or ordinary-animal ire, releases cargo, and rolls adult
loot with the existing RNG sequence. Hostile charged drops bind from the mob's
Current account before the remainder is retired according to its disposition.
Each rolled stack then receives material accounting and is queued to the recorded
attacker or emitted at the death position. Swarm brood spawning follows loot;
only then is the cosmetic settled-death record returned to the adapter.

Cargo already has physical custody and is released directly; rolled creature
loot uses the existing external-source accounting. Charge binding/accounting
failures retain the current diagnostic behavior. This structural refactor does
not silently turn those failures into a new rollback policy.

## Chunk adoption

`chunks/adoption.rs` rejects stale prepared revisions and keeps saved terrain
precedence before reaching the adoption transaction. Deep dungeon chunks retain
their existing bare insertion path. Planetary chunks follow this order:

| Order | Responsibility |
|---|---|
| 1 | Heal the bedrock floor and commit fresh-chunk water before insertion. |
| 2 | Insert resident terrain; apply loaded material retrogen for stored chunks. |
| 3 | Apply loaded water inboxes and stamp structures only for fresh terrain. |
| 4 | Reconcile saved arcane ecology and dross representatives. |
| 5 | Reserve the final physical material voxels after structure replacement. |
| 6 | Register existing country hearts without replacing recorded heart stages. |
| 7 | Admit wildlife once through persisted population seeding membership. |
| 8 | Preserve/read the prior random-tick stamp and initialize missing stamps. |
| 9 | Wake seams, wake stale saved fluids, and relight/cascade. |
| 10 | Reconcile a qualifying offline time gap and update its stamp. |

Reservation failure remains observable; it is not hidden by claiming the chunk
is fully qualified. Prepared homeland publication separately validates water and
material books and commits its manifest/digest through the existing save stages.

## Saving and unload

`storage/save_world.rs` aggregates metadata/domain/sidecar failures in SaveReport.
Palette publication precedes chunk writes so new stored IDs never reach disk
before their names. A successful chunk write clears that chunk's modified flag;
a failed write leaves it dirty for retry. Reports preserve independent failures.

`residency.rs::evict_chunks` returns immediately for no candidates. Otherwise it
settles falling blocks before saving each departing modified chunk. Only a
successful save (or an unmodified chunk) permits unload and inclusion in the
released-position list. Failed saves retain the newest in-memory chunk and report
the failure. Graphical consumers release GPU/light-cache state only for that list.
Replica eviction removes resident observations/meshes and performs no physical
settlement, generation, ledger operation, or authoritative save.
