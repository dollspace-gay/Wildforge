<!-- wildforge:guide -->

# Working in .cargo

Read [README.md](README.md) and [the root instructions](../AGENTS.md).

Portable compiler configuration. The pinned toolchain and package MSRV live at the repository root.

Keep local cache paths and machine credentials out of tracked configuration. Check Linux and Windows assumptions before changing target settings.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`cargo +1.95.0 check --locked --all-targets` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
