//! Discovery graphical guest adapter.

use super::RemoteFlow;
use crate::game::Game;
use crate::net;

impl Game {
    pub(in crate::game) fn remote_discovery_message(&mut self, message: net::S2C) -> RemoteFlow {
        match message {
            net::S2C::DiscoveryReport(record) => {
                self.present_discovery_record(&record);
            }
            net::S2C::DiscoveryRecords {
                holder,
                records,
                capacity,
            } => self.receive_discovery_catalogue(holder, records, capacity),
            net::S2C::KnowledgeText {
                instance_id: _,
                text,
            } => self.toast(text),
            _ => {}
        }
        RemoteFlow::Continue
    }
}
