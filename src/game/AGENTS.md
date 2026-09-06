<!-- wildforge:guide -->

# Working in src/game

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

The windowed app coordinates input, UI, sessions, streaming, actions, and presentation. Existing broad Game access is being replaced with cohesive owners.

Keep simulation order explicit. Guests apply host state, while UI and rendering consume it. Preserve real interaction paths when extracting helpers.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`python3 tools/run_gameplay_proofs.py` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.

World loading may have only one active operation. Keep cancellation responsive,
join before adoption, preserve a successfully published save if entry is cancelled,
and reject preparation requested from obsolete content. Read errors and panics
are failures even if the user also requested cancellation; never classify them
by searching their message text.

Content reload must invalidate prepared terrain and mesh registry identities,
including reloads whose texture variants look unchanged. Keep save-palette context
checks on the terrain path; meshes consume current runtime IDs, not disk IDs.
