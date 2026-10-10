<!-- wildforge:guide -->

# Visual campaign identity

`campaign.rs` selects the current complete evidence directory and fingerprints
the Rust, shader, verifier, and shipped content inputs. Production capture identity and WFD encoding remain in `../visual_capture.rs`.
Test schemas live in `models.rs`; `evidence.rs` checks paths, bytes and metrics.
`manifest.rs`, `geode_manifest.rs`, `geode_group.rs` and `closeout.rs` validate
their declared campaign stages without relaxing source or hardware identity.

`native.rs` binds the complete matrix to the foundation's recorded discrete GPU,
backend, and content identity. Other adapters and content generations need their
own complete campaign; individual captures cannot silently change either one.

`render_work.rs` independently validates the versioned 120-frame host/device
receipt; `geode_performance.rs` recomputes the paired CPU/GPU budget. These test
validators do not share median calculations with the native game collector.
See [the campaign protocol](../../docs/visual-qualification-campaign.md) for
measurement scope, retained presentation diagnostics, and unchanged budgets.

A campaign directory contains its own `screenshots/` tree, so a fresh campaign
does not overwrite the preceding raw metadata and reports. Run
`cargo test --locked visual_capture::tests::` after changes. A passing source
freshness check requires a complete new campaign on native GPU hardware.
