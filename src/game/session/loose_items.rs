//! Loose items graphical session adapter.

use crate::entity::ItemEntity;
use crate::game::Game;
use crate::inventory::ItemStack;
use glam::Vec3;

impl Game {
    pub(super) fn save_loose_items(&self, world: &std::path::Path) -> std::io::Result<()> {
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
            .runtime
            .view()
            .loose_items()
            .iter()
            .map(|entity| StoredDrop {
                stable_id: entity.stable_id,
                pos: entity.pos,
                vel: entity.vel.to_array(),
                item: self.content.reg.item(entity.item).name.clone(),
                count: entity.count,
                age: entity.age,
                durability: entity.durability,
                arcane_id: entity.arcane_id,
            })
            .collect();
        let text =
            toml::to_string_pretty(&File { version: 3, drop }).map_err(std::io::Error::other)?;
        crate::identity::atomic_write(&world.join("loose-items.toml"), text.as_bytes(), false)
    }

    pub(super) fn load_loose_items(&mut self, world: &std::path::Path) {
        use serde::Deserialize;

        // World::load_or_create owns the v3 host-authoritative format. This
        // reader remains only as a migration fallback for older session
        // worlds that reached the game before world-side adoption.
        if !self.runtime.view().loose_items().is_empty() {
            return;
        }

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
        let Ok(text) = std::fs::read_to_string(world.join("loose-items.toml")) else {
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
        for stored in file.drop {
            let Some(item) = self.content.reg.item_id(&stored.item) else {
                eprintln!("items: retained unknown loose item name {}", stored.item);
                continue;
            };
            if stored.count == 0
                || !stored.age.is_finite()
                || stored.vel.iter().any(|value| !value.is_finite())
            {
                continue;
            }
            let mut entity =
                ItemEntity::new(stored.pos, Vec3::from_array(stored.vel), item, stored.count);
            entity.stable_id = stored.stable_id;
            entity.age = stored.age.max(0.0);
            entity.durability = stored
                .durability
                .min(self.content.reg.item(item).durability);
            entity.arcane_id = stored.arcane_id;
            if let Some(at) = stored.pos.block()
                && self.content.reg.item(item).charm_def.is_some()
            {
                let mut stack = ItemStack {
                    item,
                    count: stored.count,
                    durability: entity.durability,
                    arcane_id: stored.arcane_id,
                };
                if self
                    .runtime
                    .local_mut()
                    .world
                    .ensure_charm_instance_at(
                        at,
                        &mut stack,
                        "explicit planetary loose-item charm migration",
                    )
                    .is_ok()
                {
                    entity.arcane_id = stack.arcane_id;
                }
            }
            self.runtime.local_mut().world.spawn_loose_item(entity);
        }
    }
}
