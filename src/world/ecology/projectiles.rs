//! Projectile ownership adapters, collision, damage, and vessel settlement.

use crate::world::World;
use crate::inventory::ItemStack;
use crate::mobs::ProjHit;
use crate::mobs::Projectile;

impl World {
    pub fn projectiles(&self) -> &[Projectile] {
        self.population.projectiles()
    }

    pub fn spawn_projectile(&mut self, projectile: Projectile) {
        self.population.spawn_projectile(projectile)
    }

    pub fn replace_projectiles(&mut self, projectiles: Vec<Projectile>) {
        self.population.replace_projectiles(projectiles)
    }

    pub fn for_each_projectile_mut(&mut self, update: impl FnMut(&mut Projectile)) {
        self.population.for_each_projectile_mut(update)
    }

    /// Advance all bolts and arrows; returns (player index, damage) hits.
    /// Player arrows strike mobs through the normal hurt path and stick
    /// into blocks as recoverable item drops.
    pub fn tick_projectiles(
        &mut self,
        players: &[crate::server::PlayerCtx],
        dt: f32,
    ) -> Vec<(usize, f32, Option<String>)> {
        let mut dmg: Vec<(usize, f32, Option<String>)> = Vec::new();
        let mut mob_hits: Vec<(usize, f32, Option<String>, crate::planet::EntityPos)> = Vec::new();
        let mut drops: Vec<(crate::planet::BlockPos, crate::registry::ItemId)> = Vec::new();
        let mut preparation_spills: Vec<(crate::planet::BlockPos, ItemStack)> = Vec::new();
        let mut projectiles = self.population.take_projectiles();
        projectiles.retain_mut(|p| {
            if self.projectile_reserved_by_working(p.stable_id) {
                return true;
            }
            let prior_cell = p.pos.block();
            let hit = p.tick(self, players, dt);
            // Resistance is checked before dispatching the hit. Otherwise a
            // bolt that reaches a player in this very tick bypasses the ward
            // while a slower bolt one cell away is stopped.
            if !p.from_player
                && p.pos.block().is_some_and(|cell| {
                    self.resist_supernatural_pressure_at(
                        cell,
                        "projectile",
                        p.damage.max(1.0).ceil() as u64,
                    )
                })
            {
                if let (Some(at), Some(stack)) =
                    (p.pos.block().or(prior_cell), p.preparation_payload.take())
                {
                    preparation_spills.push((at, stack));
                }
                return false;
            }
            if !matches!(hit, ProjHit::None)
                && let (Some(at), Some(stack)) =
                    (p.pos.block().or(prior_cell), p.preparation_payload.take())
            {
                preparation_spills.push((at, stack));
            }
            match hit {
                ProjHit::None => true,
                ProjHit::Expired => false,
                ProjHit::Player(i) => {
                    // PvE-only mode: a player's arrow passes through other
                    // players instead of hurting them (capability E1).
                    if p.from_player && !self.ruleset().pvp {
                        return true;
                    }
                    dmg.push((i, p.damage, p.damage_type.clone()));
                    false
                }
                ProjHit::Mob(i) => {
                    let from = p
                        .pos
                        .translated(-p.vel * dt)
                        .map(|moved| moved.pos)
                        .unwrap_or(p.pos);
                    mob_hits.push((i, p.damage, p.damage_type.clone(), from));
                    false
                }
                ProjHit::Block => {
                    if let Some(it) = p.drop_item {
                        if p.owner != 0 {
                            // A guest's arrow: hand it back over the wire.
                            let stack = ItemStack::new(&self.reg, it, 1);
                            self.pending_gives.push((p.owner, stack));
                        } else {
                            let back = p
                                .pos
                                .translated(-p.vel * dt * 2.0)
                                .map(|moved| moved.pos)
                                .unwrap_or(p.pos);
                            if let Some(back) = back.block() {
                                drops.push((back, it));
                            }
                        }
                    }
                    false
                }
            }
        });
        self.population.restore_projectiles(projectiles);
        let reg = self.reg.clone();
        for (i, d, dmg_type, from) in mob_hits {
            if let Some(m) = self.population.mobs_mut().get_mut(i)
                && let Some(def) = reg.animals.get(m.species)
            {
                m.hurt(def, d, dmg_type.as_deref(), from);
            }
        }
        for (pos, it) in drops {
            self.push_drop_at(pos, ItemStack::new(&reg, it, 1));
        }
        for (pos, stack) in preparation_spills {
            match self.destroy_preparation_container_at(pos, stack, "thrown vessel impact") {
                Ok(true) => {}
                Ok(false) => {
                    // An unexpected non-preparation stable payload remains
                    // recoverable rather than being silently erased.
                    self.push_drop_at(pos, stack);
                }
                Err(error) => {
                    eprintln!(
                        "alchemy: failed to settle thrown vessel {} at {:?}: {error}",
                        stack.arcane_id, pos
                    );
                    self.push_drop_at(pos, stack);
                }
            }
        }
        dmg
    }
}
