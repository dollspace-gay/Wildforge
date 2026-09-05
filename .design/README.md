<!-- wildforge:guide -->

# Designs and migration evidence

This directory holds reviewed architecture intent, measurement snapshots, and
execution records. Start with [maintainability-refactor.md](maintainability-refactor.md)
for requirements and [maintainability-progress.md](maintainability-progress.md)
for current implementation status. `maintainability-baseline.json` is the
pre-migration source-size/clone observation, not a suppression list.

Pipeline JSON identifies its design by SHA-256. Update the hash when changing
the referenced design. Preserve historical baseline evidence and record new
observations separately. Acceptance checkboxes require implementation and
verification evidence; a passing narrow test cannot establish a whole phase.
