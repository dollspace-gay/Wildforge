<!-- wildforge:guide -->

# Working in registry linking

Read [README.md](README.md) and [the registry guidance](../AGENTS.md).
Separate raw parsing, name resolution, graph validation, and publication.
Keep dependencies explicit and functions visible only within the registry.
Preserve ordering and errors in structural extraction. Do not publish a partly
linked registry. Target 400 lines per cohesive module; review above 500.
