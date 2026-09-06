//! Implements capture scene construction.

use super::DemoChart;
use crate::game::Game;
use crate::identity;
use crate::inventory::Inventory;
use crate::inventory::ItemStack;
use crate::planet::EntityPos;
use crate::registry::AIR;
use glam::Vec3;

impl Game {
    /// A visual qualification scene assembled through the real frame verbs.
    /// No decorative duplicate is spawned: the glowing vessel, mounted
    /// apparatus, component-derived held wand, charge, dross, and stable item
    /// id are the same authority state an ordinary player would create.
    pub(super) fn stage_implements_demo(&mut self, spawn: EntityPos) -> Result<(), String> {
        use crate::implements::FrameAction;

        let chart = DemoChart::new(spawn.face());
        let reg = self.content.reg.clone();
        let bx = spawn.x.round() as i32;
        let bz = spawn.z.round() as i32 - 6;
        let y = demo_height!(self.runtime.local().world, chart, bx, bz) + 1;
        let frame = chart.block(bx, y, bz);
        let focus = chart.block(bx + 1, y, bz);
        let vessel_pos = chart.block(bx - 1, y, bz);
        let conductor = chart.block(bx, y, bz - 1);
        let containment = chart.block(bx + 2, y, bz);
        for pos in [frame, focus, vessel_pos, conductor, containment] {
            for du in -1..=1 {
                for dv in -1..=1 {
                    if let Some(near) = pos.offset(du, 0, dv) {
                        self.runtime.local_mut().world.ensure_chunk(near.chunk());
                    }
                }
            }
            self.runtime.local_mut().world.set_block_at(pos, AIR);
        }
        for (pos, name) in [
            (frame, "base:binding_frame"),
            (focus, "base:focus_mount"),
            (conductor, "base:arcane_conductor"),
            (containment, "base:containment_post"),
        ] {
            let block = reg
                .block_id(name)
                .ok_or_else(|| format!("capture registry lacks {name}"))?;
            self.runtime.local_mut().world.set_block_authored_at(
                pos,
                block,
                "development implements qualification scene",
            );
        }
        let vessel_item = reg
            .item_id("base:charge_vessel")
            .ok_or("capture registry lacks the charge vessel item")?;
        let vessel = ItemStack::new(&reg, vessel_item, 1);
        self.runtime
            .local_mut()
            .world
            .record_external_stack(vessel, "development implements qualification scene")
            .map_err(|error| error.to_string())?;
        if !self
            .runtime
            .local_mut()
            .world
            .place_item_block_at(vessel_pos, vessel)
        {
            return Err("the physical charge vessel could not be placed".into());
        }
        let layout = self.runtime.local().world.binding_frame_layout(frame);
        if !layout.valid {
            return Err(layout.problems.join(" "));
        }

        let mut work = Inventory::new();
        let mut revision = self
            .runtime
            .local_mut()
            .world
            .operate_binding_frame(
                frame,
                &mut work,
                0,
                FrameAction::Calibrate,
                None,
                "development visual qualification",
            )?
            .revision;
        for name in [
            "base:seasoned_wand_body",
            "base:wellglass_shard",
            "base:echo_slate",
            "base:bronze_wand_binding",
        ] {
            let item = reg
                .item_id(name)
                .ok_or_else(|| format!("capture registry lacks {name}"))?;
            let stack = ItemStack::new(&reg, item, 1);
            self.runtime
                .local_mut()
                .world
                .record_external_stack(stack, "development implements qualification scene")
                .map_err(|error| error.to_string())?;
            work.slots[0] = Some(stack);
            revision = self
                .runtime
                .local_mut()
                .world
                .operate_binding_frame(
                    frame,
                    &mut work,
                    0,
                    FrameAction::ExchangeSelected,
                    Some(revision),
                    "development visual qualification",
                )?
                .revision;
        }
        revision = self
            .runtime
            .local_mut()
            .world
            .operate_binding_frame(
                frame,
                &mut work,
                0,
                FrameAction::Assemble,
                Some(revision),
                "development visual qualification",
            )?
            .revision;
        revision = self
            .runtime
            .local_mut()
            .world
            .operate_binding_frame(
                frame,
                &mut work,
                0,
                FrameAction::Transfer,
                Some(revision),
                "development visual qualification",
            )?
            .revision;
        self.runtime.local_mut().world.operate_binding_frame(
            frame,
            &mut work,
            0,
            FrameAction::ExchangeSelected,
            Some(revision),
            "development visual qualification",
        )?;
        let wand = work.slots[0].ok_or("the assembled wand did not leave the frame")?;
        let selected = self.input.hotbar_sel;
        if let Some(previous) = self.inventory.slots[selected] {
            self.runtime
                .local_mut()
                .world
                .record_admin_stack_deletion(previous)
                .map_err(|error| error.to_string())?;
        }
        self.inventory.slots[selected] = Some(wand);
        if std::env::var("WILDFORGE_POS").is_err() {
            self.player.pos = self
                .player
                .pos
                .relocated_local(Vec3::new(spawn.x, y as f32 + 1.2, spawn.z + 0.5))
                .map_err(|error| {
                    format!("the capture player could not stand beside the apparatus: {error}")
                })?;
        }
        self.player.vel = Vec3::ZERO;
        self.camera.follow_planet(self.player.eye());
        self.camera.yaw = -std::f32::consts::FRAC_PI_2;
        self.camera.pitch = -0.16;
        if std::env::var("WILDFORGE_DEMO_WORKINGS").is_ok() {
            let source = self
                .player
                .pos
                .block()
                .ok_or("the workings capture player has no physical source cell")?;
            let actor = identity::local_player_id(
                &self.runtime.local().world.save_dir_for_saving(),
                self.identity.device_id(),
            )
            .unwrap_or(identity::PlayerId([0; 16]));
            let result = self.runtime.local_mut().world.begin_wand_working(
                actor.0,
                &self.config.display_name,
                source,
                wand.arcane_id,
                "base:gleam",
                crate::workings::WorkingTargetIntent::None,
                Some(&self.inventory),
                false,
            )?;
            let clock = self.runtime.view().clock()
                + f64::from(crate::workings::MIN_WAND_SETTLE_SECONDS)
                + 0.01;
            self.runtime.local_mut().world.set_simulation_clock(clock);
            self.runtime
                .local_mut()
                .world
                .activate_working(result.stable_id)?;
            if let Some(cue) = self
                .runtime
                .local()
                .world
                .working_cues()
                .into_iter()
                .find(|cue| cue.stable_id == result.stable_id)
            {
                self.present_working_cue(cue);
            }
            eprintln!(
                "workings demo: active Gleam {} from wand {}",
                result.stable_id, wand.arcane_id
            );
        }
        eprintln!(
            "implements demo: frame {frame:?}, wand id {}, charge {}, layout containment {}",
            wand.arcane_id,
            self.runtime
                .local()
                .world
                .arcane_ledger
                .as_ref()
                .and_then(|ledger| ledger.item_current_total(wand.arcane_id))
                .unwrap_or_default(),
            layout.containment
        );
        // The automated capture exits the process immediately after its
        // readback frame. Persist the world and player together now so the
        // wand cannot be left in the ledger after its frame custody was
        // debited but before its hotbar custody reaches the profile.
        self.save_session()
            .map_err(|error| format!("the qualification scene could not be saved: {error}"))?;
        Ok(())
    }
}
