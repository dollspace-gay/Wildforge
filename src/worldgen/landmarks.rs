//! Pure province landmark forms, shared by generation and living-heart simulation.

/// Which shape a country's heart wears. Twelve countries, twelve
/// spirits: wooded ground grows a bole, dry ground keeps a spring that
/// should not be there, and cold or open ground raises a stone. The
/// forms are generated alongside the blocks and tiles from
/// tools/heart_table.py — that table is the source, not this match.
pub fn heart_form(biome: crate::worldgen::Biome) -> &'static str {
    use crate::worldgen::Biome as B;
    match biome {
        B::Forest => "base:heart_forest",
        B::Taiga => "base:heart_taiga",
        B::Jungle => "base:heart_jungle",
        B::Swamp => "base:heart_swamp",
        B::Desert => "base:heart_desert",
        B::Badlands => "base:heart_badlands",
        B::Savanna => "base:heart_savanna",
        B::Scrubland => "base:heart_scrubland",
        B::Plains => "base:heart_plains",
        B::Tundra => "base:heart_tundra",
        B::Arctic => "base:heart_arctic",
        B::Mountains => "base:heart_mountains",
        // Not a province culture; no country is ever labelled with it.
        B::Ocean => "base:heart_plains",
    }
}

/// How tall the site stands. A bole is a landmark you can see across a
/// valley; a spring is a thing you nearly walk past.
pub fn heart_height(form: &str) -> i32 {
    match form {
        "base:heart_forest" => 5,
        "base:heart_taiga" => 5,
        "base:heart_jungle" => 5,
        "base:heart_swamp" => 5,
        "base:heart_desert" => 1,
        "base:heart_badlands" => 1,
        "base:heart_savanna" => 1,
        "base:heart_scrubland" => 1,
        "base:heart_plains" => 3,
        "base:heart_tundra" => 3,
        "base:heart_arctic" => 3,
        "base:heart_mountains" => 3,
        _ => 1,
    }
}

