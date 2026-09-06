<!-- wildforge:guide -->

# Biomes, habitats, and countries

The parent retains the ordered country-generation coordinator and stable biome,
habitat, and edaphic identifiers. `classification.rs` interprets climate zones
and slope. `ground.rs` establishes soil and groundwater conditions; `habitats.rs`
derives water-backed local overlays and vegetation potential. `country_seeds.rs`
selects nuclei and `country_partition.rs` establishes traversable borders.
`hearts.rs` scores country heart sites. `records.rs` defines sparse country
composition/routes, `validation.rs` checks the dense partition, and `sampling.rs`
serves local queries and graft compatibility.

Read [AGENTS.md](AGENTS.md). Final checks compare pinned biome/ground cells,
country/heart IDs, reciprocal routes, seam coverage, groundwater-backed habitat
queries, and save compatibility, followed by the applicable full gates.
