<!-- wildforge:guide -->

# Working in .github

Read [README.md](README.md) and [the root instructions](../AGENTS.md).

GitHub automation is organized under workflows/. Each job should expose a concrete verification result.

Preserve read-only default permissions and the separation between subsystem, agent, MSRV, and release gates.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`actionlint .github/workflows/ci.yml` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
