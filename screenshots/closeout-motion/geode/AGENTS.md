<!-- wildforge:guide -->

# Working in screenshots/closeout-motion/geode

Read [README.md](README.md) and [the root instructions](../../../AGENTS.md).

phases.csv records the cracked-geode camera/motion phases for visual qualification.

Keep phases tied to the geode scenario and its capture provenance. A static image is not evidence for a motion requirement.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`python3 tools/verify_cracked_geode.py --help` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
