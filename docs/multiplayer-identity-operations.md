# Multiplayer identity and public-server operations

Implemented 2026-07-22. This is the operator/player companion to
`multiplayer-identity-plan.md`.

## What an identity means

Every installation creates an Ed25519 device key in `identity/` and every
player explicitly accepts an editable Wildforge name on first run. The name is
presentation only. Solo and LAN play do not need an internet connection or an
online account, and the old OS-name behavior is never transmitted without the
player saving the proposed name.

A server maps an authenticated device key, and optionally an ATProto DID, to a
server-issued `PlayerId`. World state lives under
`saves/<world>/players/<player-id>.toml`; changing a handle or display name does
not move the save. The historical `player.toml` is copied with a backup before
being migrated and is removed only after the new path saves successfully.

The Accounts screen can link ATProto. OAuth runs in the browser with PKCE,
PAR, DPoP, state/issuer/sub validation supplied by Jacquard. Wildforge asks for
only `atproto repo:gay.dollspace.wildforge.device`, writes one public
device-binding record, reads it back through the same public route a server
will use, logs out, and drops the in-memory OAuth store. It keeps only the
non-secret DID, handle, device public key, binding record key, and profile
preferences in `identity/atproto.toml`. OAuth tokens are never written to disk
and never enter a game packet, world, log, or server.

## Server policy

The first host run creates `saves/<world>/server.toml`:

```toml
identity = "local"                 # local | atproto_optional | atproto_required
admission = "open"                 # open | allowlist
port = 27431
verification_grace_secs = 3600     # 0 disables outage cache; hard max is 7 days
```

LAN beacons and the authenticated challenge advertise both policies before
content sync or ATProto disclosure. A local-only server never receives a
linked DID. Before the stock client can disclose a linked DID to an optional or
required server, the Join screen requires a second confirmation click and says
that the server can resolve the public identity. `atproto_required` is the
sensible public-server baseline; `atproto_optional` is intended for mixed
private communities, not strong ban-evasion resistance.

Run a dedicated host without a browser or graphical session:

```text
wildforge --server <world>
```

The console accepts:

```text
players
identity <connection-id>
kick <connection-id>
mute <connection-id> [seconds]
ban <connection-id> [seconds|perm]
allow <connection-id>
role <connection-id> <player|moderator|admin>
unban <player-uuid>
```

The windowed pause menu exposes the same connected-player identity summary,
kick, mute, timed/permanent ban, allowlist, and role operations with a
confirmation click for disruptive actions. Connected moderators and admins
receive the applicable controls too, but the host independently authorizes
every request from its durable role store; hiding or forging a client control
cannot grant authority. Durable changes are appended to
`saves/<world>/moderation/audit.log`. Bans target the PlayerId and every linked
principal, never a display name. IP throttles are deliberately only a soft
handshake-abuse signal.

## Trust, privacy, and recovery

- A host certificate key is installation-wide at
  `identity/server-ed25519.pk8`; copying a world does not copy it. Clients use
  TOFU pins in `identity/known-hosts.toml` and refuse a changed fingerprint
  with both expected and observed short values. Verify the change out of band
  before removing that one host entry and reconnecting.
- Losing `identity/player-ed25519.pk8` loses that local-device credential. A
  still-linked ATProto account can authorize a new device record and recover
  the same server profile. Otherwise an operator must perform a deliberate
  profile/principal recovery; copying inventory is never automatic.
- **Revoke this device** reauthenticates, deletes the public binding record,
  and re-fetches it with a bounded retry. Local metadata is removed only after
  the PDS returns a definite `RecordNotFound`; network or verifier errors leave
  it intact so the player can retry. **Unlink locally** only removes local
  metadata and clearly warns that the remote record may remain. A deleted
  record stops live verification; a server may show `VERIFIED/CACHED` only
  within its configured outage grace.
- A verified server learns the DID and can resolve public account metadata.
  Peers receive a verification badge and, only when separately enabled, the
  player's current public handle. The DID is never roster data. Social display
  name, handle sharing, and avatar preferences are separate opt-ins. Avatar
  download/rendering is intentionally deferred; no remote image is fetched in
  the render loop. Hold Tab in multiplayer to inspect the complete roster.
- Back up `identity/player-ed25519.pk8` and, for hosts,
  `identity/server-ed25519.pk8` as secrets. Never publish either file. The
  public fingerprints, PlayerIds, DIDs, and binding records are not secrets.

## ATProto proof decision record

Wildforge chose the repository binding over a central signed-ticket service.
The NSID is `gay.dollspace.wildforge.device`, backed by the checked-in Lexicon
and OAuth metadata assets. This preserves self-hosting, permits several device
records per account, survives handle changes and PDS migration, and makes
revocation an ordinary record deletion.

The verifier resolves `did:plc` or `did:web` on every live check, selects the
current ATProto PDS service, and fetches the exact collection/rkey. It accepts
the response of the DID-authorized PDS rather than downloading and validating
the repository CAR/CID chain. That is an explicit provider-trust boundary:
compromise of the account's current PDS can forge the response. URL policy is
HTTPS-only, forbids credentials/redirects/private addresses, pins screened DNS
answers, caps responses at 64 KiB, and bounds connect/total time. Communities
that require cryptographic repository proofs should keep the server private
until a signed-CAR verifier replaces this mode.

Handles are display metadata, never account keys. A handle found in the DID
document is shown only when its DNS TXT or public HTTPS handle resolution maps
back to the same DID. Handle resolution runs alongside proof lookup with its
own short timeout, so unavailable handle infrastructure does not invalidate an
otherwise valid device proof.

Jacquard 0.12.1 component crates were selected instead of implementing OAuth
or DPoP. The native loopback client profile is used by the binary; the
production deployment
payload is `docs/atproto-oauth-client-metadata.json` and must be published at
its exact HTTPS `client_id` before a hosted-metadata release profile is enabled.
The production document is served from the `gh-pages` branch's `/docs` folder
at
`https://dollspace-gay.github.io/Wildforge/atproto-oauth-client-metadata.json`;
the JSON identifies that same URL as its `client_id`.
Linux tests, strict lints, and the `x86_64-pc-windows-gnu` cross-check pass.
Default tests use local fixtures and mock OAuth/verifier inputs and never
mutate a public account. Interactive browser handoff plus
write/fetch/revoke/re-fetch against a common and independent PDS remains a
release smoke test because it requires a consenting account and provider.

Dependency cost is intentionally isolated behind `identity::atproto`; the
direct production graph adds 278 unique package lines over `origin/main` in
the recorded Cargo-tree measurement (530 versus 252), but no second game
runtime or central Wildforge service. The 2026-07-23 unstripped Linux release
binary is 35,589,240 bytes on Rust 1.96.0. Using the three required Jacquard
component crates instead of its umbrella crate removed 31 packages from the
first measurement. Join verification is bounded to eight seconds and
successful proofs are cached per server/world.

## Release qualification matrix

The default build/test gate is safe to run without public credentials:

| Check | Current evidence |
|---|---|
| Local identity, migration, signatures, profile mapping | Automated unit tests |
| Handshake transcript, host-key change, old protocol, abuse budget | Automated unit/loopback tests |
| Server-owned inventory/movement and full roster | Automated loopback tests |
| OAuth flow and failures | Mock PAR/PKCE/DPoP/nonce, state/issuer/replay/cancel, subject, scope, and redaction tests |
| Binding mismatch, PDS migration, cache expiry/limit, SSRF policy | Automated pure verifier tests |
| Linux build/lints | `cargo test --all-targets --no-fail-fast` (204 passed, 1 diagnostic ignored); `cargo clippy --all-targets --all-features -- -D warnings` |
| Windows dependency/build compatibility | `cargo check --target x86_64-pc-windows-gnu` |
| Dependency/release size | 530 vs 252 normal package lines; 35,589,240-byte unstripped Linux release binary |
| Headless hosting | Isolated release-binary smoke created host key/settings and exercised `help`, `players`, and `identity` |

The following checks require a consenting account, graphical browser, or
operator judgment and must be signed off before labeling the integration
production-ready:

| Manual check | Pass condition | Status |
|---|---|---|
| Publish OAuth metadata | Exact checked-in JSON is available over HTTPS at its `client_id` with JSON content type | Passed 2026-07-23 via repository-owned GitHub Pages |
| Linux browser link/revoke | Callback completes; exact device record writes, verifies, deletes, and fails live verification after deletion | Pending consenting account |
| Windows browser link/revoke | Same sequence, including browser handoff and callback firewall behavior | Partial: browser link/callback and write succeeded in user testing; exact public type/key/rkey read-back from the current Bluesky PDS passed 2026-07-23; revoke/firewall path not signed off |
| Independent PDS | Link/write/fetch/revoke succeeds outside the common Bluesky PDS | Pending provider/account |
| Outage and handle change | Cached badge is visible only within grace; DID maps to the same `PlayerId` after handle/PDS change | Pending live-provider exercise |
| Local UX | First run, offline solo, LAN local join, moderation, unlink, and lost-device wording are understandable end to end | Partial: isolated graphical first-run smoke proposed an editable name while leaving `profile_complete=no`; local unlink and LAN/moderation paths have automated coverage; final human wording/playtest sign-off remains |

Do not substitute personal tokens in automated tests or commit captured OAuth
state to satisfy these rows.
