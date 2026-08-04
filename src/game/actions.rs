//! Combat, world interaction, held-item art, and script command application.

use super::*;

impl Game {
    /// The tile set dressing a humanoid for a given style.
    pub(super) fn humanoid_art(st: style::Style) -> mobs::HumanoidArt {
        let b = |n: &str| *atlas::builtin_slots().get(n).unwrap_or(&0);
        mobs::HumanoidArt {
            skin: style::skin_tile(&st),
            face: style::face_tile(&st),
            hair: style::hair_tile(&st),
            hair_front: style::hair_front_tile(&st).unwrap_or(0),
            hair_top: style::hair_top_tile(&st),
            beard: style::beard_tile(&st),
            shirt: style::shirt_tile(&st),
            trousers: style::trouser_tile(&st),
            boot: b("player_boot"),
            long_hair: st.hair_style == 3,
            skirt: st.legwear == 1,
            build: st.build,
        }
    }

    /// How a held item renders in a remote hand.
    pub(super) fn held_art(&self, item: Option<ItemId>) -> mobs::HeldArt {
        let Some(item) = item else {
            return mobs::HeldArt::None;
        };
        let def = self.content.reg.item(item);
        match def.places {
            Some(b) if !self.content.reg.block(b).cross => {
                mobs::HeldArt::Cube(self.content.reg.block(b).tiles)
            }
            _ => mobs::HeldArt::Sprite(def.icon),
        }
    }

    /// Advance a remote player's walk phase from their motion.
    pub(super) fn gait_for(&mut self, id: u32, pos: Vec3, dt: f32) -> (f32, f32) {
        let e = self
            .presentation
            .player_gait
            .entry(id)
            .or_insert((pos, 0.0));
        let hspeed = Vec3::new(pos.x - e.0.x, 0.0, pos.z - e.0.z).length() / dt.max(0.001);
        e.0 = pos;
        let amp = (hspeed / 3.5).clamp(0.0, 1.0);
        e.1 += hspeed * dt * 3.2;
        (e.1, amp)
    }

    /// The carried light of a held item, if any: an explicit item glow,
    /// or derived from the light of the block it places (torches).
    pub(super) fn held_glow(&self, item: ItemId) -> Option<(Vec3, f32)> {
        let def = self.content.reg.item(item);
        if let Some(g) = def.glow {
            return Some((Vec3::from(g), 14.0));
        }
        let b = def.places?;
        let bd = self.content.reg.block(b);
        if bd.light_emit == 0 {
            return None;
        }
        let emit = bd.light_emit.max(1) as f32;
        let color = Vec3::new(
            bd.light_rgb[0] as f32 / emit,
            bd.light_rgb[1] as f32 / emit,
            bd.light_rgb[2] as f32 / emit,
        );
        Some((color * 1.8 * (emit / 14.0), emit + 2.0))
    }

    /// Read the country at a spot and toast it: the prospector's
    /// verdict, shared by pick strikes and standing survey cairns.
    /// Compass octant of a great-circle bearing (clockwise from local north).
    pub(super) fn octant_bearing(bearing: Option<f64>) -> &'static str {
        let Some(bearing) = bearing else {
            return "here";
        };
        let names = [
            "north",
            "northeast",
            "east",
            "southeast",
            "south",
            "southwest",
            "west",
            "northwest",
        ];
        names[((bearing.to_degrees() + 22.5).rem_euclid(360.0) / 45.0) as usize]
    }

    /// Commit the sign editor: write the entity (host) or send it
    /// (guest), then return to play.
    pub(super) fn commit_sign(&mut self, pos: crate::planet::BlockPos) {
        let lines = self.ui_state.sign_lines.clone();
        if let Some(rc) = &self.multiplayer.remote {
            rc.client.send(&net::C2S::SetSign {
                pos,
                lines: lines.clone(),
            });
        }
        // Local worlds and hosts apply directly (the host broadcast
        // happens on the C2S path for guests' own edits).
        if self.multiplayer.remote.is_none() {
            self.server
                .world
                .insert_block_entity_at(pos, world::BlockEntity::Sign(world::SignState { lines }));
            if let Some(hst) = &mut self.multiplayer.host {
                hst.broadcast_sign_at(pos, &self.ui_state.sign_lines);
            }
        }
        self.set_screen(Screen::Playing);
    }

    pub(super) fn toast_prospect(&mut self, pos: crate::planet::SurfacePos) {
        let r = self.server.world.generator.prospect_at(pos);
        let mut lines: Vec<String> = Vec::new();
        if let Some(province) = &r.province_name {
            if let Some(bedrock) = r.bedrock {
                lines.push(format!("{province}: {} country.", bedrock.label()));
            } else {
                lines.push(province.clone());
            }
        }
        match r.pluton {
            Some(hit) if hit.distance == 0 => lines.push("Granite country underfoot.".into()),
            Some(hit) => lines.push(format!(
                "Granite country ~{} blocks {}.",
                hit.distance,
                Self::octant_bearing(hit.bearing)
            )),
            None => lines.push("No batholith in the pick's reach.".into()),
        }
        if let Some(hit) = r.volcano {
            if hit.distance == 0 {
                lines.push("Volcanic ground — you're standing on it.".into());
            } else {
                lines.push(format!(
                    "Volcanic rock ~{} blocks {}.",
                    hit.distance,
                    Self::octant_bearing(hit.bearing)
                ));
            }
        }
        if let Some(hit) = r.pipe {
            if hit.distance == 0 {
                lines.push("BLUE GROUND. A pipe under this very spot.".into());
            } else {
                lines.push(format!(
                    "Blue ground! A pipe ~{} blocks {}.",
                    hit.distance,
                    Self::octant_bearing(hit.bearing)
                ));
            }
        }
        if let Some(hit) = r.geode {
            if hit.distance == 0 {
                lines.push("A hollow ring underfoot — geode.".into());
            } else {
                lines.push(format!(
                    "A hollow ring ~{} blocks {}.",
                    hit.distance,
                    Self::octant_bearing(hit.bearing)
                ));
            }
        }
        for l in lines {
            self.toast(l);
        }
    }

    /// Break-sound family for a block, from its tool class.
    pub(super) fn break_mat(&self, b: registry::BlockId) -> BreakMat {
        match self.content.reg.block(b).tool {
            Some(ToolKind::Pickaxe) => BreakMat::Stone,
            Some(ToolKind::Axe) => BreakMat::Wood,
            Some(ToolKind::Shovel) => BreakMat::Soft,
            Some(ToolKind::Hoe) | None => BreakMat::Leafy,
        }
    }

    pub(super) fn has_ammo(&self, class: &str) -> bool {
        self.inventory
            .slots
            .iter()
            .flatten()
            .any(|s| self.content.reg.item(s.item).ammo.as_deref() == Some(class))
    }

    /// Remove one item of the ammo class; returns its id.
    pub(super) fn take_ammo(&mut self, class: &str) -> Option<ItemId> {
        let reg = self.content.reg.clone();
        for slot in self.inventory.slots.iter_mut() {
            if let Some(s) = slot
                && reg.item(s.item).ammo.as_deref() == Some(class)
            {
                let id = s.item;
                if s.count > 1 {
                    s.count -= 1;
                } else {
                    *slot = None;
                }
                return Some(id);
            }
        }
        None
    }

    /// Loose an arrow: charge in 0..1 scales damage and speed.
    pub(super) fn fire_bow(&mut self, bow: &registry::BowDef, charge: f32) {
        let reg = self.content.reg.clone();
        let arrow_id = if self.creative {
            reg.item_id("base:arrow")
        } else {
            self.take_ammo("arrow")
        };
        let Some(arrow_id) = arrow_id else { return };
        let dir = self.camera.local_forward();
        let eye = self.player.eye();
        if let Some(r) = &self.multiplayer.remote {
            r.client.send(&net::C2S::FireProjectile {
                direction: dir,
                charge,
            });
            if !self.creative {
                self.inventory.wear_tool(&reg, self.input.hotbar_sel);
            }
            self.sfx(Sfx::Bolt(0.8 + charge * 0.8));
            return;
        }
        self.server.world.spawn_projectile(mobs::Projectile {
            pos: eye
                .translated(dir * 0.4)
                .expect("projectile muzzle stays near the player")
                .pos,
            vel: dir * bow.speed * (0.6 + 0.4 * charge),
            tile: reg.item(arrow_id).icon,
            damage: bow.damage * (0.45 + 0.55 * charge),
            age: 0.0,
            from_player: true,
            // Arrows that stick into terrain are recoverable.
            drop_item: (!self.creative).then_some(arrow_id),
            owner: 0,
        });
        if !self.creative {
            self.inventory.wear_tool(&reg, self.input.hotbar_sel);
        }
        self.sfx(Sfx::Bolt(0.8 + charge * 0.8));
    }

    /// Nearest mob under the crosshair within reach, unless a solid block
    /// sits in front of it.
    pub(super) fn mob_in_crosshair(&self, hit: &Option<raycast::PlanetHit>) -> Option<usize> {
        let origin = self.player.eye();
        let dir = self.camera.tangent_forward();
        // A wall in the way shields the mob behind it (approximate the
        // wall distance by its block center).
        let wall_t = hit
            .as_ref()
            .map(|h| origin.distance_to(h.block.entity_center()) + 0.5)
            .unwrap_or(REACH);
        let mut best: Option<(usize, f32)> = None;
        for (i, m) in self.server.world.mobs().iter().enumerate() {
            let def = &self.content.reg.animals[m.species];
            if let Some(t) = m.ray_hit_from(def, origin, dir, REACH.min(wall_t))
                && best.is_none_or(|(_, bt)| t < bt)
            {
                best = Some((i, t));
            }
        }
        best.map(|(i, _)| i)
    }

    /// Remove dead mobs: roll their drop table, spill items, notify mods.
    pub(super) fn sweep_dead_mobs(&mut self) {
        let reg = self.content.reg.clone();
        let mut i = 0;
        while i < self.server.world.mob_count() {
            if self.server.world.mob(i).is_some_and(|mob| mob.health > 0.0) {
                i += 1;
                continue;
            }
            let m = self.server.world.remove_mob(i);
            let def = &reg.animals[m.species];
            if def.hostile {
                // Where a warden falls, the wild reclaims its own —
                // the death site banks bloom (dryads leave a sapling).
                if let Some(pos) = m.pos.block() {
                    self.server.world.wild_falls_at(&def.name, pos);
                }
            }
            if !def.hostile && !def.vehicle && !def.name.ends_with(":carcass") {
                // The wild counts its dead — wardens are not
                // individuals, and a TAMED animal is a household loss,
                // not a wild one (though betrayal is still noticed).
                // Vehicles are lumber; the wild never mourns a boat.
                // A carcass is already counted: the predator's kill
                // was nature's own.
                self.server.world.add_ire_at_surface(
                    crate::planet::SurfacePos::new(
                        m.pos.face(),
                        m.pos.u().floor() as u16,
                        m.pos.v().floor() as u16,
                    )
                    .expect("mob death has a canonical surface"),
                    if m.tamed { 1.0 } else { 2.0 },
                );
            }
            self.sfx(Sfx::MobDeath(def.sound_pitch));
            let (tile, at) = (
                def.tile,
                m.pos
                    .translated(Vec3::new(0.0, 0.5, 0.0))
                    .expect("death effect stays beside the mob")
                    .pos
                    .render_pos(),
            );
            self.juice_burst(at, tile, 12, 2.0);
            // A laden carrier spills its pack where it falls.
            if let Some(cargo) = &m.cargo {
                for st in cargo.iter().flatten() {
                    let at = m
                        .pos
                        .translated(Vec3::new(0.0, 0.6, 0.0))
                        .expect("cargo spills beside its carrier")
                        .pos;
                    self.drop_stack_at(*st, at);
                }
            }
            if m.growth < 1.0 {
                continue; // the young return nothing (you monster)
            }
            for (item, min, max) in &def.drops {
                let n = min + (self.rand01() * (*max - *min + 1) as f32) as u32;
                let n = n.min(*max);
                if n == 0 {
                    continue;
                }
                let stack = ItemStack::new(&reg, *item, n);
                if let Some(ledger) = &mut self.server.world.material_ledger
                    && let Err(error) =
                        ledger.record_external_stack(&reg, stack, "wild creature drop")
                {
                    eprintln!("materials: creature drop accounting failed: {error}");
                }
                if m.last_hit_by != 0 {
                    // A guest's kill: their loot crosses the wire.
                    self.server.world.queue_give(m.last_hit_by, stack);
                    continue;
                }
                let a = self.rand01() * std::f32::consts::TAU;
                let v = Vec3::new(a.cos() * 1.2, 2.5, a.sin() * 1.2);
                self.interaction.items.push(ItemEntity::new(
                    m.pos
                        .translated(Vec3::new(0.0, def.height * 0.5, 0.0))
                        .expect("mob loot begins beside the mob")
                        .pos,
                    v,
                    *item,
                    n,
                ));
            }
            if self.content.scripts.wants("on_animal_killed") {
                self.content.scripts.dispatch(
                    &self.server.world,
                    "on_animal_killed",
                    (
                        def.name.clone(),
                        m.pos.face().name().to_string(),
                        m.pos.u().floor() as i64,
                        m.pos.y().floor() as i64,
                        m.pos.v().floor() as i64,
                    ),
                );
                self.apply_script_cmds();
            }
        }
    }

    /// Mining and placing while playing.
    pub(super) fn interact(&mut self, dt: f32) {
        let reg = self.content.reg.clone();
        let hit = raycast::raycast_at(
            &self.server.world,
            self.player.eye(),
            self.camera.local_forward(),
            REACH,
        );
        let held = self.inventory.slots[self.input.hotbar_sel].map(|s| s.item);

        // The bucket: scoop a full water cell or pour it back — the
        // A boat in hand launches onto struck water.
        if held.is_some()
            && held == reg.item_id("base:boat")
            && self.input.right_held
            && self.input.action_cooldown <= 0.0
            && let Some(w) = raycast::raycast_water_at(
                &self.server.world,
                self.player.eye(),
                self.camera.local_forward(),
                REACH,
            )
        {
            let pos = w.block;
            if reg.is_water(self.server.world.get_block_at(pos))
                && let Some(bi) = reg.animal_id("base:boat")
                && (self.creative || self.inventory.take_one(self.input.hotbar_sel).is_some())
            {
                let mut boat = mobs::Mob::new_at(
                    bi,
                    crate::planet::EntityPos::new(
                        pos.face(),
                        f32::from(pos.u()) + 0.5,
                        f32::from(pos.y()) + 0.8,
                        f32::from(pos.v()) + 0.5,
                    )
                    .expect("a launched boat is inside its water cell"),
                    self.camera.yaw,
                );
                boat.health = reg.animals[bi].health;
                boat.tamed = true; // vehicles are born ours
                self.server.world.spawn_mob(boat);
                self.sfx(Sfx::Place);
                self.input.action_cooldown = 0.5;
                return;
            }
        }
        // cell moves with you, it never multiplies. Guests request and
        // the host's echo applies the world side; the bucket swap is
        // local (inventories are player-owned).
        if held.is_some() && held == reg.item_id("base:bucket") {
            if self.input.right_held
                && self.input.action_cooldown <= 0.0
                && let Some(w) = raycast::raycast_water_at(
                    &self.server.world,
                    self.player.eye(),
                    self.camera.local_forward(),
                    REACH,
                )
            {
                let pos = w.block;
                let b = self.server.world.get_block_at(pos);
                // Either fluid fills the bucket — a full cell only.
                if reg.fluid_volume(b) == Some(8) {
                    let water_class = self
                        .server
                        .world
                        .water_mass_at(pos)
                        .map(|mass| mass.water_class());
                    let full_item = if reg.is_lava(b) {
                        reg.item_id("base:bucket_lava")
                    } else {
                        reg.item_id(match water_class {
                            Some(crate::planet_atlas::WaterClass::Brackish) => {
                                "base:bucket_brackish"
                            }
                            Some(crate::planet_atlas::WaterClass::Salt) => "base:bucket_salt",
                            _ => "base:bucket_water",
                        })
                    };
                    let moved = if let Some(r) = &self.multiplayer.remote {
                        r.client.send(&net::C2S::Scoop { pos });
                        true
                    } else if reg.is_lava(b) {
                        self.server.world.set_block_at(pos, AIR);
                        true
                    } else {
                        self.server.world.scoop_water_at(pos).is_some()
                    };
                    if moved && let Some(full) = full_item {
                        self.inventory.slots[self.input.hotbar_sel] =
                            Some(ItemStack::new(&reg, full, 1));
                    }
                    self.input.action_cooldown = 0.25;
                    self.sfx(Sfx::Splash);
                }
            }
            return;
        }
        let held_water_class = if held == reg.item_id("base:bucket_water") {
            Some(crate::planet_atlas::WaterClass::Fresh)
        } else if held == reg.item_id("base:bucket_brackish") {
            Some(crate::planet_atlas::WaterClass::Brackish)
        } else if held == reg.item_id("base:bucket_salt") {
            Some(crate::planet_atlas::WaterClass::Salt)
        } else {
            None
        };
        if let Some(water_class) = held_water_class {
            if self.input.right_held
                && self.input.action_cooldown <= 0.0
                && let Some(h) = &hit
            {
                let pos = h.adjacent;
                if self.server.world.get_block_at(pos) == AIR && !self.player.overlaps_block_at(pos)
                {
                    if let Some(r) = &self.multiplayer.remote {
                        r.client.send(&net::C2S::Place { pos });
                    } else {
                        self.server.world.place_portable_water_at(pos, water_class);
                    }
                    if let Some(empty) = reg.item_id("base:bucket") {
                        self.inventory.slots[self.input.hotbar_sel] =
                            Some(ItemStack::new(&reg, empty, 1));
                    }
                    self.input.action_cooldown = 0.25;
                    self.sfx(Sfx::Splash);
                }
            }
            return;
        }
        if held.is_some() && held == reg.item_id("base:bucket_lava") {
            if self.input.right_held
                && self.input.action_cooldown <= 0.0
                && let Some(h) = &hit
            {
                let pos = h.adjacent;
                if self.server.world.get_block_at(pos) == AIR && !self.player.overlaps_block_at(pos)
                {
                    if let Some(r) = &self.multiplayer.remote {
                        r.client.send(&net::C2S::Place { pos });
                    } else {
                        let lava = reg.lava_for_volume(8);
                        self.server.world.place_block_at(pos, lava);
                    }
                    if let Some(empty) = reg.item_id("base:bucket") {
                        self.inventory.slots[self.input.hotbar_sel] =
                            Some(ItemStack::new(&reg, empty, 1));
                    }
                    self.input.action_cooldown = 0.25;
                    self.sfx(Sfx::Splash);
                }
            }
            return;
        }

        // Bow: hold right to draw, release to loose (0.25 s minimum).
        let bow_def = held.and_then(|i| reg.item(i).bow.clone());
        if let Some(bow) = bow_def {
            if self.input.right_held && (self.creative || self.has_ammo("arrow")) {
                self.interaction.bow_draw += dt;
            } else {
                if self.interaction.bow_draw >= 0.25 {
                    let charge = ((self.interaction.bow_draw - 0.25) / 0.75).clamp(0.0, 1.0);
                    self.fire_bow(&bow, charge);
                }
                self.interaction.bow_draw = 0.0;
            }
        } else if self.interaction.bow_draw > 0.0 {
            self.interaction.bow_draw = 0.0; // switched away mid-draw
        }

        // The line in the water: the water decides when. A bite opens
        // a short window announced by a splash; miss it and the wait
        // begins again.
        let rod_held = held.is_some_and(|i| reg.item(i).name == "base:fishing_rod");
        if !rod_held {
            self.interaction.fishing = None;
        } else if let Some((bobber, mut wait, mut bite)) = self.interaction.fishing {
            if bite > 0.0 {
                bite -= dt;
                if bite <= 0.0 {
                    // Missed it: the water loses interest for a while.
                    wait = 4.0 + self.rand01() * 8.0;
                }
            } else {
                wait -= dt;
                if wait <= 0.0 {
                    bite = 1.4;
                    let tile = reg.block(reg.water_block(0)).tiles[0];
                    self.juice_burst(bobber.render_pos(), tile, 8, 1.4);
                    self.sfx(Sfx::Splash);
                }
            }
            self.interaction.fishing = Some((bobber, wait, bite));
        }

        // Archaeology and regional salvage: sweeping a remnant or sifting
        // ordinary ground is a slow, careful channel.
        let brush_held = held.is_some_and(|i| reg.item(i).brush_tool);
        let brush_target = hit.as_ref().map(|h| h.block).filter(|t| {
            brush_held
                && (reg
                    .block(self.server.world.get_block_at(*t))
                    .brush
                    .is_some()
                    || self.server.world.can_sift_salvage_at(*t))
        });
        if let (true, Some(target)) = (self.input.right_held, brush_target) {
            if self.interaction.brush_target != Some(target) {
                self.interaction.brush_target = Some(target);
                self.interaction.brushing = 0.0;
            }
            self.interaction.brushing += dt;
            if self.interaction.brushing >= 1.5 {
                self.interaction.brushing = 0.0;
                self.interaction.brush_target = None;
                if let Some(rc) = &self.multiplayer.remote {
                    // The host rolls the find and Gives it straight to
                    // us; the BlockSet echo swaps the remnant out.
                    rc.client.send(&net::C2S::BrushBlock { pos: target });
                    if !self.creative {
                        self.inventory.wear_tool(&reg, self.input.hotbar_sel);
                    }
                    return;
                }
                let archaeology = reg
                    .block(self.server.world.get_block_at(target))
                    .brush
                    .is_some();
                let found = if archaeology {
                    let mut r = self.rng;
                    let found = self.server.world.brush_block_at(target, &mut r);
                    self.rng = r;
                    found
                } else {
                    match self.server.world.sift_salvage_at(target) {
                        Ok(found) => found,
                        Err(error) => {
                            eprintln!("materials: regional salvage recovery failed: {error}");
                            None
                        }
                    }
                };
                if let Some(stack) = found {
                    let center = crate::planet::EntityPos::new(
                        target.face(),
                        f32::from(target.u()) + 0.5,
                        f32::from(target.y()) + 0.6,
                        f32::from(target.v()) + 0.5,
                    )
                    .expect("brushed item begins inside its source cell");
                    let mut ent =
                        ItemEntity::new(center, Vec3::new(0.0, 2.0, 0.0), stack.item, stack.count);
                    // Old tools surface as worn as they were buried.
                    if stack.durability < reg.item(stack.item).durability {
                        ent.durability = stack.durability;
                    }
                    self.interaction.items.push(ent);
                    self.sfx(Sfx::Pickup);
                    if !archaeology {
                        self.toast("The brush turns up usable buried stock.".into());
                    }
                } else if !archaeology {
                    self.toast("Nothing recoverable gathers in this ground yet.".into());
                }
                if !self.creative {
                    self.inventory.wear_tool(&reg, self.input.hotbar_sel);
                }
            }
            return;
        } else {
            self.interaction.brushing = 0.0;
            self.interaction.brush_target = None;
        }

        // Station work is a held channel: hammer strikes at the anvil,
        // bare-hand turns at the quern. The def decides the tool.
        let anvil_target = hit.as_ref().map(|h| h.block).filter(|t| {
            let station = reg
                .block(self.server.world.get_block_at(*t))
                .interaction
                .clone();
            let Some(station) = station else { return false };
            // Powered stations take their strikes from the shaft
            // line; hands only load and unload them.
            if world::station_powered(&station) {
                return false;
            }
            let rested = match self.server.world.block_entity_at(t) {
                Some(world::BlockEntity::Anvil(a)) => a.bloom,
                _ => None,
            };
            let Some(rested) = rested else { return false };
            let Some(def) = reg
                .worked
                .iter()
                .find(|w| w.input == rested.item && w.station == station)
            else {
                return false;
            };
            if def.needs_hammer {
                held.is_some_and(|i| reg.item(i).hammer)
            } else {
                held.is_none()
            }
        });
        if let (true, Some(target)) = (self.input.right_held, anvil_target) {
            if self.interaction.anvil_pos != Some(target) {
                self.interaction.anvil_pos = Some(target);
                self.interaction.anvil_work = 0.0;
            }
            self.interaction.anvil_work += dt;
            if self.interaction.anvil_work >= 2.0 {
                self.interaction.anvil_work = 0.0;
                self.sfx(Sfx::Break(BreakMat::Stone));
                let top_pos = crate::planet::EntityPos::new(
                    target.face(),
                    f32::from(target.u()) + 0.5,
                    f32::from(target.y()) + 1.05,
                    f32::from(target.v()) + 0.5,
                )
                .expect("station effects remain above their source");
                let top = top_pos.render_pos();
                if self.presentation.juice {
                    if held.is_some_and(|i| reg.item(i).hammer) {
                        // The promised sparks: embers ring off the bloom.
                        let v = self.vary();
                        self.sfx_vol(Sfx::Spark, v.min(1.0));
                        let ember = *atlas::builtin_slots().get("ember").unwrap_or(&0);
                        self.juice_burst(top, ember, 8, 1.8);
                    } else {
                        let v = self.vary();
                        self.sfx_vol(Sfx::Grind, v.min(1.0));
                        let b = self.server.world.get_block_at(target);
                        let tile = reg.block(b).tiles[2];
                        self.juice_puff(top, tile, 3);
                    }
                }
                if !self.creative && held.is_some() {
                    self.inventory.wear_tool(&reg, self.input.hotbar_sel);
                }
                if let Some(rc) = &self.multiplayer.remote {
                    // The host counts strikes and Gives the bar.
                    rc.client.send(&net::C2S::AnvilStrike { pos: target });
                } else {
                    let Some(out) = self.server.world.anvil_strike_at(target) else {
                        return;
                    };
                    let center = crate::planet::EntityPos::new(
                        target.face(),
                        f32::from(target.u()) + 0.5,
                        f32::from(target.y()) + 1.0,
                        f32::from(target.v()) + 0.5,
                    )
                    .expect("worked item remains above its station");
                    self.interaction.items.push(ItemEntity::new(
                        center,
                        Vec3::new(0.0, 2.0, 0.0),
                        out.item,
                        out.count,
                    ));
                    self.sfx(Sfx::Craft);
                }
            }
            return;
        } else {
            self.interaction.anvil_work = 0.0;
            self.interaction.anvil_pos = None;
        }

        // Attacking: a mob in the crosshair takes the swing before the
        // block behind it. Held tools/swords set the damage.
        if self.input.left_held
            && let Some(mi) = self.mob_in_crosshair(&hit)
        {
            self.interaction.breaking = None;
            if self.input.attack_cooldown <= 0.0 {
                self.input.attack_cooldown = 0.35;
                self.presentation.swing = 1.0;
                let Some(mob) = self.server.world.mob(mi) else {
                    return;
                };
                let (sp, mob_id, mob_pos) = (mob.species, mob.id, mob.pos);
                let pitch = reg.animals[sp].sound_pitch;
                if let Some(r) = &self.multiplayer.remote {
                    r.client.send(&net::C2S::AttackMob { id: mob_id });
                    if let Some(mob) = self.server.world.mob_mut(mi) {
                        mob.hurt_flash = 0.35; // feedback
                    }
                    if self.presentation.juice {
                        self.presentation.hitch = 0.06;
                    }
                    let at = mob_pos
                        .translated(Vec3::new(0.0, 0.5, 0.0))
                        .expect("mob hit effect stays beside the mob")
                        .pos
                        .render_pos();
                    self.juice_burst(at, reg.animals[sp].tile, 5, 1.6);
                    self.sfx(Sfx::MobHurt(pitch));
                    self.survival.hunger = (self.survival.hunger - 0.01).max(0.0);
                    if !self.creative {
                        self.inventory.wear_tool(&reg, self.input.hotbar_sel);
                    }
                    self.input.attack_cooldown = 0.35;
                    return;
                }
                let def = reg.animals[sp].clone();
                if let Some(mob) = self.server.world.mob_mut(mi) {
                    let dmg = held.map(|i| reg.item(i).damage).unwrap_or(1.0);
                    mob.hurt(&def, dmg, self.player.eye());
                }
                if self.presentation.juice {
                    self.presentation.hitch = 0.06;
                }
                let at = mob_pos
                    .translated(Vec3::new(0.0, 0.5, 0.0))
                    .expect("mob hit effect stays beside the mob")
                    .pos
                    .render_pos();
                self.juice_burst(at, def.tile, 5, 1.6);
                self.sfx(Sfx::MobHurt(pitch));
                self.survival.hunger = (self.survival.hunger - 0.01).max(0.0);
                if !self.creative {
                    self.inventory.wear_tool(&reg, self.input.hotbar_sel);
                }
            }
            return;
        }

        // Hold-to-mine; tools speed up matching blocks and wear down.
        if self.input.left_held {
            if let Some(h) = &hit {
                let target = h.block;
                let b = self.server.world.get_block_at(target);
                let hardness = if self.creative {
                    // Creative breaks anything instantly — except the
                    // unbreakable (the world's floor stays a floor).
                    reg.block(b).hardness.map(|_| 0.0001)
                } else {
                    reg.effective_hardness(b, held)
                };
                if let Some(hardness) = hardness {
                    let progress = match self.interaction.breaking {
                        Some((t, p)) if t == target => p + dt / hardness.max(0.0001),
                        _ => dt / hardness.max(0.0001),
                    };
                    if progress >= 1.0 {
                        // Cancellable mod event.
                        let allow = if self.content.scripts.wants("on_block_break") {
                            let name = reg.block(b).name.clone();
                            let ok = self.content.scripts.dispatch(
                                &self.server.world,
                                "on_block_break",
                                (
                                    target.face().name().to_string(),
                                    target.u() as i64,
                                    target.y() as i64,
                                    target.v() as i64,
                                    name,
                                ),
                            );
                            self.apply_script_cmds();
                            ok
                        } else {
                            true
                        };
                        self.interaction.breaking = None;
                        if allow && self.multiplayer.remote.is_some() {
                            // Guests request; the echo applies the change.
                            if let Some(r) = &self.multiplayer.remote {
                                r.client.send(&net::C2S::Break { pos: target });
                            }
                            self.survival.hunger = (self.survival.hunger - 0.008).max(0.0);
                            self.sfx(Sfx::Break(self.break_mat(b)));
                            self.juice_burst(
                                target.entity_center().render_pos(),
                                self.content.reg.block(b).tiles[0],
                                10,
                                2.2,
                            );
                            if !self.creative {
                                self.inventory.wear_tool(&reg, self.input.hotbar_sel);
                            }
                            return;
                        }
                        if allow {
                            self.survival.hunger = (self.survival.hunger - 0.008).max(0.0);
                            let sheared = held.is_some_and(|item| reg.item(item).shears)
                                && reg.block(b).name.contains("leaves");
                            let result = self
                                .server
                                .world
                                .break_block_at(
                                    target,
                                    held,
                                    !self.creative && !sheared,
                                    !self.creative,
                                )
                                .expect("mining target was validated before completion");
                            let b = result.block;
                            self.sfx(Sfx::Break(self.break_mat(b)));
                            self.juice_burst(
                                target.entity_center().render_pos(),
                                self.content.reg.block(b).tiles[0],
                                10,
                                2.2,
                            );
                            if !self.creative {
                                self.inventory.wear_tool(&reg, self.input.hotbar_sel);
                            }
                            // Shears: leaves come off whole.
                            if sheared
                                && !self.creative
                                && let Some(item) = reg.item_id(&reg.block(b).name)
                            {
                                let center = target.entity_at_height(0.3);
                                self.interaction.items.push(ItemEntity::new(
                                    center,
                                    Vec3::new(0.0, 2.2, 0.0),
                                    item,
                                    1,
                                ));
                            }
                            if let Some(drop) = result.drop {
                                let center = target.entity_at_height(0.3);
                                let a = self.rand01() * std::f32::consts::TAU;
                                let v = Vec3::new(a.cos() * 1.2, 2.2, a.sin() * 1.2);
                                self.interaction
                                    .items
                                    .push(ItemEntity::new(center, v, drop.item, drop.count));
                            }
                            // Chance extras (leaves drop saplings).
                            if let Some((item, ch)) = reg.block(b).bonus_drop
                                && !self.creative
                                && self.rand01() < ch
                            {
                                let center = target.entity_at_height(0.3);
                                let a = self.rand01() * std::f32::consts::TAU;
                                let v = Vec3::new(a.cos() * 1.2, 2.2, a.sin() * 1.2);
                                self.interaction
                                    .items
                                    .push(ItemEntity::new(center, v, item, 1));
                            }
                        }
                    } else {
                        let stage_before =
                            (self.interaction.breaking.map(|(_, p)| p).unwrap_or(0.0) * 4.0) as i32;
                        self.interaction.breaking = Some((target, progress));
                        // Chips fly as each crack stage lands.
                        if (progress * 4.0) as i32 > stage_before {
                            self.juice_burst(
                                target.entity_center().render_pos(),
                                self.content.reg.block(b).tiles[0],
                                2,
                                1.2,
                            );
                        }
                        // Keep the arm swinging while we chip away.
                        if self.presentation.swing <= 0.0 {
                            self.presentation.swing = 1.0;
                        }
                    }
                } else {
                    self.interaction.breaking = None;
                }
            } else {
                self.interaction.breaking = None;
            }
        } else {
            self.interaction.breaking = None;
        }

        // Right click: interact with the targeted block (crafting table),
        // otherwise place the selected block.
        // Feeding wildlife: right-click an adult with its favorite food.
        if self.input.right_held && self.input.action_cooldown <= 0.0 {
            if let Some(mi) = self.mob_in_crosshair(&hit) {
                let Some(mob) = self.server.world.mob(mi) else {
                    return;
                };
                let (sp, mob_id, growth, breed_cd, fed) =
                    (mob.species, mob.id, mob.growth, mob.breed_cd, mob.fed);
                let (tamed, led_by, has_cargo) = (mob.tamed, mob.led_by, mob.cargo.is_some());
                let def = &reg.animals[sp];
                let def_carrier = def.carrier;
                let def_label = def.label.clone();
                // Feeding: breeds as ever, and repeated meals TAME —
                // a tamed animal never flees people and takes a lead.
                if let (Some(bf), Some(h)) = (def.breed_food, held)
                    && bf == h
                    && !def.hostile
                    && growth >= 1.0
                {
                    let can_breed = breed_cd <= 0.0 && !fed;
                    let can_tame = !tamed;
                    if (can_breed || can_tame)
                        && (self.creative
                            || self.inventory.take_one(self.input.hotbar_sel).is_some())
                    {
                        // Guests request; local change is the
                        // prediction until the snapshot echoes it.
                        if let Some(rc) = &self.multiplayer.remote {
                            rc.client.send(&net::C2S::FeedMob { id: mob_id });
                        } else if !self.creative
                            && let Err(error) = self
                                .server
                                .world
                                .record_consumed_stacks([ItemStack::new(&reg, h, 1)])
                        {
                            eprintln!("materials: animal feed accounting failed: {error}");
                        }
                        let mut now_tamed = false;
                        if let Some(mob) = self.server.world.mob_mut(mi) {
                            if can_tame {
                                now_tamed = mob.feed_tame();
                            }
                            if can_breed {
                                mob.fed = true;
                            }
                            mob.calm = 30.0;
                        }
                        if now_tamed {
                            self.toast(format!("The {def_label} trusts you now."));
                        }
                        self.input.action_cooldown = 0.4;
                        self.sfx(Sfx::Pickup);
                        return;
                    }
                }
                // The lead: attach to a tamed animal, click again to
                // release (the strip comes back).
                if tamed && led_by == Some(0) && self.multiplayer.remote.is_none() {
                    if let Some(mob) = self.server.world.mob_mut(mi) {
                        mob.led_by = None;
                    }
                    if let Some(lead) = reg.item_id("base:lead") {
                        let left = self.inventory.add(&reg, lead, 1);
                        if left > 0 {
                            self.drop_stack(ItemStack::new(&reg, lead, 1));
                        }
                    }
                    self.input.action_cooldown = 0.4;
                    self.sfx(Sfx::Click);
                    return;
                }
                if tamed
                    && led_by.is_none()
                    && held == reg.item_id("base:lead")
                    && (self.creative || self.inventory.take_one(self.input.hotbar_sel).is_some())
                {
                    if let Some(rc) = &self.multiplayer.remote {
                        rc.client.send(&net::C2S::LeadMob { id: mob_id });
                    } else if let Some(mob) = self.server.world.mob_mut(mi) {
                        mob.led_by = Some(0);
                    }
                    self.input.action_cooldown = 0.4;
                    self.sfx(Sfx::Click);
                    return;
                }
                // Saddlebags: a tamed carrier takes a pack.
                if tamed
                    && def_carrier
                    && !has_cargo
                    && held == reg.item_id("base:saddlebags")
                    && (self.creative || self.inventory.take_one(self.input.hotbar_sel).is_some())
                {
                    if let Some(rc) = &self.multiplayer.remote {
                        rc.client.send(&net::C2S::SaddleMob { id: mob_id });
                    } else if let Some(mob) = self.server.world.mob_mut(mi) {
                        mob.cargo = Some(Default::default());
                    }
                    self.input.action_cooldown = 0.4;
                    self.sfx(Sfx::Place);
                    return;
                }
                // Step aboard a vehicle (empty-handed).
                if def.vehicle && held.is_none() {
                    let free = self
                        .server
                        .world
                        .mob_by_id(mob_id)
                        .is_some_and(|m| m.ridden_by.is_none());
                    if free {
                        if let Some(rc) = &self.multiplayer.remote {
                            rc.client.send(&net::C2S::RideMob {
                                id: mob_id,
                                mount: true,
                            });
                        } else if let Some(m) = self.server.world.mob_by_id_mut(mob_id) {
                            m.ridden_by = Some(0);
                        }
                        self.interaction.riding = Some(mob_id);
                        self.toast("Aboard. Jump to step off.".to_string());
                        self.input.action_cooldown = 0.4;
                        return;
                    }
                }
                // Open the pack.
                if tamed && has_cargo {
                    if let Some(rc) = &self.multiplayer.remote {
                        rc.client.send(&net::C2S::OpenMobCargo { id: mob_id });
                    } else {
                        self.set_screen(Screen::MobCargo(mob_id));
                    }
                    self.input.action_cooldown = 0.3;
                    return;
                }
            }
            // A covered log pile takes a warden's ember: the clamp.
            let ember = reg.item_id("base:ember");
            if held == ember
                && let Some(hb) = &hit
            {
                let pos = hb.block;
                let tb = self.server.world.get_block_at(pos);
                let is_log = reg.tags.get("base:logs").is_some_and(|l| {
                    reg.item_id(&reg.block(tb).name)
                        .is_some_and(|i| l.contains(&i))
                });
                if is_log {
                    if let Some(rc) = &self.multiplayer.remote {
                        self.inventory.take_one(self.input.hotbar_sel);
                        rc.client.send(&net::C2S::LightClamp { pos });
                    } else {
                        match self.server.world.try_light_clamp_at(pos) {
                            Ok(n) => {
                                self.inventory.take_one(self.input.hotbar_sel);
                                self.sfx(Sfx::Bolt(0.8));
                                self.toast(format!(
                                    "The clamp smolders - {n} logs, {:.0} minutes.",
                                    n as f32 * world::CLAMP_SECS_PER_LOG / 60.0
                                ));
                            }
                            Err(e) => self.toast(e.to_string()),
                        }
                    }
                    self.input.action_cooldown = 0.5;
                    return;
                }
            }
            // The prospector's pick: strike bare rock, read the country.
            if held == reg.item_id("base:prospect_pick")
                && let Some(hb) = &hit
            {
                let pos = hb.block;
                let tb = self.server.world.get_block_at(pos);
                if self.server.world.reg.is_solid(tb) {
                    self.toast_prospect(pos.surface());
                    if !self.creative {
                        self.inventory.wear_tool(&reg, self.input.hotbar_sel);
                    }
                    self.sfx(Sfx::Bolt(1.2));
                    self.input.action_cooldown = 0.8;
                    return;
                }
            }
            // Throwables (snowballs): loosed from the hand.
            if let Some(speed) = held.and_then(|i| reg.item(i).throw_speed)
                && (self.creative || self.inventory.take_one(self.input.hotbar_sel).is_some())
            {
                let item = held.unwrap();
                let dir = self.camera.local_forward();
                if let Some(rc) = &self.multiplayer.remote {
                    rc.client.send(&net::C2S::FireProjectile {
                        direction: dir,
                        charge: 1.0,
                    });
                } else {
                    let pos = self
                        .player
                        .eye()
                        .translated(dir * 0.4)
                        .expect("throwing muzzle stays near the player")
                        .pos;
                    let vel = dir * speed;
                    let tile = reg.item(item).icon;
                    self.server.world.spawn_projectile(mobs::Projectile {
                        pos,
                        vel,
                        tile,
                        damage: 0.0,
                        age: 0.0,
                        from_player: true,
                        drop_item: None,
                        owner: 0,
                    });
                }
                self.sfx(Sfx::Bolt(1.6));
                self.input.action_cooldown = 0.35;
                return;
            }
            // A cutting knows where it is needed: held up, it gives a
            // bearing to the nearest ground that would take it. That is
            // the only navigation that works at province range, where
            // countries are 900 blocks apart and you see a few hundred.
            //
            // NOT while pointing at a heart. This arm runs before the
            // block-interaction pass, so without that guard reading the
            // bearing would shadow PLANTING the thing — the whole
            // restoration verb, silently gone.
            let at_heart = hit.as_ref().is_some_and(|h| {
                reg.block(self.server.world.get_block_at(h.block))
                    .interaction
                    .as_deref()
                    == Some("heart")
            });
            if !at_heart && held.is_some_and(|i| world::seed_nature(&reg.item(i).name).is_some()) {
                let line = self.server.world.seed_bearing_at(self.player.pos);
                self.toast(line);
                self.sfx(Sfx::Click);
                self.input.action_cooldown = 0.6;
                return;
            }
            // Etched tablets: the lost takers speak.
            if held.is_some_and(|i| reg.item(i).tablet) {
                self.read_tablet();
                self.input.action_cooldown = 0.6;
                return;
            }
            // Bedroll: camp until dawn.
            if held.is_some_and(|i| reg.item(i).bedroll) {
                self.try_sleep();
                self.input.action_cooldown = 0.5;
                return;
            }
        }
        // Rod clicks live outside the block-hit path: open water is
        // rarely a solid target. Strike on a bite, reel in early, or
        // cast at the first water the look-ray touches.
        if self.input.right_held && self.input.action_cooldown <= 0.0 && rod_held {
            self.input.action_cooldown = 0.45;
            self.input.right_held = false;
            match self.interaction.fishing.take() {
                Some((bobber, _, bite)) if bite > 0.0 => {
                    // The strike: a real fish first, thin luck second.
                    let caught = self.server.world.catch_fish_near_at(bobber, 6.0).is_some()
                        || self.rand01() < 0.25;
                    if caught {
                        if let Some(fish) = reg.item_id("base:raw_fish") {
                            let left = self.inventory.add(&reg, fish, 1);
                            if left > 0 {
                                self.drop_stack(ItemStack::new(&reg, fish, left));
                            }
                        }
                        self.inventory.wear_tool(&reg, self.input.hotbar_sel);
                        self.sfx(Sfx::Pickup);
                    } else {
                        self.sfx(Sfx::Splash);
                    }
                }
                Some(_) => {} // reeled in empty
                None => {
                    let cast = raycast::raycast_water_at(
                        &self.server.world,
                        self.player.eye(),
                        self.camera.local_forward(),
                        14.0,
                    )
                    .filter(|hit| reg.is_water(self.server.world.get_block_at(hit.block)))
                    .map(|hit| hit.block.entity_at_height(0.9));
                    match cast {
                        Some(at) => {
                            self.interaction.fishing = Some((at, 3.0 + self.rand01() * 9.0, 0.0));
                            self.sfx(Sfx::Splash);
                        }
                        None => self.toast("Cast at water.".to_string()),
                    }
                }
            }
            return;
        }
        let held_is_food = held.is_some_and(|i| reg.item(i).food.is_some());
        if self.input.right_held
            && self.input.action_cooldown <= 0.0
            && !held_is_food
            && let Some(h) = &hit
        {
            let tb = self.server.world.get_block_at(h.block);
            // Harvestable blocks (berry bushes).
            if let Some((item, n, becomes)) = reg.block(tb).harvest {
                self.server.world.set_block_at(h.block, becomes);
                let left = self.inventory.add(&reg, item, n);
                if left > 0 {
                    self.drop_stack(ItemStack::new(&reg, item, left));
                }
                self.sfx(Sfx::Pickup);
                self.input.action_cooldown = 0.3;
                return;
            }
            // Fertilizer feeds the field it lands on: dung from the
            // pen, guano from the cave, compost from the heap.
            if let Some(hi) = held {
                let v = world::soil::fertilizer_value(&reg.item(hi).name);
                if v > 0 && self.server.world.feed_soil_at(h.block, v) {
                    self.inventory.take_one(self.input.hotbar_sel);
                    self.sfx(Sfx::Place);
                    self.input.action_cooldown = 0.3;
                    return;
                }
            }
            // The striker sets light to what you point it at. This is
            // the ONE place a fire is marked as a player's, and the
            // mark is inherited by everything it spreads to — so a
            // burn cannot change hands halfway down a hillside.
            if held.is_some_and(|i| reg.item(i).striker) {
                let f = h.adjacent;
                if reg.block(tb).burns > 0 && self.server.world.light_fire_at(f, true) {
                    self.inventory.wear_tool(&reg, self.input.hotbar_sel);
                    self.toast("It catches. It is yours now.".to_string());
                    self.sfx(Sfx::Place);
                    self.input.action_cooldown = 0.4;
                    return;
                }
                self.toast("Nothing here will take a light.".to_string());
                self.input.action_cooldown = 0.4;
                return;
            }
            // Hoe tills grass/dirt into farmland.
            if let (Some((ToolKind::Hoe, _, _)), Some(farm)) = (
                held.and_then(|i| reg.item(i).tool),
                reg.block_id("base:farmland"),
            ) {
                let name = reg.block(tb).name.as_str();
                if name == "base:grass" || name == "base:dirt" {
                    // The till reads the ground it came from: grass-fed
                    // loam starts richer than bare dirt (soil.rs).
                    let meta = self.server.world.till_meta_at(h.block);
                    self.server.world.set_block_meta_at(h.block, farm, meta);
                    self.server.world.initialize_tilled_soil_at(h.block);
                    self.inventory.wear_tool(&reg, self.input.hotbar_sel);
                    self.sfx(Sfx::Place);
                    self.input.action_cooldown = 0.3;
                    return;
                }
            }
            if self.content.scripts.wants("on_interact") {
                let name = reg.block(tb).name.clone();
                let allow = self.content.scripts.dispatch(
                    &self.server.world,
                    "on_interact",
                    (
                        h.block.face().name().to_string(),
                        h.block.u() as i64,
                        h.block.y() as i64,
                        h.block.v() as i64,
                        name,
                    ),
                );
                self.apply_script_cmds();
                if !allow {
                    self.input.right_held = false;
                    self.input.action_cooldown = 0.22;
                    return;
                }
            }
            match reg.block(tb).interaction.as_deref() {
                Some("crafting") => {
                    self.input.right_held = false;
                    self.interaction.craft_size = 3;
                    self.set_screen(Screen::Inventory);
                    return;
                }
                Some("furnace") => {
                    self.input.right_held = false;
                    self.server.world.ensure_block_entity_at(
                        h.block,
                        world::BlockEntity::Furnace(Default::default()),
                    );
                    self.set_screen(Screen::Furnace(h.block));
                    return;
                }
                Some("chest") if self.input.action_cooldown <= 0.0 => {
                    self.input.action_cooldown = 0.3;
                    if let Some(rc) = &self.multiplayer.remote {
                        rc.client.send(&net::C2S::OpenContainer { pos: h.block });
                        return;
                    }
                    let e = self.server.world.ensure_block_entity_at(
                        h.block,
                        world::BlockEntity::Chest(Default::default()),
                    );
                    if let world::BlockEntity::Chest(c) = e
                        && c.wild_owned
                    {
                        c.wild_owned = false;
                        self.server.world.add_ire_at_surface(h.block.surface(), 1.0);
                        self.toast("The wild keeps its trophies.".to_string());
                    }
                    self.set_screen(Screen::Chest(h.block));
                    return;
                }
                Some("heart") if self.input.action_cooldown <= 0.0 => {
                    self.input.action_cooldown = 0.5;
                    self.input.right_held = false;
                    let carried = held.and_then(|i| world::seed_nature(&reg.item(i).name));
                    let holding_seed = carried.is_some();
                    // A cutting from a living heart: the thing you
                    // carry across the world to wake a dead country.
                    if !holding_seed
                        && let Some(seed) = reg.item_id(world::seed_of_form(world::heart_form(
                            self.server.world.generator.biome_at(h.block.surface()),
                        )))
                        && self.server.world.take_heart_cutting_at(h.block.surface())
                    {
                        let left = self.inventory.add(&reg, seed, 1);
                        if left > 0 {
                            self.drop_stack(ItemStack::new(&reg, seed, left));
                        }
                        // Taking from the wild is taking, even gently.
                        self.server.world.add_ire_at_surface(h.block.surface(), 1.0);
                        self.toast(
                            "A cutting comes away in your hand. This country will \
                             remember that you took it."
                                .to_string(),
                        );
                        self.sfx(Sfx::Pickup);
                        return;
                    }
                    if holding_seed {
                        // What you carry decides what wakes: its own
                        // kind reawakens, a stranger's replaces.
                        match self.server.world.plant_heart_seed_from_at(h.block, carried) {
                            Some(refusal) => self.toast(refusal),
                            None => {
                                self.inventory.take_one(self.input.hotbar_sel);
                                self.toast(
                                    "You plant it in the ruin of the old heart.".to_string(),
                                );
                                self.sfx(Sfx::Place);
                            }
                        }
                        return;
                    }
                    // A dead site answers with the state of its ground
                    // and the work left on it. Bare-handed, at the one
                    // place the player is standing when they want to
                    // know: "this country is alone" alone taught
                    // nothing, and a scar you cannot read is a scar you
                    // walk away from.
                    let world = &self.server.world;
                    if world
                        .heart_at_surface(h.block.surface())
                        .is_some_and(|hh| hh.stage == 0)
                    {
                        let hp = world.heart_at_surface(h.block.surface()).unwrap().pos;
                        let (ready, total) = world.root_ground_ready_at(hp);
                        let want = (total as f32 * crate::world::ROOT_READY_FRAC).ceil() as u32;
                        self.toast(if ready >= want {
                            "Nothing answers. The ground is living again, though. Bring it a cutting from a heart still awake."
                                .to_string()
                        } else {
                            format!(
                                "Nothing answers. Around it, {ready} of {want} plots are living."
                            )
                        });
                        self.sfx(Sfx::Click);
                        return;
                    }
                    let line = match world.heart_at_surface(h.block.surface()) {
                        // It gave already. Saying so plainly is the
                        // point: the old silence read as a broken
                        // button rather than a spirit with nothing left
                        // to give this season.
                        Some(hh) if hh.stage == 2 && hh.regrow > 0.0 => {
                            "It has nothing more to give yet. Come back in a season."
                        }
                        Some(hh) if hh.stage == 2 && hh.strain > 4.0 => {
                            "Warm to the touch, and it flinches from your hand."
                        }
                        Some(hh) if hh.stage == 2 => "Warm to the touch. Something here is awake.",
                        Some(hh) if hh.stage == 1 => "It is cold, and it is going out.",
                        Some(_) => "Nothing answers. This country is alone.",
                        None => "Something stood here once.",
                    };
                    self.toast(line.to_string());
                    self.sfx(Sfx::Click);
                    return;
                }
                Some("compost") if self.input.action_cooldown <= 0.0 => {
                    self.input.action_cooldown = 0.3;
                    // A ripened heap hands over its compost bare-handed;
                    // a fresh one eats greens item by item.
                    if self.server.world.compost_take_at(h.block) {
                        if let Some(c) = reg.item_id("base:compost") {
                            let left = self.inventory.add(&reg, c, 2);
                            if left > 0 {
                                self.drop_stack(ItemStack::new(&reg, c, left));
                            }
                        }
                        self.sfx(Sfx::Pickup);
                        return;
                    }
                    if let Some(hi) = held {
                        let name = reg.item(hi).name.clone();
                        if self.server.world.compost_fill_at(h.block, &name) {
                            self.inventory.take_one(self.input.hotbar_sel);
                            if self.multiplayer.remote.is_none()
                                && let Err(error) = self
                                    .server
                                    .world
                                    .record_consumed_stacks([ItemStack::new(&reg, hi, 1)])
                            {
                                eprintln!("materials: compost feed accounting failed: {error}");
                            }
                            self.sfx(Sfx::Place);
                            return;
                        }
                    }
                    let fill = self.server.world.get_meta_at(h.block);
                    self.toast(if fill >= world::soil::COMPOST_FULL {
                        "The heap is cooking.".to_string()
                    } else {
                        format!(
                            "The heap wants greens ({fill}/{}).",
                            world::soil::COMPOST_FULL
                        )
                    });
                    return;
                }
                Some("offering") if self.input.action_cooldown <= 0.0 => {
                    self.input.action_cooldown = 0.3;
                    if let Some(rc) = &self.multiplayer.remote {
                        rc.client.send(&net::C2S::OpenContainer { pos: h.block });
                        return;
                    }
                    self.server.world.ensure_block_entity_at(
                        h.block,
                        world::BlockEntity::Offering(Default::default()),
                    );
                    self.set_screen(Screen::Offering(h.block));
                    return;
                }
                Some("stall") if self.input.action_cooldown <= 0.0 => {
                    self.input.action_cooldown = 0.3;
                    self.input.right_held = false;
                    if let Some(rc) = &self.multiplayer.remote {
                        rc.client.send(&net::C2S::OpenContainer { pos: h.block });
                        return;
                    }
                    // First open claims an unowned counter for the
                    // local player (the host's stall, by identity).
                    let my_id = identity::local_player_id(
                        &self.server.world.save_dir_for_saving(),
                        self.identity.device_id(),
                    )
                    .map(|p| p.0)
                    .unwrap_or([0; 16]);
                    let my_name = self.config.display_name.clone();
                    let e = self.server.world.ensure_block_entity_at(
                        h.block,
                        world::BlockEntity::Stall(Default::default()),
                    );
                    if let world::BlockEntity::Stall(st) = e
                        && st.owner == [0; 16]
                    {
                        st.owner = my_id;
                        st.owner_name = my_name;
                    }
                    self.set_screen(Screen::Stall(h.block));
                    return;
                }
                Some("smoker") if self.input.action_cooldown <= 0.0 => {
                    self.input.action_cooldown = 0.35;
                    let raws = reg.tags.get("base:raw_meats").cloned().unwrap_or_default();
                    let holding_raw = held.is_some_and(|h| raws.contains(&h));
                    let e = self.server.world.ensure_block_entity_at(
                        h.block,
                        world::BlockEntity::Smoker(Default::default()),
                    );
                    let world::BlockEntity::Smoker(sm) = e else {
                        return;
                    };
                    if holding_raw {
                        if let Some(slot) = sm.meat.iter_mut().find(|s| s.is_none()) {
                            let item = held.unwrap();
                            if self.creative
                                || self.inventory.take_one(self.input.hotbar_sel).is_some()
                            {
                                *slot = Some(ItemStack::new(&reg, item, 1));
                                self.sfx(Sfx::Place);
                                let torch_below = h.block.offset(0, -1, 0).is_some_and(|below| {
                                    Some(self.server.world.get_block_at(below))
                                        == reg.block_id("base:torch")
                                });
                                if !torch_below {
                                    self.toast(
                                        "The rack wants a torch burning beneath.".to_string(),
                                    );
                                }
                            }
                        } else {
                            self.toast("The rack is full.".to_string());
                        }
                        return;
                    }
                    // Empty-handed (or otherwise): take the cuts back.
                    let mut took: Option<ItemStack> = None;
                    if let world::BlockEntity::Smoker(sm) =
                        self.server.world.block_entity_mut_at(&h.block).unwrap()
                        && let Some(slot) = sm.meat.iter_mut().rev().find(|s| s.is_some())
                    {
                        took = slot.take();
                    }
                    if let Some(st) = took {
                        let left = self.inventory.add_stack(&reg, st);
                        if left > 0 {
                            self.drop_stack(ItemStack { count: left, ..st });
                        }
                        self.sfx(Sfx::Pickup);
                    }
                    return;
                }
                Some("sign") if self.input.action_cooldown <= 0.0 => {
                    // Reopen the editor with what's written.
                    self.input.action_cooldown = 0.3;
                    self.input.right_held = false;
                    let cur = match self.server.world.block_entity_at(&h.block) {
                        Some(world::BlockEntity::Sign(sg)) => sg.lines.clone(),
                        _ => Default::default(),
                    };
                    self.ui_state.sign_lines = cur;
                    self.ui_state.sign_line = 0;
                    self.set_screen(Screen::SignEdit(h.block));
                    return;
                }
                Some("waystone") if self.input.action_cooldown <= 0.0 => {
                    self.input.action_cooldown = 0.4;
                    self.read_waystone(h.block);
                    return;
                }
                Some("survey") if self.input.action_cooldown <= 0.0 => {
                    // A raised cairn is bought knowledge: anyone reads
                    // the surveyor's ground, no pick required — and a
                    // country's heart is the first thing worth knowing.
                    let report = self.server.world.heart_report_at(h.block.surface());
                    self.toast(report);
                    self.toast_prospect(h.block.surface());
                    self.sfx(Sfx::Click);
                    self.input.action_cooldown = 0.6;
                    return;
                }
                Some(
                    st @ ("anvil" | "quern" | "millstone" | "sawmill" | "lathe" | "iron_lathe"
                    | "boring"),
                ) if self.input.action_cooldown <= 0.0 => {
                    // Rest work with a click, take it back bare-handed.
                    // The held channels (hammer strikes, quern turns)
                    // run earlier and return before reaching this arm.
                    let table = world::worked_table_for(st);
                    let workable = held.is_some_and(|i| {
                        reg.worked
                            .iter()
                            .any(|w| w.input == i && w.station == table)
                    });
                    if workable {
                        self.input.action_cooldown = 0.25;
                        let stack = self.inventory.slots[self.input.hotbar_sel].unwrap();
                        if let Some(rc) = &self.multiplayer.remote {
                            rc.client.send(&net::C2S::AnvilPut { pos: h.block });
                            return;
                        }
                        let one = ItemStack { count: 1, ..stack };
                        if self.server.world.anvil_put_at(h.block, one) {
                            if !self.creative {
                                self.inventory.take_one(self.input.hotbar_sel);
                            }
                            self.sfx(Sfx::Place);
                        } else {
                            self.toast("It holds all it can.".to_string());
                        }
                        return;
                    }
                    if held.is_none() {
                        self.input.action_cooldown = 0.3;
                        if let Some(rc) = &self.multiplayer.remote {
                            rc.client.send(&net::C2S::AnvilTake { pos: h.block });
                            return;
                        }
                        if let Some(st) = self.server.world.anvil_take_at(h.block) {
                            let left = self.inventory.add_stack(&reg, st);
                            if left > 0 {
                                self.drop_stack(ItemStack { count: left, ..st });
                            }
                            self.sfx(Sfx::Pickup);
                        }
                        return;
                    }
                    return;
                }
                Some("separator") if self.input.action_cooldown <= 0.0 => {
                    // Powder and fuel in by hand; bare hands take the
                    // split back out (smoker rules, no screen).
                    self.input.action_cooldown = 0.3;
                    let powder = reg.item_id("base:rare_earth_powder");
                    // Separator persistence stores this bed as a count and
                    // returns charcoal on dismantling, so admitting arbitrary
                    // finite coal here would destroy its identity.
                    let is_fuel = held == reg.item_id("base:charcoal");
                    self.server.world.ensure_block_entity_at(
                        h.block,
                        world::BlockEntity::Multiblock(world::MachineInstance {
                            kind: world::multiblock::MachineKind::Separator,
                            ..Default::default()
                        }),
                    );
                    let valid = self.server.world.check_separator_at(h.block).is_some();
                    let Some(world::BlockEntity::Multiblock(sp)) =
                        self.server.world.block_entity_mut_at(&h.block)
                    else {
                        return;
                    };
                    if held.is_some() && held == powder {
                        if sp.powder >= 8 {
                            self.toast("The hopper is full.".to_string());
                            return;
                        }
                        if self.creative || self.inventory.take_one(self.input.hotbar_sel).is_some()
                        {
                            if let Some(world::BlockEntity::Multiblock(sp)) =
                                self.server.world.block_entity_mut_at(&h.block)
                            {
                                sp.powder += 1;
                            }
                            self.sfx(Sfx::Place);
                            if !valid {
                                self.toast("The separator wants its firebrick stack.".to_string());
                            }
                        }
                        return;
                    }
                    if is_fuel {
                        if sp.separator_fuel >= 8 {
                            self.toast("The firebed is full.".to_string());
                            return;
                        }
                        if self.creative || self.inventory.take_one(self.input.hotbar_sel).is_some()
                        {
                            if let Some(world::BlockEntity::Multiblock(sp)) =
                                self.server.world.block_entity_mut_at(&h.block)
                            {
                                sp.separator_fuel += 1;
                            }
                            self.sfx(Sfx::Place);
                        }
                        return;
                    }
                    if held.is_none() {
                        let (nd, ce) = (sp.neodymium, sp.cerium);
                        if nd == 0 && ce == 0 {
                            let (p, f) = (sp.powder, sp.separator_fuel);
                            self.toast(format!("Powder {p}, fuel {f}, nothing split yet."));
                            return;
                        }
                        if let Some(world::BlockEntity::Multiblock(sp)) =
                            self.server.world.block_entity_mut_at(&h.block)
                        {
                            sp.neodymium = 0;
                            sp.cerium = 0;
                        }
                        for (name, n) in [("base:neodymium", nd), ("base:cerium", ce)] {
                            if n > 0
                                && let Some(item) = reg.item_id(name)
                            {
                                let mut st = ItemStack::new(&reg, item, 1);
                                st.count = n;
                                let left = self.inventory.add_stack(&reg, st);
                                if left > 0 {
                                    self.drop_stack(ItemStack { count: left, ..st });
                                }
                            }
                        }
                        self.sfx(Sfx::Pickup);
                    }
                    return;
                }
                Some("firebox") if self.input.action_cooldown <= 0.0 => {
                    // Coal in at the door; bare hands read the gauges.
                    self.input.action_cooldown = 0.3;
                    let fuel = held.and_then(|i| reg.fuel_value(i));
                    let e = self.server.world.ensure_block_entity_at(
                        h.block,
                        world::BlockEntity::Steam(Default::default()),
                    );
                    let world::BlockEntity::Steam(s) = e else {
                        return;
                    };
                    if let Some((burn, _)) = fuel {
                        if s.fuel >= world::STEAM_FUEL_CAP {
                            self.toast("The firebox is banked full.".to_string());
                            return;
                        }
                        if self.creative || self.inventory.take_one(self.input.hotbar_sel).is_some()
                        {
                            if let Some(item) = held
                                && let Some(ledger) = &mut self.server.world.material_ledger
                            {
                                let materials = crate::materials::stack_materials(
                                    &reg,
                                    ItemStack::new(&reg, item, 1),
                                );
                                if let Err(error) = ledger.record_consumption(&materials) {
                                    eprintln!("materials: firebox fuel accounting failed: {error}");
                                }
                            }
                            let e = self.server.world.block_entity_mut_at(&h.block);
                            if let Some(world::BlockEntity::Steam(s)) = e {
                                s.fuel = (s.fuel + burn * 4.0).min(world::STEAM_FUEL_CAP);
                            }
                            self.sfx(Sfx::Place);
                        }
                        return;
                    }
                    let f = s.fuel as u32;
                    let blocks =
                        s.water.water_hu as f64 / crate::planet_atlas::HYDRO_UNITS_PER_BLOCK as f64;
                    let salinity = s.water.salinity();
                    self.toast(format!(
                        "Fire banked {f}s; boiler water {blocks:.2} blocks (salinity {}).",
                        salinity
                    ));
                    return;
                }
                Some(station @ ("bloomery" | "kiln" | "forge"))
                    if self.input.action_cooldown <= 0.0 =>
                {
                    self.input.action_cooldown = 0.3;
                    if let Some(rc) = &self.multiplayer.remote {
                        rc.client.send(&net::C2S::OpenContainer { pos: h.block });
                        return;
                    }
                    let (kind, screen) = match station {
                        "kiln" => (world::multiblock::MachineKind::Kiln, Screen::Kiln(h.block)),
                        "forge" => (
                            world::multiblock::MachineKind::Forge,
                            Screen::Bloomery(h.block),
                        ),
                        _ => (
                            world::multiblock::MachineKind::Bloomery,
                            Screen::Bloomery(h.block),
                        ),
                    };
                    let default = world::BlockEntity::Multiblock(world::MachineInstance {
                        kind,
                        ..Default::default()
                    });
                    self.server.world.ensure_block_entity_at(h.block, default);
                    self.set_screen(screen);
                    return;
                }
                _ => {}
            }
            let pos = h.adjacent;
            let place =
                self.inventory.slots[self.input.hotbar_sel].and_then(|s| reg.item(s.item).places);
            if let Some(block) = place {
                let bd = reg.block(block);
                // A survey cairn is raised with a prospector's strike:
                // the pick must be in the pack, and it wears.
                if Some(block) == reg.block_id("base:survey_cairn") && !self.creative {
                    let pick = reg.item_id("base:prospect_pick");
                    let slot = (0..TOTAL_SLOTS)
                        .find(|&i| self.inventory.slots[i].map(|s| Some(s.item)) == Some(pick));
                    let Some(slot) = slot else {
                        self.toast("Raising a cairn takes a prospector's pick.".to_string());
                        self.input.action_cooldown = 0.4;
                        return;
                    };
                    self.inventory.wear_tool(&reg, slot);
                }
                let needs_farmland = bd.crop_next.is_some() && !bd.crop_any_soil;
                let soil = pos
                    .offset(0, -1, 0)
                    .map_or(AIR, |below| self.server.world.get_block_at(below));
                if needs_farmland && Some(soil) != reg.block_id("base:farmland") {
                    return;
                }
                if needs_farmland
                    && let Some(below) = pos.offset(0, -1, 0)
                    && let Some(reason) = self.server.world.soil_failure_at(below)
                {
                    self.toast(reason.to_string());
                }
                // Cross blocks (torches, plants) need solid ground.
                if bd.cross && !reg.is_solid(soil) {
                    return;
                }
                // The cell must be one a block can take (air, fluid,
                // a thin layer) — checking merely "not solid" let a
                // click through into water or a crop, where the item
                // was spent and place_block then refused it.
                if reg.is_replaceable(self.server.world.get_block_at(pos))
                    && !self.player.overlaps_block_at(pos)
                {
                    let allow = if self.content.scripts.wants("on_block_place") {
                        let name = reg.block(block).name.clone();
                        let ok = self.content.scripts.dispatch(
                            &self.server.world,
                            "on_block_place",
                            (
                                pos.face().name().to_string(),
                                pos.u() as i64,
                                pos.y() as i64,
                                pos.v() as i64,
                                name,
                            ),
                        );
                        self.apply_script_cmds();
                        ok
                    } else {
                        true
                    };
                    if !allow {
                        return;
                    }
                    // Guests predict and let the host's echo correct
                    // them; the host places FIRST and only spends the
                    // item if the world actually took it.
                    if self.multiplayer.remote.is_some() {
                        if self.creative || self.inventory.take_one(self.input.hotbar_sel).is_some()
                        {
                            if let Some(r) = &self.multiplayer.remote {
                                r.client.send(&net::C2S::Place { pos });
                            }
                            self.input.action_cooldown = 0.22;
                            self.sfx(Sfx::Place);
                        }
                        return;
                    }
                    if self.inventory.slots[self.input.hotbar_sel].is_none() && !self.creative {
                        return;
                    }
                    if self.server.world.place_block_at(pos, block) {
                        if !self.creative {
                            self.inventory.take_one(self.input.hotbar_sel);
                        }
                        // A fresh sign or waystone wants its words.
                        if matches!(bd.interaction.as_deref(), Some("sign") | Some("waystone")) {
                            self.ui_state.sign_lines = Default::default();
                            self.ui_state.sign_line = 0;
                            self.set_screen(Screen::SignEdit(pos));
                        }
                        if bd.crop_next.is_some() {
                            // The wild notices things growing where you walk.
                            self.server.world.plant_ire_at_surface(pos.surface(), 0.2);
                        }
                        self.input.action_cooldown = 0.22;
                        self.sfx(Sfx::Place);
                    }
                }
            }
        }
    }

    /// Apply world mutations queued by scripts during the last dispatch.
    pub(super) fn apply_script_cmds(&mut self) {
        let reg = self.content.reg.clone();
        for cmd in self.content.scripts.take_cmds() {
            match cmd {
                script::Cmd::SetBlock(pos, name) => {
                    if let Some(b) = reg.block_id(&name) {
                        self.server
                            .world
                            .set_block_authored_at(pos, b, "mod script world event");
                    }
                }
                script::Cmd::Give(name, n) => {
                    if let Some(item) = reg.item_id(&name) {
                        if let Some(ledger) = &mut self.server.world.material_ledger
                            && let Err(error) = ledger.record_external_stack(
                                &reg,
                                ItemStack::new(&reg, item, n),
                                "mod script give",
                            )
                        {
                            eprintln!("materials: script give accounting failed: {error}");
                        }
                        let left = self.inventory.add(&reg, item, n);
                        if left > 0 {
                            self.drop_stack(ItemStack::new(&reg, item, left));
                        }
                    }
                }
                script::Cmd::Hud(msg) => self.toast(msg),
                script::Cmd::SpawnAnimal(name, pos) => {
                    if let Some(si) = reg.animal_id(&name)
                        && self.server.world.mob_count() < world::MOB_CAP
                    {
                        let mut m = mobs::Mob::new_at(si, pos, 0.0);
                        m.health = reg.animals[si].health;
                        self.server.world.spawn_mob(m);
                    }
                }
                script::Cmd::Sound(name) => {
                    let sfx = match name.as_str() {
                        "click" => Some(Sfx::Click),
                        "place" => Some(Sfx::Place),
                        "pickup" => Some(Sfx::Pickup),
                        "hurt" => Some(Sfx::Hurt),
                        "craft" => Some(Sfx::Craft),
                        "splash" => Some(Sfx::Splash),
                        _ => None,
                    };
                    if let Some(s) = sfx {
                        self.sfx(s);
                    }
                }
            }
        }
    }
}
