<!-- wildforge:guide -->

# Terrain job changes

Read [README.md](README.md) and [root guidance](../../AGENTS.md).
Keep this owner independent of UI, winit, GPU resources, audio, and wire types.
Use immutable context and explicit prepared results. Preserve caller budgets,
deduplication, entry priority, saved-terrain precedence, and exactly-once
authoritative adoption. Do not add another generator or save decoder here.

Test queue transitions without timing assumptions and use actual worker and
host/client integration tests for lifecycle/adoption changes. Keep new files
cohesive and below the soft line-size thresholds. Record behavior corrections
separately from structural extraction.

Keep generation and decoding on the same registry. Context comparison must
include the atlas, seed, saved palette, and World session. On replacement, join
old workers before starting new ones; retain startup failure against the desired
context so normal frame/pump calls cannot retry indefinitely.
