//! Seasons calendar transaction coordination.

use crate::world::LONG_WINTER_FRAC;
use crate::world::LONG_WINTER_MIN_DEAD;
use crate::world::SEASON_DAYS;
use crate::world::World;

impl World {
    /// 0 spring, 1 summer, 2 autumn, 3 winter.
    #[cfg(test)]
    pub fn season(&self) -> usize {
        // The Long Winter: enough countries dead and the year stops
        // turning. Everything winter already means — crops at zero,
        // no breeding, halved repopulation, water freezing — arrives
        // for free, because it IS winter, world-wide, until enough
        // hearts are relit.
        if self.calendar_state.long_winter() {
            return 3;
        }
        ((self.calendar_state.day() / SEASON_DAYS) % 4) as usize
    }

    /// How many known countries have lost their spirit, and how many
    /// are known at all.
    pub fn dead_countries(&self) -> (usize, usize) {
        // Ancient scars do not count, on either side of the ratio. The
        // badlands died before anyone alive walked there; letting them
        // into the tally would stop the world's year over history the
        // player never touched — walk through three of them early and
        // the Long Winter would fall on a world you had done nothing
        // to. Relight one and it becomes a living country like any
        // other, which is the right way for it to help lift a winter.
        let counted = self
            .hearts
            .values()
            .filter(|h| !self.is_ancient_scar_at(h.pos.surface()));
        let (mut dead, mut known) = (0, 0);
        for h in counted {
            known += 1;
            if h.stage == 0 {
                dead += 1;
            }
        }
        (dead, known)
    }

    /// Re-read whether the world's year has stopped. Returns Some(true)
    /// when the Long Winter falls and Some(false) when it lifts.
    pub(in crate::world) fn refresh_long_winter(&mut self) -> Option<bool> {
        let (dead, known) = self.dead_countries();
        // A handful of dead countries is a tragedy, not a winter; it
        // takes both a real count and a real share of the known world.
        let falls = dead >= LONG_WINTER_MIN_DEAD
            && known > 0
            && dead as f32 >= known as f32 * LONG_WINTER_FRAC;
        self.calendar_state.set_long_winter(falls)
    }

    /// 0..1 through the current season.
    pub fn season_progress(&self) -> f32 {
        self.calendar_view().season_progress()
    }

    /// Does the local atmospheric column currently deliver snow?
    pub fn snows_at_surface(&self, pos: crate::planet::SurfacePos) -> bool {
        self.weather_at_surface(pos).precipitation == crate::planet_atlas::PrecipitationForm::Snow
    }

    /// Is any conservative precipitation transfer active in this column?
    /// Deserts are not categorically vetoed; they simply receive little.
    pub fn rains_at_surface(&self, pos: crate::planet::SurfacePos) -> bool {
        self.weather_at_surface(pos).kind.precipitating()
    }
}
