<!-- wildforge:guide -->

# Gameplay proof mod set

Only the proof/ mod is shipped here; the runner copies it into a disposable game directory.

Start with the documented child directories. See the [repository overview](../../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Do not depend on ignored locally installed mods. Preserve the exact fixture set used by the runtime proof.

## Focused checks

```sh
python3 tools/run_gameplay_proofs.py
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
