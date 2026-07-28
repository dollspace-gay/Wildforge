# What breaks when the world gets bigger

Drafted 2026-07-28 from a full-tree audit. **Status: implemented
2026-07-28 — all six stages plus the deferred items. See the
implementation record at the end, including the two places the audit
overstated its case.**

Wildforge is in good structural health. 63.5k lines, 318 tests,
`cargo check --all-targets` clean with zero warnings, and effectively no
`TODO`/`FIXME`/`unimplemented!` debt anywhere in the tree. The
modularization plan's two passes both genuinely landed: `Game`
coordinates seven cohesive sub-state owners instead of forty loose
fields, and `World`'s collections are private behind operations.

So this is not a cleanup document. Everything below is a **scaling**
problem — code that is correct for one player standing near spawn and
wrong for two players, a long session, or a big view distance. They
sort into four groups, and one of them is much more urgent than the
others.

The short version: **the guest is a second-class citizen.** Five
separate defects all live in the multiplayer path, and they share one
cause — only 7 of the tree's 318 tests cover multiplayer, against
~5,300 lines of `host.rs` / `transport.rs` / `protocol.rs` /
`profiles.rs` / `moderation.rs`. Worldgen has 45 tests. `world` has 68.
Hearts has 36. The blind spot is exactly shaped like the bug list.

---

## Part I — The guest is a second-class citizen

Five findings, ranked by how badly they bite. All five are live on
`main` today.

### 1. State datagrams silently stop working past ~40 mobs

`multiplayer/host.rs:475-524` builds one snapshot per 20 Hz tick
containing **every mob in the world** and hands it to
`broadcast_datagram`. There is no spatial culling and no size check.

`world/mod.rs:573` sets `MOB_CAP = 320`. A postcard-encoded `MobSnap`
is 27–32 bytes (id varint, species varint, `Vec3` as three fixed f32s,
then yaw/growth/hurt as f32 and `fed` as a byte). At the cap that is
roughly **9 KB in a single datagram**.

QUIC datagrams cannot be fragmented. Quinn returns
`SendDatagramError::TooLarge` when the payload exceeds the path's
`max_datagram_size()` — call it 1200 bytes before MTU discovery, ~1450
after. And `net/transport.rs:198-201` throws that result away:

```rust
pub fn broadcast_datagram(&self, msg: &S2C) {
    let bytes = encode(msg);
    for p in self.peers.values() {
        let _ = p.conn.send_datagram(bytes.clone().into());
    }
}
```

So somewhere around **40–50 mobs**, every guest's mob replication stops
dead. No error, no log, no degradation — the wildlife simply freezes
where it stood and the host never learns. `Players`, `Bolts` and
`Falling` have exactly the same shape and the same fate; they are
smaller, so they die later.

The ecology arc ships a 32-species roster with herds, breeding and
repopulation. 40 mobs is not a stress test, it is a Tuesday.

**Repair.** Three things, and all three are wanted:

- **Cull to the guest.** A guest has no use for a mob it cannot see.
  Build the snapshot per-guest from mobs within its own streaming
  radius. This is the fix that matters — it makes the payload scale
  with what a player can perceive rather than with the size of the
  world, and it is the same change finding 3 needs.
- **Split by capacity.** Even culled, a busy clearing can overflow.
  Chunk the list against `conn.max_datagram_size()` and send several
  datagrams. Latest-wins semantics survive splitting as long as each
  part carries the same sequence stamp.
- **Stop discarding the error.** `let _ =` on a send is how this
  stayed invisible for its whole life. Count failures, log the first
  one per connection, and let a test assert the counter stays zero.

### 2. Guests get permanent holes in the world after roaming

The host streams chunks in `host.rs:439-471`:

```rust
'scan: for r in 0..5i32 {
    ...
    if !self.guests[&id].sent_chunks.contains(&cp) {
```

`sent_chunks` is a `HashSet` that is **only ever inserted into**
(`host.rs:74`, `:470`, `:866` — no removal anywhere). Meanwhile the
guest, running the same client code as everyone else, unloads chunks
outside `view_dist + 2` in `game/streaming.rs:174-195`.

And there is **no `C2S::RequestChunk`** in the protocol
(`net/protocol.rs`). Look through the whole `C2S` enum: a guest can
break, place, scoop, attack, feed, saddle, ride, buy, sign, brush,
strike an anvil, click a container — but it cannot ask for terrain.

The sequence is ordinary play. Guest stands at A; host sends the
chunks around A and marks them sent. Guest walks twenty chunks to B;
the client evicts the chunks around A to stay inside its memory
budget. Guest walks back to A. The host checks `sent_chunks`, finds
everything already marked, and sends nothing. **The guest is now
standing in a hole that will not close until they reconnect.**

**Repair.** The push-only model is the actual mistake — the host is
guessing at client state it does not own. Two halves:

- Add `C2S::RequestChunk { x, z }`, rate-limited like every other guest
  message. The guest asks for what it is missing; the host answers or
  refuses. This makes the client's eviction policy its own business,
  which is where it belongs.
- Make `sent_chunks` an eviction-aware set rather than a growing
  ledger: drop entries outside the guest's radius so the host's memory
  of what it sent tracks the guest's memory of what it has.

### 3. Guests see 5 chunks while the host sees up to 64

Same loop, same hardcoded `for r in 0..5i32`. Radius five is eighty
blocks.

`config.rs:21` sets `MAX_VIEW_DIST = 64`, and the "see further" work
raised the local view, killed the fog wall, and rebuilt the streaming
budgets to fill it (`streaming.rs:114-127` scales in-flight counts and
per-frame milliseconds off `view_dist`). None of that reached the host.
A guest can drag the view-distance slider from 4 to 64 and **nothing
changes** — the host will send eighty blocks of world either way.

There is a windowed host looking at a country's edifice from the next
valley while their guest stares into grey.

**Repair.** The guest's chosen view distance belongs in the handshake,
clamped by a host-side policy maximum so one client cannot ask a
dedicated server to page in 16,000 chunks. Then the streaming scan uses
that radius instead of the literal `5`. This lands naturally with
finding 2's request message — with `RequestChunk` in hand the host can
even go demand-driven and stop scanning entirely.

### 4. The dedicated server never unloads chunks

`dedicated.rs` runs the whole headless loop, and its only world
lifecycle call is `save_modified()` every 300 seconds. `HostSession::pump`
calls `server.world.ensure_chunk(cp)` while streaming to guests
(`host.rs:465`). Nothing anywhere calls `unload_chunk`.

Chunk eviction lives in `game/streaming.rs` — which is **client** code.
The windowed host gets it for free because it is also a player. The
dedicated server, which is the deployment that actually needs it, never
runs that function at all.

At 448 KB resident per chunk (see finding 6), one guest exploring a
thousand chunks costs the server ~450 MB that is never given back. Two
guests exploring in different directions cost double. The server does
not leak slowly; it leaks at walking speed.

**Repair.** Chunk residency is a *world* concern, not a client one.
Move eviction behind a `World::retain_chunks(centers: &[ChunkPos],
radius)` operation that both the client streaming path and the
dedicated loop call, keeping the existing save-on-unload behavior
(`save_chunk_if_modified` then `unload_chunk` — the incremental save
that replaced the autosave timer, and worth preserving exactly). The
dedicated server passes its guest positions as the centers. The eviction
cadence is **wall-clock**, not calendar-scaled — it is a memory policy,
not a world event.

### 5. Mob AI and spawning follow one player only

`world/ecology.rs:259`, first line of `tick_mobs`:

```rust
let player = players.first().map(|p| p.pos).unwrap_or(glam::Vec3::ZERO);
```

That singular `player` then drives repopulation (`:522-531`), the
watcher check (`:737`), and hostile spawning, whose signature is
literally `player: glam::Vec3` (`:706`). The `players` slice — the one
`PlayerCtx` list the simulation is handed, containing everyone — is
used for its first element and otherwise ignored by the whole ecology
layer.

`HostSession::player_ctxs` (`host.rs:260-278`) builds that list by
iterating `self.guests`, a `HashMap`. So on a dedicated server the
"first player" is whichever guest hashing happened to put first, and
**only that player** gets wildlife repopulation, hostile pressure, and
warden attention. Everyone else plays a beautiful, empty, completely
safe world.

This one is a design gap rather than a slip: the ecology arc was built
and tuned in singleplayer, where `players.first()` is exactly right.

**Repair.** Make the ecology layer plural. Repopulation and hostile
spawning both want "pick a player, then a ring around them" — so pick
*a* player per spawn attempt rather than *the* player, and budget the
attempts globally so N players do not multiply the world's mob count by
N. `MOB_CAP` stays a world cap; what changes is who the dice are rolled
around. The watcher/aggro checks want the nearest player, not the first.

**Related fragility, same file.** `dedicated.rs:52-59` takes the
`who` index out of `SimEvent::PlayerHit` — an index into the
`player_ctxs` vector — and resolves it back to a guest id through a
**second, independent** `sess.guests.keys()` iteration. This is correct
today only because the map is not mutated between the two calls, which
is a property nobody wrote down and no test protects. `PlayerCtx`
already carries a stable `id` field. The event should carry that id
instead of a positional index, and the whole class of bug goes away.

---

## Part II — Memory

### 6. Chunks are dense and uncompressed — 448 KB each

`chunk.rs:10-25`:

| field | bytes |
|---|---:|
| `blocks: Vec<u16>` | 131,072 |
| `meta: Vec<u8>` | 65,536 |
| `light_block: Vec<[u8; 3]>` | 196,608 |
| `light_sky: Vec<u8>` | 65,536 |
| **total** | **458,752 (448 KiB)** |

No sectioning, no palette compression, no empty-section skip. A chunk
of pure air above a desert costs exactly as much as a chunk full of
cave systems. The light planes — which are *derived* and never saved —
are 256 KB of that, more than half.

The comment at `config.rs:62-66` says view distance 12 is "about 120 MB
of loaded chunks." That number counted `blocks` + `meta` and forgot the
light planes. The real figure is 625 chunks × 448 KiB ≈ **273 MiB**.

At the slider's maximum the arithmetic gets serious: 129² = 16,641
chunks × 448 KiB ≈ **7.1 GiB**. The settings screen currently offers a
value the engine cannot survive on most machines, and the tooltip
undercounts the cost by 2.3×.

**Repair.** Two independent moves, either helps, both are better:

- **Section the storage.** 16 sections of 16³ per chunk, each either
  `Uniform(BlockId)` or a dense array. Most chunks are mostly air or
  mostly stone; this is where the order-of-magnitude lives. The light
  planes section the same way and compress even harder, since a
  section is usually uniformly lit or uniformly dark.
- **Bound the slider honestly.** Until storage improves, clamp
  `MAX_VIEW_DIST` against available system memory at startup and fix
  the comment. A setting that OOMs is worse than a setting that is not
  offered.

Ordering note: sectioned chunks change the in-memory representation,
not the save format — `chunk_rle` already serializes through an RLE
codec, so disk compatibility is not the obstacle. And per the standing
rule, saves are disposable: no migration path is owed.

### 7. Per-world maps grow monotonically

`World` carries eight collections keyed by chunk, 256-block cell, or
province, none of which are ever pruned: `last_random`, `mob_seeded`,
`player_touched`, `regional_ire`, `bloom`, `bloom_spent`,
`blessed_streak`, `hearts`. `save_stamps` (`persistence.rs:64`)
rewrites the entire `stamps` sidecar on every save.

These are small per entry and this is genuinely fine at today's scale.
It is on the list because it is unbounded *in explored area* and
therefore shares a failure mode with everything else here: it is
invisible until a world has been played for a long time by several
people. Worth a bounded-growth test and a decay policy for the ones
that are semantically decaying anyway (`bloom`, `regional_ire`), not
worth an urgent rewrite.

### 8. Flat save directory, one file per chunk

`persistence.rs:141` — `c.{x}.{z}.wfc`, roughly 14 KB apiece. A
well-explored world puts tens of thousands of small files in a single
directory, which costs on every backup, copy, and directory scan, and
on some filesystems on every individual open.

**Repair.** Region grouping — 32×32 chunks per file with an offset
table, the standard answer to exactly this problem. Not urgent, but it
is the kind of format decision that gets more expensive to make later,
and the disposable-saves rule means it is cheap to make *now*.

---

## Part III — The frame loop

Not a bug, but it scales the wrong way and it is worth naming.

`renderer/frame.rs` iterates the full `self.chunks` map five separate
times per frame — sun shadow cascades (`:291`), point-light shadow
faces (`:370`), and the main opaque/transparent passes (`:419`, `:478`,
`:498`). Every one of those loops *does* range- or frustum-cull before
drawing, so the draw calls are honest. But the **iteration** is over
everything loaded.

`CASCADE_RADII` is `[16.0, 48.0, 128.0]` (`renderer/mod.rs:71`) — the
farthest cascade reaches 128 blocks, eight chunks. At view distance 64
that means walking 16,641 hash-map entries to find the ~250 that can
cast into it, three times, plus once per point-light face, plus the
main passes.

**Repair.** A per-frame visible list built once from a spatial index
(or just a sorted vector of loaded positions, which is enough at this
scale), reused by every pass. Cheap change, and it makes the far view
distances behave.

**Also:** the comment at `renderer/frame.rs:262-264` says this pass
"is not frustum-culled." Eight lines below, it range-culls per cascade.
The prose is stale; the code is right.

---

## Part IV — God functions

Pass 1 of the modularization plan moved the *files* into a coherent
tree and Pass 2 gave `Game` and `World` real owners. Neither pass broke
up the largest *functions*, which came across intact:

| lines | location | what lives there |
|---:|---|---|
| 2,030 | `game/session.rs:20` `start_world` | ~85% `WILDFORGE_*` demo scaffolding |
| 1,510 | `game/actions.rs:343` `interact` | every interaction verb, one branch tree |
| 1,479 | `game/ui.rs:476` `build_ui_inner` | every screen |
| 1,112 | `renderer/setup.rs:6` `new` | pipelines, layouts, resources |
| 984 | `registry.rs:1353` `build` | the whole TOML → registry pipeline |
| 970 | `game/frame.rs:854` `build_and_render_frame` | |
| 866 | `multiplayer/host.rs:1096` `on_msg` | one arm per `C2S` variant |

(`atlas/procedural.rs`'s 2,164-line `build_procedural` is not on this
list. It is one ordered deterministic pixel recipe, the reason is
documented at the top of the file, and it should stay as it is.)

The modularization plan's own guardrail asks that ordinary modules stay
under roughly 1,200–1,500 lines and that anything larger have an
explicit reason. These functions individually exceed what that
guardrail asks of whole *files*.

**`start_world` is the one to do first, and it is nearly free.** There
are 67 distinct `WILDFORGE_*` environment variables in the tree; 62 of
the ~106 references sit inside that single function. Read it and it is
plainly two things fused: about 250 lines of genuine session startup
(spawn search, generator pool, player load, script hook) wrapped in
1,700 lines of capture harness — `DEMO_CAMP`, `DEMO_COLORSHADOW`,
`DEMO_PTLIGHT`, `DEMO_ROOM`, `DEMO_STEELWORKS`, and thirty-five more,
each staging a scene for a screenshot.

That harness is *good* — deterministic captures are how this project
verifies rendering, and the screenshot gates depend on it. It simply
does not belong in the function that starts a world. Lift it to
`game/demos.rs` behind one `self.apply_dev_overrides(name)` call,
placed exactly where the current blocks begin so ordering is preserved.
Move-only diff, no behavior change, and `start_world` becomes readable
for the first time.

`interact` and `on_msg` are the next two, and both split the same way —
one arm per verb, dispatched from a small match. `build_ui_inner`
splits per screen. None of these are urgent; all of them are cheaper
now than they will be after the next arc lands in them.

---

## Part V — Coverage and stale prose

- **Multiplayer has 7 tests.** Every finding in Part I is inside that
  gap, and none of them are subtle once you are looking. This is the
  single highest-leverage number in the document.
- **`docs/modding-plan.md:170`** still reads *"Status: plan approved,
  implementation not started."* All four phases shipped — data mods,
  the Rhai runtime with nine events and a sandboxed host API, hot
  reload, and the per-mod KV store. The README documents the whole
  thing and `mods/README.md` is executable, extracted and asserted by
  the test suite. Fix the status line.
- **`renderer/frame.rs:262-264`** — the "not frustum-culled" comment,
  above.
- **`config.rs:62-66`** — the 120 MB estimate, above.

---

## Deliberately not in scope

- **Any engine or framework migration.** The modularization plan's
  non-goals still hold: no ECS, no service locator, no event bus.
  Everything here is a targeted repair inside the engine that exists.
- **Interest management beyond radius culling.** Per-guest visibility
  by radius is enough for the player counts this game is built for.
  No octrees, no AOI grids.
- **Delta-compressed snapshots.** Culling fixes the cliff. Deltas are
  a bandwidth optimization for a problem we do not have yet.
- **Threading the simulation.** The single authoritative `Server` is
  the boundary worth protecting, and none of these findings need it
  split.
- **Reworking the demo harness itself.** Part IV moves it; it does not
  redesign it. The env-var interface stays exactly as it is so every
  existing capture command keeps working.

---

## Tests

The repairs and the coverage gap are the same job. Written first, these
fail on `main` today:

**Multiplayer**

- A host with 200 mobs in the world still delivers mob updates to a
  guest — assert the guest's mirror advances, and assert the
  send-failure counter is zero.
- No datagram the host emits exceeds the connection's
  `max_datagram_size()`, at any mob/player/projectile count.
- A guest that receives a chunk, unloads it, and returns to it has that
  chunk again within N ticks.
- A guest joining with view distance 16 receives a 16-chunk radius, not
  5; a guest asking for 64 against a host policy of 24 receives 24.
- A dedicated server's loaded-chunk count falls after a guest walks
  away — the leak assertion, and the one that would have caught
  finding 4 on day one.
- With two guests standing far apart, both accumulate wildlife and both
  draw hostile pressure. Assert against each guest's neighborhood, not
  against the world total.
- `SimEvent::PlayerHit` resolves to the correct guest after another
  guest has joined and left in the same tick — the positional-index
  fragility, pinned.

**Memory**

- A chunk of uniform air costs materially less than a chunk of mixed
  terrain (the sectioning assertion; fails until it exists).
- `World`'s per-cell maps stay bounded after a long simulated traverse.
- The view-distance clamp refuses a setting whose resident cost exceeds
  available memory.

**Structural**

- `start_world` compiles and behaves identically after the demo
  extraction — every existing `WILDFORGE_DEMO_*` capture still produces
  its scene. The screenshot gates already cover this; run them as the
  acceptance check.

---

## Stages

Each lands independently and leaves the game runnable. Ordered by
urgency, not by convenience.

1. **Multiplayer test scaffolding.** A two-guest fixture over the
   existing loopback harness, and the seven assertions above. Nothing
   is fixed in this stage — the point is that the next four stages have
   something to prove themselves against, and that the blind spot stops
   being a blind spot.

2. **The wire.** `C2S::RequestChunk`, negotiated view distance in the
   handshake with a host policy cap, per-guest snapshot culling,
   datagram splitting against `max_datagram_size()`, and a send-failure
   counter that is no longer discarded. Bumps `PROTOCOL` from 15 to 16;
   saves are disposable and no migration is owed. This stage closes
   findings 1, 2 and 3 together, because they are one design mistake —
   the host guessing at state the guest owns.

3. **World residency.** `World::retain_chunks`, called by both the
   client streaming path and the dedicated loop, preserving
   save-on-unload exactly. Closes finding 4 and gives the dedicated
   server the memory policy it has never had.

4. **Plural ecology.** Repopulation, hostile spawning and watcher
   attention pick from all players rather than the first, with globally
   budgeted attempts so the mob count does not scale with the player
   count. `PlayerCtx.id` replaces the positional index in
   `SimEvent::PlayerHit`. Closes finding 5.

5. **Sectioned chunks.** The `Uniform | Dense` section representation
   for blocks, meta and both light planes, plus the honest view-distance
   clamp and a corrected comment. This is the largest single change in
   the document and the one that unlocks the view distances the game
   already advertises.

6. **The demo extraction.** `game/demos.rs`, move-only, screenshot
   gates as acceptance. Cheap, and it should be done before the next
   feature arc adds a thirty-eighth demo to `start_world`.

Region-grouped saves (finding 8), the per-frame visible list (Part III),
map pruning (finding 7), and the remaining god functions are real but
not urgent. They belong in a later pass, or opportunistically alongside
whatever arc next touches their file.

Stages 1–4 are one arc and should ship as one: **multiplayer
hardening**. Stage 5 is its own piece of work and can go before or
after. Stage 6 can go any time and takes an afternoon.

---

## Implementation record — completed 2026-07-28

All six stages landed, and so did the four items this plan had deferred
to "a later pass". Where the audit was wrong, it is corrected below
rather than quietly fixed.

### What the audit got wrong

**Finding 5 was overstated.** The claim was that hostile spawning and
mob AI both followed `players.first()`. They do not: `server.rs:172-178`
already picks a *random* player each spawn cycle, and `Mob::tick` is
handed the whole players slice and uses it. The genuine defect was
narrower — **repopulation** followed `players.first()` only, so on a
shared world every guest but one lived in a country that never
recovered from being hunted. That is fixed, and it now rings a random
player per cycle exactly as the warden spawner already did. The
positional-index fragility in `SimEvent::PlayerHit` was real and is
also fixed.

**Part III's "five full-map scans" was half right.** The passes did
walk the whole loaded map, but they were already range- and
frustum-culled before drawing — the stale comment claiming otherwise
was the only thing actually wrong. The scans now share one visible list
built per frame.

### Finding 6 landed differently than described

The plan asked for `Uniform | Dense` **sections**. What shipped is
`Uniform | Dense` **whole planes**, which is the same idea without a
layout change: the chunk index is unchanged, so the mesher, worldgen
and RLE codecs were untouched.

The honest result is smaller than the plan implied. Measured on real
generated terrain: **262 KiB against the old 448 KiB**, a 1.71x
reduction, and a fresh chunk now costs nothing at all. Block light is
the win (uniform black in any chunk without a torch, which is nearly
all of them). Metadata stays dense wherever soil carries fertility, and
block ids and sky light genuinely vary per cell — getting *those* down
wants a block palette, which is a much more invasive change and is not
in this pass.

That still leaves a 64-chunk view asking for ~4 GB, so the other half
of the finding does the real work: the slider now runs to
`max_view_dist_for_memory()`, resolved once at startup from
`/proc/meminfo` (or `GlobalMemoryStatusEx` on Windows), and a config
file asking for more is clamped on the way in.

**Decided 2026-07-28: this is where finding 6 stops.** The remaining
levers were measured (see the ignored `measure_chunk_composition` probe)
and declined:

| lever | typical chunk | why not |
|---|---:|---|
| byte block palette | 199 KB | worth revisiting, but not on its own |
| + nibble-packed light | 131 KB | **touches the light planes** |
| + evict light for meshed chunks | 64 KB | **touches the light planes** |

Two of the three go through the lighting, and the lighting has an owner
and a look that is worth its cost. Wildforge eats the memory for the
aesthetic; that is a deliberate trade, not an oversight.

The accepted consequence is that the top of the slider needs headroom: a
machine with ~8 GB free reaches 64, ~4 GB reaches 46, ~2 GB reaches 32.
The clamp makes that graceful instead of fatal, which was the actual
defect. Anyone reopening this should start with the block palette, which
is the one lever that leaves the light planes entirely alone.

### The deferred items, done anyway

- **Region-grouped saves (finding 8).** 32x32 chunks per `r.x.z.wfr`
  file, magic plus a 1024-slot offset table. Writes append and then
  point the slot at the new payload, so an interrupted write leaves the
  previous payload addressed rather than a torn one; stranded payloads
  are compacted once dead weight outgrows live. Worlds saved before
  this still load — `read_chunk` falls back to the old
  `c.x.z.wfc` path, and the next save of that chunk migrates it.
- **The per-frame visible list (Part III).**
- **Finding 7** turned out to be mostly already handled: `regional_ire`,
  `bloom`, `bloom_spent` and `blessed_streak` all prune themselves via
  `retain` as they decay. That behaviour was untested, and now is.
  `last_random`, `mob_seeded` and `player_touched` are deliberately
  **not** pruned: they are permanent records (when a chunk last ticked,
  which chunks have been seeded, which ground people have worked), they
  cost 8-16 bytes per explored chunk, and dropping entries would change
  gameplay — lost catch-up bursts, re-seeded wildlife, forgotten
  worked ground.
- **The remaining god functions** (`interact`, `on_msg`,
  `build_ui_inner`, `Registry::build`, `Renderer::new`) are untouched,
  as the plan intended. `start_world` was the one worth doing now and
  it is done.

### Verification

The tree went from 358 to 367 tests, all passing, with no warnings.

Every repair has a test that was confirmed to fail without it, not just
to pass with it:

- Restoring the single-datagram send makes
  `a_crowded_world_still_reaches_the_guest` fail with **400 dropped
  datagrams**.
- Restoring `players.first()` in repopulation makes
  `wildlife_returns_to_every_country_someone_lives_in` fail with **56
  mobs around player one and 0 around player two**.

Run, not just tested: a deterministic world capture (terrain, water,
strata, a volcano's glow) and a torch-lit interior at midnight, which is
the case the compacted block-light plane exists for.

The demo extraction was checked A/B against `main` with an identical
`config.txt` — captures are not byte-deterministic (the settle frame
varies), so each scene was compared against its own run-to-run noise
floor. `DEMO_TORCHROOM` 0.00% of pixels differ, `DEMO_MILL` 0.18%,
`DEMO_CAMP` 0.17%; `DEMO_POOL` differs in 9.02% of pixels against a
noise floor of **9.12%** for that scene, because it is animated water.
Behaviourally identical.

### Verified on a live dedicated server

The loopback tests are real QUIC, but the dedicated server is its own
path, so it was driven end to end: `--server` in one process, an
`--agent` guest in another, over the network stack.

- The agent's whole 21x21 perception map came back **0/441 cells
  unstreamed**. Before this it got the fixed ring of five.
- It walked to (-90, -90), about 127 blocks onto ground it had never
  stood on, and reported `arrived`. The same request previously failed
  with "no standable ground at the goal", because the terrain under the
  goal had never been sent to it.
- The server logged `released 42 chunks` repeatedly as the guest moved.
  It had never released a chunk in its life.
- Those chunks landed in **2 region files**, not ~130 loose ones.

One real bug turned up while writing the ring test, and is fixed:
`stream_chunk` recorded a chunk as sent even when `chunk_rle` returned
nothing, so the ring skipped it forever and the guest kept a hole the
host believed it had already filled.

The agent now asks for a ten-chunk view on join. It is still a guest and
the host still clamps it — but an agent that never asked could not path
to anywhere it had not already been standing.

**Protocol 15 -> 16.** Saves are disposable by standing rule, so no
migration path was built — but region files read the old layout anyway,
because it cost four lines.
