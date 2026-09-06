//! Persistent block-entity serialization and world save-directory access.
use crate::world::World;
use std::path::PathBuf;

impl World {
    pub fn save_dir_for_saving(&self) -> PathBuf {
        self.save_dir.clone()
    }

    #[cfg(test)]
    pub fn save_dir_for_test(&self) -> PathBuf {
        self.save_dir.clone()
    }

    // ---------------- fluids ----------------
}

mod load;
mod save;

mod schema;
