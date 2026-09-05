<!-- wildforge:guide -->

# Shared scenario fixtures

`host.rs` owns a real loopback QUIC host and its simulation thread. Its prepared
stage keeps protocol and client behavior tests independent of terrain generation.
Agent scenarios and native graphical guest proofs use the same fixture.

`pause` joins simulation while leaving transport alive for controlled host
messages. Dropping the fixture stops and joins the owned thread. Run the serial
agent suite and `python3 tools/run_gameplay_proofs.py` for affected client paths.
Read [AGENTS.md](AGENTS.md) before changing fixture behavior.
