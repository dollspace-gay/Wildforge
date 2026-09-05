//! Bounded reconstruction of one unreliable snapshot stream.

use crate::net::Snapshot;

/// Rebuilds multi-part snapshots on the receiving side.
///
/// Holds at most one incomplete generation. A newer `seq` abandons an older
/// incomplete one rather than waiting for it — this is a latest-wins stream,
/// and a generation that lost a datagram is worth less than the one behind it.
#[derive(Debug)]
pub(crate) struct SnapshotAssembler<T> {
    seq: Option<u32>,
    slots: Vec<Option<Vec<T>>>,
}

impl<T> Default for SnapshotAssembler<T> {
    fn default() -> Self {
        SnapshotAssembler {
            seq: None,
            slots: Vec::new(),
        }
    }
}

impl<T> SnapshotAssembler<T> {
    /// Feed one part; yields the whole generation once its last part lands.
    pub(crate) fn accept(&mut self, snap: Snapshot<T>) -> Option<Vec<T>> {
        // The overwhelmingly common case: it all fit in one datagram.
        if snap.parts <= 1 {
            self.seq = Some(snap.seq);
            self.slots.clear();
            return Some(snap.items);
        }
        // A straggler from a generation we have already moved past.
        if self.seq.is_some_and(|seen| snap.seq < seen) {
            return None;
        }
        if self.seq != Some(snap.seq) {
            self.seq = Some(snap.seq);
            self.slots = (0..snap.parts).map(|_| None).collect();
        }
        let slot = self.slots.get_mut(snap.part as usize)?;
        *slot = Some(snap.items);
        if self.slots.iter().all(Option::is_some) {
            let whole = self
                .slots
                .iter_mut()
                .filter_map(Option::take)
                .flatten()
                .collect();
            self.slots.clear();
            return Some(whole);
        }
        None
    }
}
