<!-- wildforge:guide -->

# Working in .github

Read [the repository overview](../README.md), [the workflow guide](workflows/README.md),
and [the root instructions](../AGENTS.md).

`README.md` links to the canonical root README because GitHub gives this
directory priority when choosing the repository overview. Keep the game overview
in the root file and automation guidance here and under `workflows/`.
Use repository-root link paths in the shared README so its images and links
resolve correctly from both locations.

GitHub automation is organized under workflows/. Each job should expose a concrete verification result.

Preserve read-only default permissions and the separation between subsystem, agent, MSRV, and release gates.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`actionlint .github/workflows/ci.yml` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
