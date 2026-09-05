<!-- wildforge:guide -->

# Working in src/npc

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

NPC state links authored behavior to authoritative mobs and world progression.

Preserve stable NPC/mob identities and quest/reputation outcomes. Keep presentation and dialogue widgets outside the runtime owner.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`cargo test --locked tests::registry_tests::` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
