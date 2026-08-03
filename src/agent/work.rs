//! Work macros: multi-step jobs built only from player verbs. Every
//! send here is a request the host validates — reach, rate, held
//! item — and every step waits for the world's echo, not its own
//! optimism.

use super::*;
use crate::net::{C2S, InventoryArea};

const REACH: f32 = 6.5; // stay inside the host's 7.0

impl Agent {
    fn dist_to(&self, pos: crate::planet::BlockPos) -> f32 {
        self.player.eye().distance_to(pos.entity_center())
    }

    fn face_block(&mut self, pos: crate::planet::BlockPos) {
        let delta = self.player.pos.local_delta_to(pos.entity_center());
        let d = Vec3::new(delta.x, 0.0, delta.z);
        if d.length() > 0.01 {
            self.yaw = d.z.atan2(d.x);
        }
    }

    pub fn aim_working_at(&mut self, pos: crate::planet::BlockPos) -> Result<String, String> {
        if self.dist_to(pos) > REACH {
            return Err("working target is out of reach".into());
        }
        self.face_block(pos);
        self.anchor_stance();
        self.pump_for(0.1);
        Ok(format!("aiming at {pos:?}"))
    }

    /// State the stance RELIABLY before an edit: position and held
    /// slot normally ride lossy datagrams, and a dropped hotbar
    /// update makes the host refuse a Place it should love.
    fn anchor_stance(&mut self) {
        self.send(&C2S::Move {
            pos: self.player.pos,
            yaw: self.yaw,
            hotbar: self.hotbar as u8,
            sprint: false,
        });
    }

    /// Find the item anywhere in the pack and get it into the held
    /// hotbar slot (the click protocol does the moving; the host's
    /// PlayerState echo confirms every step).
    pub fn select(&mut self, item_name: &str) -> Result<(), String> {
        let want = self
            .reg
            .item_id(item_name)
            .ok_or_else(|| format!("unknown item {item_name}"))?;
        if let Some(i) =
            (0..HOTBAR_SLOTS).find(|&i| self.inventory.slots[i].is_some_and(|s| s.item == want))
        {
            self.hotbar = i;
            return Ok(());
        }
        let src = (HOTBAR_SLOTS..TOTAL_SLOTS)
            .find(|&i| self.inventory.slots[i].is_some_and(|s| s.item == want))
            .ok_or_else(|| format!("no {item_name} in the pack"))?;
        let dst = (0..HOTBAR_SLOTS)
            .find(|&i| self.inventory.slots[i].is_none())
            .unwrap_or(self.hotbar);
        for (area, slot) in [
            (InventoryArea::Inventory, src),
            (InventoryArea::Inventory, dst),
            (InventoryArea::Inventory, src),
        ] {
            self.click(area, slot);
            if self.cursor.is_none() && slot == dst {
                break; // nothing displaced: two clicks did it
            }
        }
        self.hotbar = dst;
        Ok(())
    }

    /// Break one block and wait for the world's echo.
    pub fn break_block_at(&mut self, pos: crate::planet::BlockPos) -> Result<(), String> {
        if self.dist_to(pos) > REACH {
            return Err("out of reach".into());
        }
        let before = self.world.get_block_at(pos);
        if before == registry::AIR {
            return Err("nothing there".into());
        }
        self.face_block(pos);
        self.anchor_stance();
        self.send(&C2S::Break { pos });
        // Triple-size echo budget: parallel test load can starve the
        // host pump well past a polite wait (green runs exit early).
        for _ in 0..120 {
            self.pump_for(0.05);
            if self.world.get_block_at(pos) != before {
                return Ok(());
            }
        }
        Err("the host didn't allow that break".into())
    }

    /// Walk to a tree and fell what's reachable of its trunk; the
    /// drops arrive over the wire as the host awards them.
    pub fn chop_at(&mut self, pos: crate::planet::BlockPos) -> Result<String, String> {
        let logs = self.reg.tags.get("base:logs").cloned().unwrap_or_default();
        let is_log = |a: &Agent, at: crate::planet::BlockPos| {
            let b = a.world.get_block_at(at);
            a.reg
                .item_id(&a.reg.block(b).name)
                .is_some_and(|i| logs.contains(&i))
        };
        if !is_log(self, pos) {
            return Err("that isn't a log".into());
        }
        // Walk down to the trunk base, then stand beside it.
        let mut base = pos;
        while let Some(below) = base.offset(0, -1, 0)
            && is_log(self, below)
        {
            base = below;
        }
        let spot = [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (2, 0),
            (-2, 0),
            (0, 2),
            (0, -2),
        ]
        .into_iter()
        .find_map(|(dx, dz)| {
            (-2..=2).find_map(|dy| {
                let c = base.offset(dx, dy, dz)?;
                self.stands_at(c).then_some(c)
            })
        })
        .ok_or("nowhere to stand at that tree")?;
        self.go_to(spot)?;
        self.wait_idle(30.0);
        let mut felled = 0;
        for dy in 0..8 {
            let Some(at) = base.offset(0, dy, 0) else {
                break;
            };
            if !is_log(self, at) {
                break;
            }
            if self.dist_to(at) > REACH {
                break;
            }
            self.break_block_at(at)?;
            felled += 1;
        }
        // Whatever the trunk dropped has been Given by now.
        self.pump_for(0.3);
        if felled == 0 {
            return Err("couldn't reach the trunk".into());
        }
        Ok(format!("felled {felled} logs"))
    }

    /// Place a block from the pack against the world. Self-healing:
    /// under load the inventory mirror can lag the host's truth no
    /// matter how the clicks are paced (periodic PlayerState
    /// broadcasts blur the click-echo pairing), so a misplaced hold
    /// puts the WRONG BLOCK in the world. A player who misclicks
    /// breaks it and does it again — so does the agent.
    pub fn place_at(&mut self, pos: crate::planet::BlockPos, item: &str) -> Result<(), String> {
        if self.dist_to(pos) > REACH {
            return Err("out of reach".into());
        }
        let want = self.reg.item_id(item).and_then(|i| self.reg.item(i).places);
        let want_item = self.reg.item_id(item);
        let mut last = String::from("the host didn't allow that placement");
        for attempt in 0..3 {
            self.select(item)?;
            // Wait for the echo to prove the held slot.
            let mut proven = false;
            for _ in 0..60 {
                if self.inventory.slots[self.hotbar].map(|s| s.item) == want_item {
                    proven = true;
                    break;
                }
                self.pump_for(0.05);
            }
            if !proven {
                last = format!("the hand never settled on {item}");
                continue;
            }
            self.pump_for(0.1);
            let before = self.world.get_block_at(pos);
            if Some(before) == want {
                return Ok(()); // a slow echo: the last attempt landed
            }
            if before != registry::AIR {
                return Err("that cell is occupied".into());
            }
            self.face_block(pos);
            self.anchor_stance();
            self.send(&C2S::Place { pos });
            for _ in 0..90 {
                self.pump_for(0.05);
                let now = self.world.get_block_at(pos);
                if Some(now) == want {
                    return Ok(());
                }
                if now != before && now != registry::AIR {
                    // The wrong thing landed — most likely our own
                    // stale-held item. Reclaim it and try again.
                    last = format!("misplaced ({}); reclaimed it", self.reg.block(now).name);
                    if attempt < 2 {
                        self.break_block_at(pos)?;
                        self.pump_for(0.2);
                    }
                    break;
                }
            }
        }
        Err(last)
    }

    /// Block until the standing behavior finishes (arrival, failure,
    /// or timeout). Returns the last movement event line.
    pub fn wait_idle(&mut self, timeout_secs: f32) -> String {
        let mut waited = 0.0;
        while waited < timeout_secs {
            if matches!(self.behavior, Behavior::Idle) {
                break;
            }
            self.pump_for(0.1);
            waited += 0.1;
        }
        self.events
            .iter()
            .rev()
            .find(|e| e.starts_with("arrived") || e.starts_with("stuck"))
            .cloned()
            .unwrap_or_else(|| "still walking".into())
    }

    // ---- crafting (the click protocol, choreographed) ----

    /// One click, one echo: the host answers every InventoryClick
    /// with a PlayerState, and no later decision is safe until the
    /// answer lands. Lock-step beats optimism at any load.
    fn click_confirmed(&mut self, area: InventoryArea, slot: usize, right: bool) {
        let seen = self.echoes;
        self.send(&C2S::InventoryClick {
            area,
            slot: slot as u8,
            right,
        });
        for _ in 0..125 {
            self.pump_for(0.02);
            if self.echoes > seen {
                // One extra beat: pace the click rate under the
                // host's command budget even when echoes fly.
                self.pump_for(0.02);
                return;
            }
        }
        self.event("a click went unanswered; the pack may sit odd".into());
    }

    fn click(&mut self, area: InventoryArea, slot: usize) {
        self.click_confirmed(area, slot, false);
    }

    fn click_one(&mut self, area: InventoryArea, slot: usize) {
        self.click_confirmed(area, slot, true);
    }

    fn stash_cursor(&mut self) {
        if self.cursor.is_none() {
            return;
        }
        for i in 0..TOTAL_SLOTS {
            if self.inventory.slots[i].is_none() {
                self.click(InventoryArea::Inventory, i);
                if self.cursor.is_none() {
                    return;
                }
            }
        }
    }

    fn slot_of(&self, item: ItemId) -> Option<usize> {
        (0..TOTAL_SLOTS).find(|&i| self.inventory.slots[i].is_some_and(|s| s.item == item))
    }

    pub(crate) fn named_slot(&self, name: &str) -> Result<usize, String> {
        let item = self
            .reg
            .item_id(name)
            .ok_or_else(|| format!("unknown item {name}"))?;
        self.slot_of(item)
            .ok_or_else(|| format!("no {name} in the pack"))
    }

    pub fn discovery_holder_for_item(
        &self,
        name: &str,
    ) -> Result<crate::net::RecordHolderSnap, String> {
        Ok(crate::net::RecordHolderSnap::Inventory {
            slot: self.named_slot(name)? as u8,
        })
    }

    fn discovery_summary(record: &crate::discovery::ObservationSummary) -> String {
        let mut out = format!(
            "record {} | {} | {} | {}\nobserver: {} | day {} {}",
            record.record_id,
            record.phenomenon_id,
            record.category,
            record.reading,
            record.observer_name,
            record.day,
            record.season
        );
        for (property, value) in &record.properties {
            out.push_str(&format!("\n{property}: {value}"));
        }
        if let Some(label) = &record.label {
            out.push_str(&format!("\nlabel: {label}"));
        }
        if let Some(position) = record.location {
            out.push_str(&format!(
                "\nlocation: {} {} {} {}",
                position.face(),
                position.u(),
                position.y(),
                position.v()
            ));
        } else {
            out.push_str("\nlocation: omitted");
        }
        if record.obsolete_content {
            out.push_str("\ncontent version: obsolete");
        }
        out
    }

    pub fn observe_discovery(
        &mut self,
        target: Option<crate::planet::BlockPos>,
        ledger: &str,
        calibration: Option<&str>,
        label: Option<String>,
    ) -> Result<String, String> {
        self.select("base:tuning_lens")?;
        let ledger_slot = self.named_slot(ledger)?;
        let calibration_slot = calibration.map(|name| self.named_slot(name)).transpose()?;
        if let Some(pos) = target {
            if self.dist_to(pos) > REACH {
                return Err("target is out of reach".into());
            }
            self.face_block(pos);
        }
        self.anchor_stance();
        self.last_discovery = None;
        self.send(&C2S::BeginObserve {
            target: target.map_or(crate::net::DiscoveryTargetSnap::Region, |pos| {
                crate::net::DiscoveryTargetSnap::Block(pos)
            }),
        });
        self.pump_for(1.25);
        self.send(&C2S::Observe {
            target: target.map_or(crate::net::DiscoveryTargetSnap::Region, |pos| {
                crate::net::DiscoveryTargetSnap::Block(pos)
            }),
            ledger_slot: ledger_slot as u8,
            calibration_slot: calibration_slot.map(|slot| slot as u8),
            label,
        });
        for _ in 0..80 {
            self.pump_for(0.05);
            if let Some(record) = self.last_discovery.take() {
                return Ok(Self::discovery_summary(&record));
            }
        }
        Err("the host did not produce an observation".into())
    }

    pub fn read_knowledge(&mut self, item: &str) -> Result<String, String> {
        let slot = self.named_slot(item)?;
        self.last_knowledge_text = None;
        self.last_discovery_records = None;
        self.send(&C2S::ReadKnowledge { slot: slot as u8 });
        for _ in 0..40 {
            self.pump_for(0.05);
            if let Some(text) = self.last_knowledge_text.take() {
                return Ok(text);
            }
            if let Some((records, capacity)) = self.last_discovery_records.take() {
                let mut out = format!("records {}/{}", records.len(), capacity);
                for record in records {
                    out.push_str("\n\n");
                    out.push_str(&Self::discovery_summary(&record));
                }
                return Ok(out);
            }
        }
        Err("the host did not return readable knowledge".into())
    }

    pub fn read_folio(&mut self, pos: crate::planet::BlockPos) -> Result<String, String> {
        if self.dist_to(pos) > REACH {
            return Err("folio is out of reach".into());
        }
        self.face_block(pos);
        self.anchor_stance();
        self.last_discovery_records = None;
        self.send(&C2S::OpenDiscovery {
            holder: crate::net::RecordHolderSnap::Folio { pos },
        });
        for _ in 0..40 {
            self.pump_for(0.05);
            if let Some((records, capacity)) = self.last_discovery_records.take() {
                let mut out = format!("records {}/{}", records.len(), capacity);
                for record in records {
                    out.push_str("\n\n");
                    out.push_str(&Self::discovery_summary(&record));
                }
                return Ok(out);
            }
        }
        Err("the host did not return the folio".into())
    }

    pub fn copy_observation(
        &mut self,
        writing_pos: crate::planet::BlockPos,
        source: crate::net::RecordHolderSnap,
        record_id: u64,
        destination: crate::net::RecordHolderSnap,
        include_location: bool,
    ) -> Result<String, String> {
        self.last_discovery_records = None;
        self.send(&C2S::CopyObservation {
            writing_pos,
            source,
            record_id,
            destination,
            include_location,
        });
        for _ in 0..40 {
            self.pump_for(0.05);
            if self.last_discovery_records.take().is_some() {
                return Ok("observation copied by the host".into());
            }
        }
        Err("the host did not confirm the copy".into())
    }

    pub fn run_discovery_experiment(
        &mut self,
        pos: crate::planet::BlockPos,
        kind: crate::discovery::ExperimentKind,
        sample: &str,
        ledger: &str,
        calibration: Option<&str>,
    ) -> Result<String, String> {
        if self.dist_to(pos) > REACH {
            return Err("apparatus is out of reach".into());
        }
        let sample_slot = self.named_slot(sample)?;
        let ledger_slot = self.named_slot(ledger)?;
        let reference_slot = self.named_slot(kind.reference_item())?;
        let calibration_slot = calibration.map(|name| self.named_slot(name)).transpose()?;
        self.face_block(pos);
        self.anchor_stance();
        self.send(&C2S::SetExperimentItem {
            pos,
            slot: sample_slot as u8,
        });
        self.pump_for(0.35);
        self.send(&C2S::SetExperimentItem {
            pos,
            slot: reference_slot as u8,
        });
        self.pump_for(0.35);
        self.select("base:tuning_lens")?;
        self.anchor_stance();
        self.last_discovery = None;
        self.send(&C2S::BeginExperiment { pos, kind });
        self.pump_for(1.25);
        self.send(&C2S::RunExperiment {
            pos,
            kind,
            ledger_slot: ledger_slot as u8,
            calibration_slot: calibration_slot.map(|slot| slot as u8),
        });
        for _ in 0..80 {
            self.pump_for(0.05);
            if let Some(record) = self.last_discovery.take() {
                return Ok(Self::discovery_summary(&record));
            }
        }
        Err("the host did not complete the experiment".into())
    }

    pub fn assemble_discovery_lens(
        &mut self,
        pos: crate::planet::BlockPos,
    ) -> Result<String, String> {
        if self.dist_to(pos) > REACH {
            return Err("assembly bench is out of reach".into());
        }
        self.face_block(pos);
        self.anchor_stance();
        self.send(&C2S::AssembleTuningLens { pos });
        self.pump_for(0.4);
        self.named_slot("base:tuning_lens")?;
        Ok("tuning lens assembled".into())
    }

    pub fn operate_binding_frame(
        &mut self,
        pos: crate::planet::BlockPos,
        action: crate::implements::FrameAction,
        held_item: Option<&str>,
    ) -> Result<String, String> {
        if self.dist_to(pos) > REACH {
            return Err("binding frame is out of reach".into());
        }
        // Frame verbs share the player's host-owned action cooldown. The
        // previous helper returned as soon as the result arrived, so a
        // bounded multi-step assembly immediately sent its next verb while
        // the host was still cooling down and then waited forever for a
        // response the host had correctly refused. Pace before every verb;
        // this is transport choreography, not client authority.
        self.pump_for(0.3);
        if let Some(item) = held_item {
            self.select(item)?;
        }
        self.face_block(pos);
        self.anchor_stance();
        if action != crate::implements::FrameAction::Inspect
            && !self.binding_revisions.contains_key(&pos)
        {
            self.last_binding_frame = None;
            self.send(&C2S::OperateBindingFrame {
                pos,
                slot: self.hotbar as u8,
                action: crate::implements::FrameAction::Inspect,
                expected_revision: None,
            });
            for _ in 0..50 {
                self.pump_for(0.05);
                if self.binding_revisions.contains_key(&pos) {
                    break;
                }
            }
            if !self.binding_revisions.contains_key(&pos) {
                return Err("the host did not return the binding-frame revision".into());
            }
            // Inspect is itself a successful frame action and starts the same
            // host cooldown. Wait before issuing the requested mutation.
            self.pump_for(0.3);
        }
        self.last_binding_frame = None;
        self.send(&C2S::OperateBindingFrame {
            pos,
            slot: self.hotbar as u8,
            action,
            expected_revision: self.binding_revisions.get(&pos).copied(),
        });
        for _ in 0..50 {
            self.pump_for(0.05);
            if let Some((at, result)) = self.last_binding_frame.take()
                && at == pos
            {
                let mut text = result.message;
                for line in result.lines {
                    text.push('\n');
                    text.push_str(&line);
                }
                return if result.success { Ok(text) } else { Err(text) };
            }
        }
        Err("the host did not complete the binding-frame operation".into())
    }

    /// Perform one ordinary host-authoritative laboratory action and wait for
    /// its reliable revision/result echo. Agents use precisely the same
    /// station request as windowed guests.
    pub fn operate_alchemy(
        &mut self,
        pos: crate::planet::BlockPos,
        action: crate::alchemy::ApparatusAction,
        held_item: Option<&str>,
    ) -> Result<String, String> {
        if self.dist_to(pos) > REACH {
            return Err("alchemy apparatus is out of reach".into());
        }
        self.pump_for(0.2);
        if let Some(item) = held_item {
            self.select(item)?;
        }
        let action = match action {
            crate::alchemy::ApparatusAction::Grind { .. } => {
                crate::alchemy::ApparatusAction::Grind {
                    inventory_slot: self.hotbar as u8,
                }
            }
            crate::alchemy::ApparatusAction::LoadCarrier { .. } => {
                crate::alchemy::ApparatusAction::LoadCarrier {
                    inventory_slot: self.hotbar as u8,
                }
            }
            crate::alchemy::ApparatusAction::LoadFilter { .. } => {
                crate::alchemy::ApparatusAction::LoadFilter {
                    inventory_slot: self.hotbar as u8,
                }
            }
            crate::alchemy::ApparatusAction::Charge {
                inventory_slot: Some(_),
                units,
            } => crate::alchemy::ApparatusAction::Charge {
                inventory_slot: Some(self.hotbar as u8),
                units,
            },
            crate::alchemy::ApparatusAction::Decant { .. } => {
                crate::alchemy::ApparatusAction::Decant {
                    vessel_slot: self.hotbar as u8,
                }
            }
            crate::alchemy::ApparatusAction::Clean { filter_slot, .. } => {
                crate::alchemy::ApparatusAction::Clean {
                    water_slot: self.hotbar as u8,
                    filter_slot,
                }
            }
            crate::alchemy::ApparatusAction::Repair { .. } => {
                crate::alchemy::ApparatusAction::Repair {
                    material_slot: self.hotbar as u8,
                }
            }
            crate::alchemy::ApparatusAction::PressOil { .. } => {
                crate::alchemy::ApparatusAction::PressOil {
                    seed_slot: self.hotbar as u8,
                }
            }
            action => action,
        };
        self.face_block(pos);
        self.anchor_stance();
        if !matches!(action, crate::alchemy::ApparatusAction::Inspect)
            && !self.alchemy_revisions.contains_key(&pos)
        {
            self.last_alchemy_result = None;
            self.send(&C2S::OperateAlchemy {
                pos,
                expected_revision: None,
                action: crate::alchemy::ApparatusAction::Inspect,
            });
            for _ in 0..50 {
                self.pump_for(0.05);
                if self.alchemy_revisions.contains_key(&pos) {
                    break;
                }
            }
            if !self.alchemy_revisions.contains_key(&pos) {
                return Err("the host did not return the apparatus revision".into());
            }
            self.pump_for(0.2);
        }
        self.last_alchemy_result = None;
        self.send(&C2S::OperateAlchemy {
            pos,
            expected_revision: self.alchemy_revisions.get(&pos).copied(),
            action,
        });
        for _ in 0..60 {
            self.pump_for(0.05);
            if let Some((at, result)) = self.last_alchemy_result.take()
                && at == pos
            {
                return Ok(format!(
                    "{}; revision {}; batch {}; volume {}; next {:?}",
                    result.cue.message,
                    result.revision,
                    result.batch_id,
                    result.volume_units,
                    result.next_step
                ));
            }
        }
        Err("the host did not complete the alchemy operation".into())
    }

    pub fn apply_preparation(
        &mut self,
        item: &str,
        target: crate::alchemy::AlchemyTarget,
    ) -> Result<String, String> {
        let slot = self.named_slot(item)?;
        self.hotbar = slot.min(crate::inventory::HOTBAR_SLOTS - 1);
        self.anchor_stance();
        self.last_preparation_result = None;
        self.send(&C2S::UsePreparation {
            slot: slot as u8,
            target,
        });
        for _ in 0..60 {
            self.pump_for(0.05);
            if let Some(result) = self.last_preparation_result.take() {
                return Ok(result.message);
            }
        }
        Err("the host did not complete the preparation application".into())
    }

    /// Aim and begin one host-authoritative wand working or constructed
    /// ritual. The agent sends the same intent packet as a windowed guest and
    /// waits for the ordinary result/cues; no privileged world mutation path
    /// exists here.
    pub fn start_working(
        &mut self,
        working_id: &str,
        target: crate::workings::WorkingTargetIntent,
        held_item: Option<&str>,
        forced: bool,
    ) -> Result<String, String> {
        use crate::workings::WorkingTargetIntent;
        if self.active_working_request.is_some() {
            return Err("finish or cancel the active working first".into());
        }
        let held_instance = if matches!(target, WorkingTargetIntent::Ritual { .. }) {
            0
        } else {
            if let Some(item) = held_item {
                self.select(item)?;
            }
            self.inventory.slots[self.hotbar]
                .filter(|stack| stack.arcane_id != 0)
                .map(|stack| stack.arcane_id)
                .ok_or("select a physical charged wand before starting the working")?
        };
        let aim = match target {
            WorkingTargetIntent::Block { pos, .. } => Some(pos),
            WorkingTargetIntent::Water { from, .. } => Some(from),
            WorkingTargetIntent::Ritual { controller } => Some(controller),
            WorkingTargetIntent::None
            | WorkingTargetIntent::Entity { .. }
            | WorkingTargetIntent::Inventory { .. } => None,
        };
        if let Some(pos) = aim {
            if self.dist_to(pos) > REACH {
                return Err("working target is out of reach".into());
            }
            self.face_block(pos);
        }
        self.anchor_stance();
        self.last_working_result = None;
        self.send(&C2S::OperateWorking {
            working_id: working_id.into(),
            held_instance,
            target,
            intent: if forced {
                crate::workings::WorkingIntent::StartForced
            } else {
                crate::workings::WorkingIntent::Start
            },
        });
        for _ in 0..80 {
            self.pump_for(0.05);
            if let Some(result) = self.last_working_result.take() {
                if result.success && result.phase.is_some() {
                    self.active_working_request = Some((working_id.into(), held_instance, target));
                }
                return if result.success {
                    Ok(result.message)
                } else {
                    Err(result.message)
                };
            }
        }
        Err("the host did not answer the working start request".into())
    }

    pub fn continue_working(
        &mut self,
        intent: crate::workings::WorkingIntent,
    ) -> Result<String, String> {
        if matches!(
            intent,
            crate::workings::WorkingIntent::Start | crate::workings::WorkingIntent::StartForced
        ) {
            return Err("use start_working for a new working".into());
        }
        let (working_id, held_instance, target) = self
            .active_working_request
            .clone()
            .ok_or("there is no active working to hold, release, or cancel")?;
        self.last_working_result = None;
        self.send(&C2S::OperateWorking {
            working_id,
            held_instance,
            target,
            intent,
        });
        for _ in 0..80 {
            self.pump_for(0.05);
            if let Some(result) = self.last_working_result.take() {
                if result.success && result.phase.is_none() {
                    self.active_working_request = None;
                }
                return if result.success {
                    Ok(result.message)
                } else {
                    Err(result.message)
                };
            }
        }
        Err("the host did not answer the working intent".into())
    }

    /// Craft one of the recipes the agent knows the shape of. The
    /// host re-validates the grid; a 3x3 shape honestly requires a
    /// crafting table within reach even though we hold the grid.
    pub fn craft(&mut self, what: &str, times: u32) -> Result<String, String> {
        let times = times.clamp(1, 16);
        let find = |a: &Agent, names: &[&str]| -> Option<ItemId> {
            names
                .iter()
                .find_map(|n| a.reg.item_id(n).filter(|i| a.slot_of(*i).is_some()))
        };
        let logs: Vec<&str> = vec![
            "base:log",
            "base:birch_log",
            "base:spruce_log",
            "base:jungle_log",
            "base:acacia_log",
        ];
        let planks: Vec<&str> = vec![
            "base:planks",
            "base:birch_planks",
            "base:spruce_planks",
            "base:jungle_planks",
            "base:acacia_planks",
        ];
        match what {
            "planks" => {
                let log = find(self, &logs).ok_or("no logs in the pack")?;
                let src = self.slot_of(log).unwrap();
                self.click(InventoryArea::Inventory, src);
                self.click(InventoryArea::Craft, 0);
                for _ in 0..times {
                    self.send(&C2S::CraftResult { size: 2 });
                    self.pump_for(0.1);
                }
                self.stash_cursor();
                self.click(InventoryArea::Craft, 0);
                self.stash_cursor();
                Ok(format!("cut planks x{times}"))
            }
            "stick" => {
                let plank = find(self, &planks).ok_or("no planks in the pack")?;
                for _ in 0..times {
                    let src = self.slot_of(plank).ok_or("ran out of planks")?;
                    self.click(InventoryArea::Inventory, src);
                    self.click_one(InventoryArea::Craft, 0);
                    self.click_one(InventoryArea::Craft, 2);
                    self.stash_cursor();
                    self.send(&C2S::CraftResult { size: 2 });
                    self.pump_for(0.1);
                    self.stash_cursor();
                }
                Ok(format!("whittled sticks x{times}"))
            }
            "crafting_table" => {
                let plank = find(self, &planks).ok_or("no planks in the pack")?;
                let src = self.slot_of(plank).unwrap();
                self.click(InventoryArea::Inventory, src);
                for slot in [0, 1, 2, 3] {
                    self.click_one(InventoryArea::Craft, slot);
                }
                self.stash_cursor();
                self.send(&C2S::CraftResult { size: 2 });
                self.pump_for(0.1);
                self.stash_cursor();
                Ok("built a crafting table".into())
            }
            "chest" => {
                // An honest 3x3: the host doesn't check for a table,
                // but this agent is a guest, not a god. The mirror can
                // lag its own placement under load — give it a beat.
                let mut near = self.nearest("base:crafting_table", 5);
                for _ in 0..40 {
                    if !near.starts_with("no ") {
                        break;
                    }
                    self.pump_for(0.05);
                    near = self.nearest("base:crafting_table", 5);
                }
                if near.starts_with("no ") {
                    return Err("a chest is 3x3 work: stand by a crafting table".into());
                }
                let plank = find(self, &planks).ok_or("no planks in the pack")?;
                let src = self.slot_of(plank).unwrap();
                if self.inventory.slots[src].is_none_or(|s| s.count < 8) {
                    return Err("a chest wants 8 planks".into());
                }
                self.click(InventoryArea::Inventory, src);
                for slot in [0, 1, 2, 3, 5, 6, 7, 8] {
                    self.click_one(InventoryArea::Craft, slot);
                }
                self.stash_cursor();
                self.send(&C2S::CraftResult { size: 3 });
                self.pump_for(0.1);
                self.stash_cursor();
                for slot in [0, 1, 2, 3, 5, 6, 7, 8] {
                    self.click(InventoryArea::Craft, slot);
                    self.stash_cursor();
                }
                Ok("joined a chest".into())
            }
            other => Err(format!(
                "I only know planks, stick, crafting_table, chest (asked: {other})"
            )),
        }
    }

    /// Empty matching stacks into a chest through the transactional
    /// click protocol; the HeldResult echo confirms every move.
    pub fn deposit(
        &mut self,
        pos: crate::planet::BlockPos,
        only: Option<&str>,
    ) -> Result<String, String> {
        if self.dist_to(pos) > REACH {
            return Err("out of reach of the chest".into());
        }
        let filter = match only {
            Some(n) => Some(self.reg.item_id(n).ok_or(format!("unknown item {n}"))?),
            None => None,
        };
        self.send(&C2S::OpenContainer { pos });
        self.pump_for(0.3);
        let mut moved = 0u32;
        for slot in 0..TOTAL_SLOTS {
            let Some(stack) = self.inventory.slots[slot] else {
                continue;
            };
            if filter.is_some_and(|f| stack.item != f) {
                continue;
            }
            let n = stack.count;
            self.click(InventoryArea::Inventory, slot);
            for chest_slot in 0..crate::world::CHEST_SLOTS {
                if self.cursor.is_none() {
                    break;
                }
                self.send(&C2S::ContainerClick {
                    pos,
                    slot: chest_slot as u8,
                    right: false,
                });
                self.pump_for(0.1);
                // A swap pulled something out: put it straight back.
                if self.cursor.is_some_and(|c| c.item != stack.item) {
                    self.send(&C2S::ContainerClick {
                        pos,
                        slot: chest_slot as u8,
                        right: false,
                    });
                    self.pump_for(0.1);
                }
            }
            if self.cursor.is_none() {
                moved += n;
            } else {
                self.stash_cursor();
            }
        }
        self.send(&C2S::CloseContainer);
        self.pump_for(0.1);
        Ok(format!("stowed {moved} items in the chest"))
    }

    /// Rest the held item on a station / take work back.
    pub fn station_put(&mut self, pos: crate::planet::BlockPos, item: &str) -> Result<(), String> {
        if self.dist_to(pos) > REACH {
            return Err("out of reach".into());
        }
        self.select(item)?;
        self.pump_for(0.1);
        self.send(&C2S::AnvilPut { pos });
        self.pump_for(0.2);
        Ok(())
    }

    pub fn station_take(&mut self, pos: crate::planet::BlockPos) -> Result<(), String> {
        if self.dist_to(pos) > REACH {
            return Err("out of reach".into());
        }
        self.send(&C2S::AnvilTake { pos });
        self.pump_for(0.3);
        Ok(())
    }

    pub fn eat(&mut self, item: &str) -> Result<(), String> {
        self.select(item)?;
        self.pump_for(0.1);
        self.send(&C2S::EatSelected);
        self.pump_for(0.3);
        Ok(())
    }
}
