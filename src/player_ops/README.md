<!-- wildforge:guide -->

# Authoritative player operations

Local play and authenticated network handlers call the same transactions here.
`trade.rs` owns the stall purchase: check stock, payment, and till capacity;
commit inventory changes; return overflow for the adapter's ordinary drop path.
Reach, moderation, transport replies, UI feedback, and scripts remain adapters.

Operations receive only their affected state. Validate before the first effect,
keep item identity and conservation accounting explicit, and return typed results.
Read [AGENTS.md](AGENTS.md). Final verification includes local/network parity,
rejected transactions, capacity boundaries, and applicable repository gates.
