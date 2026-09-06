<!-- wildforge:guide -->

# Block-entity sidecars

The existing input schema is isolated in schema.rs; save.rs keeps the versioned
text writer and load.rs reconstructs installations in the original family order.
Serde defaults, accepted versions, item-name lookup, optional charge/durability,
water/salt fields, belt progress and depot identity are unchanged.

Private schema fields are visible only to the sidecar module. This separation
provides a readable compatibility boundary; it does not replace the writer with
a different serializer or alter unreadable/unsupported-file behavior.

Run the applicable world/machine, sidecar compatibility, conservation and replay
tests, then root Rust gates and final native evidence from the repository root.
