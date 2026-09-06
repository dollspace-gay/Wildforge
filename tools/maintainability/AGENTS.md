<!-- wildforge:guide -->

# Working in tools/maintainability

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

lex.py tokenizes source, clones.py verifies repeated spans, report.py measures source and renders findings; revisions.py reads pinned Git source and compares findings.

Keep size advisory and distinguish clone candidates from semantic DRY violations. Preserve exact literals and test scanner failure paths.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`python3 -m unittest discover -s tools/tests -v` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.

Dependency reports must distinguish written-path coverage from compiler name or
effect resolution. Keep alias/glob cases and incomplete scans visible; do not
silently convert a parser or compiler failure into zero findings. Function size
and cognitive complexity come from Clippy diagnostics, with its version and
configuration recorded. Do not replace them with keyword-count approximations.
