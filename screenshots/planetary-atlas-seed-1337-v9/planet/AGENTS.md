<!-- wildforge:guide -->

# Working in screenshots/planetary-atlas-seed-1337-v9/planet

Read [README.md](README.md) and [the root instructions](../../../AGENTS.md).

manifest.toml records the identity and version of the atlas diagnostic dataset in the parent directory.

Treat the manifest as historical provenance. Do not alter hashes or versions to make newer output look like the original dataset.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`git diff --check` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
