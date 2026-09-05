//! Shared replica-domain updates; presentation consumes the remaining messages.

use crate::net::S2C;
use crate::world::ReplicationTarget;

pub(super) fn apply_world(
    message: S2C,
    world: &mut impl ReplicationTarget,
    time_of_day: &mut f32,
    receiving: bool,
) -> Option<S2C> {
    let shared = matches!(&message,
        S2C::TimeIre { .. } | S2C::WeatherCells { .. } | S2C::ArcaneCue { .. }
        | S2C::ArcaneItems { .. } | S2C::SignText { .. } | S2C::SwitchState { .. }
    );
    if shared && !receiving {
        return None;
    }
    match message {
            S2C::TimeIre { time, ire, day } => {
                *time_of_day = time;
                world.receive_clock(ire, day);
            }
            S2C::WeatherCells { side, cells } => {
                world.observations_mut().set_weather(side, cells);
            }
            S2C::ArcaneCue {
                bands,
                dominant,
                ecology,
            } => {
                world.observations_mut().set_arcane_cue(bands, dominant, ecology);
            }
            S2C::ArcaneItems {
                reset,
                charges,
                implements,
                apparatus,
            } => {
                if reset {
                    world.observations_mut().clear_items();
                }
                world.observations_mut().extend_charges(charges);
                world.observations_mut().extend_implements(implements);
                world.observations_mut().extend_apparatus(apparatus);
            }
            S2C::SignText { pos, lines } => {
                world.receive_block_entity(
                    pos,
                    crate::world::BlockEntity::Sign(crate::world::SignState { lines }),
                );
            }
            S2C::SwitchState { pos, selected } => {
                let selected = match selected & 3 {
                    0 => crate::planet::Direction4::East,
                    1 => crate::planet::Direction4::North,
                    2 => crate::planet::Direction4::West,
                    _ => crate::planet::Direction4::South,
                };
                world.receive_block_entity(
                    pos,
                    crate::world::BlockEntity::Switch(crate::world::SwitchState { selected }),
                );
            }
        other => return Some(other),
    }
    None
}
