//! Survival ticking, item pickup, and player-facing status messages.

pub(super) fn dross_warning_text(band: u8) -> (&'static str, &'static str) {
    match band {
        1 => ("TRACE", "GLASS HAZE"),
        2 => ("STRAINED", "TWO-PULSE HUM"),
        3 => ("SEEP", "BRANCHING SIGN"),
        4 => ("SCAR", "BROKEN RING"),
        5 => ("BREACH RISK", "REPEATING SHEAR"),
        _ => ("CLEAR", "EVEN FIELD"),
    }
}

mod feedback;
mod items;
mod nutrition;
mod survival;
