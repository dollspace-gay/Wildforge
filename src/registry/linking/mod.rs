//! Pure interpretation before registry publication.

mod material;
pub(super) use material::{inferred_material_class, salvage_def};
mod observation;
pub(super) use observation::{observation_def, discovery_item_def, discovery_fixture_def};
mod magic;
pub(super) use magic::{arcane_def, arcane_ecology_def};
mod pending;
mod register;
pub(super) use pending::{PendingAnimal, PendingContent, PendingNpc};
pub(super) use register::register;
