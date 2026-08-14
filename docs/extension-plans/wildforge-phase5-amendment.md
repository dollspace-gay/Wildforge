# Phase 5 Amendment: Canonical-Orientation Storage + Structure Removal

**Repo:** WildForge, branch `feat/multiblock-recognition`
**Primary file:** `src/world/local_structure.rs`
**Amends:** `wildforge-task-brief-phase5-local-structures.md`, against
the implementation as it currently stands. Two changes, both scoped to
this file (plus the `remove_structure` addition's natural command-surface
counterpart if one already exists elsewhere — see Change 2).

## Why

Code review of the Phase 5 implementation found one convention mismatch
worth fixing before anything is built on top of it, plus one small gap
noted at the same time. Both are cheap to fix now, while
`local_structure.rs` is still fresh and nothing downstream depends on
its current behavior yet.

## Change 1 — Store blocks in canonical (unrotated) orientation

**Problem.** `Template`/`rotated_cells` (`template.rs`, Phase 4)
establish a clear convention: captured cells are stored once, in
as-captured orientation, and rotation is applied lazily, only at the
moment a world position is actually needed. `LocalStructure::rotate`
does the opposite — it bakes the rotation into `self.blocks`' keys
*and* separately records the same rotation in `transform.rotation`.
That's internally consistent (save/load round-trips correctly as
written), but it's a second, different convention living next to the
first, and it sets up a specific bug for whoever writes the first
world-position-resolution code in Phase 6:

```rust
// looks right, matches how a Template resolves a cell — and is WRONG here,
// because local_offset (a key in `blocks`) is already rotated:
let world_pos = transform.anchor.offset(transform.rotation.apply(local_offset));
```

Silently wrong at `R90`/`R180`/`R270`, correct only at `R0` — the kind
of bug that passes casual testing and surfaces later as "why is this
train car's furniture backwards."

**Fix.** Make `LocalStructure.blocks` canonical forever, exactly like
`Template.cells`. `transform.rotation` becomes the single source of
truth for current orientation; nothing else is allowed to also encode
rotation.

Concretely:

- `LocalStructure::rotate` no longer touches `self.blocks`. It becomes:
  ```rust
  pub fn rotate(&mut self, rot: Rotation) {
      self.transform.rotation = compose_rotation(self.transform.rotation, rot);
  }
  ```
  `compose_rotation` is unchanged and still needed — orientation still
  composes across multiple rotations, it just no longer re-bakes storage
  each time (this is also strictly cheaper: O(1) instead of remapping
  every cell per rotation).
- `from_template` is unaffected — it already produces canonical keys;
  it just no longer receives a `rotate()` call in `spawn_structure` that
  would bake them.
- Add a doc comment directly on the `blocks` field stating the
  invariant plainly: *"Stored in canonical, as-captured orientation.
  Never rotated in place — consult `transform.rotation` and apply it at
  resolution time. Mirrors `Template`/`rotated_cells`."* This is the
  kind of thing worth saying in the type itself, not only in a design
  doc, since it's the type itself that future code will be staring at.
- **Recommended, not strictly required by the brief, but cheap and
  directly forecloses the bug rather than just documenting around it:**
  add the one correct resolution helper now, even though nothing calls
  it yet (Phase 6's job):
  ```rust
  /// The world position of a local cell, applying the current rotation
  /// exactly once. The only correct way to resolve a `blocks` key to a
  /// world position — mirrors `template.rs::rotated_cells`.
  pub fn world_position(&self, local_offset: (i32, i32, i32)) -> Option<BlockPos> {
      let rotated = self.transform.rotation.apply(local_offset);
      self.transform.anchor.offset(rotated.0, rotated.1, rotated.2)
  }
  ```
  Providing the correct function is a stronger guarantee than a comment
  alone — it gives Phase 6 one obviously-right thing to call instead of
  one obviously-plausible thing to reinvent incorrectly.

**Knock-on effect, in the fixer's favor:** `save_local_structures` and
`load_local_structures` need no logic change — they already just
serialize/deserialize whatever's in `blocks` plus `transform.rotation`
as separate fields. With canonical storage, the persisted `cells` now
exactly match the original template's cells (one less thing to reason
about when inspecting a save file by hand), and `rotation` is no longer
redundant with what's baked into the keys — it's the only place
orientation lives, which is the point.

## Change 2 — Structure removal

**Gap.** `template.rs` has `remove_template`/a `drop` command.
`local_structure.rs` has no equivalent — once spawned, a structure has
no way to be un-spawned short of hand-editing
`local_structures.toml`. Not required by the original brief, but worth
closing now for save-file hygiene during Phase 6 testing (spawning and
discarding test structures repeatedly while iterating on
rendering/movement will otherwise accumulate cruft with no cleanup
path).

**Fix.** Mirror `remove_template` exactly:

```rust
/// Remove a spawned structure by id (and persist). Returns whether one
/// was removed.
pub fn remove_structure(&mut self, id: LocalStructureId) -> bool {
    let before = self.local_structures.len();
    self.local_structures.retain(|s| s.id != id);
    let removed = self.local_structures.len() < before;
    if removed {
        let _ = self.save_local_structures();
    }
    removed
}
```

If a command surface for structures already exists elsewhere (not
visible in the file reviewed for the original brief), add a `despawn
<id>` command alongside it, following `template_command`'s `drop`
exactly. If no such command surface exists yet, this method alone is
sufficient for this amendment — wiring a command is fine to leave for
whenever `spawn` itself gets a command entry.

## Updated Acceptance Criteria (supersedes conflicting items in the
original Phase 5 brief)

- `LocalStructure.blocks` keys are identical to the source template's
  cells at every rotation — spawning the same template at `R0`, `R90`,
  `R180`, `R270` produces four structures with **identical** `blocks`
  and **different** `transform.rotation`, not four differently-keyed
  block stores.
- `world_position` (if added) returns the same result as manually
  computing `anchor.offset(rotation.apply(offset))` for every cell —
  effectively a restatement of `rotated_cells`' correctness guarantee,
  now for structures instead of stamps.
- Repeated `rotate()` calls compose correctly and remain O(1) each (no
  full-store remap per call).
- `remove_structure` removes the structure from both the in-memory
  collection and `local_structures.toml`; removing a nonexistent id is a
  clean no-op (returns `false`, does not error).
- New/updated unit tests: rotation-then-inspect-`blocks` confirms
  canonical storage is unchanged across rotations; multi-rotation
  composition test; remove-then-reload confirms persistence reflects
  the removal.
