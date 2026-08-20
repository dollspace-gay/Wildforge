//! Rail geometry & path-following motion (spec Part 2.2, scoped).
//!
//! A *train car* is a [`LocalStructure`] (Phase 5); the *track* it rides on
//! is not — rail pieces are ordinary blocks placed in the main world's chunk
//! grid, exactly like any other block. This phase adds: a small rail-piece
//! catalog with a connectivity query ([`World::rail_exit`]), and an optional
//! path-following step ([`World::tick_rail_motion`]) that advances
//! `LocalStructure`s carrying a [`RailState`] along connected rail cells,
//! updating their `LocalTransform` via the existing `Rotation` primitives —
//! no new orientation system introduced.
//!
//! ## Orientation encoding
//!
//! `BlockDef` has no per-block orientation field, so curve and incline
//! orientation is encoded as distinct block ids (`base:rail_curve_ne`,
//! `base:rail_curve_nw`, `base:rail_incline_n`) rather than a runtime-facing
//! property. The two curve variants cover the two perpendicular-axis pairs;
//! additional corners follow the same pattern.
//!
//! ## Units
//!
//! `RailState.speed` is **cells per second**. The deferred mass-driven
//! power-draw phase must agree with this unit.
//!
//! ## Not here
//!
//! Rendering/interpolation (the `progress` field exists for a future
//! renderer), collision, power draw, player throttle, car interiors, and
//! cargo are all explicitly out of scope for this phase.

use super::local_structure::LocalStructure;
use super::multiblock::Rotation;
use super::*;
use crate::planet::{BlockPos, Direction4};
use crate::registry::BlockId;

/// The rail piece occupying a cell, classified by block id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailKind {
    /// Continues along the travel axis.
    Straight,
    /// Connects two perpendicular directions; which pair is the orientation.
    Curve(CurveOrientation),
    /// Rises one block toward North over the run of the piece.
    Incline,
    /// A junction with two possible exits, one currently selected.
    Switch,
}

/// Which perpendicular pair a [`RailKind::Curve`] connects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveOrientation {
    NE,
    NW,
}

impl RailKind {
    pub fn from_block(reg: &Registry, block: BlockId) -> Option<RailKind> {
        match reg.block(block).name.as_str() {
            "base:rail" => Some(RailKind::Straight),
            "base:rail_switch" => Some(RailKind::Switch),
            "base:rail_curve_ne" => Some(RailKind::Curve(CurveOrientation::NE)),
            "base:rail_curve_nw" => Some(RailKind::Curve(CurveOrientation::NW)),
            "base:rail_incline_n" => Some(RailKind::Incline),
            _ => None,
        }
    }
}

impl World {
    /// Given the rail piece at `pos` and the direction the structure was
    /// traveling when it entered the cell, the direction to continue in —
    /// `None` when the piece doesn't connect that way (dead end / wrong
    /// orientation).
    pub fn rail_exit(&self, pos: BlockPos, entered_from: Direction4) -> Option<Direction4> {
        use Direction4::{East, North, South, West};
        let kind = RailKind::from_block(&self.reg, self.get_block_at(pos))?;
        match kind {
            RailKind::Straight => Some(entered_from),
            RailKind::Curve(orientation) => curve_exit(orientation, entered_from),
            RailKind::Incline => match entered_from {
                North => Some(North),
                South => Some(South),
                _ => None,
            },
            RailKind::Switch => {
                // Mainline runs North–South; the East branch is taken from a
                // North-bound approach only when selected. State = selected
                // exit; the toggle flips straight <-> branch.
                let selected = self.switch_selected(pos).unwrap_or(North);
                match entered_from {
                    North => Some(if selected == East { East } else { North }),
                    South => Some(South),
                    East => Some(North),
                    West => None,
                }
            }
        }
    }

    /// The cell reached by leaving `pos` in `exit`. A single-cell ramp is a
    /// diagonal link, so leaving North from an incline climbs one block, and
    /// a plain cell beside a ramp rejoins it from the high side to descend.
    /// The incline cell itself is the low end: leaving South from it stays
    /// flat. An unbuildable neighbor is `None` (a stop).
    pub fn rail_next(&self, pos: BlockPos, exit: Direction4) -> Option<BlockPos> {
        use Direction4::{North, South};
        let (du, dv) = direction_offset(exit);
        let dy = match (
            RailKind::from_block(&self.reg, self.get_block_at(pos)),
            exit,
        ) {
            (Some(RailKind::Incline), Direction4::North) => 1,
            _ => 0,
        };
        let via = pos.offset(du, dy, dv);
        if dy != 0 {
            // Leaving the ramp itself Northward: the climb IS the link.
            return via;
        }
        let Some(plain) = pos.offset(du, 0, dv) else {
            return via;
        };
        if RailKind::from_block(&self.reg, self.get_block_at(plain)).is_some() {
            return Some(plain);
        }
        // Ramp rejoin (the descent half of the climb link): a cell leaving
        // into plain air still connects onto an incline sitting one block
        // up (North) or down (South) whose high/low partner is this cell.
        if (exit == North || exit == South)
            && let Some(ramp) = plain.offset(0, if exit == South { -1 } else { 1 }, 0)
            && matches!(
                RailKind::from_block(&self.reg, self.get_block_at(ramp)),
                Some(RailKind::Incline)
            )
            && self.rail_next(ramp, if exit == South { North } else { South }) == Some(pos)
        {
            return Some(ramp);
        }
        Some(plain)
    }

    /// The currently-selected exit of the switch at `pos`, if it has one.
    pub fn switch_selected(&self, pos: BlockPos) -> Option<Direction4> {
        match self.block_entity_at(&pos) {
            Some(BlockEntity::Switch(state)) => Some(state.selected),
            _ => None,
        }
    }

    /// Toggle a rail switch between its straight-through and branch exits,
    /// creating the switch entity on first use. The choice affects only
    /// structures that arrive at the junction afterward.
    pub fn toggle_switch(&mut self, pos: BlockPos) {
        let entity = self.ensure_block_entity_at(
            pos,
            BlockEntity::Switch(crate::world::SwitchState::default()),
        );
        if let BlockEntity::Switch(state) = entity {
            state.selected = if state.selected == Direction4::East {
                Direction4::North
            } else {
                Direction4::East
            };
        }
    }

    /// Per-tick step for every structure riding rails. Advance `progress` by
    /// `speed × dt`; on each completed segment, snap the transform to the
    /// arrived cell, re-orient to the new travel direction, and query the
    /// next segment. A dead end parks the structure (`speed = 0`) rather
    /// than panicking or teleporting; progress carries over segment
    /// boundaries so speed stays consistent at any tick rate.
    ///
    /// Each tick the structure's mass is quantized into a load tier (spec
    /// §2.2), its draw `incline × tier_rate` is compared against the shaft
    /// line's delivered rate at the structure's cell, and the effective
    /// speed is stepped accordingly. A shortfall steps the load one tier
    /// heavier (slower); an Overloaded load stalls outright — the same
    /// parked state a dead end produces, so behavior stays consistent.
    pub(super) fn tick_rail_motion(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        for i in 0..self.local_structures.len() {
            // Take the structure out of the world while stepping it: the
            // step reads the world's rails (`self.get_block_at`/`self.reg`)
            // with no borrow conflicts, then the stepped structure is put
            // back in place. A `LocalStructure` is not `Clone` (its hosted
            // `BlockEntity`s are not cheap to copy), so this is move-not-
            // clone.
            let mut structure = self.local_structures.remove(i);
            let Some(rail) = structure.rail.clone() else {
                self.local_structures.insert(i, structure);
                continue;
            };
            if rail.speed <= 0.0 {
                self.local_structures.insert(i, structure);
                continue;
            }
            // Mass-driven power draw (spec §2.2): the rail piece the
            // structure is crossing sets the incline multiplier, the
            // structure's own mass sets the load tier, and the delivered
            // rate decides how fast it may actually roll. `rail.speed` is
            // the requested nominal; the effective speed is computed fresh
            // every tick and only ever stored back into the copy we step.
            // A shortfall (or an Overloaded jam) leaves the nominal intact
            // so the car resumes the instant power or load shifts.
            let base = crate::world::power_draw::load_tier_for_mass(structure.mass());
            let climbing = self.rail_climbing(rail.current_cell, rail.next_cell);
            let draw = crate::world::power_draw::incline_multiplier(
                RailKind::from_block(&self.reg, self.get_block_at(rail.current_cell)),
                climbing,
            ) * crate::world::power_draw::load_tier_rate(base);
            let delivered = self.power_at_pos(structure.transform.anchor);
            let nominal = rail.speed;
            let effective =
                crate::world::power_draw::effective_speed(base, delivered, draw, nominal);
            if effective <= 0.0 {
                // Overloaded, or the line delivers less than the draw: hold
                // at this cell rather than crawling. The nominal is kept so
                // the very next tick can roll again if power or load shifts.
                let rail = structure.rail.as_mut().expect("checked above");
                rail.progress = 0.0;
                self.local_structures.insert(i, structure);
                continue;
            }
            let rail = structure.rail.as_mut().expect("checked above");
            rail.progress += effective * dt;
            while rail.progress >= 1.0 {
                rail.progress -= 1.0;
                let arrived = rail.next_cell;
                let travel = direction_from(rail.current_cell, arrived);
                let Some(exit) = self.rail_exit(arrived, travel) else {
                    // Dead end / unbuilt track: park on the last valid cell.
                    structure.transform.anchor = arrived;
                    rail.current_cell = arrived;
                    rail.progress = 0.0;
                    rail.speed = 0.0;
                    break;
                };
                let Some(next) = self.rail_next(arrived, exit) else {
                    structure.transform.anchor = arrived;
                    rail.current_cell = arrived;
                    rail.progress = 0.0;
                    rail.speed = 0.0;
                    break;
                };
                if RailKind::from_block(&self.reg, self.get_block_at(next)).is_none() {
                    // The track runs out here; the structure parks at the
                    // arrival cell rather than riding off onto open air.
                    structure.transform.anchor = arrived;
                    rail.current_cell = arrived;
                    rail.progress = 0.0;
                    rail.speed = 0.0;
                    break;
                }
                structure.transform.anchor = arrived;
                structure.transform.rotation = rotation_for_direction(exit);
                rail.current_cell = arrived;
                rail.next_cell = next;
            }
            self.local_structures.insert(i, structure);
        }
    }

    /// Whether crossing from `current` to `next` climbs a ramp. The incline
    /// piece is the low end; leaving it North rises one block, so only a
    /// North-bound crossing of an incline piece pays the climb multiplier.
    fn rail_climbing(&self, current: BlockPos, next: BlockPos) -> bool {
        RailKind::from_block(&self.reg, self.get_block_at(current)) == Some(RailKind::Incline)
            && direction_from(current, next) == Direction4::North
    }
}

/// A curve's exit table. `entered` is the direction of travel on arrival.
fn curve_exit(orientation: CurveOrientation, entered: Direction4) -> Option<Direction4> {
    use Direction4::{East, North, South, West};
    match (orientation, entered) {
        (CurveOrientation::NE, North) => Some(East),
        (CurveOrientation::NE, East) => Some(North),
        (CurveOrientation::NE, South) => Some(West),
        (CurveOrientation::NE, West) => Some(South),
        (CurveOrientation::NW, North) => Some(West),
        (CurveOrientation::NW, West) => Some(North),
        (CurveOrientation::NW, South) => Some(East),
        (CurveOrientation::NW, East) => Some(South),
    }
}

/// Which `(du, dv)` offset a direction steps to, matching the offset space
/// `BlockPos::offset` and `Rotation::apply` share. Shared with the belt
/// geometry (capability E8), which steps cargo in the same offset space.
pub(crate) fn direction_offset(direction: Direction4) -> (i32, i32) {
    match direction {
        Direction4::East => (1, 0),
        Direction4::North => (0, 1),
        Direction4::West => (-1, 0),
        Direction4::South => (0, -1),
    }
}

/// The direction of travel from `from` toward `to` (same face, one cell).
fn direction_from(from: BlockPos, to: BlockPos) -> Direction4 {
    use Direction4::{East, North, South, West};
    if to.u() > from.u() {
        East
    } else if to.u() < from.u() {
        West
    } else if to.v() > from.v() {
        North
    } else {
        South
    }
}

/// The rotation that orients a structure's +du "front" along `direction`,
/// so `rotation.apply((1, 0, 0))` equals `direction_offset(direction)`.
fn rotation_for_direction(direction: Direction4) -> super::multiblock::Rotation {
    use super::multiblock::Rotation;
    match direction {
        Direction4::East => Rotation::R0,
        Direction4::South => Rotation::R90,
        Direction4::West => Rotation::R180,
        Direction4::North => Rotation::R270,
    }
}

/// A renderable world-space pose for a [`LocalStructure`]: where to draw it
/// and which way it faces. `position` is the same `glam::Vec3` render space
/// the rest of the renderer draws in (`EntityPos::render_pos`); `facing` is
/// the structure's discrete cardinal orientation.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(dead_code)]
pub struct RenderPose {
    pub position: glam::Vec3,
    pub facing: Rotation,
}

/// Compute the continuous world-space pose a structure should be drawn at.
///
/// A static structure (`rail: None`) resolves directly from its transform,
/// exactly as Phase 5's API already reported. A structure riding rails is
/// positioned by converting *both* the current and next rail cells into
/// render space first (`BlockPos::entity_center().render_pos()`) and then
/// linearly interpolating with `rail.progress` — the same convert-then-lerp
/// shape the renderer uses for guests and mobs. It never lerps the raw
/// `(u, y, v)` block integers, which remap discontinuously across planet
/// face seams.
///
/// `facing` always equals `transform.rotation`, which Phase 6 sets at
/// cell-boundary crossings; it snaps at boundaries and never turns,
/// matching the discrete four-way orientation model.
///
/// Cross-face segments are deliberately not interpolated: when the two
/// cells sit on different planet faces the pose snaps to the current cell
/// instead of producing a wrong lerp across the seam. `world` is accepted
/// for API symmetry with future topology-aware queries (light, labels); it
/// is not needed to compute the pose itself.
///
/// Forward-looking renderer API, wired into the scene graph in a later
/// phase; exercised by unit tests only for now.
#[allow(dead_code)]
pub fn interpolated_pose(structure: &LocalStructure, _world: &World) -> RenderPose {
    let facing = structure.transform.rotation;
    let Some(rail) = &structure.rail else {
        return RenderPose {
            position: structure.transform.anchor.entity_center().render_pos(),
            facing,
        };
    };
    let from = rail.current_cell.entity_center().render_pos();
    if rail.next_cell.face() != rail.current_cell.face() {
        return RenderPose {
            position: from,
            facing,
        };
    }
    let to = rail.next_cell.entity_center().render_pos();
    let position = from.lerp(to, rail.progress.clamp(0.0, 1.0));
    RenderPose { position, facing }
}
