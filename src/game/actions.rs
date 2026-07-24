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
    pub(super) fn toast_prospect(&mut self, bx: i32, bz: i32) {
        let r = self.server.world.generator.prospect(bx, bz);
        let octant = |d: (i32, i32)| -> &'static str {
            if d == (0, 0) {
                return "here";
            }
            let a = (d.0 as f32).atan2(-(d.1 as f32)).to_degrees();
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
            names[(((a + 382.5) / 45.0) as usize) % 8]
        };
        let mut lines: Vec<String> = Vec::new();
        match r.pluton {
            Some((0, _)) => lines.push("Granite country underfoot.".into()),
            Some((d, dir)) => lines.push(format!("Granite country ~{d} blocks {}.", octant(dir))),
            None => lines.push("No batholith in the pick's reach.".into()),
        }
        if let Some((d, dir)) = r.volcano {
            if d == 0 {
                lines.push("Volcanic ground — you're standing on it.".into());
            } else {
                lines.push(format!("Volcanic rock ~{d} blocks {}.", octant(dir)));
            }
        }
        if let Some((d, dir)) = r.pipe {
            if d == 0 {
                lines.push("BLUE GROUND. A pipe under this very spot.".into());
            } else {
                lines.push(format!("Blue ground! A pipe ~{d} blocks {}.", octant(dir)));
            }
        }
        if let Some((d, dir)) = r.geode {
            if d == 0 {
                lines.push("A hollow ring underfoot — geode.".into());
            } else {
                lines.push(format!("A hollow ring ~{d} blocks {}.", octant(dir)));
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
        let dir = self.camera.forward();
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
            pos: self.camera.pos + dir * 0.4,
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
    pub(super) fn mob_in_crosshair(&self, hit: &Option<raycast::Hit>) -> Option<usize> {
        let origin = self.camera.pos;
        let dir = self.camera.forward();
        // A wall in the way shields the mob behind it (approximate the
        // wall distance by its block center).
        let wall_t = hit
            .as_ref()
            .map(|h| {
                let c = Vec3::new(
                    h.block.0 as f32 + 0.5,
                    h.block.1 as f32 + 0.5,
                    h.block.2 as f32 + 0.5,
                );
                (c - origin).length() + 0.5
            })
            .unwrap_or(REACH);
        let mut best: Option<(usize, f32)> = None;
        for (i, m) in self.server.world.mobs().iter().enumerate() {
            let def = &self.content.reg.animals[m.species];
            if let Some(t) = m.ray_hit(def, origin, dir, REACH.min(wall_t))
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
            if !def.hostile {
                // The wild counts its dead — wardens are not individuals.
                self.server.world.add_ire(2.0);
            }
            self.sfx(Sfx::MobDeath(def.sound_pitch));
            let (tile, at) = (def.tile, m.pos + Vec3::new(0.0, 0.5, 0.0));
            self.juice_burst(at, tile, 12, 2.0);
            if m.growth < 1.0 {
                continue; // the young return nothing (you monster)
            }
            for (item, min, max) in &def.drops {
                let n = min + (self.rand01() * (*max - *min + 1) as f32) as u32;
                let n = n.min(*max);
                if n == 0 {
                    continue;
                }
                if m.last_hit_by != 0 {
                    // A guest's kill: their loot crosses the wire.
                    let stack = ItemStack::new(&reg, *item, n);
                    self.server.world.queue_give(m.last_hit_by, stack);
                    continue;
                }
                let a = self.rand01() * std::f32::consts::TAU;
                let v = Vec3::new(a.cos() * 1.2, 2.5, a.sin() * 1.2);
                self.interaction.items.push(ItemEntity::new(
                    m.pos + Vec3::new(0.0, def.height * 0.5, 0.0),
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
                        m.pos.x as i64,
                        m.pos.y as i64,
                        m.pos.z as i64,
                    ),
                );
                self.apply_script_cmds();
            }
        }
    }

    /// Mining and placing while playing.
    pub(super) fn interact(&mut self, dt: f32) {
        let reg = self.content.reg.clone();
        let hit = raycast::raycast(
            &self.server.world,
            self.camera.pos,
            self.camera.forward(),
            REACH,
        );
        let held = self.inventory.slots[self.input.hotbar_sel].map(|s| s.item);

        // The bucket: scoop a full water cell or pour it back — the
        // cell moves with you, it never multiplies. Guests request and
        // the host's echo applies the world side; the bucket swap is
        // local (inventories are player-owned).
        if held.is_some() && held == reg.item_id("base:bucket") {
            if self.input.right_held
                && self.input.action_cooldown <= 0.0
                && let Some(w) = raycast::raycast_water(
                    &self.server.world,
                    self.camera.pos,
                    self.camera.forward(),
                    REACH,
                )
            {
                let (x, y, z) = w.block;
                let b = self.server.world.get_block(x, y, z);
                // Either fluid fills the bucket — a full cell only.
                if reg.fluid_volume(b) == Some(8) {
                    let full_item = if reg.is_lava(b) {
                        reg.item_id("base:bucket_lava")
                    } else {
                        reg.item_id("base:bucket_water")
                    };
                    if let Some(r) = &self.multiplayer.remote {
                        r.client.send(&net::C2S::Scoop { x, y, z });
                    } else {
                        self.server.world.set_block(x, y, z, AIR);
                    }
                    if let Some(full) = full_item {
                        self.inventory.slots[self.input.hotbar_sel] =
                            Some(ItemStack::new(&reg, full, 1));
                    }
                    self.input.action_cooldown = 0.25;
                    self.sfx(Sfx::Splash);
                }
            }
            return;
        }
        if held.is_some() && held == reg.item_id("base:bucket_water") {
            if self.input.right_held
                && self.input.action_cooldown <= 0.0
                && let Some(h) = &hit
            {
                let (x, y, z) = h.adjacent;
                if self.server.world.get_block(x, y, z) == AIR
                    && !self.player.overlaps_block(x, y, z)
                {
                    if let Some(r) = &self.multiplayer.remote {
                        r.client.send(&net::C2S::Place { x, y, z });
                    } else {
                        let water = reg.water_block(0);
                        self.server.world.place_block((x, y, z), water);
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
                let (x, y, z) = h.adjacent;
                if self.server.world.get_block(x, y, z) == AIR
                    && !self.player.overlaps_block(x, y, z)
                {
                    if let Some(r) = &self.multiplayer.remote {
                        r.client.send(&net::C2S::Place { x, y, z });
                    } else {
                        let lava = reg.lava_for_volume(8);
                        self.server.world.place_block((x, y, z), lava);
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

        // Archaeology: sweeping a remnant is a slow, careful channel.
        let brush_held = held.is_some_and(|i| reg.item(i).brush_tool);
        let brush_target = hit.as_ref().map(|h| h.block).filter(|t| {
            brush_held
                && reg
                    .block(self.server.world.get_block(t.0, t.1, t.2))
                    .brush
                    .is_some()
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
                    rc.client.send(&net::C2S::BrushBlock {
                        x: target.0,
                        y: target.1,
                        z: target.2,
                    });
                    if !self.creative {
                        self.inventory.wear_tool(&reg, self.input.hotbar_sel);
                    }
                    return;
                }
                let mut r = self.rng;
                let found = self
                    .server
                    .world
                    .brush_block(target.0, target.1, target.2, &mut r);
                self.rng = r;
                if let Some(stack) = found {
                    let center = Vec3::new(
                        target.0 as f32 + 0.5,
                        target.1 as f32 + 0.6,
                        target.2 as f32 + 0.5,
                    );
                    let mut ent =
                        ItemEntity::new(center, Vec3::new(0.0, 2.0, 0.0), stack.item, stack.count);
                    // Old tools surface as worn as they were buried.
                    if stack.durability < reg.item(stack.item).durability {
                        ent.durability = stack.durability;
                    }
                    self.interaction.items.push(ent);
                    self.sfx(Sfx::Pickup);
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
                .block(self.server.world.get_block(t.0, t.1, t.2))
                .interaction
                .clone();
            let Some(station) = station else { return false };
            let rested = match self.server.world.block_entity(t) {
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
                let top = Vec3::new(
                    target.0 as f32 + 0.5,
                    target.1 as f32 + 1.05,
                    target.2 as f32 + 0.5,
                );
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
                        let b = self.server.world.get_block(target.0, target.1, target.2);
                        let tile = reg.block(b).tiles[2];
                        self.juice_puff(top, tile, 3);
                    }
                }
                if !self.creative && held.is_some() {
                    self.inventory.wear_tool(&reg, self.input.hotbar_sel);
                }
                if let Some(rc) = &self.multiplayer.remote {
                    // The host counts strikes and Gives the bar.
                    rc.client.send(&net::C2S::AnvilStrike {
                        x: target.0,
                        y: target.1,
                        z: target.2,
                    });
                } else if let Some(out) = self.server.world.anvil_strike(target) {
                    let center = Vec3::new(
                        target.0 as f32 + 0.5,
                        target.1 as f32 + 1.0,
                        target.2 as f32 + 0.5,
                    );
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
                    let at = mob_pos + Vec3::new(0.0, 0.5, 0.0);
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
                    mob.hurt(&def, dmg, self.camera.pos);
                }
                if self.presentation.juice {
                    self.presentation.hitch = 0.06;
                }
                let at = mob_pos + Vec3::new(0.0, 0.5, 0.0);
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
                let b = self.server.world.get_block(target.0, target.1, target.2);
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
                                (target.0 as i64, target.1 as i64, target.2 as i64, name),
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
                                r.client.send(&net::C2S::Break {
                                    x: target.0,
                                    y: target.1,
                                    z: target.2,
                                });
                            }
                            self.survival.hunger = (self.survival.hunger - 0.008).max(0.0);
                            self.sfx(Sfx::Break(self.break_mat(b)));
                            let center = Vec3::new(
                                target.0 as f32 + 0.5,
                                target.1 as f32 + 0.5,
                                target.2 as f32 + 0.5,
                            );
                            self.juice_burst(center, self.content.reg.block(b).tiles[0], 10, 2.2);
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
                                .break_block(
                                    target,
                                    held,
                                    !self.creative && !sheared,
                                    !self.creative,
                                )
                                .expect("mining target was validated before completion");
                            let b = result.block;
                            self.sfx(Sfx::Break(self.break_mat(b)));
                            let center = Vec3::new(
                                target.0 as f32 + 0.5,
                                target.1 as f32 + 0.5,
                                target.2 as f32 + 0.5,
                            );
                            self.juice_burst(center, self.content.reg.block(b).tiles[0], 10, 2.2);
                            if !self.creative {
                                self.inventory.wear_tool(&reg, self.input.hotbar_sel);
                            }
                            // Shears: leaves come off whole.
                            if sheared
                                && !self.creative
                                && let Some(item) = reg.item_id(&reg.block(b).name)
                            {
                                let center = Vec3::new(
                                    target.0 as f32 + 0.5,
                                    target.1 as f32 + 0.3,
                                    target.2 as f32 + 0.5,
                                );
                                self.interaction.items.push(ItemEntity::new(
                                    center,
                                    Vec3::new(0.0, 2.2, 0.0),
                                    item,
                                    1,
                                ));
                            }
                            if let Some(drop) = result.drop {
                                let center = Vec3::new(
                                    target.0 as f32 + 0.5,
                                    target.1 as f32 + 0.3,
                                    target.2 as f32 + 0.5,
                                );
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
                                let center = Vec3::new(
                                    target.0 as f32 + 0.5,
                                    target.1 as f32 + 0.3,
                                    target.2 as f32 + 0.5,
                                );
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
                            let center = Vec3::new(
                                target.0 as f32 + 0.5,
                                target.1 as f32 + 0.5,
                                target.2 as f32 + 0.5,
                            );
                            self.juice_burst(center, self.content.reg.block(b).tiles[0], 2, 1.2);
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
                let def = &reg.animals[sp];
                if let (Some(bf), Some(h)) = (def.breed_food, held)
                    && bf == h
                    && !def.hostile
                    && growth >= 1.0
                    && breed_cd <= 0.0
                    && !fed
                    && (self.creative || self.inventory.take_one(self.input.hotbar_sel).is_some())
                {
                    // Guests request; setting fed locally is the
                    // prediction until the snapshot echoes it.
                    if let Some(rc) = &self.multiplayer.remote {
                        rc.client.send(&net::C2S::FeedMob { id: mob_id });
                    }
                    if let Some(mob) = self.server.world.mob_mut(mi) {
                        mob.fed = true;
                        mob.calm = 30.0;
                    }
                    self.input.action_cooldown = 0.4;
                    self.sfx(Sfx::Pickup);
                    return;
                }
            }
            // A covered log pile takes a warden's ember: the clamp.
            let ember = reg.item_id("base:ember");
            if held == ember
                && let Some(hb) = &hit
            {
                let (bx, by, bz) = hb.block;
                let tb = self.server.world.get_block(bx, by, bz);
                let is_log = reg.tags.get("base:logs").is_some_and(|l| {
                    reg.item_id(&reg.block(tb).name)
                        .is_some_and(|i| l.contains(&i))
                });
                if is_log {
                    if let Some(rc) = &self.multiplayer.remote {
                        self.inventory.take_one(self.input.hotbar_sel);
                        rc.client.send(&net::C2S::LightClamp {
                            x: bx,
                            y: by,
                            z: bz,
                        });
                    } else {
                        match self.server.world.try_light_clamp(bx, by, bz) {
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
                let (bx, by, bz) = hb.block;
                let tb = self.server.world.get_block(bx, by, bz);
                if self.server.world.reg.is_solid(tb) {
                    self.toast_prospect(bx, bz);
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
                let dir = self.camera.forward();
                if let Some(rc) = &self.multiplayer.remote {
                    rc.client.send(&net::C2S::FireProjectile {
                        direction: dir,
                        charge: 1.0,
                    });
                } else {
                    let pos = self.camera.pos + dir * 0.4;
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
        let held_is_food = held.is_some_and(|i| reg.item(i).food.is_some());
        if self.input.right_held
            && self.input.action_cooldown <= 0.0
            && !held_is_food
            && let Some(h) = &hit
        {
            let tb = self.server.world.get_block(h.block.0, h.block.1, h.block.2);
            // Harvestable blocks (berry bushes).
            if let Some((item, n, becomes)) = reg.block(tb).harvest {
                self.server
                    .world
                    .set_block(h.block.0, h.block.1, h.block.2, becomes);
                let left = self.inventory.add(&reg, item, n);
                if left > 0 {
                    self.drop_stack(ItemStack::new(&reg, item, left));
                }
                self.sfx(Sfx::Pickup);
                self.input.action_cooldown = 0.3;
                return;
            }
            // Hoe tills grass/dirt into farmland.
            if let (Some((ToolKind::Hoe, _, _)), Some(farm)) = (
                held.and_then(|i| reg.item(i).tool),
                reg.block_id("base:farmland"),
            ) {
                let name = reg.block(tb).name.as_str();
                if name == "base:grass" || name == "base:dirt" {
                    self.server
                        .world
                        .set_block(h.block.0, h.block.1, h.block.2, farm);
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
                    (h.block.0 as i64, h.block.1 as i64, h.block.2 as i64, name),
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
                    self.server.world.ensure_block_entity(
                        h.block,
                        world::BlockEntity::Furnace(Default::default()),
                    );
                    self.set_screen(Screen::Furnace(h.block));
                    return;
                }
                Some("chest") if self.input.action_cooldown <= 0.0 => {
                    self.input.action_cooldown = 0.3;
                    if let Some(rc) = &self.multiplayer.remote {
                        rc.client.send(&net::C2S::OpenContainer {
                            x: h.block.0,
                            y: h.block.1,
                            z: h.block.2,
                        });
                        return;
                    }
                    let e = self.server.world.ensure_block_entity(
                        h.block,
                        world::BlockEntity::Chest(Default::default()),
                    );
                    if let world::BlockEntity::Chest(c) = e
                        && c.wild_owned
                    {
                        c.wild_owned = false;
                        self.server.world.add_ire(1.0);
                        self.toast("The wild keeps its trophies.".to_string());
                    }
                    self.set_screen(Screen::Chest(h.block));
                    return;
                }
                Some("offering") if self.input.action_cooldown <= 0.0 => {
                    self.input.action_cooldown = 0.3;
                    if let Some(rc) = &self.multiplayer.remote {
                        rc.client.send(&net::C2S::OpenContainer {
                            x: h.block.0,
                            y: h.block.1,
                            z: h.block.2,
                        });
                        return;
                    }
                    self.server.world.ensure_block_entity(
                        h.block,
                        world::BlockEntity::Offering(Default::default()),
                    );
                    self.set_screen(Screen::Offering(h.block));
                    return;
                }
                Some("survey") if self.input.action_cooldown <= 0.0 => {
                    // A raised cairn is bought knowledge: anyone reads
                    // the surveyor's ground, no pick required.
                    self.toast_prospect(h.block.0, h.block.2);
                    self.sfx(Sfx::Click);
                    self.input.action_cooldown = 0.6;
                    return;
                }
                Some(station @ ("bloomery" | "kiln" | "forge"))
                    if self.input.action_cooldown <= 0.0 =>
                {
                    self.input.action_cooldown = 0.3;
                    if let Some(rc) = &self.multiplayer.remote {
                        rc.client.send(&net::C2S::OpenContainer {
                            x: h.block.0,
                            y: h.block.1,
                            z: h.block.2,
                        });
                        return;
                    }
                    let (default, screen) = match station {
                        "kiln" => (
                            world::BlockEntity::Kiln(Default::default()),
                            Screen::Kiln(h.block),
                        ),
                        "forge" => (
                            world::BlockEntity::Forge(Default::default()),
                            Screen::Bloomery(h.block),
                        ),
                        _ => (
                            world::BlockEntity::Bloomery(Default::default()),
                            Screen::Bloomery(h.block),
                        ),
                    };
                    self.server.world.ensure_block_entity(h.block, default);
                    self.set_screen(screen);
                    return;
                }
                _ => {}
            }
            let (x, y, z) = h.adjacent;
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
                let soil = self.server.world.get_block(x, y - 1, z);
                if needs_farmland && Some(soil) != reg.block_id("base:farmland") {
                    return;
                }
                // Cross blocks (torches, plants) need solid ground.
                if bd.cross && !reg.is_solid(soil) {
                    return;
                }
                if !reg.is_solid(self.server.world.get_block(x, y, z))
                    && !self.player.overlaps_block(x, y, z)
                {
                    let allow = if self.content.scripts.wants("on_block_place") {
                        let name = reg.block(block).name.clone();
                        let ok = self.content.scripts.dispatch(
                            &self.server.world,
                            "on_block_place",
                            (x as i64, y as i64, z as i64, name),
                        );
                        self.apply_script_cmds();
                        ok
                    } else {
                        true
                    };
                    let consumed =
                        self.creative || self.inventory.take_one(self.input.hotbar_sel).is_some();
                    if allow && consumed && self.multiplayer.remote.is_some() {
                        if let Some(r) = &self.multiplayer.remote {
                            r.client.send(&net::C2S::Place { x, y, z });
                        }
                        self.input.action_cooldown = 0.22;
                        self.sfx(Sfx::Place);
                        return;
                    }
                    if allow && consumed {
                        self.server.world.place_block((x, y, z), block);
                        if bd.crop_next.is_some() {
                            // The wild notices things growing where you walk.
                            self.server.world.plant_ire(0.2);
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
                script::Cmd::SetBlock(x, y, z, name) => {
                    if let Some(b) = reg.block_id(&name) {
                        self.server.world.set_block(x, y, z, b);
                    }
                }
                script::Cmd::Give(name, n) => {
                    if let Some(item) = reg.item_id(&name) {
                        let left = self.inventory.add(&reg, item, n);
                        if left > 0 {
                            self.drop_stack(ItemStack::new(&reg, item, left));
                        }
                    }
                }
                script::Cmd::Hud(msg) => self.toast(msg),
                script::Cmd::SpawnAnimal(name, x, y, z) => {
                    if let Some(si) = reg.animal_id(&name)
                        && self.server.world.mob_count() < world::MOB_CAP
                    {
                        let mut m = mobs::Mob::new(si, Vec3::new(x, y, z), 0.0);
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
