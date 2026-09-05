<!-- wildforge:guide -->

# Working in tools

Read [README.md](README.md) and [the root instructions](../AGENTS.md).

Python tools generate authored assets, verify visual/gameplay evidence, install pinned test content, and report maintainability.

Use explicit input/output paths and preserve user data. Keep shared verification rules in one owner and report incomplete scans/runs as failures.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`python3 -m unittest discover -s tools/tests -v` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
