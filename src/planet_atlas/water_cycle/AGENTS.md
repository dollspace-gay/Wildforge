<!-- wildforge:guide -->

# Working in water_cycle

Read [README.md](README.md) and [the atlas guidance](../AGENTS.md).
Keep integer units, proportional/exact parcel distinctions, salt remainders,
reservoir IDs, and deterministic debit order. Validate before committing a
cross-reservoir operation; rejected requests must preserve ownership. Never
replace conserved mass with a displayed level or capacity. Keep codec widths
and record order stable and diagnostics grounded in actual ledger state.
Use explicit imports, with the existing ledger as the accounting owner. Aim
for 400 lines per cohesive module and review above 500. Run focused water,
codec/recovery, and applicable full gates before acceptance; record limitations.
