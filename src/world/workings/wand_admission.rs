//! Wand admission workings transaction coordination.

use crate::arcane::ArcaneOwner;
use crate::planet::BlockPos;
use crate::workings::DeliveryMode;
use crate::workings::WorkingEffect;
use crate::workings::WorkingHandler;
use crate::workings::WorkingPhase;
use crate::workings::WorkingResult;
use crate::world::World;

impl World {
    pub(in crate::world) fn projectile_reserved_by_working(&self, stable_id: u64) -> bool {
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
    pub(super) fn require_carried_tuning_lens(
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
}
