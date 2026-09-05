<!-- wildforge:guide -->

# Working on world shaders

Read [README.md](README.md) and [the source instructions](../AGENTS.md).
The source assembly order is explicit in `mod.rs`. Structural moves preserve
all bytes, entry-point names, binding locations, uniform layout, constants, and
arithmetic. Keep authored units cohesive and around 400 physical lines.

Real rendering stays on wgpu hardware paths. Do not replace GPU work with CPU
approximations or disable evidence pipelines to satisfy checks. Use the same
aggregate for production compilation and Naga validation. Preserve diagnostic
class/family/depth semantics and ordinary terrain/overlay paint order.
Run validation and fresh GPU qualification in the final test phase.
