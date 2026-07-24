//! Winit application lifecycle and platform event bridge.

use super::*;

#[derive(Default)]
pub(super) struct App {
    game: Option<Game>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.game.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Wildforge — loading world…")
                        .with_inner_size(LogicalSize::new(1280, 720)),
                )
                .expect("create window"),
        );
        let mut game = Game::new(window);
        // Headless/dev: WILDFORGE_WORLD=name skips the title screen.
        if let Ok(name) = std::env::var("WILDFORGE_WORLD") {
            game.start_world(&name);
        }
        self.game = Some(game);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => {
                if game.in_world {
                    game.save_player();
                    game.server.world.settle_falling();
                    game.server.world.save_modified();
                }
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                game.renderer.resize(size.width, size.height);
                game.camera.aspect = size.width as f32 / size.height.max(1) as f32;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // First-run/profile and ATProto account text entry. OAuth
                // itself runs on a worker so the render/event loop stays live.
                if game.ui_state.screen == Screen::Accounts && event.state.is_pressed() {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Tab) => {
                            game.ui_state.account_focus = 1 - game.ui_state.account_focus;
                        }
                        PhysicalKey::Code(KeyCode::Backspace) => {
                            if game.ui_state.account_focus == 0 {
                                game.ui_state.account_name.pop();
                            } else {
                                game.ui_state.account_handle.pop();
                            }
                        }
                        _ => {
                            if let Some(t) = &event.text {
                                for ch in t.chars() {
                                    if game.ui_state.account_focus == 0
                                        && (ch.is_ascii_alphanumeric()
                                            || matches!(ch, ' ' | '-' | '.'))
                                        && game.ui_state.account_name.chars().count()
                                            < identity::DISPLAY_NAME_MAX
                                    {
                                        game.ui_state.account_name.push(ch);
                                    } else if game.ui_state.account_focus == 1
                                        && (ch.is_ascii_alphanumeric()
                                            || matches!(ch, '.' | ':' | '-' | '@'))
                                        && game.ui_state.account_handle.len() < 255
                                    {
                                        game.ui_state.account_handle.push(ch);
                                    }
                                }
                            }
                        }
                    }
                    if !matches!(event.physical_key, PhysicalKey::Code(KeyCode::Escape)) {
                        return;
                    }
                }
                // Join-screen IP entry.
                if game.ui_state.screen == Screen::Join && event.state.is_pressed() {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Backspace) => {
                            game.multiplayer.join_ip.pop();
                        }
                        _ => {
                            if let Some(t) = &event.text {
                                for ch in t.chars() {
                                    if (ch.is_ascii_alphanumeric() || ".:".contains(ch))
                                        && game.multiplayer.join_ip.len() < 40
                                    {
                                        game.multiplayer.join_ip.push(ch);
                                    }
                                }
                            }
                        }
                    }
                    // Esc still handled below for leaving the screen.
                    if !matches!(event.physical_key, PhysicalKey::Code(KeyCode::Escape)) {
                        return;
                    }
                }
                // Chat entry (multiplayer).
                if game.multiplayer.chat_open && event.state.is_pressed() {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Escape) => {
                            game.multiplayer.chat_open = false;
                            game.multiplayer.chat_text.clear();
                        }
                        PhysicalKey::Code(KeyCode::Enter) => {
                            let msg: String = game
                                .multiplayer
                                .chat_text
                                .trim()
                                .chars()
                                .take(200)
                                .collect();
                            game.multiplayer.chat_open = false;
                            game.multiplayer.chat_text.clear();
                            if !msg.is_empty() {
                                let me = game.config.display_name.clone();
                                if let Some(r) = &game.multiplayer.remote {
                                    r.client.send(&net::C2S::Chat(msg.clone()));
                                } else if let Some(h) = &game.multiplayer.host {
                                    h.net.broadcast(&net::S2C::Chat {
                                        from: me.clone(),
                                        msg: msg.clone(),
                                    });
                                }
                                game.toast(format!("{me}: {msg}"));
                            }
                        }
                        PhysicalKey::Code(KeyCode::Backspace) => {
                            game.multiplayer.chat_text.pop();
                        }
                        _ => {
                            if let Some(t) = &event.text {
                                for ch in t.chars() {
                                    if !ch.is_control() && game.multiplayer.chat_text.len() < 200 {
                                        game.multiplayer.chat_text.push(ch);
                                    }
                                }
                            }
                        }
                    }
                    return;
                }
                if let Screen::SignEdit(pos) = game.ui_state.screen
                    && event.state.is_pressed()
                {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Backspace) => {
                            let l = game.ui_state.sign_line;
                            game.ui_state.sign_lines[l].pop();
                        }
                        PhysicalKey::Code(KeyCode::Enter) => {
                            if game.ui_state.sign_line < 2 {
                                game.ui_state.sign_line += 1;
                            } else {
                                game.commit_sign(pos);
                            }
                        }
                        PhysicalKey::Code(KeyCode::Escape) => game.commit_sign(pos),
                        _ => {
                            if let Some(t) = &event.text {
                                let l = game.ui_state.sign_line;
                                for ch in t.chars() {
                                    let ok = ch.is_ascii_alphanumeric() || " :_-'".contains(ch);
                                    if ok && game.ui_state.sign_lines[l].len() < 14 {
                                        game.ui_state.sign_lines[l].push(ch);
                                    }
                                }
                            }
                        }
                    }
                    return;
                }
                let searchable = matches!(
                    game.ui_state.screen,
                    Screen::Inventory
                        | Screen::Furnace(_)
                        | Screen::Chest(_)
                        | Screen::Offering(_)
                        | Screen::Bloomery(_)
                );
                if game.ui_state.search_focus && searchable && event.state.is_pressed() {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Backspace) => {
                            game.ui_state.search.pop();
                            game.ui_state.browse_page = 0;
                        }
                        PhysicalKey::Code(KeyCode::Escape) | PhysicalKey::Code(KeyCode::Enter) => {
                            game.ui_state.search_focus = false;
                        }
                        _ => {
                            if let Some(t) = &event.text {
                                for ch in t.chars() {
                                    if (ch.is_ascii_alphanumeric()
                                        || ch == ' '
                                        || ch == ':'
                                        || ch == '_')
                                        && game.ui_state.search.len() < 24
                                    {
                                        game.ui_state.search.push(ch);
                                        game.ui_state.browse_page = 0;
                                    }
                                }
                            }
                        }
                    }
                    return;
                }
                if let PhysicalKey::Code(code) = event.physical_key {
                    game.key(code, event.state.is_pressed(), event_loop);
                }
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
                if !game.input.mouse_captured {
                    if pressed {
                        game.capture_mouse(true);
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
                        if let Some(h) = raycast::raycast(
                            &game.server.world,
                            game.camera.pos,
                            game.camera.forward(),
                            REACH,
                        ) {
                            let b = game.server.world.get_block(h.block.0, h.block.1, h.block.2);
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
                    if game.input.scroll_cooldown <= 0.0 {
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
                game.input.ui_cursor = (position.x as f32, position.y as f32);
                if let Some(i) = game.ui_state.dragging_slider {
                    let (bx, _, bw, _) = game.slider_bar_rect(i);
                    game.set_slider(i, (position.x as f32 - bx - 2.0) / (bw - 4.0));
                }
                if game.input.mouse_captured
                    && !game.input.raw_look
                    && game.ui_state.screen == Screen::Playing
                {
                    game.cursor_look(position);
                }
            }
            // Crossing the window boundary teleports the cursor; never treat
            // that jump as look motion.
            WindowEvent::CursorEntered { .. } | WindowEvent::CursorLeft { .. } => {
                game.input.last_cursor = None;
            }
            WindowEvent::Focused(false) => {
                if game.ui_state.screen == Screen::Playing {
                    game.capture_mouse(false);
                }
                game.input.keys = KeysDown::default();
                game.input.left_held = false;
                game.input.right_held = false;
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
            && game.input.mouse_captured
            && game.input.raw_look
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
