//! Archaeology machines transaction coordination.

use crate::inventory::ItemStack;
use crate::planet::BlockPos;
use crate::world::World;

impl World {
    /// Archaeology: sweep a remnant block — it yields its artifact once
    /// and becomes plain. Returns what was found.
    pub fn brush_block_at(&mut self, pos: BlockPos, rng: &mut u32) -> Option<ItemStack> {
        let b = self.get_block_at(pos);
        let (table, becomes) = self.reg.block(b).brush.clone()?;
        if !self.claim_discovery_recovery(pos) {
            return None;
        }
        let mut items = if table.ends_with(":ruin_artifacts") {
            let item_names = [
                "base:etched_tablet",
                "base:maker_calibration_plate",
                "base:spent_charm_fitting",
                "base:broken_focus",
                "base:sealed_dross_ampoule",
                "base:site_survey_marks",
                "base:failed_containment_fragment",
            ];
            let index = self.mob_hash_at(pos.surface(), 0xd15c_0a11) as usize % item_names.len();
            self.reg
                .item_id(item_names[index])
                .map(|item| vec![ItemStack::new(&self.reg, item, 1)])
                .unwrap_or_default()
        } else {
            self.roll_loot(&table, 1, rng)
        };
        self.set_block_at(pos, becomes);
        let mut found = items.pop();
        if let Some(stack) = &mut found
            && let Err(error) = self.bind_arcane_stack_at(pos, stack, "archaeological recovery")
        {
            eprintln!("arcane: archaeological find could not bind: {error}");
            found = None;
        }
        if let Some(stack) = &mut found
            && let Err(error) = self.bind_discovery_stack_at(pos, stack)
        {
            eprintln!("discovery: archaeological find could not bind: {error}");
            found = None;
        }
        if let (Some(stack), Some(ledger)) = (found, &mut self.material_ledger)
            && let Err(error) =
                ledger.record_external_stack(&self.reg, stack, "pre-genesis archaeology")
        {
            eprintln!("materials: archaeology accounting failed: {error}");
        }
        found
    }

    /// Natural, non-interactive ground can be sifted for the coarse regional
    /// salvage pool. The brush does not need (and cannot reveal) the exact
    /// place an item despawned; the finite ledger intentionally remembers
    /// only a bounded 256-block recovery region.
    pub fn can_sift_salvage_at(&self, pos: BlockPos) -> bool {
        crate::world::TerrainRead::can_sift_salvage_at(self, pos)
    }

    /// Recover one usable item at the primitive 75% yield.
    pub fn sift_salvage_at(&mut self, pos: BlockPos) -> std::io::Result<Option<ItemStack>> {
        if !self.can_sift_salvage_at(pos) {
            return Ok(None);
        }
        let reg = self.reg.clone();
        let Some(ledger) = &mut self.material_ledger else {
            return Ok(None);
        };
        ledger.recover_salvage_stack(&reg, crate::materials::SalvageRegion::at(pos), 750)
    }
}
