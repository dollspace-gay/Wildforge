//! Persistent block-entity serialization and world save-directory access.
use std::path::PathBuf;
use crate::world::World;



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

mod save;
mod load;


mod schema;
