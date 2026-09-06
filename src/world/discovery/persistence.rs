//! Persistence discovery transaction coordination.

use crate::planet::BlockPos;
use crate::world::World;

impl World {
    pub fn save_discovery(&mut self) -> std::io::Result<()> {
        self.discovery_state
            .as_mut()
            .map_or(Ok(()), |state| state.save().map_err(std::io::Error::other))
    }

    pub(crate) fn claim_discovery_recovery(&mut self, pos: BlockPos) -> bool {
        let key = format!("brush:{}:{}:{}:{}", pos.face(), pos.u(), pos.y(), pos.v());
        let Some(state) = self.discovery_state.as_mut() else {
            // Atlas-free test/dev fixtures still get the ordinary once-only
            // guarantee from the remnant block transmuting immediately.
            return true;
        };
        if state.site_was_recovered(&key) {
            return false;
        }
        state.mark_site_recovered(key)
    }

    pub(crate) fn discovery_site_installed(&self, key: &str) -> bool {
        self.discovery_state
            .as_ref()
            .is_some_and(|state| state.site_was_installed(key))
    }

    pub(crate) fn mark_discovery_site_installed(&mut self, key: &str) {
        if let Some(state) = self.discovery_state.as_mut() {
            state.mark_site_installed(key);
        }
    }
}
