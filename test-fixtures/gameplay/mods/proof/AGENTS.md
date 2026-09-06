<!-- wildforge:guide -->

# Working in test-fixtures/gameplay/mods/proof

Read [README.md](README.md) and [the root instructions](../../../../AGENTS.md).

The proof mod supplies bounded animals, blocks, nests, and pieces for gameplay regression scenarios.

Keep IDs and intended interactions aligned with the tests. Avoid introducing unrelated progression or random wildlife into the fixture.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`python3 tools/run_gameplay_proofs.py` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
