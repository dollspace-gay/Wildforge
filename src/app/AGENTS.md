<!-- wildforge:guide -->

# Working on application commands

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).
Preserve command precedence even when several command flags occur in argv.
Retain diagnostics, exit statuses, defaults, and output publication order.
Keep process argument and terminal behavior here; domain operations should not
depend on this module or client UI. Reuse domain validation and persistence.

Use disposable output directories for command checks. Include CLI contracts and
applicable Rust gates in the final validation phase. New folders need local
README/AGENTS guidance; aim for coherent modules around 400 physical lines.
