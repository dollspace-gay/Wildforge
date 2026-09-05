<!-- wildforge:guide -->

# Working in docs/evidence

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

Small checked-in records connect defects and fixes to reproducible observations. gameplay/ contains the earlier gameplay sweep evidence.

Record commands, revisions, environment, and the observed result. Never replace failing evidence merely to make a gate appear green.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`git diff --check` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
