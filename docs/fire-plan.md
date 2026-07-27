# Fire — the wild's own, and yours

Fire did not exist before this. Lightning charred a single cell and paid
bloom; lava burned nothing. What follows is fire proper, built around
the one question that makes it interesting: **the wild can tell a
natural fire from arson, and answers them differently.**

Decided with Doll 2026-07-27.

## The three-way

Not a two-way. There is a middle, and the middle is agriculture.

| | who | what happens |
|---|---|---|
| **The wild's fire** | lightning, natural lava | spreads through wild growth, chars the ground, **pays bloom**. No ire. Refuses to enter a player-touched chunk — the existing invariant, unbroken. |
| **Husbandry** | your fire, your ground, your crops | burns what you planted. No ire, a little bloom. Slash-and-burn is a real practice and civilization is encouraged. |
| **Arson** | your fire, the wild's growth | **pays no bloom.** Ire per wild block consumed. Burn it yourself and the ground gives you ash, not renewal. |

The sanction is withheld renewal, not retaliation — the same shape as
the hearts arc, where the wild's real answer is withdrawal. It closes
the loop Doll flagged during the ecology arc: *"the wild resists
exploitation, but even this resistance can be exploited."* You cannot
launder a burn into a fertility harvest.

Large arson spikes regional ire, and regional ire is what strains a
heart. Burning a country's forest becomes one of the quickest ways to
kill its spirit. That needs no new machinery.

## What burns, and whose walls

**Chosen: only fire YOU started burns player-built things.**

- The wild's fire stops dead at a player-touched chunk, exactly as
  lightning already does. The wild never wrongs you unprovoked.
- Your fire burns whatever is flammable, *including your own hall*. You
  are liable for your own flames and nothing else. "The fire got away
  from me" becomes a story that can actually happen.

In multiplayer this means my fire can burn your house. That is
consistent with the shared-axe decision: the wild supplies a stake
worth governing, and the answer to a bad neighbour is a public matter
rather than a permission bit.

Flammability is a property of the block. Wood, leaves, thatch, grass
and crops burn; stone, earth, brick and glass do not. A stone hall is
worth the extra work.

## Attribution — the tool tells

**Chosen: a fire's origin is recorded when it is lit.**

- striker → yours
- lightning → the wild's
- lava → the wild's

Guilt is inherited by spread, so a fire cannot change hands halfway
down a hillside.

The named hole in this rule was lava: dig a channel into a forest and
the burn reads as natural. It closes for free, because `place_block`
and `break_block` already mark a chunk player-touched — **lava standing
on ground you have worked is your tool.** A volcano's own flow across
untouched country stays the wild's.

## Deliberately not in scope

- Fire spreading between chunks that are not loaded.
- Smoke, scorch marks on stone, or heat damage at range.
- Firebreak *tooling*. Cutting one is just breaking blocks, which is
  the point — it is a settled player's advantage and a nomad's problem.
