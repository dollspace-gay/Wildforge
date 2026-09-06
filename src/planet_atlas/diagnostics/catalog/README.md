<!-- wildforge:guide -->

# Diagnostic map registry

Each authored slice groups a real diagnostic domain. The parent retains one
ordered list of slices for both export and report generation. IDs, descriptions,
map kinds, and ordering form the diagnostic artifact contract. Value lookup
lives in the diagnostic value reader; this registry does not mutate the atlas.

Read [AGENTS.md](AGENTS.md). Final checks compare map/legend/report output,
registered order, census totals, site selection and coordinates, and the
applicable atlas/runtime qualification evidence on the final source.
