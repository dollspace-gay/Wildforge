//! Held input and pointer capture. Capture/warp state has a single owner.

use winit::window::{CursorGrabMode, Window};
use crate::camera::Camera;

#[derive(Default)]
pub(super) struct KeysDown {
    pub(super) w: bool,
    pub(super) a: bool,
    pub(super) s: bool,
    pub(super) d: bool,
    pub(super) space: bool,
    pub(super) sprint: bool,
    pub(super) block: bool,
}

/// Pointer, keyboard, capture, and input-rate state that resets together.
pub(super) struct InputState {
    pub(super) keys: KeysDown,
    mouse_captured: bool,
    raw_look: bool,
    last_cursor: Option<(f64, f64)>,
    warp_pending: bool,
    allow_warp: bool,
    pub(super) left_held: bool,
    pub(super) right_held: bool,
    /// Edge-triggered dodge request, consumed by `advance_player`.
    pub(super) dodge_pressed: bool,
    pub(super) action_cooldown: f32,
    pub(super) attack_cooldown: f32,
    pub(super) hotbar_sel: usize,
    pub(super) scroll_accum: f32,
    pub(super) scroll_cooldown: f32,
    pub(super) ui_cursor: (f32, f32),
    /// WILDFORGE_CURSOR parked the pointer for a headless capture;
    /// the window's synthetic CursorMoved events must not undo it.
    pub(super) cursor_locked: bool,
}

impl InputState {
    pub(super) fn new() -> Self {
        Self {
                keys: KeysDown::default(),
                mouse_captured: false,
                raw_look: false,
                last_cursor: None,
                warp_pending: false,
                allow_warp: std::env::var("WSL_DISTRO_NAME").is_err()
                    && !std::path::Path::new("/mnt/wslg").exists(),
                left_held: false,
                right_held: false,
                dodge_pressed: false,
                action_cooldown: 0.0,
                attack_cooldown: 0.0,
                hotbar_sel: 0,
                scroll_accum: 0.0,
                scroll_cooldown: 0.0,
                ui_cursor: (0.0, 0.0),
                cursor_locked: false,
        }
    }

    pub(super) fn captured(&self) -> bool {
        self.mouse_captured
    }

    pub(super) fn uses_raw_look(&self) -> bool {
        self.raw_look
    }

    pub(super) fn cursor_boundary(&mut self) {
        self.last_cursor = None;
    }

    pub(super) fn clear_held(&mut self) {
        self.keys = KeysDown::default();
        self.left_held = false;
        self.right_held = false;
    }

    pub(super) fn capture(&mut self, window: &Window, capture: bool) {
        if capture {
            // A Locked grab pins the cursor: raw deltas are the only signal.
            // Anything less (Confined, or no grab at all): use cursor-position
            // deltas + recentering instead — raw deltas are unreliable on some
            // stacks (notably WSLg's XWayland).
            self.raw_look = window.set_cursor_grab(CursorGrabMode::Locked).is_ok();
            if !self.raw_look {
                let _ = window.set_cursor_grab(CursorGrabMode::Confined);
                self.last_cursor = None;
                self.warp_pending =
                    self.allow_warp && window.set_cursor_position(Self::center(window)).is_ok();
            }
        } else {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
        }
        window.set_cursor_visible(!capture);
        self.mouse_captured = capture;
    }

    fn center(window: &Window) -> winit::dpi::PhysicalPosition<f64> {
        let size = window.inner_size();
        winit::dpi::PhysicalPosition::new(size.width as f64 / 2.0, size.height as f64 / 2.0)
    }

    /// Cursor-position-based look using successive position differences —
    /// exact 1:1 deltas even when events are coalesced into big jumps.
    /// The cursor is kept pinned in a small bubble around the window center;
    /// the warp's own event is recognized by landing exactly on center, so
    /// real motion events are never swallowed.
    pub(super) fn cursor_look(&mut self, window: &Window, camera: &mut Camera, pos: winit::dpi::PhysicalPosition<f64>) {
        let c = Self::center(window);
        if self.warp_pending && (pos.x - c.x).abs() < 1.5 && (pos.y - c.y).abs() < 1.5 {
            self.warp_pending = false;
            self.last_cursor = Some((c.x, c.y));
            return;
        }
        if let Some((lx, ly)) = self.last_cursor {
            camera.turn((pos.x - lx) as f32, (pos.y - ly) as f32);
        }
        self.last_cursor = Some((pos.x, pos.y));

        if self.allow_warp
            && ((pos.x - c.x).abs() > 40.0 || (pos.y - c.y).abs() > 40.0)
            && window.set_cursor_position(c).is_ok()
        {
            self.warp_pending = true;
        }
    }

}
