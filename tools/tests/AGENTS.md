<!-- wildforge:guide -->

# Working in tools/tests

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

Tests exercise source selection, scanner semantics, reports, and command exit behavior using disposable repositories.

Keep tests independent of personal files and installed game content. Test malformed input and incomplete scans as well as successful reports.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`python3 -m unittest discover -s tools/tests -v` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
