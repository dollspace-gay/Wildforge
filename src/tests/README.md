<!-- wildforge:guide -->

# Behavioral and integration suites

Domain scenarios share small fixtures from mod.rs. Agent tests exercise real QUIC guests in a separate serial lane.

`fixtures/host.rs` owns the deterministic threaded host shared by agent scenarios
and native graphical guest proofs. Pause joins its simulation before controlled
wire injection; normal drop also stops and joins the fixture.

Start with `agent.rs`, `alchemy.rs`, `archetypes.rs`, `atlas.rs`, `belt.rs`, `climate.rs`, `dross.rs`, `dungeon.rs`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Test contracts and observable outcomes. Preserve production-shaped qualification alongside deterministic fixtures. Avoid duplicating setup that encodes one invariant.

## Focused checks

```sh
cargo test --locked --all-targets -- --skip tests::agent::
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
