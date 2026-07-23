# Multiplayer Identity, Accounts & Public Servers — Design and Implementation

Drafted and implemented 2026-07-22. This plan extends the multiplayer work in
`multiplayer-plan.md`. It replaces name-claim identity and per-name player saves
with a local-first identity model that can optionally link an AT Protocol
account. It does not require an online account for solo, LAN, or private play,
and it does not create a paid Wildforge account gate.

The central decision is:

> Everyone can create a local Wildforge profile. Servers choose whether they
> accept local identities or require a portable, verified ATProto identity.
> Display names are labels; they are never account keys.

Implementation status: the Pass 0 architecture decision and all locally
implementable Pass 1/2 work are in tree. A 2026-07-23 completion pass added the
hold-Tab roster, verification/handle privacy controls, host-enforced remote
moderator requests, last-successful-save metadata, and the remaining
identity/transport/host/roster module seams. The selected proof is an ATProto
repository device-binding record in the project-controlled
`gay.dollspace.wildforge.device` namespace, verified from the DID-authorized
PDS with an explicitly documented provider-trust boundary. The default suite
is offline and deterministic.

Live deployment and provider exercises are qualification work, not code that
can be truthfully completed by an offline test suite. Browser consent plus live
write/fetch/revoke checks on common and independent PDS providers remain
release gates because they require real consenting accounts. The production
metadata URL was also unreachable during the 2026-07-23 qualification run and
must be deployed before enabling a hosted-metadata release profile. Operational
details, recovery, commands, cache policy, and the interactive matrix are
recorded in `multiplayer-identity-operations.md`.

This gives public communities persistent moderation identities without making
players buy an account from Wildforge, while keeping the one-binary,
local-first game intact.

## Goals

- Solo play, LAN play, and private hosting work without internet access or an
  external account.
- The player chooses a local Wildforge display name in-game. The OS username is
  never silently exposed to a host.
- A local profile may optionally link an ATProto account through OAuth.
- Public servers may require linked ATProto identities as an admission policy.
- Handles and display names may change without changing saves, bans, roles, or
  ownership.
- Server-side player state survives reconnects and is not editable or portable
  between unrelated servers by the client.
- Bans, allowlists, and roles target authenticated principals rather than names
  or IP addresses.
- OAuth credentials never cross the Wildforge game protocol or reach a game
  server.
- The design works with ATProto generally, not only accounts hosted by
  `bsky.social`.
- The identity work creates small ownership boundaries instead of adding more
  responsibilities to `net.rs` and `mp.rs`.

## Non-goals

- Wildforge will not operate an email/password database or invent an account
  recovery system.
- ATProto is not proof of personhood. It raises the cost of casual ban evasion;
  it does not prevent a determined user from creating another identity.
- There is no global Wildforge ban list, reputation score, or automatic import
  of Bluesky blocks and moderation labels. Communities moderate themselves.
- A social account is not required merely to discover, host, or play the game.
- This plan does not design matchmaking, friend-code rendezvous, relays, or
  public server discovery.
- This plan does not make cosmetic profile data authoritative gameplay state.
- We will not implement OAuth, signatures, key generation, or Unicode security
  algorithms from scratch when maintained libraries exist.

## Baseline before this implementation

The original friend-server model had a display label and a connection id, but
no account boundary:

- `game::whoami()` reads `WILDFORGE_NAME`, then the OS `USER`/`USERNAME`, and
  truncates the result to 16 characters. There is no name field in `Config` or
  the join UI.
- `C2S::Hello` contains an arbitrary client-supplied `name: String`. The host
  gives each connection a transient `u32`; that counter resets with every host
  process.
- The host accepts duplicate, empty, control-filled, differently-cased, and
  oversized names from custom clients. Only the stock client performs its
  small truncation.
- A kick adds the exact display string to an in-memory set. Reconnecting with a
  different name or capitalization bypasses it, and restarting the host clears
  it.
- `S2C::Joined` announces only new arrivals. A late joiner does not receive a
  complete named roster, and the host has no advertised name, so missing names
  fall back to labels such as `P0` and `P2`.
- Guest position, health, hunger, inventory, and armor are loaded from and saved
  to the single client-side `saves/.remote/profile/player.toml`. That profile is
  shared across unrelated hosts and can be edited locally. The design notes
  describe host-side per-name saves, but they are not implemented.
- The QUIC transport is encrypted, but each host creates an ephemeral
  certificate and clients accept every certificate. There is no persistent
  host identity or trust-on-first-use pin.

These are reasonable shortcuts for trusted friends. They are not foundations
for persistent profiles, public moderation, or authenticated ownership.

## Identity vocabulary

The implementation must keep five concepts separate:

| Concept | Lifetime | Visibility | Purpose |
|---|---|---|---|
| `ConnectionId` | One connection | All peers | Compact routing and snapshots |
| `PlayerId` | One server/world profile | Server; opaque to peers | Owns saved progress |
| `DeviceKeyId` | One local installation/device key | Authenticating server | Local proof of key possession |
| `AtprotoDid` | Portable ATProto account | Authenticating server | Optional verified online principal |
| `DisplayName` | Mutable preference | All peers | Human-readable label only |

Suggested domain shapes, not final wire syntax:

```rust
struct ConnectionId(u32);
struct PlayerId([u8; 16]);
struct DeviceKeyId([u8; 32]);
struct AtprotoDid(String);
struct DisplayName(String);

enum Principal {
    LocalDevice(DeviceKeyId),
    Atproto(AtprotoDid),
}
```

`PlayerId` is server-assigned and remains the primary key for the player's
world profile. A profile has one or more principals that are allowed to open
it. This indirection matters:

- a local player can link ATProto later without moving or duplicating progress;
- multiple approved devices can reach the same ATProto-backed player profile;
- rotating or revoking a device key does not rename the save;
- unlinking an account does not destroy the player profile;
- if a local and ATProto profile both already exist, they can remain separate
  until an explicit, safe merge decision is made.

Peers receive `ConnectionId`, display metadata, appearance, and a coarse
verification badge. They do not need another player's `PlayerId`, device key,
OAuth state, full DID, ban history, or role grants.

## Local identity is the universal baseline

On first launch, Wildforge asks for a display name and generates a device
signing key. Local identity is not merely a name: possession of the private key
proves that a reconnecting client is the same local installation.

The private key belongs in a separate per-user identity store, not
`config.txt`, a world save, screenshots, logs, or cloud-synced content by
accident. The public-key fingerprint becomes `DeviceKeyId`. Secret writes must
be atomic and use restrictive file permissions where the platform supports
them.

The initial local handshake is challenge-response:

1. The client sends its protocol version, public device key, requested display
   name, appearance, and content hash.
2. The server returns a fresh random challenge, its own persistent identity
   fingerprint, and its identity/admission policies.
3. The client signs a domain-separated transcript containing the challenge,
   both endpoint identities, protocol version, and relevant hello fields.
4. The server verifies the signature, maps the local principal to a `PlayerId`,
   applies admission and ban policy, and returns `Welcome` plus a full roster.

Signatures prevent someone from copying another client's device fingerprint.
The challenge and transcript binding prevent replay on another server or later
session. Exact algorithms and encodings are selected from established,
reviewed libraries during implementation; the protocol must not contain a
home-grown signature scheme.

Local identity is appropriate for solo, LAN, and trusted private servers. A
local principal is cheap to replace, so a public server may refuse it.

## Display names and social profiles

Every player has a local Wildforge display name, whether or not they link an
online account. Linking ATProto may offer to import a profile display name and
avatar, but it does not silently overwrite the local name.

The settings should be independent:

```text
Wildforge name: MOSSKEEPER
ATProto account: @moss.example [linked]
Use social display name: off
Use social avatar: off
```

This avoids making participation on a verified public server automatically
advertise a player's social profile to every peer. The server operator must be
told that a verified DID can be resolved to public account metadata; the join
screen should disclose that fact before authentication.

The server validates display names regardless of their source:

- trim leading and trailing whitespace;
- enforce both a UTF-8 byte cap and a visible-character/grapheme cap;
- normalize the accepted Unicode representation;
- reject controls, newlines, bidi overrides, zero-width spoofing characters,
  and glyphs the UI cannot render safely;
- calculate a case-folded collision key;
- reject active-roster collisions with a clear rename response; the stock
  client may suggest a short suffix, but names are not permanently claimed;
- never use a display name in a path or as a database/save key.

The first implementation may deliberately use the smaller character set the
current font can render. Expanding it later is better than accepting text the
UI displays ambiguously.

For an ATProto-linked player, presentation fallback is:

1. explicit Wildforge display name;
2. opted-in ATProto profile display name;
3. current ATProto handle;
4. a short, non-secret fallback derived from the server-local player id.

Handles are mutable and profile records are optional. The stable ATProto
identifier is the DID, not the handle or `app.bsky.actor.profile` contents.

## ATProto account linking

ATProto OAuth is used to prove account control to the Wildforge client and to
authorize the smallest required ATProto operation. It is not a credential the
client forwards to a game host.

The current ATProto OAuth profile requires authorization code flow, PKCE,
pushed authorization requests (PAR), DPoP, issuer/identity validation, and
public client metadata. An authentication-only flow may request only the
`atproto` scope and receives the account DID in the token response's `sub`
field. A native Wildforge build still needs a hosted HTTPS client metadata
document and a browser redirect/callback strategy.

Official references:

- [ATProto OAuth specification](https://atproto.com/specs/oauth)
- [ATProto identity guide](https://atproto.com/guides/identity)
- [ATProto permission scopes](https://atproto.com/guides/permission-sets)
- [Bluesky identity and profile resolution](https://docs.bsky.app/docs/advanced-guides/resolving-identities)
- [ATProto SDK catalog](https://atproto.com/sdks)

The OAuth access and refresh tokens are opaque, scoped to ATProto resources,
and bound to the OAuth client's DPoP key. Therefore:

- never place them in `C2S::Hello`, a game-server ticket, logs, crash reports,
  save files, or moderation records;
- never ask a public game server to validate or proxy a player's OAuth token;
- discard the OAuth session after a one-time link when possible;
- if a refresh session is retained for profile features, store it through an
  appropriate secure credential facility and make revocation visible in UI.

### The proof bridge

OAuth proves the DID to the Wildforge client, but an arbitrary game server must
also be able to verify that claim. Two approaches deserve a spike.

#### Preferred: an ATProto repository device-binding record

After OAuth, the client writes a small record in a Wildforge-owned Lexicon
namespace to the player's ATProto repository. The exact NSID requires a domain
the project controls. Conceptually:

```text
did: did:plc:...
device_public_key: ...
created_at: ...
label: optional device label
```

One record per device allows a second computer to link without invalidating the
first. Deleting a record revokes that device after server caches expire.

On join:

1. the client claims the DID and device-key record identifier;
2. the server resolves the DID to its current PDS;
3. the server fetches and verifies the binding record;
4. the ordinary game challenge proves current possession of that device key;
5. the authenticated principal is `Principal::Atproto(did)`.

This keeps gameplay independent of a central Wildforge login service, supports
PDS migration, and never exposes OAuth tokens. The spike must determine the
right verification level: trusting a correctly DID-resolved PDS response may
fit friend servers, while public servers may require repository/CID signature
verification or a trusted indexed view.

The consent screen must accurately request the granular write permission for
the Wildforge binding collection. It should not request general Bluesky posting
or account-management access.

#### Fallback: a Wildforge-signed account ticket

A small Wildforge service completes OAuth and issues a short-lived signed
ticket binding a DID to a game device key and a narrow audience. A game server
verifies the ticket using the published Wildforge issuer key, then challenges
the device key.

This is simpler for arbitrary servers to verify, but it introduces a central
service into new logins, creates an operational security boundary, and weakens
the self-hosting story. It remains a fallback if repository-record verification
is too brittle or poorly supported in Rust.

The implementation must not ship a third option where the official client
asserts an unverified DID and the host trusts it. Public moderation only works
when custom clients cannot impersonate that assertion.

## Server identity and transport trust

Player authentication does not authenticate the host. The current ephemeral
self-signed QUIC certificate plus `TrustAny` verifier protects confidentiality
against passive observation but does not tell the client which server answered.

Each hosting installation should generate a persistent server identity key.
Clients pin its fingerprint on first successful connection and warn before
accepting a changed key. Public servers with a domain may additionally publish
or certify that key, but LAN hosting must not require public PKI.

The server fingerprint is included in player challenge transcripts and ATProto
device-link challenges where applicable. This stops an authentication response
prepared for one host from being replayed to another.

Server identity storage is installation-level rather than player-profile data.
Copying a world must not silently publish the original host's private key.

## Server-selectable identity and admission policy

Identity strength and admission are separate settings:

```toml
identity = "local"            # local identities accepted
# identity = "atproto_optional"
# identity = "atproto_required"

admission = "open"            # any valid identity may request entry
# admission = "allowlist"
```

Expected combinations:

| Server | Identity | Admission | Intended use |
|---|---|---|---|
| Solo/LAN | `local` | `open` | No internet or account dependency |
| Friends | `atproto_optional` | `allowlist` | Familiar players, mixed account choices |
| Public | `atproto_required` | `open` | Free portable login with durable bans |
| Curated community | `atproto_required` | `allowlist` | Applications, events, private communities |

The host advertises requirements before the player starts OAuth or content
sync. The join screen says plainly, for example, `VERIFIED ATPROTO ACCOUNT
REQUIRED`. Refusal messages distinguish missing verification, a ban, an
allowlist miss, duplicate login, invalid name, incompatible protocol, and
content errors.

An operator may change policy without rewriting player profiles. Existing
local-only profiles remain on disk; they simply cannot authenticate while
`atproto_required` is active unless an ATProto principal is linked.

## Player profiles and server authority

World progress belongs to a server-owned `PlayerId`, not the display name,
device, or client filesystem. A possible layout is:

```text
saves/<world>/
  players/
    index.toml                 # principal -> PlayerId, schema version
    <player-id>.toml           # survival state and profile metadata
  moderation/
    bans.toml
    roles.toml
    audit.log
```

The player profile contains at least:

- version and `PlayerId`;
- linked principals and their status;
- current/previous display metadata for operator recognition;
- position, yaw/pitch, spawn point;
- health, hunger, nutrition, inventory, armor, cursor transaction state;
- appearance and gameplay-relevant per-player state;
- first seen, last seen, and last successful save times.

All profile mutations are host-authoritative and written atomically. A guest
does not load position or inventory from `saves/.remote/profile`, and a custom
client cannot select a different profile by sending a filename or display
name. Client-side remote storage contains only caches and preferences that have
no gameplay authority.

This identity project therefore depends on completing server authority for
guest inventory, health, hunger, armor, movement validation, combat damage,
and container cursor state. Authentication would be hollow if an authenticated
client could still mint items or submit arbitrary damage.

### Linking and merging existing profiles

When a connected local player proves an ATProto DID, the server may attach that
principal to the same `PlayerId` if no other profile already owns the DID. This
is a credential addition, not an inventory copy.

If both principals already map to different player profiles, Wildforge does not
automatically merge them. Automatic merging creates duplication and ownership
ambiguity. A future operator-mediated merge can choose one inventory/profile,
archive the other, and record the action in the audit log.

Only one active connection per authenticated principal is admitted by default.
An operator may choose whether a new connection replaces the old one or is
refused.

## Moderation and permissions

Moderation checks both the server-owned `PlayerId` and its authenticated
principals, with the DID preferred for verified public players. Banning a
connected player blocks that `PlayerId` and every principal currently attached
to it; a principal-level index also prevents the banned DID/device from being
attached to a freshly created profile. Last handle, last display name, and IP
may be recorded as context, but mutable context does not define the ban.

A ban record should contain:

```toml
principal = "did:plc:..."
player_id = "..."
reason = "chat spam"
created_at = "..."
created_by = "..."
expires_at = "..."       # optional; absent means permanent
last_handle = "..."      # informational cache
last_display_name = "..." # informational cache
```

Initial roles:

- `owner`: server configuration, roles, bans, world administration;
- `admin`: bans, allowlist, kicks, mode/administrative commands;
- `moderator`: kicks, temporary mutes/bans, chat moderation;
- `player`: ordinary gameplay;
- `spectator` can wait until gameplay actually supports it.

Roles and allowlists use the same principal representation as bans. The
windowed host and dedicated-server console need commands/UI to inspect a
player's display name, current handle, short DID/device fingerprint, role, and
`PlayerId` without dumping secrets.

Every durable moderation mutation records who, what, when, and why. There is no
automatic global propagation. Exporting/importing a community ban list, if ever
added, must be an explicit operator action with visible provenance.

## Spam and ban-evasion limits

ATProto verification helps because a display-name change does not discard the
DID, and ordinary players do not need to purchase a Wildforge-specific
identity. It does not make account creation expensive enough to stand alone as
anti-abuse.

Public-server hardening still requires:

- connection-attempt and handshake rate limits;
- per-principal chat and command rate limits enforced by the server;
- sensible message byte caps before allocation and decoding;
- optional temporary IP/subnet throttles as soft signals, never the only
  durable identity;
- a server-local `first_seen_at` probation policy if a community wants one;
- mute and temporary-ban tools in addition to permanent bans;
- limits on concurrent connections from one principal and obvious bursts from
  one source.

ATProto account age is not treated as universal proof of trust. Different DID
methods and providers have different metadata, and a long-lived compromised
account is not harmless. Reputation remains local to the community unless a
future design explicitly addresses consent, appeals, and abuse of shared lists.

## Privacy and profile safety

- Linking is explicit and reversible; local play never starts an OAuth flow.
- Before joining an ATProto-required server, UI explains that the operator can
  resolve the DID to public profile information.
- Other peers receive a verification badge, never the DID. A public handle is
  included only when its owner enables the independent sharing preference.
- Social display name and avatar import are separately opt-in.
- Profile responses and identity documents are cached with bounded lifetimes;
  handle changes never change the account key.
- Avatar support may be deferred. If enabled, downloads have strict URL,
  content-type, byte, dimension, decode, and cache limits and never occur in the
  render loop.
- OAuth tokens and private device/server keys are redacted from diagnostics.
- An ATProto/PDS outage may use a recently verified cached binding for a short,
  operator-configurable grace period. The UI identifies cached verification.
  After the grace period, a server requiring ATProto refuses new verification
  rather than silently downgrading to an unverified DID claim.

## Wire direction

The protocol will need a version bump and a multi-step handshake. A shape such
as the following keeps authentication separate from ordinary gameplay:

```rust
enum C2S {
    Hello {
        protocol: u32,
        display_name: String,
        device_public_key: Vec<u8>,
        content_hash: u64,
        style: u32,
        client_nonce: [u8; 32],
    },
    Authenticate {
        signature: Vec<u8>,
        atproto: Option<AtprotoClaim> // DID, binding rkey, share-handle choice
    },
    Moderate { target: u32, action: ModerationAction },
    // Existing authenticated gameplay requests follow.
}

enum S2C {
    Challenge {
        nonce: [u8; 32],
        server_public_key: Vec<u8>,
        identity_policy: IdentityPolicy,
        admission_policy: AdmissionPolicy,
    },
    Welcome {
        connection_id: u32,
        your_role: Role,
        roster: Vec<PlayerPresence>,
        // Existing world/bootstrap fields follow.
    },
    RoleChanged { role: Role },
    Refused(Refusal),
    // Existing state/events follow.
}
```

This is intentionally not a final encoding. The implementation must establish:

- a strict pre-auth message and byte budget;
- an authentication timeout;
- fresh, cryptographically random nonces;
- domain separation and canonical transcript encoding;
- replay rejection;
- no gameplay requests accepted before authentication;
- structured refusal codes with safe human-readable details;
- a complete roster in `Welcome`, including the host when windowed;
- public presence updates containing display data rather than principals;
- removal of sender-controlled names from chat payloads—the server always
  derives `from` from the authenticated connection.

## Module boundaries

This work should be an early consumer of the modularization plan, not a reason
to regrow god files. A target layout is:

```text
src/identity/
  mod.rs          PlayerId, Principal, DisplayName, validation
  local.rs        device-key generation, storage, challenge signing
  atproto.rs      OAuth session, DID/profile resolution, binding client

src/net/
  mod.rs          transport-facing API
  protocol.rs     postcard DTOs and protocol version
  handshake.rs    pre-auth state machine and transcript encoding
  transport.rs    QUIC endpoint, streams, datagrams, framing

src/multiplayer/
  mod.rs          session-facing API
  host.rs         authenticated connection/session orchestration
  roster.rs       public presence and connection mapping
  profiles.rs     PlayerId persistence and principal index
  moderation.rs   admission, bans, roles, audit
```

The exact move sequence follows `modularization-plan.md`; identity work should
not combine a mechanical file move with protocol redesign in the same review.

The landed tree keeps the historical `identity.rs`, `net.rs`, and `mp.rs`
module names as small compatibility facades while placing implementations in
`identity/local.rs`, `identity/atproto.rs`, `net/protocol.rs`,
`net/handshake.rs`, `net/transport.rs`, `multiplayer/host.rs`,
`multiplayer/roster.rs`, `multiplayer/profiles.rs`,
`multiplayer/moderation.rs`, and `multiplayer/settings.rs`.

Dependency direction:

```text
UI / dedicated console
          |
          v
multiplayer policy + profiles <--- identity domain
          |                              |
          v                              v
authoritative server                handshake DTOs
          ^                              |
          +----------- net transport ----+
```

- `identity` knows validation, keys, and external identity resolution but not
  gameplay, renderer, or winit screens.
- `net` transports typed handshake/game messages but does not decide bans or
  profile ownership.
- `multiplayer` maps authenticated principals to player profiles and applies
  server policy.
- `server` receives an authenticated player context and owns gameplay state.
- UI presents login, policy, and moderation results without holding authority.

## Configuration and UI

Client configuration gains a display-name preference and non-secret link
metadata. Secrets stay in the identity credential store.

Required UI states:

1. **First run / profile** — choose a local Wildforge name; generate local
   identity without mentioning OAuth as a requirement.
2. **Accounts** — show local device identity, `LINK ATPROTO`, linked handle/DID
   summary, social-profile toggles, refresh/relink, and unlink.
3. **Join browser** — display `LOCAL IDENTITIES ACCEPTED` or `VERIFIED ATPROTO
   REQUIRED` before connecting.
4. **OAuth progress** — opening browser, waiting for callback, approved,
   refused, provider error, binding record written.
5. **Roster** — stable names for all current players, verification badge,
   optional handle according to privacy choice.
6. **Moderation** — kick, mute, temporary/permanent ban, allowlist, role, and
   short identity details with confirmation.

The dedicated server needs equivalent configuration and non-interactive
moderation commands. It must never require launching a browser merely to host.

## Migration

Migration is deliberately conservative:

1. Add `display_name` to config. On first migration, propose the old `whoami()`
   value in an editable UI; do not silently save or transmit it.
2. Generate the local device key and create the identity store.
3. When opening an existing local world, assign its `player.toml` a new
   `PlayerId`, move it to `players/<id>.toml`, and index the current local
   principal. Keep a backup marker until the new profile saves successfully.
4. Do not automatically upload or trust `saves/.remote/profile/player.toml`.
   It may contain state from several unrelated hosts. Private-host migration
   can be a separate explicit import/grant tool if preserving test worlds is
   important.
5. Existing name bans cannot be safely converted to authenticated bans. Import
   them only as temporary display-name filters with an operator warning.
6. Bump the wire protocol. Old clients receive a structured incompatible-client
   refusal; do not support an unauthenticated legacy path on a public server.

## Implementation passes

### Pass 0 — ATProto and platform spike

Time-box this before committing the public identity architecture:

- complete native OAuth on Linux and Windows with browser callback;
- validate issuer, DID `sub`, PDS resolution, PKCE, PAR, and DPoP through a
  maintained Rust library or a very small audited integration surface;
- host production-shaped HTTPS client metadata;
- request only identity plus the proposed custom-record permission;
- write, fetch, revoke, and re-fetch a device-binding record on at least the
  common Bluesky PDS and one independent/test PDS;
- determine whether public servers verify direct PDS responses, repository
  signatures/CIDs, or an indexed view;
- validate cross-compilation and dependency size;
- measure join latency, cache behavior, PDS outage behavior, and handle changes;
- choose repository binding or the signed-ticket fallback in a recorded
  decision.

The spike does not land OAuth secrets, production client identifiers, or a
half-trusted DID claim in the shipping protocol.

### Pass 1 — local identity and server-owned profiles

- introduce the identity domain types and display-name validation;
- add editable local names to config/UI and retire OS-name transmission;
- generate and store device keys;
- persist and pin host identity;
- implement challenge-response and pre-auth limits;
- introduce server-owned `PlayerId` profiles and principal index;
- move guest survival/inventory authority to the host;
- send a complete roster and fix host/late-join names;
- implement generic bans, allowlists, roles, and structured refusal codes;
- migrate local `player.toml` safely;
- split protocol/handshake/profile responsibilities along the module plan.

At the end of Pass 1, private multiplayer is meaningfully safer and persistent
without ATProto.

### Pass 2 — optional ATProto and public-server policy

- add Accounts UI and OAuth callback flow;
- implement the chosen DID-to-device-key proof bridge;
- resolve and cache handles/profile metadata;
- support multiple linked device keys and revocation;
- add `atproto_optional` and `atproto_required` policies;
- map verified DIDs to existing/new `PlayerId` profiles without duplication;
- expose safe moderator identity summaries;
- add privacy controls and explicit public-server disclosure;
- harden join/chat rates and cached-verification behavior;
- document public hosting and recovery procedures.

### Implementation audit

The landed responsibilities map to the passes as follows:

| Pass concern | Implemented boundary/evidence |
|---|---|
| Local identity, names, and migration | `identity.rs`, `identity/local.rs`, Accounts first-run flow, and identity migration tests |
| Persistent host trust and authenticated handshake | `net/handshake.rs`, pre-auth budgets in `net/protocol.rs`, host-key/transcript/rate-limit tests |
| Wire DTO ownership | `net/protocol.rs`; `net.rs` is the facade and `net/transport.rs` owns QUIC, framing, and discovery |
| Server-owned profile and survival state | `multiplayer/profiles.rs`, authoritative intent handling in `multiplayer/host.rs`, reconnect/loopback tests |
| Moderation and server policy | `multiplayer/moderation.rs`, `multiplayer/settings.rs`, host-enforced remote role requests, windowed controls, dedicated console, persistence/integration tests |
| Optional ATProto link and revocation | `identity/atproto.rs`, Jacquard OAuth, pre-authorization DID pinning, immediate public write read-back, confirmed-delete revocation, checked-in Lexicon/metadata payloads, and Accounts progress/revoke/unlink UI |
| Public proof and cache | DID/PDS resolver, exact device-record verifier, bidirectional current-handle resolution, bounded per-world cache with zero-grace support, migration/revocation/outage tests |
| Privacy | DID omitted from `Hello` and all local-policy joins; explicit pre-join disclosure confirmation; peers receive a badge plus an optional, separately enabled handle |
| Roster and profile metadata | Hold Tab shows every current player and visible verification state; profiles atomically record `last_saved_at` |
| Platform/build gates | 203 passing all-target tests (one diagnostic ignored), strict Clippy, native release build, dependency-graph measurement, and `x86_64-pc-windows-gnu` cross-check |

The implementation deliberately stops short of avatar download/rendering. The
preference is stored separately so adding the bounded image pipeline later does
not change identity or admission semantics.

## Test plan

### Unit tests

- display-name normalization, byte/grapheme caps, controls, bidi/zero-width
  rejection, case-folded collisions, and renderer-supported glyphs;
- typed identifier parsing and stable serialization;
- principal-to-`PlayerId` index invariants;
- ban expiry, role hierarchy, allowlist decisions, and audit entries;
- transcript canonicalization and signature verification;
- profile atomic-write recovery and schema migration.

### Loopback/integration tests

- first local join creates one profile; reconnect opens the same profile;
- copied public fingerprint without its private key fails;
- replayed authentication response fails;
- server-key change produces a client warning;
- duplicate display names receive a deterministic refusal/rename response;
- late join receives names for host and every existing guest;
- kicked/banned principals cannot bypass the ban by renaming;
- two devices bound to one DID open the same profile but cannot connect
  simultaneously by default;
- local-to-ATProto linking attaches a credential without duplicating inventory;
- conflicting existing profiles do not auto-merge;
- remote client files cannot change server-owned inventory or position;
- no authenticated gameplay message is accepted before `Welcome`.

### ATProto conformance tests

- mock OAuth/PDS services exercise issuer mismatch, wrong DID `sub`, DPoP nonce,
  PKCE/state replay, denied scope, callback cancellation, and token redaction;
- binding record for the wrong device key fails;
- binding deletion/revocation fails after cache expiry;
- handle and profile changes preserve DID/player ownership;
- PDS migration preserves identity after re-resolution;
- cached proof grace is visible and expires without silently downgrading;
- no live public service is required for the default test suite.

### Manual verification

- Linux and Windows browser handoff/callback;
- first-run local-only path with networking disabled;
- LAN host accepting local players;
- public-policy host refusing local-only identity before content sync;
- moderator workflow for handle change, kick, mute, timed ban, permanent ban,
  unban, and role assignment;
- OAuth/PDS outage messaging and recovery;
- account unlink, device loss, and second-device link UX.

## Acceptance criteria

- A fresh player can choose a local name and play solo/LAN without internet or
  account prompts.
- No multiplayer identity defaults to or silently transmits the OS username.
- Reconnect, bans, roles, and saves use authenticated identity, never a display
  string.
- A public server can require an ATProto DID without requiring a paid Wildforge
  account or receiving an OAuth token.
- Handle/display/profile changes do not create a new player or evade a ban.
- Local-to-ATProto linking preserves one authoritative profile and inventory.
- Guest survival state is server-owned and cannot be imported across hosts from
  the old remote profile.
- Clients authenticate persistent hosts and visibly handle key changes.
- The full roster, including the host, has stable display names for late joins.
- Public-server spam controls remain effective even though ATProto is not
  treated as proof of personhood.
- Identity, transport, profile storage, and moderation have distinct modules
  and tests.

## Decisions fixed by this plan

- Local profiles are permanent first-class identities, not temporary guest
  accounts awaiting online conversion.
- ATProto linking is optional globally and may be required by individual
  servers.
- The canonical ATProto account key is the DID. Handles and profile names are
  mutable metadata.
- Server progress belongs to a server-issued `PlayerId` with authenticated
  principals attached.
- Public verification never forwards OAuth tokens to game hosts.
- Public servers may use verified identity for admission and durable
  moderation, but Wildforge does not claim it eliminates alternate accounts.
- Wildforge does not sell the right to connect to community servers.
- Moderation is server-local by default.
- Social name/avatar import is opt-in and separate from verification.

## Decisions resolved by the spike

1. Use the custom repository binding and trust the current DID-authorized PDS
   response. Do not claim CAR/CID signature verification; operators needing
   that stronger boundary should wait for a signed-repository verifier.
2. Use `gay.dollspace.wildforge.device`, owned under `dollspace.gay`, with the
   Lexicon and deployable OAuth metadata payload checked in.
3. Use Jacquard 0.12.1 for OAuth, PAR, PKCE, DPoP, issuer/session validation,
   and record writes; use reqwest/rustls for the bounded public verifier.
4. Default proof-cache grace to one hour, allow zero to disable it, cap the
   operator setting at seven days, and visibly mark cached verification.
5. Stop the first integration at DID, handle, social display name, and
   verification status. Store the independent avatar opt-in but defer the
   bounded download/decode/render pipeline.

## Qualification record

The code-side qualification was repeated on 2026-07-23 with Rust/Cargo 1.96.0:

| Gate | Result |
|---|---|
| `cargo test --all-targets --no-fail-fast` | 203 passed, 0 failed, 1 intentionally ignored diagnostic |
| `cargo clippy --all-targets --all-features -- -D warnings` | Passed |
| `cargo fmt --all -- --check` and `git diff --check` | Passed |
| `cargo check --target x86_64-pc-windows-gnu` | Passed |
| `cargo build --release` | Passed; unstripped Linux binary 35,589,240 bytes |
| Normal dependency graph | 530 unique package lines versus 252 on `origin/main` (278 added) |
| Dedicated-host smoke | Fresh isolated world started, wrote a persistent host key/settings, and answered `help`, `players`, and `identity` console commands |
| Production OAuth metadata | `https://dollspace.gay/wildforge/oauth-client-metadata.json` did not accept an HTTPS connection; deployment remains required |

The dependency measurement prompted replacing the Jacquard umbrella crate with
only `jacquard-common`, `jacquard-identity`, and `jacquard-oauth`. OAuth state
and credentials now remain in an in-memory store for the one-time operation;
the application writes no temporary OAuth token file. The component split
removed 31 packages and about 1.7 MiB from the measured unstripped release
binary compared with the first implementation in this branch. The remaining
increase is substantial but accounted for: it is the maintained OAuth, DPoP,
identity-resolution, DNS, browser-loopback, and cryptography surface selected
instead of implementing those security protocols locally.

The default suite includes a Wildforge-owned mock OAuth server/client boundary.
It proves PAR, PKCE, DPoP and nonce retry, exact scope, callback state and
issuer checks, replay rejection, cancellation handling, malformed and
wrong-identity `sub` rejection, denied scope, and token redaction. Live browser
and repository tests remain the explicitly manual release gates listed in
`multiplayer-identity-operations.md`.
