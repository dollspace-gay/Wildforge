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

`craft.rs` owns blueprint/cursor eligibility, ingredient/repair consumption,
and the ordered Current retirement, material loss, and byproduct effects.
The graphical adapter keeps its existing tech-KV gate, XP, script callback, and
prediction policy. `equipment.rs` owns armor/charm slot eligibility and cursor
exchange; component return remains with the loadout owner.

`container.rs` applies chest/offering/stall, furnace, and recognized machine
slot transactions to explicit storage and cursor inputs. Transport authorization
stays in the host adapter; local furnace XP is returned as an effect. Crafting
shares byproduct inventory publication while only authority retires charged
inputs and records material losses. Guest prediction has no ledger capability.

`nutrition.rs` owns meal admission and bounded hunger/nutrient updates.
`feeding.rs` prepares animal eligibility and applies tame/breed/calm state to an
explicit member. Each adapter retains its prior inventory/accounting order and
prediction policy. Inventory/crafting grid clicks already share
`inventory::click_stack`; their different area enums stay at their boundaries.

`combat.rs` owns tangent-frame backstab geometry and heavy/backstab multiplication.
Adapters supply admitted base damage and flags. Existing local/host differences
(host damage cap, creative backstab eligibility, extra local heavy impulse,
stamina and hunger/ire policies) remain explicit adapter behavior in this
structural move. The host no longer imports graphical combat code.
