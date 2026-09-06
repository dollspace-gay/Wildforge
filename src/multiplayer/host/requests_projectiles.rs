//! Authenticated projectiles request adapter.

use super::{C2S, HostSession, Server, Vec3, refresh_held, take_ammo};

impl HostSession {
    pub(super) fn request_projectiles(&mut self, server: &mut Server, id: u32, msg: C2S) {
        let Some(guest) = self.guests.get_mut(&id) else {
            return;
        };
        if let C2S::FireProjectile { direction, charge } = msg {
            if guest.action_cooldown > 0.0 || !direction.is_finite() || direction.length() < 0.5 {
                return;
            }
            let direction = direction.normalize();
            let selected = guest.inventory.slots[guest.hotbar];
            let Some(selected) = selected else { return };
            let def = server.world.reg.item(selected.item).clone();
            let creative = server.world.mode == "creative";
            let (speed, damage, tile, drop_item, preparation_payload) = if let Some(bow) = def.bow {
                let Some(ammo) =
                    take_ammo(&mut guest.inventory, &server.world.reg, "arrow", creative)
                else {
                    return;
                };
                let charge = charge.clamp(0.0, 1.0);
                if !creative {
                    guest.inventory.wear_tool(&server.world.reg, guest.hotbar);
                }
                (
                    bow.speed * (0.6 + 0.4 * charge),
                    bow.damage * (0.45 + 0.55 * charge),
                    server.world.reg.item(ammo).icon,
                    (!creative).then_some(ammo),
                    None,
                )
            } else if let Some(speed) = def.throw_speed {
                let state_bearing = selected.arcane_id != 0;
                let removed = if creative && !state_bearing {
                    None
                } else {
                    guest.inventory.take_one_stack(guest.hotbar)
                };
                if state_bearing && removed.is_none() {
                    return;
                }
                (
                    speed,
                    0.0,
                    def.icon,
                    None,
                    removed.filter(|stack| stack.arcane_id != 0),
                )
            } else {
                return;
            };
            guest.action_cooldown = 0.25;
            let pos = guest
                .pos
                .translated(Vec3::new(0.0, 1.6, 0.0) + direction * 0.4)
                .expect("guest projectile starts beside the player")
                .pos;
            server.world.spawn_projectile(crate::mobs::Projectile {
                stable_id: 0,
                pos,
                vel: direction * speed.min(40.0),
                tile,
                damage: damage.clamp(0.0, 12.0),
                damage_type: None,
                age: 0.0,
                from_player: true,
                drop_item,
                preparation_payload,
                owner: id,
            });
            refresh_held(guest);
            self.send_player_state(id);
        }
    }
}
