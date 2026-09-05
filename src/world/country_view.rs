//! Country guidance derived from immutable geography and visible heart state.

use crate::planet::{EntityPos, SurfacePos, geodesic_distance, great_circle_bearing};
use crate::registry::Registry;
use crate::worldgen::{Geography, ProvinceKey};
use super::{Heart, heart_form, heart_block_name};

pub(super) fn compass_octant(radians_clockwise_from_north: f64) -> &'static str {
    let index =
        ((radians_clockwise_from_north.to_degrees() + 22.5).rem_euclid(360.0) / 45.0) as usize;
    [
        "north",
        "northeast",
        "east",
        "southeast",
        "south",
        "southwest",
        "west",
        "northwest",
    ][index]
}

pub(super) fn seed_bearing(geography: &Geography, from: EntityPos, known_dead: impl Fn(ProvinceKey) -> bool) -> String {

        let from_surface = SurfacePos::new(
            from.face(),
            from.u().floor() as u16,
            from.v().floor() as u16,
        )
        .expect("a canonical entity has a canonical surface cell");
        let mut best: Option<(f64, SurfacePos, bool)> = None;
        for key in geography.province_keys_near(from_surface, 5_000.0) {
            let site = geography.province_center_at(key);
            let ancient = geography.province_at(site).biome == crate::worldgen::Biome::Badlands;
            let known_dead = known_dead(key);
            if !ancient && !known_dead {
                continue;
            }
            let d = geodesic_distance(from_surface.center(), site.center());
            if best.is_none_or(|(b, _, _)| d < b) {
                best = Some((d, site, ancient));
            }
        }
        let Some((d, site, ancient)) = best else {
            return "It stirs, and finds nowhere that needs it.".into();
        };
        if d < 12.0 {
            return "It strains in your hand. The ground it wants is here.".into();
        }
        let dir = great_circle_bearing(from_surface.center(), site.center())
            .map(compass_octant)
            .unwrap_or("somewhere beyond a stable bearing");
        let far = if ancient {
            "a country that went out long ago"
        } else {
            "a country you watched go out"
        };
        format!("It leans {dir}, about {} blocks. {far}.", d.round() as i32)
}

pub(super) fn heart_report(geography: &Geography, registry: &Registry, pos: SurfacePos, heart: Option<Heart>) -> String {

        let Some(h) = heart else {
            return "The heart of this country lies beyond your maps.".into();
        };
        let dist = geodesic_distance(pos.center(), h.pos.surface().center()).round() as i32;
        let dir = great_circle_bearing(pos.center(), h.pos.surface().center())
            .map(compass_octant)
            .unwrap_or("here");
        let state = match h.stage {
            2 if h.strain > 4.0 => "It is uneasy.",
            2 => "It is well.",
            1 => "It is FAILING.",
            // A scar is older than the reading. The badlands lost their
            // spirit before anyone alive walked there, and a cairn that
            // says so is the first thread of the whole story.
            _ if geography.province_at(h.pos.surface()).biome == crate::worldgen::Biome::Badlands => {
                "It died long before these stones were cut."
            }
            _ => "It is dead.",
        };
        // Name the shape. They are no longer all alike, so a reader is
        // looking for a particular thing rather than "a heart".
        let form = heart_form(geography.heart_biome_at(h.pos.surface()));
        let what = registry
            .block_id(&heart_block_name(form, h.stage))
            .map(|b| registry.block(b).label.clone())
            .unwrap_or_else(|| "heart".into());
        if dist <= 8 {
            format!("{what}, here. {state}")
        } else {
            format!("{what}, about {dist} blocks {dir}. {state}")
        }
}
