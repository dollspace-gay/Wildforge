<!-- wildforge:guide -->

# Working in Wildforge

## Repository contract

Wildforge is a Rust voxel game with one authoritative simulation shared by
solo, windowed-host, and dedicated-server play. Read [README.md](README.md)
and the nearest folder's README and AGENTS files before changing that area.
The active architecture work follows
[the refactor plan](.design/maintainability-refactor.md); implementation evidence
lives in [the progress record](.design/maintainability-progress.md).

- Preserve gameplay, deterministic world generation, content identities,
  saves, protocol compatibility, and authority boundaries in structural changes.
- Give state and behavior a coherent owner. Prefer explicit, narrow imports
  and domain operations over unrestricted access to `Game` or `World`.
- Aim for 400 physical lines per source file; review files above 500. Split at
  real responsibilities and keep useful documentation/tests. Size is advisory.
- Review clone candidates for shared semantics before extracting them. Keep
  domain rules in one place when local and network callers must agree.
- Keep structural moves and behavior corrections separately reviewable.
- Preserve unrelated work and ignored user saves, identities, screenshots,
  installed mods, and local settings. Use disposable runtime fixtures.
- Every maintained repository directory, including new subdirectories, needs
  a local `README.md` and `AGENTS.md` with its purpose, boundaries, and checks.
  Git metadata, build outputs, and ignored personal/runtime data are not source
  directories and are not documentation targets.

## Verification

Run focused behavioral tests while developing; before accepting an affected
Rust slice run format, strict Clippy, MSRV, subsystem tests, serial agent tests,
doctests, and release build using the commands in the root README. Streaming,
session, and rendering work also needs reproducible runtime evidence. GPU
rendering must be checked on the actual hardware; CPU fallback is not proof.

Python tooling uses `python3 -m unittest discover -s tools/tests -v`.
Run `python3 tools/check_maintainability.py` and `git diff --check` for every
slice. Record failures and incomplete checks honestly in the progress record.
Local checkpoints are separate from publishing; do not push or communicate
with other people unless the user has authorized that action.
