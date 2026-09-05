<!-- wildforge:guide -->

# Working in hydrology

Read [README.md](README.md) and [the atlas guidance](../AGENTS.md).
Preserve traversal/tie-breaking order, seed salts, stable IDs, enum layouts,
units, and floating-point operation order during structural changes. The
priority-flood graph must remain acyclic after lake outlet resolution. Do not
replace finite water or resource budgets with capacity estimates or fabricated
mass. Sampling reads the generated model and must not reroll drainage decisions.
Keep helpers scoped to hydrology and imports explicit. Aim for 400 lines per
cohesive module and review above 500. Run focused hydrology/atlas/codec scenarios
and applicable full gates before accepting a change; record incomplete evidence.
