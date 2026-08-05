# Running agents — the host's side of the bargain

Companion to docs/agent-mcp-plan.md, for whoever runs the server an
agent joins.

As of game protocol 40, connection success is not world entry. The agent reports
homeland progress, decodes the host's exact 3x3 entry manifest, acknowledges
it, and only becomes an actor after host acceptance. Its wider radius-10 view
then expands nearest-first. While pending it cannot move, drown, consume,
appear in the active roster, or affect ecology. Coordinates returned to MCP
use `{face,u,y,v}` and the six snake-case face names (`neg_x`, `pos_x`,
`neg_y`, `pos_y`, `neg_z`, `pos_z`). Idle water self-preservation uses the
same swim control available to a player; it does not teleport or grant water
walking.

## Starting an agent

```sh
wildforge --agent <host[:port]> --name SAWYER
```

One process is one agent: one QUIC connection, one persistent device
identity (under `saves/.agents/<name>/`), one roster row. It speaks
MCP (newline-delimited JSON-RPC) on stdio — point Claude Code or any
MCP client at the command above and the tools appear. A party of
agents is a party of processes.

Claude Code registration, for example:

```sh
claude mcp add wildforge-sawyer -- ./wildforge --agent 127.0.0.1 --name SAWYER
```

## What the host owes an agent: nothing special

An agent is admitted, allowlisted, muted, kicked, and banned with
exactly the player machinery — it presents a device key like any
client, honors `local` / `atproto_optional` / `atproto_required`
policies, and burns the same per-principal rate budgets. There is no
agent flag, no privileged endpoint, no such thing as an agent the
host must trust. If you can host players, you can host agents.

Worth knowing as an operator:

- **Ire is shared.** An agent's felling and mining charge the same
  meter and the same regional ledger as anyone's. A hired hand that
  clearcuts brings the same wrathful night a player would. The tools
  expose stewardship verbs too; what gets used is between the agent
  and whoever instructs it.
- **Perception is bounded by streaming.** Agents know what a player
  standing there could know — their mirror of the chunks you
  streamed them, the snapshots everyone gets. There is no
  omniscience to grant or revoke.
- **Movement is client-stated, like players'.** The host validates
  actions (reach, rate), not gait. An agent walks with the stock
  player physics; a modified client could always lie about position
  — that boundary is unchanged from ordinary multiplayer.
- **Death is real.** Agent inventories scatter on death; the
  `respawn` tool is how it gets back up.

## The tools (what an MCP client sees)

Perception: `look_around`, `nearest`, `at`, `inventory`, `status`,
`events` (chat arrives here — task requests ride ordinary chat).
Movement: `go_to` (A*, blocks until done), `follow` (a standing
behavior: chases the player's breadcrumb trail, hangs back, survives
across turns until `stop`). Work: `chop_nearest_tree`,
`break_block`, `place`, `craft` (planks, stick, crafting_table,
chest), `deposit` (chest via the transactional click protocol),
`station_put` / `station_take` (anvil, quern, and every hand-loaded
machine — they were built without screens, so agents use them all),
`eat`, `chat`, `respawn`.

Magic uses the same perceived, physical, host-authoritative actions as a
player. `observe_magic`, `read_knowledge`, `read_folio`,
`copy_observation`, `run_magic_experiment`, and `assemble_tuning_lens` cover
local discovery and durable signed records. `binding_frame` operates an
embodied frame; `working` aims, starts, holds, releases, or cancels a wand
working or constructed ritual; and `alchemy` operates the actual laboratory
or applies one carried preparation. These tools consume the same ingredients,
charge, water, material, wear, time, and residue and use the same reach,
line-of-sight, stable-item, and transaction checks as player interaction.

There is deliberately no tool for a global Current total, exact hidden dross,
private provenance, undiscovered recipes, raw ledger credit, arbitrary effect
selection, teleportation, transmutation, or privileged targeting. An agent can
learn more only by carrying records and instruments, building apparatus, and
doing the work in the world.

The fetch quest, end to end: you say "get me some wood" in chat; the
agent's `events` deliver it; `chop_nearest_tree` fells and collects;
`craft` turns some into a chest by a crafting table; `deposit` stows
the load; `chat` reports back. Delivery is a chest, not a hand-off —
guests have no drop-item message, and that's noted as a fast follow.

## Known gaps (v1, honest)

- No stall *buying*, mob leading, or boat riding yet (fast follows).
- Screens don't exist for agents; anything screen-shaped happens
  through clicks (chests, crafting) or not at all.
- The host does not currently verify a crafting table near a guest's
  3x3 craft — this agent checks for one voluntarily; host-side
  enforcement is an open hardening item for ALL guests.
