//! Ward workings transaction coordination.

use crate::arcane::ArcaneOwner;
use crate::world::BlockPos;
use crate::arcane::Current;
use crate::workings::PhysicalDebit;
use crate::workings::PhysicalDebitKind;
use crate::workings::WorkingEffect;
use crate::workings::WorkingPhase;
use crate::workings::WorkingResult;
use crate::workings::WorkingTargetSnapshot;
use crate::world::World;
use super::inside_ward;
use super::ward_radius;

impl World {
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
}
