<!-- wildforge:guide -->

# Working in graphical actions

Read [README.md](README.md) and [the parent guidance](../AGENTS.md).
Preserve handler order, captured inputs, early return behavior, and simulation
versus cosmetic RNG ownership. A false handler result permits the next handler;
use true only where the original operation consumed further interaction.

Keep protocol admission and presentation here; shared eligibility and physical
transactions belong in player_ops or the owning World domain. Guest paths must
send their request or use explicit replica prediction before borrowing local
authority. Never replace rejected physical transactions with local success.

Aim for 400 lines and review above 500. Run the root Rust gates and native
`python3 tools/run_gameplay_proofs.py` after the implementation phase, including
input priority and guest request/echo regressions for changed handlers.
