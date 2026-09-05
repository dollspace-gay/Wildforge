<!-- wildforge:guide -->

# Working in shared fixtures

Read [README.md](README.md) and [the test guidance](../AGENTS.md).

Keep fixtures test-only. Own threads, transports, and disposable state for their
whole lifetime; join before dropping or repurposing them. Preserve the stage's
seed, spawn, terrain, weather, and wildlife policy during structural moves.
Production clients must still use ordinary admission, messages, and actions.
Use bounded condition waits when checking events from the host thread.
