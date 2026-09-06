//! Seasons, weather locality, ire, offerings, and renewable growth.
use crate::world::World;

impl World {
    /// Industrial response gradient (capability E12): running machines
    /// feed regional ire alongside extraction — the valley feels a
    /// bloomery's smoke as surely as the mine that fed it. Charged once
    /// per second per lit fire machine; gated behind `ire` and
    /// `industrial_ire` so modes repoint it off cleanly.
    pub const INDUSTRIAL_IRE_PER_SEC: f32 = 0.01;

    /// One-time ire for raising an industrial building (capability E12).
    pub const INDUSTRIAL_BUILDING_IRE: f32 = 0.5;
}

mod clock_access;
mod depots;
mod industrial_ire;
mod ire_tick;
mod lightning;
mod observations;
mod offerings;
mod reciprocity;
mod regional_ire;
mod renewable_growth;
mod seasons;
mod water_reconciliation;
mod weather;
