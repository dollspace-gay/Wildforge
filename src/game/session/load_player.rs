//! Load player graphical session adapter.

use crate::identity;
use crate::inventory::HOTBAR_SLOTS;
use crate::inventory::ItemStack;
use crate::inventory::TOTAL_SLOTS;
use crate::world;
use crate::game::Game;

impl Game {
    pub(in crate::game) fn load_player(&mut self, dir: &std::path::Path) -> bool {
        use serde::Deserialize;
        #[derive(Deserialize)]
        struct SlotT {
            index: usize,
            item: String,
            count: u32,
            durability: u32,
            #[serde(default)]
            arcane_id: u64,
        }
        #[derive(Deserialize)]
        struct SkillXpCount {
            source: String,
            count: u32,
        }
        #[derive(Deserialize)]
        struct LoadoutT {
            index: usize,
            #[serde(default)]
            component: Vec<SlotT>,
        }
        #[derive(Deserialize)]
        struct LoadoutPresetSlotT {
            index: usize,
            frame: String,
            #[serde(default)]
            component: Vec<String>,
        }
        #[derive(Deserialize)]
        struct LoadoutPresetT {
            #[serde(default)]
            name: String,
            #[serde(default)]
            slot: Vec<LoadoutPresetSlotT>,
        }
        #[derive(Deserialize)]
        struct P {
            version: u32,
            face: u8,
            u: f32,
            y: f32,
            v: f32,
            yaw: f32,
            pitch: f32,
            health: f32,
            hunger: f32,
            nutrition: [f32; 5],
            hotbar: usize,
            spawn_face: u8,
            spawn_u: f32,
            spawn_y: f32,
            spawn_v: f32,
            #[serde(default, alias = "inventory")]
            slot: Vec<SlotT>,
            #[serde(default)]
            armor: Vec<SlotT>,
            #[serde(default)]
            level: u32,
            #[serde(default)]
            xp: f64,
            #[serde(default)]
            skill_points: u32,
            #[serde(default)]
            allocated: Vec<String>,
            #[serde(default)]
            respecs: u32,
            #[serde(default)]
            skill_xp: Vec<SkillXpCount>,
            #[serde(default)]
            loadout: Vec<LoadoutT>,
            #[serde(default)]
            loadout_preset: Vec<LoadoutPresetT>,
        }
        let path = match identity::local_profile_path(dir, self.identity.device_id()) {
            Ok(path) => path,
            Err(error) => {
                eprintln!("identity: player profile migration failed: {error}");
                return false;
            }
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return false;
        };
        let Ok(p) = toml::from_str::<P>(&text) else {
            return false;
        };
        if p.version != 2 {
            return false;
        }
        let Some(face) = crate::planet::Face::from_u8(p.face) else {
            return false;
        };
        let Ok(pos) = crate::planet::EntityPos::new(face, p.u, p.y, p.v) else {
            return false;
        };
        let Some(spawn_face) = crate::planet::Face::from_u8(p.spawn_face) else {
            return false;
        };
        let Ok(spawn) = crate::planet::EntityPos::new(spawn_face, p.spawn_u, p.spawn_y, p.spawn_v)
        else {
            return false;
        };
        self.player.pos = pos;
        self.camera.yaw = p.yaw;
        self.camera.pitch = p.pitch;
        self.survival.health = p.health;
        self.survival.hunger = p.hunger;
        self.survival.nutrition = p.nutrition;
        self.input.hotbar_sel = p.hotbar.min(HOTBAR_SLOTS - 1);
        self.survival.spawn_point = spawn;
        if p.level > 0 {
            self.skills.level = p.level;
        }
        self.skills.xp = p.xp;
        self.skills.points = p.skill_points;
        self.skills.allocated = p.allocated;
        self.skills.respecs = p.respecs;
        for entry in p.skill_xp {
            self.skills.source_counts.insert(entry.source, entry.count);
        }
        for s in p.slot {
            if s.index < TOTAL_SLOTS
                && let Some(item) = self.content.reg.item_id(&s.item)
            {
                self.inventory.slots[s.index] = Some(ItemStack {
                    item,
                    count: s.count,
                    durability: s.durability,
                    arcane_id: s.arcane_id,
                });
            }
        }
        for s in p.armor {
            if s.index < 5
                && let Some(item) = self.content.reg.item_id(&s.item)
            {
                self.survival.armor[s.index] = Some(ItemStack {
                    item,
                    count: s.count,
                    durability: s.durability,
                    arcane_id: s.arcane_id,
                });
            }
        }
        for entry in p.loadout {
            if entry.index >= 5 {
                continue;
            }
            let mut loadout = crate::equipment::Loadout::default();
            for component in entry.component {
                let Some(item) = self.content.reg.item_id(&component.item) else {
                    continue;
                };
                loadout.components.push(crate::equipment::SlottedComponent {
                    slot_type: self
                        .content
                        .reg
                        .item(item)
                        .component
                        .clone()
                        .unwrap_or_default(),
                    stack: ItemStack {
                        item,
                        count: component.count,
                        durability: component.durability,
                        arcane_id: component.arcane_id,
                    },
                });
            }
            self.survival.loadouts[entry.index] = loadout;
        }
        for entry in p.loadout_preset {
            let mut slots: [Option<crate::equipment::PresetSlot>; 4] = Default::default();
            for slot in entry.slot {
                if slot.index >= 4 {
                    continue;
                }
                slots[slot.index] = Some(crate::equipment::PresetSlot {
                    frame: slot.frame,
                    components: slot.component,
                });
            }
            self.survival
                .loadout_presets
                .push(crate::equipment::LoadoutPreset {
                    name: entry.name,
                    slots,
                });
        }
        if let Some(at) = self.player.pos.block() {
            let migrated = self.runtime.local_mut().world.migrate_legacy_player_charms(
                at,
                &mut self.inventory,
                &mut self.survival.armor,
                &mut self.ui_state.held_stack,
                "local player",
            );
            if migrated != 0
                && let Err(error) = self.save_player()
            {
                eprintln!(
                    "implements: migrated {migrated} local charms but profile save failed: {error}"
                );
            }
        }
        if let Ok(player_id) = identity::local_player_id(dir, self.identity.device_id()) {
            match self.runtime.local_mut().world.resume_pending_inventory_workings(player_id.0, &mut self.inventory)
            {
                Ok(ids) if !ids.is_empty() => match self.save_player() {
                    Ok(()) => {
                        for id in ids {
                            if let Err(error) = self.runtime.local_mut().world.finish_inventory_working(id) {
                                eprintln!("workings: resumed Fieldmend could not finish: {error}");
                            }
                        }
                    }
                    Err(error) => eprintln!(
                        "workings: resumed Fieldmend remains pending because its profile checkpoint failed: {error}"
                    ),
                },
                Ok(_) => {}
                Err(error) => {
                    eprintln!("workings: pending local Fieldmend is inconsistent: {error}")
                }
            }
        }
        true
    }
}
