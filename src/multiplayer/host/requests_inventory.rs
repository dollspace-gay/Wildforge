//! Authenticated inventory request adapter.

use super::{C2S, HostSession, ItemStack, Server, click_stack, net, refresh_held};

impl HostSession {
    pub(super) fn request_inventory(&mut self, server: &mut Server, id: u32, msg: C2S) {
        let Some(guest) = self.guests.get_mut(&id) else {
            return;
        };
        match msg {
            C2S::InventoryClick { area, slot, right } => {
                let slot = slot as usize;
                let reg = &server.world.reg;
                match area {
                    net::InventoryArea::Inventory if slot < guest.inventory.slots.len() => {
                        let (value, cursor) =
                            click_stack(reg, guest.inventory.slots[slot], guest.cursor, right);
                        guest.inventory.slots[slot] = value;
                        guest.cursor = cursor;
                    }
                    net::InventoryArea::Craft if slot < guest.craft_grid.len() => {
                        let (value, cursor) =
                            click_stack(reg, guest.craft_grid[slot], guest.cursor, right);
                        guest.craft_grid[slot] = value;
                        guest.cursor = cursor;
                    }
                    net::InventoryArea::Armor if slot < guest.armor.len() => {
                        crate::player_ops::equipment::exchange(
                            reg,
                            &mut guest.armor,
                            &mut guest.cursor,
                            slot,
                        );
                    }
                    _ => return,
                }
                refresh_held(guest);
                self.send_player_state(id);
            }
            C2S::CraftResult { size } => {
                let Ok(effects) = crate::player_ops::craft::take_result(
                    &server.world.reg,
                    usize::from(size),
                    &mut guest.craft_grid,
                    &mut guest.inventory,
                    &mut guest.cursor,
                ) else {
                    return;
                };
                effects.finish(
                    &mut server.world,
                    guest.pos.block(),
                    &mut guest.inventory,
                    true,
                    "full guest inventory after crafting",
                );
                self.send_player_state(id);
            }
            C2S::EatSelected => {
                let Some(stack) = guest.inventory.slots[guest.hotbar] else {
                    return;
                };
                let Some(food) = server.world.reg.item(stack.item).food.clone() else {
                    return;
                };
                if !crate::player_ops::nutrition::eat(
                    &mut guest.hunger,
                    &mut guest.nutrition,
                    &food,
                ) {
                    return;
                }
                if server.world.mode != "creative" {
                    let consumed = ItemStack::new(&server.world.reg, stack.item, 1);
                    if let Err(error) = server.world.record_consumed_stacks([consumed]) {
                        eprintln!("materials: guest eaten food accounting failed: {error}");
                    }
                    guest.inventory.take_one(guest.hotbar);
                }
                refresh_held(guest);
                self.send_player_state(id);
            }

            _ => {}
        }
    }
}
