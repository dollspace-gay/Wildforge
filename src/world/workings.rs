//! Host-authoritative execution of magical workings.
//!
//! This module is the only bridge from declarative working shells to world
//! mutation. Each public entry point constructs one named domain effect; the
//! reservation/settlement machinery never accepts scripts or arbitrary block
//! edits.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::*;
use crate::arcane::{ArcaneOwner, Current, DrossMedium, LinkedFileReplacement};
use crate::implements::{ImplementAuditEvent, ImplementKind, STRUCTURAL_SPARK_UNITS};
use crate::workings::{
    CurrentDebit, DeliveryMode, NudgeEntityKind, PhysicalDebit, PhysicalDebitKind, PlantAdvance,
    PreservationKind, StrainInputs, WorkingApparatus, WorkingCue, WorkingCueKind, WorkingEffect,
    WorkingHandler, WorkingPhase, WorkingResult, WorkingTargetSnapshot, WorkingTransaction,
};

const AMBIENT_SAFE_FLOOR: u64 = 64;
const ROOTWAKE_WATER_HU: u64 = crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL;
const ROOTWAKE_NUTRIENT_UNITS: u64 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Settlement {
    Complete,
    Cancel,
    Interrupt,
}

impl World {
    pub(super) fn projectile_reserved_by_working(&self, stable_id: u64) -> bool {
        self.workings_state.as_ref().is_some_and(|state| {
            state.active.values().any(|transaction| {
                matches!(
                    transaction.effect,
                    WorkingEffect::Impulse { entity_id, .. } if entity_id == stable_id
                ) && transaction.phase != WorkingPhase::PendingApply
            })
        })
    }

    /// Reconstruct one wand request from guest-safe intent through the same
    /// native dispatch for solo, hosted players, dedicated hosts, and agents.
    /// Callers prove custody of `wand_id`; this layer owns content, target,
    /// range, physical-ledger, and effect capability validation.
    #[allow(clippy::too_many_arguments)]
    pub fn begin_wand_working(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        target: crate::workings::WorkingTargetIntent,
        inventory: Option<&crate::inventory::Inventory>,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        use crate::workings::WorkingTargetIntent;

        let definition = self
            .reg
            .workings
            .get(working_id)
            .ok_or("That working is not registered in this world.")?;
        if definition.mode != DeliveryMode::Wand {
            return Err("A constructed ritual cannot be requested through a wand.".into());
        }
        match (definition.handler, target) {
            (WorkingHandler::Trace, WorkingTargetIntent::Block { pos, .. }) => {
                self.require_carried_tuning_lens(
                    inventory.ok_or("Trace needs the authoritative inventory.")?,
                )?;
                self.begin_trace_working_definition(
                    actor,
                    actor_label,
                    source,
                    wand_id,
                    working_id,
                    pos,
                    20 * 30,
                    forced,
                )
            }
            (WorkingHandler::Trace, WorkingTargetIntent::None) => {
                self.require_carried_tuning_lens(
                    inventory.ok_or("Trace needs the authoritative inventory.")?,
                )?;
                self.begin_trace_working_definition(
                    actor,
                    actor_label,
                    source,
                    wand_id,
                    working_id,
                    source,
                    20 * 30,
                    forced,
                )
            }
            (WorkingHandler::Gleam, WorkingTargetIntent::Block { pos, .. }) => {
                let physical_focus = self
                    .reg
                    .block(self.get_block_at(pos))
                    .observation
                    .as_ref()
                    .is_some_and(|observation| {
                        observation
                            .categories
                            .iter()
                            .any(|category| category == "apparatus")
                    });
                if !physical_focus {
                    return Err(
                        "A detached Gleam needs a visible physical apparatus focus; use empty air for the wand tip."
                            .into(),
                    );
                }
                self.begin_gleam_working_definition(
                    actor,
                    actor_label,
                    source,
                    wand_id,
                    working_id,
                    pos,
                    6,
                    20 * 30,
                    forced,
                )
            }
            (WorkingHandler::Gleam, WorkingTargetIntent::None) => self
                .begin_gleam_working_definition(
                    actor,
                    actor_label,
                    source,
                    wand_id,
                    working_id,
                    source,
                    6,
                    20 * 30,
                    forced,
                ),
            (
                WorkingHandler::Ignite,
                WorkingTargetIntent::Block {
                    pos,
                    adjacent: Some(fire_cell),
                },
            ) => self.begin_kindle_working_definition(
                actor,
                actor_label,
                source,
                wand_id,
                working_id,
                pos,
                fire_cell,
                forced,
            ),
            (WorkingHandler::Rootwake, WorkingTargetIntent::Block { pos, .. }) => self
                .begin_rootwake_working_definition(
                    actor,
                    actor_label,
                    source,
                    wand_id,
                    working_id,
                    pos,
                    forced,
                ),
            (WorkingHandler::Draw, WorkingTargetIntent::Water { from, to, water_hu }) => self
                .begin_draw_working_definition(
                    actor,
                    actor_label,
                    source,
                    wand_id,
                    working_id,
                    from,
                    to,
                    water_hu,
                    forced,
                ),
            (WorkingHandler::Nudge, WorkingTargetIntent::Entity { stable_id }) => self
                .begin_nudge_working_definition(
                    actor,
                    actor_label,
                    source,
                    wand_id,
                    working_id,
                    stable_id,
                    2,
                    forced,
                ),
            (WorkingHandler::Nudge, WorkingTargetIntent::Block { pos, .. }) => self
                .begin_nudge_mechanism_working_definition(
                    actor,
                    actor_label,
                    source,
                    wand_id,
                    working_id,
                    pos,
                    forced,
                ),
            (
                WorkingHandler::Fieldmend,
                WorkingTargetIntent::Inventory {
                    target_slot,
                    material_slot: Some(material_slot),
                    magnitude,
                },
            ) => self.begin_fieldmend_working_definition(
                actor,
                actor_label,
                source,
                wand_id,
                working_id,
                inventory.ok_or("Fieldmend needs the authoritative inventory.")?,
                usize::from(target_slot),
                usize::from(material_slot),
                magnitude,
                forced,
            ),
            (
                WorkingHandler::Holdfast,
                WorkingTargetIntent::Inventory {
                    target_slot,
                    material_slot: None,
                    ..
                },
            ) => self.begin_holdfast_working_definition(
                actor,
                actor_label,
                source,
                wand_id,
                working_id,
                inventory.ok_or("Holdfast needs the authoritative inventory.")?,
                usize::from(target_slot),
                20 * 60,
                forced,
            ),
            (WorkingHandler::Holdfast, WorkingTargetIntent::Block { pos, .. }) => self
                .begin_holdfast_mounted_working_definition(
                    actor,
                    actor_label,
                    source,
                    wand_id,
                    working_id,
                    pos,
                    20 * 60,
                    forced,
                ),
            _ => Err(format!(
                "{} does not accept that target through its native handler.",
                definition.label
            )),
        }
    }

    /// Trace is not clairvoyance supplied by a wand alone: it sharpens the
    /// discovery arc's fitted physical instrument. The host accepts only a
    /// live, embodied lens whose opaque identity has real Current custody.
    fn require_carried_tuning_lens(
        &self,
        inventory: &crate::inventory::Inventory,
    ) -> Result<(), String> {
        let lens = self
            .reg
            .item_id("base:tuning_lens")
            .ok_or("This world has no fitted tuning-lens content.")?;
        let present = inventory.slots.iter().flatten().any(|stack| {
            stack.item == lens
                && stack.count == 1
                && stack.durability != 0
                && stack.arcane_id != 0
                && self
                    .arcane_ledger
                    .as_ref()
                    .and_then(|ledger| ledger.account(&ArcaneOwner::Item(stack.arcane_id)))
                    .is_some()
        });
        present.then_some(()).ok_or_else(|| {
            "Trace can only strengthen a carried, fitted tuning lens with an unspent element."
                .into()
        })
    }

    /// Begin the harmless sensing working. The active transaction itself is
    /// the bounded observation capability read by lens/renderer code.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn begin_trace_working(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        target: BlockPos,
        duration_ticks: u64,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        self.begin_trace_working_definition(
            actor,
            actor_label,
            source,
            wand_id,
            "base:trace",
            target,
            duration_ticks,
            forced,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_trace_working_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        target: BlockPos,
        duration_ticks: u64,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        let effect = WorkingEffect::Observe {
            origin: target,
            expires_tick: self.working_tick().saturating_add(duration_ticks),
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
            duration_ticks,
            forced,
        )
    }

    /// Begin a temporary point light. No voxel or inventory item is created.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn begin_gleam_working(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        target: BlockPos,
        intensity: u8,
        duration_ticks: u64,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        self.begin_gleam_working_definition(
            actor,
            actor_label,
            source,
            wand_id,
            "base:gleam",
            target,
            intensity,
            duration_ticks,
            forced,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_gleam_working_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        target: BlockPos,
        intensity: u8,
        duration_ticks: u64,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        if intensity == 0 || intensity > 8 {
            return Err("Gleam is bounded to intensity 1..=8.".into());
        }
        let effect = WorkingEffect::PointLight {
            source,
            target,
            intensity,
            expires_tick: self.working_tick().saturating_add(duration_ticks),
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
            u32::from(intensity),
            working_distance(source, target),
            duration_ticks,
            forced,
        )
    }

    /// Kindle supplies initial heat only. The named effect records both the
    /// ordinary fuel and the replaceable cell where normal fire will live.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn begin_kindle_working(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        fuel: BlockPos,
        fire_cell: BlockPos,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        self.begin_kindle_working_definition(
            actor,
            actor_label,
            source,
            wand_id,
            "base:kindle",
            fuel,
            fire_cell,
            forced,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_kindle_working_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        fuel: BlockPos,
        fire_cell: BlockPos,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        self.validate_kindle_target(fuel, fire_cell)?;
        let effect = WorkingEffect::Ignite {
            fuel,
            fire_cell,
            expected_fuel: self.get_block_at(fuel).0,
            expected_air: self.get_block_at(fire_cell).0,
            player_caused: true,
        };
        let targets = vec![self.block_snapshot(fuel), self.block_snapshot(fire_cell)];
        let physical = vec![PhysicalDebit {
            kind: PhysicalDebitKind::Heat,
            source: format!("fuel:{fuel:?}"),
            content_id: self.reg.block(self.get_block_at(fuel)).name.clone(),
            units: 1,
            expected_version: 0,
        }];
        self.reserve_wand_effect(
            actor,
            actor_label,
            source,
            wand_id,
            working_id,
            targets,
            physical,
            effect,
            1,
            working_distance(source, fuel),
            0,
            forced,
        )
    }

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
    fn begin_nudge_loose_item_working(
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

    /// Reserve a wasteful portable repair against two exact inventory slots.
    /// The actual stack edit is a write-ahead external adapter completed by
    /// the authoritative local/host profile owner.
    #[allow(clippy::too_many_arguments)]
    #[cfg(test)]
    pub fn begin_fieldmend_working(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        inventory: &crate::inventory::Inventory,
        target_slot: usize,
        material_slot: usize,
        magnitude: u32,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        self.begin_fieldmend_working_definition(
            actor,
            actor_label,
            source,
            wand_id,
            "base:fieldmend",
            inventory,
            target_slot,
            material_slot,
            magnitude,
            forced,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_fieldmend_working_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        inventory: &crate::inventory::Inventory,
        target_slot: usize,
        material_slot: usize,
        magnitude: u32,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        if target_slot >= crate::inventory::TOTAL_SLOTS
            || material_slot >= crate::inventory::TOTAL_SLOTS
            || target_slot == material_slot
        {
            return Err("Fieldmend needs two distinct authoritative inventory slots.".into());
        }
        let target_stack =
            inventory.slots[target_slot].ok_or("Fieldmend's target slot is empty.")?;
        let material_stack =
            inventory.slots[material_slot].ok_or("Fieldmend's matching-stock slot is empty.")?;
        let target = self.reg.item(target_stack.item);
        if target_stack.count != 1
            || target_stack.arcane_id != 0
            || target.food.is_some()
            || target.durability == 0
            || target_stack.durability == 0
            || target_stack.durability >= target.durability
        {
            return Err("Fieldmend supports one damaged, still-serviceable portable item.".into());
        }
        let repair_name = format!("{}/forge_scrap", target.name);
        let residue_name = format!("{}/primitive_scale", target.name);
        let repair_item = self
            .reg
            .item_id(&repair_name)
            .ok_or("This portable item has no recipe-declared matching repair stock.")?;
        let residue_item = self
            .reg
            .item_id(&residue_name)
            .ok_or("This repair family has no ordinary residue definition.")?;
        if material_stack.item != repair_item
            || material_stack.count != 1
            || material_stack.arcane_id != 0
        {
            return Err(format!(
                "Fieldmend needs one separated stack of {}.",
                self.reg.item(repair_item).label
            ));
        }
        let restored = magnitude
            .clamp(1, 16)
            .min(target.durability - target_stack.durability);
        let after = target_stack.durability.saturating_add(restored);
        let item_id = inventory_target_id(actor, target_slot, target_stack.item.0);
        let material_id = inventory_target_id(actor, material_slot, material_stack.item.0);
        let material_units = self
            .reg
            .item(repair_item)
            .materials
            .values()
            .try_fold(0u64, |sum, units| sum.checked_add(*units))
            .ok_or("Fieldmend matching material quantity overflowed.")?;
        if material_units == 0 {
            return Err("Fieldmend refuses unaccounted or massless repair stock.".into());
        }
        let targets = vec![
            WorkingTargetSnapshot::Item {
                stable_id: item_id,
                item_name: target.name.clone(),
                durability: target_stack.durability,
                age_ticks: 0,
                version: target_slot as u64,
            },
            WorkingTargetSnapshot::Item {
                stable_id: material_id,
                item_name: repair_name.clone(),
                durability: material_stack.durability,
                age_ticks: 0,
                version: material_slot as u64,
            },
        ];
        let physical = vec![
            PhysicalDebit {
                kind: PhysicalDebitKind::Material,
                source: format!("inventory:{material_slot}"),
                content_id: repair_name.clone(),
                units: material_units,
                expected_version: material_slot as u64,
            },
            PhysicalDebit {
                kind: PhysicalDebitKind::Item,
                source: format!("inventory:{target_slot}"),
                content_id: target.name.clone(),
                units: u64::from(restored),
                expected_version: target_slot as u64,
            },
        ];
        self.reserve_wand_effect(
            actor,
            actor_label,
            source,
            wand_id,
            working_id,
            targets,
            physical,
            WorkingEffect::RepairItem {
                item_id,
                item_name: target.name.clone(),
                before_durability: target_stack.durability,
                after_durability: after,
                repair_material: repair_name,
                material_units,
                residue_item: self.reg.item(residue_item).name.clone(),
                residue_units: 1,
            },
            restored,
            0,
            0,
            forced,
        )
    }

    /// Begin continuous preservation of one exact carried fragile stack.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn begin_holdfast_working(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        inventory: &crate::inventory::Inventory,
        target_slot: usize,
        duration_ticks: u64,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        self.begin_holdfast_working_definition(
            actor,
            actor_label,
            source,
            wand_id,
            "base:holdfast",
            inventory,
            target_slot,
            duration_ticks,
            forced,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_holdfast_working_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        inventory: &crate::inventory::Inventory,
        target_slot: usize,
        duration_ticks: u64,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        let stack = inventory
            .slots
            .get(target_slot)
            .copied()
            .flatten()
            .ok_or("Holdfast's target slot is empty.")?;
        let definition = self.reg.item(stack.item);
        let ages = (definition.food.is_some() || definition.name.ends_with("_seed"))
            && definition.durability != 0;
        let leaks_charge = definition.arcane.is_some()
            && definition.places.is_some()
            && stack.arcane_id != 0
            && self
                .arcane_ledger
                .as_ref()
                .and_then(|ledger| ledger.account(&ArcaneOwner::Item(stack.arcane_id)))
                .is_some_and(|account| !account.current.is_empty());
        if (!ages && !leaks_charge) || stack.count != 1 {
            return Err(
                "Holdfast needs one declared perishable, seed, or charged botanical specimen."
                    .into(),
            );
        }
        let preservation_kind = if leaks_charge {
            PreservationKind::ChargeLeakage
        } else {
            PreservationKind::ElapsedAge
        };
        let item_id = if leaks_charge {
            stack.arcane_id
        } else {
            inventory_target_id(actor, target_slot, stack.item.0)
        };
        let elapsed = if ages {
            u64::from(definition.durability.saturating_sub(stack.durability))
        } else {
            0
        };
        self.reserve_wand_effect(
            actor,
            actor_label,
            source,
            wand_id,
            working_id,
            vec![WorkingTargetSnapshot::Item {
                stable_id: item_id,
                item_name: definition.name.clone(),
                durability: stack.durability,
                age_ticks: elapsed,
                version: target_slot as u64,
            }],
            vec![PhysicalDebit {
                kind: PhysicalDebitKind::ElapsedAge,
                source: format!("inventory:{target_slot}"),
                content_id: definition.name.clone(),
                units: duration_ticks.max(1),
                expected_version: target_slot as u64,
            }],
            WorkingEffect::Preserve {
                item_id,
                preservation_kind,
                before_age_ticks: elapsed,
                elapsed_ticks: 0,
                age_advance_ticks: 0,
                charge_spent_units: 0,
            },
            1,
            0,
            duration_ticks,
            forced,
        )
    }

    /// Preserve one real sample held by the discovery apparatus. This is a
    /// physical mount, not a remote inventory: ordinary mounted aging owns the
    /// actual durability decrement and consults this exact reservation.
    #[allow(clippy::too_many_arguments)]
    pub fn begin_holdfast_mounted_working_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        mount: BlockPos,
        duration_ticks: u64,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        let (bay, stack) = self
            .mounted_fragile_at(mount)
            .ok_or("Holdfast needs one live perishable or seed in that physical sample mount.")?;
        let definition = self.reg.item(stack.item);
        let item_id = mounted_target_id(mount, bay, stack.item.0);
        let elapsed = u64::from(definition.durability.saturating_sub(stack.durability));
        self.reserve_wand_effect(
            actor,
            actor_label,
            source,
            wand_id,
            working_id,
            vec![
                self.block_snapshot(mount),
                WorkingTargetSnapshot::Item {
                    stable_id: item_id,
                    item_name: definition.name.clone(),
                    durability: stack.durability,
                    age_ticks: elapsed,
                    version: u64::from(bay),
                },
            ],
            vec![PhysicalDebit {
                kind: PhysicalDebitKind::ElapsedAge,
                source: format!("sample_mount:{mount:?}:{bay}"),
                content_id: definition.name.clone(),
                units: duration_ticks.max(1),
                expected_version: u64::from(bay),
            }],
            WorkingEffect::Preserve {
                item_id,
                preservation_kind: PreservationKind::ElapsedAge,
                before_age_ticks: elapsed,
                elapsed_ticks: 0,
                age_advance_ticks: 0,
                charge_spent_units: 0,
            },
            1,
            working_distance(source, mount),
            duration_ticks,
            forced,
        )
    }

    /// Guest-safe context discovery may identify the physical apparatus, but
    /// only the authoritative host can see and accept its mounted specimen.
    pub fn holdfast_mounted_target_at(&self, mount: BlockPos) -> bool {
        self.mounted_fragile_at(mount).is_some()
    }

    fn mounted_fragile_at(&self, mount: BlockPos) -> Option<(u8, crate::inventory::ItemStack)> {
        let BlockEntity::DiscoveryApparatus(apparatus) = self.block_entity_at(&mount)? else {
            return None;
        };
        [apparatus.sample, apparatus.reference]
            .into_iter()
            .enumerate()
            .find_map(|(bay, stack)| {
                let stack = stack?;
                let definition = self.reg.item(stack.item);
                (stack.count == 1
                    && stack.durability != 0
                    && definition.durability != 0
                    && (definition.food.is_some() || definition.name.ends_with("_seed")))
                .then_some((bay as u8, stack))
            })
    }

    /// Discover two physically mounted adjacent vessels and reserve an exact
    /// payload plus the circle's lower transfer cost. Payload Current remains
    /// distinguishable from conversion Current all the way through settlement.
    #[cfg(test)]
    pub fn begin_transfer_circle_ritual(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        controller: BlockPos,
        requested_units: u64,
    ) -> Result<WorkingResult, String> {
        self.begin_transfer_circle_ritual_definition(
            actor,
            actor_label,
            "base:transfer_circle",
            controller,
            requested_units,
        )
    }

    pub fn begin_transfer_circle_ritual_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        working_id: &str,
        controller: BlockPos,
        requested_units: u64,
    ) -> Result<WorkingResult, String> {
        let layout = self.binding_frame_layout(controller);
        if !layout.valid || layout.conductor_endpoints == 0 {
            return Err(format!(
                "The transfer circle is not physically complete: {}",
                layout.problems.join(" ")
            ));
        }
        let vessels = self.ritual_vessels(controller, 2);
        if vessels.len() != 2 {
            return Err(
                "A transfer circle needs exactly two mounted charge vessels within two cells."
                    .into(),
            );
        }
        let ledger = self
            .arcane_ledger
            .as_ref()
            .ok_or("The finite Current ledger is unavailable.")?;
        let mut ranked = vessels
            .into_iter()
            .map(|(pos, stack, revision, damage)| {
                let total = ledger
                    .account(&ArcaneOwner::Item(stack.arcane_id))
                    .map_or(0, |account| account.current.total());
                (total, pos, stack, revision, damage)
            })
            .collect::<Vec<_>>();
        ranked.sort_by_key(|entry| (entry.0, entry.1));
        let destination = ranked.remove(0);
        let source = ranked.remove(0);
        if source.0 <= destination.0 {
            return Err("The two vessels are already at the same charge level.".into());
        }
        let destination_instance = self
            .implements_state
            .as_ref()
            .and_then(|state| state.instance(destination.2.arcane_id))
            .ok_or("The destination vessel has no embodied implement record.")?;
        let destination_current = ledger
            .account(&ArcaneOwner::Item(destination.2.arcane_id))
            .map_or(0, |account| account.current.total());
        let free = destination_instance
            .kind
            .capacity()
            .saturating_sub(destination_current);
        let definition = self
            .reg
            .workings
            .get(working_id)
            .cloned()
            .ok_or("That transfer-circle definition is not registered.")?;
        let maximum = requested_units
            .clamp(1, u64::from(definition.max_magnitude))
            .min(free)
            .min(source.0);
        let mut amount = maximum;
        while amount != 0 {
            let quote = definition
                .quote(amount as u32, 1, 20 * 4)
                .map_err(|error| error.to_string())?;
            if amount.saturating_add(quote.charge) <= source.0 {
                break;
            }
            amount -= 1;
        }
        if amount == 0 {
            return Err("The source cannot pay both payload and circle conversion cost.".into());
        }
        let source_owner = ArcaneOwner::Item(source.2.arcane_id);
        let destination_owner = ArcaneOwner::Item(destination.2.arcane_id);
        let mut source_current = ledger
            .account(&source_owner)
            .ok_or("The source vessel is empty.")?
            .current
            .clone();
        let payload = source_current
            .take_units(amount, [definition.focus.clone()])
            .map_err(|error| error.to_string())?;
        let targets = vec![
            WorkingTargetSnapshot::Area {
                controller,
                revision: self.binding_frame_revision(controller)?,
                cells: vec![source.1, destination.1],
            },
            WorkingTargetSnapshot::Item {
                stable_id: source.2.arcane_id,
                item_name: self.reg.item(source.2.item).name.clone(),
                durability: source.2.durability,
                age_ticks: 0,
                version: source.3,
            },
            WorkingTargetSnapshot::Item {
                stable_id: destination.2.arcane_id,
                item_name: self.reg.item(destination.2.item).name.clone(),
                durability: destination.2.durability,
                age_ticks: 0,
                version: destination.3,
            },
        ];
        let physical = vec![
            PhysicalDebit {
                kind: PhysicalDebitKind::Item,
                source: format!("vessel:{:?}", source.1),
                content_id: self.reg.item(source.2.item).name.clone(),
                units: amount,
                expected_version: source.3,
            },
            PhysicalDebit {
                kind: PhysicalDebitKind::Item,
                source: format!("vessel:{:?}", destination.1),
                content_id: self.reg.item(destination.2.item).name.clone(),
                units: amount,
                expected_version: destination.3,
            },
        ];
        self.reserve_ritual_effect(
            actor,
            actor_label,
            controller,
            source.2.arcane_id,
            self.binding_frame_revision(controller)?,
            source.4,
            working_id,
            targets,
            physical,
            WorkingEffect::TransferCurrent {
                from: source_owner,
                to: destination_owner,
                current: payload.clone(),
            },
            amount as u32,
            1,
            20 * 4,
            payload,
        )
    }

    /// Stabilize one already-running adjacent magical process. The rite uses
    /// a distinct charge source and dross vessel, deliberately lengthens the
    /// target interval, and can later route (not delete) its reduced dross.
    #[cfg(test)]
    pub fn begin_settling_rite(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        controller: BlockPos,
    ) -> Result<WorkingResult, String> {
        self.begin_settling_rite_definition(actor, actor_label, "base:settling_rite", controller)
    }

    pub fn begin_settling_rite_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        working_id: &str,
        controller: BlockPos,
    ) -> Result<WorkingResult, String> {
        let layout = self.binding_frame_layout(controller);
        if !layout.valid {
            return Err(format!(
                "The settling frame is incomplete: {}",
                layout.problems.join(" ")
            ));
        }
        let tick = self.working_tick();
        let process = self
            .workings_state
            .as_ref()
            .into_iter()
            .flat_map(|state| state.active.values())
            .filter(|transaction| {
                transaction.definition.handler != WorkingHandler::SettlingRite
                    && transaction.phase != WorkingPhase::PendingApply
                    && controller
                        .entity_center()
                        .distance_to(transaction.source.entity_center())
                        <= 3.5
            })
            .min_by_key(|transaction| transaction.id)
            .cloned()
            .ok_or("The rite needs one running magical process within three blocks.")?;
        let vessels = self.ritual_vessels(controller, 2);
        if vessels.len() < 2 {
            return Err(
                "The settling rite needs separate mounted charge and dross vessels.".into(),
            );
        }
        let ledger = self
            .arcane_ledger
            .as_ref()
            .ok_or("The finite Current ledger is unavailable.")?;
        let source = vessels
            .iter()
            .max_by_key(|(_, stack, _, _)| {
                ledger
                    .account(&ArcaneOwner::Item(stack.arcane_id))
                    .map_or(0, |account| account.current.total())
            })
            .cloned()
            .ok_or("The settling rite has no mounted charge source.")?;
        let dross_vessel = vessels
            .iter()
            .filter(|(_, stack, _, _)| stack.arcane_id != source.1.arcane_id)
            .min_by_key(|(_, stack, _, _)| {
                ledger
                    .account(&ArcaneOwner::ItemDross(stack.arcane_id))
                    .map_or(0, |account| account.current.total())
            })
            .cloned()
            .ok_or("The settling rite needs a distinct dross vessel.")?;
        let vessel_capacity = self
            .implements_state
            .as_ref()
            .and_then(|state| state.instance(dross_vessel.1.arcane_id))
            .map(|instance| instance.kind.capacity())
            .ok_or("The dross vessel has no authoritative capacity.")?;
        let vessel_load = ledger
            .account(&ArcaneOwner::Item(dross_vessel.1.arcane_id))
            .map_or(0, |account| account.current.total())
            .saturating_add(
                ledger
                    .account(&ArcaneOwner::ItemDross(dross_vessel.1.arcane_id))
                    .map_or(0, |account| account.current.total()),
            );
        if vessel_load.saturating_add(process.dross_current.total()) > vessel_capacity {
            return Err("The physical dross vessel cannot contain the target process load.".into());
        }
        let cells = vec![source.0, dross_vessel.0];
        let mut targets = vec![WorkingTargetSnapshot::Area {
            controller,
            revision: self.binding_frame_revision(controller)?,
            cells: cells.clone(),
        }];
        for (_, stack, revision, _) in [&source, &dross_vessel] {
            targets.push(WorkingTargetSnapshot::Item {
                stable_id: stack.arcane_id,
                item_name: self.reg.item(stack.item).name.clone(),
                durability: stack.durability,
                age_ticks: 0,
                version: *revision,
            });
        }
        targets.push(WorkingTargetSnapshot::Entity {
            stable_id: process.id,
            kind: "magical_process".into(),
            version: process.completion_nonce,
        });
        let remaining = process.due_tick.saturating_sub(tick).max(20);
        self.reserve_ritual_effect(
            actor,
            actor_label,
            controller,
            source.1.arcane_id,
            self.binding_frame_revision(controller)?,
            source.3.max(dross_vessel.3),
            working_id,
            targets,
            vec![
                PhysicalDebit {
                    kind: PhysicalDebitKind::Durability,
                    source: format!("settling_frame:{controller:?}"),
                    content_id: "frame_conductor_containment".into(),
                    units: u64::from(layout.network_size.max(1)),
                    expected_version: self.binding_frame_revision(controller)?,
                },
                PhysicalDebit {
                    kind: PhysicalDebitKind::Item,
                    source: format!("dross_vessel:{:?}", dross_vessel.0),
                    content_id: self.reg.item(dross_vessel.1.item).name.clone(),
                    units: 1,
                    expected_version: dross_vessel.2,
                },
            ],
            WorkingEffect::Settle {
                controller,
                process_id: process.id,
                dross_vessel_id: dross_vessel.1.arcane_id,
                dross_routed: 0,
                stabilizer_wear: 0,
            },
            u32::try_from(process.dross_current.total().clamp(1, 64)).unwrap_or(64),
            working_distance(controller, process.source),
            remaining.min(20 * 60),
            Current::default(),
        )
    }

    /// One host-side ritual dispatcher shared by local play, guests, and
    /// agents. Clients name intent and a controller; they never name costs or
    /// mutation state.
    pub fn begin_ritual(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        working_id: &str,
        controller: BlockPos,
    ) -> Result<WorkingResult, String> {
        match self
            .reg
            .workings
            .get(working_id)
            .map(|definition| definition.handler)
        {
            Some(WorkingHandler::SettlingRite) => {
                self.begin_settling_rite_definition(actor, actor_label, working_id, controller)
            }
            Some(WorkingHandler::RootingBed) => {
                self.begin_rooting_bed_ritual_definition(actor, actor_label, working_id, controller)
            }
            Some(WorkingHandler::WardBoundary) => self.begin_ward_boundary_ritual_definition(
                actor,
                actor_label,
                working_id,
                controller,
            ),
            Some(WorkingHandler::TransferCircle) => self.begin_transfer_circle_ritual_definition(
                actor,
                actor_label,
                working_id,
                controller,
                64,
            ),
            Some(_) => Err("That content is a wand working, not a constructed ritual.".into()),
            None => Err("That ritual is not registered in this world.".into()),
        }
    }

    pub fn suggested_ritual_id(&self, controller: BlockPos) -> Result<&'static str, String> {
        let layout = self.binding_frame_layout(controller);
        if !layout.valid {
            return Err(format!(
                "The frame is not yet a complete ritual apparatus: {}",
                layout.problems.join(" ")
            ));
        }
        let vessels = self.ritual_vessels(controller, 2).len();
        let adjacent_process = self
            .workings_state
            .as_ref()
            .into_iter()
            .flat_map(|state| state.active.values())
            .any(|transaction| {
                transaction.definition.handler != WorkingHandler::SettlingRite
                    && transaction.phase != WorkingPhase::PendingApply
                    && controller
                        .entity_center()
                        .distance_to(transaction.source.entity_center())
                        <= 3.5
            });
        if vessels >= 2 && adjacent_process {
            return Ok("base:settling_rite");
        }
        if self.closed_ward_boundary(controller).is_ok() {
            return Ok("base:ward_boundary");
        }
        let has_bed = (-2..=2).any(|du| {
            (-2..=2).any(|dv| {
                controller.offset(du, 0, dv).is_some_and(|pos| {
                    let definition = self.reg.block(self.get_block_at(pos));
                    definition.crop_next.is_some() || definition.sapling.is_some()
                })
            })
        });
        if has_bed {
            return Ok("base:rooting_bed");
        }
        if vessels >= 2 {
            return Ok("base:transfer_circle");
        }
        Err("The complete frame has no closed ward, prepared bed, adjacent process, or second transfer vessel to operate.".into())
    }

    pub fn begin_contextual_ritual(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        controller: BlockPos,
    ) -> Result<WorkingResult, String> {
        let working_id = self.suggested_ritual_id(controller)?;
        let started = self.begin_ritual(actor, actor_label, working_id, controller)?;
        self.activate_working(started.stable_id)
    }

    /// Plan a small prepared bed against staged shared water and soil budgets.
    /// Planning temporarily mirrors the exact sequential debits in memory and
    /// restores the world before reservation; the durable advances then replay
    /// in the same sorted order even after chunk unload or a crash.
    #[cfg(test)]
    pub fn begin_rooting_bed_ritual(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        controller: BlockPos,
    ) -> Result<WorkingResult, String> {
        self.begin_rooting_bed_ritual_definition(actor, actor_label, "base:rooting_bed", controller)
    }

    pub fn begin_rooting_bed_ritual_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        working_id: &str,
        controller: BlockPos,
    ) -> Result<WorkingResult, String> {
        let layout = self.binding_frame_layout(controller);
        if !layout.valid {
            return Err(format!(
                "The rooting bed controller is incomplete: {}",
                layout.problems.join(" ")
            ));
        }
        let focus_posts = (-2..=2)
            .flat_map(|du| (-2..=2).map(move |dv| (du, dv)))
            .filter(|(du, dv)| *du != 0 || *dv != 0)
            .filter_map(|(du, dv)| controller.offset(du, 0, dv))
            .filter(|pos| {
                self.reg
                    .block(self.get_block_at(*pos))
                    .interaction
                    .as_deref()
                    == Some("containment")
            })
            .count();
        if focus_posts < 2 {
            return Err(
                "A rooting bed needs two visible focus posts around its prepared soil.".into(),
            );
        }
        let source = self
            .ritual_vessels(controller, 2)
            .into_iter()
            .max_by_key(|(_, stack, _, _)| {
                self.arcane_ledger
                    .as_ref()
                    .and_then(|ledger| ledger.account(&ArcaneOwner::Item(stack.arcane_id)))
                    .map_or(0, |account| account.current.total())
            })
            .ok_or("The rooting bed needs a mounted charge vessel.")?;
        let mut candidates = Vec::new();
        for du in -2..=2 {
            for dv in -2..=2 {
                let Some(pos) = controller.offset(du, 0, dv) else {
                    continue;
                };
                let definition = self.reg.block(self.get_block_at(pos));
                if definition.crop_next.is_some() || definition.sapling.is_some() {
                    candidates.push(pos);
                }
            }
        }
        candidates.sort();
        candidates.truncate(16);
        if candidates.is_empty() {
            return Err("The prepared bed contains no declared crop or sapling.".into());
        }

        let mut advances = Vec::new();
        let mut initial_water = BTreeMap::new();
        let mut initial_soil = BTreeMap::new();
        let mut initial_blocks = BTreeMap::new();
        for plant in candidates {
            let Ok((advance, _)) = self.prepare_rootwake(plant) else {
                continue;
            };
            initial_blocks
                .entry(plant)
                .or_insert_with(|| self.block_snapshot(plant));
            if let Some(soil) = advance.soil_pos {
                initial_blocks
                    .entry(soil)
                    .or_insert_with(|| self.block_snapshot(soil));
                initial_soil.entry(soil).or_insert(advance.before_soil_meta);
                let block = self.get_block_at(soil);
                self.set_block_meta_at(soil, block, advance.after_soil_meta);
            }
            if let Some(water) = advance.water_source {
                initial_water
                    .entry(water)
                    .or_insert_with(|| crate::planet_atlas::ReservoirMass {
                        water_hu: advance.water_before_hu,
                        salt_mass: advance.salt_before,
                    });
                self.write_water_mass_at(
                    water,
                    crate::planet_atlas::ReservoirMass {
                        water_hu: advance.water_after_hu,
                        salt_mass: advance.salt_after,
                    },
                );
            }
            advances.push(advance);
        }
        for (soil, meta) in &initial_soil {
            let block = self.get_block_at(*soil);
            self.set_block_meta_at(*soil, block, *meta);
        }
        for (water, mass) in &initial_water {
            self.write_water_mass_at(*water, *mass);
        }
        if advances.is_empty() {
            return Err(
                "No plant in the bed presently passes habitat, space, water, and nutrient checks."
                    .into(),
            );
        }
        let mut targets = vec![WorkingTargetSnapshot::Area {
            controller,
            revision: self.binding_frame_revision(controller)?,
            cells: advances.iter().map(|advance| advance.pos).collect(),
        }];
        targets.extend(initial_blocks.into_values());
        targets.extend(
            initial_water
                .iter()
                .map(|(pos, mass)| self.reservoir_snapshot(*pos, *mass)),
        );
        let mut physical = Vec::new();
        for (water, before) in &initial_water {
            let after = advances
                .iter()
                .filter(|advance| advance.water_source == Some(*water))
                .fold(*before, |_, advance| crate::planet_atlas::ReservoirMass {
                    water_hu: advance.water_after_hu,
                    salt_mass: advance.salt_after,
                });
            physical.push(PhysicalDebit {
                kind: PhysicalDebitKind::SoilWater,
                source: format!("bed_water:{water:?}"),
                content_id: "water".into(),
                units: before.water_hu.saturating_sub(after.water_hu),
                expected_version: 0,
            });
        }
        for (soil, before) in &initial_soil {
            let after = advances
                .iter()
                .rev()
                .find(|advance| advance.soil_pos == Some(*soil))
                .map_or(*before, |advance| advance.after_soil_meta);
            physical.push(PhysicalDebit {
                kind: PhysicalDebitKind::SoilNutrients,
                source: format!("bed_soil:{soil:?}"),
                content_id: "soil_nutrients".into(),
                units: u64::from(soil::fert_of(*before).saturating_sub(soil::fert_of(after))),
                expected_version: 0,
            });
        }
        physical.retain(|debit| debit.units != 0);
        physical.push(PhysicalDebit {
            kind: PhysicalDebitKind::Durability,
            source: format!("focus_posts:{controller:?}"),
            content_id: "focus_posts".into(),
            units: focus_posts as u64,
            expected_version: 0,
        });
        self.reserve_ritual_effect(
            actor,
            actor_label,
            controller,
            source.1.arcane_id,
            self.binding_frame_revision(controller)?,
            source.3,
            working_id,
            targets,
            physical,
            WorkingEffect::AdvanceBed {
                controller,
                plants: advances.clone(),
                scheduled_tick: self.working_tick().saturating_add(20 * 12),
            },
            advances.len() as u32,
            2,
            20 * 12,
            Current::default(),
        )
    }

    /// Validate a closed, degree-two conductor loop around the controller and
    /// prepay a bounded interval of supernatural pressure resistance.
    #[cfg(test)]
    pub fn begin_ward_boundary_ritual(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        controller: BlockPos,
    ) -> Result<WorkingResult, String> {
        self.begin_ward_boundary_ritual_definition(
            actor,
            actor_label,
            "base:ward_boundary",
            controller,
        )
    }

    pub fn begin_ward_boundary_ritual_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        working_id: &str,
        controller: BlockPos,
    ) -> Result<WorkingResult, String> {
        let layout = self.binding_frame_layout(controller);
        if !layout.valid {
            return Err(format!(
                "The ward controller is incomplete: {}",
                layout.problems.join(" ")
            ));
        }
        let boundary = self.closed_ward_boundary(controller)?;
        let source = self
            .ritual_vessels(controller, 2)
            .into_iter()
            .max_by_key(|(_, stack, _, _)| {
                self.arcane_ledger
                    .as_ref()
                    .and_then(|ledger| ledger.account(&ArcaneOwner::Item(stack.arcane_id)))
                    .map_or(0, |account| account.current.total())
            })
            .ok_or("The ward needs a mounted charge source.")?;
        let segments = boundary
            .iter()
            .map(|pos| crate::workings::WardSegment {
                pos: *pos,
                expected_block: self.get_block_at(*pos).0,
                expected_damage: u16::from(self.get_meta_at(*pos)),
            })
            .collect::<Vec<_>>();
        let ire = (self.ire * 1_000.0).round() as i64;
        let mut targets = vec![WorkingTargetSnapshot::Area {
            controller,
            revision: self.binding_frame_revision(controller)?,
            cells: boundary.clone(),
        }];
        targets.extend(boundary.iter().map(|pos| self.block_snapshot(*pos)));
        targets.push(WorkingTargetSnapshot::Item {
            stable_id: source.1.arcane_id,
            item_name: self.reg.item(source.1.item).name.clone(),
            durability: source.1.durability,
            age_ticks: 0,
            version: source.2,
        });
        let physical = vec![
            PhysicalDebit {
                kind: PhysicalDebitKind::Durability,
                source: format!("ward_boundary:{controller:?}"),
                content_id: "closed_boundary".into(),
                units: boundary.len() as u64,
                expected_version: self.binding_frame_revision(controller)?,
            },
            PhysicalDebit {
                kind: PhysicalDebitKind::Item,
                source: format!("ward_source:{:?}", source.0),
                content_id: self.reg.item(source.1.item).name.clone(),
                units: 1,
                expected_version: source.2,
            },
        ];
        self.reserve_ritual_effect(
            actor,
            actor_label,
            controller,
            source.1.arcane_id,
            self.binding_frame_revision(controller)?,
            source.3,
            working_id,
            targets,
            physical,
            WorkingEffect::Ward {
                controller,
                segments,
                pressure_kind: "supernatural".into(),
                pressure_units: 0,
                ire_before_millipoints: ire,
                ire_after_millipoints: ire,
            },
            boundary.len() as u32,
            ward_radius(controller, &boundary),
            20 * 60,
            Current::default(),
        )
    }

    /// Debit a continuous ward when a supported supernatural pressure crosses
    /// its physical interior. Ordinary players, animals, and player-fired
    /// projectiles never call this path. Broken geometry leaks; overload is
    /// deterministic.
    pub fn resist_supernatural_pressure_at(
        &mut self,
        pos: BlockPos,
        kind: &str,
        pressure_units: u64,
    ) -> bool {
        if pressure_units == 0 {
            return false;
        }
        let candidate = self.workings_state.as_ref().and_then(|state| {
            state.active.values().find_map(|transaction| {
                let WorkingEffect::Ward {
                    controller,
                    segments,
                    ..
                } = &transaction.effect
                else {
                    return None;
                };
                (transaction.phase == WorkingPhase::Active
                    && inside_ward(*controller, pos, segments)
                    && segments.iter().all(|segment| {
                        self.get_block_at(segment.pos).0 == segment.expected_block
                            && u16::from(self.get_meta_at(segment.pos)) == segment.expected_damage
                    }))
                .then_some(transaction.id)
            })
        });
        let Some(id) = candidate else {
            return false;
        };
        let ire_multiplier = 1u64.saturating_add((self.ire.max(0.0) as u64).div_ceil(20));
        let cost = pressure_units.saturating_mul(ire_multiplier).max(1);
        let mut overloaded = false;
        if let Some(state) = self.workings_state.as_mut()
            && let Some(transaction) = state.active.get_mut(&id)
        {
            let convert = cost.min(transaction.return_current.total());
            if convert != 0 {
                let Ok(moved) = transaction
                    .return_current
                    .take_units(convert, std::iter::empty())
                else {
                    return false;
                };
                if transaction.dross_current.checked_add(&moved).is_err() {
                    return false;
                }
            }
            if let WorkingEffect::Ward {
                pressure_kind,
                pressure_units,
                ..
            } = &mut transaction.effect
            {
                *pressure_kind = kind.chars().take(48).collect();
                *pressure_units = pressure_units.saturating_add(cost);
            }
            overloaded = convert < cost || transaction.return_current.is_empty();
            transaction.strain.warning_band = if overloaded {
                3
            } else if transaction.return_current.total() < transaction.reserved_current.total() / 4
            {
                2
            } else {
                transaction.strain.warning_band.max(1)
            };
            if state.save().is_err() {
                return false;
            }
        }
        if overloaded {
            let _ = self.interrupt_working(id);
        }
        !overloaded
    }

    /// Build one exact conservative water parcel. Both reservoir snapshots
    /// remain in the durable transaction so replay can distinguish before,
    /// after, and an unsafe competing third state.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn begin_draw_working(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        from: BlockPos,
        to: BlockPos,
        water_hu: u64,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        self.begin_draw_working_definition(
            actor,
            actor_label,
            source,
            wand_id,
            "base:draw",
            from,
            to,
            water_hu,
            forced,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_draw_working_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        from: BlockPos,
        to: BlockPos,
        water_hu: u64,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        if from == to
            || water_hu == 0
            || water_hu > 64
            || !water_hu.is_multiple_of(crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL)
        {
            return Err("Draw moves 32 or 64 hydro-units between distinct reservoirs.".into());
        }
        let mut source_mass = self
            .water_mass_at(from)
            .ok_or("The source is not a legitimate visible water reservoir.")?;
        if source_mass.water_hu < water_hu {
            return Err("The source does not contain that exact volume.".into());
        }
        let destination_mass = self.water_mass_at(to).unwrap_or_default();
        if !self.reg.is_air(self.get_block_at(to)) && !self.reg.is_water(self.get_block_at(to)) {
            return Err(
                "The destination is neither water nor a replaceable empty reservoir.".into(),
            );
        }
        let source_before = source_mass;
        let parcel = source_mass.take(water_hu);
        let destination_after = destination_mass
            .checked_add(parcel)
            .filter(|mass| mass.water_hu <= crate::planet_atlas::HYDRO_UNITS_PER_BLOCK)
            .ok_or("The destination would overflow; no water was moved.")?;
        let source_carrier = self.water_carrier_at(from, source_before);
        let destination_carrier = self.water_carrier_at(to, destination_mass);
        let mut source_after_carrier = source_carrier;
        let parcel_carrier = source_after_carrier
            .take(source_before.water_hu, water_hu)
            .ok_or("Draw could not split the source's exact carriers.")?;
        let _destination_after_carrier = destination_carrier
            .checked_add(parcel_carrier)
            .ok_or("Draw destination carrier accounting overflowed.")?;
        let targets = vec![
            self.reservoir_snapshot_with_carrier(from, source_before, source_carrier),
            self.reservoir_snapshot_with_carrier(to, destination_mass, destination_carrier),
        ];
        let effect = WorkingEffect::TransferWater {
            from,
            to,
            water_hu: parcel.water_hu,
            salt_mass: parcel.salt_mass,
            temperature_millic: parcel_carrier.temperature_millic(parcel.water_hu),
            thermal_millic_hu: parcel_carrier.thermal_millic_hu,
            dross_units: parcel_carrier.dross_subunits / 256,
            dross_subunits: parcel_carrier.dross_subunits,
            carrier_remainder_before: source_carrier.dross_subunits % 256,
            carrier_remainder_after: source_after_carrier.dross_subunits % 256,
        };
        let mut physical = vec![PhysicalDebit {
            kind: PhysicalDebitKind::Fluid,
            source: format!("reservoir:{from:?}"),
            content_id: "water".into(),
            units: parcel.water_hu,
            expected_version: 0,
        }];
        if parcel.salt_mass != 0 {
            physical.push(PhysicalDebit {
                kind: PhysicalDebitKind::Fluid,
                source: format!("reservoir:{from:?}"),
                content_id: "salt".into(),
                units: parcel.salt_mass,
                expected_version: 0,
            });
        }
        if parcel_carrier.dross_subunits != 0 {
            physical.push(PhysicalDebit {
                kind: PhysicalDebitKind::Fluid,
                source: format!("reservoir:{from:?}"),
                content_id: "waterborne_dross_subunit".into(),
                units: parcel_carrier.dross_subunits,
                expected_version: source_carrier.dross_subunits,
            });
        }
        let _ = destination_after;
        self.reserve_wand_effect(
            actor,
            actor_label,
            source,
            wand_id,
            working_id,
            targets,
            physical,
            effect,
            u32::try_from(water_hu / crate::planet_atlas::HYDRO_UNITS_PER_VISIBLE_LEVEL)
                .map_err(|_| "Draw magnitude overflowed.")?,
            working_distance(from, to),
            0,
            forced,
        )
    }

    /// Rootwake is factored out of the random-tick path but validates the same
    /// climate/light/soil authority and additionally reserves explicit water
    /// and nutrients before changing one crop stage.
    #[cfg(test)]
    pub fn begin_rootwake_working(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        plant: BlockPos,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        self.begin_rootwake_working_definition(
            actor,
            actor_label,
            source,
            wand_id,
            "base:rootwake",
            plant,
            forced,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_rootwake_working_definition(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        plant: BlockPos,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        let (advance, targets) = self.prepare_rootwake(plant)?;
        let mut physical = vec![PhysicalDebit {
            kind: PhysicalDebitKind::SoilWater,
            source: format!("plant:{plant:?}"),
            content_id: "water".into(),
            units: advance.water_hu,
            expected_version: 0,
        }];
        if advance.nutrient_units != 0 {
            physical.push(PhysicalDebit {
                kind: PhysicalDebitKind::SoilNutrients,
                source: format!("soil:{:?}", advance.soil_pos),
                content_id: "soil_nutrients".into(),
                units: advance.nutrient_units,
                expected_version: 0,
            });
        }
        self.reserve_wand_effect(
            actor,
            actor_label,
            source,
            wand_id,
            working_id,
            targets,
            physical,
            WorkingEffect::AdvancePlant(advance),
            1,
            working_distance(source, plant),
            0,
            forced,
        )
    }

    /// Transition a settled channel from its charge-up presentation into an
    /// active host-owned process. This changes no Current custody.
    pub fn activate_working(&mut self, id: u64) -> Result<WorkingResult, String> {
        let before = self
            .workings_state
            .as_ref()
            .and_then(|state| state.active.get(&id))
            .ok_or_else(|| format!("Working {id} is not active."))?;
        if before.definition.mode == DeliveryMode::Wand
            && before.phase == WorkingPhase::Charging
            && self.working_tick().saturating_sub(before.started_tick)
                < crate::workings::MIN_WAND_SETTLE_TICKS
        {
            return Err(format!(
                "{} is still settling; its visible channel has not reached safe commitment.",
                before.definition.label
            ));
        }
        let transaction = {
            let state = self
                .workings_state
                .as_mut()
                .ok_or("The world has no workings authority.")?;
            state
                .phase(id, WorkingPhase::Active)
                .map_err(|error| error.to_string())?;
            state.save().map_err(|error| error.to_string())?;
            state
                .active
                .get(&id)
                .expect("phase checked active id")
                .clone()
        };
        let message = if transaction.definition.handler == WorkingHandler::Trace {
            self.trace_report(&transaction)?
        } else {
            format!(
                "{} settles into a stable working.",
                transaction.definition.label
            )
        };
        Ok(WorkingResult {
            success: true,
            stable_id: id,
            phase: Some(WorkingPhase::Active),
            cue: WorkingCueKind::Active,
            warning_band: transaction.strain.warning_band,
            message,
        })
    }

    pub fn complete_working(&mut self, id: u64) -> Result<WorkingResult, String> {
        self.settle_working(id, Settlement::Complete, None)
    }

    pub fn cancel_working(&mut self, id: u64) -> Result<WorkingResult, String> {
        self.settle_working(id, Settlement::Cancel, None)
    }

    pub fn interrupt_working(&mut self, id: u64) -> Result<WorkingResult, String> {
        self.settle_working(id, Settlement::Interrupt, None)
    }

    /// Settle every non-replay transaction still owned by one authenticated
    /// actor. Hosted death and disconnect both use this authority boundary so
    /// a stale connection-side channel pointer cannot strand a reservation.
    pub fn interrupt_actor_workings(
        &mut self,
        actor: [u8; 16],
    ) -> Result<Vec<WorkingResult>, String> {
        let ids = self
            .workings_state
            .as_ref()
            .into_iter()
            .flat_map(|state| state.active.values())
            .filter(|transaction| {
                transaction.actor == actor && transaction.phase != WorkingPhase::PendingApply
            })
            .map(|transaction| transaction.id)
            .collect::<Vec<_>>();
        ids.into_iter()
            .map(|id| self.interrupt_working(id))
            .collect()
    }

    /// Fieldmend's target lives in a player profile rather than a chunk. Its
    /// Current and material loss are committed first as a write-ahead
    /// `PendingApply`; the inventory mutation is then idempotent. The caller
    /// must durably save the authoritative profile and call
    /// [`World::finish_inventory_working`] before acknowledging completion.
    pub fn complete_inventory_working(
        &mut self,
        id: u64,
        inventory: &mut crate::inventory::Inventory,
    ) -> Result<WorkingResult, String> {
        let transaction = self
            .workings_state
            .as_ref()
            .and_then(|state| state.active.get(&id))
            .cloned()
            .ok_or_else(|| format!("Working {id} is not active."))?;
        if transaction.definition.mode == DeliveryMode::Wand
            && transaction.phase == WorkingPhase::Charging
        {
            if self.working_tick().saturating_sub(transaction.started_tick)
                < crate::workings::MIN_WAND_SETTLE_TICKS
            {
                return self.cancel_working(id);
            }
            self.activate_working(id)?;
        }
        self.settle_working(id, Settlement::Complete, Some(inventory))
    }

    pub fn finish_inventory_working(&mut self, id: u64) -> Result<WorkingResult, String> {
        let transaction = self
            .workings_state
            .as_ref()
            .and_then(|state| state.active.get(&id))
            .cloned()
            .ok_or_else(|| format!("Working {id} is not pending."))?;
        if transaction.phase != WorkingPhase::PendingApply
            || !matches!(transaction.effect, WorkingEffect::RepairItem { .. })
        {
            return Err("Only a profile-checkpointed inventory working may finish here.".into());
        }
        let tick = self.working_tick();
        let state = self
            .workings_state
            .as_mut()
            .ok_or("The world has no workings authority.")?;
        state
            .settle(id, "completed_profile_checkpoint", tick)
            .map_err(|error| error.to_string())?;
        state.save().map_err(|error| error.to_string())?;
        Ok(WorkingResult {
            success: true,
            stable_id: id,
            phase: None,
            cue: WorkingCueKind::Complete,
            warning_band: transaction.strain.warning_band,
            message: format!("{} completes exactly once.", transaction.definition.label),
        })
    }

    /// Reconcile write-ahead Fieldmend effects after the authenticated player
    /// profile has loaded. Applying the saved after-state is idempotent; the
    /// caller must checkpoint that profile and only then call
    /// `finish_inventory_working` for each returned id.
    pub fn resume_pending_inventory_workings(
        &mut self,
        actor: [u8; 16],
        inventory: &mut crate::inventory::Inventory,
    ) -> Result<Vec<u64>, String> {
        let ids = self
            .workings_state
            .as_ref()
            .into_iter()
            .flat_map(|state| state.active.values())
            .filter(|transaction| {
                transaction.actor == actor
                    && transaction.phase == WorkingPhase::PendingApply
                    && matches!(transaction.effect, WorkingEffect::RepairItem { .. })
            })
            .map(|transaction| transaction.id)
            .collect::<Vec<_>>();
        for id in &ids {
            self.complete_inventory_working(*id, inventory)?;
        }
        Ok(ids)
    }

    /// Ritual release leaves the constructed process running; continuous and
    /// one-shot wand workings use release as their normal settlement edge.
    pub fn release_working(&mut self, id: u64) -> Result<WorkingResult, String> {
        let transaction = self
            .workings_state
            .as_ref()
            .and_then(|state| state.active.get(&id))
            .cloned()
            .ok_or_else(|| format!("Working {id} is not active."))?;
        if transaction.definition.mode == DeliveryMode::Ritual {
            if transaction.phase == WorkingPhase::Charging {
                return self.activate_working(id);
            }
            return Ok(WorkingResult {
                success: true,
                stable_id: id,
                phase: Some(transaction.phase),
                cue: WorkingCueKind::Active,
                warning_band: transaction.strain.warning_band,
                message: format!(
                    "{} remains embodied and continues on its physical schedule.",
                    transaction.definition.label
                ),
            });
        }
        if transaction.phase == WorkingPhase::Charging {
            if self.working_tick().saturating_sub(transaction.started_tick)
                < crate::workings::MIN_WAND_SETTLE_TICKS
            {
                return self.cancel_working(id);
            }
            self.activate_working(id)?;
        }
        self.complete_working(id)
    }

    /// Moving targets continue through ordinary physics while the player
    /// settles the wand. At commit, capture the exact host velocity into the
    /// write-ahead transaction; effect replay can then distinguish its before
    /// and after states without freezing or teleporting the entity.
    fn refresh_nudge_velocity(&mut self, id: u64) -> Result<(), String> {
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

    /// Settle bounded-duration workings on the simulation clock. Rituals use
    /// this path while unloaded; a transient target that disappeared is
    /// interrupted deterministically instead of leaving orphan Current.
    pub fn tick_workings(&mut self) -> Vec<(WorkingResult, WorkingCue)> {
        let tick = self.working_tick();
        let active = self
            .workings_state
            .as_ref()
            .map(|state| {
                state
                    .active
                    .values()
                    .filter(|transaction| transaction.phase != WorkingPhase::PendingApply)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut outcomes = Vec::new();
        for transaction in active {
            let path = effect_path(&transaction.effect);
            let missing = path.iter().any(|pos| self.chunk(pos.chunk()).is_none());
            let due =
                transaction.due_tick > transaction.started_tick && transaction.due_tick <= tick;
            let broken_loaded_ritual = !missing
                && transaction.phase == WorkingPhase::Active
                && transaction.definition.mode == DeliveryMode::Ritual
                && self.validate_ritual_apparatus(&transaction).is_err();
            let should_interrupt = broken_loaded_ritual
                || (missing
                    && transaction.interruption
                        != crate::workings::InterruptionPolicy::ContinueUnloaded);
            if !due && !should_interrupt {
                continue;
            }
            if due
                && transaction.interruption == crate::workings::InterruptionPolicy::ContinueUnloaded
            {
                for pos in &path {
                    self.ensure_chunk(pos.chunk());
                }
            }
            let mut cue = self
                .working_cues()
                .into_iter()
                .find(|cue| cue.stable_id == transaction.id)
                .unwrap_or(WorkingCue {
                    stable_id: transaction.id,
                    working_id: transaction.definition.id.clone(),
                    handler: transaction.definition.handler,
                    source: transaction.source,
                    path: transaction.path.clone(),
                    kind: WorkingCueKind::Strain,
                    warning_band: transaction.strain.warning_band,
                    completion_permille: 0,
                });
            let result = if should_interrupt {
                self.interrupt_working(transaction.id)
            } else {
                match self.complete_working(transaction.id) {
                    Ok(result) => Ok(result),
                    Err(completion_error) => {
                        self.interrupt_working(transaction.id).map(|mut result| {
                            result.message = format!(
                                "{} could not complete: {completion_error} It interrupts with accounted dross.",
                                transaction.definition.label
                            );
                            result
                        })
                    }
                }
            };
            if let Ok(result) = result {
                cue.kind = result.cue;
                cue.warning_band = result.warning_band;
                cue.completion_permille = 1_000;
                outcomes.push((result, cue));
            }
        }
        outcomes
    }

    /// Apply one ordinary aging sweep to a carried target. The caller still
    /// performs the real freshness/age decrement; this method merely returns
    /// the accounted slowed amount and advances the durable monotonic age
    /// evidence. A moved or replaced target receives no benefit.
    pub fn holdfast_age_step(
        &mut self,
        actor: [u8; 16],
        slot: usize,
        stack: crate::inventory::ItemStack,
        ordinary_step: u32,
        elapsed_seconds: u32,
    ) -> u32 {
        self.holdfast_age_step_for(
            Some(actor),
            inventory_target_id(actor, slot, stack.item.0),
            slot as u64,
            stack,
            ordinary_step,
            elapsed_seconds,
        )
    }

    /// Mounted samples age on the world's ordinary container clock. The mount
    /// and bay form a stable physical identity, so moving or replacing the
    /// sample invalidates the benefit exactly like moving an inventory target.
    pub(crate) fn holdfast_mounted_age_step(
        &mut self,
        mount: BlockPos,
        bay: u8,
        stack: crate::inventory::ItemStack,
        ordinary_step: u32,
        elapsed_seconds: u32,
    ) -> u32 {
        self.holdfast_age_step_for(
            None,
            mounted_target_id(mount, bay, stack.item.0),
            u64::from(bay),
            stack,
            ordinary_step,
            elapsed_seconds,
        )
    }

    fn holdfast_age_step_for(
        &mut self,
        actor: Option<[u8; 16]>,
        expected_id: u64,
        expected_version: u64,
        stack: crate::inventory::ItemStack,
        ordinary_step: u32,
        elapsed_seconds: u32,
    ) -> u32 {
        if ordinary_step == 0 {
            return 0;
        }
        let candidate = self.workings_state.as_ref().and_then(|state| {
            state.active.values().find_map(|transaction| {
                let WorkingEffect::Preserve {
                    item_id,
                    preservation_kind: PreservationKind::ElapsedAge,
                    age_advance_ticks,
                    ..
                } = transaction.effect
                else {
                    return None;
                };
                if actor.is_some_and(|actor| transaction.actor != actor)
                    || transaction.phase != WorkingPhase::Active
                    || item_id != expected_id
                {
                    return None;
                }
                let target = transaction.targets.iter().find_map(|target| match target {
                    WorkingTargetSnapshot::Item {
                        stable_id,
                        item_name,
                        durability,
                        version,
                        ..
                    } if *stable_id == item_id && *version == expected_version => {
                        Some((item_name.as_str(), *durability))
                    }
                    _ => None,
                })?;
                Some((
                    transaction.id,
                    target.0.to_string(),
                    target.1,
                    age_advance_ticks,
                ))
            })
        });
        let Some((id, item_name, initial_durability, prior_advance)) = candidate else {
            return ordinary_step;
        };
        if self.reg.item(stack.item).name != item_name
            || stack.durability
                != initial_durability.saturating_sub(prior_advance.min(u64::from(u32::MAX)) as u32)
        {
            let _ = self.interrupt_working(id);
            return ordinary_step;
        }
        let slowed = ordinary_step.div_ceil(4).max(1);
        if let Some(state) = self.workings_state.as_mut()
            && let Some(transaction) = state.active.get_mut(&id)
            && let WorkingEffect::Preserve {
                elapsed_ticks,
                age_advance_ticks,
                charge_spent_units,
                ..
            } = &mut transaction.effect
        {
            *elapsed_ticks = elapsed_ticks.saturating_add(u64::from(ordinary_step));
            *age_advance_ticks = age_advance_ticks.saturating_add(u64::from(slowed));
            *charge_spent_units = charge_spent_units
                .saturating_add(
                    transaction
                        .definition
                        .charge_per_second
                        .saturating_mul(u64::from(elapsed_seconds)),
                )
                .min(transaction.return_current.total());
            if state.save().is_err() {
                return ordinary_step;
            }
        }
        slowed
    }

    /// Leak a charged botanical specimen through the ordinary finite Current
    /// ledger. Holdfast quarters—not reverses—the real leak and consumes only
    /// the portion of its reservation corresponding to elapsed work.
    pub fn leak_fragile_item_charge(
        &mut self,
        actor: [u8; 16],
        slot: usize,
        stack: crate::inventory::ItemStack,
        at: BlockPos,
        elapsed_seconds: u32,
    ) -> Result<u64, String> {
        if elapsed_seconds == 0 || stack.arcane_id == 0 || stack.count != 1 {
            return Ok(0);
        }
        let definition = self.reg.item(stack.item);
        let Some(arcane) = definition.arcane.as_ref() else {
            return Ok(0);
        };
        if definition.places.is_none() {
            return Ok(0);
        }
        let instability = u64::from(1_000u16.saturating_sub(arcane.stability_permille));
        let ordinary = instability
            .saturating_mul(u64::from(elapsed_seconds))
            .div_ceil(2_000)
            .max(1);
        let candidate = self.workings_state.as_ref().and_then(|state| {
            state.active.values().find_map(|transaction| {
                let WorkingEffect::Preserve {
                    item_id,
                    preservation_kind: PreservationKind::ChargeLeakage,
                    ..
                } = transaction.effect
                else {
                    return None;
                };
                if transaction.actor != actor
                    || transaction.phase != WorkingPhase::Active
                    || item_id != stack.arcane_id
                {
                    return None;
                }
                let target = transaction.targets.iter().find_map(|target| match target {
                    WorkingTargetSnapshot::Item {
                        stable_id,
                        item_name,
                        durability,
                        version,
                        ..
                    } if *stable_id == item_id && *version == slot as u64 => {
                        Some((item_name.as_str(), *durability))
                    }
                    _ => None,
                })?;
                Some((transaction.id, target.0.to_string(), target.1))
            })
        });
        let active = if let Some((id, item_name, durability)) = candidate {
            if self.reg.item(stack.item).name != item_name || stack.durability != durability {
                let _ = self.interrupt_working(id);
                None
            } else {
                Some(id)
            }
        } else {
            None
        };
        let requested = if active.is_some() {
            ordinary.div_ceil(4)
        } else {
            ordinary
        };
        let owner = ArcaneOwner::Item(stack.arcane_id);
        let (source_version, mut current) = match self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&owner))
        {
            Some(account) => (account.version, account.current.clone()),
            None => return Ok(0),
        };
        let amount = requested.min(current.total());
        if amount == 0 {
            return Ok(0);
        }
        let moved = current
            .take_units(amount, std::iter::empty())
            .map_err(|error| error.to_string())?;
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(at.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let destination = ArcaneOwner::Ambient(region);
        let mut next_state = self.workings_state.clone();
        let replacement = if let Some(id) = active {
            let state = next_state
                .as_mut()
                .ok_or("The Holdfast transaction state is unavailable.")?;
            let transaction = state
                .active
                .get_mut(&id)
                .ok_or("The Holdfast transaction ended during leakage.")?;
            let return_total = transaction.return_current.total();
            if let WorkingEffect::Preserve {
                elapsed_ticks,
                age_advance_ticks,
                charge_spent_units,
                ..
            } = &mut transaction.effect
            {
                *elapsed_ticks = elapsed_ticks.saturating_add(ordinary);
                *age_advance_ticks = age_advance_ticks.saturating_add(amount);
                *charge_spent_units = charge_spent_units
                    .saturating_add(
                        transaction
                            .definition
                            .charge_per_second
                            .saturating_mul(u64::from(elapsed_seconds)),
                    )
                    .min(return_total);
            }
            transaction.validate().map_err(|error| error.to_string())?;
            Some(LinkedFileReplacement {
                subsystem: "workings".into(),
                operation_id: id,
                relative_path: crate::workings::WORKINGS_FILE.into(),
                after: Some(state.encode().map_err(|error| error.to_string())?),
            })
        } else {
            None
        };
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The finite Current ledger is unavailable.")?;
        let transaction = crate::arcane::ArcaneTransaction::transfer(
            ledger
                .system_transaction_id()
                .map_err(|error| error.to_string())?,
            owner,
            source_version,
            destination.clone(),
            ledger.version_of(&destination),
            moved,
            crate::arcane::ArcaneAuthority::System,
            "ordinary charged botanical leakage",
        );
        if let Some(replacement) = replacement {
            ledger
                .commit_linked_files(transaction, vec![replacement])
                .map_err(|error| error.to_string())?;
            self.workings_state = next_state;
        } else {
            ledger
                .commit(transaction)
                .map_err(|error| error.to_string())?;
        }
        Ok(amount)
    }

    /// Visible active paths for local rendering and interest-managed network
    /// replication. Exact costs and target snapshots stay host-side.
    pub fn working_cues(&self) -> Vec<WorkingCue> {
        self.workings_state
            .as_ref()
            .into_iter()
            .flat_map(|state| state.active.values())
            .map(|transaction| WorkingCue {
                stable_id: transaction.id,
                working_id: transaction.definition.id.clone(),
                handler: transaction.definition.handler,
                source: transaction.source,
                path: transaction.path.clone(),
                kind: match transaction.phase {
                    WorkingPhase::Charging => WorkingCueKind::Settle,
                    WorkingPhase::Active => WorkingCueKind::Active,
                    WorkingPhase::PendingApply => WorkingCueKind::Complete,
                },
                warning_band: transaction.strain.warning_band,
                completion_permille: working_completion(self.working_tick(), transaction),
            })
            .collect()
    }

    /// Resume the second half of a crash-safe completion. The Current was
    /// already settled atomically with `PendingApply`; typed world mutation is
    /// idempotent and the durable transaction is removed only after its chunks
    /// have landed.
    pub(super) fn replay_pending_workings(&mut self) -> Result<usize, String> {
        let pending = self
            .workings_state
            .as_ref()
            .map(|state| {
                state
                    .active
                    .values()
                    .filter(|transaction| transaction.phase == WorkingPhase::PendingApply)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut replayed = 0usize;
        for transaction in pending {
            if matches!(transaction.effect, WorkingEffect::RepairItem { .. }) {
                // Player profiles are loaded by their authenticated session
                // owner. The pending transaction is the write-ahead proof;
                // that adapter applies/recognises the exact inventory after
                // state and then calls `finish_inventory_working`.
                continue;
            }
            for pos in effect_path(&transaction.effect) {
                self.ensure_chunk(pos.chunk());
            }
            self.apply_working_effect(&transaction.effect, &transaction.targets)?;
            self.save_effect_chunks(&transaction.effect)?;
            self.save_effect_sidecars(&transaction.effect)?;
            let tick = self.working_tick();
            let state = self
                .workings_state
                .as_mut()
                .ok_or("Pending working lost its authority state.")?;
            state
                .settle(transaction.id, "crash_replay_complete", tick)
                .map_err(|error| error.to_string())?;
            state.save().map_err(|error| error.to_string())?;
            replayed += 1;
        }
        Ok(replayed)
    }

    /// A held wand channel has no owner after a process restart: mouse/key
    /// state and authenticated session custody are intentionally transient.
    /// Settle those channels through their declared interruption rule while
    /// leaving embodied rituals and write-ahead PendingApply work untouched.
    pub(super) fn interrupt_loaded_wand_workings(&mut self) -> Result<usize, String> {
        let ids = self
            .workings_state
            .as_ref()
            .into_iter()
            .flat_map(|state| state.active.values())
            .filter(|transaction| {
                transaction.definition.mode == DeliveryMode::Wand
                    && transaction.phase != WorkingPhase::PendingApply
            })
            .map(|transaction| transaction.id)
            .collect::<Vec<_>>();
        for id in &ids {
            self.interrupt_working(*id)?;
        }
        Ok(ids.len())
    }

    fn binding_frame_revision(&self, controller: BlockPos) -> Result<u64, String> {
        match self.block_entity_at(&controller) {
            Some(BlockEntity::BindingFrame(frame)) => Ok(frame.revision),
            _ => Err("The ritual controller is not an embodied binding frame.".into()),
        }
    }

    /// Find the smallest intact conductor cycle that physically encloses the
    /// controller. Coordinates are reconstructed through `BlockPos::offset`
    /// so an otherwise local ward remains valid when it crosses a cube-face
    /// seam; raw face-local `u/v` arithmetic would split the same structure.
    fn closed_ward_boundary(&self, controller: BlockPos) -> Result<Vec<BlockPos>, String> {
        const RADIUS: i32 = 16;
        const MAX_SEGMENTS: usize = 64;

        let local = ward_local_positions(controller, RADIUS);
        let conductors = local
            .keys()
            .copied()
            .filter(|pos| {
                self.reg
                    .block(self.get_block_at(*pos))
                    .interaction
                    .as_deref()
                    == Some("arcane_conductor")
            })
            .collect::<BTreeSet<_>>();
        let mut unseen = conductors.clone();
        let mut candidates = Vec::<Vec<BlockPos>>::new();

        while let Some(seed) = unseen.pop_first() {
            let mut queue = VecDeque::from([seed]);
            let mut component = BTreeSet::from([seed]);
            while let Some(pos) = queue.pop_front() {
                for neighbor in ward_horizontal_neighbors(pos) {
                    if conductors.contains(&neighbor) && component.insert(neighbor) {
                        unseen.remove(&neighbor);
                        queue.push_back(neighbor);
                    }
                }
            }
            if !(4..=MAX_SEGMENTS).contains(&component.len()) {
                continue;
            }
            let is_cycle = component.iter().all(|pos| {
                ward_horizontal_neighbors(*pos)
                    .into_iter()
                    .filter(|neighbor| component.contains(neighbor))
                    .count()
                    == 2
            });
            if !is_cycle {
                continue;
            }
            let Some(boundary) = component
                .iter()
                .map(|pos| local.get(pos).copied())
                .collect::<Option<BTreeSet<_>>>()
            else {
                continue;
            };
            if !ward_interior(&boundary).is_some_and(|interior| interior.contains(&(0, 0))) {
                continue;
            }
            candidates.push(component.into_iter().collect());
        }

        candidates.sort_by_key(|candidate| (candidate.len(), candidate.clone()));
        candidates.into_iter().next().ok_or_else(|| {
            "The ward needs one closed, unbranched loop of 4..=64 Choirstone conductors around its controller within sixteen blocks.".into()
        })
    }

    fn ritual_vessels(
        &self,
        controller: BlockPos,
        radius: i32,
    ) -> Vec<(BlockPos, crate::inventory::ItemStack, u64, u16)> {
        let mut vessels = Vec::new();
        for du in -radius..=radius {
            for dv in -radius..=radius {
                if du == 0 && dv == 0 {
                    continue;
                }
                let Some(pos) = controller.offset(du, 0, dv) else {
                    continue;
                };
                if let Some(BlockEntity::ChargeVessel(vessel)) = self.block_entity_at(&pos)
                    && let Some(stack) = vessel.vessel
                    && stack.arcane_id != 0
                    && self
                        .implements_state
                        .as_ref()
                        .and_then(|state| state.instance(stack.arcane_id))
                        .is_some_and(|instance| {
                            matches!(instance.kind, ImplementKind::Vessel { .. })
                        })
                {
                    vessels.push((pos, stack, vessel.revision, vessel.damage));
                }
            }
        }
        vessels.sort_by_key(|entry| entry.0);
        vessels
    }

    #[allow(clippy::too_many_arguments)]
    fn reserve_ritual_effect(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        controller: BlockPos,
        source_vessel_id: u64,
        controller_revision: u64,
        apparatus_damage: u16,
        working_id: &str,
        targets: Vec<WorkingTargetSnapshot>,
        physical_debits: Vec<PhysicalDebit>,
        effect: WorkingEffect,
        magnitude: u32,
        distance: u16,
        duration_ticks: u64,
        payload: Current,
    ) -> Result<WorkingResult, String> {
        let definition = self
            .reg
            .workings
            .get(working_id)
            .cloned()
            .ok_or_else(|| format!("Unknown ritual {working_id}."))?;
        if definition.mode != DeliveryMode::Ritual
            || definition.handler.effect_kind() != effect.kind()
        {
            return Err("The selected content does not own that native ritual capability.".into());
        }
        let quote = definition
            .quote(magnitude, distance, duration_ticks)
            .map_err(|error| error.to_string())?;
        let source_owner = ArcaneOwner::Item(source_vessel_id);
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(controller.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let local_capacity_permille = self.local_capacity_permille(region);
        let tick = self.working_tick();
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no finite Current ledger.")?;
        let source_account = ledger
            .account(&source_owner)
            .cloned()
            .ok_or("The ritual's mounted source vessel is empty.")?;
        let mut available = source_account.current.clone();
        available
            .checked_sub(&payload)
            .map_err(|_| "The ritual payload is not present in its named source vessel.")?;
        let cost = available
            .take_units(quote.charge, [definition.focus.clone()])
            .map_err(|error| error.to_string())?;
        let mut reserved = payload.clone();
        reserved
            .checked_add(&cost)
            .map_err(|error| error.to_string())?;
        let strain = crate::workings::deterministic_strain(StrainInputs {
            resonance_mismatch_permille: 0,
            component_instability_permille: 0,
            throughput: quote.charge,
            safe_throughput: definition.safe_throughput.max(1),
            local_capacity_permille,
            apparatus_damage_permille: apparatus_damage.min(1_000),
            ..StrainInputs::default()
        })
        .map_err(|error| error.to_string())?;
        if strain.refuses {
            return Err("The visibly damaged ritual apparatus refuses this load.".into());
        }
        let dross_units = quote
            .base_dross
            .saturating_add(strain.extra_dross)
            .min(cost.total());
        let mut clean_cost = cost;
        let dross_current = clean_cost
            .take_units(dross_units, std::iter::empty())
            .map_err(|error| error.to_string())?;
        let mut return_current = payload;
        return_current
            .checked_add(&clean_cost)
            .map_err(|error| error.to_string())?;
        let id = ledger
            .allocate_working_id()
            .map_err(|error| error.to_string())?;
        let transaction = WorkingTransaction {
            id,
            definition: definition.clone(),
            actor,
            actor_label: actor_label.into(),
            source: controller,
            path: {
                let path = effect_path(&effect);
                if path.is_empty() {
                    vec![controller]
                } else {
                    path
                }
            },
            apparatus: WorkingApparatus::Ritual {
                controller,
                expected_revision: controller_revision,
            },
            targets,
            current_debits: vec![CurrentDebit {
                owner: source_owner.clone(),
                expected_version: source_account.version,
                current: reserved.clone(),
            }],
            reserved_current: reserved.clone(),
            physical_debits,
            effect,
            return_current,
            dross_current,
            phase: WorkingPhase::Charging,
            started_tick: tick,
            due_tick: tick.saturating_add(duration_ticks),
            interruption: definition.interruption,
            strain,
            forced: false,
            trace: format!("{working_id} by {actor_label} at {controller:?}"),
            completion_nonce: id,
        };
        transaction.validate().map_err(|error| error.to_string())?;
        let mut next_state = self
            .workings_state
            .clone()
            .ok_or("The world has no workings authority.")?;
        if let WorkingEffect::Settle { process_id, .. } = &transaction.effect {
            let process = next_state
                .active
                .get_mut(process_id)
                .ok_or("The adjacent process ended before the settling rite reserved.")?;
            let remaining = process.due_tick.saturating_sub(tick).max(1);
            let latest = process
                .started_tick
                .saturating_add(process.definition.max_duration_ticks);
            process.due_tick = process
                .due_tick
                .saturating_add(remaining.div_ceil(2))
                .min(latest);
        }
        next_state
            .start(transaction)
            .map_err(|error| error.to_string())?;
        let arcane = super::implements::transaction_from_maps(
            ledger,
            BTreeMap::from([(source_owner, reserved.clone())]),
            BTreeMap::from([(ArcaneOwner::Working(id), reserved)]),
            working_id,
            "constructed ritual reserved exact cost, payload, apparatus, and targets",
        )?;
        ledger
            .commit_linked_files(
                arcane,
                vec![LinkedFileReplacement {
                    subsystem: "workings".into(),
                    operation_id: id,
                    relative_path: crate::workings::WORKINGS_FILE.into(),
                    after: Some(next_state.encode().map_err(|error| error.to_string())?),
                }],
            )
            .map_err(|error| error.to_string())?;
        self.workings_state = Some(next_state);
        Ok(WorkingResult {
            success: true,
            stable_id: id,
            phase: Some(WorkingPhase::Charging),
            cue: if strain.warning_band >= 2 {
                WorkingCueKind::Strain
            } else {
                WorkingCueKind::Settle
            },
            warning_band: strain.warning_band,
            message: format!("{} begins through its physical circle.", definition.label),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn reserve_wand_effect(
        &mut self,
        actor: [u8; 16],
        actor_label: &str,
        source: BlockPos,
        wand_id: u64,
        working_id: &str,
        targets: Vec<WorkingTargetSnapshot>,
        physical_debits: Vec<PhysicalDebit>,
        effect: WorkingEffect,
        magnitude: u32,
        distance: u16,
        duration_ticks: u64,
        forced: bool,
    ) -> Result<WorkingResult, String> {
        let definition = self
            .reg
            .workings
            .get(working_id)
            .cloned()
            .ok_or_else(|| format!("Unknown working {working_id}."))?;
        if definition.mode != DeliveryMode::Wand
            || definition.handler.effect_kind() != effect.kind()
        {
            return Err("The selected content does not own that native wand capability.".into());
        }
        self.validate_wand_line_of_sight(source, &effect, definition.range)?;
        let quote = definition
            .quote(magnitude, distance, duration_ticks)
            .map_err(|error| error.to_string())?;
        let instance = self
            .implements_state
            .as_ref()
            .and_then(|state| state.instance(wand_id))
            .cloned()
            .ok_or("The held wand has no authoritative physical instance.")?;
        let ImplementKind::Wand { resolved, .. } = &instance.kind else {
            return Err("That implement is not a wand.".into());
        };
        if instance.wear >= crate::implements::MAX_WAND_WEAR
            || instance.strain >= crate::implements::MAX_WAND_STRAIN
        {
            return Err("The wand is visibly too damaged or strained to channel safely.".into());
        }
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(source.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let wand_owner = ArcaneOwner::Item(wand_id);
        let wand_usable = self
            .arcane_ledger
            .as_ref()
            .and_then(|ledger| ledger.account(&wand_owner))
            .map_or(0, |account| {
                account
                    .current
                    .total()
                    .saturating_sub(STRUCTURAL_SPARK_UNITS)
            });
        if wand_usable < quote.charge && definition.ambient {
            // A normal draw exports enough local custody to leave the
            // measured floor intact. Explicit forced draw exports only the
            // missing effect charge, so crossing the floor is real, finite,
            // legible, and subsequently priced by deterministic strain.
            self.ensure_regional_ambient_units(
                region,
                quote
                    .charge
                    .saturating_sub(wand_usable)
                    .saturating_add(if forced { 0 } else { AMBIENT_SAFE_FLOOR }),
                "working drew measured local Ambient Current",
            )?;
        }
        let local_capacity_permille = self.local_capacity_permille(region);
        let tick = self.working_tick();
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no finite Current ledger.")?;
        let mut remaining = quote.charge;
        let mut current_debits = Vec::new();
        let mut reserved = Current::default();
        if let Some(account) = ledger.account(&wand_owner).cloned() {
            let amount = remaining.min(
                account
                    .current
                    .total()
                    .saturating_sub(STRUCTURAL_SPARK_UNITS),
            );
            if amount != 0 {
                let mut current = account.current.clone();
                let selected = current
                    .take_units(amount, [definition.focus.clone()])
                    .map_err(|error| error.to_string())?;
                reserved
                    .checked_add(&selected)
                    .map_err(|error| error.to_string())?;
                current_debits.push(CurrentDebit {
                    owner: wand_owner.clone(),
                    expected_version: account.version,
                    current: selected,
                });
                remaining -= amount;
            }
        }
        let ambient_owner = ArcaneOwner::Ambient(region);
        if remaining != 0 && definition.ambient {
            let account = ledger
                .account(&ambient_owner)
                .cloned()
                .ok_or("No measured Ambient Current is available here.")?;
            let safe_available = account.current.total().saturating_sub(AMBIENT_SAFE_FLOOR);
            let amount = if forced {
                remaining.min(account.current.total())
            } else {
                remaining.min(safe_available)
            };
            if amount != 0 {
                let mut current = account.current.clone();
                let selected = current
                    .take_units(amount, [definition.focus.clone()])
                    .map_err(|error| error.to_string())?;
                reserved
                    .checked_add(&selected)
                    .map_err(|error| error.to_string())?;
                current_debits.push(CurrentDebit {
                    owner: ambient_owner.clone(),
                    expected_version: account.version,
                    current: selected,
                });
                remaining -= amount;
            }
        }
        if remaining != 0 {
            return Err(format!(
                "The wand and measured local Current are {remaining} units short."
            ));
        }
        let preferred_weight = resolved
            .resonance
            .get(&definition.focus)
            .copied()
            .unwrap_or_default();
        let total_weight = resolved.resonance.values().copied().max().unwrap_or(1);
        let mismatch = 1_000u16.saturating_sub(
            u16::try_from(
                u32::from(preferred_weight)
                    .saturating_mul(1_000)
                    .checked_div(u32::from(total_weight.max(1)))
                    .unwrap_or_default(),
            )
            .unwrap_or(1_000),
        );
        let over_safe = quote
            .charge
            .saturating_sub(definition.safe_throughput.min(resolved.safe_transfer));
        let below_floor = ledger.account(&ambient_owner).map_or(0, |account| {
            AMBIENT_SAFE_FLOOR.saturating_sub(account.current.total())
        });
        let strain = crate::workings::deterministic_strain(StrainInputs {
            resonance_mismatch_permille: mismatch,
            component_instability_permille: 1_000u16.saturating_sub(resolved.stability),
            throughput: quote.charge,
            safe_throughput: definition
                .safe_throughput
                .min(resolved.safe_transfer)
                .max(1),
            local_capacity_permille,
            below_safe_floor_units: below_floor,
            apparatus_damage_permille: ((u64::from(instance.wear) * 1_000)
                / u64::from(crate::implements::MAX_WAND_WEAR))
                as u16,
            contamination_permille: (instance.strain / 10).min(1_000) as u16,
            interruption: false,
            forced_overdraw_units: u64::from(forced).saturating_mul(over_safe.max(below_floor)),
        })
        .map_err(|error| error.to_string())?;
        if strain.refuses {
            return Err("The forced overdraw exceeds this visibly damaged apparatus.".into());
        }
        let apparatus_dross = quote
            .charge
            .saturating_mul(u64::from(resolved.dross_per_thousand))
            .div_ceil(1_000);
        let dross_units = quote
            .base_dross
            .saturating_add(apparatus_dross)
            .saturating_add(strain.extra_dross)
            .min(reserved.total());
        let mut return_current = reserved.clone();
        let dross_current = return_current
            .take_units(dross_units, std::iter::empty())
            .map_err(|error| error.to_string())?;
        let id = ledger
            .allocate_working_id()
            .map_err(|error| error.to_string())?;
        let transaction = WorkingTransaction {
            id,
            definition: definition.clone(),
            actor,
            actor_label: actor_label.into(),
            source,
            path: {
                let mut path = effect_path(&effect);
                if path.is_empty() {
                    path.push(source);
                    for target in &targets {
                        let pos = match target {
                            WorkingTargetSnapshot::Block { pos, .. }
                            | WorkingTargetSnapshot::Reservoir { pos, .. } => Some(*pos),
                            WorkingTargetSnapshot::Area { controller, .. } => Some(*controller),
                            WorkingTargetSnapshot::Item { .. }
                            | WorkingTargetSnapshot::Entity { .. } => None,
                        };
                        if let Some(pos) = pos
                            && !path.contains(&pos)
                        {
                            path.push(pos);
                        }
                    }
                }
                path
            },
            apparatus: WorkingApparatus::Wand {
                instance_id: wand_id,
                expected_revision: u64::from(instance.wear) << 32 | u64::from(instance.strain),
            },
            targets,
            current_debits: current_debits.clone(),
            reserved_current: reserved.clone(),
            physical_debits,
            effect,
            return_current,
            dross_current,
            phase: WorkingPhase::Charging,
            started_tick: tick,
            due_tick: tick.saturating_add(duration_ticks),
            interruption: definition.interruption,
            strain,
            forced,
            trace: format!("{working_id} by {actor_label} from {source:?}"),
            completion_nonce: id,
        };
        transaction.validate().map_err(|error| error.to_string())?;
        let mut next_state = self
            .workings_state
            .clone()
            .ok_or("The world has no workings authority.")?;
        next_state
            .start(transaction)
            .map_err(|error| error.to_string())?;
        let mut debits = BTreeMap::new();
        for debit in current_debits {
            super::implements::add_current(&mut debits, debit.owner, &debit.current)
                .map_err(|error| error.to_string())?;
        }
        let mut credits = BTreeMap::new();
        super::implements::add_current(&mut credits, ArcaneOwner::Working(id), &reserved)
            .map_err(|error| error.to_string())?;
        let arcane = super::implements::transaction_from_maps(
            ledger,
            debits,
            credits,
            working_id,
            "working reserved exact Current and targets",
        )?;
        let replacement = LinkedFileReplacement {
            subsystem: "workings".into(),
            operation_id: id,
            relative_path: crate::workings::WORKINGS_FILE.into(),
            after: Some(next_state.encode().map_err(|error| error.to_string())?),
        };
        ledger
            .commit_linked_files(arcane, vec![replacement])
            .map_err(|error| error.to_string())?;
        self.workings_state = Some(next_state);
        Ok(WorkingResult {
            success: true,
            stable_id: id,
            phase: Some(WorkingPhase::Charging),
            cue: if strain.warning_band >= 2 {
                WorkingCueKind::Strain
            } else {
                WorkingCueKind::Settle
            },
            warning_band: strain.warning_band,
            message: format!("{} begins to settle.", definition.label),
        })
    }

    fn settle_working(
        &mut self,
        id: u64,
        settlement: Settlement,
        mut inventory: Option<&mut crate::inventory::Inventory>,
    ) -> Result<WorkingResult, String> {
        if settlement == Settlement::Complete {
            self.refresh_nudge_velocity(id)?;
        }
        let transaction = self
            .workings_state
            .as_ref()
            .and_then(|state| state.active.get(&id))
            .cloned()
            .ok_or_else(|| format!("Working {id} is not active."))?;
        if transaction.phase == WorkingPhase::PendingApply {
            if matches!(transaction.effect, WorkingEffect::RepairItem { .. }) {
                let inventory = inventory
                    .as_deref_mut()
                    .ok_or("Fieldmend is waiting for its authoritative player profile.")?;
                self.apply_inventory_effect(&transaction, inventory)?;
                return Ok(WorkingResult {
                    success: true,
                    stable_id: id,
                    phase: Some(WorkingPhase::PendingApply),
                    cue: WorkingCueKind::Complete,
                    warning_band: transaction.strain.warning_band,
                    message: format!(
                        "{} has landed; its profile checkpoint is pending.",
                        transaction.definition.label
                    ),
                });
            }
            self.apply_working_effect(&transaction.effect, &transaction.targets)?;
            self.save_effect_chunks(&transaction.effect)?;
            self.save_effect_sidecars(&transaction.effect)?;
            let tick = self.working_tick();
            let state = self
                .workings_state
                .as_mut()
                .expect("transaction came from state");
            state
                .settle(id, "completion_retried", tick)
                .map_err(|error| error.to_string())?;
            state.save().map_err(|error| error.to_string())?;
            return Ok(WorkingResult {
                success: true,
                stable_id: id,
                phase: None,
                cue: WorkingCueKind::Complete,
                warning_band: transaction.strain.warning_band,
                message: format!("{} completes exactly once.", transaction.definition.label),
            });
        }
        if settlement == Settlement::Complete {
            if transaction.definition.mode == DeliveryMode::Ritual {
                self.validate_ritual_apparatus(&transaction)?;
            }
            if matches!(transaction.effect, WorkingEffect::RepairItem { .. }) {
                let inventory = inventory
                    .as_deref_mut()
                    .ok_or("Fieldmend must complete through an authoritative inventory adapter.")?;
                self.validate_inventory_effect(&transaction, inventory)?;
            } else {
                self.validate_working_effect_before(&transaction.effect)?;
            }
        }
        let region = self
            .planet_atlas
            .as_ref()
            .map(|atlas| atlas.atlas_pos(transaction.source.surface()))
            .ok_or("The finite Current atlas is unavailable.")?;
        let mut next_state = self
            .workings_state
            .clone()
            .ok_or("The world has no workings authority.")?;
        let mut settled = next_state
            .active
            .get(&id)
            .cloned()
            .expect("cloned active transaction");
        if settlement != Settlement::Complete {
            let extra = crate::workings::deterministic_strain(StrainInputs {
                safe_throughput: settled.definition.safe_throughput.max(1),
                local_capacity_permille: 1_000,
                interruption: true,
                ..StrainInputs::default()
            })
            .map_err(|error| error.to_string())?;
            let convert = extra.extra_dross.min(settled.return_current.total());
            if convert != 0 {
                let moved = settled
                    .return_current
                    .take_units(convert, std::iter::empty())
                    .map_err(|error| error.to_string())?;
                settled
                    .dross_current
                    .checked_add(&moved)
                    .map_err(|error| error.to_string())?;
            }
            settled.strain.strain = settled.strain.strain.saturating_add(extra.strain);
            settled.strain.extra_dross =
                settled.strain.extra_dross.saturating_add(extra.extra_dross);
            settled.strain.warning_band = settled.strain.warning_band.max(extra.warning_band);
            next_state.active.insert(id, settled.clone());
        }
        let settlement_tick = self.working_tick();
        let staged_material = if settlement == Settlement::Complete
            && matches!(settled.effect, WorkingEffect::RepairItem { .. })
        {
            self.stage_fieldmend_material_loss(&settled.effect)?
        } else {
            None
        };
        let mut settling_destination = None;
        let mut settling_wear = None;
        if settlement == Settlement::Complete
            && settled.definition.handler != WorkingHandler::SettlingRite
        {
            let rite = next_state
                .active
                .values()
                .filter(|candidate| {
                    candidate.phase == WorkingPhase::Active
                        && matches!(
                            candidate.effect,
                            WorkingEffect::Settle { process_id, .. } if process_id == id
                        )
                        && candidate.targets.iter().any(|target| {
                            matches!(
                                target,
                                WorkingTargetSnapshot::Entity {
                                    stable_id,
                                    kind,
                                    version,
                                } if *stable_id == id
                                    && kind == "magical_process"
                                    && *version == settled.completion_nonce
                            )
                        })
                })
                .min_by_key(|candidate| candidate.id)
                .cloned();
            if let Some(rite) = rite {
                let WorkingEffect::Settle {
                    dross_vessel_id, ..
                } = &rite.effect
                else {
                    unreachable!("settling candidate was matched above")
                };
                let dross_vessel_id = *dross_vessel_id;
                let vessel_capacity = self
                    .implements_state
                    .as_ref()
                    .and_then(|state| state.instance(dross_vessel_id))
                    .map(|instance| instance.kind.capacity())
                    .unwrap_or_default();
                let vessel_load = self
                    .arcane_ledger
                    .as_ref()
                    .and_then(|ledger| ledger.account(&ArcaneOwner::Item(dross_vessel_id)))
                    .map_or(0, |account| account.current.total())
                    .saturating_add(
                        self.arcane_ledger
                            .as_ref()
                            .and_then(|ledger| {
                                ledger.account(&ArcaneOwner::ItemDross(dross_vessel_id))
                            })
                            .map_or(0, |account| account.current.total()),
                    );
                if self.validate_ritual_apparatus(&rite).is_ok()
                    && vessel_load.saturating_add(settled.dross_current.total()) <= vessel_capacity
                {
                    // Stabilization trades throughput for a deterministic 25%
                    // reduction in disordered output. The recovered mixture
                    // remains clean return Current; every remaining unit is
                    // credited to the real mounted dross vessel below.
                    let recovered_units = settled.dross_current.total() / 4;
                    if recovered_units != 0 {
                        let recovered = settled
                            .dross_current
                            .take_units(recovered_units, std::iter::empty())
                            .map_err(|error| error.to_string())?;
                        settled
                            .return_current
                            .checked_add(&recovered)
                            .map_err(|error| error.to_string())?;
                    }
                    let routed = settled.dross_current.total();
                    settling_destination = Some(ArcaneOwner::ItemDross(dross_vessel_id));
                    let wear = u16::try_from(routed.div_ceil(16)).unwrap_or(u16::MAX);
                    settling_wear = Some((dross_vessel_id, u32::from(wear)));
                    if let Some(active_rite) = next_state.active.get_mut(&rite.id)
                        && let WorkingEffect::Settle {
                            dross_routed,
                            stabilizer_wear,
                            ..
                        } = &mut active_rite.effect
                    {
                        *dross_routed = dross_routed.saturating_add(routed);
                        *stabilizer_wear = stabilizer_wear.saturating_add(wear);
                    }
                    next_state.active.insert(id, settled.clone());
                } else if let Some(active_rite) = next_state.active.get_mut(&rite.id) {
                    active_rite.strain.warning_band = 3;
                    active_rite.strain.strain = active_rite.strain.strain.saturating_add(250);
                }
            }
        }
        let staged_loose_items = if settlement == Settlement::Complete
            && matches!(
                settled.effect,
                WorkingEffect::Impulse {
                    entity_kind: NudgeEntityKind::DroppedItem,
                    ..
                }
            ) {
            let after = self
                .encode_loose_items()
                .map_err(|error| error.to_string())?;
            let path = self.save_dir.join("loose-items.toml");
            let before = match std::fs::read(path) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
                Err(error) => return Err(error.to_string()),
            };
            (before != after).then_some(after)
        } else {
            None
        };
        let ledger = self
            .arcane_ledger
            .as_mut()
            .ok_or("The world has no finite Current ledger.")?;
        let mut debits = BTreeMap::new();
        super::implements::add_current(
            &mut debits,
            ArcaneOwner::Working(id),
            &settled.reserved_current,
        )
        .map_err(|error| error.to_string())?;
        let mut credits = BTreeMap::new();
        let mut ordinary_return = settled.return_current.clone();
        let preserve_spent_units = match settled.effect {
            WorkingEffect::Preserve {
                charge_spent_units, ..
            } => charge_spent_units.min(ordinary_return.total()),
            _ => 0,
        };
        if preserve_spent_units != 0 {
            let spent = ordinary_return
                .take_units(preserve_spent_units, [settled.definition.focus.clone()])
                .map_err(|error| error.to_string())?;
            super::implements::add_current(&mut credits, ArcaneOwner::Ambient(region), &spent)
                .map_err(|error| error.to_string())?;
        }
        if settlement == Settlement::Complete
            && let WorkingEffect::TransferCurrent { to, current, .. } = &settled.effect
        {
            ordinary_return
                .checked_sub(current)
                .map_err(|error| error.to_string())?;
            super::implements::add_current(&mut credits, to.clone(), current)
                .map_err(|error| error.to_string())?;
        }
        if !ordinary_return.is_empty() {
            let refunds_original_sources = settlement != Settlement::Complete
                || matches!(settled.effect, WorkingEffect::Preserve { .. });
            if refunds_original_sources {
                // Refund only into the accounts that actually supplied the
                // reservation, bounded by each exact debit. This prevents an
                // interrupted ambient-assisted channel from overfilling the
                // wand while preserving the conserved resonance mixture.
                for debit in &settled.current_debits {
                    let amount = ordinary_return.total().min(debit.current.total());
                    if amount == 0 {
                        break;
                    }
                    let refunded = ordinary_return
                        .take_units(amount, debit.current.parts().keys().cloned())
                        .map_err(|error| error.to_string())?;
                    super::implements::add_current(&mut credits, debit.owner.clone(), &refunded)
                        .map_err(|error| error.to_string())?;
                }
            } else {
                super::implements::add_current(
                    &mut credits,
                    ArcaneOwner::Ambient(region),
                    &ordinary_return,
                )
                .map_err(|error| error.to_string())?;
                ordinary_return = Current::default();
            }
            if !ordinary_return.is_empty() {
                super::implements::add_current(
                    &mut credits,
                    ArcaneOwner::Ambient(region),
                    &ordinary_return,
                )
                .map_err(|error| error.to_string())?;
            }
        }
        if !settled.dross_current.is_empty() {
            super::implements::add_current(
                &mut credits,
                settling_destination.unwrap_or(ArcaneOwner::Dross {
                    region,
                    medium: dross_medium(settled.definition.handler),
                }),
                &settled.dross_current,
            )
            .map_err(|error| error.to_string())?;
        }
        let mut next_implements = self
            .implements_state
            .clone()
            .ok_or("The world has no implement authority.")?;
        let operation_id = next_implements
            .operation_id()
            .map_err(|error| error.to_string())?;
        if let WorkingApparatus::Wand { instance_id, .. } = settled.apparatus
            && let Some(instance) = next_implements.instances.get_mut(&instance_id)
        {
            instance.wear = instance
                .wear
                .saturating_add(u32::from(settled.definition.wear))
                .min(crate::implements::MAX_WAND_WEAR);
            instance.strain = instance
                .strain
                .saturating_add(settled.strain.strain)
                .min(crate::implements::MAX_WAND_STRAIN);
        }
        let ritual_instance_id = if matches!(settled.apparatus, WorkingApparatus::Ritual { .. }) {
            settled
                .current_debits
                .iter()
                .find_map(|debit| match debit.owner {
                    ArcaneOwner::Item(instance_id) => Some(instance_id),
                    _ => None,
                })
        } else {
            None
        };
        if let Some(instance_id) = ritual_instance_id
            && let Some(instance) = next_implements.instances.get_mut(&instance_id)
        {
            instance.wear = instance
                .wear
                .saturating_add(u32::from(settled.definition.wear))
                .min(crate::implements::MAX_WAND_WEAR);
            instance.strain = instance
                .strain
                .saturating_add(settled.strain.strain / 2)
                .min(crate::implements::MAX_WAND_STRAIN);
        }
        if let Some((vessel_id, wear)) = settling_wear
            && let Some(instance) = next_implements.instances.get_mut(&vessel_id)
        {
            // The dross vessel is the rite's real stabilizer. Its embodied
            // implement record wears in the same linked commit that granted
            // the target process its reduced/routed dross, so a crash cannot
            // keep the benefit while losing the physical cost.
            instance.wear = instance
                .wear
                .saturating_add(wear)
                .min(crate::implements::MAX_WAND_WEAR);
            instance.strain = instance
                .strain
                .saturating_add(wear.saturating_mul(8))
                .min(crate::implements::MAX_WAND_STRAIN);
        }
        next_implements.record(ImplementAuditEvent {
            operation_id,
            kind: format!("working_{settlement:?}").to_ascii_lowercase(),
            instance_id: match settled.apparatus {
                WorkingApparatus::Wand { instance_id, .. } => instance_id,
                WorkingApparatus::Ritual { .. } => ritual_instance_id.unwrap_or_default(),
            },
            units: settled.reserved_current.total(),
            dross: settled.dross_current.total(),
            actor: settled.actor_label.clone(),
            note: settled.trace.clone(),
        });
        if settlement == Settlement::Complete {
            next_state
                .phase(id, WorkingPhase::PendingApply)
                .map_err(|error| error.to_string())?;
        } else {
            next_state
                .settle(
                    id,
                    if settlement == Settlement::Cancel {
                        "cancelled"
                    } else {
                        "interrupted"
                    },
                    settlement_tick,
                )
                .map_err(|error| error.to_string())?;
        }
        let arcane = super::implements::transaction_from_maps(
            ledger,
            debits,
            credits,
            &settled.definition.id,
            if settlement == Settlement::Complete {
                "working settled Current before typed effect replay"
            } else {
                "working interruption settled its declared refund and dross"
            },
        )?;
        let mut replacements = vec![
            LinkedFileReplacement {
                subsystem: "workings".into(),
                operation_id,
                relative_path: crate::workings::WORKINGS_FILE.into(),
                after: Some(next_state.encode().map_err(|error| error.to_string())?),
            },
            LinkedFileReplacement {
                subsystem: "implements".into(),
                operation_id,
                relative_path: crate::implements::IMPLEMENTS_FILE.into(),
                after: Some(
                    next_implements
                        .encode()
                        .map_err(|error| error.to_string())?,
                ),
            },
        ];
        if let Some((_, bytes)) = &staged_material {
            replacements.push(LinkedFileReplacement {
                subsystem: "materials".into(),
                operation_id,
                relative_path: crate::materials::MaterialLedger::linked_delta_path().into(),
                after: Some(bytes.clone()),
            });
        }
        if let Some(staged_loose_items) = staged_loose_items {
            // The PendingApply journal and the exact loose-entity before
            // state land together. A crash can therefore replay the impulse
            // once instead of losing the transient target or guessing.
            replacements.push(LinkedFileReplacement {
                subsystem: "loose_items".into(),
                operation_id,
                relative_path: "loose-items.toml".into(),
                after: Some(staged_loose_items),
            });
        }
        ledger
            .commit_linked_files(arcane, replacements)
            .map_err(|error| error.to_string())?;
        self.workings_state = Some(next_state);
        self.implements_state = Some(next_implements);
        if let Some((next_material, _)) = staged_material {
            self.material_ledger = Some(next_material);
        }
        let inventory_pending = settlement == Settlement::Complete
            && matches!(settled.effect, WorkingEffect::RepairItem { .. });
        if inventory_pending {
            let inventory = inventory.expect("external working was validated with an inventory");
            self.apply_inventory_effect(&settled, inventory)?;
        } else if settlement == Settlement::Complete {
            self.apply_working_effect(&settled.effect, &settled.targets)?;
            self.save_effect_chunks(&settled.effect)?;
            self.save_effect_sidecars(&settled.effect)?;
            let tick = self.working_tick();
            let state = self
                .workings_state
                .as_mut()
                .expect("pending state was installed");
            state
                .settle(id, "completed", tick)
                .map_err(|error| error.to_string())?;
            state.save().map_err(|error| error.to_string())?;
        }
        Ok(WorkingResult {
            success: true,
            stable_id: id,
            phase: inventory_pending.then_some(WorkingPhase::PendingApply),
            cue: match settlement {
                Settlement::Complete => WorkingCueKind::Complete,
                Settlement::Cancel => WorkingCueKind::Cancel,
                Settlement::Interrupt => WorkingCueKind::Strain,
            },
            warning_band: settled.strain.warning_band,
            message: match settlement {
                Settlement::Complete if inventory_pending => format!(
                    "{} has landed; saving the authoritative profile finishes it.",
                    settled.definition.label
                ),
                Settlement::Complete => format!("{} completes.", settled.definition.label),
                Settlement::Cancel => format!(
                    "{} is cancelled and settles visibly.",
                    settled.definition.label
                ),
                Settlement::Interrupt => {
                    format!("{} breaks with accounted dross.", settled.definition.label)
                }
            },
        })
    }

    fn validate_inventory_effect(
        &self,
        transaction: &WorkingTransaction,
        inventory: &crate::inventory::Inventory,
    ) -> Result<(), String> {
        let WorkingEffect::RepairItem {
            item_name,
            before_durability,
            after_durability,
            repair_material,
            residue_item,
            ..
        } = &transaction.effect
        else {
            return Err("That effect has no inventory adapter.".into());
        };
        let (target_slot, material_slot) = repair_inventory_slots(transaction)?;
        let target = inventory.slots[target_slot];
        let material = inventory.slots[material_slot];
        let target_item = self
            .reg
            .item_id(item_name)
            .ok_or("Fieldmend's saved target content is unavailable.")?;
        let repair_item = self
            .reg
            .item_id(repair_material)
            .ok_or("Fieldmend's matching material content is unavailable.")?;
        let residue = self
            .reg
            .item_id(residue_item)
            .ok_or("Fieldmend's ordinary residue content is unavailable.")?;
        let before = target.is_some_and(|stack| {
            stack.item == target_item
                && stack.count == 1
                && stack.arcane_id == 0
                && stack.durability == *before_durability
        }) && material.is_some_and(|stack| {
            stack.item == repair_item && stack.count == 1 && stack.arcane_id == 0
        });
        let after = target.is_some_and(|stack| {
            stack.item == target_item
                && stack.count == 1
                && stack.arcane_id == 0
                && stack.durability == *after_durability
        }) && material
            .is_some_and(|stack| stack.item == residue && stack.count == 1 && stack.arcane_id == 0);
        if before || after {
            Ok(())
        } else {
            Err("Fieldmend found neither its exact before nor after inventory state.".into())
        }
    }

    fn validate_saved_world_targets(&self, transaction: &WorkingTransaction) -> Result<(), String> {
        for target in &transaction.targets {
            match target {
                WorkingTargetSnapshot::Block {
                    pos,
                    block_name,
                    metadata,
                    ..
                } => {
                    if self.reg.block(self.get_block_at(*pos)).name != *block_name
                        || self.get_meta_at(*pos) != *metadata
                    {
                        return Err(format!(
                            "A reserved ritual block at {pos:?} changed before completion."
                        ));
                    }
                }
                WorkingTargetSnapshot::Reservoir {
                    pos,
                    water_hu,
                    salt_mass,
                    thermal_millic_hu,
                    dross_units,
                    carrier_remainder,
                    ..
                } => {
                    let actual = self.water_mass_at(*pos).unwrap_or_default();
                    let expected_carrier = crate::workings::WaterCarrier {
                        thermal_millic_hu: *thermal_millic_hu,
                        dross_subunits: dross_units
                            .checked_mul(256)
                            .and_then(|units| units.checked_add(*carrier_remainder))
                            .ok_or("A ritual reservoir carrier snapshot overflowed.")?,
                    };
                    if actual.water_hu != *water_hu
                        || actual.salt_mass != *salt_mass
                        || (transaction.definition.handler == WorkingHandler::Draw
                            && self.water_carrier_at(*pos, actual) != expected_carrier)
                    {
                        return Err(format!(
                            "A reserved ritual reservoir at {pos:?} changed before completion."
                        ));
                    }
                }
                WorkingTargetSnapshot::Area { .. }
                | WorkingTargetSnapshot::Item { .. }
                | WorkingTargetSnapshot::Entity { .. } => {}
            }
        }
        Ok(())
    }

    fn validate_ritual_apparatus(&self, transaction: &WorkingTransaction) -> Result<(), String> {
        let WorkingApparatus::Ritual {
            controller,
            expected_revision,
        } = transaction.apparatus
        else {
            return Ok(());
        };
        if self.binding_frame_revision(controller)? != expected_revision {
            return Err("The ritual controller changed while Current was reserved.".into());
        }
        let layout = self.binding_frame_layout(controller);
        if !layout.valid {
            return Err(format!(
                "The ritual's visible containment path is broken: {}",
                layout.problems.join(" ")
            ));
        }
        let area_cells = transaction.targets.iter().find_map(|target| match target {
            WorkingTargetSnapshot::Area {
                controller: at,
                cells,
                ..
            } if *at == controller => Some(cells.as_slice()),
            _ => None,
        });
        for target in &transaction.targets {
            let WorkingTargetSnapshot::Item {
                stable_id,
                durability,
                version,
                ..
            } = target
            else {
                continue;
            };
            let present = area_cells.into_iter().flatten().any(|pos| {
                matches!(
                    self.block_entity_at(pos),
                    Some(BlockEntity::ChargeVessel(vessel))
                        if vessel.revision == *version
                            && vessel.vessel.is_some_and(|stack| {
                                stack.arcane_id == *stable_id && stack.durability == *durability
                            })
                )
            });
            if !present {
                return Err(
                    "A ritual vessel was moved, damaged, or replaced before completion.".into(),
                );
            }
        }
        Ok(())
    }

    fn apply_inventory_effect(
        &self,
        transaction: &WorkingTransaction,
        inventory: &mut crate::inventory::Inventory,
    ) -> Result<(), String> {
        self.validate_inventory_effect(transaction, inventory)?;
        let WorkingEffect::RepairItem {
            item_name,
            before_durability,
            after_durability,
            repair_material,
            residue_item,
            residue_units,
            ..
        } = &transaction.effect
        else {
            return Err("That effect has no inventory adapter.".into());
        };
        let (target_slot, material_slot) = repair_inventory_slots(transaction)?;
        let target_item = self
            .reg
            .item_id(item_name)
            .ok_or("Fieldmend's saved target content is unavailable.")?;
        let repair_item = self
            .reg
            .item_id(repair_material)
            .ok_or("Fieldmend's matching material content is unavailable.")?;
        let residue = self
            .reg
            .item_id(residue_item)
            .ok_or("Fieldmend's ordinary residue content is unavailable.")?;
        let already_after = inventory.slots[target_slot].is_some_and(|stack| {
            stack.item == target_item && stack.durability == *after_durability
        }) && inventory.slots[material_slot]
            .is_some_and(|stack| stack.item == residue && stack.count == *residue_units);
        if already_after {
            return Ok(());
        }
        if !inventory.slots[target_slot].is_some_and(|stack| {
            stack.item == target_item && stack.durability == *before_durability
        }) || !inventory.slots[material_slot]
            .is_some_and(|stack| stack.item == repair_item && stack.count == 1)
        {
            return Err("Fieldmend target changed during its write-ahead apply.".into());
        }
        inventory.slots[target_slot]
            .as_mut()
            .expect("validated target slot")
            .durability = *after_durability;
        let mut residue_stack =
            crate::inventory::ItemStack::new(&self.reg, residue, *residue_units);
        residue_stack.count = *residue_units;
        inventory.slots[material_slot] = Some(residue_stack);
        Ok(())
    }

    fn stage_fieldmend_material_loss(
        &self,
        effect: &WorkingEffect,
    ) -> Result<Option<(crate::materials::MaterialLedger, Vec<u8>)>, String> {
        let WorkingEffect::RepairItem {
            repair_material,
            residue_item,
            residue_units,
            ..
        } = effect
        else {
            return Ok(None);
        };
        let repair = self
            .reg
            .item_id(repair_material)
            .ok_or("Fieldmend matching material disappeared.")?;
        let residue = self
            .reg
            .item_id(residue_item)
            .ok_or("Fieldmend residue disappeared.")?;
        let input = &self.reg.item(repair).materials;
        let output = &self.reg.item(residue).materials;
        let mut loss = crate::registry::MaterialVector::new();
        for (material, output_units) in output {
            let available = input.get(material).copied().unwrap_or_default();
            let required = output_units.saturating_mul(u64::from(*residue_units));
            if required > available {
                return Err("Fieldmend residue would transmute or create ordinary matter.".into());
            }
        }
        for (material, input_units) in input {
            let output_units = output
                .get(material)
                .copied()
                .unwrap_or_default()
                .saturating_mul(u64::from(*residue_units));
            if *input_units > output_units {
                loss.insert(material.clone(), input_units - output_units);
            }
        }
        self.material_ledger
            .as_ref()
            .ok_or("The finite material ledger is unavailable.")?
            .stage_linked_recipe_loss(&loss)
            .map_err(|error| error.to_string())
    }

    fn validate_kindle_target(&self, fuel: BlockPos, fire_cell: BlockPos) -> Result<(), String> {
        if !crate::planet::neighbors6(fuel).any(|neighbor| neighbor == fire_cell)
            || self.reg.block(self.get_block_at(fuel)).burns == 0
            || !self.reg.is_replaceable(self.get_block_at(fire_cell))
        {
            return Err(
                "Kindle needs one adjacent dry combustible and a replaceable fire cell.".into(),
            );
        }
        let weather = self.weather_at_surface(fuel.surface());
        if weather.kind.precipitating() && self.light_at_pos(fuel).1 == 15 {
            return Err("The exposed fuel is too wet to take ordinary fire.".into());
        }
        if crate::planet::neighbors6(fuel)
            .any(|neighbor| self.reg.is_water(self.get_block_at(neighbor)))
        {
            return Err("Waterlogged fuel refuses Kindle.".into());
        }
        Ok(())
    }

    fn validate_wand_line_of_sight(
        &self,
        source: BlockPos,
        effect: &WorkingEffect,
        range: u16,
    ) -> Result<(), String> {
        let target = match effect {
            WorkingEffect::Observe { origin, .. } => Some(*origin),
            WorkingEffect::PointLight { target, .. } => Some(*target),
            WorkingEffect::Ignite { fuel, .. } => Some(*fuel),
            WorkingEffect::Impulse { target, .. }
            | WorkingEffect::OperateMechanism { target, .. } => Some(*target),
            WorkingEffect::AdvancePlant(advance) => Some(advance.pos),
            WorkingEffect::TransferWater { from, .. } => Some(*from),
            WorkingEffect::RepairItem { .. } | WorkingEffect::Preserve { .. } => None,
            WorkingEffect::Settle { .. }
            | WorkingEffect::AdvanceBed { .. }
            | WorkingEffect::Ward { .. }
            | WorkingEffect::TransferCurrent { .. } => {
                return Err("A ritual effect cannot be channeled through a held wand.".into());
            }
        };
        let Some(target) = target else {
            return Ok(());
        };
        if target == source {
            return Ok(());
        }
        let origin = source.entity_center();
        let delta = origin.local_delta_to(target.entity_center());
        let distance = delta.length();
        if !distance.is_finite() || distance > f32::from(range) + 0.75 {
            return Err("The target is outside this working's bounded reach.".into());
        }
        if crate::raycast::raycast_at(self, origin, delta, (distance - 0.65).max(0.0)).is_some() {
            return Err("Solid terrain breaks the wand's line of sight.".into());
        }
        Ok(())
    }

    /// Revalidate a live wand channel from the actor's current physical cell.
    /// Inventory workings move with their carried targets; spatial workings
    /// end when the actor leaves bounded reach or a wall breaks the path.
    pub fn wand_working_reachable_from(&self, id: u64, source: BlockPos) -> bool {
        let Some(transaction) = self
            .workings_state
            .as_ref()
            .and_then(|state| state.active.get(&id))
        else {
            return false;
        };
        if !matches!(transaction.apparatus, WorkingApparatus::Wand { .. }) {
            return true;
        }
        self.validate_wand_line_of_sight(source, &transaction.effect, transaction.definition.range)
            .is_ok()
    }

    fn prepare_rootwake(
        &self,
        plant: BlockPos,
    ) -> Result<(PlantAdvance, Vec<WorkingTargetSnapshot>), String> {
        let before = self.get_block_at(plant);
        let definition = self.reg.block(before);
        if definition.sapling.is_some() {
            return self.prepare_sapling_rootwake(plant);
        }
        let next = definition
            .crop_next
            .ok_or("Rootwake supports only a declared growing plant, crop, or sapling.")?;
        let soil_pos = plant
            .offset(0, -1, 0)
            .ok_or("The plant has no supporting soil cell.")?;
        let farmland = self.reg.block_id("base:farmland");
        if !definition.crop_any_soil && Some(self.get_block_at(soil_pos)) != farmland {
            return Err("The crop is not rooted in prepared soil.".into());
        }
        if definition.crop_any_soil && !self.heart_alive_at_surface(plant.surface()) {
            return Err("A dead heart remains authoritative over wild biological renewal.".into());
        }
        let season = self.season_at_surface(plant.surface());
        let (block_light, sky_light) = self.light_at_pos(plant);
        let protected = block_light >= 10
            && (sky_light < 15
                || (1..=16)
                    .filter_map(|dy| plant.offset(0, dy, 0))
                    .any(|pos| self.reg.block(self.get_block_at(pos)).glass));
        if !definition.crop_any_soil && season == 3 && !protected {
            return Err("Winter stops this crop outside a lit greenhouse.".into());
        }
        if definition.crop_any_soil && !matches!(season, 1 | 2) {
            return Err("This wild plant is outside its fruiting season.".into());
        }
        let effective_temperature = self.weather_at_surface(plant.surface()).temperature_c
            + if protected { 10.0 } else { 0.0 };
        if !(0.0..42.0).contains(&effective_temperature) {
            return Err("The habitat temperature cannot support this growth interval.".into());
        }
        if block_light.max(sky_light) < 9 {
            return Err("The plant does not have enough ordinary light.".into());
        }
        if !definition.crop_any_soil {
            if let Some(failure) = self.soil_failure_at(soil_pos) {
                return Err(format!("The soil refuses growth: {failure}."));
            }
            if self.fertility_at_pos(soil_pos) < ROOTWAKE_NUTRIENT_UNITS as u8 {
                return Err("The prepared soil has no nutrient budget left.".into());
            }
        }
        let (water_source, water_before) = crate::planet::neighbors6(plant)
            .filter_map(|pos| self.water_mass_at(pos).map(|mass| (pos, mass)))
            .filter(|(_, mass)| mass.water_hu >= ROOTWAKE_WATER_HU)
            .min_by_key(|(pos, _)| *pos)
            .ok_or("Rootwake needs a real adjacent water reservoir to debit.")?;
        let mut water_after = water_before;
        let parcel = water_after.take(ROOTWAKE_WATER_HU);
        let before_soil_meta = self.get_meta_at(soil_pos);
        let after_soil_meta = if definition.crop_any_soil {
            before_soil_meta
        } else {
            soil::soil_meta(
                soil::fert_of(before_soil_meta).saturating_sub(ROOTWAKE_NUTRIENT_UNITS as u8),
                soil::family_of(before_soil_meta),
            )
        };
        let advance = PlantAdvance {
            pos: plant,
            before_block: before.0,
            after_block: next.0,
            before_meta: self.get_meta_at(plant),
            after_meta: self.get_meta_at(plant),
            soil_pos: Some(soil_pos),
            before_soil_meta,
            after_soil_meta,
            water_source: Some(water_source),
            water_before_hu: water_before.water_hu,
            water_after_hu: water_after.water_hu,
            salt_before: water_before.salt_mass,
            salt_after: water_after.salt_mass,
            water_hu: parcel.water_hu,
            nutrient_units: if definition.crop_any_soil {
                0
            } else {
                ROOTWAKE_NUTRIENT_UNITS
            },
        };
        let targets = vec![
            self.block_snapshot(plant),
            self.block_snapshot(soil_pos),
            self.reservoir_snapshot(water_source, water_before),
        ];
        Ok((advance, targets))
    }

    fn prepare_sapling_rootwake(
        &self,
        plant: BlockPos,
    ) -> Result<(PlantAdvance, Vec<WorkingTargetSnapshot>), String> {
        let before = self.get_block_at(plant);
        let definition = self.reg.block(before);
        if definition.sapling.is_none() {
            return Err("Rootwake target is not a declared sapling.".into());
        }
        let before_meta = self.get_meta_at(plant);
        if before_meta & 1 != 0 {
            return Err(
                "This sapling has already received its bounded accelerated interval.".into(),
            );
        }
        let soil_pos = plant
            .offset(0, -1, 0)
            .ok_or("The sapling has no supporting prepared soil.")?;
        if self
            .reg
            .block(self.get_block_at(soil_pos))
            .fert_tiles
            .is_none()
        {
            return Err(
                "Rootwake needs the sapling rooted in prepared finite-nutrient soil.".into(),
            );
        }
        if let Some(failure) = self.soil_failure_at(soil_pos) {
            return Err(format!("The sapling's soil refuses growth: {failure}."));
        }
        if self.fertility_at_pos(soil_pos) < ROOTWAKE_NUTRIENT_UNITS as u8 {
            return Err("The sapling's prepared soil has no nutrient budget left.".into());
        }
        let (block_light, sky_light) = self.light_at_pos(plant);
        if block_light.max(sky_light) < 9 {
            return Err("The sapling does not have enough ordinary light.".into());
        }
        let temperature = self.weather_at_surface(plant.surface()).temperature_c;
        if !(0.0..42.0).contains(&temperature) {
            return Err("The habitat temperature cannot support this sapling interval.".into());
        }
        if (1..=2)
            .filter_map(|dy| plant.offset(0, dy, 0))
            .any(|pos| !self.reg.is_replaceable(self.get_block_at(pos)))
        {
            return Err("The sapling lacks even the bounded space for its next interval.".into());
        }
        let (water_source, water_before) = crate::planet::neighbors6(plant)
            .filter_map(|pos| self.water_mass_at(pos).map(|mass| (pos, mass)))
            .filter(|(_, mass)| mass.water_hu >= ROOTWAKE_WATER_HU)
            .min_by_key(|(pos, _)| *pos)
            .ok_or("Rootwake needs a real adjacent water reservoir to debit.")?;
        let mut water_after = water_before;
        let parcel = water_after.take(ROOTWAKE_WATER_HU);
        let before_soil_meta = self.get_meta_at(soil_pos);
        let after_soil_meta = soil::soil_meta(
            soil::fert_of(before_soil_meta).saturating_sub(ROOTWAKE_NUTRIENT_UNITS as u8),
            soil::family_of(before_soil_meta),
        );
        let advance = PlantAdvance {
            pos: plant,
            before_block: before.0,
            after_block: before.0,
            before_meta,
            after_meta: before_meta | 1,
            soil_pos: Some(soil_pos),
            before_soil_meta,
            after_soil_meta,
            water_source: Some(water_source),
            water_before_hu: water_before.water_hu,
            water_after_hu: water_after.water_hu,
            salt_before: water_before.salt_mass,
            salt_after: water_after.salt_mass,
            water_hu: parcel.water_hu,
            nutrient_units: ROOTWAKE_NUTRIENT_UNITS,
        };
        Ok((
            advance,
            vec![
                self.block_snapshot(plant),
                self.block_snapshot(soil_pos),
                self.reservoir_snapshot(water_source, water_before),
            ],
        ))
    }

    fn validate_working_effect_before(&self, effect: &WorkingEffect) -> Result<(), String> {
        match effect {
            WorkingEffect::Observe { .. }
            | WorkingEffect::PointLight { .. }
            | WorkingEffect::Preserve { .. }
            | WorkingEffect::TransferCurrent { .. } => Ok(()),
            WorkingEffect::Ignite {
                fuel,
                fire_cell,
                expected_fuel,
                expected_air,
                ..
            } => {
                if self.get_block_at(*fuel).0 != *expected_fuel
                    || self.get_block_at(*fire_cell).0 != *expected_air
                {
                    return Err(
                        "Kindle target changed before release; the working remains reserved."
                            .into(),
                    );
                }
                self.validate_kindle_target(*fuel, *fire_cell)
            }
            WorkingEffect::AdvancePlant(advance) => {
                if self.get_block_at(advance.pos).0 != advance.before_block
                    || self.get_meta_at(advance.pos) != advance.before_meta
                    || advance
                        .soil_pos
                        .is_some_and(|soil| self.get_meta_at(soil) != advance.before_soil_meta)
                    || advance.water_source.is_some_and(|water| {
                        self.water_mass_at(water).is_none_or(|mass| {
                            mass.water_hu != advance.water_before_hu
                                || mass.salt_mass != advance.salt_before
                        })
                    })
                {
                    return Err(
                        "Rootwake target or reserved biological inputs changed before release."
                            .into(),
                    );
                }
                Ok(())
            }
            WorkingEffect::TransferWater { from, to, .. } => {
                let snapshots = self
                    .workings_state
                    .as_ref()
                    .into_iter()
                    .flat_map(|state| state.active.values())
                    .find(|transaction| &transaction.effect == effect)
                    .map(|transaction| &transaction.targets)
                    .ok_or("Draw lost its exact target snapshots.")?;
                let source = reservoir_from_snapshots(snapshots, *from)
                    .ok_or("Draw source snapshot is missing.")?;
                let destination = reservoir_from_snapshots(snapshots, *to)
                    .ok_or("Draw destination snapshot is missing.")?;
                let source_carrier = carrier_from_snapshots(snapshots, *from)
                    .ok_or("Draw source carrier snapshot is missing.")?;
                let destination_carrier = carrier_from_snapshots(snapshots, *to)
                    .ok_or("Draw destination carrier snapshot is missing.")?;
                if self.water_mass_at(*from) != Some(source)
                    || self.water_mass_at(*to).unwrap_or_default() != destination
                    || self.water_carrier_at(*from, source) != source_carrier
                    || self.water_carrier_at(*to, destination) != destination_carrier
                {
                    return Err("Draw source or destination changed before release.".into());
                }
                Ok(())
            }
            WorkingEffect::Impulse {
                entity_id,
                entity_kind,
                before_velocity_milli,
                expected_version,
                ..
            } => {
                let velocity = match entity_kind {
                    NudgeEntityKind::Projectile => self
                        .projectiles()
                        .iter()
                        .find(|projectile| projectile.stable_id == *entity_id)
                        .map(|projectile| vec3_milli(projectile.vel)),
                    NudgeEntityKind::DroppedItem => self
                        .loose_items()
                        .iter()
                        .find(|item| item.stable_id == *entity_id)
                        .map(|item| vec3_milli(item.vel)),
                }
                .ok_or("The Nudge target left the host simulation before release.")?;
                if *entity_id != *expected_version || velocity != *before_velocity_milli {
                    return Err("The Nudge target changed before release.".into());
                }
                Ok(())
            }
            WorkingEffect::OperateMechanism {
                target,
                block_name,
                before_state,
                ..
            } => {
                if self.reg.block(self.get_block_at(*target)).name != *block_name
                    || !self.is_nudge_mechanism_at(*target)
                    || !matches!(
                        self.block_entity_at(target),
                        Some(BlockEntity::Steam(steam))
                            if u8::from(steam.draft_closed) == *before_state
                    )
                {
                    return Err("The Nudge mechanism changed before release.".into());
                }
                Ok(())
            }
            WorkingEffect::AdvanceBed { .. } | WorkingEffect::Ward { .. } => {
                let transaction = self
                    .workings_state
                    .as_ref()
                    .into_iter()
                    .flat_map(|state| state.active.values())
                    .find(|transaction| &transaction.effect == effect)
                    .ok_or("The ritual lost its exact target snapshots.")?;
                self.validate_saved_world_targets(transaction)
            }
            WorkingEffect::Settle { .. } => Ok(()),
            _ => Err("That native effect needs its domain-specific completion adapter.".into()),
        }
    }

    fn apply_working_effect(
        &mut self,
        effect: &WorkingEffect,
        targets: &[WorkingTargetSnapshot],
    ) -> Result<(), String> {
        match effect {
            WorkingEffect::Observe { .. }
            | WorkingEffect::PointLight { .. }
            | WorkingEffect::Preserve { .. }
            | WorkingEffect::TransferCurrent { .. } => Ok(()),
            WorkingEffect::Ignite {
                fuel,
                fire_cell,
                expected_fuel,
                expected_air,
                player_caused,
            } => {
                if self
                    .reg
                    .block_id("base:fire")
                    .is_some_and(|fire| self.get_block_at(*fire_cell) == fire)
                {
                    return Ok(());
                }
                if self.get_block_at(*fuel).0 != *expected_fuel
                    || self.get_block_at(*fire_cell).0 != *expected_air
                {
                    return Err("Kindle replay found neither its before nor after state.".into());
                }
                self.validate_kindle_target(*fuel, *fire_cell)?;
                if self.light_fire_at(*fire_cell, *player_caused) {
                    Ok(())
                } else {
                    Err("Ordinary fire refused the Kindle effect.".into())
                }
            }
            WorkingEffect::AdvancePlant(advance) => self.apply_plant_advance(*advance),
            WorkingEffect::TransferWater {
                from,
                to,
                water_hu,
                salt_mass,
                temperature_millic,
                thermal_millic_hu,
                dross_units,
                dross_subunits,
                carrier_remainder_before,
                carrier_remainder_after,
            } => self.apply_draw(
                targets,
                *from,
                *to,
                *water_hu,
                *salt_mass,
                *temperature_millic,
                *thermal_millic_hu,
                *dross_units,
                *dross_subunits,
                *carrier_remainder_before,
                *carrier_remainder_after,
            ),
            WorkingEffect::Impulse {
                entity_id,
                entity_kind,
                before_velocity_milli,
                impulse_milli,
                expected_version,
                ..
            } => {
                let before = milli_vec3(*before_velocity_milli);
                let after = (before + milli_vec3(*impulse_milli)).clamp_length_max(40.0);
                let mut outcome = Err("Nudge replay could not find its transient target.".into());
                let mut apply = |stable_id: u64, velocity: &mut glam::Vec3| {
                    if stable_id != *entity_id || stable_id != *expected_version {
                        return;
                    }
                    let actual = vec3_milli(*velocity);
                    if actual == vec3_milli(after) {
                        outcome = Ok(());
                    } else if actual == *before_velocity_milli {
                        *velocity = after;
                        outcome = Ok(());
                    } else {
                        outcome = Err(
                            "Nudge replay found neither its exact before nor after velocity."
                                .into(),
                        );
                    }
                };
                match entity_kind {
                    NudgeEntityKind::Projectile => self.for_each_projectile_mut(|projectile| {
                        apply(projectile.stable_id, &mut projectile.vel)
                    }),
                    NudgeEntityKind::DroppedItem => {
                        self.for_each_loose_item_mut(|item| apply(item.stable_id, &mut item.vel))
                    }
                }
                outcome
            }
            WorkingEffect::OperateMechanism {
                target,
                block_name,
                before_state,
                after_state,
                ..
            } => {
                if self.reg.block(self.get_block_at(*target)).name != *block_name {
                    return Err("Nudge replay found a different physical mechanism.".into());
                }
                let Some(BlockEntity::Steam(steam)) = self.block_entities.get_mut(target) else {
                    return Err("Nudge replay found no embodied draft latch.".into());
                };
                let actual = u8::from(steam.draft_closed);
                if actual == *after_state {
                    return Ok(());
                }
                if actual != *before_state {
                    return Err(
                        "Nudge replay found neither the exact before nor after latch state.".into(),
                    );
                }
                steam.draft_closed = *after_state != 0;
                Ok(())
            }
            WorkingEffect::AdvanceBed { plants, .. } => {
                for advance in plants {
                    self.apply_plant_advance(*advance)?;
                }
                Ok(())
            }
            WorkingEffect::Settle { .. } | WorkingEffect::Ward { .. } => Ok(()),
            _ => Err(
                "That working must complete through its named entity/item/ritual adapter.".into(),
            ),
        }
    }

    fn apply_plant_advance(&mut self, advance: PlantAdvance) -> Result<(), String> {
        let after_block = BlockId(advance.after_block);
        let plant_after = self.get_block_at(advance.pos) == after_block
            && self.get_meta_at(advance.pos) == advance.after_meta;
        let soil_after = advance
            .soil_pos
            .is_none_or(|soil| self.get_meta_at(soil) == advance.after_soil_meta);
        let water_after = advance.water_source.is_none_or(|water| {
            self.water_mass_at(water).is_some_and(|mass| {
                mass.water_hu == advance.water_after_hu && mass.salt_mass == advance.salt_after
            })
        });
        if plant_after && soil_after && water_after {
            return Ok(());
        }
        if self.get_block_at(advance.pos).0 != advance.before_block
            || self.get_meta_at(advance.pos) != advance.before_meta
            || advance
                .soil_pos
                .is_some_and(|soil| self.get_meta_at(soil) != advance.before_soil_meta)
            || advance.water_source.is_some_and(|water| {
                self.water_mass_at(water).is_none_or(|mass| {
                    mass.water_hu != advance.water_before_hu
                        || mass.salt_mass != advance.salt_before
                })
            })
        {
            return Err(
                "Rootwake replay found neither its complete before nor after state.".into(),
            );
        }
        self.set_block_meta_at(advance.pos, after_block, advance.after_meta);
        if let Some(soil) = advance.soil_pos {
            let block = self.get_block_at(soil);
            self.set_block_meta_at(soil, block, advance.after_soil_meta);
        }
        if let Some(water) = advance.water_source {
            self.write_water_mass_at(
                water,
                crate::planet_atlas::ReservoirMass {
                    water_hu: advance.water_after_hu,
                    salt_mass: advance.salt_after,
                },
            );
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_draw(
        &mut self,
        targets: &[WorkingTargetSnapshot],
        from: BlockPos,
        to: BlockPos,
        water_hu: u64,
        salt_mass: u64,
        temperature_millic: i32,
        thermal_millic_hu: i64,
        dross_units: u64,
        dross_subunits: u64,
        carrier_remainder_before: u64,
        carrier_remainder_after: u64,
    ) -> Result<(), String> {
        let source_before =
            reservoir_from_snapshots(targets, from).ok_or("Draw source snapshot is missing.")?;
        let destination_before =
            reservoir_from_snapshots(targets, to).ok_or("Draw destination snapshot is missing.")?;
        let source_carrier_before = carrier_from_snapshots(targets, from)
            .ok_or("Draw source carrier snapshot is missing.")?;
        let destination_carrier_before = carrier_from_snapshots(targets, to)
            .ok_or("Draw destination carrier snapshot is missing.")?;
        let mut source_after = source_before;
        let parcel = source_after.take(water_hu);
        if parcel.water_hu != water_hu || parcel.salt_mass != salt_mass {
            return Err("Draw parcel disagrees with its conserved source snapshot.".into());
        }
        let destination_after = destination_before
            .checked_add(parcel)
            .filter(|mass| mass.water_hu <= crate::planet_atlas::HYDRO_UNITS_PER_BLOCK)
            .ok_or("Draw destination overflows during replay.")?;
        let mut source_carrier_after = source_carrier_before;
        let parcel_carrier = source_carrier_after
            .take(source_before.water_hu, water_hu)
            .ok_or("Draw carrier parcel disagrees with its source snapshot.")?;
        if parcel_carrier.thermal_millic_hu != thermal_millic_hu
            || parcel_carrier.temperature_millic(water_hu) != temperature_millic
            || parcel_carrier.dross_subunits != dross_subunits
            || parcel_carrier.dross_subunits / 256 != dross_units
            || source_carrier_before.dross_subunits % 256 != carrier_remainder_before
            || source_carrier_after.dross_subunits % 256 != carrier_remainder_after
        {
            return Err("Draw's typed effect disagrees with its exact carrier snapshots.".into());
        }
        let destination_carrier_after = destination_carrier_before
            .checked_add(parcel_carrier)
            .ok_or("Draw destination carrier overflows during replay.")?;
        let actual_source = self.water_mass_at(from).unwrap_or_default();
        let actual_destination = self.water_mass_at(to).unwrap_or_default();
        let actual_source_carrier = self.water_carrier_at(from, actual_source);
        let actual_destination_carrier = self.water_carrier_at(to, actual_destination);
        if actual_source == source_after
            && actual_destination == destination_after
            && actual_source_carrier == source_carrier_after
            && actual_destination_carrier == destination_carrier_after
        {
            return Ok(());
        }
        if ![source_before, source_after].contains(&actual_source)
            || ![destination_before, destination_after].contains(&actual_destination)
            || ![source_carrier_before, source_carrier_after].contains(&actual_source_carrier)
            || ![destination_carrier_before, destination_carrier_after]
                .contains(&actual_destination_carrier)
        {
            return Err("Draw replay found neither its complete before nor after state.".into());
        }
        if actual_destination != destination_after {
            self.write_water_mass_at(to, destination_after);
        }
        if actual_source != source_after {
            self.write_water_mass_at(from, source_after);
        }
        self.set_water_carrier_at(to, destination_after, destination_carrier_after)?;
        self.set_water_carrier_at(from, source_after, source_carrier_after)?;
        Ok(())
    }

    fn save_effect_chunks(&self, effect: &WorkingEffect) -> Result<(), String> {
        let mut chunks = effect_path(effect)
            .into_iter()
            .map(BlockPos::chunk)
            .collect::<Vec<_>>();
        chunks.sort();
        chunks.dedup();
        for chunk in chunks {
            self.save_chunk(chunk).map_err(|error| {
                format!("working effect chunk {chunk:?} could not be persisted: {error}")
            })?;
        }
        Ok(())
    }

    fn save_effect_sidecars(&self, effect: &WorkingEffect) -> Result<(), String> {
        if matches!(effect, WorkingEffect::TransferWater { .. }) {
            self.save_water_carriers()?;
        }
        if matches!(
            effect,
            WorkingEffect::Impulse {
                entity_kind: NudgeEntityKind::DroppedItem,
                ..
            }
        ) {
            self.save_loose_items().map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn block_snapshot(&self, pos: BlockPos) -> WorkingTargetSnapshot {
        WorkingTargetSnapshot::Block {
            pos,
            block_name: self.reg.block(self.get_block_at(pos)).name.clone(),
            metadata: self.get_meta_at(pos),
            version: 0,
        }
    }

    fn reservoir_snapshot(
        &self,
        pos: BlockPos,
        mass: crate::planet_atlas::ReservoirMass,
    ) -> WorkingTargetSnapshot {
        self.reservoir_snapshot_with_carrier(pos, mass, self.water_carrier_at(pos, mass))
    }

    fn reservoir_snapshot_with_carrier(
        &self,
        pos: BlockPos,
        mass: crate::planet_atlas::ReservoirMass,
        carrier: crate::workings::WaterCarrier,
    ) -> WorkingTargetSnapshot {
        WorkingTargetSnapshot::Reservoir {
            pos,
            water_hu: mass.water_hu,
            salt_mass: mass.salt_mass,
            temperature_millic: carrier.temperature_millic(mass.water_hu),
            thermal_millic_hu: carrier.thermal_millic_hu,
            dross_units: carrier.dross_subunits / 256,
            carrier_remainder: carrier.dross_subunits % 256,
            version: 0,
        }
    }

    pub(crate) fn water_carrier_at(
        &self,
        pos: BlockPos,
        mass: crate::planet_atlas::ReservoirMass,
    ) -> crate::workings::WaterCarrier {
        self.water_carriers
            .as_ref()
            .and_then(|state| state.cells.get(&pos).copied())
            .unwrap_or_else(|| {
                let temperature =
                    (self.weather_at_surface(pos.surface()).temperature_c * 1_000.0).round() as i64;
                crate::workings::WaterCarrier {
                    thermal_millic_hu: temperature
                        .saturating_mul(i64::try_from(mass.water_hu).unwrap_or_default()),
                    dross_subunits: 0,
                }
            })
    }

    pub(super) fn set_water_carrier_at(
        &mut self,
        pos: BlockPos,
        mass: crate::planet_atlas::ReservoirMass,
        carrier: crate::workings::WaterCarrier,
    ) -> Result<(), String> {
        let Some(state) = self.water_carriers.as_mut() else {
            return Ok(());
        };
        if mass.water_hu == 0 {
            state.cells.remove(&pos);
        } else {
            validate_water_carrier(mass, carrier)?;
            if !state.cells.contains_key(&pos)
                && state.cells.len() >= crate::workings::MAX_WATER_CARRIER_CELLS
            {
                return Err("The bounded detailed water-carrier census is full.".into());
            }
            state.cells.insert(pos, carrier);
        }
        Ok(())
    }

    pub(super) fn move_tracked_water_carrier(
        &mut self,
        from: BlockPos,
        to: BlockPos,
        source_before: crate::planet_atlas::ReservoirMass,
        destination_before: crate::planet_atlas::ReservoirMass,
        water_hu: u64,
    ) -> Result<(), String> {
        let tracked = self
            .water_carriers
            .as_ref()
            .is_some_and(|state| state.cells.contains_key(&from) || state.cells.contains_key(&to));
        if !tracked {
            return Ok(());
        }
        let mut source = self.water_carrier_at(from, source_before);
        let parcel = source
            .take(source_before.water_hu, water_hu)
            .ok_or("ordinary water flow could not split a tracked carrier")?;
        let destination = self
            .water_carrier_at(to, destination_before)
            .checked_add(parcel)
            .ok_or("ordinary water flow overflowed a tracked carrier")?;
        let mut source_after = source_before;
        let moved = source_after.take(water_hu);
        let destination_after = destination_before
            .checked_add(moved)
            .ok_or("ordinary water flow overflowed its destination mass")?;
        validate_water_carrier(source_after, source)?;
        validate_water_carrier(destination_after, destination)?;
        let state = self
            .water_carriers
            .as_mut()
            .expect("tracked carrier test proved authoritative state exists");
        let existing_after =
            usize::from(source_after.water_hu != 0) + usize::from(destination_after.water_hu != 0);
        let existing_before = usize::from(state.cells.contains_key(&from))
            + usize::from(state.cells.contains_key(&to));
        if state
            .cells
            .len()
            .saturating_sub(existing_before)
            .saturating_add(existing_after)
            > crate::workings::MAX_WATER_CARRIER_CELLS
        {
            return Err("The bounded detailed water-carrier census is full.".into());
        }
        if source_after.water_hu == 0 {
            state.cells.remove(&from);
        } else {
            state.cells.insert(from, source);
        }
        if destination_after.water_hu == 0 {
            state.cells.remove(&to);
        } else {
            state.cells.insert(to, destination);
        }
        Ok(())
    }

    fn save_water_carriers(&self) -> Result<(), String> {
        self.water_carriers.as_ref().map_or(Ok(()), |state| {
            state.save().map_err(|error| error.to_string())
        })
    }

    fn working_tick(&self) -> u64 {
        (self.clock.max(0.0) * 20.0).round() as u64
    }

    fn local_capacity_permille(&self, region: crate::planet_atlas::AtlasPos) -> u16 {
        self.arcane_geography
            .as_ref()
            .map(|geography| {
                let index = region.index(geography.manifest.side);
                let cell = geography.dynamic.cells[index];
                let baseline = geography.controls[index].capacity.max(1);
                u16::try_from(
                    cell.ambient_total()
                        .saturating_mul(1_000)
                        .checked_div(u64::from(baseline))
                        .unwrap_or_default()
                        .min(1_000),
                )
                .unwrap_or(1_000)
            })
            .unwrap_or(1_000)
    }

    /// Bounded, qualitative information unlocked while Trace actually holds
    /// its reserved Current. This intentionally reports neither identities,
    /// exact historic actions, inventories, balances, nor a global map.
    fn trace_report(&self, transaction: &WorkingTransaction) -> Result<String, String> {
        let WorkingEffect::Observe {
            origin,
            expires_tick,
        } = transaction.effect
        else {
            return Err("Trace lost its bounded observation target.".into());
        };
        if transaction.phase != WorkingPhase::Active || self.working_tick() > expires_tick {
            return Err("Trace is no longer holding Current around its lens.".into());
        }
        let atlas = self
            .planet_atlas
            .as_ref()
            .ok_or("Trace needs the finite planetary Current atlas.")?;
        let region = atlas.atlas_pos(origin.surface());
        let survey = self
            .arcane_geography
            .as_ref()
            .map(|geography| geography.survey(region, true))
            .ok_or("Trace needs the authoritative Current geography.")?;
        let drift = survey.drift.map_or_else(
            || "no stable drift".to_string(),
            |direction| format!("weak drift {direction:?}").to_ascii_lowercase(),
        );
        let recent_window = 20 * 60 * 5;
        let now = self.working_tick();
        let nearby =
            self.workings_state
                .as_ref()
                .into_iter()
                .flat_map(|state| state.history.iter())
                .filter(|event| {
                    event.working_id != "base:trace"
                        && now.saturating_sub(event.completed_tick) <= recent_window
                        && event.path.iter().any(|pos| {
                            pos.entity_center().distance_to(origin.entity_center()) <= 8.0
                        })
                })
                .collect::<Vec<_>>();
        let traces = match nearby.len() {
            0 => "no recent working trace".to_string(),
            1 => "one faint recent working trace".to_string(),
            count => format!("{} overlapping recent working traces", count.min(9)),
        };
        let local_dross = self.arcane_cue_at(region)[1];
        let warning = nearby
            .iter()
            .map(|event| event.warning_band)
            .max()
            .unwrap_or_default();
        let leakage = match local_dross.max(warning) {
            0 => "no resolved charge leakage",
            1 => "a faint charge leak",
            2 => "a discordant charge leak",
            _ => "a fouled charge leak",
        };
        let uncertainty = survey.uncertainty.saturating_sub(7).max(5);
        Ok(format!(
            "Trace resolves {drift}; {traces}; {leakage}; uncertainty {uncertainty}%."
        ))
    }
}

fn ward_local_positions(controller: BlockPos, radius: i32) -> BTreeMap<BlockPos, (i32, i32)> {
    let mut positions = BTreeMap::new();
    for du in -radius..=radius {
        for dv in -radius..=radius {
            if let Some(pos) = controller.offset(du, 0, dv) {
                positions
                    .entry(pos)
                    .and_modify(|saved: &mut (i32, i32)| {
                        *saved = (*saved).min((du, dv));
                    })
                    .or_insert((du, dv));
            }
        }
    }
    positions
}

fn ward_horizontal_neighbors(pos: BlockPos) -> Vec<BlockPos> {
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .filter_map(|(du, dv)| pos.offset(du, 0, dv))
        .collect()
}

/// Return the finite component trapped around the controller, or `None` when
/// the supplied segments do not actually close. This is stricter than a
/// bounding-box test and handles concave player-built boundaries correctly.
fn ward_interior(boundary: &BTreeSet<(i32, i32)>) -> Option<BTreeSet<(i32, i32)>> {
    if boundary.is_empty() || boundary.contains(&(0, 0)) {
        return None;
    }
    let limit = boundary
        .iter()
        .map(|(u, v)| u.abs().max(v.abs()))
        .max()?
        .saturating_add(1);
    let mut interior = BTreeSet::<(i32, i32)>::from([(0, 0)]);
    let mut queue = VecDeque::<(i32, i32)>::from([(0, 0)]);
    while let Some((u, v)) = queue.pop_front() {
        if u.abs() == limit || v.abs() == limit {
            return None;
        }
        for next in [(u + 1, v), (u - 1, v), (u, v + 1), (u, v - 1)] {
            if next.0.abs() <= limit
                && next.1.abs() <= limit
                && !boundary.contains(&next)
                && interior.insert(next)
            {
                queue.push_back(next);
            }
        }
    }
    Some(interior)
}

fn ward_radius(controller: BlockPos, boundary: &[BlockPos]) -> u16 {
    let local = ward_local_positions(controller, 16);
    boundary
        .iter()
        .filter_map(|pos| local.get(pos))
        .map(|(u, v)| u.abs().max(v.abs()) as u16)
        .max()
        .unwrap_or(1)
}

fn inside_ward(
    controller: BlockPos,
    pos: BlockPos,
    segments: &[crate::workings::WardSegment],
) -> bool {
    let radius = ward_radius(
        controller,
        &segments
            .iter()
            .map(|segment| segment.pos)
            .collect::<Vec<_>>(),
    );
    if i32::from(pos.y()).abs_diff(i32::from(controller.y())) > u32::from(radius) {
        return false;
    }
    let local = ward_local_positions(controller, i32::from(radius).saturating_add(1));
    let Some(target) = local.get(&pos.with_y(controller.y())).copied() else {
        return false;
    };
    let Some(boundary) = segments
        .iter()
        .map(|segment| local.get(&segment.pos).copied())
        .collect::<Option<BTreeSet<_>>>()
    else {
        return false;
    };
    ward_interior(&boundary).is_some_and(|interior| interior.contains(&target))
}

fn reservoir_from_snapshots(
    snapshots: &[WorkingTargetSnapshot],
    pos: BlockPos,
) -> Option<crate::planet_atlas::ReservoirMass> {
    snapshots.iter().find_map(|snapshot| match snapshot {
        WorkingTargetSnapshot::Reservoir {
            pos: at,
            water_hu,
            salt_mass,
            ..
        } if *at == pos => Some(crate::planet_atlas::ReservoirMass {
            water_hu: *water_hu,
            salt_mass: *salt_mass,
        }),
        _ => None,
    })
}

fn carrier_from_snapshots(
    snapshots: &[WorkingTargetSnapshot],
    pos: BlockPos,
) -> Option<crate::workings::WaterCarrier> {
    snapshots.iter().find_map(|snapshot| match snapshot {
        WorkingTargetSnapshot::Reservoir {
            pos: at,
            thermal_millic_hu,
            dross_units,
            carrier_remainder,
            ..
        } if *at == pos => Some(crate::workings::WaterCarrier {
            thermal_millic_hu: *thermal_millic_hu,
            dross_subunits: dross_units
                .checked_mul(256)?
                .checked_add(*carrier_remainder)?,
        }),
        _ => None,
    })
}

fn validate_water_carrier(
    mass: crate::planet_atlas::ReservoirMass,
    carrier: crate::workings::WaterCarrier,
) -> Result<(), String> {
    if (mass.water_hu == 0 && carrier != crate::workings::WaterCarrier::default())
        || mass.water_hu > crate::planet_atlas::HYDRO_UNITS_PER_BLOCK
        || carrier.thermal_millic_hu.unsigned_abs() > mass.water_hu.saturating_mul(100_000)
        || carrier.dross_subunits > u64::from(u32::MAX).saturating_mul(256)
    {
        return Err("A detailed water carrier is unbounded or detached from water.".into());
    }
    Ok(())
}

fn repair_inventory_slots(transaction: &WorkingTransaction) -> Result<(usize, usize), String> {
    let WorkingEffect::RepairItem {
        item_id,
        repair_material,
        ..
    } = &transaction.effect
    else {
        return Err("That transaction is not an inventory repair.".into());
    };
    let mut target = None;
    let mut material = None;
    for snapshot in &transaction.targets {
        if let WorkingTargetSnapshot::Item {
            stable_id,
            item_name,
            version,
            ..
        } = snapshot
        {
            let slot = usize::try_from(*version)
                .map_err(|_| "Saved Fieldmend inventory slot overflowed.")?;
            if *stable_id == *item_id {
                target = Some(slot);
            } else if item_name == repair_material {
                material = Some(slot);
            }
        }
    }
    let target = target.ok_or("Fieldmend target slot snapshot is missing.")?;
    let material = material.ok_or("Fieldmend material slot snapshot is missing.")?;
    if target >= crate::inventory::TOTAL_SLOTS
        || material >= crate::inventory::TOTAL_SLOTS
        || target == material
    {
        return Err("Fieldmend saved invalid inventory slots.".into());
    }
    Ok((target, material))
}

fn dross_medium(handler: WorkingHandler) -> DrossMedium {
    match handler {
        WorkingHandler::Draw | WorkingHandler::Rootwake | WorkingHandler::RootingBed => {
            DrossMedium::Water
        }
        WorkingHandler::Ignite
        | WorkingHandler::Nudge
        | WorkingHandler::Trace
        | WorkingHandler::Gleam
        | WorkingHandler::Holdfast
        | WorkingHandler::WardBoundary => DrossMedium::Air,
        WorkingHandler::Fieldmend
        | WorkingHandler::SettlingRite
        | WorkingHandler::TransferCircle => DrossMedium::Soil,
    }
}

fn effect_path(effect: &WorkingEffect) -> Vec<BlockPos> {
    match effect {
        WorkingEffect::Observe { origin, .. } => vec![*origin],
        WorkingEffect::PointLight { source, target, .. } => vec![*source, *target],
        WorkingEffect::Ignite {
            fuel, fire_cell, ..
        } => vec![*fuel, *fire_cell],
        WorkingEffect::AdvancePlant(advance) => {
            let mut path = vec![advance.pos];
            path.extend(advance.soil_pos);
            path.extend(advance.water_source);
            path
        }
        WorkingEffect::TransferWater { from, to, .. } => vec![*from, *to],
        WorkingEffect::Settle { controller, .. } => vec![*controller],
        WorkingEffect::AdvanceBed {
            controller, plants, ..
        } => std::iter::once(*controller)
            .chain(plants.iter().map(|plant| plant.pos))
            .collect(),
        WorkingEffect::Ward {
            controller,
            segments,
            ..
        } => std::iter::once(*controller)
            .chain(segments.iter().map(|segment| segment.pos))
            .collect(),
        WorkingEffect::Impulse { source, target, .. }
        | WorkingEffect::OperateMechanism { source, target, .. } => vec![*source, *target],
        WorkingEffect::RepairItem { .. }
        | WorkingEffect::Preserve { .. }
        | WorkingEffect::TransferCurrent { .. } => Vec::new(),
    }
}

fn working_completion(tick: u64, transaction: &WorkingTransaction) -> u16 {
    let duration = transaction
        .due_tick
        .saturating_sub(transaction.started_tick);
    if duration == 0 {
        return if transaction.phase == WorkingPhase::Charging {
            0
        } else {
            1_000
        };
    }
    u16::try_from(
        tick.saturating_sub(transaction.started_tick)
            .min(duration)
            .saturating_mul(1_000)
            / duration,
    )
    .unwrap_or(1_000)
}

fn working_distance(from: BlockPos, to: BlockPos) -> u16 {
    let distance = from
        .entity_center()
        .render_pos()
        .distance(to.entity_center().render_pos())
        .ceil();
    if !distance.is_finite() || distance <= 0.0 {
        0
    } else {
        distance.min(f32::from(u16::MAX)) as u16
    }
}

fn vec3_milli(value: glam::Vec3) -> [i32; 3] {
    value
        .to_array()
        .map(|component| (component * 1_000.0).round().clamp(-80_000.0, 80_000.0) as i32)
}

fn milli_vec3(value: [i32; 3]) -> glam::Vec3 {
    glam::Vec3::from_array(value.map(|component| component as f32 / 1_000.0))
}

fn inventory_target_id(actor: [u8; 16], slot: usize, item: u16) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in actor
        .into_iter()
        .chain((slot as u64).to_le_bytes())
        .chain(item.to_le_bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash.max(1)
}

fn mounted_target_id(pos: BlockPos, bay: u8, item: u16) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in [pos.face() as u8]
        .into_iter()
        .chain(pos.u().to_le_bytes())
        .chain([pos.y()])
        .chain(pos.v().to_le_bytes())
        .chain([bay])
        .chain(item.to_le_bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dross_media_are_explicit_for_every_native_handler() {
        for handler in WorkingHandler::ALL {
            let _ = dross_medium(handler);
        }
    }
}
