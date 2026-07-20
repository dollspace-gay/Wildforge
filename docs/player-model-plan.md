# The player, seen — model, skin, and appearance plan

Drafted and **IMPLEMENTED** 2026-07-19, all five stages. Notes vs.
this spec: hair tiles stay procedural even in the gemini pack (the
alpha-cut fringe is not something the image model paints reliably;
everything else regenerated as desaturated tint bases with a
scripted grey pass); the variant tiles live at slots 980–1023 (44:
6 skins ×2 for faces, 8 hairs ×2 for the crown tile, 10 shirts, 6
trousers) — derived from the bases *after* pack layering, so
repainted bases carry into every color automatically; the title
screen went two-column to fit the APPEARANCE button (which also
fixed its long-standing overflow with many worlds); and the walk
cycle/held-item work landed with the model rebuild rather than as a
separate stage. The gemini face came out softly plush-friendly —
epicene as ordered, more woodland companion than action figure,
which suits the game.

**Amended same day** (dollspace: "people should be able to choose a
gendered appearance and clothing, it should just default to
neutral"): the style grew hair length (bald / cropped / short /
long — long keeps the short fringe up front and falls at the sides
and back), facial hair (none / moustache / trimmed / full, tinted
with the hair color), legwear (trousers / knee-length skirt over
leggings), and build (slight / standard / broad shoulders and
arms). Everything still defaults to the neutral look; gendered
reads are opt-in choices. The u32 repacked to bit fields (protocol
8); variant tiles grew to 84 (940–1023) with the extra neutral
bases at 932+; the APPEARANCE screen is eight rows.

As drafted — the state this plan fixed: the remote-player render was a half-scale
mannequin: ~1.05 blocks tall against a 1.8-block hitbox (it reads as
waist-deep in the ground), every box face wrapped in one "tunic"
tile, a face pasted on the head front, no hands, no animation, no
held item — and what identity it has defaults masculine. Decisions
settled with dollspace: **full pass** (proportions + skin + walk
animation + held item), **gender-neutral default**, **texture-pack
override documented**, and an **appearance menu** so players choose
their look.

## Proportions (the Steve bar, at our scale)

Rebuild `emit_humanoid` on a 16px-per-block grid, ~29px (≈1.81
blocks) tall to match the hitbox:

- **Legs**: 3×12×3 px each, hips at x ±1.5 — trouser upper, boot
  lower (boot is part of the leg tile, bottom 3px).
- **Torso**: 8×10×4 px.
- **Arms**: 3×9×3 px sleeves hanging from the shoulders, plus
  **3×3×3 px hand boxes** at the sleeve ends. Separate hand boxes
  are what make per-part tinting possible: a hand face is all skin,
  a sleeve face is all shirt.
- **Head**: 7×7×7 px on the shoulders. A **hair overlay box** at
  7.6px (slightly inflated, alpha-cut texture) wraps the top and
  upper sides — hair silhouette without painting hair into the head
  tiles.
- Total: 12 (legs) + 10 (torso) + 7 (head) ≈ 29px. Arms reach to
  ~hip level like they should, not T-rex stubs.

## Skin tiles: a neutral base that tints

Six tiles, each a **single material** painted in near-greyscale so a
per-part color multiply (via the existing vertex light channel — the
same mechanism as the mob hurt-flash) produces the final look:

| slot | tile | tinted by |
|---|---|---|
| 203 (reuse) | `player_shirt` — fabric with fold shading, light grey | shirt color |
| 204 (reuse) | `player_face` — neutral eyes + subtle mouth on plain skin, **no stubble, no gendered coding** | skin tone |
| 234 | `player_skin` — plain skin (head sides/back/top under hair, hands) | skin tone |
| 235 | `player_hair` — alpha-cut cap/fringe band, mid-length, androgynous | hair color |
| 236 | `player_trousers` — plain weave | trouser color |
| 237 | `player_boot` — laced leather, darkish (fixed, untinted) | — |

This exactly fills the remaining builtin atlas budget (238/239 stay
spare). Procedural painters and gemini prompts both specify the
androgynous read; the gemini face prompt asks for "neutral friendly
face, epicene, simple dark eyes" — no beard-by-default.

Because these are ordinary named tiles, **texture packs and mods can
already reskin them** (`packs/<id>/tiles/player_face.png` etc.) —
document it in the packs section of the README. A pack that wants a
specific look can paint in full color and players' tints will
multiply over it (packs aiming for fixed art should paint mid-grey
where they want tinting to read true).

## Appearance: chosen, palette-based, on the wire

- **Palette, not sliders**: 6 skin tones (light to deep, plus one
  frankly unnatural for the ghosts among us), 8 hair colors, 10
  shirt colors, 6 trouser colors. Palette indices pack into a single
  `u32` — trivial to persist and to ship over the wire, and every
  combination is art-directed enough to look right.
- **Menu**: an APPEARANCE screen reached from the title screen and
  the pause menu — four cycling swatch rows (SKIN / HAIR / SHIRT /
  TROUSERS) plus a live preview: the humanoid model rendered
  slowly rotating in the screen center (drawn through the existing
  entity pipeline into a small viewport quad — same trick as the
  viewmodel's depth-cleared pass).
- **Persistence**: `appearance=<u32>` in config.txt (it's identity,
  like the player name).
- **Multiplayer (protocol 7)**: `C2S::Hello` gains `style: u32`;
  the host stores it per guest and the `Players` snapshot carries
  `(id, pos, yaw, held, style)`. Everyone renders everyone else
  with their chosen appearance. Loopback-tested round trip like
  held items were.

## Motion and hands

- **Walk cycle**: arms and legs swing from horizontal speed, exactly
  the mob mechanism (phase accumulates with distance, diagonal
  pairs opposed). Remote players already interpolate positions —
  the apparent speed drives the phase, so guests and host see the
  same gait. Idle = arms at rest, no swing.
- **Held item in hand**: the right hand renders the held item —
  blocks as a small cube, items as their sprite quad — riding the
  arm swing. The data has been on the wire since protocol 6; a
  friend carrying a torch finally *holds a torch* (whose light
  already follows them).

## First-person consistency

The viewmodel arm uses the same skin-tone tint from your own
appearance, so the hand you see is the hand others see.

## Tests

- Humanoid geometry: total height within 1.75–1.85 blocks; hands
  present; no box extends below y=0 (the sinking bug, pinned).
- Style pack/unpack round-trips all palette indices; out-of-range
  indices clamp.
- Config roundtrip for `appearance=`.
- Loopback: style rides Hello → Players and comes back.
- Screenshots: the demo player (WILDFORGE_DEMO_PLAYER) at day,
  mid-stride; appearance screen with preview; two players with
  different styles side by side (loopback-staged).
- All suites green; mods/README packs section documents the player
  tile names.

## Stages

1. **Proportions + tiles** — new box layout with hands and hair
   overlay, six neutral tiles (painters + gemini prompts), tint
   plumbing through emit_humanoid.
2. **Appearance data + menu** — palette, config, APPEARANCE screen
   with rotating preview.
3. **Protocol 7** — style in Hello/Players, loopback test.
4. **Motion** — walk cycle + held item in hand + first-person hand
   tint.
5. **Docs & screenshots** — README packs note, demo shots.
