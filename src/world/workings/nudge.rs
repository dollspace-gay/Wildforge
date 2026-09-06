//! Nudge workings transaction coordination.

use super::vec3_milli;
use super::working_distance;
use crate::planet::BlockPos;
use crate::workings::NudgeEntityKind;
use crate::workings::WorkingEffect;
use crate::workings::WorkingResult;
use crate::workings::WorkingTargetSnapshot;
use crate::world::BlockEntity;
use crate::world::World;

impl World {
    /// Deflect one host-owned projectile through its ordinary velocity and
    /// collision path. The client supplies only the stable target identity;
    /// direction, magnitude, reach, and visibility are reconstructed here.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn begin_nudge_working(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        projectile_id: u64,
        magnitude: u32,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        self.begin_nudge_working_definition(
            actor,
            actor_label,
            source,
            wand_id,
            "base:nudge",
            projectile_id,
            magnitude,
            forced,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_nudge_working_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        projectile_id: u64,
        magnitude: u32,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        if !(1..=4).contains(&magnitude) {
            return Err("Nudge magnitude is bounded to 1..=4.".into());
        }
        let projectile = self
            .projectiles()
            .iter()
            .find(|projectile| projectile.stable_id == projectile_id)
            .cloned();
        let Some(projectile) = projectile else {
            return self.begin_nudge_loose_item_working(
                actor,
                actor_label,
                source,
                wand_id,
                working_id,
                projectile_id,
                forced,
            );
        };
        let target = projectile
            .pos
            .block()
            .ok_or("That projectile is outside a valid target cell.")?;
        let origin = source.entity_center();
        let delta = origin.local_delta_to(projectile.pos);
        let distance = delta.length();
        if !distance.is_finite() || distance > 6.25 {
            return Err("Nudge is a short-range working.".into());
        }
        if crate::raycast::raycast_at(self, origin, delta, (distance - 0.2).max(0.0)).is_some() {
            return Err("Solid terrain breaks Nudge's line of sight.".into());
        }
        let away = delta.normalize_or_zero();
        if away == glam::Vec3::ZERO {
            return Err("Nudge cannot resolve an impulse at zero distance.".into());
        }
        let impulse = away * (1.25 + magnitude as f32 * 0.75);
        let effect = WorkingEffect::Impulse {
            entity_id: projectile_id,
            entity_kind: NudgeEntityKind::Projectile,
            source,
            target,
            before_velocity_milli: vec3_milli(projectile.vel),
            impulse_milli: vec3_milli(impulse),
            expected_version: projectile_id,
        };
        self.reserve_wand_effect(
            actor,
            actor_label,
            source,
            wand_id,
            working_id,
            vec![WorkingTargetSnapshot::Entity {
                stable_id: projectile_id,
                kind: "projectile".into(),
                version: projectile_id,
            }],
            Vec::new(),
            effect,
            magnitude,
            working_distance(source, target),
            0,
            forced,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn begin_nudge_loose_item_working(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        item_id: u64,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        let item = self
            .loose_items()
            .iter()
            .find(|item| item.stable_id == item_id)
            .cloned()
            .ok_or("That dropped item is no longer present.")?;
        let target = item
            .pos
            .block()
            .ok_or("That dropped item is outside a valid target cell.")?;
        let origin = source.entity_center();
        let delta = origin.local_delta_to(item.pos);
        let distance = delta.length();
        if !distance.is_finite() || distance > 6.25 {
            return Err("Nudge is a short-range working.".into());
        }
        if crate::raycast::raycast_at(self, origin, delta, (distance - 0.2).max(0.0)).is_some() {
            return Err("Solid terrain breaks Nudge's line of sight.".into());
        }
        let away = delta.normalize_or_zero();
        if away == glam::Vec3::ZERO {
            return Err("Nudge cannot resolve an impulse at zero distance.".into());
        }
        // Count is the ordinary mass proxy carried by a loose stack. It
        // raises cost while lowering acceleration, keeping bulk cargo firmly
        // in the domain of belts, carts, cranes, and boats.
        let mass_band = 1 + item.count.max(1).ilog2().min(3);
        let impulse = away * (2.0 / mass_band as f32);
        let effect = WorkingEffect::Impulse {
            entity_id: item_id,
            entity_kind: NudgeEntityKind::DroppedItem,
            source,
            target,
            before_velocity_milli: vec3_milli(item.vel),
            impulse_milli: vec3_milli(impulse),
            expected_version: item_id,
        };
        self.reserve_wand_effect(
            actor,
            actor_label,
            source,
            wand_id,
            working_id,
            vec![WorkingTargetSnapshot::Entity {
                stable_id: item_id,
                kind: "dropped_item".into(),
                version: item_id,
            }],
            Vec::new(),
            effect,
            mass_band,
            working_distance(source, target),
            0,
            forced,
        )
    }

    /// Toggle the physical draft latch on one visible firebox. This is a
    /// named machine operation: it changes no block identity, creates no
    /// matter, and the ordinary steam simulation consumes the resulting
    /// open/closed state.
    #[cfg(test)]
    pub fn begin_nudge_mechanism_working(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        target: BlockPos,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        self.begin_nudge_mechanism_working_definition(
            actor,
            actor_label,
            source,
            wand_id,
            "base:nudge",
            target,
            forced,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_nudge_mechanism_working_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        target: BlockPos,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        let block = self.get_block_at(target);
        let definition = self.reg.block(block);
        if definition.interaction.as_deref() != Some("firebox")
            || !matches!(self.block_entity_at(&target), Some(BlockEntity::Steam(_)))
        {
            return Err("Nudge supports only an embodied firebox draft latch here.".into());
        }
        let before_state = match self.block_entity_at(&target) {
            Some(BlockEntity::Steam(steam)) => u8::from(steam.draft_closed),
            _ => return Err("The firebox lost its physical draft latch.".into()),
        };
        let effect = WorkingEffect::OperateMechanism {
            source,
            target,
            block_name: definition.name.clone(),
            before_state,
            after_state: before_state ^ 1,
        };
        self.reserve_wand_effect(
            actor,
            actor_label,
            source,
            wand_id,
            working_id,
            vec![self.block_snapshot(target)],
            Vec::new(),
            effect,
            1,
            working_distance(source, target),
            0,
            forced,
        )
    }

    pub fn is_nudge_mechanism_at(&self, target: BlockPos) -> bool {
        self.reg
            .block(self.get_block_at(target))
            .interaction
            .as_deref()
            == Some("firebox")
            && matches!(self.block_entity_at(&target), Some(BlockEntity::Steam(_)))
    }

    /// Moving targets continue through ordinary physics while the player
    /// settles the wand. At commit, capture the exact host velocity into the
    /// write-ahead transaction; effect replay can then distinguish its before
    /// and after states without freezing or teleporting the entity.
    pub(super) fn refresh_nudge_velocity(&mut self, id: u64) -> Result<(), String> {
        let Some((entity_id, entity_kind)) = self
            .workings_state
            .as_ref()
            .and_then(|state| state.active.get(&id))
            .and_then(|transaction| match transaction.effect {
                WorkingEffect::Impulse {
                    entity_id,
                    entity_kind,
                    ..
                } => Some((entity_id, entity_kind)),
                _ => None,
            })
        else {
            return Ok(());
        };
        let velocity = match entity_kind {
            NudgeEntityKind::Projectile => self
                .projectiles()
                .iter()
                .find(|projectile| projectile.stable_id == entity_id)
                .map(|projectile| vec3_milli(projectile.vel)),
            NudgeEntityKind::DroppedItem => self
                .loose_items()
                .iter()
                .find(|item| item.stable_id == entity_id)
                .map(|item| vec3_milli(item.vel)),
        }
        .ok_or("The Nudge target left the host simulation before release.")?;
        let state = self
            .workings_state
            .as_mut()
            .ok_or("The world has no workings authority.")?;
        let transaction = state
            .active
            .get_mut(&id)
            .ok_or("The Nudge transaction ended before release.")?;
        if let WorkingEffect::Impulse {
            before_velocity_milli,
            ..
        } = &mut transaction.effect
        {
            *before_velocity_milli = velocity;
        }
        transaction.validate().map_err(|error| error.to_string())
    }
}
