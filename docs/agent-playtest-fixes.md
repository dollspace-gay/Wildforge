# Agent navigation and predator playtest repairs

The live Carmilla playtest on `8c1ec40` exposed an agent pushing into leaves, a walk returning an earlier arrival message, and a wolf pursuing a player despite nearby wildlife. Changes are on `fix/agent-leaf-navigation`.

## Navigation

Category A (root cause): follow mode now plans and caches walkable waypoints separately from the leader's observed trail. It previously steered directly at the next breadcrumb and waited for a stuck detector before pathfinding. That produced 4.64 seconds of continuous collision with a known leaf canopy in the QUIC regression. The repaired canopy, narrow-turn, and planet-edge scenarios each recorded 0.00 seconds pinned. They verify both local movement and the host's authoritative player position, and leave the leaf blocks intact.

Waypoint completion stays within the player's horizontal clearance; vertical hopping cannot reset horizontal stuck detection. Agent motion also uses the same surface-to-chart conversion as the graphical player. The existing camera calculation was extracted unchanged into `LocalFrame::chart_basis`, and the agent now applies face rotations after physics.

Recorded breadcrumbs are completed at their planned cell centers, matching the route destinations. A final-review regression initially stopped 9.23 blocks behind the leader because an off-center player snapshot could never be reached by a center-ending route. The regression records several such snapshots over QUIC and checks progress through the entire trail, including the authoritative host position.

Category B (local correctness): `wait_idle` reports an active walk timing out instead of searching old events for a misleading arrival. It does not cancel the ongoing walk. A regression begins a new walk with an old arrival still in the event queue and verifies the timeout result.

Call sites were inspected with `rg -n 'chart_basis|motion::basis|wait_idle\(' src --glob '*.rs'`. The shared conversion is used by the camera and agent; the wait helper is used by MCP movement and tree gathering. These are internal module changes in the single Wildforge crate; no public crate API, protocol version, dependencies, MSRV, or lint allowances changed.

## Wildlife

Category B (local correctness): the trophic pre-pass selects animal prey before enabling desperation attacks on players. Wolves and other ordinary predators must be starving and have no eligible prey within their existing hunting range. Existing night, winter, and industrial-pressure conditions still apply. The AI rechecks qualification during an active chase: food becoming available or hunger ending cancels it. Wounded wildlife retains its flee state. Fierce animals and hostile wardens keep their existing rules.

The original starving-wolf-with-a-nearby-deer regression failed because the wolf became bold toward the player. Regression coverage now includes available prey, prey appearing during a chase, a fed wolf ending a chase, hunger short of starvation, and wounded flight. The existing desperate-wolf and fierce-polar-bear tests retain the positive aggression cases.

The user clarified that the frog-like attackers dropped vines, consistent with thornlings and their thorn-fiber drop. No frog chase was established. The base frog has no prey list and is configured to flee; a direct simulation regression verifies that it flees a nearby player even when hungry at night. Thornling hostility and frog balance or appearance data were not changed.

## Validation

- Agent integration lane: 15 passed, including authoritative leaf navigation, off-center breadcrumbs, and timeout reporting.
- Focused wildlife lane: 10 passed, including six new predator/frog scenarios.
- Strict all-target/all-feature Clippy: passed.
- Formatting: passed.
- Full subsystem suite: 1017 passed, 24 intentionally ignored; agent integration runs separately.
- Doctests: passed (zero defined).
- Release rebuild: passed after the final breadcrumb correction.
- No unsafe code was added or modified; Miri is not applicable to this change.

Local runtime evidence and complete command output are retained under the ignored `saves/carmilla-agent-playtest-20260904/` directory. The original live world, player identities, and MCP transcript are preserved there.

The corrected release was restarted on the original survival world. Carmilla rejoined with the same identity and saved position, then followed the graphical Laurelai guest across the PosZ/PosY face boundary and back to camp. MCP observations recorded the gap closing from 37 blocks to 3 while continuing to follow subsequent player movement. The visible client rendered on the NVIDIA Vulkan adapter. This is a live movement/reconnect check; deterministic leaf and wildlife behavior is covered by the regressions above.
