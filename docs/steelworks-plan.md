# Steelworks — steel becomes A Process

Drafted and **IMPLEMENTED** 2026-07-19. Bronze is easy and stays
easy. Iron stays a furnace job. Steel became infrastructure.
Implementation notes vs. this spec: the shell is 23 firebrick + the
mouth (the mouth counts as a ring cell); the clamp smolders without
an interior glow (smoke quads only) and mining its lit log
extinguishes the whole burn; the anvil renders its bloom as a
floating sprite rather than a block model; strikes are a 2 s hammer
channel each; charcoal-only fuel is enforced at the container-click
layer; and guests ride five new protocol-4 messages plus container
kind 3 (the `Container` message also gained an `aux` field, which
incidentally gave guest furnace screens live progress).

This is the template for every metal after iron/steel: each new tier
should add a *process*, not a recipe. Design the machinery here so it
generalizes (the block-entity state machine, multiblock validation,
channel-work) and later metals reuse it.

## The chain at a glance

```
logs ─┐
      ├─ charcoal clamp (fire, wait) ──> charcoal (in bulk)
dirt ─┘
stone + ember ──> firebrick ──> bloomery stack (3x3x3 multiblock)
iron ingots + charcoal ──> charge ──> FIRE (half a day, sheltered)
        ──> steel blooms ──> anvil + hammer (worked) ──> steel ingots
```

The old `steel_blend` recipe is removed; an alias keeps old saves
loading and existing steel items are untouched. Steel output scales
with the batch: one firing turns 8 iron + 8 charcoal into 6 blooms
(≈ 6 steel) — better yield than 1:1 cooking, because you built for
it.

## Stage 1 — charcoal in bulk: the clamp

Today charcoal is smelted log-by-log; that stays (small batches).
The clamp is the bulk path:

- Build a solid pile of **log blocks** (any wood family, 2–8 of
  them, orthogonally connected), cover every exposed face with
  **dirt or packed earth**, leave exactly one log face exposed, and
  right-click it with an **ember** (consumed) to light it.
- The pile smolders — smoke puffs (entity-batch quads), interior
  light 6 — for 0.5 in-game days per log. Any covering block removed
  mid-burn vents the clamp: that log burns to nothing (tend your
  pile).
- When done, the logs have become **charcoal blocks**
  (`base:charcoal_block`, mines into 4 charcoal; also craftable
  4 charcoal ↔ 1 block; burns as fuel worth its contents).
- Implementation: a `Clamp` block entity on the lit log tracking the
  connected set + timer in `tick_entities`; validation re-checked on
  any neighboring block change (the edit log makes this cheap).

## Stage 2 — firebrick and the bloomery stack

- **`base:firebrick`**: crafted 4 stone + 1 **ember** → 4 firebrick
  (the wild's fire, baked into clay-less brick — embers come from
  fighting wardens or looting ruins, tying steel to engagement with
  the wild). Hardness 7, pickaxe.
- **`base:bloomery`**: the mouth block (interaction = its own
  screen). The multiblock is a **3×3×3 firebrick shell** centered
  above the mouth: 8 firebrick walls per layer around a hollow 1×1
  core, 3 layers tall, open top. 26 firebrick + 1 bloomery block
  total — cheap enough to build twice, dear enough to feel like
  infrastructure.
- Validation runs when the mouth is opened or lit: scan the fixed
  relative offsets; a broken shell shows "the stack is breached" in
  the UI. (Same relative-offset scan the structure placer uses,
  in reverse.)

## Stage 3 — the firing

The bloomery screen is furnace-like but batch-oriented — a 4-slot
charge (iron ingots), a 4-slot fuel bank (charcoal only; wood is not
hot enough), and a state readout:

```
Idle -> Charged -> Firing(progress) -> Done (blooms) 
             (light with an ember)
```

- A full charge is 8 iron + 8 charcoal (2 per slot). Partial
  charges fire proportionally (min 2 iron + 2 charcoal → 1 bloom;
  8+8 → 6 blooms: full batches reward the wait).
- Light it with an **ember** (consumed). It burns
  **0.5 in-game days (300 s)** real time in `tick_entities`, glowing
  light 13 from the mouth, smoke from the stack top.
- **Weather crosses in** (lands with or after the weather plan): an
  open-top stack in the rain fires 50% slower; roof it (any block ≥ 2
  above the opening — checked via sky light at the stack top) and it
  fires clean. A storm douses an unroofed firing entirely (charge
  survives, ember does not). If the weather plan lands second, the
  hook is a one-line multiplier.
- Breaking the shell mid-fire vents it: firing stops, charge
  survives, ember lost.
- Done: the mouth holds **`base:steel_bloom`** items (spongy,
  slag-ridden steel — not yet a bar).

## Stage 4 — the anvil

- **`base:stone_anvil`**: crafted from 3 stone + 2 stick (a
  hammering base); later tiers can add an iron anvil for speed.
- **`base:smith_hammer`**: 2 iron + 2 stick, a tool with
  durability.
- Right-click a bloom onto the anvil (it sits visibly on top — the
  chest-content rendering pattern), then **channel with the hammer**
  — the archaeology-brush channel mechanic reused verbatim: hold
  right-click ~2 s per strike, 3 strikes per bloom, clanging sfx and
  spark quads — and the bloom becomes a **steel ingot**. Hammer
  wears per strike.
- The anvil is deliberately generic: `interaction = "anvil"` +
  a worked-item table in data, so future metals (and mods) declare
  their own bloom→bar work without engine changes.

## What stays simple

- **Bronze**: untouched. Copper + tin cook in any furnace.
- **Iron**: raw iron still smelts to ingots in the regular furnace
  (tier gate is the bronze pick to mine it). The bloomery consumes
  iron, it doesn't replace iron-making.
- **Old saves**: `steel_blend` gets an alias to `steel_bloom`;
  blends in inventories become blooms (anvil-work them). No steel
  item ids change.

## Content inventory

Blocks: `firebrick`, `bloomery`, `charcoal_block`, `stone_anvil`.
Items: `steel_bloom`, `smith_hammer`.
Entities: `Clamp`, `Bloomery { charge, fuel, lit, progress }`,
`Anvil { held bloom, strikes }` — all persisted like furnaces, all
broadcast to guests through the existing container RPC (bloomery is
container kind 3; anvil interactions ride the authoritative
click/channel paths guests already use for brushing).
Tiles: firebrick, bloomery mouth (idle/lit), charcoal block, anvil,
bloom, hammer + gemini pack prompts for each.

## Engine work checklist

1. `BlockEntity::{Clamp, Bloomery, Anvil}` + ticks in
   `tick_entities` + TOML persistence (same shape as furnaces).
2. Multiblock validation helper (relative-offset pattern scan) —
   written generic, data-driven shell definition.
3. Bloomery screen (furnace screen variant, batch slots + state
   line) + container kind 3 over the wire.
4. Anvil render-what-it-holds + hammer channel (brush-channel code
   path parameterized).
5. Clamp lighting/venting logic hooked to the edit log.
6. Data: blocks/items/recipes/aliases + `worked` table (anvil
   recipes) — new small TOML section, mod-extensible.
7. Weather multiplier hook (no-op until weather lands).

## Tests

- Multiblock: correct shell validates; any missing/wrong block
  fails; re-validates after edits.
- Firing: full charge + ember + time → 6 blooms; partial charge
  scales; venting mid-fire keeps charge, loses ember; charcoal-only
  fuel enforced.
- Clamp: covered pile converts logs to charcoal blocks; venting
  burns the exposed log; count bounds (2–8) hold.
- Anvil: 3 hammer strikes make an ingot, hammer durability drops,
  wrong tool does nothing; `worked` table is data-driven (a test
  mod adds its own).
- Aliases: a save containing `steel_blend` loads as blooms; every
  existing steel recipe still resolves (content-graph test extends
  to the new chain — obtainability fixpoint must still close).
- Loopback: a guest charges, lights, and later collects a bloomery
  through the container RPC; anvil strikes by a guest work a bloom.
- Screenshot: lit bloomery at night (light 13), clamp smoking,
  anvil with bloom.

## Sequencing with weather-seasons

Independent — either can land first. The two touchpoints (rain
slowing an unroofed firing, storms dousing it) are one multiplier
read from whichever weather state exists, guarded to no-op when it
doesn't.
