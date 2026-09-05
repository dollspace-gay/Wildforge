<!-- wildforge:guide -->

# Working in screenshots/closeout-motion

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

The geode/ and strata/ scenario directories hold phase sequences used to assess movement and visual stability.

Preserve phase order and capture metadata. Validate changes through the corresponding real renderer scenario.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`python3 tools/verify_visual_closeout.py --help` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
