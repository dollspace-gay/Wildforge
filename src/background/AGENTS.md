<!-- wildforge:guide -->

# Working on background owners

Read [README.md](README.md) and [root guidance](../../AGENTS.md). Keep this
boundary independent of game state, content, rendering, transport, and platform
event loops. Snapshot processors receive owned immutable inputs and return data.

Bound queued, running, and ready work together. Stop accepting requests before
cancellation, wake waiters, join every started thread even after partial startup
failure, and preserve observable failure causes. Recover poisoned locks solely
to stop work. Do not hide errors with detached threads or synchronous fallback.

Test lifecycle transitions with channels/barriers rather than assumptions about
processor speed. Preserve adapter policies, use focused integration tests, and
record native GPU/runtime verification separately from CPU worker tests.

One-shot operations have a separate terminal result from their latest progress.
Do not discard that result when cancellation races with durable publication.
Supervise context initialization as well as processing, and test both paths.
