# Trade & travel — the economy becomes something people do

Drafted 2026-07-24. Decisions settled with dollspace: **markets are a
market stall multiblock** (our own, in the forge/bloomery validation
tradition — not floating chest-shops), pack animals before boats,
waystones with **no teleportation ever**, and barter-native pricing
(any stack can be a price; silver becomes money only if players make
it money).

The economy update built the reasons to trade: regional scarcity,
capital workshops, continuous demand. This arc builds the *acts* of
trade: moving goods, finding places, and exchanging safely. The test
for the whole arc: two strangers on a server can discover each
other's goods, travel to them, and trade — without either being
online at the same time, without dropping items on the ground, and
without trusting anyone.

## Stage 1 — pack animals (husbandry becomes logistics)

Taming rides the systems that exist: feeding (`fed` timer and
breeding already work), spawn-once wildlife, and growth.

- **Taming**: feed a wild adult its `breed_food` repeatedly (3-5
  feedings, hash-deterministic per mob) and it becomes **tamed** —
  a new mob flag, persisted with the mob, owner recorded by
  PlayerId. Tamed animals stop fleeing their owner, don't count
  against the wild's dead (no ire when a wolf... when a warden gets
  them — the loss is punishment enough), and never despawn.
- **The lead**: `leather_strip` (leather on the grid, yields 4) →
  `lead` (3 strips). Right-click a tamed animal to attach; it
  follows at walking pace. Right-click again (or click a fence-ish
  block later) to release. One animal per lead, one lead held.
- **Saddlebags**: leather + planks → `saddlebags`, applied to a
  tamed **boar or deer** (the carriers; hares and grouse are
  friends, not freight). Opens a 12-slot container riding the mob —
  a `MobCargo` inventory persisted with it, host-authoritative,
  guests through the existing container protocol (a mob-keyed
  container area — the one protocol addition this stage).
- **The risk is the point**: a laden animal killed by a warden or a
  fall spills its cargo where it died. Caravans through wild
  country want guards; that's cooperation pressure, not a tax.
- Animals under lead path like followers (reuse follow/flee
  steering), balk at drops deeper than 3, swim badly (slow in
  water). Speed matches walking so travel time is unchanged — the
  win is CAPACITY, not speed. Speed tiers come with roads (future
  work note: packed-dirt path block, +15% on paths).

Ire stance: taming draws no ire (partnership, not taking); killing
your own tamed animal refunds nothing and costs 1 ire (the wild
notices betrayal).

## Stage 2 — signs and waystones (places get names)

- **Sign**: planks + stick, places on ground or wall, holds 3 short
  lines (the 5×7 font). Text entry reuses the display-name editing
  path; rendering reuses the nameplate billboard machinery
  (raycast-gated, like player names). Sign text lives in a block
  entity and syncs to guests (extend the container payload or a
  small S2C — **PROTOCOL bump**, batched with the stall's).
- **Waystone**: 2 stone + 1 bronze ingot. Placed and named once
  (sign path). Interacting **attunes** you (client-side list,
  persisted per-world in the player file); interacting any waystone
  after that toasts bearing and distance to your other attuned
  waystones — the cairn's octant formatter, reused. No map screen,
  no minimap, **no teleport** — you still walk, you just know which
  way "Three Pines" is.
- Waystones are how stall addresses spread: a sign at a crossroads
  ("salt, fair prices — attune and follow northeast") is a working
  advertisement.

## Stage 3 — the market stall (a multiblock, like everything real)

The stall is a BUILDING in the workshop tradition: a **counter**
mouth block validated by its structure, trading only while it
stands.

- **The shape**: the counter block, flanked by two post columns
  (any log, 2 tall) one block left and right, bridged by a 3-wide
  awning row (any solid or glass) at post-top height over the
  counter line. Cheap — a stall is not a forge — but it's a *place*:
  visible from the road, wrecked if griefed (and rebuilt in a
  minute; the goods live in the entity, spilled only when the
  counter itself is broken).
- **The entity** (`StallState`): owner PlayerId + display name,
  `goods: [slot; 6]`, `price: one template stack` (item + count —
  ANY item: barter-native, three salted meats for an iron ingot is
  a legal price), `till: [slot; 6]` for earnings.
- **Owner interaction** (matched by PlayerId): a manage screen —
  stock goods, set the price template, empty the till.
- **Visitor interaction**: the shop screen — goods and price
  visible; BUY moves one price-worth of payment from the visitor's
  inventory into the till and one goods unit out. Host-authoritative
  (new C2S::StallBuy, container kind 6; the same PROTOCOL bump as
  signs). No stock or no till space → the stall politely refuses.
- **Unattended by design**: the owner mines while the stall sells.
  This is the piece that lets prices EXIST — and the observation
  post from the economy plan: when stalls start pricing in cupelled
  silver unprompted, the currency emerged. Only then consider a
  mint (still future work).
- Moderation hooks: stalls belong to their owner's PlayerId like
  saves do; a banned player's stalls stop trading (their goods stay
  theirs).

## Stage 4 — boats (the coast opens)

Deliberately last: rivers are terraced pools behind weirs, so the
water highways are **lakes, rift seas, and the coast** — which is
where regional goods live anyway (monazite beaches, arc volcanoes).

- `boat`: planks + tin ingot (tacks), a placeable vehicle entity on
  water. Mount, WASD, dismount; floats on the fluid surface height
  (the corner-stitched mesher already computes it). Slow in
  shallows, refuses land.
- `cargo boat`: boat + saddlebags pattern — 18 slots, slower.
  Sinking (boat broken on rocks/by hostiles) spills cargo to the
  bed: salvage diving is gameplay, not loss prevention.
- Guests: a vehicle entity snapshot in the existing mob/falling
  streams (small protocol addition, can ride the same bump if this
  stage lands with the others).

## Touchpoints

- mobs.rs: tamed/owner/cargo fields, lead-follow steering.
- world/ecology + entities: mob persistence grows tame/cargo data.
- New blocks: sign, waystone, stall counter. New items: leather
  strip, lead, saddlebags, boat. Tiles via gen_base_tiles.py.
- Block entities: Sign, Stall (save format additive, like Forge).
- Protocol: ONE bump covering sign text sync, stall kind + StallBuy,
  mob-cargo container area, (optionally) vehicle snapshots.
- UI: sign text entry, stall manage/shop screens (bloomery-screen
  budget: slot rects + two buttons).

## Tests

- Taming is deterministic and persists across save/load; a tamed
  animal ignores its owner's approach (no flee) and follows a lead.
- Saddlebag cargo survives save/load; death spills it.
- Stall refuses to trade unvalidated (no awning, no trade), trades
  correctly when stocked (payment lands in till, goods dispense),
  refuses on empty stock or full till, and only the owner manages.
- Waystone attunement persists; bearings match the octant math.
- Content-graph: every new item obtainable in survival.

## Stages

1. Pack animals (tame, lead, saddlebags, spill-on-death).
2. Signs + waystones (text entry, attunement, bearings).
3. The market stall multiblock (validate, manage, buy, till).
4. Boats (mount, cargo variant, salvage).

Each stage lands independently; 1-3 are the trade loop, 4 widens
the map it plays on. Solo guard holds everywhere: every feature
works alone (a stall sells to nobody but still stores; a waystone
guides its own builder home).
