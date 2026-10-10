# Dependency advisories

Advisories that are open against this tree, why they are still open, and what
would close them. One entry per advisory; delete an entry when it is gone.

---

## RUSTSEC-2023-0071 — `rsa` Marvin timing side channel

**Status: accepted, vulnerable operation is unreachable. Assessed
2026-07-30.**

The `rsa` crate's private-key operations are not constant time, so a remote
observer could recover an RSA private key from enough timing measurements.
There is no patched `rsa 0.9` release.

### Why it is still here

The dependency path is `jacquard-oauth 0.12.1` -> `jose-jwk 0.1.2` -> `rsa
0.9.10`. `jacquard-oauth` requests the P-256 and P-384 features from
`jose-jwk`, but does not disable that crate's default `crypto` feature; the
default also compiles RSA. Wildforge cannot turn off a default selected by a
transitive dependency. `jacquard-oauth 0.12.1` is the newest published
version.

### Why the vulnerable operation does not reach us

Wildforge registers as a public OAuth client with `keyset: None`
(`identity/atproto.rs`, `oauth_client_data`). It therefore has no client
authentication private key. Its only OAuth private key is the short-lived
DPoP key generated inside Jacquard:

- Jacquard's `generate_key` implements only `ES256` and creates a P-256 key.
- Jacquard's `build_dpop_proof` accepts only a P-256 secret and signs ES256.
- Wildforge's OAuth conformance fixture advertises only ES256.

No Wildforge or active Jacquard path constructs an RSA private key, decrypts
with RSA, or signs with RSA. Merely compiling the optional key representation
does not expose the timing oracle described by the advisory.

### What would change this

- A Jacquard release that disables `jose-jwk` defaults — take it and remove
  this exception.
- Wildforge adding confidential-client authentication, RSA DPoP, or any RSA
  private-key operation — this exception is no longer valid.

Re-check `cargo tree -e features -i rsa` and search the OAuth path for RSA when
upgrading Jacquard.

---

## RUSTSEC-2023-0089 — `atomic-polyfill` is unmaintained

**Status: accepted, no vulnerable behavior; removal is upstream-blocked.
Assessed 2026-07-30.**

This is a maintenance advisory, not a memory-safety or security vulnerability.
The archived `atomic-polyfill` crate is reached through `heapless 0.7`, whose
replacement is a newer major version using `portable-atomic`.

### Why it is still here

Wildforge disables Postcard's own default features and uses its allocation
APIs. `jacquard-common 0.12.1`, however, declares Postcard without
`default-features = false`, which globally enables Postcard's `heapless-cas`
feature and pulls in `heapless 0.7` -> `atomic-polyfill 1.0.3`. Cargo features
are additive, so Wildforge cannot disable that selection.

### Why the legacy implementation is not used

Wildforge encodes network and identity values into allocated `Vec<u8>` buffers
with `postcard::to_allocvec` and decodes byte slices. The Jacquard source path
used here likewise calls `postcard::from_bytes`; neither codebase instantiates
Postcard's optional heapless containers. The retired atomic compatibility
implementation is therefore compiled as feature baggage, not used by the
serialization path.

### What would change this

- A Jacquard release that disables Postcard defaults, or upgrades to a
  Postcard/heapless combination without `atomic-polyfill` — adopt it and
  remove this exception.
- Wildforge beginning to use Postcard's heapless serializers — reassess and
  replace the path before shipping.

Re-check with `cargo tree --target all -i atomic-polyfill` when upgrading
Jacquard or Postcard.

---

## RUSTSEC-2026-0249 — `smartstring` is unmaintained

**Status: accepted maintenance risk; removal is upstream-blocked. Assessed
2026-08-14.**

The `smartstring` repository was archived by its owner on 2026-05-03. The
advisory reports no memory-safety defect or exploitable behavior; it records
that the crate no longer receives maintenance.

### Why it is still here

The dependency path is `wildforge` -> `rhai 1.25.1` -> `smartstring 1.0.1`.
Rhai uses `smartstring::SmartString` throughout its tokenizer, parser, scopes,
modules, and public `ImmutableString` representation, so it is not an optional
feature Wildforge can disable. Rhai 1.25.1 is the latest published release,
and the upstream 1.26.0 development manifest still declares `smartstring`.

Replacing the crate locally would amount to maintaining a fork of the
scripting engine's central string type. That would add substantially more
security and compatibility risk than carrying this explicit maintenance-only
exception while Rhai remains maintained.

### Why there is no vulnerable operation to fence

Unlike a vulnerability advisory, RUSTSEC-2026-0249 identifies no unsafe or
attacker-controlled operation. Wildforge already bounds Rhai script operations,
call depth, expression depth, and data sizes; those controls remain unchanged.
The residual risk is that a future defect in `smartstring` will not be fixed
upstream, not a known exploit in the current code.

### What would change this

- A Rhai release that replaces `smartstring` with a maintained string type —
  adopt it immediately and remove this exception.
- Rhai becoming unmaintained, or a concrete `smartstring` vulnerability being
  published — replace or fork the scripting dependency rather than extending
  this exception.

Re-check with `cargo tree -i smartstring` whenever Rhai is upgraded.
