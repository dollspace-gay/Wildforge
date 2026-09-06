<!-- wildforge:guide -->

# Player status adapters

Feedback/toasts, nutrition, survival and item pickup remain ordered graphical
adapters. Shared nutrition rules live in player_ops; local survival applies the
existing world/gear/status effects and guest presentation consumes authoritative
state. Keep physics and presentation RNG effects in their original order.
Pickup preserves stable identity and complete physical stacks; inventory or world
custody failures must not be silently represented as successful collection.

Run applicable UI/session/inventory characterization and parity tests, then the
root Rust gates and final native gameplay/GPU proofs from the repository root.
