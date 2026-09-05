<!-- wildforge:guide -->

# Working in src/world

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

World owns spatial state, persistence, ecology, environment, structures, machines, and magic integration. The migration extracts domain state with its behavior.

Preserve block-edit fan-out, side-effect order, conservation ledgers, and save compatibility. Guests must never acquire authoritative generation/persistence capabilities.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`cargo test --locked tests::world::` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.

Cancellation stops speculative preparation before adoption. Once a homeland or
retrogen save sequence has begun, finish that sequence before cancelling entry.
Propagate the caller's token into missing-world creation. Use fallible residency
when entry depends on a chunk; a corrupt save must not become regenerated terrain
or a supposedly ready doorway. Trial workers must be joined on every exit path.
