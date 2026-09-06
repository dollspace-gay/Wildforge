//! Chunk loading/generation, structures, and loot adoption.
use crate::world::World;



/// How far above the surface a heart's own column is still searched
/// when the ledger goes looking for it. An edifice can bury the site
/// under courses of stone or lift a canopy over it; the spirit is
/// still down there.
const EDIFICE_CLEARANCE: i32 = 48;


mod adoption;
mod ecology_reconciliation;
mod dross_reconciliation;
mod material_retrogen;
mod water_commit;
mod structures;
mod loot;


impl World {
    #[cfg(test)]
    pub(crate) fn refresh_arcane_ecology_for_test(&mut self) {
        self.refresh_loaded_arcane_ecology();
    }

    #[cfg(test)]
    pub(crate) fn refresh_dross_scars_for_test(&mut self) {
        self.refresh_loaded_dross_scars();
    }
}
