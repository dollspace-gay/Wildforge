//! Entities graphical guest adapter.

use super::RemoteFlow;
use super::presence_label;
use crate::game::Game;
use crate::game::Lerp;
use crate::game::Remote;
use crate::net;

impl Game {
    pub(in crate::game) fn remote_entities_message(
        &mut self,
        r: &mut Remote,
        message: net::S2C,
    ) -> RemoteFlow {
        match message {
            net::S2C::Players(part) => {
                let Some(list) = r.session.players(part) else {
                    return RemoteFlow::Continue;
                };
                // Anyone the host stopped mentioning has walked out of
                // our reach; drop them rather than leaving a statue.
                let present: std::collections::HashSet<u32> =
                    list.iter().map(|(id, ..)| *id).collect();
                r.players.retain(|id, _| present.contains(id));
                r.player_positions.retain(|id, _| present.contains(id));
                r.player_lerp.retain(|id, _| present.contains(id));
                r.player_held.retain(|id, _| present.contains(id));
                r.player_implement.retain(|id, _| present.contains(id));
                r.player_style.retain(|id, _| present.contains(id));
                // New span: from wherever each player currently
                // renders, toward the fresh snapshot.
                let t = (r.player_age / r.player_interval.max(0.001)).clamp(0.0, 1.0);
                for (id, pos, yaw, held, pstyle, implement) in list {
                    if id == r.my_id {
                        continue;
                    }
                    r.player_held.insert(id, held);
                    if let Some(visual) = implement {
                        r.player_implement.insert(id, visual);
                    } else {
                        r.player_implement.remove(&id);
                    }
                    r.player_style.insert(id, pstyle);
                    r.player_positions.insert(id, pos);
                    let render_pos = pos.render_pos();
                    let cur = match r.player_lerp.get(&id) {
                        Some(l) => l.at(t),
                        None => (render_pos, yaw),
                    };
                    r.player_lerp.insert(
                        id,
                        Lerp {
                            from: cur.0,
                            to: render_pos,
                            from_yaw: cur.1,
                            to_yaw: yaw,
                            phase: 0.0,
                        },
                    );
                    let name = r
                        .session
                        .roster()
                        .get(&id)
                        .map(presence_label)
                        .unwrap_or_else(|| format!("P{id}"));
                    r.players.insert(id, (name, cur.0, cur.1));
                }
                r.player_interval = r.player_age.clamp(0.03, 0.3);
                r.player_age = 0.0;
            }
            net::S2C::Mobs(part) => {
                let Some(mut mobs) = r.session.mobs(part) else {
                    return RemoteFlow::Continue;
                };
                let t = (r.mob_age / r.mob_interval.max(0.001)).clamp(0.0, 1.0);
                let mut lerps = std::collections::HashMap::new();
                for mob in &mut mobs {
                    let render_pos = mob.pos.render_pos();
                    let (cur, phase) = match r.mob_lerp.get(&mob.id) {
                        Some(lerp) if mob.id != 0 => (lerp.at(t), lerp.phase),
                        _ => ((render_pos, mob.yaw), 0.0),
                    };
                    lerps.insert(
                        mob.id,
                        Lerp {
                            from: cur.0,
                            to: render_pos,
                            from_yaw: cur.1,
                            to_yaw: mob.yaw,
                            phase,
                        },
                    );
                    mob.present_replica_at(cur.1, phase);
                }
                if let Some((world, _)) = self.runtime.guest_mut() {
                    world.replace_mobs(mobs);
                }
                r.mob_lerp = lerps; // dead mobs' spans fall away
                r.mob_interval = r.mob_age.clamp(0.03, 0.3);
                r.mob_age = 0.0;
            }
            net::S2C::Falling(part) => {
                if let Some(falling) = r.session.falling(part)
                    && let Some((world, _)) = self.runtime.guest_mut()
                {
                    world.replace_falling_blocks(falling);
                }
            }
            net::S2C::Bolts(part) => {
                if let Some(projectiles) = r.session.bolts(part)
                    && let Some((world, _)) = self.runtime.guest_mut()
                {
                    world.replace_projectiles(projectiles);
                }
            }
            net::S2C::LooseItems(part) => {
                if let Some(items) = r.session.loose_items(part)
                    && let Some((world, _)) = self.runtime.guest_mut()
                {
                    world.replace_loose_items(items);
                }
            }
            _ => {}
        }
        RemoteFlow::Continue
    }
}
