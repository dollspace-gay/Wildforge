//! Shared guest protocol state, independent of presentation and agent policy.

mod admission;
mod assembly;
mod palette;
mod snapshots;

pub(crate) use admission::{Admission, PresentationRequirement};
#[cfg(test)]
pub(crate) use assembly::SnapshotAssembler;
pub(crate) use palette::ContentMap;
pub(crate) use snapshots::Snapshots;
