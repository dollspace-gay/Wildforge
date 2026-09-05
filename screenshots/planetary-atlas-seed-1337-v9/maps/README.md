<!-- wildforge:guide -->

# Atlas map legends

Legend files define the scales and meanings of the pinned climate, biome, geology, water, and wildlife maps.

Start with `animal_frog_suitability.legend.txt`, `animal_seal_suitability.legend.txt`, `aquifer_capacity.legend.txt`, `aquifer_permeability.legend.txt`, `aridity.legend.txt`, `atmospheric_vapor.legend.txt`, `autumn_precipitation.legend.txt`, `autumn_temperature.legend.txt`. See the [repository overview](../../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Keep labels and units aligned with the generation that produced the data. Distinguish visual scaling from simulation values.

## Focused checks

```sh
git diff --check
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
