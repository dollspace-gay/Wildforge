//! Projectile input in the ordered graphical action pipeline.

use crate::game::Game;
use crate::world::TerrainRead;
use crate::audio::Sfx;
use crate::mobs;
use crate::net;
use crate::registry;
use crate::registry::ItemId;

impl Game {

    pub(in crate::game) fn has_ammo(&self, class: &str) -> bool {
        self.inventory
            .slots
            .iter()
            .flatten()
            .any(|s| self.content.reg.item(s.item).ammo.as_deref() == Some(class))
    }

    /// Remove one item of the ammo class; returns its id.
    pub(in crate::game) fn take_ammo(&mut self, class: &str) -> Option<ItemId> {
        let reg = self.content.reg.clone();
        for slot in self.inventory.slots.iter_mut() {
            if let Some(s) = slot
                && reg.item(s.item).ammo.as_deref() == Some(class)
            {
                let id = s.item;
                if s.count > 1 {
                    s.count -= 1;
                } else {
                    *slot = None;
                }
                return Some(id);
            }
        }
        None
    }

    /// Loose an arrow: charge in 0..1 scales damage and speed.
    pub(in crate::game) fn fire_bow(&mut self, bow: &registry::BowDef, charge: f32) {
        let reg = self.content.reg.clone();
        let arrow_id = if self.creative {
            reg.item_id("base:arrow")
        } else {
            self.take_ammo("arrow")
        };
        let Some(arrow_id) = arrow_id else { return };
        let dir = self.camera.local_forward();
        let eye = self.player.eye();
        if let Some(r) = &self.multiplayer.remote {
            r.session.send(&net::C2S::FireProjectile {
                direction: dir,
                charge,
            });
            if !self.creative {
                self.inventory.wear_tool(&reg, self.input.hotbar_sel);
            }
            self.sfx(Sfx::Bolt(0.8 + charge * 0.8));
            return;
        }
        self.runtime.local_mut().world.spawn_projectile(mobs::Projectile {
            stable_id: 0,
            pos: eye
                .translated(dir * 0.4)
                .expect("projectile muzzle stays near the player")
                .pos,
            vel: dir * bow.speed * (0.6 + 0.4 * charge),
            tile: reg.item(arrow_id).icon,
            damage: bow.damage * (0.45 + 0.55 * charge),
            damage_type: None,
            age: 0.0,
            from_player: true,
            // Arrows that stick into terrain are recoverable.
            drop_item: (!self.creative).then_some(arrow_id),
            preparation_payload: None,
            owner: 0,
        });
        if !self.creative {
            self.inventory.wear_tool(&reg, self.input.hotbar_sel);
        }
        self.sfx(Sfx::Bolt(0.8 + charge * 0.8));
    }
}
