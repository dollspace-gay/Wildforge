<!-- wildforge:guide -->

# Working in src/net

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

protocol.rs defines wire values; handshake.rs authenticates admission; transport.rs owns QUIC channels; content.rs owns mod hashes and transfer inventories.

client.rs owns guest endpoint/runtime and stream lifetimes. Finish reliable
messages before closing the connection, drain the endpoint while its runtime
is alive, and join every owned stream task. Do not replace completion with a
fixed sleep or rely on peer idle timeout as ordinary disconnection.

Keep transport separate from gameplay. Preserve protocol compatibility, snapshot bounds, host-key checks, and data/script boundaries.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`cargo test --locked net::` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.

New folder guides opt out of mod identity only when named README.md or
AGENTS.md and beginning with `<!-- wildforge:guide -->`. Historical unmarked
documents remain content. Never broaden this rule to runtime data or change
legacy path hashing as part of a structural refactor.
