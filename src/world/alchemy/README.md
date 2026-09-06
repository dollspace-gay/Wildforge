<!-- wildforge:guide -->

# Authoritative alchemy integration

The action dispatcher routes named apparatus requests. Carrier/filter loading,
processing, charging, decanting, cleaning/draining/repair, ordinary carrier work,
and batch input have separate transaction coordinators. Preparation use, storage
aging/destruction, status ticking, and apparatus ticking retain their distinct
custody boundaries. Pure liquid/material, inventory, failure, status and apparatus
helpers share the existing rules without receiving World.

The existing AlchemyState and Current/material/water ledgers remain the authority
owners. Preserve clone/preflight/commit/physical-application order and all stable
container, batch, status and operation identities. Failed and spoiled preparations
remain physically accounted outcomes; they are not silently discarded.

These modules coordinate cross-domain transactions; moving a method here does
not make it an independent simulation owner. Public World entry points and save
formats are preserved. Run the applicable world, alchemy, conservation, replay,
and multiplayer parity tests followed by the root Rust/native gates.
