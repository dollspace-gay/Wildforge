//! Offerings calendar transaction coordination.

use crate::world::BlockEntity;
use crate::inventory::ItemStack;
use crate::world::RegionCell;
use crate::world::World;

impl World {
    /// The season's appetite: what the wild wants brought this time
    /// of year, and how it says so. Fixed to the calendar — players
    /// learn the year, not a dice roll.
    #[cfg(test)]
    pub fn season_want(&self) -> (usize, &'static str) {
        Self::want_for_season(self.season())
    }

    pub fn season_want_at_surface(&self, pos: crate::planet::SurfacePos) -> (usize, &'static str) {
        Self::want_for_season(self.season_at_surface(pos))
    }

    pub(super) fn want_for_season(season: usize) -> (usize, &'static str) {
        crate::world::calendar_view::seasonal_want(season)
    }

    /// Does a stack satisfy the season's want?
    pub fn satisfies_want(&self, want: usize, s: &ItemStack) -> bool {
        let d = self.reg.item(s.item);
        match want {
            // Spring: things that grow — saplings and plantables.
            0 => {
                d.name.ends_with("_sapling")
                    || d.places
                        .is_some_and(|b| self.reg.block(b).crop_next.is_some())
            }
            // Summer: water, carried by hand.
            1 => d.name == "base:bucket_water",
            // Autumn: the harvest's produce (plant nutrition).
            2 => d
                .food
                .as_ref()
                .is_some_and(|f| f.nutrition[..4].iter().any(|&n| n > 0.0)),
            // Winter: anything that feeds.
            _ => d.food.is_some(),
        }
    }

    /// What the wild values: its own materials most, then life given.
    pub fn offering_value(&self, s: &ItemStack) -> f32 {
        let d = self.reg.item(s.item);
        let per = if d.name == "base:diamond" {
            // The wild prizes what the deep earth surrenders rarest.
            6.0
        } else if [
            "base:amethyst_shard",
            "base:gold_ingot",
            "base:silver_ingot",
        ]
        .contains(&d.name.as_str())
        {
            3.0
        } else if [
            "base:heartwood",
            "base:living_wood",
            "base:thorn_fiber",
            "base:dryad_heartwood",
            "base:ember",
            "base:frost_shard",
        ]
        .contains(&d.name.as_str())
        {
            2.0
        } else if d.name.ends_with("_sapling")
            || d.name.contains("raw_")
            || d.name.contains("cooked_")
        {
            1.0
        } else if let Some(f) = &d.food {
            f.hunger * 0.25
        } else {
            0.25
        };
        per * s.count as f32
    }

    /// Dawn: the wild takes everything left on offering stones. Items are
    /// consumed regardless; the refund is capped at 10 per dawn.
    pub fn accept_offerings(&mut self) -> f32 {
        // The ire cells whose country has no spirit left to hear.
        let dead_country: std::collections::HashSet<RegionCell> = self
            .hearts
            .values()
            .filter(|h| h.stage == 0)
            .map(|h| RegionCell::from_surface(h.pos.surface()))
            .collect();
        let mut taken: Vec<(crate::planet::BlockPos, RegionCell, usize, ItemStack)> = Vec::new();
        for (&pos, e) in self.installations.iter_mut() {
            let BlockEntity::Offering(o) = e else {
                continue;
            };
            let cell = RegionCell::from_surface(pos.surface());
            // In a country whose heart is dead the stone accepts
            // nothing. Not refused — unreceived. Nobody is home.
            if dead_country.contains(&cell) {
                continue;
            }
            let latitude = crate::planet_atlas::latitude_longitude(crate::planet::surface_to_unit(
                pos.surface().center(),
            ))
            .0;
            let season = if self.calendar_state.long_winter() {
                3
            } else {
                crate::planet_atlas::local_season(self.calendar_state.day(), latitude)
            };
            let (want, _) = Self::want_for_season(season);
            for slot in o.slots.iter_mut() {
                if let Some(s) = slot.take() {
                    taken.push((pos, cell, want, s));
                }
            }
        }
        if taken.is_empty() {
            return 0.0;
        }
        // Charged gifts remain part of the finite world: the listening
        // country's heart takes their exact mixture. If ledger state is not
        // available or rejects the transfer, give the physical item back as
        // a drop instead of allowing the offering path to destroy Current.
        let mut accepted = Vec::with_capacity(taken.len());
        for (pos, cell, want, stack) in taken {
            if stack.arcane_id != 0 {
                let destination = self.planet_atlas.as_ref().map(|atlas| {
                    atlas
                        .country_at(pos.surface())
                        .map(|country| crate::arcane::ArcaneOwner::Heart(country.id))
                        .unwrap_or_else(|| {
                            crate::arcane::ArcaneOwner::Ambient(atlas.atlas_pos(pos.surface()))
                        })
                });
                let transfer = destination
                    .and_then(|destination| {
                        self.arcane_ledger.as_mut().map(|ledger| {
                            ledger.move_all_item(
                                stack.arcane_id,
                                destination,
                                "charged offering accepted",
                            )
                        })
                    })
                    .transpose();
                if !matches!(transfer, Ok(Some(_))) {
                    if let Err(error) = transfer {
                        eprintln!("arcane: charged offering rejected: {error}");
                    } else {
                        eprintln!("arcane: charged offering rejected: ledger unavailable");
                    }
                    self.push_drop_at(pos, stack);
                    continue;
                }
            }
            accepted.push((pos, cell, want, stack));
        }
        if accepted.is_empty() {
            return 0.0;
        }
        // The season's want counts double — a bonus for listening,
        // never a penalty — and every stone credits its own valley.
        let mut value = 0.0f32;
        for (_, cell, want, s) in &accepted {
            let mut v = self.offering_value(s);
            if self.satisfies_want(*want, s) {
                v *= 2.0;
            }
            value += v;
            self.charge_cell(*cell, -v.min(6.0));
        }
        if let Err(error) =
            self.record_consumed_stacks(accepted.iter().map(|(_, _, _, stack)| *stack))
        {
            eprintln!("materials: offering consumption accounting failed: {error}");
        }
        let refund = value.min(10.0);
        self.add_ire(-refund);
        refund
    }
}
