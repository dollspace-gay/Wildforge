//! Save entities transaction coordination.

use crate::inventory::ItemStack;
use crate::planet::BlockPos;
use crate::world::BlockEntity;
use crate::world::World;
use std::path::PathBuf;

impl World {
    pub(in crate::world) fn entities_path(&self) -> PathBuf {
        self.save_dir.join("entities.toml")
    }

    pub(in crate::world) fn save_entities(&self) -> std::io::Result<()> {
        use std::fmt::Write as _;
        let mut out = String::from("version = 11\n");
        let pos_value = |pos: BlockPos| {
            format!(
                "{{ face = \"{:?}\", u = {}, y = {}, v = {} }}",
                pos.face(),
                pos.u(),
                pos.y(),
                pos.v()
            )
        };
        for (pos, e) in self.installations.iter() {
            let pos_line = format!("pos = {}", pos_value(*pos));
            match e {
                BlockEntity::Furnace(f) => {
                    let _ = writeln!(out, "[[furnace]]\n{pos_line}");
                    let mut slot = |k: &str, s: &Option<ItemStack>| {
                        if let Some(s) = s {
                            let _ = writeln!(
                                out,
                                "{k} = {{ item = \"{}\", count = {}, durability = {}, arcane_id = {} }}",
                                self.reg.item(s.item).name,
                                s.count,
                                s.durability,
                                s.arcane_id
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
                                "[[chest.slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}\narcane_id = {}",
                                self.reg.item(st.item).name,
                                st.count,
                                st.durability,
                                st.arcane_id
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
                                "[[offering.slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}\narcane_id = {}",
                                self.reg.item(st.item).name,
                                st.count,
                                st.durability,
                                st.arcane_id
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
                        self.reg
                            .machine(m.kind)
                            .map(|def| def.id.as_str())
                            .unwrap_or(""),
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
                                "[[machine.slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}\narcane_id = {}",
                                self.reg.item(st.item).name,
                                st.count,
                                st.durability,
                                st.arcane_id
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
                                "[[stall.slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}\narcane_id = {}",
                                self.reg.item(stk.item).name,
                                stk.count,
                                stk.durability,
                                stk.arcane_id
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
                                "[[smoker.slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}\narcane_id = {}",
                                self.reg.item(st.item).name,
                                st.count,
                                st.durability,
                                st.arcane_id
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
                        "[[steam]]\n{pos_line}\nfuel = {:?}\nwater_hu = {}\nsalt_mass = {}\ndraft_closed = {}\nsteam_numerator_remainder = {}\n",
                        s.fuel,
                        s.water.water_hu,
                        s.water.salt_mass,
                        s.draft_closed,
                        s.steam_numerator_remainder,
                    );
                }
                BlockEntity::Anvil(a) => {
                    let _ = writeln!(out, "[[anvil]]\n{pos_line}\nstrikes = {}", a.strikes);
                    if let Some(st) = &a.bloom {
                        let _ = writeln!(
                            out,
                            "bloom = {{ item = \"{}\", count = {}, durability = {}, arcane_id = {} }}",
                            self.reg.item(st.item).name,
                            st.count,
                            st.durability,
                            st.arcane_id
                        );
                    }
                    let _ = writeln!(out);
                }
                BlockEntity::SurveyFolio(folio) => {
                    let _ = writeln!(
                        out,
                        "[[survey_folio]]\n{pos_line}\nobject_id = {}\n",
                        folio.object_id
                    );
                }
                BlockEntity::DiscoveryApparatus(apparatus) => {
                    let _ = writeln!(out, "[[discovery_apparatus]]\n{pos_line}");
                    for (name, stack) in [
                        ("sample", apparatus.sample),
                        ("reference", apparatus.reference),
                    ] {
                        if let Some(stack) = stack {
                            let _ = writeln!(
                                out,
                                "{name} = {{ item = \"{}\", count = {}, durability = {}, arcane_id = {} }}",
                                self.reg.item(stack.item).name,
                                stack.count,
                                stack.durability,
                                stack.arcane_id
                            );
                        }
                    }
                    let _ = writeln!(out);
                }
                BlockEntity::BindingFrame(frame) => {
                    let _ = writeln!(
                        out,
                        "[[binding_frame]]\n{pos_line}\nrevision = {}",
                        frame.revision
                    );
                    for (name, stack) in [
                        ("body", frame.body),
                        ("reservoir", frame.reservoir),
                        ("focus", frame.focus),
                        ("binding", frame.binding),
                        ("output", frame.output),
                    ] {
                        if let Some(stack) = stack {
                            let _ = writeln!(
                                out,
                                "{name} = {{ item = \"{}\", count = {}, durability = {}, arcane_id = {} }}",
                                self.reg.item(stack.item).name,
                                stack.count,
                                stack.durability,
                                stack.arcane_id
                            );
                        }
                    }
                    let _ = writeln!(out);
                }
                BlockEntity::ChargeVessel(vessel) => {
                    let _ = writeln!(
                        out,
                        "[[charge_vessel]]\n{pos_line}\ndamage = {}\nrevision = {}",
                        vessel.damage, vessel.revision
                    );
                    if let Some(stack) = vessel.vessel {
                        let _ = writeln!(
                            out,
                            "vessel = {{ item = \"{}\", count = {}, durability = {}, arcane_id = {} }}",
                            self.reg.item(stack.item).name,
                            stack.count,
                            stack.durability,
                            stack.arcane_id
                        );
                    }
                    let _ = writeln!(out);
                }
                BlockEntity::Switch(sw) => {
                    let _ = writeln!(
                        out,
                        "[[switch]]\n{pos_line}\nselected = \"{:?}\"",
                        sw.selected
                    );
                }
                BlockEntity::Depot(d) => {
                    let _ = writeln!(
                        out,
                        "[[depot]]\n{pos_line}\nsettlement = \"{}\"",
                        d.settlement.replace(['\\', '"'], "")
                    );
                    for (i, st) in d.storage.iter().enumerate() {
                        if let Some(st) = st {
                            let _ = writeln!(
                                out,
                                "[[depot.slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}\narcane_id = {}",
                                self.reg.item(st.item).name,
                                st.count,
                                st.durability,
                                st.arcane_id
                            );
                        }
                    }
                    let _ = writeln!(out);
                }
            }
        }
        // Belt cells (capability E8) are persisted here too: cargo, progress,
        // entry direction, and splitter phase so a reloaded line resumes.
        // Cells are pruned when empty, so the only empty records worth
        // writing are splitter cells still holding their alternation phase.
        for (pos, state) in &self.belt_state {
            if state.cargo.is_empty() && !state.split_phase {
                continue;
            }
            let _ = writeln!(
                out,
                "[[belt]]\npos = {}\nentry_dir = \"{:?}\"\nprogress = {}\nsplit_phase = {}",
                pos_value(*pos),
                state.entry_dir,
                state.progress,
                state.split_phase
            );
            for (i, st) in state.cargo.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "[[belt.slot]]\nindex = {i}\nitem = \"{}\"\ncount = {}\ndurability = {}\narcane_id = {}",
                    self.reg.item(st.item).name,
                    st.count,
                    st.durability,
                    st.arcane_id
                );
            }
            let _ = writeln!(out);
        }
        crate::world::persistence::replace_or_remove(
            &self.entities_path(),
            (!out.is_empty()).then_some(out.as_bytes()),
        )
    }
}
