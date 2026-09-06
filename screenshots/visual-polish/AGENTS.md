<!-- wildforge:guide -->

# Working in screenshots/visual-polish

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

Report TOML files capture measured visual and runtime outcomes for the staged polish scenarios.

Preserve capture identity, hardware, timings, and acceptance scope. Structural refactors require fresh evidence when source identity changes.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`python3 tools/verify_visual_polish.py --help` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
