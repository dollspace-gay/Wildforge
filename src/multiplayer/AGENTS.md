<!-- wildforge:guide -->

# Working in src/multiplayer

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

Host requests, player profiles, roles, moderation, roster, and terrain delivery connect authenticated guests to shared simulation.

Validate identity, permission, reach, and actor readiness before effects. Move duplicated gameplay rules into shared authoritative operations.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`cargo test --locked tests::multiplayer::` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
