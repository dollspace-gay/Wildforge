//! Accounts menu actions.

use crate::identity;
use crate::game::Game;
use crate::game::navigation::Screen;

impl Game {
    pub(in crate::game) fn click_accounts_menu(&mut self) {
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
}
