//! Lunar observation coordinator for the authoritative world.

use super::{LUNAR_DAYS, MoonPhase, World};

impl World {
    /// The named phase band for the current day — the discrete signal game
    /// systems should gate on. See [`MoonPhase`]. (Hook API; unused in-engine.)
    #[allow(dead_code)]
    pub fn moon_phase(&self) -> MoonPhase {
        MoonPhase::ORDER[(self.calendar_state.day() % LUNAR_DAYS) as usize]
    }
}
