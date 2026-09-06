<!-- wildforge:guide -->

# Graphical action arbitration

The parent `actions.rs` captures definition/target inputs once and runs handlers
in the established order: wand, portable items, preparations, held channels,
observation, station work, melee, mining, held use, fishing, then block use.
A handled result consumes the frame's remaining interaction pipeline. A channel
that only updates a timer can continue to later handlers as before.

`ActionFrame` contains immutable targeting inputs, not a world or actor owner.
Handlers are application coordinators: domain transactions live in player_ops
and World, while input, UI, presentation, and guest requests retain their owners.
Do not reread the selected definition or raycast midway through the pipeline;
scripts and inventory operations may already have changed live state.

Block use keeps one ordered match and its original guards. Its helpers are
organized into menus, exploration, household interactions, and machines; the
match decides whether ordinary placement follows. Other modules own held art,
magic feedback, field tools, projectiles, mob feedback, wand channels, and script
command application. Those helpers do not alter arbitration priority.

Read [AGENTS.md](AGENTS.md). Final checks include simultaneous buttons, held
channels, changing tools, occlusion, guest request/echo, script cancellation,
native gameplay proofs, and the complete repository Rust/GPU gates.
