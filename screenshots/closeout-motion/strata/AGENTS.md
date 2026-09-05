<!-- wildforge:guide -->

# Working in screenshots/closeout-motion/strata

Read [README.md](README.md) and [the root instructions](../../../AGENTS.md).

phases.csv records the strata camera/motion sequence used by the visual closeout.

Preserve the ordered route and compare on the actual GPU. Document lighting and camera differences in new evidence.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`python3 tools/verify_visual_closeout.py --help` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
