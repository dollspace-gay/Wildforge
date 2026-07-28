# Dependency advisories

Advisories that are open against this tree, why they are still open, and what
would close them. One entry per advisory; delete an entry when it is gone.

---

## GHSA-q2qq-hmj6-3wpp — `hickory-proto` CPU exhaustion during message encoding

**Status: accepted, not exploitable in this usage. Assessed 2026-07-28.**

`hickory-proto`'s `BinEncoder` keeps name-compression candidates in a `Vec`
and searches it with a linear scan, so **encoding** a message with many
records is O(n²). Fixed upstream in 0.26.1.

### Why it is still here

There is no version of this tree that fixes it:

- We reach it through `jacquard-identity` -> `hickory-resolver` ->
  `hickory-proto`.
- `jacquard-identity 0.12.1` is the newest published version and it requires
  `hickory-resolver ^0.24`.
- `hickory-proto 0.24.4` is the last release on that line. The fix landed only
  in 0.26.1, and there is no backport.

So closing this needs an upstream release of `jacquard-identity` built against
hickory 0.26+. Nothing on our side can pull it forward.

### Why it does not reach us

The advisory needs a malicious message **with many records** to be *encoded*.
That is a DNS server or forwarder shape: something that builds large responses
out of untrusted input.

Wildforge is a stub client. The one thing it encodes is a single-question TXT
query for `_atproto.<handle>` (`identity/atproto.rs`, `handle_matches_did`),
built from a handle we have already parsed and length-bounded. One question,
one name — there is nothing for the quadratic scan to chew on. Responses are
decoded, not encoded.

### Why we do not simply drop the `dns` feature

Dropping `features = ["dns"]` from `jacquard-identity` would remove the
dependency outright, and `handle_matches_did` already falls back to
`https://<handle>/.well-known/atproto-did`.

It would also break handle verification for most real ATProto handles.
Measured 2026-07-28:

| handle | `/.well-known/atproto-did` | `_atproto` DNS TXT |
|---|---|---|
| `bsky.app` | 404 | resolves |
| `jay.bsky.team` | unreachable | resolves |
| `atproto.com` | 404 | resolves |

The well-known route is the fallback, not the common case. Trading working
verification for a vulnerability we cannot reach is a bad trade.

### What would change this

- `jacquard-identity` publishing against hickory 0.26+ — take it immediately.
- Wildforge ever encoding DNS messages it did not construct itself (a resolver,
  a forwarder, a cache that re-encodes). Then this becomes reachable and the
  answer changes.

Re-check with `cargo tree -i hickory-proto` when bumping the jacquard crates.
