//! Clock access calendar transaction coordination.

use crate::world::World;

impl World {
    pub fn day(&self) -> u32 { self.calendar_state.day() }

    pub fn clock(&self) -> f64 { self.calendar_state.clock() }

    pub fn long_winter(&self) -> bool { self.calendar_state.long_winter() }

    pub(crate) fn set_calendar_day(&mut self, day: u32) { self.calendar_state.set_day(day); }

    pub(crate) fn set_simulation_clock(&mut self, clock: f64) { self.calendar_state.set_clock(clock); }

    pub(crate) fn advance_calendar_day(&mut self) { self.calendar_state.advance_day(); }

    #[cfg(test)]
    pub(crate) fn set_long_winter_for_test(&mut self, falls: bool) { self.calendar_state.set_long_winter(falls); }
}
