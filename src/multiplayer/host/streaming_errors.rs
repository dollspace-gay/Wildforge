//! Map retained terrain/worker failures to the existing guest refusal contract.

use super::HostSession;
use std::collections::HashSet;

impl HostSession {
    pub(super) fn refuse_failed_terrain(&mut self) {
        let failures: Vec<_> = self
            .chunk_jobs
            .as_ref()
            .map(|jobs| jobs.failures().cloned().collect())
            .unwrap_or_default();
        for failure in failures {
            let mut affected: HashSet<_> = self
                .pending_guests
                .iter()
                .filter(|(_, pending)| pending.required.contains(&failure.position))
                .map(|(id, _)| *id)
                .collect();
            affected.extend(
                self.guests
                    .iter()
                    .filter(|(_, guest)| {
                        (!guest.entry_ready && guest.entry_required.contains(&failure.position))
                            || (guest.entry_ready
                                && guest.pos.chunk().is_some_and(|center| {
                                    failure.position.distance(center)
                                        <= f64::from(guest.view_dist * 16) + 1.0
                                }))
                    })
                    .map(|(id, _)| *id),
            );
            for id in affected {
                self.refuse_server_error(
                    id,
                    &format!("terrain {:?}", failure.position),
                    &failure.source,
                );
            }
        }
    }

    /// A failed worker pool cannot admit or continue guests on missing terrain.
    pub(super) fn refuse_failed_workers(&mut self) -> bool {
        if let Some(error) = self.chunk_jobs.take_failure_notification() {
            eprintln!("host: terrain preparation stopped: {error}");
        }
        let Some(error) = self.chunk_jobs.failure() else {
            return false;
        };
        let affected: HashSet<_> = self
            .pending_guests
            .keys()
            .chain(self.guests.keys())
            .copied()
            .collect();
        for id in affected {
            self.refuse_server_error(id, "terrain preparation", &error);
        }
        true
    }
}
