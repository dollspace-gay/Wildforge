//! Authoritative calendar state and explicit transitions.

use super::calendar_view::CalendarView;

#[derive(Default)]
pub(super) struct CalendarState {
    day: u32,
    clock: f64,
    long_winter: bool,
    /// Reciprocity dawn cadence stays independent of the server day counter.
    day_progress: f32,
}

impl CalendarState {
    pub(super) fn day(&self) -> u32 { self.day }
    pub(super) fn clock(&self) -> f64 { self.clock }
    pub(super) fn long_winter(&self) -> bool { self.long_winter }
    pub(super) fn view(&self) -> CalendarView { CalendarView::new(self.day, self.clock, self.long_winter) }
    pub(super) fn set_day(&mut self, day: u32) { self.day = day; }
    pub(super) fn set_clock(&mut self, clock: f64) { self.clock = clock; }
    pub(super) fn advance_day(&mut self) { self.day = self.day.wrapping_add(1); }

    pub(super) fn set_long_winter(&mut self, falls: bool) -> Option<bool> {
        if falls == self.long_winter { return None; }
        self.long_winter = falls;
        Some(falls)
    }

    /// Preserve the original single rollover per call, including any remaining
    /// fraction when a caller advances by more than a day.
    pub(super) fn advance_reciprocity(&mut self, fraction: f32) -> bool {
        self.day_progress += fraction;
        if self.day_progress >= 1.0 { self.day_progress -= 1.0; true } else { false }
    }
}
