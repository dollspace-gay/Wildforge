//! Persistent block-entity serialization and world save-directory access.

use super::multiblock::MachineKind;
use super::*;

impl World {
    pub(super) fn entities_path(&self) -> PathBuf {
        self.save_dir.join("entities.toml")
    }

    pub(super) fn save_entities(&self) -> std::io::Result<()> {
        use std::fmt::Write as _;
        let mut out = String::from("version = 4\n");
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
                BlockEntity::Multiblock(m) => {
                    let core = m
                        .core
                        .map(|core| format!("\ncore = {}", pos_value(core)))
                        .unwrap_or_default();
                    let _ = writeln!(
                        out,
                        "[[machine]]\n{pos_line}\nkind = \"{}\"\nlit = {}\nprogress = {}{core}\npowder = {}\nseparator_fuel = {}\nneodymium = {}\ncerium = {}",
                        m.kind.name(),
                        m.lit,
                        m.progress,
                        m.powder,
                        m.separator_fuel,
                        m.neodymium,
                        m.cerium
                    );
                    let all: Vec<&Option<ItemStack>> = m
                        .charge
                        .iter()
                        .chain([&m.reagent])
                        .chain(m.fuel.iter())
                        .collect();
                    for (i, st) in all.into_iter().enumerate() {
                        if let Some(st) = st {
                            let _ = writeln!(
                                out,
                                "[[machine.slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}",
                                self.reg.item(st.item).name,
                                st.count,
                                st.durability
                            );
                        }
                    }
                    for (material, units) in &m.reclaim {
                        let material = material.replace(['\\', '"'], "");
                        let _ = writeln!(
                            out,
                            "[[machine.reclaim]]\nmaterial = \"{material}\"\nunits = {units}"
                        );
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
                BlockEntity::Steam(s) => {
                    let _ = writeln!(
                        out,
                        "[[steam]]\n{pos_line}\nfuel = {:?}\nwater_hu = {}\nsalt_mass = {}\nsteam_numerator_remainder = {}\n",
                        s.fuel, s.water.water_hu, s.water.salt_mass, s.steam_numerator_remainder,
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
        struct MachineT {
            pos: crate::planet::BlockPos,
            #[serde(default)]
            kind: String,
            #[serde(default)]
            lit: bool,
            #[serde(default)]
            progress: f32,
            #[serde(default)]
            core: Option<crate::planet::BlockPos>,
            #[serde(default)]
            slot: Vec<ChestSlotT>,
            #[serde(default)]
            reclaim: Vec<MaterialT>,
            #[serde(default)]
            powder: u32,
            #[serde(default)]
            separator_fuel: u32,
            #[serde(default)]
            neodymium: u32,
            #[serde(default)]
            cerium: u32,
        }
        #[derive(Deserialize)]
        struct MaterialT {
            material: String,
            units: u64,
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
            water: Option<f32>,
            #[serde(default)]
            water_hu: Option<u64>,
            #[serde(default)]
            salt_mass: u64,
            #[serde(default)]
            steam_numerator_remainder: u64,
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
            clamp: Vec<ClampT>,
            #[serde(default)]
            anvil: Vec<AnvilT>,
            #[serde(default)]
            sign: Vec<SignT>,
            #[serde(default)]
            stall: Vec<StallT>,
            #[serde(default)]
            smoker: Vec<SmokerT>,
            #[serde(default)]
            steam: Vec<SteamT>,
            #[serde(default)]
            machine: Vec<MachineT>,
        }
        let Ok(text) = fs::read_to_string(self.entities_path()) else {
            return;
        };
        let Ok(parsed) = toml::from_str::<FileT>(&text) else {
            return;
        };
        if parsed.version != 4 {
            return;
        }
        let conv = |reg: &Registry, s: Option<SlotT>| -> Option<ItemStack> {
            let s = s?;
            let item = reg.item_id(&s.item)?;
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
                    input: conv(&self.reg, fu.input),
                    fuel: conv(&self.reg, fu.fuel),
                    output: conv(&self.reg, fu.output),
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
        for m in parsed.machine {
            let Some(kind) = MachineKind::from_name(&m.kind) else {
                continue;
            };
            let mut state = MachineInstance {
                kind,
                lit: m.lit,
                progress: m.progress,
                core: m.core,
                powder: m.powder,
                separator_fuel: m.separator_fuel,
                neodymium: m.neodymium,
                cerium: m.cerium,
                ..Default::default()
            };
            for material in m.reclaim {
                if material.units != 0 {
                    *state.reclaim.entry(material.material).or_default() += material.units;
                }
            }
            for sl in m.slot {
                if let Some(item) = self.reg.item_id(&sl.item) {
                    let st = Some(ItemStack {
                        item,
                        count: sl.count,
                        durability: sl.durability,
                    });
                    match sl.index {
                        0..=3 => state.charge[sl.index] = st,
                        4 => state.reagent = st,
                        5..=8 => state.fuel[sl.index - 5] = st,
                        _ => {}
                    }
                }
            }
            self.block_entities
                .insert(m.pos, BlockEntity::Multiblock(state));
            // Revalidate on load: fold stats and douse any machine whose
            // shell broke while it was saved.
            self.revalidate_machine_at(m.pos);
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
                    water: crate::planet_atlas::ReservoirMass {
                        water_hu: st.water_hu.unwrap_or_else(|| {
                            ((st.water.unwrap_or(0.0) / STEAM_SECS_PER_WATER)
                                * crate::planet_atlas::HYDRO_UNITS_PER_BLOCK as f32)
                                .round()
                                .max(0.0) as u64
                        }),
                        salt_mass: st.salt_mass,
                    },
                    steam_numerator_remainder: st.steam_numerator_remainder,
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
        for an in parsed.anvil {
            self.block_entities.insert(
                an.pos,
                BlockEntity::Anvil(AnvilState {
                    bloom: conv(&self.reg, an.bloom),
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
