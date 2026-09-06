<!-- wildforge:guide -->

# Working in player operations

Read [README.md](README.md) and [the source guidance](../AGENTS.md).
Keep gameplay eligibility and transaction rules shared by both adapters.
Network permission and rate checks precede these operations. Do not depend on
windowing, rendering, audio, MCP, or transport. Preserve existing effect order
in structural moves; isolate behavior corrections and document their evidence.
Use typed outcomes and narrow state inputs; avoid unrestricted Game access.
Target 400 lines, review above 500, and add guides for new subdirectories.
