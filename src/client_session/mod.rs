//! Shared guest protocol state, independent of presentation and agent policy.

mod admission;
mod assembly;
mod palette;
mod replica;
mod session;
mod terrain;

pub(crate) use admission::PresentationRequirement;
#[cfg(test)]
pub(crate) use assembly::SnapshotAssembler;
pub(crate) use palette::ContentMap;
pub(crate) use session::GuestSession;
