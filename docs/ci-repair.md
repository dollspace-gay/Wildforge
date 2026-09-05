# CI repair for PR 86

Run `33923376213` failed every required job. Five jobs never invoked rustc:
`.cargo/config.toml` required a Windows-local `sccache` executable that is absent
on both Ubuntu runners and the cargo-deny container. The format job stopped on
62 unformatted Rust files. The MSRV log also revealed that the repository's
1.96 toolchain pin overrode the job's installed 1.95 default.

## Corrections

- **Portable build configuration (root cause):** remove the unconditional
  compiler wrapper and 16-job machine assumption. Cargo selects host defaults;
  developers with sccache can opt in using `RUSTC_WRAPPER=sccache`.
- **Actual MSRV (local correctness):** invoke `cargo +1.95.0 check --locked
  --all-targets` explicitly. The package MSRV remains 1.95.
- **Complete test inputs (root cause):** the eight baseline failures depended
  on an ignored, external `mods/belt_quest` installation that CI never supplied.
  `tools/install_test_mods.py` installs the public content author's repository,
  `eternaldensity/belt-quest`, at commit
  `0f090889f2de692b47526d4f9da9f5dd8864ab57`. CI and local development use that
  same installer. It checks the fetched commit and refuses to overwrite an
  existing mod; no test is removed or skipped.
- **Formatting (local correctness):** run Rustfmt 1.96 over the 62 failing
  files. Every changed Rust file was compared with rustfmt's exact output for
  its contents at `7e79e1562f96ec5ad62887e13ad99eb42db70203`; there are no
  handwritten Rust behavior changes in this CI repair.
- **Dependency audit (local correctness):** after fixing Cargo startup, the
  audit exposed [RUSTSEC-2026-0258](https://github.com/hyperium/hyper/security/advisories/GHSA-q83h-524g-xf6h).
  Update `h2` from 0.4.15 to the patched 0.4.16, and update the yanked
  `chacha20` 0.10.1 to 0.10.2. No advisory exceptions are added.

## Visual evidence and formatting

The visual validator hashes source bytes, so formatting also requires a
reviewed source-digest refresh. The two pre-format digests were first verified
against the source at `7e79e15`. For every file in each source set, the new
contents were required to be either byte-identical or exactly the output of:

```sh
rustfmt --edition 2024 --emit stdout --config skip_children=true
```

Only `src/game/frame.rs`, `src/game/mod.rs`, `src/lib.rs`, and `src/world/mod.rs`
changed across those source sets, all through rustfmt. Per-file hashes and both
aggregate before/after hashes are preserved in
[ci-format-refresh.json](evidence/gameplay/ci-format-refresh.json).
The manifest's two source digests were updated only after that comparison.
Capture commits, pixel hashes, performance reports, validation logic, and
acceptance thresholds are unchanged. This is a formatting provenance update,
not a claim to have recorded new visual captures.

## Validation commands

The same six GitHub checks remain required. No failure is hidden with
`continue-on-error`, a reduced feature set, or extra test skips.

```sh
python3 tools/install_test_mods.py  # once, into an absent mods/belt_quest
cargo fmt --all -- --check
actionlint .github/workflows/ci.yml
cargo +1.95.0 check --locked --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets -- --skip tests::agent::
cargo test --locked tests::agent:: -- --test-threads=1
cargo test --locked --doc
cargo build --locked --release
cargo deny --all-features check advisories
```

The gameplay proof logs remain historical evidence from the initial fix.
Current CI results are attached to [PR 86](https://github.com/dollspace-gay/Wildforge/pull/86).
