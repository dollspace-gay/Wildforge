//! Bounded reconstruction of one unreliable snapshot stream.

use crate::net::Snapshot;

/// Rebuilds multi-part snapshots on the receiving side.
///
/// Holds at most one incomplete generation. A newer `seq` abandons an older
/// incomplete one rather than waiting for it — this is a latest-wins stream,
/// and a generation that lost a datagram is worth less than the one behind it.
#[derive(Debug)]
pub(crate) struct SnapshotAssembler<T> {
    generation: Option<Generation<T>>,
}

#[derive(Debug)]
enum Generation<T> {
    Collecting {
        sequence: u32,
        parts: Vec<Option<Vec<T>>>,
    },
    Applied(u32),
}

impl<T> Generation<T> {
    fn sequence(&self) -> u32 {
        match self {
            Self::Collecting { sequence, .. } | Self::Applied(sequence) => *sequence,
        }
    }
}

impl<T> Default for SnapshotAssembler<T> {
    fn default() -> Self {
        Self { generation: None }
    }
}

impl<T> SnapshotAssembler<T> {
    /// Feed one part; yields the whole generation once its last part lands.
    pub(crate) fn accept(&mut self, snap: Snapshot<T>) -> Option<Vec<T>> {
        // Invalid geometry must not retire a valid pending generation.
        if snap.parts == 0 || snap.part >= snap.parts {
            return None;
        }
        let fresh = match &self.generation {
            None => true,
            Some(generation) if generation.sequence() == snap.seq => false,
            Some(generation) => {
                // The host increments a wrapping u32. Forward distances below
                // half the sequence space are newer; the ambiguous half is not.
                if snap.seq.wrapping_sub(generation.sequence()) >= (1 << 31) {
                    return None;
                }
                true
            }
        };
        if !fresh && matches!(self.generation, Some(Generation::Applied(_))) {
            return None;
        }
        // Preserve the allocation-free common path, after sequence validation.
        if snap.parts == 1 {
            if !fresh {
                return None; // a fragmented generation cannot change its layout
            }
            self.generation = Some(Generation::Applied(snap.seq));
            return Some(snap.items);
        }
        if fresh {
            self.generation = Some(Generation::Collecting {
                sequence: snap.seq,
                parts: (0..snap.parts).map(|_| None).collect(),
            });
        }
        let Some(Generation::Collecting { parts, .. }) = &mut self.generation else {
            return None;
        };
        if parts.len() != usize::from(snap.parts) {
            return None;
        }
        let slot = parts.get_mut(usize::from(snap.part))?;
        if slot.is_some() {
            return None;
        }
        *slot = Some(snap.items);
        if !parts.iter().all(Option::is_some) {
            return None;
        }
        let whole = parts
            .iter_mut()
            .filter_map(Option::take)
            .flatten()
            .collect();
        self.generation = Some(Generation::Applied(snap.seq));
        Some(whole)
    }
}

#[cfg(test)]
#[path = "assembly_tests.rs"]
mod tests;
