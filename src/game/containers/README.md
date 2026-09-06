<!-- wildforge:guide -->

# Graphical inventory transactions

Inventory/crafting, cargo, stall, equipment, smelting and workbench adapters call
the existing shared player operations or send the established guest request.
The exchange coordinator checks the selected panel against the received physical
container and applies local/replica prediction through the runtime owner.

Layout helpers serve both input and drawing. Script-screen controls stay separate
from physical station crafting. Preserve request-before-prediction order, full
ItemStack identity, refusal behavior and the supported/unsupported guest routes.
Do not duplicate eligibility or transaction rules from player_ops here.

Run applicable UI/session/inventory characterization and parity tests, then the
root Rust gates and final native gameplay/GPU proofs from the repository root.
