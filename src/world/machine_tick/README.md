<!-- wildforge:guide -->

# Machine tick coordinators

Named on-grid food/storage, clamps, station, steam and entity tick stages retain
their original simulation order. Shared bloomery, forge, kiln and separator
algorithms operate over BlockStore and serve both World and LocalStructure.
The dispatch adapter chooses the same sequence and does not duplicate recipes.

World installations use their material ledger; independent structures retain
structure-local state/outboxes and the existing absence of world ire/accounting.
Preserve weather, power, custody and output order when changing a shared machine.

Run the applicable world/machine, sidecar compatibility, conservation and replay
tests, then root Rust gates and final native evidence from the repository root.
