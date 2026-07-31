//! Persistent block-entity serialization and world save-directory access.

use super::*;

impl World {
    pub(super) fn entities_path(&self) -> PathBuf {
        self.save_dir.join("entities.toml")
    }

    pub(super) fn save_entities(&self) -> std::io::Result<()> {
        use std::fmt::Write as _;
        let mut out = String::from("version = 3\n");
        let pos_value = |pos: BlockPos| {
            format!(
                "{{ face = \"{:?}\", u = {}, y = {}, v = {} }}",
                pos.face(),
                pos.u(),
                pos.y(),
                pos.v()
            )
        };
        for (pos, e) in &self.block_entities {
            let pos_line = format!("pos = {}", pos_value(*pos));
            match e {
                BlockEntity::Furnace(f) => {
                    let _ = writeln!(out, "[[furnace]]\n{pos_line}");
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
                    let _ = writeln!(out, "[[chest]]\n{pos_line}");
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
                    let _ = writeln!(out, "[[offering]]\n{pos_line}");
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
                    let core = b
                        .core
                        .map(|core| format!("\ncore = {}", pos_value(core)))
                        .unwrap_or_default();
                    let _ = writeln!(
                        out,
                        "[[bloomery]]\n{pos_line}\nlit = {}\nprogress = {}{core}",
                        b.lit, b.progress
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
                    let core = f
                        .core
                        .map(|core| format!("\ncore = {}", pos_value(core)))
                        .unwrap_or_default();
                    let _ = writeln!(
                        out,
                        "[[forge]]\n{pos_line}\nlit = {}\nprogress = {}{core}",
                        f.lit, f.progress
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
                        "[[sign]]\n{pos_line}\nlines = [\"{}\", \"{}\", \"{}\"]\n",
                        esc(&sg.lines[0]),
                        esc(&sg.lines[1]),
                        esc(&sg.lines[2])
                    );
                }
                BlockEntity::Stall(st) => {
                    let hex: String = st.owner.iter().map(|b| format!("{b:02x}")).collect();
                    let _ = writeln!(
                        out,
                        "[[stall]]\n{pos_line}\nowner = \"{hex}\"\nowner_name = \"{}\"",
                        st.owner_name.replace(['\\', '"'], "")
                    );
                    let all: Vec<&Option<ItemStack>> = st
                        .goods
                        .iter()
                        .chain([&st.price])
                        .chain(st.till.iter())
                        .collect();
                    for (i, stk) in all.into_iter().enumerate() {
                        if let Some(stk) = stk {
                            let _ = writeln!(
                                out,
                                "[[stall.slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}",
                                self.reg.item(stk.item).name,
                                stk.count,
                                stk.durability
                            );
                        }
                    }
                    let _ = writeln!(out);
                }
                BlockEntity::Smoker(sm) => {
                    let _ = writeln!(out, "[[smoker]]\n{pos_line}\nprogress = {:?}", sm.progress);
                    for (i, st) in sm.meat.iter().enumerate() {
                        if let Some(st) = st {
                            let _ = writeln!(
                                out,
                                "[[smoker.slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}",
                                self.reg.item(st.item).name,
                                st.count,
                                st.durability
                            );
                        }
                    }
                    let _ = writeln!(out);
                }
                BlockEntity::Clamp(c) => {
                    let logs: Vec<String> = c.logs.iter().map(|pos| pos_value(*pos)).collect();
                    let _ = writeln!(
                        out,
                        "[[clamp]]\n{pos_line}\ntimer = {}\nlogs = [{}]\n",
                        c.timer,
                        logs.join(", ")
                    );
                }
                BlockEntity::Kiln(k) => {
                    let core = k
                        .core
                        .map(|core| format!("\ncore = {}", pos_value(core)))
                        .unwrap_or_default();
                    let _ = writeln!(
                        out,
                        "[[kiln]]\n{pos_line}\nlit = {}\nprogress = {}{core}",
                        k.lit, k.progress
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
                BlockEntity::Steam(s) => {
                    let _ = writeln!(
                        out,
                        "[[steam]]\n{pos_line}\nfuel = {:?}\nwater = {:?}\n",
                        s.fuel, s.water
                    );
                }
                BlockEntity::Separator(sp) => {
                    let _ = writeln!(
                        out,
                        "[[separator]]\n{pos_line}\npowder = {}\nfuel = {}\nnd = {}\nce = {}\nprogress = {:?}\n",
                        sp.powder, sp.fuel, sp.nd, sp.ce, sp.progress
                    );
                }
                BlockEntity::Anvil(a) => {
                    let _ = writeln!(out, "[[anvil]]\n{pos_line}\nstrikes = {}", a.strikes);
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
        super::persistence::replace_or_remove(
            &self.entities_path(),
            (!out.is_empty()).then_some(out.as_bytes()),
        )
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
            pos: crate::planet::BlockPos,
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
            pos: crate::planet::BlockPos,
            #[serde(default)]
            wild_owned: bool,
            #[serde(default)]
            slot: Vec<ChestSlotT>,
        }
        #[derive(Deserialize)]
        struct BloomeryT {
            pos: crate::planet::BlockPos,
            #[serde(default)]
            lit: bool,
            #[serde(default)]
            progress: f32,
            #[serde(default)]
            core: Option<crate::planet::BlockPos>,
            #[serde(default)]
            slot: Vec<ChestSlotT>,
        }
        #[derive(Deserialize)]
        struct SignT {
            pos: crate::planet::BlockPos,
            #[serde(default)]
            lines: Vec<String>,
        }
        #[derive(Deserialize)]
        struct StallT {
            pos: crate::planet::BlockPos,
            #[serde(default)]
            owner: String,
            #[serde(default)]
            owner_name: String,
            #[serde(default)]
            slot: Vec<ChestSlotT>,
        }
        #[derive(Deserialize)]
        struct SmokerT {
            pos: crate::planet::BlockPos,
            #[serde(default)]
            progress: f32,
            #[serde(default)]
            slot: Vec<ChestSlotT>,
        }
        #[derive(Deserialize)]
        struct ClampT {
            pos: crate::planet::BlockPos,
            timer: f32,
            #[serde(default)]
            logs: Vec<crate::planet::BlockPos>,
        }
        #[derive(Deserialize)]
        struct AnvilT {
            pos: crate::planet::BlockPos,
            #[serde(default)]
            strikes: u32,
            #[serde(default)]
            bloom: Option<SlotT>,
        }
        #[derive(Deserialize)]
        struct SteamT {
            pos: crate::planet::BlockPos,
            #[serde(default)]
            fuel: f32,
            #[serde(default)]
            water: f32,
        }
        #[derive(Deserialize)]
        struct SeparatorT {
            pos: crate::planet::BlockPos,
            #[serde(default)]
            powder: u32,
            #[serde(default)]
            fuel: u32,
            #[serde(default)]
            nd: u32,
            #[serde(default)]
            ce: u32,
            #[serde(default)]
            progress: f32,
        }
        #[derive(Deserialize)]
        struct FileT {
            version: u32,
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
            #[serde(default)]
            stall: Vec<StallT>,
            #[serde(default)]
            smoker: Vec<SmokerT>,
            #[serde(default)]
            steam: Vec<SteamT>,
            #[serde(default)]
            separator: Vec<SeparatorT>,
        }
        let Ok(text) = fs::read_to_string(self.entities_path()) else {
            return;
        };
        let Ok(parsed) = toml::from_str::<FileT>(&text) else {
            return;
        };
        if parsed.version != 3 {
            return;
        }
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
                fu.pos,
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
                .insert(ch.pos, BlockEntity::Chest(state));
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
            self.block_entities
                .insert(of.pos, BlockEntity::Offering(state));
        }
        for bl in parsed.bloomery {
            let mut state = BloomeryState {
                lit: bl.lit,
                progress: bl.progress,
                core: bl.core,
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
            self.block_entities
                .insert(bl.pos, BlockEntity::Bloomery(state));
        }
        for fo in parsed.forge {
            let mut state = BloomeryState {
                lit: fo.lit,
                progress: fo.progress,
                core: fo.core,
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
                .insert(fo.pos, BlockEntity::Forge(state));
        }
        for sg in parsed.sign {
            let mut state = SignState::default();
            for (i, l) in sg.lines.into_iter().take(3).enumerate() {
                state.lines[i] = l;
            }
            self.block_entities.insert(sg.pos, BlockEntity::Sign(state));
        }
        for st in parsed.stall {
            let mut state = StallState {
                owner_name: st.owner_name,
                ..Default::default()
            };
            if st.owner.len() == 32 {
                for (i, b) in state.owner.iter_mut().enumerate() {
                    *b = u8::from_str_radix(&st.owner[i * 2..i * 2 + 2], 16).unwrap_or(0);
                }
            }
            for sl in st.slot {
                if let Some(item) = self.reg.item_id(&sl.item) {
                    let stk = Some(ItemStack {
                        item,
                        count: sl.count,
                        durability: sl.durability,
                    });
                    match sl.index {
                        0..=5 => state.goods[sl.index] = stk,
                        6 => state.price = stk,
                        7..=12 => state.till[sl.index - 7] = stk,
                        _ => {}
                    }
                }
            }
            self.block_entities
                .insert(st.pos, BlockEntity::Stall(state));
        }
        for sm in parsed.smoker {
            let mut state = SmokerState {
                progress: sm.progress,
                ..Default::default()
            };
            for sl in sm.slot {
                if sl.index < 4
                    && let Some(item) = self.reg.item_id(&sl.item)
                {
                    state.meat[sl.index] = Some(ItemStack {
                        item,
                        count: sl.count,
                        durability: sl.durability,
                    });
                }
            }
            self.block_entities
                .insert(sm.pos, BlockEntity::Smoker(state));
        }
        for st in parsed.steam {
            self.block_entities.insert(
                st.pos,
                BlockEntity::Steam(SteamState {
                    fuel: st.fuel,
                    water: st.water,
                }),
            );
        }
        for sp in parsed.separator {
            self.block_entities.insert(
                sp.pos,
                BlockEntity::Separator(SeparatorState {
                    powder: sp.powder,
                    fuel: sp.fuel,
                    nd: sp.nd,
                    ce: sp.ce,
                    progress: sp.progress,
                }),
            );
        }
        for cl in parsed.clamp {
            self.block_entities.insert(
                cl.pos,
                BlockEntity::Clamp(ClampState {
                    logs: cl.logs,
                    timer: cl.timer,
                }),
            );
        }
        for kl in parsed.kiln {
            let mut state = KilnState {
                lit: kl.lit,
                progress: kl.progress,
                core: kl.core,
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
            self.block_entities.insert(kl.pos, BlockEntity::Kiln(state));
        }
        for an in parsed.anvil {
            self.block_entities.insert(
                an.pos,
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
