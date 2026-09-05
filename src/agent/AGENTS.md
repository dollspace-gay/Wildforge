<!-- wildforge:guide -->

# Working in src/agent

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

MCP exposes ordinary guest perception, movement, and work. motion.rs follows walkable terrain; work.rs performs host-authorized actions.

The agent is an ordinary guest. Preserve entry readiness, terrain clearance, authoritative inventory, and honest timeout results. Do not add privileged game shortcuts.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`cargo test --locked tests::agent:: -- --test-threads=1` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
