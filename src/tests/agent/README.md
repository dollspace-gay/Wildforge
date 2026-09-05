<!-- wildforge:guide -->

# Agent protocol scenarios

These child modules exercise the ordinary guest over the real loopback QUIC
transport. They reuse the deterministic stage and owned host thread in
`../agent.rs`. `admission.rs` pauses simulation after a normal join to control
subsequent host messages without changing the production client.
`replication.rs` uses the same paused host to check authoritative entity fields
received by the actual agent adapter.

Run `cargo test --locked tests::agent:: -- --test-threads=1` from the root.
Read [AGENTS.md](AGENTS.md) before adding scenarios.
