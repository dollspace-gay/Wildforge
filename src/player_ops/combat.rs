//! Shared melee geometry and damage rules, independent of presentation.

use crate::planet::EntityPos;
use glam::Vec2;

const HEAVY_MULT: f32 = 2.5;
const BACKSTAB_MULT: f32 = 2.0;
const BACKSTAB_CONE_DEG: f32 = 110.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MeleeDamage {
    pub(crate) amount: f32,
    pub(crate) critical: bool,
}

/// Adapters supply their admitted base damage and attack flags. Stamina,
/// cooldowns, mode-specific eligibility, and presentation remain with the actor.
pub(crate) fn melee_damage(base: f32, heavy: bool, backstab: bool) -> MeleeDamage {
    let mut amount = base;
    if heavy {
        amount *= HEAVY_MULT;
    }
    if backstab {
        amount *= BACKSTAB_MULT;
    }
    MeleeDamage {
        amount,
        critical: heavy || backstab,
    }
}

/// The attacker lies behind the mob's facing in its local tangent frame.
pub(crate) fn mob_facing_away(yaw: f32, mob_pos: EntityPos, from: EntityPos) -> bool {
    let delta = mob_pos.local_delta_to(from);
    let to = Vec2::new(delta.x, delta.z);
    let len = to.length();
    if len < 1e-4 {
        return false;
    }
    let to = to / len;
    let forward = Vec2::new(yaw.sin(), yaw.cos());
    forward.dot(to) < BACKSTAB_CONE_DEG.to_radians().cos()
}
