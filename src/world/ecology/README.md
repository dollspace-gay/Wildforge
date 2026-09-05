<!-- wildforge:guide -->

# Ecology transactions

Population owns private live collections, identity cursors, and NPC links.
The parent ecology facade admits manifested mobs only after their Current
custody is established. These modules coordinate domain effects against World:
habitat reads, deterministic wildlife placement, ordered mob/NPC stepping,
watcher/hostile/nest admission, loose-item loss, projectile collisions, and death.

Keep stateful physics and custody effects in their existing order. Admission
cannot silently create material or Current, and replica presentation never calls
these coordinators. Persistence reads Population through the existing codecs.
The large mob step is an explicit ordered transaction; further extraction must
pass local query/effect contexts rather than a broad mutable world to helpers.

Read [AGENTS.md](AGENTS.md). Final validation covers ecology/mob/projectile tests,
NPC links and ID allocation, save/load, cargo/drop conservation, seam-aware
movement, predator feeding eligibility, and the complete applicable Rust gates.

Population also owns wildlife seeded-chunk marks and the distinct hostile, nest,
and season-scaled repopulation cadences. World adoption and blessing coordinate
mark insertion/removal; the population owner determines whether a spawn cycle is
due. The WFA1 seeded-chunk sidecar bytes and iteration policy are unchanged.
