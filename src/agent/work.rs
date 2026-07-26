//! Work macros: multi-step jobs built only from player verbs. Every
//! send here is a request the host validates — reach, rate, held
//! item — and every step waits for the world's echo, not its own
//! optimism.

use super::*;
use crate::net::{C2S, InventoryArea};

const REACH: f32 = 6.5; // stay inside the host's 7.0

impl Agent {
    fn dist_to(&self, x: i32, y: i32, z: i32) -> f32 {
        (Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5)
            - (self.player.pos + Vec3::new(0.0, 1.0, 0.0)))
        .length()
    }

    fn face(&mut self, x: i32, z: i32) {
        let d = Vec3::new(
            x as f32 + 0.5 - self.player.pos.x,
            0.0,
            z as f32 + 0.5 - self.player.pos.z,
        );
        if d.length() > 0.01 {
            self.yaw = d.z.atan2(d.x);
        }
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
    pub fn break_block(&mut self, x: i32, y: i32, z: i32) -> Result<(), String> {
        if self.dist_to(x, y, z) > REACH {
            return Err("out of reach".into());
        }
        let before = self.world.get_block(x, y, z);
        if before == registry::AIR {
            return Err("nothing there".into());
        }
        self.face(x, z);
        self.anchor_stance();
        self.send(&C2S::Break { x, y, z });
        // Triple-size echo budget: parallel test load can starve the
        // host pump well past a polite wait (green runs exit early).
        for _ in 0..120 {
            self.pump_for(0.05);
            if self.world.get_block(x, y, z) != before {
                return Ok(());
            }
        }
        Err("the host didn't allow that break".into())
    }

    /// Walk to a tree and fell what's reachable of its trunk; the
    /// drops arrive over the wire as the host awards them.
    pub fn chop(&mut self, x: i32, y: i32, z: i32) -> Result<String, String> {
        let logs = self.reg.tags.get("base:logs").cloned().unwrap_or_default();
        let is_log = |a: &Agent, cx: i32, cy: i32, cz: i32| {
            let b = a.world.get_block(cx, cy, cz);
            a.reg
                .item_id(&a.reg.block(b).name)
                .is_some_and(|i| logs.contains(&i))
        };
        if !is_log(self, x, y, z) {
            return Err("that isn't a log".into());
        }
        // Walk down to the trunk base, then stand beside it.
        let mut base = y;
        while base > 1 && is_log(self, x, base - 1, z) {
            base -= 1;
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
                let c = (x + dx, base + dy, z + dz);
                self.stands(c.0, c.1, c.2).then_some(c)
            })
        })
        .ok_or("nowhere to stand at that tree")?;
        self.go_to(spot)?;
        self.wait_idle(30.0);
        let mut felled = 0;
        for cy in base..base + 8 {
            if !is_log(self, x, cy, z) {
                break;
            }
            if self.dist_to(x, cy, z) > REACH {
                break;
            }
            self.break_block(x, cy, z)?;
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
    pub fn place(&mut self, x: i32, y: i32, z: i32, item: &str) -> Result<(), String> {
        if self.dist_to(x, y, z) > REACH {
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
            let before = self.world.get_block(x, y, z);
            if Some(before) == want {
                return Ok(()); // a slow echo: the last attempt landed
            }
            if before != registry::AIR {
                return Err("that cell is occupied".into());
            }
            self.face(x, z);
            self.anchor_stance();
            self.send(&C2S::Place { x, y, z });
            for _ in 0..90 {
                self.pump_for(0.05);
                let now = self.world.get_block(x, y, z);
                if Some(now) == want {
                    return Ok(());
                }
                if now != before && now != registry::AIR {
                    // The wrong thing landed — most likely our own
                    // stale-held item. Reclaim it and try again.
                    last = format!("misplaced ({}); reclaimed it", self.reg.block(now).name);
                    if attempt < 2 {
                        self.break_block(x, y, z)?;
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
        x: i32,
        y: i32,
        z: i32,
        only: Option<&str>,
    ) -> Result<String, String> {
        if self.dist_to(x, y, z) > REACH {
            return Err("out of reach of the chest".into());
        }
        let filter = match only {
            Some(n) => Some(self.reg.item_id(n).ok_or(format!("unknown item {n}"))?),
            None => None,
        };
        self.send(&C2S::OpenContainer { x, y, z });
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
                    x,
                    y,
                    z,
                    slot: chest_slot as u8,
                    right: false,
                });
                self.pump_for(0.1);
                // A swap pulled something out: put it straight back.
                if self.cursor.is_some_and(|c| c.item != stack.item) {
                    self.send(&C2S::ContainerClick {
                        x,
                        y,
                        z,
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
    pub fn station_put(&mut self, x: i32, y: i32, z: i32, item: &str) -> Result<(), String> {
        if self.dist_to(x, y, z) > REACH {
            return Err("out of reach".into());
        }
        self.select(item)?;
        self.pump_for(0.1);
        self.send(&C2S::AnvilPut { x, y, z });
        self.pump_for(0.2);
        Ok(())
    }

    pub fn station_take(&mut self, x: i32, y: i32, z: i32) -> Result<(), String> {
        if self.dist_to(x, y, z) > REACH {
            return Err("out of reach".into());
        }
        self.send(&C2S::AnvilTake { x, y, z });
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
