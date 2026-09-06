//! Transport lifetime and outgoing requests belong to the shared guest session.

use std::sync::Arc;
use std::time::Instant;

use super::{GuestSession, PresentationRequirement};
use crate::net::{C2S, Client, S2C};
use crate::registry::Registry;

impl GuestSession {
    pub(crate) fn with_client(
        registry: Arc<Registry>,
        requirement: PresentationRequirement,
        now: Instant,
        client: Client,
    ) -> Self {
        let mut session = Self::new(registry, requirement, now);
        session.connection = Some(client);
        session
    }

    pub(crate) fn is_connected(&self) -> bool {
        self.connection.as_ref().is_some_and(Client::is_connected)
    }

    pub(crate) fn poll(&mut self) -> Vec<S2C> {
        self.connection.as_mut().map_or_else(Vec::new, Client::poll)
    }

    pub(crate) fn send(&self, message: &C2S) {
        if let Some(client) = &self.connection {
            client.send(message);
        }
    }

    pub(crate) fn send_datagram(&self, message: &C2S) {
        if let Some(client) = &self.connection {
            client.send_datagram(message);
        }
    }
}
