<!-- wildforge:guide -->

# Visual campaign identity

`campaign.rs` selects the current complete evidence directory and fingerprints
the Rust, shader, verifier, and shipped content inputs. The capture and metric
contracts currently remain in `../visual_capture.rs`.

A campaign directory contains its own `screenshots/` tree, so a fresh campaign
does not overwrite the preceding raw metadata and reports. Run
`cargo test --locked visual_capture::tests::` after changes. A passing source
freshness check requires a complete new campaign on native GPU hardware.
