<!-- wildforge:guide -->

# Working in test-fixtures/gameplay/mods

Read [README.md](README.md) and [the root instructions](../../../AGENTS.md).

Only the proof/ mod is shipped here; the runner copies it into a disposable game directory.

Do not depend on ignored locally installed mods. Preserve the exact fixture set used by the runtime proof.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`python3 tools/run_gameplay_proofs.py` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
