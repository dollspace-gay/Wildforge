//! Menu click handling and screen transitions.

use super::*;

impl Game {
    pub(super) fn menu_click(&mut self, event_loop: &ActiveEventLoop, right: bool) {
        if std::env::var("WILDFORGE_DEBUG").is_ok() {
            eprintln!("menu_click at {:?} right={right}", self.input.ui_cursor);
        }
        match self.ui_state.screen {
            Screen::Inventory => {
                if self.hit(self.inventory_tab_rect(0)) {
                    self.sfx(Sfx::Click);
                    self.ui_state.inventory_status_open = false;
                    self.ui_state.inventory_browser_open = false;
                    self.ui_state.search_focus = false;
                    self.ui_state.browse_view = None;
                    return;
                }
                if self.hit(self.inventory_tab_rect(1)) {
                    self.sfx(Sfx::Click);
                    self.ui_state.inventory_status_open = true;
                    self.ui_state.inventory_browser_open = false;
                    self.ui_state.search_focus = false;
                    self.ui_state.browse_view = None;
                    return;
                }
                if self.hit(self.inventory_tab_rect(2)) {
                    self.sfx(Sfx::Click);
                    self.ui_state.inventory_browser_open = !self.ui_state.inventory_browser_open;
                    self.ui_state.inventory_status_open = false;
                    if !self.ui_state.inventory_browser_open {
                        self.ui_state.search_focus = false;
                        self.ui_state.browse_view = None;
                    }
                    return;
                }
                if self.ui_state.inventory_browser_open && self.browser_click(right) {
                    return;
                }
                if !self.ui_state.inventory_status_open {
                    for i in 0..5 {
                        if self.hit(self.armor_slot_rect(i)) {
                            self.armor_click(i);
                            return;
                        }
                    }
                }
                for i in 0..TOTAL_SLOTS {
                    if self.hit(self.inv_slot_rect(i)) {
                        self.inventory_click(false, i, right);
                        return;
                    }
                }
                if !self.ui_state.inventory_status_open {
                    for i in 0..self.interaction.craft_size * self.interaction.craft_size {
                        if self.hit(self.craft_slot_rect(i)) {
                            self.inventory_click(true, i, right);
                            return;
                        }
                    }
                    if self.hit(self.result_slot_rect()) {
                        self.result_click();
                    }
                }
            }
            Screen::Paused => {
                if self.hit(self.menu_button_rect(0)) {
                    self.sfx(Sfx::Click);
                    self.set_screen(Screen::Playing);
                } else if self.hit(self.menu_button_rect(1)) {
                    self.sfx(Sfx::Click);
                    self.creative = !self.creative;
                    self.flying = false;
                    let mode = if self.creative {
                        "creative"
                    } else {
                        "survival"
                    };
                    world::write_world_meta_full(
                        &self.server.world.save_dir_for_saving(),
                        self.server.world.seed,
                        mode,
                        self.server.world.ire,
                        self.server.world.day,
                        self.server.world.weather,
                    );
                    if self.content.scripts.wants("on_mode_change") {
                        self.content.scripts.dispatch(
                            &self.server.world,
                            "on_mode_change",
                            (mode.to_string(),),
                        );
                        self.apply_script_cmds();
                    }
                } else if self.hit(self.menu_button_rect(2)) {
                    self.sfx(Sfx::Click);
                    if self.multiplayer.host.is_none() && self.multiplayer.remote.is_none() {
                        let wname = self
                            .server
                            .world
                            .save_dir_for_saving()
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "world".into());
                        let host_name = identity::DisplayName::parse(&self.config.display_name)
                            .expect("configured display name is valid");
                        match mp::HostSession::start_windowed(wname, host_name) {
                            Ok(sess) => {
                                self.server.world.set_edit_logging(true);
                                self.toast(format!(
                                    "Open to friends on port {} (LAN + direct IP).",
                                    sess.net.port
                                ));
                                self.multiplayer.host = Some(sess);
                            }
                            Err(e) => self.toast(format!("Could not host: {e}")),
                        }
                    }
                } else if self.hit(self.menu_button_rect(3)) {
                    self.sfx(Sfx::Click);
                    self.ui_state.settings_from_pause = true;
                    self.set_screen(Screen::Settings);
                } else if self.hit(self.menu_button_rect(4)) {
                    self.sfx(Sfx::Click);
                    self.ui_state.appearance_from_pause = true;
                    self.set_screen(Screen::Appearance);
                } else if self.hit(self.menu_button_rect(5)) {
                    self.sfx(Sfx::Click);
                    self.quit_to_title();
                } else {
                    for (row, (id, _)) in self.guest_rows().iter().enumerate() {
                        if self.hit(self.kick_rect(row)) {
                            self.sfx(Sfx::Click);
                            self.ui_state.moderation_confirm = None;
                            self.set_screen(Screen::Moderation(*id));
                            break;
                        }
                    }
                }
            }
            Screen::Moderation(id) => {
                for action in 0..6 {
                    if !self.hit(self.menu_button_rect(action)) {
                        continue;
                    }
                    self.sfx(Sfx::Click);
                    // Disconnecting and ban actions require a second click.
                    if action <= 3 && self.ui_state.moderation_confirm != Some(action as u8) {
                        self.ui_state.moderation_confirm = Some(action as u8);
                        return;
                    }
                    self.ui_state.moderation_confirm = None;
                    let result = if let Some(host) = &mut self.multiplayer.host {
                        match action {
                            0 => Ok(host.kick_guest(id).map(|name| format!("{name} kicked"))),
                            1 => host
                                .mute_guest(id, "windowed host mute", Some(600), "host")
                                .map(|changed| {
                                    changed.then_some("player muted for 10 minutes".into())
                                }),
                            2 => host
                                .ban_guest(id, "windowed host timed ban", Some(3600), "host")
                                .map(|name| name.map(|name| format!("{name} banned for one hour"))),
                            3 => host
                                .ban_guest(id, "windowed host permanent ban", None, "host")
                                .map(|name| name.map(|name| format!("{name} permanently banned"))),
                            4 => host.allow_guest(id, "host").map(|changed| {
                                changed.then_some("player added to allowlist".into())
                            }),
                            _ => {
                                let next = match host.guest_role(id).unwrap_or_default() {
                                    mp::Role::Player => mp::Role::Moderator,
                                    mp::Role::Moderator => mp::Role::Admin,
                                    mp::Role::Admin | mp::Role::Owner => mp::Role::Player,
                                };
                                host.set_guest_role(id, next, "host").map(|changed| {
                                    changed.then_some(format!("role set to {next:?}"))
                                })
                            }
                        }
                    } else if let Some(remote) = &self.multiplayer.remote {
                        let moderation_action = match action {
                            0 => net::ModerationAction::Kick,
                            1 => net::ModerationAction::Mute { seconds: 600 },
                            2 => net::ModerationAction::Ban {
                                seconds: Some(3600),
                            },
                            3 => net::ModerationAction::Ban { seconds: None },
                            4 => net::ModerationAction::Allow,
                            _ => net::ModerationAction::CycleRole,
                        };
                        remote.client.send(&net::C2S::Moderate {
                            target: id,
                            action: moderation_action,
                        });
                        Ok(Some("Moderation request sent to the host.".into()))
                    } else {
                        Ok(None)
                    };
                    match result {
                        Ok(Some(message)) => self.toast(message),
                        Ok(None) => self.toast("Player is no longer connected.".into()),
                        Err(error) => self.toast(format!("Moderation failed: {error}")),
                    }
                    if matches!(action, 0 | 2 | 3) {
                        self.set_screen(Screen::Paused);
                    }
                    return;
                }
                if self.hit(self.menu_button_rect(6)) {
                    self.sfx(Sfx::Click);
                    self.ui_state.moderation_confirm = None;
                    self.set_screen(Screen::Paused);
                }
            }
            Screen::Dead => {
                if self.hit(self.menu_button_rect(0)) {
                    self.sfx(Sfx::Click);
                    self.respawn();
                }
            }
            Screen::Title => {
                for i in 0..self.worlds.len().min(6) {
                    if self.hit(self.title_play_rect(i)) {
                        self.sfx(Sfx::Click);
                        let name = self.worlds[i].0.clone();
                        self.start_world(&name);
                        return;
                    }
                    if self.hit(self.title_delete_rect(i)) {
                        self.sfx(Sfx::Click);
                        self.ui_state.pending_delete = Some(i);
                        self.set_screen(Screen::ConfirmDelete);
                        return;
                    }
                }
                if self.hit(self.title_action_rect(0)) {
                    self.sfx(Sfx::Click);
                    self.new_world_mode("survival");
                } else if self.hit(self.title_action_rect(1)) {
                    self.sfx(Sfx::Click);
                    self.new_world_mode("creative");
                } else if self.hit(self.title_action_rect(2)) {
                    self.sfx(Sfx::Click);
                    self.multiplayer.discovery = net::Discovery::start().ok();
                    self.multiplayer.join_status.clear();
                    self.set_screen(Screen::Join);
                } else if self.hit(self.title_action_rect(3)) {
                    self.sfx(Sfx::Click);
                    self.ui_state.account_name = self.config.display_name.clone();
                    self.set_screen(Screen::Accounts);
                } else if self.hit(self.title_action_rect(4)) {
                    self.sfx(Sfx::Click);
                    self.ui_state.appearance_from_pause = false;
                    self.set_screen(Screen::Appearance);
                } else if self.hit(self.title_action_rect(5)) {
                    self.sfx(Sfx::Click);
                    self.set_screen(Screen::Mods);
                } else if self.hit(self.title_action_rect(6)) {
                    self.sfx(Sfx::Click);
                    self.content.packs = atlas::discover_packs();
                    self.set_screen(Screen::Packs);
                } else if self.hit(self.title_action_rect(7)) {
                    self.sfx(Sfx::Click);
                    self.ui_state.settings_from_pause = false;
                    self.set_screen(Screen::Settings);
                } else if self.hit(self.title_action_rect(8)) {
                    event_loop.exit();
                }
            }
            Screen::Accounts => {
                if self.hit(self.account_field_rect(0)) {
                    self.ui_state.account_focus = 0;
                    return;
                }
                if self.hit(self.account_field_rect(1)) {
                    self.ui_state.account_focus = 1;
                    return;
                }
                if self.hit(self.account_button_rect(0)) {
                    match identity::DisplayName::parse(&self.ui_state.account_name) {
                        Ok(name) => {
                            self.config.display_name = name.to_string();
                            self.ui_state.account_name = name.to_string();
                            self.config.profile_complete = true;
                            self.config.save();
                            self.ui_state.account_status = "LOCAL PROFILE SAVED".into();
                        }
                        Err(error) => {
                            self.ui_state.account_status = error.to_string().to_uppercase()
                        }
                    }
                } else if self.hit(self.account_button_rect(1)) {
                    self.start_account_link();
                } else if self.hit(self.account_button_rect(2)) {
                    if let Some(account) = &mut self.atproto_account {
                        account.use_social_display_name = !account.use_social_display_name;
                        let _ = account.save(&identity::identity_dir());
                    }
                } else if self.hit(self.account_button_rect(3)) {
                    if let Some(account) = &mut self.atproto_account {
                        account.use_social_avatar = !account.use_social_avatar;
                        let _ = account.save(&identity::identity_dir());
                    }
                } else if self.hit(self.account_button_rect(4)) {
                    if let Some(account) = &mut self.atproto_account {
                        account.share_social_handle = !account.share_social_handle;
                        let _ = account.save(&identity::identity_dir());
                    }
                } else if self.hit(self.account_button_rect(5)) {
                    self.start_account_revoke();
                } else if self.hit(self.account_button_rect(6)) {
                    match identity::atproto::AtprotoAccount::unlink_local(&identity::identity_dir())
                    {
                        Ok(()) => {
                            self.atproto_account = None;
                            self.ui_state.account_status =
                                "LOCAL LINK REMOVED; REMOTE DEVICE RECORD MAY STILL EXIST".into();
                        }
                        Err(error) => {
                            self.ui_state.account_status = format!("UNLINK FAILED: {error}")
                        }
                    }
                } else if self.hit(self.account_button_rect(7)) && self.config.profile_complete {
                    self.set_screen(Screen::Title);
                }
            }
            Screen::Mods => {
                if self.hit(self.menu_button_rect(4)) {
                    self.sfx(Sfx::Click);
                    self.set_screen(Screen::Title);
                }
            }
            Screen::Packs => {
                if self.hit(self.pack_back_rect()) {
                    self.sfx(Sfx::Click);
                    self.set_screen(Screen::Title);
                    return;
                }
                for i in 0..=self.content.packs.len().min(7) {
                    if self.hit(self.pack_row_rect(i)) {
                        let sel = if i == 0 {
                            String::new()
                        } else {
                            self.content.packs[i - 1].id.clone()
                        };
                        self.sfx(Sfx::Click);
                        if sel != self.active_pack_id() {
                            self.content.pack_override = None;
                            self.config.pack = sel;
                            self.apply_pack();
                        }
                        return;
                    }
                }
            }
            Screen::Furnace(pos) => {
                if self.browser_click(right) {
                    return;
                }
                for i in 0..3 {
                    if self.hit(self.furnace_slot_rect(i)) {
                        self.furnace_click(pos, i, right);
                        return;
                    }
                }
                for i in 0..TOTAL_SLOTS {
                    if self.hit(self.inv_slot_rect(i)) {
                        self.inventory_click(false, i, right);
                        return;
                    }
                }
            }
            Screen::Bloomery(pos) => {
                if self.browser_click(right) {
                    return;
                }
                if self.hit(self.bloomery_light_rect()) {
                    self.sfx(Sfx::Click);
                    self.light_bloomery_action(pos);
                    return;
                }
                for i in 0..8 {
                    if self.hit(self.bloomery_slot_rect(i)) {
                        self.bloomery_click(pos, i, right);
                        return;
                    }
                }
                for i in 0..TOTAL_SLOTS {
                    if self.hit(self.inv_slot_rect(i)) {
                        self.inventory_click(false, i, right);
                        return;
                    }
                }
            }
            Screen::Kiln(pos) => {
                if self.browser_click(right) {
                    return;
                }
                if self.hit(self.bloomery_light_rect()) {
                    self.sfx(Sfx::Click);
                    self.light_bloomery_action(pos);
                    return;
                }
                for i in 0..9 {
                    if self.hit(self.kiln_slot_rect(i)) {
                        self.kiln_click(pos, i, right);
                        return;
                    }
                }
                for i in 0..TOTAL_SLOTS {
                    if self.hit(self.inv_slot_rect(i)) {
                        self.inventory_click(false, i, right);
                        return;
                    }
                }
            }
            Screen::SignEdit(_) => {}
            Screen::MobCargo(id) => {
                for i in 0..12 {
                    if self.hit(self.mob_cargo_slot_rect(i)) {
                        self.mob_cargo_click(id, i, right);
                        return;
                    }
                }
                for i in 0..TOTAL_SLOTS {
                    if self.hit(self.inv_slot_rect(i)) {
                        self.inventory_click(false, i, right);
                        return;
                    }
                }
            }
            Screen::Chest(pos) => {
                if self.browser_click(right) {
                    return;
                }
                for i in 0..world::CHEST_SLOTS {
                    if self.hit(self.chest_slot_rect(i)) {
                        self.chest_click(pos, i, right);
                        return;
                    }
                }
                for i in 0..TOTAL_SLOTS {
                    if self.hit(self.inv_slot_rect(i)) {
                        self.inventory_click(false, i, right);
                        return;
                    }
                }
            }
            Screen::Offering(pos) => {
                if self.browser_click(right) {
                    return;
                }
                for i in 0..3 {
                    if self.hit(self.offering_slot_rect(i)) {
                        self.offering_click(pos, i, right);
                        return;
                    }
                }
                for i in 0..TOTAL_SLOTS {
                    if self.hit(self.inv_slot_rect(i)) {
                        self.inventory_click(false, i, right);
                        return;
                    }
                }
            }
            Screen::Join => {
                if self.hit(self.pack_back_rect()) {
                    self.sfx(Sfx::Click);
                    self.multiplayer.discovery = None;
                    self.multiplayer.pending_join_disclosure = None;
                    self.set_screen(Screen::Title);
                    return;
                }
                let w = self.renderer.config.width as f32;
                let h = self.renderer.config.height as f32;
                let found: Vec<net::DiscoveredServer> = self
                    .multiplayer
                    .discovery
                    .as_ref()
                    .map(|d| d.found.clone())
                    .unwrap_or_default();
                for (i, found) in found.iter().take(5).enumerate() {
                    let r = (w / 2.0 - 220.0, h * 0.20 + i as f32 * 56.0, 440.0, 42.0);
                    if self.hit(r) {
                        self.sfx(Sfx::Click);
                        self.request_join(found.addr, Some(found.identity));
                        return;
                    }
                }
                let y = h * 0.20 + found.len().clamp(1, 5) as f32 * 56.0 + 26.0;
                let cr = (w / 2.0 + 240.0, y - 6.0, 160.0, 34.0);
                if self.hit(cr) {
                    self.sfx(Sfx::Click);
                    let text = self.multiplayer.join_ip.trim().to_string();
                    let addr = if text.contains(':') {
                        text.parse().ok()
                    } else {
                        format!("{text}:{}", net::GAME_PORT).parse().ok()
                    };
                    match addr {
                        Some(a) => {
                            self.request_join(a, None);
                        }
                        None => self.multiplayer.join_status = "BAD ADDRESS".to_string(),
                    }
                }
            }
            Screen::Settings => {
                for i in 0..Self::SLIDERS.len() {
                    let (bx, by, bw, bh) = self.slider_bar_rect(i);
                    let (cx, cy) = self.input.ui_cursor;
                    if cx >= bx && cx < bx + bw && cy >= by && cy < by + bh {
                        self.ui_state.dragging_slider = Some(i);
                        self.set_slider(i, (cx - bx - 2.0) / (bw - 4.0));
                        return;
                    }
                }
                if self.hit(self.settings_toggle_rect(0)) {
                    self.sfx(Sfx::Click);
                    self.config.lights = (self.config.lights + 1) % 3;
                    return;
                }
                if self.hit(self.settings_toggle_rect(1)) {
                    self.sfx(Sfx::Click);
                    self.config.stark = !self.config.stark;
                    return;
                }
                if self.hit(self.settings_toggle_rect(2)) {
                    self.sfx(Sfx::Click);
                    self.config.outline = !self.config.outline;
                    return;
                }
                if self.hit(self.settings_toggle_rect(3)) {
                    self.sfx(Sfx::Click);
                    self.config.bloom = !self.config.bloom;
                    return;
                }
                if self.hit(self.settings_back_rect()) {
                    self.sfx(Sfx::Click);
                    self.config.save();
                    self.set_screen(if self.ui_state.settings_from_pause {
                        Screen::Paused
                    } else {
                        Screen::Title
                    });
                }
            }
            Screen::Appearance => {
                let lens: [usize; 8] = [
                    style::SKIN_TONES.len(),
                    style::HAIR_COLORS.len(),
                    style::HAIR_STYLE_NAMES.len(),
                    style::BEARD_NAMES.len(),
                    style::BUILD_NAMES.len(),
                    style::SHIRT_COLORS.len(),
                    style::LEGWEAR_NAMES.len(),
                    style::TROUSER_COLORS.len(),
                ];
                for (i, len) in lens.iter().enumerate() {
                    if self.hit(self.appearance_row_rect(i)) {
                        self.sfx(Sfx::Click);
                        let n = *len as i32;
                        let cur = match i {
                            0 => &mut self.style.skin,
                            1 => &mut self.style.hair,
                            2 => &mut self.style.hair_style,
                            3 => &mut self.style.beard,
                            4 => &mut self.style.build,
                            5 => &mut self.style.shirt,
                            6 => &mut self.style.legwear,
                            _ => &mut self.style.trousers,
                        };
                        let step = if right { -1 } else { 1 };
                        *cur = ((*cur as i32 + step).rem_euclid(n)) as u8;
                        self.config.appearance = self.style.pack();
                        return;
                    }
                }
                if self.hit(self.appearance_back_rect()) {
                    self.sfx(Sfx::Click);
                    self.config.save();
                    self.set_screen(if self.ui_state.appearance_from_pause {
                        Screen::Paused
                    } else {
                        Screen::Title
                    });
                }
            }
            Screen::ConfirmDelete => {
                if self.hit(self.menu_button_rect(0)) {
                    self.sfx(Sfx::Click);
                    if let Some(i) = self.ui_state.pending_delete.take() {
                        if let Some((name, _)) = self.worlds.get(i) {
                            let _ = std::fs::remove_dir_all(PathBuf::from("saves").join(name));
                        }
                        self.refresh_worlds();
                    }
                    self.set_screen(Screen::Title);
                } else if self.hit(self.menu_button_rect(1)) {
                    self.sfx(Sfx::Click);
                    self.ui_state.pending_delete = None;
                    self.set_screen(Screen::Title);
                }
            }
            Screen::Playing => {}
        }
    }

    fn start_account_link(&mut self) {
        if self.ui_state.account_task.is_some() {
            return;
        }
        let input = self.ui_state.account_handle.trim().to_string();
        if input.is_empty() {
            self.ui_state.account_status = "ENTER A HANDLE OR DID FIRST".into();
            return;
        }
        let root = identity::identity_dir();
        let public_key = self.identity.public_key();
        let (tx, rx) = std::sync::mpsc::channel();
        self.ui_state.account_task = Some(rx);
        self.ui_state.account_status = "OPENING BROWSER - WAITING FOR OAUTH CALLBACK...".into();
        std::thread::spawn(move || {
            let result = identity::atproto::link_account(&root, &input, public_key)
                .map_err(|error| error.to_string());
            let _ = tx.send(AccountTaskResult::Linked(result));
        });
    }

    fn start_account_revoke(&mut self) {
        if self.ui_state.account_task.is_some() {
            return;
        }
        let Some(account) = self.atproto_account.clone() else {
            self.ui_state.account_status = "NO ATPROTO ACCOUNT IS LINKED".into();
            return;
        };
        let input = if self.ui_state.account_handle.trim().is_empty() {
            account.did.to_string()
        } else {
            self.ui_state.account_handle.trim().to_string()
        };
        let root = identity::identity_dir();
        let (tx, rx) = std::sync::mpsc::channel();
        self.ui_state.account_task = Some(rx);
        self.ui_state.account_status = "REAUTHENTICATE IN BROWSER TO REVOKE THIS DEVICE...".into();
        std::thread::spawn(move || {
            let result = identity::atproto::revoke_account(&root, &input, &account)
                .map_err(|error| error.to_string());
            let _ = tx.send(AccountTaskResult::Revoked(result));
        });
    }

    pub(super) fn poll_account_task(&mut self) {
        let result = self
            .ui_state
            .account_task
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok());
        let Some(result) = result else { return };
        self.ui_state.account_task = None;
        match result {
            AccountTaskResult::Linked(Ok(account)) => {
                let approved_as = account
                    .profile_display_name
                    .as_deref()
                    .or(account.handle.as_deref())
                    .unwrap_or(account.did.as_str())
                    .to_uppercase();
                self.ui_state.account_handle = account
                    .handle
                    .clone()
                    .unwrap_or_else(|| account.did.to_string());
                self.atproto_account = Some(account);
                self.ui_state.account_status =
                    format!("APPROVED AS {approved_as} - DEVICE BINDING RECORD WRITTEN");
            }
            AccountTaskResult::Revoked(Ok(())) => {
                self.atproto_account = None;
                self.ui_state.account_status = "DEVICE BINDING REVOKED AND ACCOUNT UNLINKED".into();
            }
            AccountTaskResult::Linked(Err(error)) | AccountTaskResult::Revoked(Err(error)) => {
                self.ui_state.account_status =
                    format!("OAUTH/PROVIDER ERROR: {error}").to_uppercase();
            }
        }
    }
}
