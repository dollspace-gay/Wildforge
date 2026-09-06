<!-- wildforge:guide -->

# Screen composition

The parent `ui.rs` owns the frame's UiBatch, polls asynchronous menu state,
and chooses a screen. Menu-only screens skip status and gameplay overlays;
in-world screens draw those layers first, then their panel. The tooltip is last.

Screen modules borrow the batch and use the existing inventory-panel and widget
contracts. They may read the graphical runtime and navigation state, but physical
transactions belong to input/action adapters and shared player operations.
Layout modules supply the same geometry to drawing and hit testing. World labels
project read-only observations; missing replica data is not generated here.

Inventory drawing remains in `inventory_ui.rs`; this directory does not wrap or
duplicate that renderer. Station and storage painters retain their distinct
availability, labels, and controls. A screen's early exit returns to the batch
owner so the frame still publishes its UI and draws the final tooltip.

Run UI characterization and the native gameplay/capture proofs from the repository
root, followed by the full Rust and hardware gates documented in the root README.
