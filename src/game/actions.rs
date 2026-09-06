//! Ordered input arbitration for graphical world interaction.

use super::Game;
use crate::{
    raycast,
    registry::{ItemId, Registry},
};
use std::sync::Arc;

#[cfg(all(test, target_os = "linux"))]
#[path = "gameplay_proofs.rs"]
mod gameplay_proofs;

pub(super) struct ActionFrame {
    reg: Arc<Registry>,
    reach: f32,
    hit: Option<raycast::PlanetHit>,
    aim: Option<raycast::TargetHit>,
    held: Option<ItemId>,
    dt: f32,
}

impl Game {
    pub(super) fn interact(&mut self, dt: f32) {
        let reg = self.content.reg.clone();
        let reach = self.reach();
        let hit = raycast::raycast_at(
            &self.runtime.view(),
            self.player.eye(),
            self.camera.local_forward(),
            reach,
        );
        let aim = raycast::raycast_target_at(
            &self.runtime.view(),
            self.player.eye(),
            self.camera.local_forward(),
            reach,
        );
        let held = self.inventory.slots[self.input.hotbar_sel].map(|stack| stack.item);
        if self.interact_wand(dt, hit.as_ref()) {
            return;
        }
        let frame = ActionFrame {
            reg,
            reach,
            hit,
            aim,
            held,
            dt,
        };
        if self.interact_portable_items(&frame) {
            return;
        }
        if self.interact_preparation_use(&frame) {
            return;
        }
        if self.interact_held_channels(&frame) {
            return;
        }
        if self.interact_observation_channels(&frame) {
            return;
        }
        if self.interact_station_work(&frame) {
            return;
        }
        if self.interact_melee(&frame) {
            return;
        }
        if self.interact_mining(&frame) {
            return;
        }
        if self.interact_held_use(&frame) {
            return;
        }
        if self.interact_fishing(&frame) {
            return;
        }
        if self.interact_block_use(&frame) {}
    }
}

mod block_exploration;
mod block_household;
mod block_machines;
mod block_menus;
mod block_use;
mod field_tools;
mod fishing;
mod held_channels;
mod held_use;
mod held_visuals;
mod magic_feedback;
mod melee;
mod mining;
mod mob_feedback;
mod observation_channels;
mod portable_items;
mod preparation_use;
mod projectile_input;
mod script_commands;
mod station_work;
mod wand;
