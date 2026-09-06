<!-- wildforge:guide -->

# Working in gpu frame stages

Read [README.md](README.md), the parent renderer guide, and root AGENTS.
Preserve descriptor settings, resource formats, dynamic offsets, pass/culling
order, and capture provenance. Keep actual GPU execution; CPU reconstruction is
not rendering evidence. Maintain explicit imports and typed stage inputs.
Review files above 500 lines, and verify on real hardware after Rust/WGSL gates.
