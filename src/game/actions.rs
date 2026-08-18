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

    pub(super) fn held_art_stack(&self, stack: Option<ItemStack>) -> mobs::HeldArt {
        let Some(stack) = stack else {
            return mobs::HeldArt::None;
        };
        if let Some(visual) = self.server.world.implement_visual(stack) {
            return self.held_art_implement(visual, |wire| Some(ItemId(wire)));
        }
        self.held_art(Some(stack.item))
    }

    pub(super) fn held_art_implement(
        &self,
        visual: crate::implements::ImplementVisual,
        mut map: impl FnMut(u16) -> Option<ItemId>,
    ) -> mobs::HeldArt {
        let icon = |item: Option<ItemId>| {
            item.map(|item| self.content.reg.item(item).icon)
                .unwrap_or(crate::atlas::UNKNOWN_SLOT)
        };
        mobs::HeldArt::Wand {
            body: icon(map(visual.body)),
            reservoir: icon(map(visual.reservoir)),
            focus: icon(map(visual.focus)),
            binding: icon(map(visual.binding)),
            focus_shape: visual.focus_shape,
            charge_band: visual.charge_band.min(3),
        }
    }

    /// Present an authoritative implement event without learning exact charge
    /// or provenance. `wire_items` is present for a guest receiving host item
    /// ids; a windowed host passes `None` because its visual already names the
    /// local registry.
    pub(super) fn present_implement_activation(
        &mut self,
        pos: crate::planet::EntityPos,
        cue: crate::implements::ImplementCue,
        visual: Option<crate::implements::ImplementVisual>,
        wire_items: Option<&[Option<ItemId>]>,
    ) {
        let sound = match cue {
            crate::implements::ImplementCue::Use => Sfx::ImplementUse,
            crate::implements::ImplementCue::Transfer => Sfx::ImplementTransfer,
            crate::implements::ImplementCue::Strain => Sfx::ImplementStrain,
            crate::implements::ImplementCue::Empty => Sfx::ImplementEmpty,
            crate::implements::ImplementCue::Failure => Sfx::ImplementFailure,
        };
        self.sfx(sound);
        if !self.presentation.juice {
            return;
        }
        let focus_item = visual.and_then(|visual| {
            wire_items.map_or_else(
                || {
                    self.content
                        .reg
                        .items
                        .get(visual.focus as usize)
                        .map(|_| ItemId(visual.focus))
                },
                |map| map.get(visual.focus as usize).copied().flatten(),
            )
        });
        let tile = focus_item
            .map(|item| self.content.reg.item(item).icon)
            .unwrap_or_else(|| {
                *crate::atlas::builtin_slots()
                    .get("ember")
                    .unwrap_or(&crate::atlas::UNKNOWN_SLOT)
            });
        let count = match cue {
            crate::implements::ImplementCue::Transfer => 10,
            crate::implements::ImplementCue::Failure => 18,
            crate::implements::ImplementCue::Use | crate::implements::ImplementCue::Strain => 6,
            crate::implements::ImplementCue::Empty => 2,
        };
        self.juice_burst(pos.render_pos(), tile, count, 1.4);
    }

    pub(super) fn present_working_cue(&mut self, cue: crate::workings::WorkingCue) {
        use crate::workings::WorkingCueKind;
        if matches!(
            cue.kind,
            WorkingCueKind::Complete | WorkingCueKind::Cancel | WorkingCueKind::Refuse
        ) {
            self.presentation.working_cues.remove(&cue.stable_id);
        } else {
            self.presentation
                .working_cues
                .insert(cue.stable_id, (cue.clone(), self.time_abs));
        }
        self.presentation.swing = 1.0;
        self.sfx(
            if cue.warning_band >= 2
                || matches!(cue.kind, WorkingCueKind::Strain | WorkingCueKind::Overload)
            {
                Sfx::WorkingStrain(cue.warning_band)
            } else if matches!(cue.kind, WorkingCueKind::Refuse) {
                Sfx::ImplementFailure
            } else {
                Sfx::ImplementUse
            },
        );
        if !self.presentation.juice {
            return;
        }
        let source_tile = *crate::atlas::builtin_slots()
            .get("ember")
            .unwrap_or(&crate::atlas::UNKNOWN_SLOT);
        let target_tile = *crate::atlas::builtin_slots()
            .get("water")
            .unwrap_or(&crate::atlas::UNKNOWN_SLOT);
        let path_tile = *crate::atlas::builtin_slots()
            .get("stone")
            .unwrap_or(&crate::atlas::UNKNOWN_SLOT);
        self.juice_burst(
            cue.source.entity_center().render_pos(),
            source_tile,
            4,
            0.75,
        );
        if let Some(target) = cue.path.last().copied() {
            self.juice_burst(
                target.entity_center().render_pos(),
                target_tile,
                if cue.warning_band >= 2 { 10 } else { 6 },
                1.0,
            );
        }
        let inner = cue.path.len().saturating_sub(2);
        let stride = inner.div_ceil(8).max(1);
        for pos in cue.path.iter().skip(1).take(inner).step_by(stride) {
            self.juice_burst(
                pos.entity_center().render_pos(),
                path_tile,
                if cue.warning_band >= 2 { 3 } else { 1 },
                0.28,
            );
        }
    }

    pub(super) fn present_alchemy_cue(&mut self, cue: crate::alchemy::AlchemyCue) {
        use crate::alchemy::AlchemyCueKind;
        self.presentation.swing = 1.0;
        self.sfx(match cue.kind {
            AlchemyCueKind::Grind => Sfx::Grind,
            AlchemyCueKind::Bubble | AlchemyCueKind::Drip | AlchemyCueKind::Pour => Sfx::Splash,
            AlchemyCueKind::Filter | AlchemyCueKind::Clean => Sfx::Click,
            AlchemyCueKind::Leak
            | AlchemyCueKind::Overcharge
            | AlchemyCueKind::Spoil
            | AlchemyCueKind::Pulse => Sfx::ImplementStrain,
            AlchemyCueKind::Drink | AlchemyCueKind::Apply => Sfx::Pickup,
        });
        if self.presentation.juice {
            let tile_name = if cue.color[0] > cue.color[1] && cue.color[0] > cue.color[2] {
                "ember"
            } else if cue.color[2] > cue.color[0] {
                "water"
            } else {
                "plant"
            };
            let tile = *crate::atlas::builtin_slots()
                .get(tile_name)
                .unwrap_or(&crate::atlas::UNKNOWN_SLOT);
            let count = 3 + usize::from(cue.intensity) / 20;
            self.juice_burst(
                cue.pos.entity_center().render_pos(),
                tile,
                count.min(18),
                0.9,
            );
        }
        self.toast(cue.message);
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

    pub(super) fn implement_glow(
        &self,
        visual: crate::implements::ImplementVisual,
    ) -> Option<(Vec3, f32)> {
        let band = visual.charge_band.min(3);
        if band == 0 {
            return None;
        }
        let strength = f32::from(band) / 3.0;
        Some((
            Vec3::new(0.32, 0.58, 0.95) * (0.32 + strength * 0.48),
            3.5 + strength * 3.5,
        ))
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
            stable_id: 0,
            pos: eye
                .translated(dir * 0.4)
                .expect("projectile muzzle stays near the player")
                .pos,
            vel: dir * bow.speed * (0.6 + 0.4 * charge),
            tile: reg.item(arrow_id).icon,
            damage: bow.damage * (0.45 + 0.55 * charge),
            damage_type: None,
            age: 0.0,
            from_player: true,
            // Arrows that stick into terrain are recoverable.
            drop_item: (!self.creative).then_some(arrow_id),
            preparation_payload: None,
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
        let reach = self.reach();
        // A wall in the way shields the mob behind it (approximate the
        // wall distance by its block center).
        let wall_t = hit
            .as_ref()
            .map(|h| origin.distance_to(h.block.entity_center()) + 0.5)
            .unwrap_or(reach);
        let mut best: Option<(usize, f32)> = None;
        for (i, m) in self.server.world.mobs().iter().enumerate() {
            let def = &self.content.reg.animals[m.species];
            if let Some(t) = m.ray_hit_from(def, origin, dir, reach.min(wall_t))
                && best.is_none_or(|(_, bt)| t < bt)
            {
                best = Some((i, t));
            }
        }
        best.map(|(i, _)| i)
    }

    /// Remove dead mobs: roll their drop table, spill items, notify mods.
    pub(super) fn present_settled_mob_death(&mut self, death: crate::world::SettledMobDeath) {
        let reg = self.content.reg.clone();
        let Some(def) = reg.animals.get(death.species) else {
            return;
        };
        self.sfx(Sfx::MobDeath(def.sound_pitch));
        let (tile, at) = (
            def.tile,
            death
                .pos
                .translated(Vec3::new(0.0, 0.5, 0.0))
                .expect("death effect stays beside the mob")
                .pos
                .render_pos(),
        );
        self.juice_burst(at, tile, 12, 2.0);
        if def.hostile && self.content.scripts.wants("on_enemy_destroyed") {
            self.content.scripts.dispatch(
                &self.server.world,
                "on_enemy_destroyed",
                (
                    def.name.clone(),
                    death.pos.face().name().to_string(),
                    death.pos.u().floor() as i64,
                    death.pos.y().floor() as i64,
                    death.pos.v().floor() as i64,
                ),
            );
            self.apply_script_cmds();
        }
        if self.content.scripts.wants("on_animal_killed") {
            self.content.scripts.dispatch(
                &self.server.world,
                "on_animal_killed",
                (
                    def.name.clone(),
                    death.pos.face().name().to_string(),
                    death.pos.u().floor() as i64,
                    death.pos.y().floor() as i64,
                    death.pos.v().floor() as i64,
                ),
            );
            self.apply_script_cmds();
        }
    }

    /// Mining and placing while playing.
    pub(super) fn interact(&mut self, dt: f32) {
        let reg = self.content.reg.clone();
        let reach = self.reach();
        let hit = raycast::raycast_at(
            &self.server.world,
            self.player.eye(),
            self.camera.local_forward(),
            reach,
        );
        let aim = raycast::raycast_target_at(
            &self.server.world,
            self.player.eye(),
            self.camera.local_forward(),
            reach,
        );
        let held = self.inventory.slots[self.input.hotbar_sel].map(|s| s.item);
        if self.interact_wand(dt, hit.as_ref()) {
            return;
        }

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
                self.reach(),
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
                    reach,
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

        // Stable preparation bottles are not generic food. Their saved dose
        // identity decides the bounded application and the host performs the
        // water/Current/status transaction.
        let preparation_application =
            self.inventory.slots[self.input.hotbar_sel].and_then(|stack| {
                (stack.arcane_id != 0).then_some(())?;
                let item_name = &reg.item(stack.item).name;
                reg.preparations
                    .values()
                    .find(|definition| definition.output_item == *item_name)
                    .map(|definition| definition.application)
            });
        if self.input.right_held
            && self.input.action_cooldown <= 0.0
            && let Some(application) = preparation_application
        {
            let adjacent_slot = (self.input.hotbar_sel + 1) % crate::inventory::HOTBAR_SLOTS;
            let adjacent_id = self.inventory.slots[adjacent_slot]
                .filter(|stack| stack.count == 1)
                .map_or(0, |stack| stack.arcane_id);
            let target = match application {
                crate::alchemy::ApplicationKind::Drink => crate::alchemy::AlchemyTarget::SelfActor,
                crate::alchemy::ApplicationKind::Plot => {
                    let Some(hit) = &hit else {
                        self.toast("Aim Root Wash at one plot or rooting bed.".into());
                        return;
                    };
                    crate::alchemy::AlchemyTarget::Plot(hit.block)
                }
                crate::alchemy::ApplicationKind::Wash => {
                    if let Some(hit) = &hit {
                        crate::alchemy::AlchemyTarget::Surface(hit.block)
                    } else if adjacent_id != 0 {
                        crate::alchemy::AlchemyTarget::Item(adjacent_id)
                    } else {
                        self.toast(
                            "Aim Ashlace Wash at a small surface, or carry one stable tool immediately right of it."
                                .into(),
                        );
                        return;
                    }
                }
                crate::alchemy::ApplicationKind::Coat => {
                    if adjacent_id == 0 {
                        self.toast(
                            "Carry one stable botanical specimen immediately right of the Frostlace jar."
                                .into(),
                        );
                        return;
                    }
                    crate::alchemy::AlchemyTarget::Item(adjacent_id)
                }
            };
            self.use_selected_preparation(target);
            self.input.right_held = false;
            self.input.action_cooldown = 0.3;
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

        // A tuning lens is deliberately slow and local. Holding the aim still
        // for the full settle period produces one qualitative, signed record;
        // moving off the target or releasing use starts the reading over.
        let lens_held = self.inventory.slots[self.input.hotbar_sel].is_some_and(|stack| {
            reg.item(stack.item)
                .discovery
                .as_ref()
                .is_some_and(|definition| definition.kind == "tuning_lens")
                && stack.arcane_id != 0
        });
        if self.input.right_held && lens_held {
            let aim = hit.as_ref().map_or_else(
                || self.player.pos.block().map(DiscoveryAim::Region),
                |hit| Some(DiscoveryAim::Block(hit.block)),
            );
            let Some(aim) = aim else {
                return;
            };
            if self.interaction.lens_target != Some(aim) {
                self.interaction.lens_target = Some(aim);
                self.interaction.lens_settle = 0.0;
                if let Some(remote) = &self.multiplayer.remote {
                    if let DiscoveryAim::Block(pos) = aim
                        && reg
                            .block(self.server.world.get_block_at(pos))
                            .discovery_fixture
                            .as_ref()
                            .is_some_and(|fixture| fixture.kind == "experiment_apparatus")
                    {
                        let kind =
                            crate::discovery::ExperimentKind::ALL[self.interaction.experiment_kind
                                % crate::discovery::ExperimentKind::ALL.len()];
                        remote.client.send(&net::C2S::BeginExperiment { pos, kind });
                    } else {
                        remote.client.send(&net::C2S::BeginObserve {
                            target: match aim {
                                DiscoveryAim::Region(_) => net::DiscoveryTargetSnap::Region,
                                DiscoveryAim::Block(pos) => net::DiscoveryTargetSnap::Block(pos),
                            },
                        });
                    }
                }
                self.sfx(Sfx::Lens(0.78));
            }
            let prior_settle = self.interaction.lens_settle;
            self.interaction.lens_settle += dt;
            if prior_settle < 0.62 && self.interaction.lens_settle >= 0.62 {
                self.sfx(Sfx::Lens(0.96));
            }
            if self.interaction.lens_settle >= 1.25 {
                self.interaction.lens_settle = 0.0;
                self.interaction.lens_target = None;
                self.input.right_held = false;
                self.input.action_cooldown = 0.25;
                if let DiscoveryAim::Block(pos) = aim
                    && reg
                        .block(self.server.world.get_block_at(pos))
                        .discovery_fixture
                        .as_ref()
                        .is_some_and(|fixture| fixture.kind == "experiment_apparatus")
                {
                    self.settle_discovery_experiment(pos);
                } else {
                    self.settle_discovery_reading(aim);
                }
                self.sfx(Sfx::Click);
            }
            return;
        } else {
            self.interaction.lens_settle = 0.0;
            self.interaction.lens_target = None;
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
                    ent.arcane_id = stack.arcane_id;
                    self.server.world.spawn_loose_item(ent);
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
                    self.server.world.spawn_loose_item(ItemEntity::new(
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
        // block behind it. Held tools/swords set the damage. Every third
        // press inside the combo window is a heavy finisher; a hit from
        // behind the mob's facing backstabs for double.
        if self.input.left_held
            && let Some(mi) = self.mob_in_crosshair(&hit)
            && !matches!(aim, Some(raycast::TargetHit::Structure { .. }))
        {
            self.interaction.breaking = None;
            if self.input.attack_cooldown <= 0.0 {
                let heavy = self.combat.combo >= combat::HEAVY_PRESS;
                if !self.can_swing(heavy) {
                    return;
                }
                self.combat.swing(heavy, self.swing_cost(heavy));
                self.input.attack_cooldown = if heavy {
                    combat::HEAVY_SWING_INTERVAL
                } else {
                    combat::SWING_INTERVAL
                };
                self.presentation.swing = 1.0;
                let Some(mob) = self.server.world.mob(mi) else {
                    return;
                };
                let (sp, mob_id, mob_pos) = (mob.species, mob.id, mob.pos);
                let pitch = reg.animals[sp].sound_pitch;
                if let Some(r) = &self.multiplayer.remote {
                    // The host is the damage authority; it applies heavy and
                    // backstab and reports the true number back.
                    r.client.send(&net::C2S::AttackMob { id: mob_id, heavy });
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
                    return;
                }
                let def = reg.animals[sp].clone();
                if let Some(mob) = self.server.world.mob_mut(mi) {
                    let base = held.map(|i| reg.item(i).damage).unwrap_or(1.0);
                    let backstab = !self.creative
                        && combat::mob_facing_away(mob.yaw, mob.pos, self.player.pos);
                    let mut dmg = base;
                    let mut crit = false;
                    if heavy {
                        dmg *= combat::HEAVY_MULT;
                        crit = true;
                    }
                    if backstab {
                        dmg *= combat::BACKSTAB_MULT;
                        crit = true;
                    }
                    let dmg_type = held.and_then(|i| reg.item(i).damage_type.clone());
                    mob.hurt(&def, dmg, dmg_type.as_deref(), self.player.eye());
                    // Heavy finishers shove: mob.hurt already knocked back
                    // along the attack line; an extra impulse sells the hit.
                    if heavy {
                        let mut dir = self.player.pos.local_delta_to(mob.pos);
                        dir.y = 0.0;
                        if dir.length_squared() > 0.001 {
                            mob.vel += dir.normalize() * 3.0;
                        }
                    }
                    let hit_pos = mob.pos;
                    self.spawn_damage_number(hit_pos, dmg, crit);
                    if self.content.scripts.wants("on_hurt") {
                        self.content.scripts.dispatch(
                            &self.server.world,
                            "on_hurt",
                            (
                                def.name.clone(),
                                dmg as f64,
                                dmg_type.clone().unwrap_or_default(),
                            ),
                        );
                        self.apply_script_cmds();
                    }
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
            // Structure mining (takes priority over world mining).
            if let Some(raycast::TargetHit::Structure {
                id,
                block,
                adjacent: _adj,
            }) = &aim
            {
                let (sid, soff) = (*id, *block);
                let s_block_id = self
                    .server
                    .world
                    .local_structure(sid)
                    .map(|s| s.get_block(soff))
                    .unwrap_or(AIR);
                let hardness = if self.creative {
                    reg.block(s_block_id).hardness.map(|_| 0.0001)
                } else {
                    reg.effective_hardness(s_block_id, held)
                };
                if let Some(hardness) = hardness {
                    let target_break = super::BreakTarget::Structure(sid, soff);
                    let progress = match self.interaction.breaking {
                        Some((t, p)) if t == target_break => p + dt / hardness.max(0.0001),
                        _ => dt / hardness.max(0.0001),
                    };
                    if progress >= 1.0 {
                        // No `on_block_break` script hook for structure
                        // blocks (they have no world BlockPos).
                        self.interaction.breaking = None;
                        let drop = self
                            .server
                            .world
                            .local_structure_mut(sid)
                            .and_then(|s| s.break_block(soff, held));
                        // 8c: drop to player inventory directly.
                        if !self.creative {
                            if let Some(stack) = drop {
                                let item = stack.item;
                                let remaining = self.inventory.add_stack(&reg, stack);
                                if remaining > 0
                                    && let Some(wp) = self
                                        .server
                                        .world
                                        .local_structure(sid)
                                        .and_then(|s| s.world_position(soff))
                                {
                                    self.server.world.spawn_loose_item(ItemEntity::new(
                                        wp.entity_at_height(0.3),
                                        Vec3::new(0.0, 2.2, 0.0),
                                        item,
                                        remaining,
                                    ));
                                }
                            }
                            self.inventory.wear_tool(&reg, self.input.hotbar_sel);
                        }
                        self.survival.hunger = (self.survival.hunger - 0.008).max(0.0);
                        self.sfx(Sfx::Break(self.break_mat(s_block_id)));
                        if let Some(wp) = self
                            .server
                            .world
                            .local_structure(sid)
                            .and_then(|s| s.world_position(soff))
                        {
                            self.juice_burst(
                                wp.entity_center().render_pos(),
                                self.content.reg.block(s_block_id).tiles[0],
                                10,
                                2.2,
                            );
                        }
                    } else {
                        self.interaction.breaking = Some((target_break, progress));
                    }
                } else {
                    self.interaction.breaking = None;
                }
            } else if let Some(h) = &hit {
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
                        Some((t, p)) if t == super::BreakTarget::World(target) => {
                            p + dt / hardness.max(0.0001)
                        }
                        _ => dt / hardness.max(0.0001),
                    };
                    if progress >= 1.0 {
                        // Flag-gated features (spec 2.5): a sealed gate cannot
                        // be mined open. The world refuses anyway (backstop);
                        // here we surface the reason as a toast instead of
                        // letting the swing hit the None path.
                        let gate_blocked = if let Some(gate) =
                            self.server.world.gate_at(target)
                            && self
                                .content
                                .reg
                                .gates
                                .get(gate)
                                .is_some_and(|g| g.unbreakable_when_locked)
                        {
                            let definition = self.content.reg.gates[gate].clone();
                            let unlocked = self
                                .read_player_kv(&definition.flag)
                                .is_some_and(|v| v == definition.value);
                            if !unlocked {
                                self.toast(definition.message.clone());
                            } else {
                                self.toast("Right-click to open the sealed gate.".to_string());
                            }
                            true
                        } else {
                            false
                        };
                        self.interaction.breaking = None;
                        if gate_blocked {
                            return;
                        }
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
                                self.server.world.spawn_loose_item(ItemEntity::new(
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
                                self.server.world.spawn_loose_item(ItemEntity::new(
                                    center, v, drop.item, drop.count,
                                ));
                            }
                            // Chance extras (leaves drop saplings).
                            if !self.creative
                                && let Some(stack) =
                                    self.server
                                        .world
                                        .roll_bonus_drop_at(target, b, &mut self.rng)
                            {
                                let center = target.entity_at_height(0.3);
                                let a = self.rand01() * std::f32::consts::TAU;
                                let v = Vec3::new(a.cos() * 1.2, 2.2, a.sin() * 1.2);
                                let mut entity =
                                    ItemEntity::new(center, v, stack.item, stack.count);
                                entity.arcane_id = stack.arcane_id;
                                self.server.world.spawn_loose_item(entity);
                            }
                        }
                    } else {
                        let stage_before =
                            (self.interaction.breaking.map(|(_, p)| p).unwrap_or(0.0) * 4.0) as i32;
                        self.interaction.breaking =
                            Some((super::BreakTarget::World(target), progress));
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
            // Structure hit: place the held block at the adjacent cell.
            // In-structure interaction (machines, containers) is deferred
            // this phase — right-clicking a structure always places.
            if let Some(raycast::TargetHit::Structure { id, adjacent, .. }) = &aim {
                let sid = *id;
                let off = *adjacent;
                let place = self.inventory.slots[self.input.hotbar_sel]
                    .and_then(|s| reg.item(s.item).places);
                if let Some(block) = place
                    && (self.creative || self.inventory.slots[self.input.hotbar_sel].is_some())
                {
                    let placed = self
                        .server
                        .world
                        .local_structure_mut(sid)
                        .map(|s| s.place_block(off, block))
                        .unwrap_or(false);
                    if placed {
                        if !self.creative {
                            self.inventory.take_one(self.input.hotbar_sel);
                        }
                        self.input.action_cooldown = 0.22;
                        self.sfx(Sfx::Place);
                    }
                }
                return;
            }
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
                // Talking: right-clicking a friendly NPC opens its dialogue
                // tree (spec 3.2) instead of the animal interactions below.
                // NPCs never feed/tame/cargo/ride.
                if reg.is_npc_species(sp) {
                    let root = self
                        .server
                        .world
                        .npc_by_mob(mob_id)
                        .and_then(|npc| npc.dialogue.clone())
                        .and_then(|d| reg.dialogues.iter().find(|dd| dd.id == d).cloned())
                        .map(|dd| dd.root)
                        .unwrap_or_default();
                    self.input.action_cooldown = 0.3;
                    self.set_screen(Screen::Dialog {
                        npc: mob_id,
                        node_id: root,
                        choice_sel: 0,
                    });
                    return;
                }
                // Hacking: right-clicking a construct with a tagged tool
                // disables it instead of destroying it (spec 3.6).
                // Destroying one yields its scrap `drops`; hacking it
                // yields the core. The hack arm sits next to dialogue,
                // before feeding/taming ever runs.
                if def.hack.is_some()
                    && let Some(hack) = &def.hack
                    && let Some(tool) = hack.tool.as_deref()
                    && held.is_some_and(|i| {
                        let item = reg.item(i);
                        match tool {
                            "hack" => item.hack,
                            other => item.name.ends_with(&format!(":{other}")),
                        }
                    })
                {
                    if let Some(rc) = &self.multiplayer.remote {
                        rc.client.send(&net::C2S::HackMob { id: mob_id });
                    } else {
                        self.server.world.hack_mob(mi, &mut self.server.rng);
                    }
                    self.input.action_cooldown = 0.5;
                    self.sfx(Sfx::Place);
                    self.toast(format!("The {def_label} goes still."));
                    return;
                }
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
                                let consumed = self.inventory.slots[self.input.hotbar_sel];
                                self.inventory.take_one(self.input.hotbar_sel);
                                if !self.creative
                                    && let Some(stack) = consumed
                                {
                                    self.server.world.retire_arcane_stack_at(
                                        pos,
                                        ItemStack { count: 1, ..stack },
                                        "clamp ignition",
                                    );
                                }
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
            // Throwables are loosed from the hand. A preparation remains a
            // stable physical vessel in flight even in creative mode; it may
            // never be cloned or discarded as a cosmetic projectile.
            if let Some(speed) = held.and_then(|i| reg.item(i).throw_speed) {
                let item = held.unwrap();
                let selected = self.inventory.slots[self.input.hotbar_sel];
                let state_bearing = selected.is_some_and(|stack| stack.arcane_id != 0);
                let removed = if self.creative && !state_bearing {
                    None
                } else {
                    self.inventory.take_one_stack(self.input.hotbar_sel)
                };
                if state_bearing && removed.is_none() {
                    return;
                }
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
                        stable_id: 0,
                        pos,
                        vel,
                        tile,
                        damage: 0.0,
                        damage_type: None,
                        age: 0.0,
                        from_player: true,
                        drop_item: None,
                        preparation_payload: removed.filter(|stack| stack.arcane_id != 0),
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
            // Knowledge is physical: artifacts retain their generated words,
            // while ledgers and folios expose only the signed records inside.
            if held.is_some_and(|item| reg.item(item).discovery.is_some()) {
                self.read_held_knowledge();
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
            // Flag-gated features (spec 2.5): right-clicking a sealed block
            // either opens it (player's KV flag met) or is refused with the
            // gate's message. Only the interact path opens a gate — mining
            // a sealed block is refused at the world level regardless.
            if let Some(gate) = self.server.world.gate_at(h.block) {
                let definition = self.content.reg.gates[gate].clone();
                let unlocked = self
                    .read_player_kv(&definition.flag)
                    .is_some_and(|v| v == definition.value);
                if !unlocked {
                    self.toast(definition.message.clone());
                    self.input.right_held = false;
                    self.input.action_cooldown = 0.35;
                    return;
                }
                if let Some(unlocked_block) = definition.unlocked_block {
                    self.server.world.set_block_at(h.block, unlocked_block);
                    self.server.world.ungate_at(h.block);
                    self.sfx(Sfx::Place);
                    self.toast("The gate opens.".to_string());
                    self.input.right_held = false;
                    self.input.action_cooldown = 0.35;
                    return;
                }
                // No unlocked_block: the gate stays but is now breakable.
                // Pass through to normal behavior below.
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
                Some("rail_switch") if self.input.action_cooldown <= 0.0 => {
                    self.input.action_cooldown = 0.25;
                    self.input.right_held = false;
                    self.server.world.toggle_switch(h.block);
                    self.toast("The switch points differently now.".to_string());
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
                Some("discovery_folio") if self.input.action_cooldown <= 0.0 => {
                    self.input.action_cooldown = 0.35;
                    self.input.right_held = false;
                    self.open_discovery_folio(h.block);
                    return;
                }
                Some("discovery_writing") if self.input.action_cooldown <= 0.0 => {
                    self.input.action_cooldown = 0.35;
                    self.input.right_held = false;
                    self.copy_at_writing_surface(h.block);
                    return;
                }
                Some("discovery_lab") if self.input.action_cooldown <= 0.0 => {
                    self.input.action_cooldown = 0.35;
                    self.input.right_held = false;
                    self.exchange_discovery_apparatus_item(h.block);
                    return;
                }
                Some("lens_assembly") if self.input.action_cooldown <= 0.0 => {
                    self.input.action_cooldown = 0.35;
                    self.input.right_held = false;
                    self.assemble_tuning_lens(h.block);
                    return;
                }
                Some("binding_frame") if self.input.action_cooldown <= 0.0 => {
                    self.input.action_cooldown = 0.35;
                    self.input.right_held = false;
                    self.operate_binding_frame(h.block);
                    return;
                }
                Some("alchemy_mortar" | "alchemy_basin" | "alchemy_alembic" | "alchemy_filter")
                    if self.input.action_cooldown <= 0.0 =>
                {
                    self.input.action_cooldown = 0.25;
                    self.input.right_held = false;
                    self.operate_alchemy_contextual(h.block);
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
                        if self.inventory.slots[self.input.hotbar_sel]
                            .is_some_and(|stack| stack.durability == 0)
                        {
                            self.toast(
                                "The cutting is still matter, but its living interval has spent itself."
                                    .to_string(),
                            );
                            return;
                        }
                        // What you carry decides what wakes: its own
                        // kind reawakens, a stranger's replaces.
                        let seed_stack = self.inventory.slots[self.input.hotbar_sel]
                            .expect("holding_seed was derived from this authoritative slot");
                        match self
                            .server
                            .world
                            .plant_heart_seed_stack_at(h.block, seed_stack)
                        {
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
                            if let Some(consumed) =
                                self.inventory.take_one_stack(self.input.hotbar_sel)
                                && self.multiplayer.remote.is_none()
                            {
                                if let Err(error) =
                                    self.server.world.record_consumed_stacks([consumed])
                                {
                                    eprintln!("materials: compost feed accounting failed: {error}");
                                }
                                self.server.world.retire_arcane_stack_at(
                                    h.block,
                                    consumed,
                                    "magical biomass composted",
                                );
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
            // Slot-module swap (spec Part 1.3): right-click an installed
            // module while holding a replacement from its category. The
            // host applies the swap (the world's 2c hook re-folds the
            // frame); guests see it through the host's echo.
            if self.multiplayer.remote.is_none()
                && let Some(category) = self.server.world.slot_category_at(h.block)
                && let Some(replacement) = held.and_then(|i| reg.item(i).places)
                && self.server.world.get_block_at(h.block) != replacement
                && crate::world::multiblock::modules_in_category(&reg, category)
                    .contains(&replacement)
            {
                if let Ok(()) =
                    self.server
                        .world
                        .swap_slot_module_at(h.block, category, replacement)
                {
                    if !self.creative {
                        self.inventory.take_one(self.input.hotbar_sel);
                    }
                    self.sfx(Sfx::Place);
                }
                self.input.action_cooldown = 0.3;
                return;
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
                    let placed = if self.creative {
                        self.server.world.place_block_at(pos, block)
                    } else {
                        self.inventory.slots[self.input.hotbar_sel]
                            .is_some_and(|stack| self.server.world.place_item_block_at(pos, stack))
                    };
                    if placed {
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

    fn interact_wand(&mut self, dt: f32, hit: Option<&raycast::PlanetHit>) -> bool {
        use crate::workings::{WorkingIntent, WorkingTargetIntent};

        let left_range = if self.multiplayer.remote.is_none() {
            self.interaction.working.as_ref().and_then(|channel| {
                let source = self.player.pos.block()?;
                (channel.stable_id != 0
                    && !self
                        .server
                        .world
                        .wand_working_reachable_from(channel.stable_id, source))
                .then_some(channel.stable_id)
            })
        } else {
            None
        };
        if let Some(stable_id) = left_range {
            self.interaction.working = None;
            let mut cue = self
                .server
                .world
                .working_cues()
                .into_iter()
                .find(|cue| cue.stable_id == stable_id);
            match self.server.world.interrupt_working(stable_id) {
                Ok(result) => {
                    if let Some(cue) = cue.as_mut() {
                        cue.kind = result.cue;
                        cue.warning_band = result.warning_band;
                        cue.completion_permille = 1_000;
                    }
                    if let Some(cue) = cue {
                        self.present_working_cue(cue);
                    }
                    self.toast("The wand path leaves its bounded reach and breaks cleanly.".into());
                }
                Err(error) => self.toast(error),
            }
            return true;
        }

        let held_stack = self.inventory.slots[self.input.hotbar_sel];
        let held_wand = held_stack.filter(|stack| {
            stack.arcane_id != 0
                && self
                    .content
                    .reg
                    .item(stack.item)
                    .implement
                    .as_ref()
                    .is_some_and(|definition| {
                        definition.kind == crate::implements::ImplementItemKind::Wand
                    })
        });
        if let Some(channel) = self.interaction.working.as_ref()
            && held_wand.is_none_or(|wand| wand.arcane_id != channel.wand_id)
        {
            let channel = self.interaction.working.take().unwrap();
            if let Some(remote) = &self.multiplayer.remote {
                remote.client.send(&net::C2S::OperateWorking {
                    working_id: channel.working_id,
                    held_instance: channel.wand_id,
                    target: channel.target,
                    intent: WorkingIntent::Cancel,
                });
            } else if channel.stable_id != 0
                && let Err(error) = self.server.world.interrupt_working(channel.stable_id)
            {
                self.toast(error);
            }
            return true;
        }
        let Some(wand) = held_wand else {
            return false;
        };
        if let Some(channel) = self.interaction.working.as_mut() {
            if self.input.right_held {
                channel.held_secs += dt;
                if channel.held_secs >= crate::workings::MIN_WAND_SETTLE_SECONDS
                    && !channel.hold_sent
                {
                    channel.hold_sent = true;
                    let working_id = channel.working_id.clone();
                    let wand_id = channel.wand_id;
                    let target = channel.target;
                    let stable_id = channel.stable_id;
                    if let Some(remote) = &self.multiplayer.remote {
                        remote.client.send(&net::C2S::OperateWorking {
                            working_id,
                            held_instance: wand_id,
                            target,
                            intent: WorkingIntent::Hold,
                        });
                    } else if stable_id != 0 {
                        match self.server.world.activate_working(stable_id) {
                            Ok(result) => self.toast(result.message),
                            Err(error) if !error.contains("cannot move") => self.toast(error),
                            Err(_) => {}
                        }
                    }
                }
                return true;
            }
            let channel = self.interaction.working.take().unwrap();
            if let Some(remote) = &self.multiplayer.remote {
                remote.client.send(&net::C2S::OperateWorking {
                    working_id: channel.working_id,
                    held_instance: channel.wand_id,
                    target: channel.target,
                    intent: WorkingIntent::Release,
                });
            } else if channel.stable_id != 0 {
                let completion = if channel.working_id == "base:fieldmend" {
                    self.server
                        .world
                        .complete_inventory_working(channel.stable_id, &mut self.inventory)
                } else {
                    self.server.world.release_working(channel.stable_id)
                };
                match completion {
                    Ok(result) => {
                        if result.phase == Some(crate::workings::WorkingPhase::PendingApply) {
                            match self.save_player() {
                                Ok(()) => match self
                                    .server
                                    .world
                                    .finish_inventory_working(channel.stable_id)
                                {
                                    Ok(finished) => self.toast(finished.message),
                                    Err(error) => self.toast(error),
                                },
                                Err(error) => self.toast(format!(
                                    "Fieldmend landed, but its profile checkpoint failed: {error}"
                                )),
                            }
                        } else {
                            self.toast(result.message);
                        }
                        self.sfx(Sfx::ImplementUse);
                    }
                    Err(error) => {
                        self.toast(error);
                        self.sfx(Sfx::ImplementFailure);
                    }
                }
            }
            self.input.action_cooldown = crate::workings::WAND_RECOVERY_SECONDS;
            return true;
        }
        if !self.input.right_held || self.input.action_cooldown > 0.0 {
            return false;
        }
        let source = match self.player.pos.block() {
            Some(source) => source,
            None => return true,
        };
        let water_hit = raycast::raycast_water_at(
            &self.server.world,
            self.player.eye(),
            self.camera.local_forward(),
            self.reach(),
        )
        .filter(|water| {
            self.content
                .reg
                .is_water(self.server.world.get_block_at(water.block))
        });
        let eye = self.player.eye();
        let forward = self.camera.local_forward().normalize_or_zero();
        let reach = self.reach();
        let entity_target = self
            .server
            .world
            .projectiles()
            .iter()
            .map(|projectile| (projectile.pos, projectile.stable_id, 0.45))
            .chain(
                self.server
                    .world
                    .loose_items()
                    .iter()
                    .map(|item| (item.pos, item.stable_id, 0.35)),
            )
            .filter_map(|(pos, stable_id, radius)| {
                let delta = eye.local_delta_to(pos);
                let along = delta.dot(forward);
                (along > 0.0 && along <= reach && (delta - forward * along).length() <= radius)
                    .then_some((along, stable_id))
            })
            .min_by(|left, right| left.0.total_cmp(&right.0))
            .map(|(_, stable_id)| stable_id);
        let (working_id, target) = if let Some(stable_id) = entity_target {
            (
                "base:nudge".to_string(),
                WorkingTargetIntent::Entity { stable_id },
            )
        } else if let Some(water) = water_hit
            && (self
                .content
                .reg
                .is_air(self.server.world.get_block_at(water.adjacent))
                || self
                    .content
                    .reg
                    .is_water(self.server.world.get_block_at(water.adjacent)))
        {
            (
                "base:draw".to_string(),
                WorkingTargetIntent::Water {
                    from: water.block,
                    to: water.adjacent,
                    water_hu: crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL,
                },
            )
        } else if let Some(hit) = hit {
            let block = self.server.world.get_block_at(hit.block);
            let definition = self.content.reg.block(block);
            if definition.interaction.as_deref() == Some("discovery_lab")
                && (self.multiplayer.remote.is_some()
                    || self.server.world.holdfast_mounted_target_at(hit.block))
            {
                // The client identifies only the physical mount. Its hidden
                // sample contents remain host-owned and are validated by the
                // Holdfast handler before any Current is reserved.
                (
                    "base:holdfast".to_string(),
                    WorkingTargetIntent::Block {
                        pos: hit.block,
                        adjacent: None,
                    },
                )
            } else if self.server.world.is_nudge_mechanism_at(hit.block) {
                (
                    "base:nudge".to_string(),
                    WorkingTargetIntent::Block {
                        pos: hit.block,
                        adjacent: None,
                    },
                )
            } else if definition.crop_next.is_some() || definition.sapling.is_some() {
                (
                    "base:rootwake".to_string(),
                    WorkingTargetIntent::Block {
                        pos: hit.block,
                        adjacent: None,
                    },
                )
            } else if definition.burns != 0 {
                (
                    "base:kindle".to_string(),
                    WorkingTargetIntent::Block {
                        pos: hit.block,
                        adjacent: Some(hit.adjacent),
                    },
                )
            } else {
                (
                    "base:trace".to_string(),
                    WorkingTargetIntent::Block {
                        pos: hit.block,
                        adjacent: None,
                    },
                )
            }
        } else {
            // Inventory workings use a physical little tableau instead of a
            // spell hotbar: target immediately right of the wand, matching
            // stock one slot farther right. With no valid tableau, empty-air
            // use remains Gleam.
            let target_slot = (self.input.hotbar_sel + 1) % crate::inventory::HOTBAR_SLOTS;
            let material_slot = (self.input.hotbar_sel + 2) % crate::inventory::HOTBAR_SLOTS;
            let staged = self.inventory.slots[target_slot];
            let matching = staged.and_then(|target| {
                let definition = self.content.reg.item(target.item);
                let repair = self
                    .content
                    .reg
                    .item_id(&format!("{}/forge_scrap", definition.name))?;
                self.inventory.slots[material_slot]
                    .is_some_and(|stock| stock.item == repair && stock.count == 1)
                    .then_some(())
            });
            let fragile = staged.is_some_and(|stack| {
                let definition = self.content.reg.item(stack.item);
                stack.count == 1
                    && definition.durability != 0
                    && (definition.food.is_some()
                        || definition.name.ends_with("_seed")
                        || (definition.arcane.is_some() && definition.places.is_some()))
            });
            if matching.is_some() {
                (
                    "base:fieldmend".to_string(),
                    WorkingTargetIntent::Inventory {
                        target_slot: target_slot as u8,
                        material_slot: Some(material_slot as u8),
                        magnitude: 16,
                    },
                )
            } else if fragile {
                (
                    "base:holdfast".to_string(),
                    WorkingTargetIntent::Inventory {
                        target_slot: target_slot as u8,
                        material_slot: None,
                        magnitude: 1,
                    },
                )
            } else {
                ("base:gleam".to_string(), WorkingTargetIntent::None)
            }
        };
        // Ctrl + use is an explicit unsafe choice. It never changes the
        // effect requested by the client; it only authorizes the host to draw
        // below the measured safe floor with visible, deterministic cost.
        let forced = self.input.keys.sprint;
        let start_intent = if forced {
            WorkingIntent::StartForced
        } else {
            WorkingIntent::Start
        };
        if let Some(remote) = &self.multiplayer.remote {
            remote.client.send(&net::C2S::OperateWorking {
                working_id: working_id.clone(),
                held_instance: wand.arcane_id,
                target,
                intent: start_intent,
            });
            self.interaction.working = Some(LocalWorkingChannel {
                stable_id: 0,
                working_id,
                wand_id: wand.arcane_id,
                target,
                held_secs: 0.0,
                hold_sent: false,
            });
        } else {
            let player_id = identity::local_player_id(
                &self.server.world.save_dir_for_saving(),
                self.identity.device_id(),
            )
            .unwrap_or(identity::PlayerId([0; 16]));
            let result = self.server.world.begin_wand_working(
                player_id.0,
                &self.config.display_name,
                source,
                wand.arcane_id,
                &working_id,
                target,
                Some(&self.inventory),
                forced,
            );
            match result {
                Ok(result) => {
                    if let Some(cue) = self
                        .server
                        .world
                        .working_cues()
                        .into_iter()
                        .find(|cue| cue.stable_id == result.stable_id)
                    {
                        self.present_working_cue(cue);
                    }
                    self.toast(result.message);
                    self.interaction.working = Some(LocalWorkingChannel {
                        stable_id: result.stable_id,
                        working_id,
                        wand_id: wand.arcane_id,
                        target,
                        held_secs: 0.0,
                        hold_sent: false,
                    });
                }
                Err(error) => {
                    self.toast(error);
                    self.sfx(Sfx::ImplementFailure);
                }
            }
        }
        self.input.action_cooldown = 0.1;
        true
    }

    /// Apply world mutations queued by scripts during the last dispatch.
    pub(super) fn apply_script_cmds(&mut self) {
        let reg = self.content.reg.clone();
        for cmd in self.content.scripts.take_cmds() {
            match cmd {
                script::Cmd::SetBlock(pos, name) => {
                    if let Some(b) = reg.block_id(&name) {
                        if reg.block(b).arcane_ecology.is_some() {
                            eprintln!(
                                "arcane ecology: script placement of {name} rejected; lifecycle sites are engine-owned"
                            );
                            continue;
                        }
                        if reg.block(b).name.starts_with("base:scar_")
                            || reg
                                .block(b)
                                .observation
                                .as_ref()
                                .is_some_and(|observation| {
                                    observation
                                        .categories
                                        .iter()
                                        .any(|category| category == "scar")
                                })
                        {
                            eprintln!(
                                "dross: script placement of {name} rejected; scar manifestations are ledger-owned"
                            );
                            continue;
                        }
                        self.server
                            .world
                            .set_block_authored_at(pos, b, "mod script world event");
                    }
                }
                script::Cmd::Give(name, n) => {
                    if let Some(item) = reg.item_id(&name) {
                        let item_definition = &reg.items[item.0 as usize];
                        let ecology_product = item_definition.arcane_ecology.is_some()
                            || item_definition
                                .places
                                .is_some_and(|block| reg.block(block).arcane_ecology.is_some())
                            || reg.blocks.iter().any(|block| {
                                block.arcane_ecology.is_some()
                                    && block.drops.is_some_and(|(drop, _)| drop == item)
                            });
                        if ecology_product {
                            eprintln!(
                                "arcane ecology: script give of {name} rejected; growth and harvest are authoritative"
                            );
                            continue;
                        }
                        let mut stack = ItemStack::new(&reg, item, n);
                        if reg.item(item).arcane.is_some() {
                            if n != 1 {
                                eprintln!("arcane: script give rejected a charged stack of {n}");
                                continue;
                            }
                            let Some(at) = self.player.pos.block() else {
                                continue;
                            };
                            if let Err(error) = self.server.world.bind_arcane_stack_at(
                                at,
                                &mut stack,
                                "mod script discovery",
                            ) {
                                eprintln!("arcane: script give rejected: {error}");
                                continue;
                            }
                        }
                        if let Some(ledger) = &mut self.server.world.material_ledger
                            && let Err(error) =
                                ledger.record_external_stack(&reg, stack, "mod script give")
                        {
                            eprintln!("materials: script give accounting failed: {error}");
                        }
                        let left = self.inventory.add_stack(&reg, stack);
                        if left > 0 {
                            self.drop_stack(ItemStack {
                                count: left,
                                ..stack
                            });
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
                script::Cmd::SpawnNpc(name, pos) => {
                    if let Some(ni) = reg.npc_id(&name)
                        && self.server.world.mob_count() < world::MOB_CAP
                        && self.server.world.npc_count() < world::NPC_CAP
                    {
                        self.server.world.spawn_npc_at(ni, pos);
                    }
                }
                script::Cmd::QuestProgress {
                    quest_id,
                    objective,
                    n,
                } => {
                    if !objective.is_empty() {
                        self.quest_progress_apply(&quest_id, &objective, n);
                    }
                }
                script::Cmd::QuestAccept(quest_id) => {
                    if let Err(error) = self.quest_accept(&quest_id) {
                        self.toast(error);
                    }
                }
                script::Cmd::ArcaneMoveWorking {
                    mod_id,
                    from,
                    to,
                    resonance,
                    units,
                    reason,
                } => {
                    let result = self
                        .server
                        .world
                        .arcane_ledger
                        .as_mut()
                        .ok_or_else(|| "world has no arcane ledger".to_string())
                        .and_then(|ledger| {
                            ledger
                                .mod_working_transfer(
                                    &mod_id,
                                    from,
                                    to,
                                    crate::arcane::Current::single(resonance, units),
                                    &reason,
                                )
                                .map(|_| ())
                                .map_err(|error| error.to_string())
                        });
                    if let Err(error) = result {
                        eprintln!("[mod:{mod_id}] arcane transaction rejected: {error}");
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
