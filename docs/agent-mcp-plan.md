# Agents at the stall — an MCP so agents can play too

> **Status: implemented (stages 1–5), 2026-07-25 — and playtested
> live**: two agents on a real dedicated server over QUIC; SAWYER
> walked 25 blocks of natural terrain to the nearest tree, felled 5
> logs, collected the wire-awarded drops, and cut planks through the
> click protocol; HEELER followed him over two march legs and ended
> 3.0 blocks off his heel; a windowed guest joined the same server
> and photographed them both. Where the build diverged from the spec:
> - **`give(player)` became chest delivery**: guests have no
>   drop-item message, so the fetch quest ends with `craft chest` +
>   `deposit` beside the requester — noted as the intended fast
>   follow (a C2S drop/give message).
> - **`craft` knows shapes, not the whole book** (planks, stick,
>   crafting_table, chest, via the transactional click protocol);
>   the agent voluntarily requires a crafting table in reach for 3x3
>   work because the HOST doesn't check one — an open hardening item
>   for all guests, recorded in the operations doc.
> - **Standing behaviors bank breadcrumbs from `S2C::Players`** as
>   designed; stuck recovery replans through A* straight to the
>   leader and rebuilds the trail.
> - `WILDFORGE_JOIN` (join a host headlessly) and remote-session
>   `WILDFORGE_POS` were added as dev hooks for multiplayer
>   screenshots and agent playtests.
> - Stage 6 (stall buying, mob leading, boat riding, offering runs)
>   remains open by design.

Drafted 2026-07-25 after design discussion with dollspace. Decisions
settled: **agents are guests, not gods** — an agent connects over the
same QUIC protocol as any player, gets a PlayerId and a roster row,
and is validated (reach, rate, mode) by the same host code paths, so
there is no cheating surface to audit because there is no new
surface at all; **perception is queries, not chunk dumps** — an LLM
drowns in raw voxels, so the tools answer questions; **actions are
macros, not ticks** — the LLM thinks in seconds while the sim ticks
at 30 Hz, so a competence layer (pathfinding, walking, collecting)
executes between thoughts; and **"follow me" is a standing
behavior** — set once, runs autonomously, interrupts the LLM only
when something needs deciding.

The design spine: the game already wants more players. The economy
rewards specialization (somebody runs the salt route, somebody feeds
the boiler), the stalls trade while owners sleep, and **ire is
shared** — a hired agent's clearcut is the camp's wrathful night.
An agent isn't a gimmick bolted onto Wildforge; it's another
neighbor the wild keeps score on.

## Architecture

Three layers, one new crate-level module (`agent/`), no protocol
bump for v1:

```text
MCP server (stdio)  ->  bot layer (competence)  ->  headless guest
   tools/events          A*, macros, follow         net::Client, chunks
```

- **Headless guest**: reuse `net::transport::Client`, the chunk
  decode path, and the guest-side world mirror the windowed client
  already maintains — minus renderer, audio, and winit. It presents
  a device identity like any client (its own key, its own editable
  name — "Sawyer", "Packmule"); hosts admit, allowlist, mute, or
  ban it with the machinery that already exists. Runs as
  `wildforge --agent <addr>` (the one-binary rule holds).
- **Bot layer**: the Mineflayer-for-Wildforge. Walks with the same
  AABB physics constants as players (never teleports — the design
  guard applies to agents doubly), A* over the local chunk mirror
  with jump/step/swim moves, stuck detection and re-path, drop
  collection by proximity, and standing behaviors (follow, wait,
  patrol-to) that keep executing between LLM turns.
- **MCP server**: a thin stdio adapter exposing the tools below.
  One MCP session = one guest connection. Multiple agents = multiple
  processes, each its own identity — the host sees a party, not a
  hive.

## Perception (queries, not dumps)

- `look_around()` — a compact digest: biome, weather, season,
  time-of-day, light, standing-on/in, a coarse 3-level top-down map
  of the surrounding ~32 blocks (surface material classes, not
  block-by-block), notable features (water bodies, cliffs, built
  structures), entities with distances, and the local ire tier as
  the wild presents it (night ambience tier — what a player senses,
  not the ledger).
- `nearest(kind, radius)` — "nearest `#base:logs`", "nearest water",
  "nearest stall", "nearest player". Answers position + distance +
  path reachability. This one tool is most of "get me some wood."
- `at(x, y, z)` / `in_reach()` — exact block queries where precision
  matters (is the trunk mined out, what's under my feet).
- `inventory()` / `status()` — slots, tools with durability, health,
  hunger, air; what the hotbar holds.
- `events()` — the drained buffer: chat lines (task requests arrive
  here), toasts and whispers the client was shown, arrival/stuck
  notices from standing behaviors, damage taken, death. The MCP
  surfaces these as notifications where the client supports it, and
  as a pollable tool where it doesn't.

Perception honesty rule: an agent knows what a player could know.
It reads the world mirror it was streamed — no server-side
omniscience endpoint, no querying chunks it hasn't received, no ire
ledger internals. If a player would have to walk there to see it,
so does the agent.

## Action macros

Locomotion:
- `go_to(x, z | landmark)` — A* with jump/step/swim; completes,
  fails with a reason, or reports partial progress. Vertical intent
  ("get to the surface", "down to y 40") rides the same planner.
- `follow(player, distance = 3)` — **the standing behavior.** The
  leader's position streams in `S2C::Players` already; the bot lays
  breadcrumbs from it and chases the trail — not the straight line —
  so it takes the bridge the player took, the pass the player
  climbed, as best it can. Keeps ~distance blocks back, jumps and
  swims, sprints to catch up when the gap grows, and on losing the
  trail (leader out of streamed range) walks to last-seen and
  reports via `events` instead of guessing. `stop()` ends any
  standing behavior. Follow survives across LLM turns indefinitely —
  "follow me" then silence is a valid session.
- `face(target)` — look at a player, block, or heading (politeness
  and screenshots).

Work:
- `break_block(x, y, z)` / `chop(tree_at)` — chop walks the trunk
  top-down like a player must, collects the drops, replants the
  sapling if told to (the good-neighbor flag; the wild is watching
  either way).
- `place_block(x, y, z, item)`, `use_block(x, y, z)` — the generic
  right-click: opens nothing screen-shaped (agents don't have
  screens), but rests work on stations, loads smokers and fireboxes,
  reads waystones and cairns — the hand-loaded machine surface is
  fully agent-usable *because* it was built without GUIs.
- `craft(recipe, count)` — grid crafting via the same request path;
  station work via put/take macros (`station_put`, `station_take`).
- `give(player, items)` / `drop(items)` — hand over the wood. The
  fetch quest ends with the goods in your pack, not a claim.
- `chat(text)` — how the agent answers "got 32 oak, heading back."

Every macro is interruptible by the host's own rules: reach checks,
rate limits, action cooldowns. A macro that would need to violate
one simply fails with the reason.

## The wild's stake

Nothing special-cased, and that's the feature: the agent's chopping
charges the regional ledger, its planting refunds, watchers stand at
its treeline, wardens hunt it at night, its deaths scatter its
inventory. The LLM is the ethics layer — the tools expose
stewardship verbs (plant, offer at the stone) with the same weight
as extraction verbs, and `look_around` reports the wild's mood, so
"get me some wood *without angering the forest*" is a real,
followable instruction. Shared ire makes agent labor a real
decision, not free energy — exactly the economy's give-and-take.

## What stays out (v1)

- No server-side agent privileges: no omniscient queries, no
  teleport, no creative shortcuts, no pathfinding done by the host.
- No per-tick LLM control (macros only; the bot layer owns ticks).
- No new protocol messages until a real wall demands one — v1
  speaks the player protocol exactly.
- No screen-based interactions (chests/stalls-as-buyer need the
  transactional click protocol; agents trade through hand-loaded
  surfaces and `give` first — stall buying is a fast follow).
- No autonomous goal invention: agents act on instructions from
  chat or their operator's MCP session; idle means idle (or follow).

## Touchpoints

- `agent/` module + `--agent` flag beside `--server` in the one
  binary; reuses `net::transport::Client`, protocol types, chunk
  decode, and the collision constants (extracted where currently
  entangled with `game/`).
- MCP over stdio (JSON-RPC): a small hand-rolled loop — no new
  heavyweight dependency for v1.
- `S2C::Players` already streams leader positions for follow;
  `C2S::Chat`/`S2C::Chat` already carry task requests and replies.
- Docs: `docs/agent-mcp-operations.md` for hosts (admitting agents,
  policies) once stage 5 lands.

## Tests

- A loopback agent connects, is admitted, appears on the roster,
  and its identity persists (name, PlayerId) across reconnect.
- Perception: `nearest` finds a placed log cluster and refuses one
  in an unstreamed chunk; `look_around` fits under a token budget.
- Locomotion: `go_to` crosses a built course (gap to jump, step to
  climb, water to swim) within N sim-seconds; stuck reports fire.
- **Follow**: a scripted leader walks a bent path with a bridge
  crossing; the follower stays within distance+slack the whole way
  and takes the bridge, not the water; losing the leader produces
  the last-seen report.
- Work: "get me some wood" end to end — chat request in, chop,
  collect, return to requester, `give`, confirmation chat out.
- The wild: an agent clearcut measurably raises the shared meter
  (assert the ire delta) — the design guard, as a test.

## Stages

1. **Headless guest**: connect, admit, mirror chunks and snapshots,
   chat in/out — a bot that stands there and answers.
2. **Perception**: the query tools over the mirror + events buffer.
3. **Locomotion**: A* + go_to + **follow** (the standing-behavior
   machinery lands here).
4. **Work macros**: break/chop/collect, place, craft, station
   put/take, give — "get me some wood" works at this stage.
5. **MCP skin + operations doc + demo**: stdio server, tool schemas,
   host-side admission notes, and the two demo transcripts (fetch
   quest; follow-me expedition).
6. **Fast follows**: stall buying (transactional clicks), multiple
   coordinated agents, boat riding, offering-stone stewardship runs.

Each stage lands independently; stages 1–4 are a useful scripted
bot even with no MCP client attached.
