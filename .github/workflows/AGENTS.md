<!-- wildforge:guide -->

# Working in .github/workflows

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

ci.yml runs native Rust gates, dependency advisories, and the Python maintainability report.

Keep cold-build time separate from scenario runtime. Findings in the maintainability report are advisory; analyzer failures must remain visible.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`actionlint .github/workflows/ci.yml` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
