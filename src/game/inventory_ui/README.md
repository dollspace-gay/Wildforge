<!-- wildforge:guide -->

# Inventory presentation

The parent owns the inventory screen composition and discovery-row count.
Discovery cards/catalogue, detailed status/tab painting and equipment/crafting
panels borrow UiBatch and use the same inventory layout and widget contracts.
Their read-only views retain missing-data behavior and all original text/geometry.
Physical clicks and crafting belong to containers/player operations, not drawing.

Run applicable UI/session/inventory characterization and parity tests, then the
root Rust gates and final native gameplay/GPU proofs from the repository root.
