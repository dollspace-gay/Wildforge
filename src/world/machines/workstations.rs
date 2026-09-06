//! Workstations machines transaction coordination.

use crate::world::BlockEntity;
use crate::planet::BlockPos;
use crate::inventory::ItemStack;
use crate::world::World;
use super::STATION_BULK;
use super::station_powered;
use super::worked_table_for;

impl World {
    /// The station kind ("anvil"/"quern"/"millstone"/...) of the block at pos.
    pub(in crate::world) fn station_at(&self, pos: BlockPos) -> Option<String> {
        self.reg.block(self.get_block_at(pos)).interaction.clone()
    }

    /// A vice within three blocks: precision machines refuse to cut
    /// without workholding (the screw's first gift, mechanization
    /// rung 2).
    pub fn vice_near_at(&self, pos: BlockPos) -> bool {
        let Some(v) = self.reg.block_id("base:vice") else {
            return false;
        };
        for dx in -3..=3i32 {
            for dy in -1..=1i32 {
                for dz in -3..=3i32 {
                    if pos
                        .offset(dx, dy, dz)
                        .is_some_and(|at| self.get_block_at(at) == v)
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Rest a workable item on a station. Hand stations take one at a
    /// time; powered stations pile a batch (the millstone's whole
    /// point is grinding sixteen while you're elsewhere). Only items
    /// this station's worked-table accepts may rest.
    pub fn anvil_put_at(&mut self, pos: BlockPos, stack: ItemStack) -> bool {
        let Some(st) = self.station_at(pos) else {
            return false;
        };
        let table = worked_table_for(&st);
        if !self
            .reg
            .worked
            .iter()
            .any(|w| w.input == stack.item && w.station == table)
        {
            return false;
        }
        let e = self.installations
            .entry(pos)
            .or_insert_with(|| BlockEntity::Anvil(Default::default()));
        if let BlockEntity::Anvil(a) = e {
            match &mut a.bloom {
                None => {
                    a.bloom = Some(ItemStack { count: 1, ..stack });
                    a.strikes = 0;
                    return true;
                }
                Some(b)
                    if station_powered(&st) && b.item == stack.item && b.count < STATION_BULK =>
                {
                    b.count += 1;
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    pub fn anvil_take_at(&mut self, pos: BlockPos) -> Option<ItemStack> {
        if let Some(BlockEntity::Anvil(a)) = self.installations.get_mut(&pos) {
            a.strikes = 0;
            return a.bloom.take();
        }
        None
    }

    /// One hammer strike; finishing the work returns the output.
    pub fn anvil_strike_at(&mut self, pos: BlockPos) -> Option<ItemStack> {
        let reg = self.reg.clone();
        let st = self.station_at(pos)?;
        if let Some(BlockEntity::Anvil(a)) = self.installations.get_mut(&pos)
            && let Some(b) = a.bloom
            && let Some(def) = reg
                .worked
                .iter()
                .find(|w| w.input == b.item && w.station == st)
        {
            a.strikes += 1;
            if a.strikes >= def.strikes {
                a.bloom = None;
                a.strikes = 0;
                let mut out = ItemStack::new(&reg, def.output, 1);
                out.count = def.count;
                if let Some(ledger) = &mut self.material_ledger
                    && let Err(error) = ledger.record_recipe_loss(&def.loss)
                {
                    eprintln!("materials: station process accounting failed: {error}");
                }
                return Some(out);
            }
        }
        None
    }
}
