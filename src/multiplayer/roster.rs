//! Public presence projection and active-principal connection matching.

use crate::identity::{Principal, Principal::Atproto};
use crate::net::PlayerPresence;

use super::Guest;

pub(super) fn shares_principal(left: &[Principal], right: &[Principal]) -> bool {
    left.iter().any(|principal| right.contains(principal))
}

pub(super) fn host_presence(display_name: &str) -> PlayerPresence {
    PlayerPresence {
        id: 0,
        display_name: display_name.to_owned(),
        verified: false,
        cached_verification: false,
        handle: None,
    }
}

pub(super) fn guest_presence(id: u32, guest: &Guest) -> PlayerPresence {
    presence(
        id,
        guest.name.clone(),
        &guest.principal,
        guest.verification_cached,
        guest.public_handle.clone(),
    )
}

pub(super) fn presence(
    id: u32,
    display_name: String,
    principal: &Principal,
    cached_verification: bool,
    handle: Option<String>,
) -> PlayerPresence {
    PlayerPresence {
        id,
        display_name,
        verified: matches!(principal, Atproto(_)),
        cached_verification,
        handle,
    }
}

pub(super) fn public_label(guest: &Guest) -> String {
    let handle = guest
        .public_handle
        .as_deref()
        .map(|handle| format!(" @{handle}"))
        .unwrap_or_default();
    if guest.verification_cached {
        format!("{}{handle} [VERIFIED/CACHED]", guest.name)
    } else if matches!(guest.principal, Atproto(_)) {
        format!("{}{handle} [VERIFIED]", guest.name)
    } else {
        guest.name.clone()
    }
}
