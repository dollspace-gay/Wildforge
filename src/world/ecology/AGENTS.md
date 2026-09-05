<!-- wildforge:guide -->

# Working in ecology

Read [README.md](README.md) and [world guidance](../AGENTS.md).
Population owns collection/cursor state. World coordinates cross-domain custody
and side effects; do not add ledger copies or expose mutable collection vectors.
Preserve spawn/admission and death/drop ordering, lazy identity timing, host
budgets, deterministic rolls, and unloaded-chunk behavior. Guest presentation
has a separate ReplicaWorld and cannot use this simulation path.

Aim for 400 lines and review above 500; split by actual responsibility and keep
cohesive transaction order visible. Add guides to new directories. Final checks
must include successful/rejected admission, save restoration, stable IDs, NPC
links, physical accounting, and behavior parity. Report unrun checks honestly.
