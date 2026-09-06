<!-- wildforge:guide -->

# Working in workings integration

Read [README.md](README.md), the parent world guide, and root AGENTS.
Keep conserved quantities in their existing ledgers. Preserve stable identities,
version checks, journal/file replacement order, rollback, replay and physical
application. Do not expose arbitrary script mutation through a named operation.
Use explicit imports and keep helpers at the narrowest domain visibility.
Review files above 500 lines with an exact responsibility and review point;
do not divide a commit/recovery sequence into unrelated numbered fragments.
Validate rejection and interrupted-replay paths as well as successful operations.
