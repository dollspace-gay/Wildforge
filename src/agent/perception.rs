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
    fn loaded(&self, x: i32, z: i32) -> bool {
        // Streamed chunks always carry the bedrock floor.
        self.world.get_block(x, 0, z) != registry::AIR
    }

    fn surface_y(&self, x: i32, z: i32, around_y: i32) -> Option<i32> {
        (around_y - 14..=around_y + 12)
            .rev()
            .find(|&y| self.reg.is_solid(self.world.get_block(x, y, z)))
    }

    fn block_name(&self, x: i32, y: i32, z: i32) -> String {
        self.reg.block(self.world.get_block(x, y, z)).name.clone()
    }

    /// The compact digest: who/where/when, a 21x21 minimap (2 blocks
    /// per cell), company, and anything the wire recently said.
    pub fn look_around(&self) -> String {
        let p = self.player.pos;
        let (px, py, pz) = motion::cell_of(p);
        let mut out = String::new();
        out.push_str(&format!(
            "pos {px} {py} {pz}; {}; day {}; {:?}; health {:.0}/14 hunger {:.0}/20\n",
            time_phase(self.time_of_day),
            self.world.day,
            self.world.weather,
            self.health,
            self.hunger,
        ));
        out.push_str(&format!(
            "standing on {}; in {}\n",
            self.block_name(px, py - 1, pz),
            self.block_name(px, py, pz),
        ));
        // Minimap: 2-block cells, north up. Legend in the footer.
        out.push_str("map (21x21, 2 blocks/cell, north up):\n");
        for row in -10i32..=10 {
            for col in -10i32..=10 {
                let (x, z) = (px + col * 2, pz + row * 2);
                let ch = if (row, col) == (0, 0) {
                    '@'
                } else if !self.loaded(x, z) {
                    '?'
                } else if let Some(sy) = self.surface_y(x, z, py) {
                    let b = self.world.get_block(x, sy, z);
                    let name = &self.reg.block(b).name;
                    let over = self.world.get_block(x, sy + 1, z);
                    if self.reg.water_volume(over).is_some() {
                        '~'
                    } else if name.contains("log") || name.contains("leaves") {
                        'T'
                    } else if sy > py + 3 {
                        '#'
                    } else if sy > py + 1 {
                        '^'
                    } else if sy < py - 4 {
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
            let d = *pos - p;
            out.push_str(&format!(
                "player {name} (id {id}): {:.0} blocks {}\n",
                d.length(),
                octant(d.x as i32, d.z as i32),
            ));
        }
        let mut counts: HashMap<&str, (usize, f32)> = HashMap::new();
        for m in self.world.mobs() {
            let d = (m.pos - p).length();
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
                    "{n} (id {id}) at {:.0} {:.0} {:.0}, {:.0} blocks away",
                    pos.x,
                    pos.y,
                    pos.z,
                    (*pos - p).length()
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
        let (px, py, pz) = motion::cell_of(p);
        let r = radius.clamp(2, 48);
        let mut hits: Vec<((i32, i32, i32), f32)> = Vec::new();
        for x in px - r..=px + r {
            for z in pz - r..=pz + r {
                if !self.loaded(x, z) {
                    continue;
                }
                for y in (py - r).max(1)..=py + r {
                    if matcher(self.world.get_block(x, y, z)) {
                        let d = Vec3::new(x as f32 - p.x, y as f32 - p.y, z as f32 - p.z).length();
                        hits.push(((x, y, z), d));
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
        let mut last: Option<(i32, i32, i32)> = None;
        for (c, d) in hits {
            // One line per cluster, not per block of the same trunk.
            if let Some(l) = last
                && (c.0 - l.0).abs() + (c.1 - l.1).abs() + (c.2 - l.2).abs() < 4
            {
                continue;
            }
            out.push_str(&format!(
                "{} at {} {} {} - {d:.0} blocks {}\n",
                self.block_name(c.0, c.1, c.2),
                c.0,
                c.1,
                c.2,
                octant(c.0 - px, c.2 - pz),
            ));
            last = Some(c);
            shown += 1;
            if shown >= 5 {
                break;
            }
        }
        out
    }

    pub fn at(&self, x: i32, y: i32, z: i32) -> String {
        if !self.loaded(x, z) {
            return "that chunk hasn't streamed to you".into();
        }
        let (bl, sky) = self.world.light_at(x, y, z);
        // Farmland wears its fertility in its meta byte; report it so
        // an agent can judge a field the way a farmer reads the tint.
        let soil = if self
            .reg
            .block(self.world.get_block(x, y, z))
            .fert_tiles
            .is_some()
        {
            format!(
                "; soil {}/{}",
                self.world.fertility_at(x, y, z),
                crate::world::soil::FERT_MAX
            )
        } else {
            String::new()
        };
        format!(
            "{} (light {bl}, sky {sky}){soil}; above: {}; below: {}",
            self.block_name(x, y, z),
            self.block_name(x, y + 1, z),
            self.block_name(x, y - 1, z),
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
                let bar = if i < HOTBAR_SLOTS { " [hotbar]" } else { "" };
                out.push_str(&format!("slot {i}: {}x {}{wear}{bar}\n", s.count, d.name));
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
                    "walking to {} {} {} ({} waypoints left)",
                    goal.0,
                    goal.1,
                    goal.2,
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
            "pos {:.1} {:.1} {:.1}; health {:.0}; hunger {:.0}; {}; {}; behavior: {doing}",
            p.x,
            p.y,
            p.z,
            self.health,
            self.hunger,
            time_phase(self.time_of_day),
            format!("{:?}", self.world.weather).to_lowercase(),
        )
    }
}
