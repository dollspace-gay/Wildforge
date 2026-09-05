<!-- wildforge:guide -->

# Working in tools/maintainability

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

lex.py tokenizes source, clones.py verifies repeated spans, and report.py measures repository source and renders findings.

Keep size advisory and distinguish clone candidates from semantic DRY violations. Preserve exact literals and test scanner failure paths.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`python3 -m unittest discover -s tools/tests -v` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
