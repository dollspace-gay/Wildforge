//! Catalogue discovery transaction coordination.

use crate::discovery::DiscoveryError;
use crate::discovery::ObservationSummary;
use crate::world::World;

impl World {
    pub fn discovery_summaries(
        &self,
        holder_id: u64,
        reveal_locations: bool,
    ) -> Result<Vec<ObservationSummary>, DiscoveryError> {
        self.discovery_state
            .as_ref()
            .ok_or_else(|| DiscoveryError::Corrupt("world has no discovery authority".into()))?
            .summaries(holder_id, reveal_locations, self.reg.content_hash)
    }

    pub fn discovery_library_index(
        &self,
        holder_id: u64,
        reveal_locations: bool,
    ) -> Result<crate::discovery::LibraryIndex, DiscoveryError> {
        self.discovery_state
            .as_ref()
            .ok_or_else(|| DiscoveryError::Corrupt("world has no discovery authority".into()))?
            .library_index(holder_id, reveal_locations, self.reg.content_hash)
    }

    pub fn copy_discovery_record(
        &mut self,
        source_holder: u64,
        record: u64,
        destination_holder: u64,
        include_location: bool,
    ) -> Result<u64, DiscoveryError> {
        self.discovery_state
            .as_mut()
            .ok_or_else(|| DiscoveryError::Corrupt("world has no discovery authority".into()))?
            .copy_record(source_holder, record, destination_holder, include_location)
    }
}
