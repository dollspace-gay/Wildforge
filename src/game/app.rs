//! Winit application lifecycle and platform event bridge.

use crate::world::TerrainRead;

use super::{BUILD_MARKER, Game, Screen};
use crate::inventory::HOTBAR_SLOTS;
use crate::raycast;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{
    DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

const SHOT_MIN_WIDTH: u32 = 320;
const SHOT_MIN_HEIGHT: u32 = 200;
const SHOT_MAX_DIMENSION: u32 = 4096;
const SHOT_MAX_PIXELS: u64 = 16_777_216;

fn parse_shot_size(value: &str) -> Result<PhysicalSize<u32>, String> {
    let Some((width, height)) = value.trim().split_once(['x', 'X']) else {
        return Err("expected <width>x<height>".into());
    };
    let width = width
        .trim()
        .parse::<u32>()
        .map_err(|_| "width is not an unsigned integer")?;
    let height = height
        .trim()
        .parse::<u32>()
        .map_err(|_| "height is not an unsigned integer")?;
    if !(SHOT_MIN_WIDTH..=SHOT_MAX_DIMENSION).contains(&width)
        || !(SHOT_MIN_HEIGHT..=SHOT_MAX_DIMENSION).contains(&height)
    {
        return Err(format!(
            "dimensions must be within {SHOT_MIN_WIDTH}x{SHOT_MIN_HEIGHT} and \
             {SHOT_MAX_DIMENSION}x{SHOT_MAX_DIMENSION}"
        ));
    }
    if u64::from(width) * u64::from(height) > SHOT_MAX_PIXELS {
        return Err(format!("capture exceeds the {SHOT_MAX_PIXELS}-pixel limit"));
    }
    Ok(PhysicalSize::new(width, height))
}

/// An exact physical-pixel override for automated screenshots. The variable is
/// deliberately ignored without WILDFORGE_SHOT, so it can neither resize
/// ordinary play nor enter persistent Config.
fn requested_shot_size() -> Option<PhysicalSize<u32>> {
    std::env::var_os("WILDFORGE_SHOT")?;
    let value = std::env::var("WILDFORGE_SHOT_SIZE").ok()?;
    Some(
        parse_shot_size(&value)
            .unwrap_or_else(|error| panic!("invalid WILDFORGE_SHOT_SIZE={value:?}: {error}")),
    )
}

#[derive(Default)]
pub(super) struct App {
    game: Option<Game>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.game.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title(format!("Wildforge {BUILD_MARKER} — loading world…"));
        let attributes = if let Some(size) = requested_shot_size() {
            attributes.with_inner_size(size)
        } else {
            attributes.with_inner_size(LogicalSize::new(1280, 720))
        };
        let window = Arc::new(event_loop.create_window(attributes).expect("create window"));
        let mut game = Game::new(window);
        // Headless/dev: WILDFORGE_WORLD=name skips the title screen.
        if let Ok(name) = std::env::var("WILDFORGE_WORLD") {
            game.start_world(&name);
        }
        // Headless/dev: WILDFORGE_JOIN=addr joins a host directly
        // (screenshots of multiplayer scenes, agent playtests).
        if let Ok(addr) = std::env::var("WILDFORGE_JOIN")
            && let Ok(addr) = addr.parse()
        {
            game.join_server(addr);
        }
        self.game = Some(game);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => {
                if game.in_world
                    && let Err(error) = game.save_session()
                {
                    eprintln!("world: close cancelled because save failed: {error}");
                    game.toast(format!("Could not save; close cancelled: {error}"));
                    return;
                }
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                game.renderer.resize(size.width, size.height);
                game.camera.aspect = size.width as f32 / size.height.max(1) as f32;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                game.keyboard_event(&event, event_loop);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                if !pressed {
                    game.ui_state.dragging_slider = None;
                }
                // Menu screens take clicks directly.
                if game.ui_state.screen != Screen::Playing {
                    if pressed {
                        game.presentation.press_dip = 0.07;
                        match button {
                            MouseButton::Left => game.menu_click(event_loop, false),
                            MouseButton::Right => game.menu_click(event_loop, true),
                            _ => {}
                        }
                    }
                    return;
                }
                if !game.input.captured() {
                    if pressed {
                        game.input.capture(&game.window, true);
                    }
                    return;
                }
                match button {
                    MouseButton::Left => {
                        game.input.left_held = pressed;
                        if pressed {
                            game.presentation.swing = 1.0;
                        } else {
                            game.interaction.breaking = None;
                        }
                    }
                    MouseButton::Right => {
                        game.input.right_held = pressed;
                        if pressed {
                            game.input.action_cooldown = 0.0;
                            game.presentation.swing = 1.0;
                        }
                    }
                    MouseButton::Middle if pressed => {
                        if let Some(h) = raycast::raycast_at(
                            &game.runtime.view(),
                            game.player.eye(),
                            game.camera.local_forward(),
                            game.reach(),
                        ) {
                            let b = game.runtime.view().get_block_at(h.block);
                            let reg = game.content.reg.clone();
                            let found = game.inventory.slots[..HOTBAR_SLOTS]
                                .iter()
                                .position(|s| s.map(|s| reg.item(s.item).places) == Some(Some(b)));
                            if let Some(i) = found {
                                if game.input.hotbar_sel != i {
                                    game.presentation.sel_bounce = 0.0;
                                }
                                game.input.hotbar_sel = i;
                            }
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // WSLg can synthesize a scroll event as the window opens.
                if game.total_frames < 30 || game.ui_state.screen != Screen::Playing {
                    return;
                }
                // Some stacks fire many small wheel events per physical notch;
                // accumulate and step one hotbar slot per whole notch.
                game.input.scroll_accum += match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 120.0,
                };
                // One slot per notch, rate-limited: platforms like WSLg fire
                // multiple events per physical notch.
                let steps = game.input.scroll_accum.trunc() as i32;
                if steps != 0 {
                    if game.camera.mode == crate::camera::CameraMode::Orbit {
                        // The factory camera zooms instead of flipping the
                        // hotbar.
                        game.camera.orbit_dist =
                            (game.camera.orbit_dist - steps as f32 * 0.8).clamp(2.0, 20.0);
                    } else if game.input.scroll_cooldown <= 0.0 {
                        let n = HOTBAR_SLOTS as i32;
                        let sel = (game.input.hotbar_sel as i32 - steps.signum()).rem_euclid(n);
                        game.input.hotbar_sel = sel as usize;
                        game.presentation.sel_bounce = 0.0;
                        game.input.scroll_cooldown = 0.15;
                    }
                    game.input.scroll_accum = 0.0;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if !game.input.cursor_locked {
                    game.input.ui_cursor = (position.x as f32, position.y as f32);
                }
                if let Some(i) = game.ui_state.dragging_slider {
                    let (bx, _, bw, _) = game.slider_bar_rect(i);
                    game.set_slider(i, (position.x as f32 - bx - 2.0) / (bw - 4.0));
                }
                if game.input.captured()
                    && !game.input.uses_raw_look()
                    && game.ui_state.screen == Screen::Playing
                {
                    game.input
                        .cursor_look(&game.window, &mut game.camera, position);
                }
            }
            // Crossing the window boundary teleports the cursor; never treat
            // that jump as look motion.
            WindowEvent::CursorEntered { .. } | WindowEvent::CursorLeft { .. } => {
                game.input.cursor_boundary();
            }
            WindowEvent::Focused(false) => {
                if game.ui_state.screen == Screen::Playing {
                    game.input.capture(&game.window, false);
                }
                game.input.clear_held();
                game.interaction.breaking = None;
            }
            WindowEvent::RedrawRequested => {
                game.update();
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event
            && let Some(game) = self.game.as_mut()
            && game.input.captured()
            && game.input.uses_raw_look()
            && game.ui_state.screen == Screen::Playing
        {
            game.camera.turn(dx as f32, dy as f32);
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(game) = self.game.as_ref() {
            game.window.request_redraw();
        }
    }
}

/// Start the platform event loop and windowed client.
pub fn run_windowed() {
    // Prefer X11/XWayland on Linux: it supports cursor confinement and
    // warping, which pure Wayland compositors (notably WSLg) often don't.
    #[cfg(target_os = "linux")]
    let event_loop = {
        use winit::platform::x11::EventLoopBuilderExtX11;
        let mut builder = EventLoop::builder();
        if std::env::var("DISPLAY").is_ok() {
            builder.with_x11();
        }
        builder.build().expect("create event loop")
    };
    #[cfg(not(target_os = "linux"))]
    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::default();
    event_loop.run_app(&mut app).expect("run event loop");
}

#[cfg(test)]
mod tests {
    use super::parse_shot_size;

    #[test]
    fn capture_size_parser_is_exact_and_bounded() {
        let full_hd = parse_shot_size("1920x1080").unwrap();
        assert_eq!((full_hd.width, full_hd.height), (1920, 1080));
        let upper = parse_shot_size("4096X4096").unwrap();
        assert_eq!((upper.width, upper.height), (4096, 4096));

        for invalid in [
            "1920",
            "x1080",
            "319x1080",
            "1920x199",
            "4097x1080",
            "4096x4097",
            "4096x4096x1",
        ] {
            assert!(
                parse_shot_size(invalid).is_err(),
                "invalid capture size accepted: {invalid}"
            );
        }
    }
}
