//! Pointer capture, cursor look, and screen/input state transitions.

use super::*;

impl Game {
    pub(super) fn rand01(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.rng >> 8) as f32 / (1 << 24) as f32
    }

    pub(super) fn capture_mouse(&mut self, capture: bool) {
        if capture {
            // A Locked grab pins the cursor: raw deltas are the only signal.
            // Anything less (Confined, or no grab at all): use cursor-position
            // deltas + recentering instead — raw deltas are unreliable on some
            // stacks (notably WSLg's XWayland).
            self.input.raw_look = self.window.set_cursor_grab(CursorGrabMode::Locked).is_ok();
            if !self.input.raw_look {
                let _ = self.window.set_cursor_grab(CursorGrabMode::Confined);
                self.input.last_cursor = None;
                self.input.warp_pending =
                    self.input.allow_warp && self.window.set_cursor_position(self.center()).is_ok();
            }
        } else {
            let _ = self.window.set_cursor_grab(CursorGrabMode::None);
        }
        self.window.set_cursor_visible(!capture);
        self.input.mouse_captured = capture;
    }

    pub(super) fn center(&self) -> winit::dpi::PhysicalPosition<f64> {
        let size = self.window.inner_size();
        winit::dpi::PhysicalPosition::new(size.width as f64 / 2.0, size.height as f64 / 2.0)
    }

    /// Cursor-position-based look using successive position differences —
    /// exact 1:1 deltas even when events are coalesced into big jumps.
    /// The cursor is kept pinned in a small bubble around the window center;
    /// the warp's own event is recognized by landing exactly on center, so
    /// real motion events are never swallowed.
    pub(super) fn cursor_look(&mut self, pos: winit::dpi::PhysicalPosition<f64>) {
        let c = self.center();
        if self.input.warp_pending && (pos.x - c.x).abs() < 1.5 && (pos.y - c.y).abs() < 1.5 {
            self.input.warp_pending = false;
            self.input.last_cursor = Some((c.x, c.y));
            return;
        }
        if let Some((lx, ly)) = self.input.last_cursor {
            self.camera.turn((pos.x - lx) as f32, (pos.y - ly) as f32);
        }
        self.input.last_cursor = Some((pos.x, pos.y));

        if self.input.allow_warp
            && ((pos.x - c.x).abs() > 40.0 || (pos.y - c.y).abs() > 40.0)
            && self.window.set_cursor_position(c).is_ok()
        {
            self.input.warp_pending = true;
        }
    }

    pub(super) fn set_screen(&mut self, screen: Screen) {
        if self.ui_state.screen == screen {
            return;
        }
        self.presentation.screen_age = 0.0;
        self.interaction.bow_draw = 0.0; // opening any screen relaxes the draw

        // Leaving a container tells the host to stop streaming it.
        if matches!(
            self.ui_state.screen,
            Screen::Furnace(_)
                | Screen::Chest(_)
                | Screen::Offering(_)
                | Screen::Bloomery(_)
                | Screen::Kiln(_)
        ) && let Some(r) = &self.multiplayer.remote
        {
            r.client.send(&net::C2S::CloseContainer);
        }
        // Leaving the inventory returns the cursor-held stack and craft grid.
        if self.ui_state.screen == Screen::Inventory
            || matches!(
                self.ui_state.screen,
                Screen::Furnace(_)
                    | Screen::Chest(_)
                    | Screen::Offering(_)
                    | Screen::Bloomery(_)
                    | Screen::Kiln(_)
            )
        {
            let mut back: Vec<ItemStack> = self.ui_state.held_stack.take().into_iter().collect();
            for slot in self.interaction.craft_grid.iter_mut() {
                if let Some(s) = slot.take() {
                    back.push(s);
                }
            }
            let reg = self.content.reg.clone();
            for s in back {
                let left = self.inventory.add_stack(&reg, s);
                if left > 0 {
                    self.drop_stack(ItemStack { count: left, ..s });
                }
            }
        }
        if screen == Screen::Inventory {
            self.ui_state.inventory_status_open = false;
            // Creative mode uses the browser as its item source. In survival
            // it is secondary help, so keep it tucked away until requested.
            self.ui_state.inventory_browser_open = self.creative;
        } else {
            self.ui_state.search_focus = false;
            self.ui_state.browse_view = None;
        }
        self.ui_state.screen = screen;
        let playing = screen == Screen::Playing;
        if playing {
            self.capture_mouse(true);
        } else {
            self.capture_mouse(false);
            self.input.keys = KeysDown::default();
            self.input.left_held = false;
            self.input.right_held = false;
            self.interaction.breaking = None;
        }
    }
}
