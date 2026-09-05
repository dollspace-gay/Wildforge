<!-- wildforge:guide -->

# Working in test-fixtures/gameplay

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

The mods/ tree supplies deterministic content for actual graphical interaction proofs.

Keep the fixture small and explicit about its differences from production. Graphical proofs need a real display and disposable runtime directory.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`python3 tools/run_gameplay_proofs.py` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
