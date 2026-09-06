//! Ritual layout workings transaction coordination.

use super::ward_horizontal_neighbors;
use super::ward_interior;
use super::ward_local_positions;
use crate::implements::ImplementKind;
use crate::planet::BlockPos;
use crate::world::BlockEntity;
use crate::world::World;
use std::collections::BTreeSet;
use std::collections::VecDeque;

impl World {
    pub(super) fn binding_frame_revision(&self, controller: BlockPos) -> Result<u64, String> {
        match self.block_entity_at(&controller) {
            Some(BlockEntity::BindingFrame(frame)) => Ok(frame.revision),
            _ => Err("The ritual controller is not an embodied binding frame.".into()),
        }
    }

    /// Find the smallest intact conductor cycle that physically encloses the
    /// controller. Coordinates are reconstructed through `BlockPos::offset`
    /// so an otherwise local ward remains valid when it crosses a cube-face
    /// seam; raw face-local `u/v` arithmetic would split the same structure.
    pub(super) fn closed_ward_boundary(
        &self,
        controller: BlockPos,
    ) -> Result<Vec<BlockPos>, String> {
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

    pub(super) fn ritual_vessels(
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
}
