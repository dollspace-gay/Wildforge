//! Bounded FIFO queues with host-entry promotion and preemption.

use std::collections::{HashSet, VecDeque};
use std::io;
use std::sync::Arc;

use crate::chunk::ChunkPos;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Priority {
    Ordinary,
    Entry,
}

#[derive(Default)]
pub(super) struct WorkQueue {
    entry: VecDeque<ChunkPos>,
    ordinary: VecDeque<ChunkPos>,
    pending: HashSet<ChunkPos>,
    stopped: bool,
    failure: Option<Arc<io::Error>>,
}

impl WorkQueue {
    /// Returns whether a worker should be notified of new/promoted work.
    pub(super) fn request(&mut self, position: ChunkPos, priority: Priority, limit: usize) -> bool {
        if self.stopped {
            return false;
        }
        if self.pending.contains(&position) {
            if priority == Priority::Entry
                && let Some(index) = self.ordinary.iter().position(|queued| *queued == position)
            {
                self.ordinary.remove(index);
                self.entry.push_back(position);
                return true;
            }
            return false;
        }
        if self.pending.len() >= limit {
            if priority == Priority::Ordinary {
                return false;
            }
            let Some(displaced) = self.ordinary.pop_back() else {
                return false;
            };
            self.pending.remove(&displaced);
        }
        self.pending.insert(position);
        match priority {
            Priority::Entry => self.entry.push_back(position),
            Priority::Ordinary => self.ordinary.push_back(position),
        }
        true
    }

    pub(super) fn take(&mut self) -> Option<ChunkPos> {
        if self.stopped {
            return None;
        }
        self.entry.pop_front().or_else(|| self.ordinary.pop_front())
    }

    pub(super) fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub(super) fn complete(&mut self, position: ChunkPos) {
        self.pending.remove(&position);
    }

    pub(super) fn cancel_queued(&mut self, wanted: &impl Fn(ChunkPos) -> bool) {
        for queue in [&mut self.entry, &mut self.ordinary] {
            queue.retain(|position| {
                if wanted(*position) {
                    true
                } else {
                    self.pending.remove(position);
                    false
                }
            });
        }
    }

    pub(super) fn stop(&mut self) {
        self.stopped = true;
        self.entry.clear();
        self.ordinary.clear();
        self.pending.clear();
    }

    pub(super) fn is_stopped(&self) -> bool {
        self.stopped
    }

    pub(super) fn fail(&mut self, error: io::Error) {
        self.stop();
        self.failure.get_or_insert_with(|| Arc::new(error));
    }

    pub(super) fn failure(&self) -> Option<Arc<io::Error>> {
        self.failure.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{Priority, WorkQueue};
    use crate::chunk::ChunkPos;
    use crate::planet::Face;

    fn pos(index: u16) -> ChunkPos {
        ChunkPos::new(Face::PosZ, index, 0).unwrap()
    }

    #[test]
    fn ordinary_work_is_fifo_and_deduplicated_until_completion() {
        let mut queue = WorkQueue::default();
        assert!(queue.request(pos(1), Priority::Ordinary, 2));
        assert!(!queue.request(pos(1), Priority::Ordinary, 2));
        assert!(queue.request(pos(2), Priority::Ordinary, 2));
        assert_eq!(queue.take(), Some(pos(1)));
        assert!(!queue.request(pos(1), Priority::Ordinary, 2));
        assert_eq!(queue.take(), Some(pos(2)));
        assert_eq!(queue.take(), None);
        queue.complete(pos(1));
        assert!(queue.request(pos(1), Priority::Ordinary, 2));
    }

    #[test]
    fn entry_promotes_queued_terrain_without_a_second_request() {
        let mut queue = WorkQueue::default();
        queue.request(pos(1), Priority::Ordinary, 2);
        queue.request(pos(2), Priority::Ordinary, 2);
        assert!(queue.request(pos(2), Priority::Entry, 2));
        assert_eq!(queue.take(), Some(pos(2)));
        assert_eq!(queue.take(), Some(pos(1)));
        assert_eq!(queue.take(), None);
    }

    #[test]
    fn entry_preempts_ordinary_backlog_but_never_running_work() {
        let mut queue = WorkQueue::default();
        queue.request(pos(1), Priority::Ordinary, 2);
        queue.request(pos(2), Priority::Ordinary, 2);
        assert_eq!(queue.take(), Some(pos(1)));
        assert!(!queue.request(pos(3), Priority::Ordinary, 2));
        assert!(queue.request(pos(3), Priority::Entry, 2));
        assert_eq!(queue.take(), Some(pos(3)));
        assert!(!queue.request(pos(4), Priority::Entry, 2));
        queue.complete(pos(1));
        assert!(queue.request(pos(2), Priority::Ordinary, 2));
    }

    #[test]
    fn cancellation_releases_queued_capacity_without_losing_running_work() {
        let mut queue = WorkQueue::default();
        queue.request(pos(1), Priority::Ordinary, 2);
        queue.request(pos(2), Priority::Entry, 2);
        assert_eq!(queue.take(), Some(pos(2)));
        queue.cancel_queued(&|_| false);
        assert_eq!(queue.take(), None);
        assert!(!queue.request(pos(2), Priority::Ordinary, 2));
        assert!(queue.request(pos(3), Priority::Ordinary, 2));
    }

    #[test]
    fn stopped_queues_reject_new_work() {
        let mut queue = WorkQueue::default();
        queue.request(pos(1), Priority::Entry, 3);
        queue.request(pos(2), Priority::Ordinary, 3);
        queue.request(pos(3), Priority::Ordinary, 3);
        assert_eq!(queue.take(), Some(pos(1)));
        queue.stop();
        assert!(queue.is_stopped());
        assert_eq!(queue.take(), None, "queued terrain is cancelled");
        assert_eq!(queue.pending_count(), 0);
        assert!(!queue.request(pos(1), Priority::Entry, 2));
    }
}
