//! Perception: queries, not chunk dumps. Everything here reads only
//! the streamed mirror — what a player standing in the agent's boots
//! could know.

use super::*;

/// Compass octant of a world-space offset ("north" is -z).
pub fn octant(dx: i32, dz: i32) -> &'static str {
    if dx == 0 && dz == 0 {
        return "here";
    }
    let a = (dx as f32).atan2(-(dz as f32)).to_degrees();
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
}

fn time_phase(t: f32) -> &'static str {
    match t {
        t if t < 0.05 => "dawn",
        t if t < 0.42 => "day",
        t if t < 0.55 => "dusk",
        t if t < 0.95 => "night",
        _ => "dawn",
    }
}

impl Agent {
    fn loaded_at(&self, pos: crate::planet::BlockPos) -> bool {
        // Streamed chunks always carry the bedrock floor.
        self.world.get_block_at(pos.with_y(0)) != registry::AIR
    }

    fn surface_near(
        &self,
        pos: crate::planet::BlockPos,
        around_y: i32,
    ) -> Option<crate::planet::BlockPos> {
        (around_y - 14..=around_y + 12)
            .rev()
            .filter_map(|y| u8::try_from(y).ok().map(|y| pos.with_y(y)))
            .find(|&at| self.reg.is_solid(self.world.get_block_at(at)))
    }

    fn block_name_at(&self, pos: crate::planet::BlockPos) -> String {
        self.reg.block(self.world.get_block_at(pos)).name.clone()
    }

    /// The compact digest: who/where/when, a 21x21 minimap (2 blocks
    /// per cell), company, and anything the wire recently said.
    pub fn look_around(&self) -> String {
        let p = self.player.pos;
        let Some(feet) = motion::cell_of(p) else {
            return "outside the voxel shell".into();
        };
        let py = i32::from(feet.y());
        let weather = self.world.weather_at_surface(feet.surface());
        let mut out = String::new();
        out.push_str(&format!(
            "pos {} {} {} {}; {}; day {}; {}; health {:.0}/14 hunger {:.0}/20\n",
            feet.face().name(),
            feet.u(),
            feet.y(),
            feet.v(),
            time_phase(self.time_of_day),
            self.world.day,
            weather.kind.name(),
            self.health,
            self.hunger,
        ));
        out.push_str(&format!(
            "standing on {}; in {}\n",
            feet.offset(0, -1, 0)
                .map_or_else(|| "bedrock".into(), |at| self.block_name_at(at)),
            self.block_name_at(feet),
        ));
        let local_arcane = self.world.planet_atlas().and_then(|atlas| {
            self.world
                .arcane_survey_at(atlas.atlas_pos(feet.surface()), false)
        });
        if let Some(survey) = local_arcane {
            let cue = survey.sensory_cue();
            out.push_str(&format!(
                "arcane signs: {}; wild ire {:.0}/100\n",
                cue.trim_end_matches('.'),
                self.world.ire
            ));
        } else {
            let cue = crate::arcane_geography::coarse_sensory_cue(
                self.world.remote_arcane_cue(),
                self.world.remote_arcane_dominant(),
            );
            out.push_str(&format!(
                "arcane signs: {}; wild ire {:.0}/100\n",
                cue.trim_end_matches('.'),
                self.world.ire
            ));
        }
        if let Some(observation) = self.world.perceived_arcane_ecology_at(feet.surface()) {
            out.push_str(&format!("ecology: {}\n", observation.text));
        }
        // Minimap: 2-block cells, north up. Legend in the footer.
        out.push_str("map (21x21, 2 blocks/cell, north up):\n");
        for row in -10i32..=10 {
            for col in -10i32..=10 {
                let Some(column) = feet.offset(col * 2, 0, row * 2) else {
                    out.push('?');
                    continue;
                };
                let ch = if (row, col) == (0, 0) {
                    '@'
                } else if !self.loaded_at(column) {
                    '?'
                } else if let Some(surface) = self.surface_near(column, py) {
                    let b = self.world.get_block_at(surface);
                    let name = &self.reg.block(b).name;
                    let over = surface
                        .offset(0, 1, 0)
                        .map_or(registry::AIR, |at| self.world.get_block_at(at));
                    if self.reg.water_volume(over).is_some() {
                        '~'
                    } else if name.contains("log") || name.contains("leaves") {
                        'T'
                    } else if i32::from(surface.y()) > py + 3 {
                        '#'
                    } else if i32::from(surface.y()) > py + 1 {
                        '^'
                    } else if i32::from(surface.y()) < py - 4 {
                        'v'
                    } else {
                        '.'
                    }
                } else {
                    '~'
                };
                out.push(ch);
            }
            out.push('\n');
        }
        out.push_str("(@ you, T trees, ~ water/void, ^ rise, # cliff, v drop, ? unstreamed)\n");
        for (id, (name, pos, _)) in &self.players {
            let d = p.local_delta_to(*pos);
            out.push_str(&format!(
                "player {name} (id {id}): {:.0} blocks {}\n",
                p.distance_to(*pos),
                octant(d.x as i32, d.z as i32),
            ));
        }
        let mut counts: HashMap<&str, (usize, f32)> = HashMap::new();
        for m in self.world.mobs() {
            let d = m.pos.distance_to(p);
            if d < 32.0 {
                let e = counts
                    .entry(self.reg.animals[m.species].name.as_str())
                    .or_insert((0, f32::MAX));
                e.0 += 1;
                e.1 = e.1.min(d);
            }
        }
        for (species, (n, d)) in counts {
            out.push_str(&format!("{n}x {species} nearby (closest {d:.0})\n"));
        }
        for item in self
            .world
            .loose_items()
            .iter()
            .filter(|item| item.pos.distance_to(p) < 32.0)
        {
            let delta = p.local_delta_to(item.pos);
            out.push_str(&format!(
                "dropped {}x {} (entity id {}) {:.0} blocks {}\n",
                item.count,
                self.reg.item(item.item).name,
                item.stable_id,
                item.pos.distance_to(p),
                octant(delta.x as i32, delta.z as i32),
            ));
        }
        for projectile in self
            .world
            .projectiles()
            .iter()
            .filter(|projectile| projectile.pos.distance_to(p) < 32.0)
        {
            let delta = p.local_delta_to(projectile.pos);
            out.push_str(&format!(
                "projectile (entity id {}) {:.0} blocks {}\n",
                projectile.stable_id,
                projectile.pos.distance_to(p),
                octant(delta.x as i32, delta.z as i32),
            ));
        }
        out
    }

    /// Nearest matches for a kind: a block/item name ("base:log"), a
    /// tag ("#base:logs"), "water", "tree", or "player <name>".
    pub fn nearest(&self, kind: &str, radius: i32) -> String {
        let p = self.player.pos;
        if let Some(name) = kind.strip_prefix("player ") {
            let want = name.trim().to_lowercase();
            let hit = self
                .players
                .iter()
                .find(|(_, (n, _, _))| n.to_lowercase().contains(&want));
            return match hit {
                Some((id, (n, pos, _))) => format!(
                    "{n} (id {id}) at {} {:.0} {:.0} {:.0}, {:.0} blocks away",
                    pos.face().name(),
                    pos.u(),
                    pos.y(),
                    pos.v(),
                    pos.distance_to(p)
                ),
                None => "no such player in sight".into(),
            };
        }
        let kind = if kind == "tree" { "#base:logs" } else { kind };
        let matcher: Box<dyn Fn(crate::registry::BlockId) -> bool> = if kind == "water" {
            let reg = self.reg.clone();
            Box::new(move |b| reg.water_volume(b).is_some())
        } else if let Some(tag) = kind.strip_prefix('#') {
            let items = self.reg.tags.get(tag).cloned().unwrap_or_default();
            let reg = self.reg.clone();
            Box::new(move |b| {
                reg.item_id(&reg.block(b).name)
                    .is_some_and(|i| items.contains(&i))
            })
        } else {
            let want = self.reg.block_id(kind);
            if want.is_none() {
                return format!("unknown kind {kind}");
            }
            Box::new(move |b| Some(b) == want)
        };
        let Some(origin) = motion::cell_of(p) else {
            return "outside the voxel shell".into();
        };
        let py = i32::from(origin.y());
        let r = radius.clamp(2, 48);
        let mut hits: Vec<(crate::planet::BlockPos, f32)> = Vec::new();
        for du in -r..=r {
            for dv in -r..=r {
                let Some(column) = origin.offset(du, 0, dv) else {
                    continue;
                };
                if !self.loaded_at(column) {
                    continue;
                }
                for y in (py - r).max(1)..=py + r {
                    let Some(at) = u8::try_from(y).ok().map(|y| column.with_y(y)) else {
                        continue;
                    };
                    if matcher(self.world.get_block_at(at)) {
                        hits.push((at, p.distance_to(at.entity_center())));
                    }
                }
            }
        }
        hits.sort_by(|a, b| a.1.total_cmp(&b.1));
        if hits.is_empty() {
            return format!("no {kind} within {r} blocks (of streamed ground)");
        }
        let mut out = String::new();
        let mut shown = 0;
        let mut last: Option<crate::planet::BlockPos> = None;
        for (c, d) in hits {
            // One line per cluster, not per block of the same trunk.
            if let Some(l) = last
                && c.entity_center().distance_to(l.entity_center()) < 4.0
            {
                continue;
            }
            let delta = p.local_delta_to(c.entity_center());
            out.push_str(&format!(
                "{} at {} {} {} {} - {d:.0} blocks {}\n",
                self.block_name_at(c),
                c.face().name(),
                c.u(),
                c.y(),
                c.v(),
                octant(delta.x as i32, delta.z as i32),
            ));
            last = Some(c);
            shown += 1;
            if shown >= 5 {
                break;
            }
        }
        out
    }

    pub fn at(&self, pos: crate::planet::BlockPos) -> String {
        if !self.loaded_at(pos) {
            return "that chunk hasn't streamed to you".into();
        }
        let (bl, sky) = self.world.light_at_pos(pos);
        // Farmland wears its fertility in its meta byte; report it so
        // an agent can judge a field the way a farmer reads the tint.
        let soil = if self
            .reg
            .block(self.world.get_block_at(pos))
            .fert_tiles
            .is_some()
        {
            format!(
                "; soil {}/{}",
                self.world.fertility_at_pos(pos),
                crate::world::soil::FERT_MAX
            )
        } else {
            String::new()
        };
        format!(
            "{} (light {bl}, sky {sky}){soil}; above: {}; below: {}",
            self.block_name_at(pos),
            pos.offset(0, 1, 0)
                .map_or_else(|| "outside shell".into(), |at| self.block_name_at(at)),
            pos.offset(0, -1, 0)
                .map_or_else(|| "outside shell".into(), |at| self.block_name_at(at)),
        )
    }

    pub fn inventory_text(&self) -> String {
        let mut out = String::new();
        for (i, s) in self.inventory.slots.iter().enumerate() {
            if let Some(s) = s {
                let d = self.reg.item(s.item);
                let wear = if d.durability > 0 {
                    format!(" ({}/{})", s.durability, d.durability)
                } else {
                    String::new()
                };
                let charge = self
                    .world
                    .inspectable_item_current(s.arcane_id)
                    .map_or_else(String::new, |units| {
                        let capacity = d
                            .arcane
                            .as_ref()
                            .map_or(units.max(1), |arcane| arcane.capacity);
                        format!(
                            " [Current {}]",
                            crate::arcane::qualitative_current(units, capacity)
                        )
                    });
                let bar = if i < HOTBAR_SLOTS { " [hotbar]" } else { "" };
                out.push_str(&format!(
                    "slot {i}: {}x {}{wear}{charge}{bar}\n",
                    s.count, d.name
                ));
            }
        }
        if out.is_empty() {
            out.push_str("empty-handed\n");
        }
        out.push_str(&format!("held (slot {}): {}\n", self.hotbar, {
            match self.inventory.slots[self.hotbar] {
                Some(s) => self.reg.item(s.item).name.clone(),
                None => "nothing".into(),
            }
        }));
        out
    }

    pub fn status(&self) -> String {
        let p = self.player.pos;
        let doing = match &self.behavior {
            Behavior::Idle => "idle".to_string(),
            Behavior::GoTo { goal, path } => {
                format!(
                    "walking to {} {} {} {} ({} waypoints left)",
                    goal.face().name(),
                    goal.u(),
                    goal.y(),
                    goal.v(),
                    path.len()
                )
            }
            Behavior::Follow { id, .. } => format!(
                "following {}",
                self.players
                    .get(id)
                    .map(|(n, _, _)| n.clone())
                    .unwrap_or_else(|| format!("player {id}"))
            ),
        };
        format!(
            "pos {} {:.1} {:.1} {:.1}; health {:.0}; hunger {:.0}; {}; {}; behavior: {doing}",
            p.face().name(),
            p.u(),
            p.y(),
            p.v(),
            self.health,
            self.hunger,
            time_phase(self.time_of_day),
            self.world.weather_at_surface(p.surface()).kind.name(),
        )
    }
}
