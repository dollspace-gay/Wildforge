//! Ire tick calendar transaction coordination.

use crate::chunk::ChunkPos;
use crate::world::RegionCell;
use crate::world::SEASON_DAYS;
use crate::world::World;

impl World {
    pub fn ire_tier(&self) -> usize {
        Self::tier_of(self.ire)
    }

    pub(super) fn tier_of(ire: f32) -> usize {
        crate::world::calendar_view::ire_tier(ire)
    }

    /// The tier as this ground feels it: the world's mood shifted by
    /// the local ledger (±20 regional ≈ ±2 tiers — an angry forest is
    /// menacing, not lethal; a tended valley forgives a lot).
    #[cfg(test)]
    pub fn ire_tier_at(&self, x: i32, z: i32) -> usize {
        Self::tier_of((self.ire + self.regional_ire_at(x, z) * 3.0).clamp(0.0, 100.0))
    }

    pub fn ire_tier_at_surface(&self, pos: crate::planet::SurfacePos) -> usize {
        Self::tier_of((self.ire + self.regional_ire_at_surface(pos) * 3.0).clamp(0.0, 100.0))
    }

    pub fn add_ire(&mut self, amt: f32) {
        self.ire = (self.ire + amt).clamp(0.0, 100.0);
    }

    /// Planting refunds ire, capped per day — mending stays slower than
    /// taking; a clearcut can't be laundered with a seed drawer.
    pub fn plant_ire(&mut self, amt: f32) {
        let room = (8.0 - self.plant_ire_today).max(0.0);
        let refund = amt.min(room);
        if refund > 0.0 {
            self.plant_ire_today += refund;
            self.add_ire(-refund);
        }
    }

    /// Advance ire time by a fraction of a day: passive decay (-4/day)
    /// and the daily reset of the planting cap. Returns true at dawn
    /// (day rollover) — the moment offerings are accepted.
    pub fn tick_ire(&mut self, day_frac: f32) -> bool {
        // The wild breathes easier when the land drinks.
        let planetary_rain = self.weather_state.live().is_some_and(|weather| {
            weather.last_report.precipitation_units > 0
                && weather.last_report.unexplained_water_drift == 0
        });
        let decay = if planetary_rain { 5.0 } else { 4.0 };
        self.add_ire(-decay * day_frac);
        // Grudges and gratitude both fade (2 per day toward zero).
        self.regional_ire.retain(|_, v| {
            *v -= v.signum() * (2.0 * day_frac).min(v.abs());
            v.abs() >= 0.01
        });
        if self.ruleset().hearts {
            self.tick_hearts(day_frac);
        }
        self.refresh_long_winter();
        self.tick_rooting(day_frac);
        self.tick_graft(day_frac);
        // Blooms burn down day by day.
        self.bloom.retain(|_, v| {
            *v -= day_frac;
            *v > 0.0
        });
        if self.calendar_state.advance_reciprocity(day_frac) {
            self.plant_ire_today = 0.0;
            // The wild forgives, slowly: a cell held deeply blessed
            // for a full season earns ONE wildlife reseed — its
            // hunted-out chunks roll again when next visited.
            let blessed: Vec<RegionCell> = self
                .regional_ire
                .iter()
                .filter(|(_, v)| **v < -10.0)
                .map(|(c, _)| *c)
                .collect();
            for cell in blessed {
                let streak = self.blessed_streak.entry(cell).or_insert(0);
                *streak += 1;
                if *streak >= SEASON_DAYS {
                    self.blessed_streak.remove(&cell);
                    let (cu0, cv0) = (u16::from(cell.u) * 16, u16::from(cell.v) * 16);
                    for du in 0..16 {
                        for dv in 0..16 {
                            let pos = ChunkPos::new(cell.face, cu0 + du, cv0 + dv)
                                .expect("regional ledger cells partition each face");
                            self.population.forget_seeded(pos);
                        }
                    }
                    self.whispers
                        .push("The land breathes. Something returns.".to_string());
                }
            }
            self.blessed_streak
                .retain(|c, _| self.regional_ire.get(c).is_some_and(|&v| v < -10.0));
            return true;
        }
        false
    }
}
