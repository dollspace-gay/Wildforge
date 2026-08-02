//! Fire: the wild's own, and yours.
//!
//! The whole point of this module is that a flame remembers the hand
//! that lit it. A fire started by lightning in country nobody has
//! worked is part of the biome's cycle — it chars the ground and pays
//! bloom, and it will not cross onto ground a player has touched. A
//! fire a player struck is theirs: it burns whatever is flammable,
//! their own hall included, it pays no bloom at all, and it costs ire
//! for every wild thing it eats. Burning your own crops on your own
//! ground is neither — that is agriculture.
//!
//! See docs/fire-plan.md.

use super::*;
use crate::planet::{BlockPos, Direction6, step6};

/// Ticks a flame lasts before it goes out, unless it finds more fuel.
const FIRE_LIFE: u8 = 6;
/// Bit of a fire's metadata that records whose fire it is.
const MINE: u8 = 0x80;
/// Ire per wild block an arsonist's fire consumes. Small per block —
/// a fire that runs through a forest charges by the acre, not by the
/// leaf, and it is the total that strains a country's heart.
const ARSON_IRE: f32 = 0.35;
/// Days of regrowth a natural burn leaves in the ground it cleared.
const WILD_BLOOM: f32 = 0.6;
/// A tended burn gives back a little: stubble is worth turning in.
const STUBBLE_BLOOM: f32 = 0.15;

impl World {
    pub(super) fn schedule_fire_at(&mut self, pos: BlockPos) {
        if self.fire_queued.insert(pos) {
            self.fire_queue.push_back(pos);
        }
    }

    /// Strike a light. `mine` is the whole distinction: true when a
    /// player's tool did it, false when the sky or a mountain did.
    /// Returns whether anything caught.
    #[cfg(test)]
    pub fn light_fire(&mut self, x: i32, y: i32, z: i32, mine: bool) -> bool {
        BlockPos::of_world(x, y, z).is_some_and(|pos| self.light_fire_at(pos, mine))
    }

    pub fn light_fire_at(&mut self, pos: BlockPos, mine: bool) -> bool {
        let Some(fire) = self.reg.block_id("base:fire") else {
            return false;
        };
        if !self.reg.is_replaceable(self.get_block_at(pos)) {
            return false;
        }
        // The wild does not set foot on worked ground, so its fire
        // cannot start there either.
        if !mine && self.player_touched.contains(&pos.chunk()) {
            return false;
        }
        let meta = FIRE_LIFE | if mine { MINE } else { 0 };
        self.set_block_meta_at(pos, fire, meta);
        self.schedule_fire_at(pos);
        true
    }

    /// Is there anything here worth burning?
    fn fuel_at(&self, pos: BlockPos) -> u8 {
        self.reg.block(self.get_block_at(pos)).burns
    }

    /// What a burned cell leaves behind, and what the ledger owes for
    /// it. Ground itself chars; everything standing on it is gone.
    fn consume(&mut self, pos: BlockPos, mine: bool) {
        let b = self.get_block_at(pos);
        let name = self.reg.block(b).name.clone();
        let worked = self.player_touched.contains(&pos.chunk());
        let crop = self.reg.block(b).crop_family != 0;
        // Grass burns down to charred earth rather than to nothing —
        // the same soil the wild's lightning has always left, which
        // tills into the richest ground in the game.
        let leaves = (name == "base:grass")
            .then(|| self.reg.block_id("base:charred_soil"))
            .flatten();
        match leaves {
            Some(ch) => self.set_block_at(pos, ch),
            None => self.set_block_at(pos, AIR),
        }
        if mine {
            // Your fire, your ground, your crop: that is husbandry and
            // the wild has no opinion about it. Anything else you burn
            // is taken, and taken things are never paid back in bloom.
            if worked && crop {
                self.add_bloom_at_surface(pos.surface(), STUBBLE_BLOOM);
            } else {
                self.add_ire_at_surface(pos.surface(), ARSON_IRE);
            }
        } else {
            self.add_bloom_at_surface(pos.surface(), WILD_BLOOM);
        }
    }

    /// Something hot stands here: light whatever it is touching. The
    /// hand on it is read from the ground the heat is standing on,
    /// which is what closes the lava-channel hole in "the tool tells"
    /// — you cannot dig a race into a forest without working the
    /// ground you dug it through.
    pub(super) fn ignite_around_at(&mut self, pos: BlockPos) -> bool {
        let mine = self.player_touched.contains(&pos.chunk());
        let mut lit = false;
        for direction in [
            Direction6::East,
            Direction6::West,
            Direction6::North,
            Direction6::South,
            Direction6::Up,
        ] {
            let Some(fuel) = step6(pos, direction).map(|step| step.pos) else {
                continue;
            };
            if self.fuel_at(fuel) == 0 {
                continue;
            }
            // Stand the flame in the air above the fuel it found.
            if step6(fuel, Direction6::Up).is_some_and(|step| self.light_fire_at(step.pos, mine)) {
                lit = true;
            }
        }
        lit
    }

    /// Advance every burning cell. Fire moves by eating: it consumes a
    /// neighbouring fuel block and stands up in the cell it emptied,
    /// so the flame front is always touching what feeds it.
    pub fn tick_fire(&mut self, budget: usize, rng: &mut u32) -> bool {
        let mut changed = false;
        // Only what was already burning when this tick began. A live
        // flame re-queues itself, and draining the queue would pop it
        // straight back off — the whole fire would run from strike to
        // ash inside one call, which is both wrong and invisible.
        let due = self.fire_queue.len().min(budget);
        for _ in 0..due {
            let Some(pos) = self.fire_queue.pop_front() else {
                break;
            };
            self.fire_queued.remove(&pos);
            let here = self.get_block_at(pos);
            if self.reg.block(here).name != "base:fire" {
                continue;
            }
            let meta = self.get_meta_at(pos);
            let mine = meta & MINE != 0;
            let life = meta & !MINE;
            let weather = self.weather_at_surface(pos.surface());
            let exposed = self.light_at_pos(pos).1 == 15;
            if exposed
                && weather.precipitation == crate::planet_atlas::PrecipitationForm::Rain
                && (weather.kind == crate::planet_atlas::LocalWeather::Storm || life <= 2)
            {
                self.set_block_at(pos, AIR);
                changed = true;
                continue;
            }
            let wind_speed = weather.wind[0].hypot(weather.wind[1]);
            let damp_penalty = if exposed && weather.kind.precipitating() {
                3
            } else {
                0
            };

            // Reach for fuel. Sides and below first, then up: fire
            // climbs, but it takes the near thing first.
            let mut lit = false;
            for direction in [
                Direction6::East,
                Direction6::West,
                Direction6::North,
                Direction6::South,
                Direction6::Down,
                Direction6::Up,
            ] {
                let Some(fuel) = step6(pos, direction).map(|step| step.pos) else {
                    continue;
                };
                let burns = self.fuel_at(fuel);
                if burns == 0 {
                    continue;
                }
                // The invariant, kept: the wild's fire will not cross
                // onto ground a player has worked. Yours will, and
                // that includes your own walls.
                if !mine && self.player_touched.contains(&fuel.chunk()) {
                    continue;
                }
                *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                let spread_threshold =
                    (u32::from(burns) + wind_speed.round() as u32).saturating_sub(damp_penalty);
                if (*rng >> 16) % 10 >= spread_threshold {
                    continue; // damp today
                }
                self.consume(fuel, mine);
                // Stand up in the cell it emptied — unless the burn
                // left charred ground there, in which case the flame
                // goes over it.
                let cell = if self.reg.is_air(self.get_block_at(fuel)) {
                    Some(fuel)
                } else {
                    step6(fuel, Direction6::Up).map(|step| step.pos)
                };
                if cell.is_some_and(|cell| self.light_fire_at(cell, mine)) {
                    lit = true;
                }
                changed = true;
                break;
            }

            // Burning down. A flame that found fuel this tick keeps
            // its strength; one that found none is going out.
            let left = if lit {
                FIRE_LIFE
            } else {
                life.saturating_sub(1)
            };
            if left == 0 {
                self.set_block_at(pos, AIR);
                // Scorch what it stood on, so a burn leaves a mark on
                // the map and not just a gap in the trees.
                if let Some(below) = step6(pos, Direction6::Down).map(|step| step.pos)
                    && self.reg.block(self.get_block_at(below)).name == "base:grass"
                    && let Some(ch) = self.reg.block_id("base:charred_soil")
                {
                    self.set_block_at(below, ch);
                }
                changed = true;
            } else {
                self.set_block_meta_at(pos, here, left | if mine { MINE } else { 0 });
                self.schedule_fire_at(pos);
            }
        }
        changed
    }
}
