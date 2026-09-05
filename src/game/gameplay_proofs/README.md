<!-- wildforge:guide -->

# Native client scenario proofs

These scenarios run the real graphical client with a hardware Vulkan adapter.
`guest.rs` joins the shared loopback host, checks the first uploaded entry mesh,
exchanges ordinary protocol traffic with an agent, drives movement through the
frame update, and captures the resulting view before normal disconnection.

Run `python3 tools/run_gameplay_proofs.py --output <new-artifact-directory>`.
The runner supplies disposable content, saves, and identities. This is a
functional stage proof; full visual and performance campaigns use production
worlds separately. Read [AGENTS.md](AGENTS.md) before changing a scenario.
