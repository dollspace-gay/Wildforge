# Roadmap — Iron & Steel, Ruins & Archaeology, Multiplayer

Drafted 2026-07-18. Three future milestones at design level; each gets a
detailed implementation doc when its `/goal` comes. Recommended order:
**iron & steel** (content on existing rails) → **ruins** (worldgen +
ties into iron) → **multiplayer** (the deep one — though its Phase 1
refactor is worth doing early, before the codebase grows further).

---

## 1. Iron & Steel

The tier above bronze, and the tools the ruins milestone wants.

- **Iron ore**: deep (y < 48, rarer than tin), `min_tier = 3` — bronze
  picks required. Smelts to **iron ingots** (slow, 10 s).
- **Steel**: not a straight smelt. Craft 2 iron ingots + 2 charcoal →
  **steel blend** (the bronze-blend pattern, consistent with what
  players know), then a long furnace burn (14 s) → **steel ingot**.
  Ember as fuel halves steel smelt time — the wild's fire makes the
  best steel (a reason to hunt emberkin beyond arrows).
- **Tiers**: iron tools tier 4 (speed 16×, durability 450), steel
  tier 5 (speed 20×, durability 1000). Swords: iron 10, steel 13.
  Armor: iron pieces total 14 points, steel 18 (cap stays 60% — steel
  reaches it with 15, headroom is durability). Future ores (mods) can
  gate on `min_tier = 4/5`.
- **Marquee items** (steel should unlock verbs, not just numbers):
  - **Shears** (iron): harvest leaves as placeable blocks (hedges!) and
    a future wool hook.
  - **Excavation brush** (steel + fiber): the archaeology tool — see
    ruins. Steel is the gate to the past.
- Ire: mining iron ore +0.4 like other ores; nothing new.
- Scope: content-only milestone — no engine work. Tests mirror the
  bronze-age suite (bands, chains, tier gates, recipes).

---

## 2. Ruins & Archaeology

**Lore** (fits "the wild answers" — no retcon): we are not the first
takers. Others forged here, pushed the wild to wrath, and lost the
dialogue. The wild reclaimed them — that's what a ruin *is*: a
homestead the forest finished arguing with. No ghosts, no dungeons.
Quiet places, overgrown, half-eaten by moss and root. Their bones are
lessons.

- **Structures** (worldgen feature pass, after terrain/trees):
  - *Surface*: mossy stone circles, collapsed cabin foundations,
    toppled towers (2–4 per 1000×1000 region, biome-appropriate
    materials — spruce ruins in taiga, sandstone-ish in desert).
  - *Buried*: cellar rooms and old forge chambers 5–20 blocks down,
    findable by surface hints (a chimney stub, a depression).
  - *Rare*: one **overgrown offering stone** per region, already
    ancient — it still works.
  - **Data-driven templates**: structures defined in `structures.toml`
    (palette + layer strings, like recipe patterns but 3D), placement
    rules (biome, rarity, burial depth). Mods add ruins for free.
    Structures generate deterministically from the world seed and mark
    chunks like wildlife seeding does.
- **Archaeology**: ruins contain **remnant blocks** (cracked masonry,
  root-bound chests, packed-earth floors) that the **excavation brush**
  (steel milestone) sweeps — a short channel like eating, 1 durability
  per sweep — yielding artifacts the block then permanently loses:
  - *Useful*: worn iron tools (pre-iron-age preview if found early —
    low durability, unrepairable), old coins/ingots, seeds of every
    crop, the odd bronze piece.
  - *Lore*: **etched tablets** — short generated inscriptions from the
    lost takers ("We burned the south wood. The nights grew teeth.").
    A found-lore channel for the ire system's fiction, written by
    procedure from a phrase table.
  - *Singular*: rare **charms** with small, capped passive effects
    (warden aggro −2 blocks; +1 armor point; slower hunger — one charm
    slot, no stacking, deliberately modest).
- Breaking remnant blocks instead of brushing them destroys the
  artifact (greed is punished, gently). Chests in ruins use the
  existing chest entity with loot rolled at generation.
- Ire hook: brushing is free; *looting a ruin's chest* +1 ire — the
  wild keeps its trophies.

---

## 3. Multiplayer

> The original name-claim/account notes in this roadmap are superseded by
> `multiplayer-identity-plan.md`: local profiles remain first-class, ATProto
> linking is optional globally, and individual public servers may require a
> verified DID for admission and durable moderation.

The explicit design brief: **do not repeat Minecraft's mistakes.**
The failures to design against, and our answers:

| Minecraft failure | Wildforge answer |
|---|---|
| Separate server jar, config files, port docs | **One binary.** Every copy hosts: "OPEN TO FRIENDS" in the pause menu, "JOIN" on the title screen. A headless `--server` flag exists for dedicated hosting but is the *same executable*. |
| Singleplayer and multiplayer as different codebases (merged painfully in 1.3, lag legacy) | **Integrated-server-first.** Phase 1 splits the code so singleplayer *is* a local server + a client in one process from then on. One simulation path, forever. |
| Joining a modded server = manually installing matching mods | **Content sync on join.** Our registry is string-id TOML + PNGs — the server streams its entire content set (data mods included) to joining clients, which build their registry/atlas from it. Rhai scripts run server-side only, so clients never need mod code. Join any modded server with zero setup. Texture packs stay client-side (cosmetic freedom). |
| Single-threaded server struggling at scale | Honest scope: **2–8 friends**, not public infra. Fixed 30 Hz sim tick decoupled from render; worldgen already chunk-parallelizable if needed later. |
| Trust-the-client netcode, rampant cheating | **Server-authoritative everything** — block edits validated (reach, rate, tier), inventory server-side, movement lightly validated (speed/teleport sanity). Client predicts only its own movement. |
| LAN-only or paid Realms for easy joining | v1: direct IP + LAN discovery (UDP broadcast). v2: a tiny free rendezvous for **friend codes** (hole-punching only, no game traffic through it). Never a subscription. |

- **Stack**: **quinn (QUIC)** — encryption built-in via rustls, reliable
  streams (chunks, registry sync, chat) + unreliable datagrams (entity
  snapshots) in one protocol, pure Rust (keeps the windows-gnu build
  clean). Tokio only in the network layer; the sim stays synchronous.
- **Protocol sketch**: join → registry+atlas sync (versioned, cached by
  hash) → chunk streaming near player (WFC3 RLE already exists) →
  per-tick deltas: block edits, entity snapshots (mobs interpolated
  client-side, 3–5 snapshot buffer), events (sounds, toasts, damage).
  Player saves per-name under the host's world
  (`saves/<world>/players/<name>.toml`).
- **Players are box models** — the mob model system renders remote
  players (head/body/arms/legs + name over head); armor later.
- **Design decisions the game must make** (flagged now, decided in the
  implementation doc):
  - **Ire is shared.** One world, one meter — collective
    responsibility. Your friend's clearcut is your Wrathful night.
    This is the most Wildforge thing multiplayer can do, and it's also
    a griefing vector — mitigation: per-player contribution tracking
    shown on the meter ("who angered the wild"), host kick/whitelist.
  - **Sleep**: all present players must use bedrolls to skip the night
    (offline players ignored).
  - Permissions: whitelist by default, host = op (kick, mode changes).
    No auth service v1 — name-claim among friends; note honestly that
    public servers would need real identity later.
- **Phases** (each its own doc + goal):
  1. **The split** — extract sim from Game into a tick-driven server
     struct + a client (render/input/UI) talking through an in-process
     channel pair. Singleplayer ships on this architecture and must
     feel identical. No networking yet. *This is the load-bearing
     phase and is worth doing soon.*
  2. **The wire** — quinn transport, join/sync/chunk-stream/deltas,
     LAN discovery, remote players visible and simulated, 2–8 players.
  3. **The feel** — prediction/interpolation polish, chat, shared-ire
     UI, sleep votes, per-player spawns, `--server` headless flag,
     friend-code rendezvous.

---

## Sequencing recommendation

1. **Iron & steel** — a week-scale content milestone, immediately
   playable, unlocks the brush.
2. **Ruins & archaeology** — worldgen depth + the lore payoff; wants
   iron's brush and drops iron artifacts.
3. **Multiplayer Phase 1 (the split)** — pure refactor, invisible to
   players, but every milestone built after it is multiplayer-ready
   for free. Then Phases 2–3 when ready.

Alternative if multiplayer hunger is high: do Phase 1 *first* — it
only gets more expensive as systems accrete.
