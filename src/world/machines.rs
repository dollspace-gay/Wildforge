//! Falling blocks, multiblock machines, clamps, anvils, and archaeology.

use super::multiblock::{
    BlockConstraint, MachineKind, MatchResult, MultiblockShape, Rotation, ShapeCell,
    fold_capabilities, fold_stats, match_shape, modules_in_category, pos_within_extent,
    shape_extent,
};
use super::*;

/// A powered station's batch limit: what one loading can hold.
pub const STATION_BULK: u32 = 16;

/// Powered stations that are the capital sibling of a hand process
/// read that process's worked table (the millstone IS a quern with a
/// shaft where your arm was).
pub fn worked_table_for(station: &str) -> &str {
    match station {
        "millstone" => "quern",
        s => s,
    }
}

/// Stations whose strikes come from the shaft line, not a player.
pub fn station_powered(station: &str) -> bool {
    matches!(
        station,
        "millstone" | "sawmill" | "lathe" | "iron_lathe" | "boring"
    )
}

impl World {
    pub fn falling_blocks(&self) -> &[FallingBlock] {
        &self.falling
    }

    pub fn replace_falling_blocks(&mut self, falling: Vec<FallingBlock>) {
        self.falling = falling;
    }

    /// Lift a block out of the grid and into the air (atomically: the
    /// cell empties in the same call, so it can't be duped).
    pub(super) fn detach_at(&mut self, pos: crate::planet::BlockPos, b: BlockId) {
        self.set_block_at(pos, AIR);
        self.falling.push(FallingBlock {
            pos: crate::planet::EntityPos::new(
                pos.face(),
                pos.u() as f32,
                pos.y() as f32,
                pos.v() as f32,
            )
            .expect("block corner is a canonical entity position"),
            vel: 0.0,
            block: b,
        });
    }

    /// Advance airborne blocks; landings re-plant (popping any plant or
    /// layer they crush) and re-trigger the cell above the launch site
    /// through the normal edit cascade.
    pub fn tick_falling(&mut self, dt: f32) {
        if self.falling.is_empty() {
            return;
        }
        // Landings apply immediately so a stacked column settles one on
        // top of the other instead of racing into the same cell.
        let mut fallen = std::mem::take(&mut self.falling);
        let mut still = Vec::with_capacity(fallen.len());
        for mut f in fallen.drain(..) {
            f.vel = (f.vel + 20.0 * dt).min(30.0);
            let Ok(moved) = f.pos.translated(glam::Vec3::new(0.0, -f.vel * dt, 0.0)) else {
                continue;
            };
            f.pos = moved.pos;
            let surface = crate::planet::SurfacePos::new(
                f.pos.face(),
                f.pos.u().floor() as u16,
                f.pos.v().floor() as u16,
            )
            .expect("canonical falling position has a valid surface cell");
            let below = f.pos.y.floor() as i32;
            if below < 0 {
                continue; // out of the world (should be impossible)
            }
            let at = |y: i32| {
                crate::planet::BlockPos::new(surface.face(), surface.u(), y as u8, surface.v())
                    .expect("falling block height is inside the shell")
            };
            if !self.reg.is_solid(self.get_block_at(at(below))) {
                still.push(f);
                continue;
            }
            // Land on the first free cell above the obstruction - a
            // second sand in the same column stacks instead of popping.
            let mut y = below + 1;
            while y < CHUNK_Y as i32 - 1 && self.reg.is_solid(self.get_block_at(at(y))) {
                y += 1;
            }
            let b = f.block;
            let cur = self.get_block_at(at(y));
            if cur != AIR {
                // Crushed: the plant/layer pops as its drop first.
                if let Some((item, n)) = self.reg.block(cur).drops {
                    let reg = self.reg.clone();
                    self.push_drop_at(at(y), ItemStack::new(&reg, item, n));
                }
            }
            self.set_block_at(at(y), b);
        }
        // Landings may have detached more (rare); keep both sets.
        self.falling.extend(still);
    }

    /// Land every airborne block instantly (world save/quit).
    pub fn settle_falling(&mut self) {
        while !self.falling.is_empty() {
            self.tick_falling(0.5);
        }
    }

    // ---------------- steelworks ----------------

    /// Validate the bloomery multiblock at this mouth: a hollow 1x1
    /// core beside the mouth wrapped in a 3-wide, 3-tall firebrick
    /// ring (23 firebrick + the mouth), open on top. Returns the core.
    pub fn check_bloomery_at(&self, pos: BlockPos) -> Option<BlockPos> {
        MachineKind::Bloomery
            .validate(self, pos)
            .map(|result| result.core)
    }

    /// Validate the forge: the firebrick stack with a forge mouth,
    /// PLUS a chimney (three more courses of firebrick ring around an
    /// open flue above the stack — rain never reaches the fire) and a
    /// stone anvil within three blocks of the mouth. A building, not
    /// a block: the workshop is the capital (economy plan, leg 2).
    pub fn check_forge_at(&self, pos: BlockPos) -> Option<BlockPos> {
        MachineKind::Forge
            .validate(self, pos)
            .map(|result| result.core)
    }

    /// Light a charged forge. Errors name what's missing.
    pub fn light_forge_at(&mut self, pos: BlockPos) -> Result<(), &'static str> {
        let matched = MachineKind::Forge
            .validate(self, pos)
            .ok_or("the forge wants its stack, chimney, and anvil")?;
        self.light_machine_at(pos, MachineKind::Forge, matched)
    }

    /// Three more courses of firebrick ring over the stack, flue
    /// open: the chimney that turns a station into a workshop. Rain
    /// never reaches a chimneyed fire.
    pub(crate) fn has_chimney_at(&self, core: BlockPos) -> bool {
        let shape = chimney_shape();
        match_shape(self, core, &shape).is_some()
    }

    /// A kiln whose stack carries the chimney is a GLASSWORKS: the
    /// draft doubles what each fuel fires, and weather means nothing
    /// (economy plan, leg 2 — same capital rule as the forge).
    pub fn check_glassworks_at(&self, pos: BlockPos) -> Option<BlockPos> {
        let core = self.check_kiln_at(pos)?;
        if self.has_chimney_at(core) {
            Some(core)
        } else {
            None
        }
    }

    /// The same stack with a separator in its mouth splits the mixed
    /// rare-earth powder instead (mechanization stage 6).
    pub fn check_separator_at(&self, pos: BlockPos) -> Option<BlockPos> {
        MachineKind::Separator
            .validate(self, pos)
            .map(|result| result.core)
    }

    /// The same stack with a kiln in its mouth fires glass instead.
    pub fn check_kiln_at(&self, pos: BlockPos) -> Option<BlockPos> {
        MachineKind::Kiln
            .validate(self, pos)
            .map(|result| result.core)
    }

    /// Validate a market stall at its counter: two log posts (two
    /// tall) flanking the counter along either axis, bridged by a
    /// three-wide awning of solid or glass at post-top height. A
    /// stall trades only while it stands (trade & travel, stage 3).
    pub fn check_stall_at(&self, pos: BlockPos) -> bool {
        match_shape(self, pos, &stall_shape()).is_some()
    }

    /// Light a charged machine on a validated shell: fold its stats,
    /// flip the mouth block to its lit face, and bank the fire.
    fn light_machine_at(
        &mut self,
        pos: BlockPos,
        kind: MachineKind,
        matched: MatchResult,
    ) -> Result<(), &'static str> {
        let wants = match kind {
            MachineKind::Bloomery => (2, 2),
            MachineKind::Forge => (1, 1),
            MachineKind::Kiln => (2, 2),
            MachineKind::Separator => (0, 0),
        };
        let mut stats = fold_stats(self, &matched.matched);
        if kind == MachineKind::Kiln {
            stats.chimney = self.has_chimney_at(matched.core);
        }
        let capabilities = fold_capabilities(self, &matched.matched, &matched.slots);
        let lit_block = match kind {
            MachineKind::Bloomery => "base:bloomery_lit",
            MachineKind::Forge => "base:forge_lit",
            MachineKind::Kiln => "base:kiln_lit",
            MachineKind::Separator => "base:separator_lit",
        };
        let Some(BlockEntity::Multiblock(m)) = self.block_entities.get_mut(&pos) else {
            return Err("nothing charged");
        };
        if m.kind != kind || m.lit {
            return Err("already firing");
        }
        let n_charge: u32 = m.charge.iter().flatten().map(|s| s.count).sum();
        let n_fuel: u32 = m.fuel.iter().flatten().map(|s| s.count).sum();
        if n_charge < wants.0 || n_fuel < wants.1 {
            return Err(match kind {
                MachineKind::Bloomery => "needs at least 2 charge and 2 charcoal",
                MachineKind::Forge => "needs charge and fuel",
                MachineKind::Kiln => "needs at least 2 sand and 2 charcoal",
                MachineKind::Separator => "nothing to charge",
            });
        }
        m.lit = true;
        m.progress = 0.0;
        m.core = Some(matched.core);
        m.stats = stats;
        m.capabilities = capabilities;
        self.swap_block_keep_entity_at(pos, lit_block);
        Ok(())
    }

    /// Light a charged bloomery. Errors name what's missing.
    pub fn light_bloomery_at(&mut self, pos: BlockPos) -> Result<(), &'static str> {
        let matched = MachineKind::Bloomery
            .validate(self, pos)
            .ok_or("the stack is breached")?;
        self.light_machine_at(pos, MachineKind::Bloomery, matched)
    }

    /// Light a charged kiln. Errors name what's missing.
    pub fn light_kiln_at(&mut self, pos: BlockPos) -> Result<(), &'static str> {
        let matched = MachineKind::Kiln
            .validate(self, pos)
            .ok_or("the stack is breached")?;
        self.light_machine_at(pos, MachineKind::Kiln, matched)
    }

    /// Fire every lit kiln: shared shell/weather rules, glass out. The
    /// shell is revalidated by the edit hook, not here; a chimneyed
    /// kiln reads its glassworks bonus from the folded stats.
    pub(super) fn tick_kilns(&mut self, dt: f32) {
        let keys: Vec<BlockPos> = self
            .block_entities
            .iter()
            .filter(|(_, e)| {
                matches!(e, BlockEntity::Multiblock(m) if m.kind == MachineKind::Kiln && m.lit)
            })
            .map(|(k, _)| *k)
            .collect();
        for pos in keys {
            let Some(BlockEntity::Multiblock(mut k)) = self.block_entities.remove(&pos) else {
                continue;
            };
            // A chimneyed kiln is a glassworks: rain can't reach the
            // fire, and the draft doubles what each fuel fires.
            let glassworks = k.stats.chimney;
            let unroofed = k
                .core
                .and_then(|core| core.offset(0, 3, 0))
                .is_some_and(|above| self.light_at_pos(above).1 == 15);
            let wet = !glassworks
                && self.weather_at_surface(pos.surface()).precipitation
                    == crate::planet_atlas::PrecipitationForm::Rain
                && unroofed;
            if wet
                && self.weather_at_surface(pos.surface()).kind
                    == crate::planet_atlas::LocalWeather::Storm
            {
                k.lit = false;
                k.progress = 0.0;
                self.swap_block_keep_entity_at(pos, "base:kiln");
                self.block_entities.insert(pos, BlockEntity::Multiblock(k));
                continue;
            }
            let heat = k.stats.heat_multiplier();
            k.progress += dt * if wet { 0.5 } else { 1.0 } * heat;
            if k.progress >= KILN_FIRE_SECS {
                if let Some((_, fuel_item, clear)) = self.reg.kiln_base {
                    let n_sand: u32 = k.charge.iter().flatten().map(|s| s.count).sum();
                    let n_fuel: u32 = k.fuel.iter().flatten().map(|s| s.count).sum();
                    let fuel_reach = if glassworks { n_fuel * 2 } else { n_fuel };
                    let pairs = n_sand.min(fuel_reach) / 2;
                    let out_n = pairs * 2;
                    // One powder colors the whole batch.
                    let colored = k.reagent.as_ref().and_then(|p| {
                        self.reg
                            .kiln
                            .iter()
                            .find(|recipe| recipe.powder == p.item)
                            .map(|recipe| recipe.glass)
                    });
                    let out_item = colored.unwrap_or(clear);
                    let powder_materials = colored
                        .and(k.reagent)
                        .map(|stack| {
                            crate::materials::stack_materials(
                                &self.reg,
                                ItemStack { count: 1, ..stack },
                            )
                        })
                        .unwrap_or_default();
                    if colored.is_some()
                        && let Some(p) = &mut k.reagent
                    {
                        p.count -= 1;
                        if p.count == 0 {
                            k.reagent = None;
                        }
                    }
                    let eat = |slots: &mut [Option<ItemStack>; 4], mut n: u32| {
                        for s in slots.iter_mut() {
                            if n == 0 {
                                break;
                            }
                            if let Some(st) = s {
                                let take = st.count.min(n);
                                n -= take;
                                st.count -= take;
                                if st.count == 0 {
                                    *s = None;
                                }
                            }
                        }
                    };
                    eat(&mut k.charge, pairs * 2);
                    let fuel_used = if glassworks {
                        (pairs * 2).div_ceil(2)
                    } else {
                        pairs * 2
                    };
                    let mut fuel_materials = crate::registry::MaterialVector::new();
                    let mut remaining = fuel_used;
                    for stack in k.fuel.iter().flatten() {
                        let take = stack.count.min(remaining);
                        remaining -= take;
                        let materials = crate::materials::stack_materials(
                            &self.reg,
                            ItemStack {
                                count: take,
                                ..*stack
                            },
                        );
                        for (material, amount) in materials {
                            *fuel_materials.entry(material).or_default() += amount;
                        }
                        if remaining == 0 {
                            break;
                        }
                    }
                    eat(&mut k.fuel, fuel_used);
                    if let Some(ledger) = &mut self.material_ledger {
                        if let Err(error) = ledger.record_consumption(&powder_materials) {
                            eprintln!("materials: kiln pigment accounting failed: {error}");
                        }
                        if let Err(error) = ledger.record_consumption(&fuel_materials) {
                            eprintln!("materials: kiln fuel accounting failed: {error}");
                        }
                    }
                    let _ = fuel_item;
                    if out_n > 0 {
                        let reg = self.reg.clone();
                        let mut out = ItemStack::new(&reg, out_item, 1);
                        out.count = out_n;
                        for s in k.charge.iter_mut() {
                            if s.is_none() {
                                *s = Some(out);
                                break;
                            }
                        }
                    }
                }
                k.lit = false;
                k.progress = 0.0;
                self.swap_block_keep_entity_at(pos, "base:kiln");
            }
            self.block_entities.insert(pos, BlockEntity::Multiblock(k));
        }
    }

    /// Swap a block without invalidating the machine living there.
    pub(super) fn swap_block_keep_entity_at(&mut self, pos: BlockPos, to: &str) {
        let Some(to) = self.reg.block_id(to) else {
            return;
        };
        let e = self.block_entities.remove(&pos);
        self.set_block_at(pos, to);
        if let Some(e) = e {
            self.block_entities.insert(pos, e);
        }
    }

    /// Find the `(anchor, category)` of the instance whose matched shell
    /// has `pos` as a module-slot cell. The O(1) `edit_region` test gates
    /// every candidate before its (more expensive) shape re-match.
    fn slot_of_instance_at(&self, pos: BlockPos) -> Option<(BlockPos, &'static str)> {
        for (anchor, entity) in &self.block_entities {
            let BlockEntity::Multiblock(m) = entity else {
                continue;
            };
            let extent = m.kind.edit_region(self, *anchor);
            if !pos_within_extent(pos, *anchor, extent) {
                continue;
            }
            if let Some(matched) = m.kind.validate(self, *anchor)
                && let Some(category) = matched.slots.get(&pos)
            {
                return Some((*anchor, *category));
            }
        }
        None
    }

    /// The module category installed at `pos`, if `pos` is a slot cell of
    /// a registered machine's shell. The game reads this to offer a swap.
    pub fn slot_category_at(&self, pos: BlockPos) -> Option<&'static str> {
        self.slot_of_instance_at(pos).map(|(_, category)| category)
    }

    /// Swap the module installed in a slot cell in place (spec Part 1.3).
    /// Only a real slot cell of a registered frame may be swapped, the
    /// replacement must belong to the slot's catalog, and the swap is a
    /// plain block edit: the machine's `BlockEntity` at the anchor is
    /// untouched, and the 2c edit hook re-folds the frame's stats and
    /// capabilities immediately.
    pub fn swap_slot_module_at(
        &mut self,
        pos: BlockPos,
        category: &'static str,
        replacement: BlockId,
    ) -> Result<(), &'static str> {
        let (_, found) = self.slot_of_instance_at(pos).ok_or("no module slot here")?;
        if found != category {
            return Err("this slot takes a different module category");
        }
        if !modules_in_category(&self.reg, category).contains(&replacement) {
            return Err("that is not a module of this slot's category");
        }
        if self.get_block_at(pos) == replacement {
            return Ok(());
        }
        self.set_block_at(pos, replacement);
        Ok(())
    }

    /// Flood-fill a covered log pile from the clicked log and light it.
    /// Exactly one face (the lighting face) may be exposed.
    pub fn try_light_clamp_at(&mut self, pos: BlockPos) -> Result<usize, &'static str> {
        let logs_tag = self.reg.tags.get("base:logs").cloned().unwrap_or_default();
        let is_log = |w: &World, p: BlockPos| {
            let b = w.get_block_at(p);
            w.reg
                .item_id(&w.reg.block(b).name)
                .is_some_and(|i| logs_tag.contains(&i))
        };
        if !is_log(self, pos) {
            return Err("light a log");
        }
        let mut set = HashSet::from([pos]);
        let mut logs = vec![pos];
        let mut queue = vec![pos];
        while let Some(p) = queue.pop() {
            for n in crate::planet::neighbors6(p) {
                if !set.contains(&n) && is_log(self, n) {
                    set.insert(n);
                    logs.push(n);
                    if set.len() > 8 {
                        return Err("the pile is too big to smolder (8 logs at most)");
                    }
                    queue.push(n);
                }
            }
        }
        if set.len() < 2 {
            return Err("a clamp needs at least 2 logs");
        }
        let mut exposed = 0;
        for p in &set {
            for n in crate::planet::neighbors6(*p) {
                if set.contains(&n) {
                    continue;
                }
                if !self.reg.is_solid(self.get_block_at(n)) {
                    exposed += 1;
                }
            }
        }
        if exposed > 1 {
            return Err("cover the pile with earth (one face open)");
        }
        let n = set.len();
        self.block_entities.insert(
            pos,
            BlockEntity::Clamp(ClampState {
                logs,
                timer: n as f32 * CLAMP_SECS_PER_LOG,
            }),
        );
        Ok(n)
    }

    /// The station kind ("anvil"/"quern"/"millstone"/...) of the block at pos.
    pub(super) fn station_at(&self, pos: BlockPos) -> Option<String> {
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
        let e = self
            .block_entities
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
        if let Some(BlockEntity::Anvil(a)) = self.block_entities.get_mut(&pos) {
            a.strikes = 0;
            return a.bloom.take();
        }
        None
    }

    /// One hammer strike; finishing the work returns the output.
    pub fn anvil_strike_at(&mut self, pos: BlockPos) -> Option<ItemStack> {
        let reg = self.reg.clone();
        let st = self.station_at(pos)?;
        if let Some(BlockEntity::Anvil(a)) = self.block_entities.get_mut(&pos)
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

    /// Archaeology: sweep a remnant block — it yields its artifact once
    /// and becomes plain. Returns what was found.
    pub fn brush_block_at(&mut self, pos: BlockPos, rng: &mut u32) -> Option<ItemStack> {
        let b = self.get_block_at(pos);
        let (table, becomes) = self.reg.block(b).brush.clone()?;
        let mut items = self.roll_loot(&table, 1, rng);
        self.set_block_at(pos, becomes);
        let found = items.pop();
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
        let block = self.reg.block(self.get_block_at(pos));
        block.brush.is_none()
            && block.interaction.is_none()
            && block.hardness.is_some()
            && block.material_class == crate::registry::MaterialClass::TransformativeFinite
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

    // Positive-Z adapters exist only for the pre-topology fixture suite.
    #[cfg(test)]
    pub fn check_bloomery(&self, x: i32, y: i32, z: i32) -> Option<BlockPos> {
        BlockPos::of_world(x, y, z).and_then(|pos| self.check_bloomery_at(pos))
    }

    #[cfg(test)]
    pub fn check_forge(&self, x: i32, y: i32, z: i32) -> Option<BlockPos> {
        BlockPos::of_world(x, y, z).and_then(|pos| self.check_forge_at(pos))
    }

    #[cfg(test)]
    pub fn check_glassworks(&self, x: i32, y: i32, z: i32) -> Option<BlockPos> {
        BlockPos::of_world(x, y, z).and_then(|pos| self.check_glassworks_at(pos))
    }

    #[cfg(test)]
    pub fn check_separator(&self, x: i32, y: i32, z: i32) -> Option<BlockPos> {
        BlockPos::of_world(x, y, z).and_then(|pos| self.check_separator_at(pos))
    }

    #[cfg(test)]
    pub fn check_kiln(&self, x: i32, y: i32, z: i32) -> Option<BlockPos> {
        BlockPos::of_world(x, y, z).and_then(|pos| self.check_kiln_at(pos))
    }

    #[cfg(test)]
    pub fn check_stall(&self, x: i32, y: i32, z: i32) -> bool {
        BlockPos::of_world(x, y, z).is_some_and(|pos| self.check_stall_at(pos))
    }

    #[cfg(test)]
    pub fn light_bloomery(&mut self, x: i32, y: i32, z: i32) -> Result<(), &'static str> {
        self.light_bloomery_at(BlockPos::of_world(x, y, z).ok_or("outside the world")?)
    }

    #[cfg(test)]
    pub fn light_forge(&mut self, x: i32, y: i32, z: i32) -> Result<(), &'static str> {
        self.light_forge_at(BlockPos::of_world(x, y, z).ok_or("outside the world")?)
    }

    #[cfg(test)]
    pub fn light_kiln(&mut self, x: i32, y: i32, z: i32) -> Result<(), &'static str> {
        self.light_kiln_at(BlockPos::of_world(x, y, z).ok_or("outside the world")?)
    }

    #[cfg(test)]
    pub fn try_light_clamp(&mut self, x: i32, y: i32, z: i32) -> Result<usize, &'static str> {
        self.try_light_clamp_at(BlockPos::of_world(x, y, z).ok_or("outside the world")?)
    }

    #[cfg(test)]
    pub fn anvil_put(&mut self, pos: (i32, i32, i32), stack: ItemStack) -> bool {
        BlockPos::of_world(pos.0, pos.1, pos.2).is_some_and(|at| self.anvil_put_at(at, stack))
    }

    #[cfg(test)]
    pub fn anvil_take(&mut self, pos: (i32, i32, i32)) -> Option<ItemStack> {
        BlockPos::of_world(pos.0, pos.1, pos.2).and_then(|at| self.anvil_take_at(at))
    }

    #[cfg(test)]
    pub fn anvil_strike(&mut self, pos: (i32, i32, i32)) -> Option<ItemStack> {
        BlockPos::of_world(pos.0, pos.1, pos.2).and_then(|at| self.anvil_strike_at(at))
    }

    #[cfg(test)]
    pub fn brush_block(&mut self, x: i32, y: i32, z: i32, rng: &mut u32) -> Option<ItemStack> {
        BlockPos::of_world(x, y, z).and_then(|pos| self.brush_block_at(pos, rng))
    }

    // ---------------- wildlife ----------------
}

impl MachineKind {
    /// The two mouth blocks a kind routes its craft through: the handed
    /// and lit faces of its mouth station.
    fn mouth(self, reg: &Registry) -> [Option<BlockId>; 2] {
        let (a, b) = match self {
            MachineKind::Bloomery => ("base:bloomery", "base:bloomery_lit"),
            MachineKind::Forge => ("base:forge", "base:forge_lit"),
            MachineKind::Kiln => ("base:kiln", "base:kiln_lit"),
            MachineKind::Separator => ("base:separator", "base:separator_lit"),
        };
        [reg.block_id(a), reg.block_id(b)]
    }

    /// Validate this kind's full shell at `anchor`: the firebrick stack,
    /// the mouth block, and — for the forge — the chimney and anvil.
    /// Returns the match result (core + folded cell map) on success.
    pub fn validate(self, world: &World, anchor: BlockPos) -> Option<MatchResult> {
        let shape = stack_shape(&self.mouth(&world.reg));
        let matched = match_shape(world, anchor, &shape)?;
        if self == MachineKind::Forge {
            if !world.has_chimney_at(matched.core) {
                return None;
            }
            let anvil = world.reg.block_id("base:stone_anvil")?;
            let mut found = false;
            for dx in -3i32..=3 {
                for dz in -3i32..=3 {
                    for dy in -1..=1 {
                        if anchor
                            .offset(dx, dy, dz)
                            .is_some_and(|at| world.get_block_at(at) == anvil)
                        {
                            found = true;
                        }
                    }
                }
            }
            if !found {
                return None;
            }
        }
        Some(matched)
    }

    /// The axis-aligned block-cell region (relative to `anchor`) that an
    /// edit must fall inside to warrant revalidating this instance. Covers
    /// the shell cells, the core, the forge's chimney, and its anvil scan.
    pub fn edit_region(
        self,
        world: &World,
        _anchor: BlockPos,
    ) -> ((i32, i32, i32), (i32, i32, i32)) {
        let shell = shape_extent(&stack_shape(&self.mouth(&world.reg)));
        if self == MachineKind::Kiln {
            // A kiln's stats read the chimney too (glassworks): cover the
            // three courses of ring over the core, which sits one cell
            // out from the anchor in any cardinal direction.
            let (mn, mx) = shell;
            return (
                (mn.0.min(-2), mn.1, mn.2.min(-2)),
                (mx.0.max(2), mx.1.max(5), mx.2.max(2)),
            );
        }
        if self != MachineKind::Forge {
            return shell;
        }
        // Union with the chimney (three courses over the core) and the
        // anvil search box (3x3x3 around the mouth anchor).
        let chimney = shape_extent(&chimney_shape());
        let (mut mn, mut mx) = (shell.0, shell.1);
        mn.0 = mn.0.min(chimney.0.0);
        mn.1 = mn.1.min(chimney.0.1);
        mn.2 = mn.2.min(chimney.0.2);
        mx.0 = mx.0.max(chimney.1.0);
        mx.1 = mx.1.max(chimney.1.1);
        mx.2 = mx.2.max(chimney.1.2);
        mn.0 = mn.0.min(-3);
        mn.1 = mn.1.min(-1);
        mn.2 = mn.2.min(-3);
        mx.0 = mx.0.max(3);
        mx.1 = mx.1.max(1);
        mx.2 = mx.2.max(3);
        (mn, mx)
    }
}

/// The shared shell: a 3-wide, 3-tall firebrick ring around an open core
/// cell (1,0,0) from the mouth anchor, with the mouth block filling the
/// cell opposite the core on the base course. Tries all four cardinal
/// directions; the first that satisfies the ring returns its core.
fn stack_shape(mouth: &[Option<BlockId>; 2]) -> MultiblockShape {
    let mut cells = Vec::with_capacity(3 * 8 + 3);
    for ly in 0..3 {
        // Core column: open air.
        cells.push(ShapeCell {
            offset: (1, ly, 0),
            constraint: BlockConstraint::Air,
        });
        for rx in -1..=1 {
            for rz in -1..=1 {
                if rx == 0 && rz == 0 {
                    continue;
                }
                let offset = (1 + rx, ly, rz);
                let constraint = if offset == (0, 0, 0) {
                    // The mouth block selects WHICH machine this is.
                    BlockConstraint::OneOf(mouth.iter().flatten().copied().collect())
                } else if offset == (1, 0, -1) {
                    // One base-course ring cell is a swappable casing
                    // module slot (spec Part 1.3): any catalog member
                    // holds the stack, and the installed module's
                    // capabilities fold into the frame.
                    BlockConstraint::Module("casing")
                } else {
                    // Any firebrick tier holds a stack together.
                    BlockConstraint::Tag("base:firebrick")
                };
                cells.push(ShapeCell { offset, constraint });
            }
        }
    }
    MultiblockShape {
        cells,
        core: (1, 0, 0),
        rotations: &Rotation::CARDINAL,
    }
}

/// Three more courses of firebrick ring over the stack's core, flue
/// open — the chimney that makes a station a workshop. Rotation-invariant.
fn chimney_shape() -> MultiblockShape {
    let mut cells = Vec::with_capacity(3 * 8 + 3);
    for ly in 3..6 {
        cells.push(ShapeCell {
            offset: (0, ly, 0),
            constraint: BlockConstraint::Air,
        });
        for rx in -1..=1 {
            for rz in -1..=1 {
                if rx == 0 && rz == 0 {
                    continue;
                }
                cells.push(ShapeCell {
                    offset: (rx, ly, rz),
                    constraint: BlockConstraint::Tag("base:firebrick"),
                });
            }
        }
    }
    MultiblockShape {
        cells,
        core: (0, 0, 0),
        rotations: &[Rotation::R0],
    }
}

/// A market stall: two log posts flanking the counter (two tall), bridged
/// by a three-wide awning of solid or glass at post-top height. Tries both
/// axes; the stall stands while either reads true.
fn stall_shape() -> MultiblockShape {
    let mut cells = Vec::with_capacity(7);
    for side in [-1, 1] {
        cells.push(ShapeCell {
            offset: (side, 0, 0),
            constraint: BlockConstraint::Tag("base:logs"),
        });
        cells.push(ShapeCell {
            offset: (side, 1, 0),
            constraint: BlockConstraint::Tag("base:logs"),
        });
    }
    for i in -1..=1 {
        cells.push(ShapeCell {
            offset: (i, 2, 0),
            constraint: BlockConstraint::SolidOrGlass,
        });
    }
    MultiblockShape {
        cells,
        core: (0, 0, 0),
        rotations: &Rotation::CARDINAL,
    }
}
