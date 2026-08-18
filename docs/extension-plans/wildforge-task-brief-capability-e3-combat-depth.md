# Task Brief: Player Combat Depth (capability E3)

**Repo:** WildForge, branch `feat/capability-ramp`
**Primary files:** `src/game/combat.rs` (new — stamina, combo, dodge,
block, backstab, damage numbers), `src/game/actions.rs` (swing path),
`src/game/frame.rs` (per-frame combat tick, sprint gate, dodge, guard
walk), `src/game/survival.rs` (i-frames, block reduction/knockback,
respawn), `src/game/ui.rs` (stamina bar, health bars, damage numbers),
`src/game/keymap.rs` (Alt = dodge, F = block), `src/audio.rs`
(`Dodge`/`Block` sfx), `src/net/protocol.rs` + `multiplayer/host.rs` +
`streaming.rs` + `src/game/remote.rs` (mob health sync, heavy swings,
authoritative `MobHit` feedback; PROTOCOL 40→41)
**Related design doc:** `belt-quest/docs/port-plan.md`, capability E3.

## Goal

Combat depth for belt-quest: a stamina resource gating combat actions, a
light/light/heavy combo, a dodge with i-frames, a block with a guard
break, a backstab angle check, floating damage numbers, and world-space
enemy health bars.

## What shipped

- **Stamina** (0..=10): sprint, swing, heavy, dodge, and block all cost;
  recovery waits out a short exertion delay. Sprint cuts out at the floor,
  the guard breaks when blocked dry, and the meter never drives negative.
  Creative ignores stamina. A green→amber bar sits under the hearts.
- **Combo**: the third press inside the 0.9 s combo window is a heavy
  finisher (2.5×, slower swing, extra shove); a missed window resets.
- **Dodge (Alt)**: dash in the move direction (backward when idle) at
  11 blocks/s with a 0.4 s i-frame window (every damage class is skipped)
  and a 0.8 s cooldown; costs 3.5 stamina in survival.
- **Block (F)**: holding the guard cuts wild damage 65% and knockback by
  the same factor, slows the guard walk to 45%, and drains stamina
  (hold + per-hit). Stamina running out mid-block staggers the guard for
  1 s.
- **Backstab**: a hit lands for 2× when the attacker is more than 110° off
  the mob's facing (mob forward matches its yaw).
- **Floating damage numbers**: heavy finishers and backstabs render larger
  and orange; every swing floats the dealt value over the mob.
- **Enemy health bars**: world-space bars over damaged mobs and all
  hostiles (green→amber→red), occluded behind terrain. Mob health now
  travels in snapshots so guests see the same bars.
- **Multiplayer**: guests send `heavy` with `AttackMob`; the host applies
  heavy + backstab and replies with the authoritative `MobHit { dmg, crit }`
  so guest damage numbers match the server's numbers. PROTOCOL bumped to 41.
- **Tests**: 8 combat tests (combo wheel and reset, stamina regen and
  never-negative, guard break, dodge cost/cooldown/i-frames, backstab
  geometry, damage-number expiry).

## Verification

- `cargo clippy --lib --tests -- -D warnings` clean.
- `cargo test --lib` green (suite now 937 passing).
- `wildforge --mod-qualification mods` PASS.
- Visual qualification hash refreshed (frame.rs is a qualification source).

## Keybind notes

- **Alt** dodge, **F** block, left-click combo swings. Multiplayer Tab stays
  the roster; single-player Tab still cycles the camera.

## Out of scope (later capabilities)

- E4 stat surface (stamina/hearts become modifiable there).
- Damage *type* versus the player's block (flat reduction for now; E4 may
  specialize).
- Stamina persistence: it is a combat resource that regens, so it is not
  written to the player save (E14 may revisit).