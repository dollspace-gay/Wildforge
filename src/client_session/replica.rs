//! Reassembled host entities become local replica values before presentation.

use super::assembly::SnapshotAssembler;
use super::palette::ContentMap;
use crate::entity::ItemEntity;
use crate::mobs::{Mob, Projectile};
use crate::net::{BoltSnap, FallSnap, LooseItemSnap, MobSnap, PlayerSnap, Snapshot};
use crate::registry::Registry;
use crate::world::FallingBlock;

/// Welcome replaces every receiver, including partial entity snapshots.
#[derive(Default)]
pub(super) struct EntitySnapshots {
    players: SnapshotAssembler<PlayerSnap>,
    mobs: SnapshotAssembler<MobSnap>,
    bolts: SnapshotAssembler<BoltSnap>,
    loose_items: SnapshotAssembler<LooseItemSnap>,
    falling: SnapshotAssembler<FallSnap>,
}

impl EntitySnapshots {
    pub(super) fn players(&mut self, part: Snapshot<PlayerSnap>) -> Option<Vec<PlayerSnap>> {
        self.players.accept(part)
    }

    pub(super) fn mobs(
        &mut self,
        part: Snapshot<MobSnap>,
        registry: &Registry,
    ) -> Option<Vec<Mob>> {
        Some(
            self.mobs
                .accept(part)?
                .into_iter()
                .filter(|snapshot| usize::from(snapshot.species) < registry.animals.len())
                .map(|snapshot| {
                    let mut mob =
                        Mob::new_at(usize::from(snapshot.species), snapshot.pos, snapshot.yaw);
                    mob.id = snapshot.id;
                    mob.growth = snapshot.growth;
                    mob.health = snapshot.health;
                    mob.hurt_flash = snapshot.hurt;
                    mob.fed = snapshot.fed;
                    mob
                })
                .collect(),
        )
    }

    pub(super) fn bolts(&mut self, part: Snapshot<BoltSnap>) -> Option<Vec<Projectile>> {
        Some(
            self.bolts
                .accept(part)?
                .into_iter()
                .map(|snapshot| Projectile {
                    stable_id: snapshot.id,
                    pos: snapshot.pos,
                    vel: snapshot.vel,
                    tile: snapshot.tile,
                    damage: 0.0,
                    damage_type: None,
                    age: snapshot.age,
                    from_player: false,
                    drop_item: None,
                    preparation_payload: None,
                    owner: 0,
                })
                .collect(),
        )
    }

    pub(super) fn loose_items(
        &mut self,
        part: Snapshot<LooseItemSnap>,
        content: &ContentMap,
    ) -> Option<Vec<ItemEntity>> {
        Some(
            self.loose_items
                .accept(part)?
                .into_iter()
                .filter_map(|snapshot| content.loose_item(&snapshot))
                .collect(),
        )
    }

    pub(super) fn falling(
        &mut self,
        part: Snapshot<FallSnap>,
        content: &ContentMap,
    ) -> Option<Vec<FallingBlock>> {
        Some(
            self.falling
                .accept(part)?
                .into_iter()
                .map(|snapshot| FallingBlock {
                    pos: snapshot.pos,
                    vel: 0.0,
                    block: content.block(snapshot.block),
                })
                .collect(),
        )
    }
}

#[cfg(test)]
#[path = "replica_tests.rs"]
mod tests;
