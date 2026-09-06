# Clone candidate review

This reviews the exact-token candidates at the final implementation checkpoint.
The complete locations and stable IDs are in `maintainability-final.json`.
The detector is advisory: the retained entries below remain reported and are
not suppressions or a claim of zero semantic duplication. Counts overlap.

The migration removed copied local/network trade, inventory, crafting, equipment,
nutrition and terrain rules, unified terrain-job/session machinery, and shared
visual fog/family metrics, implement instability and working effect paths.
The final working path extraction removed candidate `b9f1b26931` after confirming
identical ordered physical routes in live execution and restored transactions.

Retained algorithm/compatibility helper candidates are existing maintenance debt.
They are bounded to the named owning domain and next review trigger below, rather
than requiring a speculative generic transaction or codec abstraction in this
structural migration. Scenario copies retain independent assertions; helper
extraction must not make a test compute its oracle by calling the implementation.

| Candidate | Representative location | Review and next trigger |
|---|---|---|
| `c3486f8f9b` | `src/shader/diagnostics.wgsl:19` | Retain the two shader entry points with their distinct fragment outputs. Coverage sampling remains an advisory helper candidate; review with the next diagnostic shader change and native coverage captures. |
| `63a1dd3593` | `src/game/demos/materials.rs:14` | Retain demo scene construction as explicit fixture geometry; review when either scene contract changes. |
| `b71f287dca` | `src/tests/world/ponds.rs:127` | Retain scenario-specific setup and independent assertions. Shared geography/host/content fixture builders already own common contracts; review further setup factoring when this scenario family changes. |
| `39ceb3d640` | `src/tests/power_draw.rs:171` | Retain scenario-specific setup and independent assertions. Shared geography/host/content fixture builders already own common contracts; review further setup factoring when this scenario family changes. |
| `129487fdf5` | `src/world/implements/repair.rs:208` | Retain the ordered linked-file commit/adopt steps beside each transaction. Repair additionally consumes inventory and changes a frame; retirement routes residual custody. Shared ledger commit mechanics already have one owner. |
| `5c651c3276` | `src/registry/loading.rs:162` | Retain explicit RawMod assembly for fallible external loading versus embedded base content. These field lists are DTO construction, not two validators. |
| `606bdcbe84` | `src/tests/world/water_seams.rs:17` | Retain scenario-specific setup and independent assertions. Shared geography/host/content fixture builders already own common contracts; review further setup factoring when this scenario family changes. |
| `837b35b5c3` | `src/arcane.rs:3994` | Retain independent historical-ledger migration fixtures and their version-specific assertions. |
| `ceb6d146ed` | `src/tests/player.rs:244` | Retain scenario-specific setup and independent assertions. Shared geography/host/content fixture builders already own common contracts; review further setup factoring when this scenario family changes. |
| `1951d50875` | `src/tests/alchemy/nutrition.rs:14` | Retain scenario-specific setup and independent assertions. Shared geography/host/content fixture builders already own common contracts; review further setup factoring when this scenario family changes. |
| `9afd7a186f` | `src/planet_atlas/storage/arcane.rs:83` | Existing manifest field-copy debt remains visible. Direct publication and linked payload preparation have distinct commit boundaries; review this helper candidate with the next manifest schema change. |
| `22a7e1f2c0` | `src/geode_capture.rs:435` | Retain an independently calculated shell-connectivity test oracle beside the production aperture search. |
| `0c6cb1e602` | `src/world/spawn/tests.rs:112` | Retain scenario-specific setup and independent assertions. Shared geography/host/content fixture builders already own common contracts; review further setup factoring when this scenario family changes. |
| `ebeb9cadeb` | `src/tests/workings/rituals.rs:71` | Retain scenario-specific setup and independent assertions. Shared geography/host/content fixture builders already own common contracts; review further setup factoring when this scenario family changes. |
| `83de0d3b2a` | `src/tests/world/discovery.rs:8` | Retain scenario-specific setup and independent assertions. Shared geography/host/content fixture builders already own common contracts; review further setup factoring when this scenario family changes. |
| `09f73da0ae` | `src/arcane.rs:723` | Retain explicit migration from each historical ledger DTO; their old field layouts and defaults are compatibility contracts. |
| `36adad3824` | `src/arcane.rs:3107` | Existing framing similarity spans separate material and Current codecs/magic/checksums. Review a shared bounded frame reader only with both recovery suites and format identities preserved. |
| `52cc9d8e44` | `src/tests/fixtures/geography.rs:11` | Retain scenario-specific setup and independent assertions. Shared geography/host/content fixture builders already own common contracts; review further setup factoring when this scenario family changes. |
| `786f656f7e` | `src/world/alchemy/filter_load.rs:81` | Retain explicit custody reads beside different filter-load and grinding transactions. No account balance is owned by the adapter; the shared ledger remains authoritative. |
| `505e671711` | `src/renderer/frame/diagnostic.rs:61` | Retain distinct render-pass descriptors and bind/clear order for diagnostic outputs. Review a descriptor helper only with pass-lifetime and native capture evidence. |
| `5a07f65e62` | `src/tests/interiors/machines.rs:55` | Retain scenario-specific setup and independent assertions. Shared geography/host/content fixture builders already own common contracts; review further setup factoring when this scenario family changes. |
| `881e943fe0` | `src/renderer/frame/scene.rs:27` | Retain world and hand pass setup with distinct color/load state and pipeline sequence; the repeated wgpu descriptor fields are not a shared gameplay policy. |
| `42b3866b07` | `src/tests/rail.rs:72` | Retain scenario-specific setup and independent assertions. Shared geography/host/content fixture builders already own common contracts; review further setup factoring when this scenario family changes. |
| `e0fdb0e506` | `src/tests/worldgen.rs:1338` | Retain scenario-specific setup and independent assertions. Shared geography/host/content fixture builders already own common contracts; review further setup factoring when this scenario family changes. |
| `07aae539ad` | `src/tests/world/funded_offerings.rs:33` | Retain scenario-specific setup and independent assertions. Shared geography/host/content fixture builders already own common contracts; review further setup factoring when this scenario family changes. |
| `9df4e6de86` | `src/game/demos/industry.rs:39` | Retain explicit independent machine-demo geometry; review fixture builders when those scene layouts change. |
| `905ad3ddb9` | `src/world/replica/scene.rs:141` | Retain narrow trait delegation for two real read owners. Shared planetary delta math already has one implementation; no authority mutation is exposed. |
| `f94464a008` | `src/geode_capture.rs:491` | Retain production aperture planning and the distinct chamber path checks; matching parameter/setup spans do not establish identical complete algorithms. |
| `056e86468c` | `src/tests/rail.rs:72` | Retain scenario-specific setup and independent assertions. Shared geography/host/content fixture builders already own common contracts; review further setup factoring when this scenario family changes. |
| `e0ffb51914` | `src/registry/placeholders.rs:38` | Retain explicit placeholder DTO defaults preserving saved durability/placement and missing-content semantics. Review with the next item schema addition. |
| `c433794dbc` | `src/tests/archetypes.rs:8` | Retain scenario-specific setup and independent assertions. Shared geography/host/content fixture builders already own common contracts; review further setup factoring when this scenario family changes. |
| `01fc2c0e81` | `src/world/implements/item_transfer.rs:25` | Retain admission beside item-transfer and source-transfer transactions. Their source identity, routing and costs differ; review a typed frame preflight on the next transfer change. |
| `22a739e1b7` | `src/world/implements/charm_binding.rs:4` | Explicit dependency imports at a boundary are intentionally retained; replacing them with a prelude would hide dependencies. |
| `35508734e1` | `src/world/implements/charm_binding.rs:228` | Retain journal commit before state adoption and frame publication in each named transaction. Binding, disassembly and focus swap retain different custody/physical outcomes. |
| `7d6849066a` | `src/tests/archetypes.rs:8` | Retain scenario-specific setup and independent assertions. Shared geography/host/content fixture builders already own common contracts; review further setup factoring when this scenario family changes. |
| `3bd7aafce3` | `src/world/workings/ritual_reservation.rs:6` | Explicit dependency imports are intentional boundary evidence; reservation policies remain different. |
| `96c0ed2d37` | `src/tests/ecology.rs:4` | Retain scenario-specific setup and independent assertions. Shared geography/host/content fixture builders already own common contracts; review further setup factoring when this scenario family changes. |
| `8bf3a52213` | `src/tests/interiors/machines.rs:152` | Retain scenario-specific setup and independent assertions. Shared geography/host/content fixture builders already own common contracts; review further setup factoring when this scenario family changes. |
| `f4b2c28ee2` | `src/script.rs:313` | Existing script alias registration repeats input conversion. Keep compatibility names and command routing visible; review with the next scripting API change. |
| `896127f614` | `src/game/ui/station_panels.rs:122` | Retain station-specific status/button arbitration and common inventory/footer calls. Common widget implementation is already shared. |
| `b8aaf265d2` | `src/atlas/procedural.rs:1849` | Retain authored tile recipes and exact deterministic sampling order. Shared texture primitives already exist; review recipe-level factoring with atlas image fixtures. |
| `d4787b5f9d` | `src/planet_atlas/codec/container.rs:146` | Retain fixed-record versus variable-payload readers and their independent bounds. Shared ByteReader and bounded file reads own the primitive validation. |
| `8b2390b2e7` | `src/planet_atlas/codec/container.rs:16` | Retain borrowed versus owned buffer assembly paths and their allocation behavior. Header primitives are shared; review with the next codec change. |
| `db022bb99d` | `src/planet_atlas/storage/mutable.rs:24` | Retain direct mutable-atlas save and borrowed-world snapshot save coordinators. Manifest mutation and failure observation differ; backup and atomic-write primitives are shared. |
| `ec89bf3579` | `src/tests/climate/normals.rs:176` | Retain scenario-specific setup and independent assertions. Shared geography/host/content fixture builders already own common contracts; review further setup factoring when this scenario family changes. |
| `7cf46da7a7` | `src/world/storage/load_population.rs:268` | Retain historical gate and settlement decoding with their distinct population destinations; preserve WFG1 compatibility fixtures. |
| `5451eea504` | `src/registry/linking/bootstrap.rs:69` | Retain explicit AIR/unknown block DTO defaults and reserved identities; review together with registry schema changes. |
| `8f422c5473` | `src/world/workings/nudge.rs:77` | Retain projectile versus dropped-item admission and distinct magnitude/mass impulse laws. Common reach/visibility preflight remains advisory helper debt for a future nudge policy change. |
| `38da9d011f` | `src/world/fluids.rs:568` | Retain fluid simulation traversal/cadence and exact neighbor ordering. Existing finite-mass transfer primitive is shared; review algorithm unification only with water conservation/seam fixtures. |
| `3ec82cf4fb` | `src/renderer/frame/composite.rs:26` | Retain ordinary versus capture-overlay pass order and attachment lifetimes; shared render resources remain owned by Renderer. |
| `5b6e5398c8` | `src/registry/linking/register_blocks.rs:307` | Retain explicit generated item DTO construction with placement/material provenance from different inputs; review together on schema change. |
| `eddb0279ce` | `src/world/entities/load.rs:122` | Retain planetary versus independent-structure sidecar loading and coordinate/ownership contexts; shared item identity remains the registry contract. |
| `41525507b1` | `src/renderer/frame/diagnostic.rs:66` | Overlapping descriptor candidate with the other render-pass groups; retain explicit pass order, not an additional independent debt count. |
| `4982fb1328` | `src/game/actions/portable_items.rs:132` | Retain water versus lava action adapters with different physical operations; Placement owns mutation rules and common inventory type owns stacks. Bucket completion is advisory UI helper debt. |
| `335082f74b` | `src/visual_capture/closeout.rs:93` | Retain separate ordinary and repeated-closeout matrix validation entry points. Common metadata validation is already shared; review axis-check factoring with the next campaign schema change. |
| `927542c323` | `src/registry/linking/register_blocks.rs:307` | Overlapping explicit item DTO defaults; preserve different salvage/placeholder/material inputs. This is not an additional independent duplicate implementation. |
| `0a2dca1fee` | `src/world/chunks/water_commit.rs:115` | Existing hydro reservoir classification is advisory shared-query debt within the world domain. Adoption and loaded fluid initialization retain their distinct accounting order; review together when reservoir classification changes. |
| `115d120da6` | `src/game/actions/portable_items.rs:120` | Overlapping water/lava adapter guards; physical placement rules already have a shared owner, with separate water-class and lava payloads. |
| `41daefee8a` | `src/world/implements/failure.rs:92` | Retain failure versus vessel-tick custody preflight adjacent to their distinct leakage/retirement effects; ledgers remain the sole balance owners. |
| `60287760f9` | `src/game/session/loose_items.rs:59` | Retain historical graphical loose-item restore versus authoritative persistence DTOs and different error/return policies. Review unified schema ownership on the next save version. |
| `f5c9e3d9bd` | `src/planet_atlas/climate/moisture.rs:85` | Retain convergence iteration and accepted-equilibrium evaluation order in the pinned climate algorithm. Repeated lift arithmetic is advisory helper debt; review with deterministic atlas fixtures. |
