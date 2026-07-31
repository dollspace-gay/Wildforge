//! Falling blocks, multiblock machines, clamps, anvils, and archaeology.

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
    #[cfg(test)]
    pub(super) fn detach(&mut self, x: i32, y: i32, z: i32, b: BlockId) {
        let Some(pos) = crate::planet::BlockPos::of_world(x, y, z) else {
            return;
        };
        self.detach_at(pos, b);
    }

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
        let mouth = [
            self.reg.block_id("base:bloomery"),
            self.reg.block_id("base:bloomery_lit"),
        ];
        self.check_stack_at(pos, &mouth)
    }

    /// Validate the forge: the firebrick stack with a forge mouth,
    /// PLUS a chimney (three more courses of firebrick ring around an
    /// open flue above the stack — rain never reaches the fire) and a
    /// stone anvil within three blocks of the mouth. A building, not
    /// a block: the workshop is the capital (economy plan, leg 2).
    pub fn check_forge_at(&self, pos: BlockPos) -> Option<BlockPos> {
        let mouth = [
            self.reg.block_id("base:forge"),
            self.reg.block_id("base:forge_lit"),
        ];
        let core = self.check_stack_at(pos, &mouth)?;
        if !self.has_chimney_at(core) {
            return None;
        }
        let anvil = self.reg.block_id("base:stone_anvil")?;
        for dx in -3i32..=3 {
            for dz in -3i32..=3 {
                for dy in -1..=1 {
                    if pos
                        .offset(dx, dy, dz)
                        .is_some_and(|at| self.get_block_at(at) == anvil)
                    {
                        return Some(core);
                    }
                }
            }
        }
        None
    }

    /// Light a charged forge. Errors name what's missing.
    pub fn light_forge_at(&mut self, pos: BlockPos) -> Result<(), &'static str> {
        let core = self
            .check_forge_at(pos)
            .ok_or("the forge wants its stack, chimney, and anvil")?;
        let Some(BlockEntity::Forge(f)) = self.block_entities.get_mut(&pos) else {
            return Err("nothing charged");
        };
        if f.lit {
            return Err("already firing");
        }
        let n_charge: u32 = f.charge.iter().flatten().map(|s| s.count).sum();
        let n_fuel: u32 = f.fuel.iter().flatten().map(|s| s.count).sum();
        if n_charge < 1 || n_fuel < 1 {
            return Err("needs charge and fuel");
        }
        f.lit = true;
        f.progress = 0.0;
        f.core = Some(core);
        self.swap_block_keep_entity_at(pos, "base:forge_lit");
        Ok(())
    }

    /// Three more courses of firebrick ring over the stack, flue
    /// open: the chimney that turns a station into a workshop. Rain
    /// never reaches a chimneyed fire.
    fn has_chimney_at(&self, core: BlockPos) -> bool {
        let Some(fb) = self.reg.block_id("base:firebrick") else {
            return false;
        };
        for ly in 3..6 {
            let Some(flue) = core.offset(0, ly, 0) else {
                return false;
            };
            if self.get_block_at(flue) != AIR {
                return false;
            }
            for rx in -1..=1 {
                for rz in -1..=1 {
                    if rx == 0 && rz == 0 {
                        continue;
                    }
                    if core
                        .offset(rx, ly, rz)
                        .is_none_or(|at| self.get_block_at(at) != fb)
                    {
                        return false;
                    }
                }
            }
        }
        true
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
        let mouth = [
            self.reg.block_id("base:separator"),
            self.reg.block_id("base:separator_lit"),
        ];
        self.check_stack_at(pos, &mouth)
    }

    /// The same stack with a kiln in its mouth fires glass instead.
    pub fn check_kiln_at(&self, pos: BlockPos) -> Option<BlockPos> {
        let mouth = [
            self.reg.block_id("base:kiln"),
            self.reg.block_id("base:kiln_lit"),
        ];
        self.check_stack_at(pos, &mouth)
    }

    /// Validate a market stall at its counter: two log posts (two
    /// tall) flanking the counter along either axis, bridged by a
    /// three-wide awning of solid or glass at post-top height. A
    /// stall trades only while it stands (trade & travel, stage 3).
    pub fn check_stall_at(&self, pos: BlockPos) -> bool {
        let logs = self.reg.tags.get("base:logs").cloned().unwrap_or_default();
        let is_log = |b: BlockId| {
            self.reg
                .item_id(&self.reg.block(b).name)
                .is_some_and(|i| logs.contains(&i))
        };
        let awning_ok = |b: BlockId| self.reg.is_solid(b) || self.reg.block(b).glass;
        'axes: for (dx, dz) in [(1, 0), (0, 1)] {
            for side in [-1, 1] {
                let Some(post) = pos.offset(dx * side, 0, dz * side) else {
                    continue 'axes;
                };
                let Some(post_top) = post.offset(0, 1, 0) else {
                    continue 'axes;
                };
                if !is_log(self.get_block_at(post)) || !is_log(self.get_block_at(post_top)) {
                    continue 'axes;
                }
            }
            for i in -1..=1 {
                if pos
                    .offset(dx * i, 2, dz * i)
                    .is_none_or(|at| !awning_ok(self.get_block_at(at)))
                {
                    continue 'axes;
                }
            }
            return true;
        }
        false
    }

    /// The shared shell scan: the stack is the stack; the mouth block
    /// decides the craft.
    pub(super) fn check_stack_at(
        &self,
        pos: BlockPos,
        mouth: &[Option<BlockId>; 2],
    ) -> Option<BlockPos> {
        let fb = self.reg.block_id("base:firebrick")?;
        'dirs: for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let Some(core) = pos.offset(dx, 0, dz) else {
                continue;
            };
            for ly in 0..3 {
                if core
                    .offset(0, ly, 0)
                    .is_none_or(|at| self.get_block_at(at) != AIR)
                {
                    continue 'dirs;
                }
                for rx in -1..=1 {
                    for rz in -1..=1 {
                        if rx == 0 && rz == 0 {
                            continue;
                        }
                        let Some(at) = core.offset(rx, ly, rz) else {
                            continue 'dirs;
                        };
                        let b = self.get_block_at(at);
                        if at == pos && ly == 0 {
                            if !mouth.contains(&Some(b)) {
                                continue 'dirs;
                            }
                        } else if b != fb {
                            continue 'dirs;
                        }
                    }
                }
            }
            return Some(core);
        }
        None
    }

    /// Light a charged bloomery. Errors name what's missing.
    pub fn light_bloomery_at(&mut self, pos: BlockPos) -> Result<(), &'static str> {
        let core = self.check_bloomery_at(pos).ok_or("the stack is breached")?;
        let Some(BlockEntity::Bloomery(b)) = self.block_entities.get_mut(&pos) else {
            return Err("nothing charged");
        };
        if b.lit {
            return Err("already firing");
        }
        let n_charge: u32 = b.charge.iter().flatten().map(|s| s.count).sum();
        let n_fuel: u32 = b.fuel.iter().flatten().map(|s| s.count).sum();
        if n_charge < 2 || n_fuel < 2 {
            return Err("needs at least 2 charge and 2 charcoal");
        }
        b.lit = true;
        b.progress = 0.0;
        b.core = Some(core);
        self.swap_block_keep_entity_at(pos, "base:bloomery_lit");
        Ok(())
    }

    /// Light a charged kiln. Errors name what's missing.
    pub fn light_kiln_at(&mut self, pos: BlockPos) -> Result<(), &'static str> {
        let core = self.check_kiln_at(pos).ok_or("the stack is breached")?;
        let Some(BlockEntity::Kiln(k)) = self.block_entities.get_mut(&pos) else {
            return Err("nothing charged");
        };
        if k.lit {
            return Err("already firing");
        }
        let n_sand: u32 = k.sand.iter().flatten().map(|s| s.count).sum();
        let n_fuel: u32 = k.fuel.iter().flatten().map(|s| s.count).sum();
        if n_sand < 2 || n_fuel < 2 {
            return Err("needs at least 2 sand and 2 charcoal");
        }
        k.lit = true;
        k.progress = 0.0;
        k.core = Some(core);
        self.swap_block_keep_entity_at(pos, "base:kiln_lit");
        Ok(())
    }

    /// Fire every lit kiln: shared shell/weather rules, glass out.
    pub(super) fn tick_kilns(&mut self, dt: f32) {
        let keys: Vec<BlockPos> = self
            .block_entities
            .iter()
            .filter(|(_, e)| matches!(e, BlockEntity::Kiln(k) if k.lit))
            .map(|(k, _)| *k)
            .collect();
        for pos in keys {
            let Some(BlockEntity::Kiln(mut k)) = self.block_entities.remove(&pos) else {
                continue;
            };
            if self.check_kiln_at(pos).is_none() {
                k.lit = false;
                k.progress = 0.0;
                self.swap_block_keep_entity_at(pos, "base:kiln");
                self.block_entities.insert(pos, BlockEntity::Kiln(k));
                continue;
            }
            // A chimneyed kiln is a glassworks: rain can't reach the
            // fire, and the draft doubles what each fuel fires.
            let glassworks = self.check_glassworks_at(pos).is_some();
            let unroofed = k
                .core
                .and_then(|core| core.offset(0, 3, 0))
                .is_some_and(|above| self.light_at_pos(above).1 == 15);
            let wet = !glassworks
                && self.weather.precipitating()
                && self.rains_at_surface(pos.surface())
                && unroofed;
            if wet && self.weather == Weather::Storm {
                k.lit = false;
                k.progress = 0.0;
                self.swap_block_keep_entity_at(pos, "base:kiln");
                self.block_entities.insert(pos, BlockEntity::Kiln(k));
                continue;
            }
            k.progress += dt * if wet { 0.5 } else { 1.0 };
            if k.progress >= KILN_FIRE_SECS {
                if let Some((_, fuel_item, clear)) = self.reg.kiln_base {
                    let n_sand: u32 = k.sand.iter().flatten().map(|s| s.count).sum();
                    let n_fuel: u32 = k.fuel.iter().flatten().map(|s| s.count).sum();
                    let fuel_reach = if glassworks { n_fuel * 2 } else { n_fuel };
                    let pairs = n_sand.min(fuel_reach) / 2;
                    let out_n = pairs * 2;
                    // One powder colors the whole batch.
                    let colored = k.powder.as_ref().and_then(|p| {
                        self.reg
                            .kiln
                            .iter()
                            .find(|(pw, _)| *pw == p.item)
                            .map(|(_, g)| *g)
                    });
                    let out_item = colored.unwrap_or(clear);
                    if colored.is_some()
                        && let Some(p) = &mut k.powder
                    {
                        p.count -= 1;
                        if p.count == 0 {
                            k.powder = None;
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
                    eat(&mut k.sand, pairs * 2);
                    eat(
                        &mut k.fuel,
                        if glassworks {
                            (pairs * 2).div_ceil(2)
                        } else {
                            pairs * 2
                        },
                    );
                    let _ = fuel_item;
                    if out_n > 0 {
                        let reg = self.reg.clone();
                        let mut out = ItemStack::new(&reg, out_item, 1);
                        out.count = out_n;
                        for s in k.sand.iter_mut() {
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
            self.block_entities.insert(pos, BlockEntity::Kiln(k));
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
        items.pop()
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
