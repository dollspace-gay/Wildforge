<!-- wildforge:guide -->

# Working in src

Read [README.md](README.md) and [the root instructions](../AGENTS.md).

lib.rs assembles the single package; main.rs dispatches into it. Server simulation, world state, content, clients, transport, and rendering have separate responsibilities.

Keep simulation and generation independent of window/audio/GPU state. Use domain owners and explicit imports as the migration progresses.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`cargo clippy --locked --all-targets --all-features -- -D warnings` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
