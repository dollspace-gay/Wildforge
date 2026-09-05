//! Block edits coordinator for the authoritative world.

use super::{AIR, BlockEntity, BlockId, BlockPos, Chunk, ChunkPos, ItemStack, World, machines};

impl World {
    #[cfg(test)]
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, b: BlockId) {
        self.set_block_meta(x, y, z, b, 0);
    }

    /// Planetary block mutation used by topology-aware simulation walks.
    pub fn set_block_at(&mut self, pos: crate::planet::BlockPos, block: BlockId) {
        self.set_block_meta_at(pos, block, 0);
    }

    /// A mod/script-authored edit is an explicit source/sink, never an
    /// untracked shortcut around finite extraction. This deliberately marks
    /// the chunk touched so later retrogen cannot overwrite the authored cell.
    pub fn set_block_authored_at(
        &mut self,
        pos: crate::planet::BlockPos,
        block: BlockId,
        source: &str,
    ) {
        let old = self.get_block_at(pos);
        if old == block {
            return;
        }
        self.player_touched.insert(pos.chunk());
        let mut before = self.reg.block(old).name.clone();
        let old_materials = self.reg.block(old).materials.clone();
        if !old_materials.is_empty() {
            if self.break_block_at(pos, None, false, false).is_none() {
                self.set_block_at(pos, AIR);
                if let Some(ledger) = &mut self.material_ledger
                    && let Err(error) = ledger.record_admin_deletion(&old_materials)
                {
                    eprintln!("materials: authored block deletion failed: {error}");
                }
            }
            before = self.reg.block(AIR).name.clone();
        }
        // When the old block is nonmaterial, keep it in place until the
        // authored-placement journal exists. The eventual chunk write then
        // commits that replacement and the material addition together.
        let materials = self.reg.block(block).materials.clone();
        let material_operation = if materials.is_empty() {
            None
        } else if let Some(ledger) = &mut self.material_ledger {
            match ledger.begin_authored_place(
                pos,
                &before,
                &self.reg.block(block).name,
                &materials,
                source,
            ) {
                Ok(operation) => operation,
                Err(error) => {
                    eprintln!("materials: could not journal authored block at {pos:?}: {error}");
                    return;
                }
            }
        } else {
            None
        };
        self.set_block_at(pos, block);
        if let Some(operation) = material_operation {
            self.complete_material_operation(&operation);
        }
    }

    /// Low-level typed mutation. Subsystems that already own a canonical
    /// address never convert it back through a planar tuple.
    pub fn set_block_meta_at(&mut self, pos: BlockPos, block: BlockId, meta: u8) {
        self.set_block_water_at(pos, block, meta, 0);
    }

    /// Low-level block update carrying exact dissolved salt. Ordinary block
    /// edits call this with zero; water transfers must provide the parcel's
    /// mass and derive concentration metadata from it.
    pub fn set_block_water_at(&mut self, pos: BlockPos, block: BlockId, meta: u8, salt_mass: u16) {
        let soil_salinity = if self.reg.block(block).fert_tiles.is_some() {
            self.get_soil_salinity_at(pos)
        } else {
            0
        };
        self.set_block_state_at(pos, block, meta, salt_mass, soil_salinity);
    }

    /// Exact authoritative voxel state used by chunk replication. Soil salt
    /// is independent of dissolved water salt and must survive crop/meta edits.
    pub fn set_block_state_at(
        &mut self,
        pos: BlockPos,
        block: BlockId,
        meta: u8,
        salt_mass: u16,
        soil_salinity: u8,
    ) {
        let chunk_pos = pos.chunk();
        let old = self.get_block_at(pos);
        let old_holds_water_carrier =
            self.reg.is_water(old) || self.reg.block(old).name == "base:ice";
        let new_holds_water_carrier =
            self.reg.is_water(block) || self.reg.block(block).name == "base:ice";
        if self.chunks.write_state(pos, block, meta, salt_mass, soil_salinity).is_none() {
            return;
        }
        if self.log_edits {
            self.edit_log.push((pos, block, meta, salt_mass, soil_salinity));
        }
        if old_holds_water_carrier
            && !new_holds_water_carrier
            && let Some(carriers) = self.water_carriers.as_mut()
        {
            // The regional arcane ledger remains authoritative for the dross;
            // this only retires a no-longer-physical per-voxel allocation.
            // Ice deliberately retains the allocation for exact thawing.
            carriers.cells.remove(&pos);
        }
        self.chunks.dirty_edit_neighbors(pos);
        self.wake_water_at(pos);

        // Gravity blocks detach when support vanishes, and a newly placed
        // gravity block over air starts falling. All neighbor addressing goes
        // through BlockPos::offset so this works identically at face seams.
        if !self.reg.is_solid(block)
            && let Some(above) = pos.offset(0, 1, 0)
        {
            let above_block = self.get_block_at(above);
            if self.reg.block(above_block).falls {
                self.detach_at(above, above_block);
            }
        }
        if self.reg.block(block).falls
            && let Some(below) = pos.offset(0, -1, 0)
            && !self.reg.is_solid(self.get_block_at(below))
        {
            self.detach_at(pos, block);
            return;
        }

        // Resident support classification is shared with replica application;
        // the authoritative coordinator owns the displaced items.
        if let Some((above, above_block)) = self.chunks.unsupported_above(&self.reg, pos, block) {
            if let Some((item, count)) = self.reg.block(above_block).drops {
                let reg = self.reg.clone();
                self.push_drop_at(above, ItemStack::new(&reg, item, count));
            }
            self.set_block_at(above, AIR);
        }

        let fluid_level_only = self.reg.is_fluid(old)
            && self.reg.is_fluid(block)
            && self.reg.is_lava(old) == self.reg.is_lava(block);
        let front_move = self.fluid_batch
            && (self.reg.is_fluid(old) || self.reg.is_fluid(block))
            && (self.reg.is_fluid(old) || self.reg.is_air(old))
            && (self.reg.is_fluid(block) || self.reg.is_air(block));
        let bulk_edit = self.edit_relight_batch && !fluid_level_only;
        if front_move || bulk_edit {
            self.pending_relight.insert(chunk_pos);
        } else if !fluid_level_only {
            self.relight_and_cascade(chunk_pos);
        }

        // Changing material identity invalidates the machine living here.
        // Metadata is ordinary state on the same physical block (crop stage,
        // mechanism latch, water level) and must not silently delete its
        // embodied block entity.
        if old != block
            && let Some(entity) = self.installations.remove(&pos)
        {
            let spilled: Vec<ItemStack> = match entity {
                BlockEntity::Furnace(f) => {
                    [f.input, f.fuel, f.output].into_iter().flatten().collect()
                }
                BlockEntity::Chest(c) => c.slots.into_iter().flatten().collect(),
                BlockEntity::Depot(d) => d.storage.into_iter().flatten().collect(),
                BlockEntity::Offering(o) => o.slots.into_iter().flatten().collect(),
                BlockEntity::Multiblock(m) => {
                    let reg = &self.reg;
                    let mut stacks: Vec<ItemStack> =
                        m.charge.into_iter().chain(m.fuel).flatten().collect();
                    if let Some(r) = m.reagent {
                        stacks.push(r);
                    }
                    let mut push = |name: &str, count: u32| {
                        if count > 0
                            && let Some(item) = reg.item_id(name)
                        {
                            let mut stack = ItemStack::new(reg, item, 1);
                            stack.count = count;
                            stacks.push(stack);
                        }
                    };
                    if !m.reclaim.is_empty()
                        && let Some(ledger) = &mut self.material_ledger
                        && let Err(error) = ledger.bury_materials(
                            pos,
                            &m.reclaim,
                            "machine dismantled with fractional recovered stock",
                        )
                    {
                        eprintln!("materials: machine stock salvage failed: {error}");
                    }
                    push("base:rare_earth_powder", m.powder);
                    push("base:charcoal", m.separator_fuel);
                    push("base:neodymium", m.neodymium);
                    push("base:cerium", m.cerium);
                    stacks
                }
                BlockEntity::Sign(_) => Vec::new(),
                BlockEntity::Stall(st) => st
                    .goods
                    .into_iter()
                    .chain(st.till)
                    .chain([st.price])
                    .flatten()
                    .collect(),
                BlockEntity::Smoker(sm) => sm.meat.into_iter().flatten().collect(),
                BlockEntity::Clamp(_) => Vec::new(),
                BlockEntity::Anvil(a) => a.bloom.into_iter().collect(),
                BlockEntity::Steam(_) => Vec::new(),
                BlockEntity::SurveyFolio(folio) => self
                    .reg
                    .item_id("base:survey_folio")
                    .map(|item| {
                        let mut stack = ItemStack::new(&self.reg, item, 1);
                        stack.arcane_id = folio.object_id;
                        vec![stack]
                    })
                    .unwrap_or_default(),
                BlockEntity::DiscoveryApparatus(apparatus) => {
                    [apparatus.sample, apparatus.reference]
                        .into_iter()
                        .flatten()
                        .collect()
                }
                BlockEntity::BindingFrame(frame) => frame
                    .mounts()
                    .into_iter()
                    .chain([frame.output])
                    .flatten()
                    .collect(),
                BlockEntity::ChargeVessel(vessel) => vessel.vessel.into_iter().collect(),
                BlockEntity::Switch(_) => Vec::new(),
            };
            for stack in spilled {
                self.push_drop_at(pos, stack);
            }
        }

        // Nest spawn-gates (capability E9) live exactly while their marker
        // block does: placing a nest block records it; replacing or breaking
        // it clears the record, which stops that species respawning nearby.
        if old != block {
            match self.reg.nest_index_for_block(block) {
                Some(nest) => {
                    self.nests.insert(pos, nest);
                    self.nest_spawn_cd.entry(pos).or_insert(0.0);
                }
                None => {
                    self.nests.remove(&pos);
                    self.nest_spawn_cd.remove(&pos);
                }
            }
        }

        // Event-driven multiblock revalidation (Phase 2, spec Part 1.2's
        // scaling note): a real block change anywhere within a registered
        // instance's shell region re-runs that instance's shape match and
        // re-folds its stats immediately — no dependence on the machine
        // being lit or ticked. Water and meta-only edits keep `old ==
        // block` and skip this; remote replicas let the host decide.
        if old != block {
            // A ghost overlay (spec Part 1.4) is satisfied cell-by-cell by
            // ordinary placement: this is the only hook, and it clears a
            // pending cell exactly when the voxel holds the required block.
            // Any other write is an ordinary edit and leaves the fill alone.
            self.clear_pending_fill_at(pos, block);
            self.revalidate_multiblocks_around(pos);
        }
    }

    /// Revalidate every registered multiblock instance whose shell region
    /// could contain the edited position. The per-instance test is O(1)
    /// arithmetic ([`crate::world::multiblock::pos_within_extent`]); only
    /// instances actually in range re-run their shape match. Delegates to
    /// the store-generic hook shared with structure-hosted machines.
    pub(super) fn revalidate_multiblocks_around(&mut self, pos: BlockPos) {
        let revalidated = machines::revalidate_machines_around(self, pos);
        #[cfg(not(test))]
        let _ = revalidated;
        #[cfg(test)]
        {
            self.multiblock_revalidations += revalidated;
        }
    }

    pub fn set_soil_salinity_at(&mut self, pos: BlockPos, salinity: u8) {
        let block = self.get_block_at(pos);
        if self.reg.block(block).fert_tiles.is_none() {
            return;
        }
        self.set_block_state_at(
            pos,
            block,
            self.get_meta_at(pos),
            self.get_water_salt_at(pos),
            salinity,
        );
    }

    /// Apply an authored group of edits with normal logging, support checks,
    /// and fluid wakeups, but settle lighting only once per touched chunk.
    pub(crate) fn edit_batch(&mut self, edit: impl FnOnce(&mut Self)) {
        assert!(
            !self.fluid_batch && !self.edit_relight_batch && self.pending_relight.is_empty(),
            "block-edit batches cannot nest"
        );
        self.edit_relight_batch = true;
        edit(self);
        self.edit_relight_batch = false;
        let starts = std::mem::take(&mut self.pending_relight);
        self.relight_chunks_and_cascade(starts);
    }




    #[cfg(test)]
    pub(crate) fn edit_fixture_for_test(&mut self, edit: impl FnOnce(&mut Self)) {
        self.edit_batch(edit);
    }

    /// Convenience wrapper for block-only fixtures.
    #[cfg(test)]
    pub(crate) fn set_blocks_for_test(
        &mut self,
        edits: impl IntoIterator<Item = (i32, i32, i32, BlockId)>,
    ) {
        self.edit_fixture_for_test(|world| {
            for (x, y, z, block) in edits {
                world.set_block(x, y, z, block);
            }
        });
    }

    /// Install blank authoritative chunks for protocol fixtures that exercise
    /// a hand-built stage rather than terrain generation.
    #[cfg(test)]
    pub(crate) fn insert_empty_chunks_for_test(
        &mut self,
        positions: impl IntoIterator<Item = ChunkPos>,
    ) {
        let bedrock = self
            .reg
            .block_id("base:bedrock")
            .expect("base test registry has bedrock");
        for pos in positions {
            let mut chunk = Chunk::new();
            for x in 0..CHUNK_X {
                for z in 0..CHUNK_Z {
                    chunk.set(x, 0, z, bedrock);
                }
            }
            assert!(
                self.chunks.insert(pos, chunk).is_none(),
                "test fixture inserted chunk {pos:?} twice"
            );
        }
    }

    /// Set a block with an explicit metadata byte.
    #[cfg(test)]
    #[doc(hidden)]
    pub fn set_block_meta(&mut self, x: i32, y: i32, z: i32, b: BlockId, meta: u8) {
        if let Some(pos) = BlockPos::of_world(x, y, z) {
            self.set_block_meta_at(pos, b, meta);
        }
    }
}
