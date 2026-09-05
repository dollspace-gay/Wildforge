//! Lunar observation coordinator for the authoritative world.

use super::{LUNAR_DAYS, MoonPhase, World};

impl World {
    /// Position in the lunar cycle, 0..1 (0 = new moon, 0.5 = full moon). A
    /// pure, deterministic function of the persisted calendar `day`, so every
    /// client and every replay agrees. Constant across a given day (it steps at
    /// dawn), so "tonight is a full moon" is a fixed, plannable fact.
    pub fn moon_cycle(&self) -> f32 {
        self.calendar_view().moon_cycle()
    }

    /// Illuminated fraction of the moon, 0..1 (0 = new/dark, 1 = full/bright).
    /// Drives moonlight strength and the disc's lit sliver.
    pub fn moon_illumination(&self) -> f32 {
        self.calendar_view().moon_illumination()
    }

    /// The named phase band for the current day — the discrete signal game
    /// systems should gate on. See [`MoonPhase`]. (Hook API; unused in-engine.)
    #[allow(dead_code)]
    pub fn moon_phase(&self) -> MoonPhase {
        MoonPhase::ORDER[(self.calendar_state.day() % LUNAR_DAYS) as usize]
    }
}
