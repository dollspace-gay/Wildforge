<!-- wildforge:guide -->

# Working on post-processing

Read [README.md](README.md) and [the renderer instructions](../AGENTS.md).
Keep post resources private to their owner. Resize replaces complete target and
binding sets while retaining compatible pipelines. Preserve HDR formats,
half-resolution bloom dimensions, exposure arithmetic, pass order, and the
cleared bloom target when bloom is disabled. Do not sample an attachment that
the same pass writes.

Render and capture paths use the same GPU operations. Target cohesive files
around 400 physical lines and defer validation to the final test phase.
