<!-- wildforge:guide -->

# Working in src/tests

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

Domain scenarios share small fixtures from mod.rs. Agent tests exercise real QUIC guests in a separate serial lane.

Test contracts and observable outcomes. Preserve production-shaped qualification alongside deterministic fixtures. Avoid duplicating setup that encodes one invariant.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`cargo test --locked --all-targets -- --skip tests::agent::` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
