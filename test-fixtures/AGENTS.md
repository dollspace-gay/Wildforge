<!-- wildforge:guide -->

# Working in test-fixtures

Read [README.md](README.md) and [the root instructions](../AGENTS.md).

Small authored fixtures make regression scenarios reproducible without depending on private player worlds.

Keep fixture intent documented. Do not weaken a fixture to hide a production defect or commit personal saves/identities.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`python3 tools/run_gameplay_proofs.py` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
