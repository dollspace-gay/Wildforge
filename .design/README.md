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

`architecture-boundaries.json` is the explicit written-dependency contract for
established owners. It is source policy rather than a scan-derived exemption
list. Keep migration seams and test allowances specific, and review changes
alongside the owning modules.

The final implementation inventory is `maintainability-final.json`, compared
against the recorded main baseline. [clone-review.md](clone-review.md) records
retained candidate decisions and [source-size-review.md](source-size-review.md)
records exact oversized-source exceptions. Neither file suppresses the analyzer.

[maintainability-acceptance.md](maintainability-acceptance.md) consolidates the
final criterion-to-proof mapping, gate commands and measurement limits.
Generation hashes, native lifecycle observations and the selected hardware
campaign complement the ordinary Rust/Python fixtures.
