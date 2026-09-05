//! Shared guest protocol state, independent of presentation and agent policy.

mod assembly;
mod palette;
mod snapshots;

#[cfg(test)]
pub(crate) use assembly::SnapshotAssembler;
pub(crate) use palette::ContentMap;
pub(crate) use snapshots::Snapshots;
