//! Smelting graphical containers adapter.

use super::ContainerPanel;
use crate::audio::Sfx;
use crate::game::Game;
use crate::inventory::ItemStack;
use crate::inventory::TOTAL_SLOTS;
use crate::net;
use crate::world;
use crate::world::TerrainRead;

impl Game {
    pub(in crate::game) fn bloomery_click(
        &mut self,
        pos: crate::planet::BlockPos,
        slot: usize,
        right: bool,
    ) {
        self.exchange_container_slot(pos, slot, right, ContainerPanel::Bloomery);
    }

    /// The LIGHT action: needs an ember in hand or inventory, a valid
    /// shell, and a charge. Guests request; the host answers.
    pub(in crate::game) fn light_bloomery_action(&mut self, pos: crate::planet::BlockPos) {
        let reg = self.content.reg.clone();
        let Some(ember) = reg.item_id("base:ember") else {
            return;
        };
        let slot =
            (0..TOTAL_SLOTS).find(|&i| self.inventory.slots[i].is_some_and(|s| s.item == ember));
        let Some(slot) = slot else {
            self.toast("Lighting the stack takes a warden's ember.".to_string());
            return;
        };
        if let Some(rc) = &self.multiplayer.remote {
            self.inventory.take_one(slot);
            rc.session.send(&net::C2S::LightBloomery { pos });
            return;
        }
        let block = self.runtime.view().get_block_at(pos);
        let station = self.content.reg.block(block).interaction.as_deref();
        // Capability E7: light any fire handler by its interaction; the
        // kind's shell and charge rules come from the machine def.
        let res = match station
            .and_then(|interaction| reg.machine_by_interaction(interaction))
            .filter(|kind| reg.machine(*kind).is_some_and(|def| def.handler.has_fire()))
        {
            Some(kind) => {
                let matched = match kind.validate(&self.runtime.local().world, pos) {
                    Some(matched) => matched,
                    None => {
                        self.toast("The stack is breached.".to_string());
                        return;
                    }
                };
                crate::world::machines::light_machine_at(
                    &mut self.runtime.local_mut().world,
                    pos,
                    kind,
                    matched,
                )
            }
            None => self.runtime.local_mut().world.light_bloomery_at(pos),
        };
        match res {
            Ok(()) => {
                let consumed = self.inventory.slots[slot];
                self.inventory.take_one(slot);
                if !self.creative
                    && let Some(stack) = consumed
                {
                    self.runtime.local_mut().world.retire_arcane_stack_at(
                        pos,
                        ItemStack { count: 1, ..stack },
                        "high-heat station ignition",
                    );
                }
                self.sfx(Sfx::Bolt(0.8));
                let kilnish = self.runtime.view().block_entity_at(&pos).is_some_and(|e| {
                    matches!(
                        e,
                        world::BlockEntity::Multiblock(m)
                            if m.kind.handler(&reg)
                                == Some(crate::machines::MachineHandler::Kiln)
                    )
                });
                self.toast(if kilnish {
                    "The kiln takes the ember. White heat.".to_string()
                } else {
                    "The stack takes the ember. Half a day of fire.".to_string()
                });
            }
            Err(e) => self.toast(e.to_string()),
        }
    }

    pub(in crate::game) fn kiln_click(
        &mut self,
        pos: crate::planet::BlockPos,
        slot: usize,
        right: bool,
    ) {
        self.exchange_container_slot(pos, slot, right, ContainerPanel::Kiln);
    }

    #[allow(clippy::type_complexity)]
    pub(in crate::game) fn furnace_view(
        &self,
        pos: crate::planet::BlockPos,
    ) -> (
        Option<ItemStack>,
        Option<ItemStack>,
        Option<ItemStack>,
        f32,
        f32,
    ) {
        match self.runtime.view().block_entity_at(&pos) {
            Some(world::BlockEntity::Furnace(f)) => {
                let time = f
                    .input
                    .and_then(|s| self.content.reg.smelt_for(s.item))
                    .map(|s| s.time)
                    .unwrap_or(8.0);
                let burn = if f.burn_total > 0.0 {
                    f.burn_left / f.burn_total
                } else {
                    0.0
                };
                (
                    f.input,
                    f.fuel,
                    f.output,
                    (f.progress / time).min(1.0),
                    burn,
                )
            }
            _ => (None, None, None, 0.0, 0.0),
        }
    }
}
