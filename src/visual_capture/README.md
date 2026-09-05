<!-- wildforge:guide -->

# Visual campaign identity

`campaign.rs` selects the current complete evidence directory and fingerprints
the Rust, shader, verifier, and shipped content inputs. The capture and metric
contracts currently remain in `../visual_capture.rs`.

`native.rs` binds the complete matrix to the foundation's recorded discrete GPU,
backend, and content identity. Other adapters and content generations need their
own complete campaign; individual captures cannot silently change either one.

A campaign directory contains its own `screenshots/` tree, so a fresh campaign
does not overwrite the preceding raw metadata and reports. Run
`cargo test --locked visual_capture::tests::` after changes. A passing source
freshness check requires a complete new campaign on native GPU hardware.
