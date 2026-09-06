<!-- wildforge:guide -->

# Working in script loading

Read [README.md](README.md) and [the source guidance](../AGENTS.md).
Compilation is read-only preparation; publication replaces the whole prepared
set. Do not dispatch scripts or mutate KV/queued world commands during prepare.
Propagate file and compilation failures. Preserve existing event/sandbox limits
and previous-AST behavior for callers of `load_mods`. Keep imports explicit and
modules around 400 lines; document new subdirectories. Run focused script and
reload scenarios plus the applicable Rust gates before acceptance.
