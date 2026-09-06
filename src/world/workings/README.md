<!-- wildforge:guide -->

# Authoritative workings integration

Named admission modules construct only declared effects. Ritual/wand reservation,
target validation, physical/inventory application, lifecycle, and persistence
remain separate transaction responsibilities. The parent defines pure geometry,
snapshot, identity and strain helpers plus Settlement, keeping the old API paths.

The reservation and settlement coordinators span the existing workings state,
Current accounts, material journals, world effects, and saved player profiles.
Their order is part of recovery behavior: validate before journaling, preserve
PendingApply until the relevant inventory/world save completes, then settle.
They do not own a second copy of any conserved account.

These modules coordinate cross-domain transactions; moving a method here does
not make it an independent simulation owner. Public World entry points and save
formats are preserved. Run the applicable world, workings, conservation, replay,
and multiplayer parity tests followed by the root Rust/native gates.
