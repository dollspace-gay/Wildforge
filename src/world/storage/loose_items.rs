//! Loose items storage transaction coordination.

use crate::world::World;
use std::fs;

impl World {
    pub(in crate::world) fn encode_loose_items(&self) -> std::io::Result<Vec<u8>> {
        use serde::Serialize;

        #[derive(Serialize)]
        struct StoredDrop {
            stable_id: u64,
            pos: crate::planet::EntityPos,
            vel: [f32; 3],
            item: String,
            count: u32,
            age: f32,
            durability: u32,
            arcane_id: u64,
        }
        #[derive(Serialize)]
        struct File {
            version: u32,
            drop: Vec<StoredDrop>,
        }
        let drop = self
            .population
            .loose_items()
            .iter()
            .filter(|item| item.stable_id != 0 && item.count != 0)
            .map(|item| StoredDrop {
                stable_id: item.stable_id,
                pos: item.pos,
                vel: item.vel.to_array(),
                item: self.reg.item(item.item).name.clone(),
                count: item.count,
                age: item.age,
                durability: item.durability,
                arcane_id: item.arcane_id,
            })
            .collect();
        toml::to_string_pretty(&File { version: 3, drop })
            .map(String::into_bytes)
            .map_err(std::io::Error::other)
    }

    pub(in crate::world) fn save_loose_items(&self) -> std::io::Result<()> {
        #[cfg(test)]
        if self.fail_loose_item_save {
            return Err(std::io::Error::other(
                "injected loose-item sidecar save failure",
            ));
        }
        crate::identity::atomic_write(
            &self.save_dir.join("loose-items.toml"),
            &self.encode_loose_items()?,
            false,
        )
    }

    pub(in crate::world) fn load_loose_items(&mut self) {
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct StoredDrop {
            #[serde(default)]
            stable_id: u64,
            pos: crate::planet::EntityPos,
            vel: [f32; 3],
            item: String,
            count: u32,
            age: f32,
            durability: u32,
            #[serde(default)]
            arcane_id: u64,
        }
        #[derive(Deserialize)]
        struct File {
            version: u32,
            #[serde(default)]
            drop: Vec<StoredDrop>,
        }
        let Ok(text) = fs::read_to_string(self.save_dir.join("loose-items.toml")) else {
            return;
        };
        let Ok(file) = toml::from_str::<File>(&text) else {
            eprintln!("items: could not parse loose-items.toml; file left untouched");
            return;
        };
        if !(1..=3).contains(&file.version) {
            eprintln!(
                "items: unsupported loose item save version {}",
                file.version
            );
            return;
        }
        let mut loaded = Vec::new();
        for stored in file.drop {
            let Some(item) = self.reg.item_id(&stored.item) else {
                eprintln!("items: retained unknown loose item name {}", stored.item);
                continue;
            };
            if stored.count == 0
                || !stored.age.is_finite()
                || stored.vel.iter().any(|value| !value.is_finite())
            {
                continue;
            }
            let mut entity = crate::entity::ItemEntity::new(
                stored.pos,
                glam::Vec3::from_array(stored.vel),
                item,
                stored.count,
            );
            entity.stable_id = stored.stable_id;
            entity.age = stored.age.max(0.0);
            entity.durability = stored.durability.min(self.reg.item(item).durability);
            entity.arcane_id = stored.arcane_id;
            loaded.push(entity);
        }
        for item in loaded {
            self.spawn_loose_item(item);
        }
    }
}
