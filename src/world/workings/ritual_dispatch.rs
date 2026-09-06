//! Ritual dispatch workings transaction coordination.

use crate::world::BlockPos;
use crate::workings::WorkingHandler;
use crate::workings::WorkingPhase;
use crate::workings::WorkingResult;
use crate::world::World;

impl World {
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
}
