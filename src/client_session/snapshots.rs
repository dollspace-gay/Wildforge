//! One receiver lifecycle for every independent guest snapshot stream.

use super::assembly::SnapshotAssembler;
use crate::net::{BoltSnap, FallSnap, LooseItemSnap, MobSnap, PlayerSnap, Snapshot};

/// Replaced on Welcome so parts and sequence numbers never cross sessions.
#[derive(Debug, Default)]
pub(crate) struct Snapshots {
    players: SnapshotAssembler<PlayerSnap>,
    mobs: SnapshotAssembler<MobSnap>,
    bolts: SnapshotAssembler<BoltSnap>,
    loose_items: SnapshotAssembler<LooseItemSnap>,
    falling: SnapshotAssembler<FallSnap>,
}

impl Snapshots {
    pub(crate) fn players(&mut self, part: Snapshot<PlayerSnap>) -> Option<Vec<PlayerSnap>> {
        self.players.accept(part)
    }

    pub(crate) fn mobs(&mut self, part: Snapshot<MobSnap>) -> Option<Vec<MobSnap>> {
        self.mobs.accept(part)
    }

    pub(crate) fn bolts(&mut self, part: Snapshot<BoltSnap>) -> Option<Vec<BoltSnap>> {
        self.bolts.accept(part)
    }

    pub(crate) fn loose_items(
        &mut self,
        part: Snapshot<LooseItemSnap>,
    ) -> Option<Vec<LooseItemSnap>> {
        self.loose_items.accept(part)
    }

    pub(crate) fn falling(&mut self, part: Snapshot<FallSnap>) -> Option<Vec<FallSnap>> {
        self.falling.accept(part)
    }
}

#[cfg(test)]
mod tests {
    use super::Snapshots;
    use crate::net::Snapshot;

    #[test]
    fn streams_advance_independently_and_a_new_session_resets_all_of_them() {
        let mut snapshots = Snapshots::default();
        assert!(snapshots.players(Snapshot::whole(90, vec![])).is_some());
        assert!(snapshots.mobs(Snapshot::whole(5, vec![])).is_some());
        assert!(snapshots.bolts(Snapshot::whole(20, vec![])).is_some());
        assert!(snapshots.loose_items(Snapshot::whole(30, vec![])).is_some());
        assert!(snapshots.falling(Snapshot::whole(15, vec![])).is_some());
        snapshots = Snapshots::default();
        assert!(snapshots.players(Snapshot::whole(0, vec![])).is_some());
        assert!(snapshots.mobs(Snapshot::whole(0, vec![])).is_some());
        assert!(snapshots.bolts(Snapshot::whole(0, vec![])).is_some());
        assert!(snapshots.loose_items(Snapshot::whole(0, vec![])).is_some());
        assert!(snapshots.falling(Snapshot::whole(0, vec![])).is_some());
    }
}
