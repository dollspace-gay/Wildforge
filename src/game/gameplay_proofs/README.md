<!-- wildforge:guide -->

# Native client scenario proofs

These scenarios run the real graphical client with a hardware Vulkan adapter.
`guest.rs` joins the shared loopback host, checks the first uploaded entry mesh,
exchanges ordinary protocol traffic with an agent, drives movement through the
frame update, and captures the resulting view before normal disconnection.
It also targets real water with a held boat and verifies unsupported guest use
refuses without spending inventory, then disconnects and rejoins the same host.
The JSON report includes entry latency, raw native update p50/p95/p99 timings,
initial terrain/mesh queue high-water counts, and process peak resident memory.
Timings exclude harness sleeps. Cold entry means fresh guest state with content
already initialized; warm entry follows full disconnect. The test profile and
small host fixture are functional measurements, not production benchmarks.

Run `python3 tools/run_gameplay_proofs.py --output <new-artifact-directory>`.
The runner supplies disposable content, saves, and identities. This is a
functional stage proof; full visual and performance campaigns use production
worlds separately. Read [AGENTS.md](AGENTS.md) before changing a scenario.
