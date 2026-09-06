//! Account tasks menu actions.

use crate::game::Game;
use crate::game::navigation::AccountTaskResult;
use crate::identity;

impl Game {
    pub(in crate::game) fn start_account_link(&mut self) {
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

    pub(in crate::game) fn start_account_revoke(&mut self) {
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

    pub(in crate::game) fn poll_account_task(&mut self) {
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
