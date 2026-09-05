<!-- wildforge:guide -->

# Working in docs

Read [README.md](README.md) and [the root instructions](../AGENTS.md).

Feature plans, operating instructions, qualification records, and reports explain the game and its history. The active maintainability plan is in .design/.

Distinguish intended design from verified current behavior. Preserve dated evidence; record new outcomes instead of rewriting old results.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`git diff --check` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
