<!-- wildforge:guide -->

# Working on workings scenarios

Read [README.md](README.md) and [the parent instructions](../AGENTS.md).

Use the shared fixture helpers for common setup and keep each test focused on
an observable contract. Preserve protocol ordering and exact ledger assertions.
Aim for cohesive files around 400 lines and review those above 500. Add a named
scenario module when a new contract no longer belongs with its current family.
Run the module filter during edits and the complete required suite at acceptance.
