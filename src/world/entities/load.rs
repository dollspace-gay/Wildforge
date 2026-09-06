//! Load entities transaction coordination.

use super::schema::{FileT, SlotT};
use crate::inventory::ItemStack;
use crate::registry::Registry;
use crate::world::AnvilState;
use crate::world::BindingFrameState;
use crate::world::BlockEntity;
use crate::world::CHEST_SLOTS;
use crate::world::ChargeVesselState;
use crate::world::ChestState;
use crate::world::ClampState;
use crate::world::DiscoveryApparatusState;
use crate::world::FurnaceState;
use crate::world::MachineInstance;
use crate::world::OfferingState;
use crate::world::STEAM_SECS_PER_WATER;
use crate::world::SignState;
use crate::world::SmokerState;
use crate::world::StallState;
use crate::world::SteamState;
use crate::world::SurveyFolioState;
use crate::world::SwitchState;
use crate::world::World;
use crate::world::multiblock::MachineKind;
use std::fs;

impl World {
    pub(in crate::world) fn load_entities(&mut self) {
        let Ok(text) = fs::read_to_string(self.entities_path()) else {
            return;
        };
        let Ok(parsed) = toml::from_str::<FileT>(&text) else {
            return;
        };
        // Version 9 saves still load (they simply have no belt records); a
        // version 10 file adds the belt table. Anything else is foreign and
        // the whole sidecar is left alone rather than half-misread.
        if !(9..=11).contains(&parsed.version) {
            return;
        }
        let conv = |reg: &Registry, s: Option<SlotT>| -> Option<ItemStack> {
            let s = s?;
            let item = reg.item_id(&s.item)?;
            Some(ItemStack {
                item,
                count: s.count,
                durability: s.durability,
                arcane_id: s.arcane_id,
            })
        };
        for fu in parsed.furnace {
            self.installations.insert(
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
                        arcane_id: sl.arcane_id,
                    });
                }
            }
            self.installations.insert(ch.pos, BlockEntity::Chest(state));
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
                        arcane_id: sl.arcane_id,
                    });
                }
            }
            self.installations
                .insert(of.pos, BlockEntity::Offering(state));
        }
        for m in parsed.machine {
            let Some(kind) = MachineKind::from_name(&self.reg, &m.kind) else {
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
                        arcane_id: sl.arcane_id,
                    });
                    match sl.index {
                        0..=3 => state.charge[sl.index] = st,
                        4 => state.reagent = st,
                        5..=8 => state.fuel[sl.index - 5] = st,
                        _ => {}
                    }
                }
            }
            self.installations
                .insert(m.pos, BlockEntity::Multiblock(state));
            // Revalidate on load: fold stats and douse any machine whose
            // shell broke while it was saved.
            crate::world::machines::revalidate_machine_at(self, m.pos);
        }
        for sg in parsed.sign {
            let mut state = SignState::default();
            for (i, l) in sg.lines.into_iter().take(3).enumerate() {
                state.lines[i] = l;
            }
            self.installations.insert(sg.pos, BlockEntity::Sign(state));
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
                        arcane_id: sl.arcane_id,
                    });
                    match sl.index {
                        0..=5 => state.goods[sl.index] = stk,
                        6 => state.price = stk,
                        7..=12 => state.till[sl.index - 7] = stk,
                        _ => {}
                    }
                }
            }
            self.installations.insert(st.pos, BlockEntity::Stall(state));
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
                        arcane_id: sl.arcane_id,
                    });
                }
            }
            self.installations
                .insert(sm.pos, BlockEntity::Smoker(state));
        }
        for st in parsed.steam {
            self.installations.insert(
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
                    draft_closed: st.draft_closed,
                    steam_numerator_remainder: st.steam_numerator_remainder,
                }),
            );
        }
        for cl in parsed.clamp {
            self.installations.insert(
                cl.pos,
                BlockEntity::Clamp(ClampState {
                    logs: cl.logs,
                    timer: cl.timer,
                }),
            );
        }
        for an in parsed.anvil {
            self.installations.insert(
                an.pos,
                BlockEntity::Anvil(AnvilState {
                    bloom: conv(&self.reg, an.bloom),
                    strikes: an.strikes,
                }),
            );
        }
        for folio in parsed.survey_folio {
            if folio.object_id != 0 {
                self.installations.insert(
                    folio.pos,
                    BlockEntity::SurveyFolio(SurveyFolioState {
                        object_id: folio.object_id,
                    }),
                );
            }
        }
        for apparatus in parsed.discovery_apparatus {
            self.installations.insert(
                apparatus.pos,
                BlockEntity::DiscoveryApparatus(DiscoveryApparatusState {
                    sample: conv(&self.reg, apparatus.sample),
                    reference: conv(&self.reg, apparatus.reference),
                }),
            );
        }
        for frame in parsed.binding_frame {
            self.installations.insert(
                frame.pos,
                BlockEntity::BindingFrame(BindingFrameState {
                    body: conv(&self.reg, frame.body),
                    reservoir: conv(&self.reg, frame.reservoir),
                    focus: conv(&self.reg, frame.focus),
                    binding: conv(&self.reg, frame.binding),
                    output: conv(&self.reg, frame.output),
                    revision: frame.revision,
                }),
            );
        }
        for vessel in parsed.charge_vessel {
            self.installations.insert(
                vessel.pos,
                BlockEntity::ChargeVessel(ChargeVesselState {
                    vessel: conv(&self.reg, vessel.vessel),
                    damage: vessel.damage.min(1_000),
                    revision: vessel.revision,
                }),
            );
        }
        for sw in parsed.switch {
            let selected = match sw.selected.to_ascii_lowercase().as_str() {
                "east" => crate::planet::Direction4::East,
                "west" => crate::planet::Direction4::West,
                "south" => crate::planet::Direction4::South,
                _ => crate::planet::Direction4::North,
            };
            self.installations
                .insert(sw.pos, BlockEntity::Switch(SwitchState { selected }));
        }
        for bt in parsed.belt {
            use crate::planet::Direction4;
            let mut state = crate::world::belt::BeltState::new();
            state.progress = bt.progress;
            state.split_phase = bt.split_phase;
            state.entry_dir = match bt.entry_dir.to_ascii_lowercase().as_str() {
                "east" => Direction4::East,
                "west" => Direction4::West,
                "south" => Direction4::South,
                _ => Direction4::North,
            };
            for sl in bt.slot {
                if let Some(item) = self.reg.item_id(&sl.item) {
                    state.cargo.push_back(ItemStack {
                        item,
                        count: sl.count,
                        durability: sl.durability.min(self.reg.item(item).durability),
                        arcane_id: sl.arcane_id,
                    });
                }
            }
            // Restore unconditionally: chunks load lazily, so a `get_block_at`
            // check here would see unloaded chunks as air and drop every
            // record. The belt tick parks cells whose chunk is still unloaded
            // and spills cargo only once the cell genuinely is not a belt.
            self.belt_state.insert(bt.pos, state);
        }
        for dt_ent in parsed.depot {
            let mut state = crate::world::DepotState {
                settlement: dt_ent.settlement,
                storage: Default::default(),
            };
            for sl in dt_ent.slot {
                if sl.index < 12
                    && let Some(item) = self.reg.item_id(&sl.item)
                {
                    state.storage[sl.index] = Some(ItemStack {
                        item,
                        count: sl.count,
                        durability: sl.durability.min(self.reg.item(item).durability),
                        arcane_id: sl.arcane_id,
                    });
                }
            }
            self.installations
                .insert(dt_ent.pos, BlockEntity::Depot(state));
        }
    }
}
