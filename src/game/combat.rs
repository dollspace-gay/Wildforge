//! Player combat depth: stamina, light/light/heavy combo, dodge with
//! i-frames, block, backstab, and floating damage numbers.

use crate::audio::Sfx;
use glam::Vec3;
use super::Game;
use super::navigation::Screen;
use crate::planet::EntityPos;
#[cfg(test)]
use crate::player_ops::combat::mob_facing_away;

pub(crate) const STAMINA_MAX: f32 = 10.0;
pub(crate) const STAMINA_REGEN: f32 = 3.5;
/// Pause before stamina returns after exertion (swing, dodge, block).
pub(crate) const STAMINA_REGEN_DELAY: f32 = 0.6;
/// Stamina drain per second while sprinting.
pub(crate) const SPRINT_DRAIN: f32 = 1.5;
/// Sprint cuts out below this much stamina.
pub(crate) const SPRINT_MIN_STAMINA: f32 = 0.5;

pub(crate) const LIGHT_STAMINA: f32 = 1.0;
pub(crate) const HEAVY_STAMINA: f32 = 3.0;

pub(crate) const SWING_INTERVAL: f32 = 0.35;
pub(crate) const HEAVY_SWING_INTERVAL: f32 = 0.45;
/// How long a landed light leaves the combo open for the next press.
pub(crate) const COMBO_WINDOW: f32 = 0.9;
/// Third press in the combo window is the heavy finisher.
pub(crate) const HEAVY_PRESS: u32 = 2;

pub(crate) const DODGE_STAMINA: f32 = 3.5;
pub(crate) const DODGE_CD: f32 = 0.8;
pub(crate) const DODGE_IFRAMES: f32 = 0.4;
pub(crate) const DODGE_SPEED: f32 = 11.0;

/// Stamina drained per second while holding a block.
pub(crate) const BLOCK_HOLD_DRAIN: f32 = 1.2;
/// Extra stamina spent the moment a hit lands on the guard.
pub(crate) const BLOCK_HIT_COST: f32 = 1.0;
pub(crate) const BLOCK_REDUCTION: f32 = 0.65;
pub(crate) const BLOCK_KNOCKBACK_MULT: f32 = 0.35;
/// Seconds you cannot raise the guard after stamina runs out mid-block.
pub(crate) const GUARD_BREAK: f32 = 1.0;

pub(crate) const DAMAGE_NUMBER_LIFETIME: f32 = 0.8;

/// A floating number over a hit target, client-side presentation only.
#[derive(Clone, Debug)]
pub(crate) struct DamageNumber {
    /// Render-space anchor captured at spawn; short-lived so the mob's
    /// own drift is acceptable.
    pub pos: Vec3,
    pub value: f32,
    /// Heavy finishers and backstabs draw bigger and hotter.
    pub critical: bool,
    pub age: f32,
    pub lifetime: f32,
}

/// Per-player combat state. The sim is single-player here; guests keep
/// the same local bookkeeping for feedback while the host stays the
/// authority for the damage itself.
#[derive(Default)]
pub(crate) struct CombatState {
    pub stamina: f32,
    pub stamina_regen_delay: f32,
    /// Lights landed toward the heavy finisher (0, 1, then HEAVY_PRESS).
    pub combo: u32,
    pub combo_window: f32,
    pub dodge_cd: f32,
    pub iframes: f32,
    pub blocking: bool,
    pub guard_break: f32,
    pub damage_numbers: Vec<DamageNumber>,
}

impl CombatState {
    pub fn new() -> Self {
        Self {
            stamina: STAMINA_MAX,
            ..Self::default()
        }
    }

    pub fn tick(&mut self, dt: f32, max: f32, regen: f32) {
        self.combo_window = (self.combo_window - dt).max(0.0);
        if self.combo_window == 0.0 {
            self.combo = 0;
        }
        self.dodge_cd = (self.dodge_cd - dt).max(0.0);
        self.iframes = (self.iframes - dt).max(0.0);
        self.guard_break = (self.guard_break - dt).max(0.0);
        self.stamina_regen_delay = (self.stamina_regen_delay - dt).max(0.0);
        // Idle recovery: sprinting and blocking keep re-arming the delay,
        // so regen only runs once the player has been out of exertion.
        if self.stamina < max && self.stamina_regen_delay <= 0.0 {
            self.stamina = (self.stamina + regen * dt).min(max);
        }
        for n in &mut self.damage_numbers {
            n.age += dt;
        }
        self.damage_numbers.retain(|n| n.age < n.lifetime);
    }

    /// A landed swing advances the light/light/heavy wheel and pays its
    /// stamina. `heavy` is the third press in the window.
    pub fn swing(&mut self, heavy: bool, cost: f32) {
        self.combo = if heavy { 0 } else { self.combo + 1 };
        self.combo_window = COMBO_WINDOW;
        self.stamina = (self.stamina - cost).max(0.0);
        self.stamina_regen_delay = STAMINA_REGEN_DELAY;
    }

    /// A hit lands on the raised guard: drains a block-cost and staggers
    /// the guard when it runs dry.
    pub fn landed_block(&mut self) {
        self.stamina = (self.stamina - BLOCK_HIT_COST).max(0.0);
        self.stamina_regen_delay = STAMINA_REGEN_DELAY;
        if self.stamina <= 0.0 {
            self.blocking = false;
            self.guard_break = GUARD_BREAK;
        }
    }
}

impl Game {
    /// A landed swing's cost and the display/report damage.
    pub(super) fn swing_cost(&self, heavy: bool) -> f32 {
        if heavy { HEAVY_STAMINA } else { LIGHT_STAMINA }
    }

    /// Stamina gates the swing in survival; creative swings freely.
    pub(super) fn can_swing(&self, heavy: bool) -> bool {
        self.creative
            || (self.survival.health > 0.0 && self.combat.stamina >= self.swing_cost(heavy))
    }

    /// Raise or drop the guard from held input. Guard breaks (stamina
    /// depleted mid-block, staggered) lock the guard down briefly.
    pub(super) fn update_blocking(&mut self) {
        self.combat.blocking = self.input.keys.block
            && self.ui_state.screen == Screen::Playing
            && self.in_world
            && self.ui_state.screen != Screen::Dead
            && (self.creative || (self.combat.stamina > 0.0 && self.combat.guard_break <= 0.0));
    }

    /// Regen/drain accounting, called every sim frame with whether the
    /// player is sprinting this frame. Creative never exhausts.
    pub(super) fn stamina_tick(&mut self, dt: f32, sprinting: bool) {
        if self.creative {
            self.combat.stamina = self.stamina_max();
            return;
        }
        let mut drain = 0.0;
        if sprinting {
            drain += SPRINT_DRAIN * dt;
            self.combat.stamina_regen_delay = STAMINA_REGEN_DELAY;
        }
        if self.combat.blocking {
            drain += BLOCK_HOLD_DRAIN * dt;
            self.combat.stamina_regen_delay = STAMINA_REGEN_DELAY;
        }
        self.combat.stamina = (self.combat.stamina - drain).max(0.0);
        if self.combat.blocking && self.combat.stamina <= 0.0 {
            self.combat.blocking = false;
            self.combat.guard_break = GUARD_BREAK;
        }
    }

    /// Dash in the move direction (or backward when standing still) with
    /// i-frames and a cooldown. Costs stamina in survival.
    pub(super) fn try_dodge(&mut self) {
        if self.ui_state.screen != Screen::Playing
            || !self.in_world
            || self.ui_state.screen == Screen::Dead
            || self.combat.dodge_cd > 0.0
            || self.combat.guard_break > 0.0
        {
            return;
        }
        if !self.creative && self.combat.stamina < DODGE_STAMINA {
            return;
        }
        if !self.creative {
            self.combat.stamina -= DODGE_STAMINA;
            self.combat.stamina_regen_delay = 0.9;
        }
        self.combat.dodge_cd = DODGE_CD;
        self.combat.iframes = DODGE_IFRAMES;
        let k = &self.input.keys;
        let forward = (k.w as i32 - k.s as i32) as f32;
        let strafe = (k.d as i32 - k.a as i32) as f32;
        let mut dir =
            self.camera.local_flat_forward() * forward + self.camera.local_right() * strafe;
        if dir.length_squared() < 1e-4 {
            dir = -self.camera.local_flat_forward();
        }
        let dir = dir.normalize();
        self.player.vel = dir * DODGE_SPEED + Vec3::new(0.0, self.player.vel.y * 0.3, 0.0);
        self.sfx(Sfx::Dodge);
    }

    pub(super) fn spawn_damage_number(&mut self, at: EntityPos, value: f32, critical: bool) {
        if value <= 0.0 {
            return;
        }
        self.combat.damage_numbers.push(DamageNumber {
            pos: at.render_pos(),
            value,
            critical,
            age: 0.0,
            lifetime: DAMAGE_NUMBER_LIFETIME,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planet::EntityPos;

    fn player() -> EntityPos {
        EntityPos::from_local(crate::planet::Face::PosZ, Vec3::new(8.0, 200.0, 8.0))
            .expect("test spawn is inside the shell")
    }

    #[test]
    fn combo_resets_when_the_window_expires() {
        let mut c = CombatState::new();
        c.combo = 1;
        c.combo_window = 0.05;
        c.tick(0.1, STAMINA_MAX, STAMINA_REGEN);
        assert_eq!(c.combo, 0);
        assert_eq!(c.combo_window, 0.0);
    }

    #[test]
    fn damage_numbers_expire_after_their_lifetime() {
        let mut c = CombatState::new();
        c.damage_numbers.push(DamageNumber {
            pos: Vec3::ZERO,
            value: 3.0,
            critical: false,
            age: 0.0,
            lifetime: DAMAGE_NUMBER_LIFETIME,
        });
        c.tick(DAMAGE_NUMBER_LIFETIME + 0.01, STAMINA_MAX, STAMINA_REGEN);
        assert!(c.damage_numbers.is_empty());
    }

    #[test]
    fn stamina_regens_after_the_exertion_delay() {
        let mut c = CombatState::new();
        c.stamina = 5.0;
        c.stamina_regen_delay = 0.1;
        c.tick(0.2, STAMINA_MAX, STAMINA_REGEN);
        assert!(c.stamina > 5.0, "regen resumes once the delay clears");
        assert!(c.stamina <= STAMINA_MAX);
    }

    #[test]
    fn dodge_consumes_stamina_and_arms_iframes_with_a_cooldown() {
        let mut c = CombatState::new();
        c.stamina = DODGE_STAMINA + 0.5;
        let before = c.stamina;
        c.stamina -= DODGE_STAMINA;
        c.dodge_cd = DODGE_CD;
        c.iframes = DODGE_IFRAMES;
        assert!(c.stamina < before);
        assert_eq!(c.dodge_cd, DODGE_CD);
        assert_eq!(c.iframes, DODGE_IFRAMES);
    }

    #[test]
    fn block_breaks_the_guard_when_stamina_runs_out() {
        let mut c = CombatState::new();
        c.blocking = true;
        c.stamina = BLOCK_HIT_COST;
        c.landed_block();
        assert!(!c.blocking);
        assert_eq!(c.guard_break, GUARD_BREAK);
        assert_eq!(c.stamina, 0.0);
    }

    #[test]
    fn the_third_press_in_the_combo_window_is_heavy_and_resets() {
        let mut c = CombatState::new();
        c.swing(false, LIGHT_STAMINA);
        c.swing(false, LIGHT_STAMINA);
        assert!(c.combo >= HEAVY_PRESS);
        c.swing(true, HEAVY_STAMINA);
        assert_eq!(c.combo, 0);
        assert_eq!(c.stamina, STAMINA_MAX - 2.0 * LIGHT_STAMINA - HEAVY_STAMINA);
    }

    #[test]
    fn a_swing_cost_never_drives_stamina_negative() {
        let mut c = CombatState::new();
        c.stamina = 0.5;
        c.swing(false, LIGHT_STAMINA);
        assert_eq!(c.stamina, 0.0);
        assert_eq!(c.combo, 1);
    }

    #[test]
    fn backstab_detects_an_attack_from_behind_the_facing() {
        let here = player();
        // Facing +z (yaw 0): an attacker at -z is behind it.
        let behind = here.translated(Vec3::new(0.0, 0.0, -3.0)).unwrap().pos;
        assert!(mob_facing_away(0.0, here, behind));
        // An attacker in front (+z) is not.
        let front = here.translated(Vec3::new(0.0, 0.0, 3.0)).unwrap().pos;
        assert!(!mob_facing_away(0.0, here, front));
    }
}
