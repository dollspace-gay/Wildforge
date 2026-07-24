//! Persistent block-entity serialization and world save-directory access.

use super::*;

impl World {
    pub(super) fn entities_path(&self) -> PathBuf {
        self.save_dir.join("entities.toml")
    }

    pub(super) fn save_entities(&self) {
        use std::fmt::Write as _;
        let mut out = String::new();
        for ((x, y, z), e) in &self.block_entities {
            match e {
                BlockEntity::Furnace(f) => {
                    let _ = writeln!(out, "[[furnace]]\npos = [{x}, {y}, {z}]");
                    let mut slot = |k: &str, s: &Option<ItemStack>| {
                        if let Some(s) = s {
                            let _ = writeln!(
                                out,
                                "{k} = {{ item = \"{}\", count = {}, durability = {} }}",
                                self.reg.item(s.item).name,
                                s.count,
                                s.durability
                            );
                        }
                    };
                    slot("input", &f.input);
                    slot("fuel", &f.fuel);
                    slot("output", &f.output);
                    let _ = writeln!(
                        out,
                        "progress = {}\nburn_left = {}\nburn_total = {}\nburn_speed = {}\n",
                        f.progress, f.burn_left, f.burn_total, f.burn_speed
                    );
                }
                BlockEntity::Chest(c) => {
                    let _ = writeln!(out, "[[chest]]\npos = [{x}, {y}, {z}]");
                    if c.wild_owned {
                        let _ = writeln!(out, "wild_owned = true");
                    }
                    for (i, st) in c.slots.iter().enumerate() {
                        if let Some(st) = st {
                            let _ = writeln!(
                                out,
                                "[[chest.slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}",
                                self.reg.item(st.item).name,
                                st.count,
                                st.durability
                            );
                        }
                    }
                    let _ = writeln!(out);
                }
                BlockEntity::Offering(o) => {
                    let _ = writeln!(out, "[[offering]]\npos = [{x}, {y}, {z}]");
                    for (i, st) in o.slots.iter().enumerate() {
                        if let Some(st) = st {
                            let _ = writeln!(
                                out,
                                "[[offering.slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}",
                                self.reg.item(st.item).name,
                                st.count,
                                st.durability
                            );
                        }
                    }
                    let _ = writeln!(out);
                }
                BlockEntity::Bloomery(b) => {
                    let _ = writeln!(
                        out,
                        "[[bloomery]]\npos = [{x}, {y}, {z}]\nlit = {}\nprogress = {}\ncore = [{}, {}, {}]",
                        b.lit, b.progress, b.core.0, b.core.1, b.core.2
                    );
                    for (i, st) in b.charge.iter().chain(b.fuel.iter()).enumerate() {
                        if let Some(st) = st {
                            let _ = writeln!(
                                out,
                                "[[bloomery.slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}",
                                self.reg.item(st.item).name,
                                st.count,
                                st.durability
                            );
                        }
                    }
                    let _ = writeln!(out);
                }
                BlockEntity::Forge(f) => {
                    let _ = writeln!(
                        out,
                        "[[forge]]\npos = [{x}, {y}, {z}]\nlit = {}\nprogress = {}\ncore = [{}, {}, {}]",
                        f.lit, f.progress, f.core.0, f.core.1, f.core.2
                    );
                    for (i, st) in f.charge.iter().chain(f.fuel.iter()).enumerate() {
                        if let Some(st) = st {
                            let _ = writeln!(
                                out,
                                "[[forge.slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}",
                                self.reg.item(st.item).name,
                                st.count,
                                st.durability
                            );
                        }
                    }
                    let _ = writeln!(out);
                }
                BlockEntity::Sign(sg) => {
                    let esc = |l: &str| l.replace(['\\', '"'], "");
                    let _ = writeln!(
                        out,
                        "[[sign]]\npos = [{x}, {y}, {z}]\nlines = [\"{}\", \"{}\", \"{}\"]\n",
                        esc(&sg.lines[0]),
                        esc(&sg.lines[1]),
                        esc(&sg.lines[2])
                    );
                }
                BlockEntity::Clamp(c) => {
                    let logs: Vec<String> = c
                        .logs
                        .iter()
                        .map(|(a, b2, c2)| format!("[{a}, {b2}, {c2}]"))
                        .collect();
                    let _ = writeln!(
                        out,
                        "[[clamp]]\npos = [{x}, {y}, {z}]\ntimer = {}\nlogs = [{}]\n",
                        c.timer,
                        logs.join(", ")
                    );
                }
                BlockEntity::Kiln(k) => {
                    let _ = writeln!(
                        out,
                        "[[kiln]]\npos = [{x}, {y}, {z}]\nlit = {}\nprogress = {}\ncore = [{}, {}, {}]",
                        k.lit, k.progress, k.core.0, k.core.1, k.core.2
                    );
                    let all: Vec<&Option<ItemStack>> = k
                        .sand
                        .iter()
                        .chain([&k.powder])
                        .chain(k.fuel.iter())
                        .collect();
                    for (i, st) in all.into_iter().enumerate() {
                        if let Some(st) = st {
                            let _ = writeln!(
                                out,
                                "[[kiln.slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}",
                                self.reg.item(st.item).name,
                                st.count,
                                st.durability
                            );
                        }
                    }
                    let _ = writeln!(out);
                }
                BlockEntity::Anvil(a) => {
                    let _ = writeln!(
                        out,
                        "[[anvil]]\npos = [{x}, {y}, {z}]\nstrikes = {}",
                        a.strikes
                    );
                    if let Some(st) = &a.bloom {
                        let _ = writeln!(
                            out,
                            "bloom = {{ item = \"{}\", count = {}, durability = {} }}",
                            self.reg.item(st.item).name,
                            st.count,
                            st.durability
                        );
                    }
                    let _ = writeln!(out);
                }
            }
        }
        if out.is_empty() {
            let _ = fs::remove_file(self.entities_path());
        } else {
            let _ = fs::write(self.entities_path(), out);
        }
    }

    pub(super) fn load_entities(&mut self) {
        use serde::Deserialize;
        #[derive(Deserialize)]
        struct SlotT {
            item: String,
            count: u32,
            durability: u32,
        }
        #[derive(Deserialize)]
        struct FurnaceT {
            pos: [i32; 3],
            input: Option<SlotT>,
            fuel: Option<SlotT>,
            output: Option<SlotT>,
            #[serde(default)]
            progress: f32,
            #[serde(default)]
            burn_left: f32,
            #[serde(default)]
            burn_total: f32,
            #[serde(default)]
            burn_speed: f32,
        }
        #[derive(Deserialize)]
        struct ChestSlotT {
            index: usize,
            item: String,
            count: u32,
            durability: u32,
        }
        #[derive(Deserialize)]
        struct ChestT {
            pos: [i32; 3],
            #[serde(default)]
            wild_owned: bool,
            #[serde(default)]
            slot: Vec<ChestSlotT>,
        }
        #[derive(Deserialize)]
        struct BloomeryT {
            pos: [i32; 3],
            #[serde(default)]
            lit: bool,
            #[serde(default)]
            progress: f32,
            #[serde(default)]
            core: Option<[i32; 3]>,
            #[serde(default)]
            slot: Vec<ChestSlotT>,
        }
        #[derive(Deserialize)]
        struct SignT {
            pos: [i32; 3],
            #[serde(default)]
            lines: Vec<String>,
        }
        #[derive(Deserialize)]
        struct ClampT {
            pos: [i32; 3],
            timer: f32,
            #[serde(default)]
            logs: Vec<[i32; 3]>,
        }
        #[derive(Deserialize)]
        struct AnvilT {
            pos: [i32; 3],
            #[serde(default)]
            strikes: u32,
            #[serde(default)]
            bloom: Option<SlotT>,
        }
        #[derive(Deserialize)]
        struct FileT {
            #[serde(default)]
            furnace: Vec<FurnaceT>,
            #[serde(default)]
            chest: Vec<ChestT>,
            #[serde(default)]
            offering: Vec<ChestT>,
            #[serde(default)]
            bloomery: Vec<BloomeryT>,
            #[serde(default)]
            clamp: Vec<ClampT>,
            #[serde(default)]
            anvil: Vec<AnvilT>,
            #[serde(default)]
            kiln: Vec<BloomeryT>,
            #[serde(default)]
            forge: Vec<BloomeryT>,
            #[serde(default)]
            sign: Vec<SignT>,
        }
        let Ok(text) = fs::read_to_string(self.entities_path()) else {
            return;
        };
        let Ok(parsed) = toml::from_str::<FileT>(&text) else {
            return;
        };
        let conv = |s: Option<SlotT>| -> Option<ItemStack> {
            let s = s?;
            let item = self.reg.item_id(&s.item)?;
            Some(ItemStack {
                item,
                count: s.count,
                durability: s.durability,
            })
        };
        for fu in parsed.furnace {
            self.block_entities.insert(
                (fu.pos[0], fu.pos[1], fu.pos[2]),
                BlockEntity::Furnace(FurnaceState {
                    input: conv(fu.input),
                    fuel: conv(fu.fuel),
                    output: conv(fu.output),
                    progress: fu.progress,
                    burn_left: fu.burn_left,
                    burn_total: fu.burn_total,
                    burn_speed: fu.burn_speed.max(1.0),
                }),
            );
        }
        for ch in parsed.chest {
            let mut state = ChestState {
                wild_owned: ch.wild_owned,
                ..Default::default()
            };
            for sl in ch.slot {
                if sl.index < CHEST_SLOTS
                    && let Some(item) = self.reg.item_id(&sl.item)
                {
                    state.slots[sl.index] = Some(ItemStack {
                        item,
                        count: sl.count,
                        durability: sl.durability,
                    });
                }
            }
            self.block_entities
                .insert((ch.pos[0], ch.pos[1], ch.pos[2]), BlockEntity::Chest(state));
        }
        for of in parsed.offering {
            let mut state = OfferingState::default();
            for sl in of.slot {
                if sl.index < 3
                    && let Some(item) = self.reg.item_id(&sl.item)
                {
                    state.slots[sl.index] = Some(ItemStack {
                        item,
                        count: sl.count,
                        durability: sl.durability,
                    });
                }
            }
            self.block_entities.insert(
                (of.pos[0], of.pos[1], of.pos[2]),
                BlockEntity::Offering(state),
            );
        }
        for bl in parsed.bloomery {
            let mut state = BloomeryState {
                lit: bl.lit,
                progress: bl.progress,
                core: bl.core.map(|c| (c[0], c[1], c[2])).unwrap_or_default(),
                ..Default::default()
            };
            for sl in bl.slot {
                if sl.index < 8
                    && let Some(item) = self.reg.item_id(&sl.item)
                {
                    let st = Some(ItemStack {
                        item,
                        count: sl.count,
                        durability: sl.durability,
                    });
                    if sl.index < 4 {
                        state.charge[sl.index] = st;
                    } else {
                        state.fuel[sl.index - 4] = st;
                    }
                }
            }
            self.block_entities.insert(
                (bl.pos[0], bl.pos[1], bl.pos[2]),
                BlockEntity::Bloomery(state),
            );
        }
        for fo in parsed.forge {
            let mut state = BloomeryState {
                lit: fo.lit,
                progress: fo.progress,
                core: fo.core.map(|c| (c[0], c[1], c[2])).unwrap_or_default(),
                ..Default::default()
            };
            for sl in fo.slot {
                if sl.index < 8
                    && let Some(item) = self.reg.item_id(&sl.item)
                {
                    let st = Some(ItemStack {
                        item,
                        count: sl.count,
                        durability: sl.durability,
                    });
                    if sl.index < 4 {
                        state.charge[sl.index] = st;
                    } else {
                        state.fuel[sl.index - 4] = st;
                    }
                }
            }
            self.block_entities
                .insert((fo.pos[0], fo.pos[1], fo.pos[2]), BlockEntity::Forge(state));
        }
        for sg in parsed.sign {
            let mut state = SignState::default();
            for (i, l) in sg.lines.into_iter().take(3).enumerate() {
                state.lines[i] = l;
            }
            self.block_entities
                .insert((sg.pos[0], sg.pos[1], sg.pos[2]), BlockEntity::Sign(state));
        }
        for cl in parsed.clamp {
            self.block_entities.insert(
                (cl.pos[0], cl.pos[1], cl.pos[2]),
                BlockEntity::Clamp(ClampState {
                    logs: cl.logs.iter().map(|l| (l[0], l[1], l[2])).collect(),
                    timer: cl.timer,
                }),
            );
        }
        for kl in parsed.kiln {
            let mut state = KilnState {
                lit: kl.lit,
                progress: kl.progress,
                core: kl.core.map(|c| (c[0], c[1], c[2])).unwrap_or_default(),
                ..Default::default()
            };
            for sl in kl.slot {
                if sl.index < 9
                    && let Some(item) = self.reg.item_id(&sl.item)
                {
                    let st = Some(ItemStack {
                        item,
                        count: sl.count,
                        durability: sl.durability,
                    });
                    match sl.index {
                        0..=3 => state.sand[sl.index] = st,
                        4 => state.powder = st,
                        _ => state.fuel[sl.index - 5] = st,
                    }
                }
            }
            self.block_entities
                .insert((kl.pos[0], kl.pos[1], kl.pos[2]), BlockEntity::Kiln(state));
        }
        for an in parsed.anvil {
            self.block_entities.insert(
                (an.pos[0], an.pos[1], an.pos[2]),
                BlockEntity::Anvil(AnvilState {
                    bloom: conv(an.bloom),
                    strikes: an.strikes,
                }),
            );
        }
    }

    pub fn save_dir_for_saving(&self) -> PathBuf {
        self.save_dir.clone()
    }

    #[cfg(test)]
    pub fn save_dir_for_test(&self) -> PathBuf {
        self.save_dir.clone()
    }

    // ---------------- fluids ----------------
}
