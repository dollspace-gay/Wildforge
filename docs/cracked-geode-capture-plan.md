# Cracked geode reveal and capture

Drafted 2026-08-04. **DESIGN COMPLETE; IMPLEMENTATION PENDING.**

The minerals and geology arc shipped real finite geodes but deferred its final
aspirational image: a geode cracked open in situ. This plan closes that promise
without building a fake sphere for a camera. The photographed geode must be a
shipping-worldgen structure in its natural limestone or marble host, opened by
ordinary mining, lit through ordinary game systems, and conserved by the
finite-material ledger.

"Cracked" describes the player's aperture through the rough quartz shell. It
does not require a new cracked-geode block, a decal, or a prebuilt decorative
model. The drama is the transition from anonymous pale country rock to quartz
shell, purple lining, and a dark hollow heart.

## North star

The image should make a player think, "I could have walked past that wall."
The host rock and the small worked opening establish discovery; quartz around
the lip proves a shell; amethyst beyond it gives the reveal; darkness and torch
light prove a cavity. It should look found, not spawned for a screenshot.

The capture is also a compact end-to-end qualification of atlas deposits,
chunk worldgen, lighting, block breaking, persistence, drops, and material
accounting. A beautiful fake would fail the more important half of the job.

## Scope and non-goals

This pass owns:

- deterministic selection and recording of a real production geode;
- a read-only locator/report suitable for capture preparation;
- a reproducible, ledger-honest way to open a copy of the selected save;
- framing, lighting, native-GPU capture, visual checks, and evidence metadata;
- any narrowly justified geode texture or lighting polish found necessary by
  the real capture.

It does not own:

- geode abundance, size, host rules, atlas tonnage, or mineral balance;
- a new geode gameplay loop, special loot, magical behavior, or transmutation;
- hand-placing quartz/amethyst to improve the composition;
- increasing emissive values until the unlit cavity glows by itself;
- editing, compositing, painting, or image-generating the hero frame;
- changing unrelated caves or terrain to expose a convenient cross-section.

If no current production geode can produce a readable aperture, that is a
worldgen defect to document and fix under a separate reviewed change. The
capture harness may not conceal it with a studio prop.

## What counts as a real geode

A candidate passes only when all of these agree:

1. The immutable production atlas contains a `MineralKind::Geode` deposit at
   the site and the material ledger has the corresponding finite account.
2. `Generator::geode_at(ChunkPos)` selects the chunk from that atlas deposit.
3. `Generator::plant_geode` materializes the shipping composition: natural
   limestone or marble host, quartz outer shell, quartz/amethyst lining, and
   an air heart.
4. Before preparation, the generated shell is sealed from player-reachable
   air. A natural cave may be nearby, but it may not already expose the lining.
5. The selected blocks reserve against the geode's quartz and amethyst bands;
   the material audit balances before and after the opening.

The existing `pipes_and_geodes_seed_the_deep` test proves a geode can contain
quartz, amethyst, and a void. Extend the proof from "one exists" to the exact
candidate, topology, account, and persisted reveal used by the capture.

## Deterministic site selection

### Read-only locator

Add an ignored diagnostic test or small development subcommand named for the
task, not a new survival command. Given an existing ready world, it scans the
atlas geode deposits in stable id order and evaluates only their owning chunks
plus immediate neighbors. It never mutates or saves the world.

For each candidate it reports:

- seed, generator version, atlas version, deposit id, host geology, and finite
  quartz/amethyst allowance;
- face, chunk `(u, v)`, local center `(cx, cy, cz)`, radius, and canonical
  surface/block coordinates;
- shell, lining, void, host, and accidental-air counts;
- depth below surface and the shortest solid path from reachable cave or
  surface air to the shell;
- candidate aperture directions and camera alcoves expressed in local tangent
  coordinates;
- estimated visible pixel area of host, shell, lining, and void from each
  proposed camera position.

Candidate scoring favors a radius-five geode, limestone host, a three-to-six
block discovery tunnel, a shell face not cut by a chunk boundary, enough solid
rock behind the camera, and a view direction that shows all four visual layers.
It rejects a geode intersected by unrelated caves, fluids, ore structures,
bedrock, or the world shell. Tie-break by deposit id, then face/u/v. Running it
twice on the same commit and save must select byte-for-byte identical metadata.

The chosen record is committed as the `cracked-geode` site in
`screenshots/visual-polish.toml`. It names the exact save seed and generator
version; an implementation that changes worldgen must deliberately reselect
and review the site.

### Proof test

`selected_capture_geode_is_a_sealed_production_structure` regenerates the
owning chunk from immutable inputs and asserts:

- every expected structure block matches the shipping generator;
- the heart contains air and has no path to external air before excavation;
- the shell is six-connected and surrounds the lining/heart;
- at least one planned view ray crosses host, quartz shell, amethyst or lining,
  and the heart in that order;
- the same deposit id owns every finite geode reservation involved.

The test records coordinates, not a screenshot-color interpretation.

## Opening the geode honestly

Never perform the reveal on the canonical evidence save. Copy the ready world
to a temporary qualification save and record its immutable manifest and
material audit hashes. The copy begins with the geode sealed.

Prepare it through the authoritative player-operation path:

1. Spawn or walk a qualification player to the reported tunnel start.
2. Supply a real suitable pick and torch through the capture harness's audited
   external-source mechanism, so the ledger names the addition instead of
   pretending the tools grew underground.
3. Mine the access tunnel and only the planned shell aperture using the same
   break operation, hardness, drops, tool wear, inventory, and material-ledger
   transactions as solo survival.
4. Make the aperture two blocks wide at its face and at most three shell blocks
   deep. Leave most of the sphere intact; this is a discovery, not a museum
   cross-section.
5. Place one ordinary torch just outside or just inside the lip through the
   normal placement path. A held torch may provide fill light, but it cannot be
   the only evidence if the normal held-light behavior is not persistent.
6. Save, close, reload, and capture only after the exact local entry region and
   renderer meshes have settled.

The scripted preparation helper, if added, must call the same authoritative
operations; it may not call `set_block_authored_at` for the geode or tunnel.
It fails before writing when a target block differs from the recorded sealed
structure. This prevents a stale coordinate list from drilling through a new
worldgen result.

Retain all mined drops in player inventory, loose entities, or a named chest.
Do not delete them for a clean frame. They may be moved outside the camera.

After opening, assert:

- only the recorded tunnel/aperture and torch position differ from the sealed
  local snapshot;
- shell, lining, and hollow-heart minimum counts still pass;
- no unplanned external quartz or amethyst was added;
- extracted blocks, inventory/loose/placed materials, and remaining deposit
  account reconcile exactly;
- the edits and material ledger survive a save/reload round trip.

## Capture set

The deliverable is one hero frame plus three proof frames. All four use the
same selected site, immutable source manifest, and shipping renderer. The
sealed frame is taken from the untouched source copy; the other three use the
opened qualification copy derived from it.

| Capture | Purpose |
|---|---|
| `geode-sealed-context` | Natural host wall and approach before excavation; proves the reveal was hidden. |
| `geode-aperture-proof` | Wider neutral-light frame showing tunnel, host, quartz lip, lining, and cavity together. |
| `geode-cracked-hero` | Player-eye composition with warm torch light, purple amethyst, quartz rim, and dark heart. |
| `geode-reload-proof` | Same aperture after save/reload, with capture telemetry and no staging overlay. |

The hero frame is 1920x1080 at default FOV, Gemini, view distance 12, stark
ambient, exact point-grid shadows, and bloom on. Use a fixed nighttime or deep
underground clock only if the surface sky is visible; underground lighting is
primarily the placed torch. Hide debug text and toasts. The ordinary crosshair
and hand may remain if they strengthen the first-person discovery, but no
creative/admin UI appears.

The aperture-proof frame uses a neutral exposure and enough light to verify
geometry rather than maximize mood. The sealed and reload frames use matching
camera parameters so an image diff can isolate the authored opening and normal
lighting changes.

The image must visibly include:

- at least 15% natural host rock around the opening;
- an unbroken quartz lip on three sides of the aperture;
- both quartz and amethyst lining, each larger than a 16x16-pixel patch at
  output resolution;
- dark heart pixels beyond the lining, not a wall painted black;
- no unloaded chunk face, fog wall, clipping plane, or exposed fixture edge.

Automatic segmentation uses the diagnostic block/depth attachment described
by the strata plan. It verifies composition by block identity and view depth,
not purple/white color thresholds. A human taste pass still decides whether the
hero is good; automation only rejects frames that cannot be what they claim.

## Capture invocation and artifacts

The prepared native-Windows save lives on NTFS beside the executable. The
tracked manifest supplies the exact values for a run like:

```powershell
$env:WILDFORGE_WORLD='visual-polish-geode'
$env:WILDFORGE_SHOT='screenshots/geode-cracked-hero.ppm'
$env:WILDFORGE_SHOT_SIZE='1920x1080' # capture-only override added by Goal 1
$env:WILDFORGE_SHOT_MIN_FRAME='120'
$env:WILDFORGE_PACK='gemini'
$env:WILDFORGE_VIEW_DIST='12'
$env:WILDFORGE_POS='<u>,<y>,<v>'
$env:WILDFORGE_LOOK='<yaw>,<pitch>'
$env:WILDFORGE_TIME='<manifest time>'
$env:WILDFORGE_HELD='base:torch'
.\wildforge.exe
```

Convert PPM to PNG without rescaling, filtering, recoloring, denoising, or
cropping. Record SHA-256 for both files, conversion command/tool version,
resolution, executable commit, adapter/backend, capture frame, FPS, sim/draw
milliseconds, chunk counts, and whether the world reported settled. PNG/PPM
files stay ignored; the manifest and verification report are committed.

Any frame marked `TIMED OUT, world still changing` is rejected even if it
looks correct. The capture must be produced by hardware GPU; WSLg and a CPU
adapter are not substitutes.

## Automated gates

Add focused tests and tools for the contracts above:

- `selected_capture_geode_is_a_sealed_production_structure`;
- `geode_reveal_uses_only_authoritative_break_and_place_operations`;
- `geode_reveal_materials_balance_before_and_after_reload`;
- `cracked_geode_capture_manifest_is_complete`;
- `cracked_geode_capture_contains_host_shell_lining_and_heart`.

The last test reads the committed small segmentation report, not the ignored
hero PNG. The manifest validator requires the four capture records, all hashes
and camera/world metadata, GPU backend, settled status, and matching site id.

Run the focused qualification serially because it opens and copies one save;
all ordinary tests remain parallel-safe. Then run the complete repository gate:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --release
cargo deny check advisories
```

## Performance and safety budgets

The locator must touch only the atlas and bounded candidate chunks; it may not
generate a planet-wide voxel world or exceed 512 MiB peak additional memory.
Report scanned deposits, generated chunks, elapsed time, and peak resident
memory. On the review machine it should complete in under 60 seconds after the
world manifest is loaded.

Capture staging is development-only and inert when its explicit environment
switch is absent. It adds no release tick work, network messages, save fields,
or renderer draw calls. The opened geode should render within the existing
ordinary-block path. At identical camera/settings, median draw time across five
settled frames may not regress by more than 0.20 ms and simulation by more than
0.10 ms from the sealed scene.

The helper refuses paths outside the selected qualification save, refuses a
dirty or mismatched world manifest, and never mutates the source save. A failed
or cancelled preparation removes its temporary destination rather than leaving
a half-open evidence world.

## Definition of done

This plan is complete when:

- a deterministic production geode and camera site are tracked;
- sealed topology and its finite deposit account are proven in tests;
- a copied save is opened only through normal authoritative operations;
- material conservation and persistence pass before and after reload;
- all four native-GPU captures and their reports meet the composition gates;
- the hero frame passes a human review at full resolution and thumbnail size;
- the full repository gate is green;
- `docs/minerals-geology-plan.md` replaces "deferred" with a dated link to the
  evidence and keeps the honest implementation notes intact.

Only then may the cracked-geode image be considered shipped. Finding a geode,
opening it, and discovering that the view is ugly is a useful failure: fix the
real material, light, or generator and take the picture again.
