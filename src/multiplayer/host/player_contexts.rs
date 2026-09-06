//! Player contexts for the authoritative host session.

use super::HostSession;
#[cfg(test)]
use super::MAX_GUEST_VIEW_DIST;

impl HostSession {
    /// PlayerCtx list for the simulation: host (when windowed) + guests.
    pub fn authoritative_player_ctxs(
        &self,
        world: &crate::world::World,
        host: Option<crate::server::PlayerCtx>,
    ) -> Vec<crate::server::PlayerCtx> {
        self.player_ctxs_impl(Some(world), host)
    }

    pub(super) fn player_ctxs_impl(
        &self,
        world: Option<&crate::world::World>,
        host: Option<crate::server::PlayerCtx>,
    ) -> Vec<crate::server::PlayerCtx> {
        let mut out = Vec::new();
        if let Some(h) = host {
            out.push(h);
        }
        for (id, g) in self.guests.iter().filter(|(_, guest)| guest.entry_ready) {
            let quiet_charm = self.profiles.as_ref().and_then(|profiles| {
                g.armor[4]
                    .is_some_and(|stack| {
                        stack.arcane_id != 0
                            && profiles.registry_hint().item(stack.item).charm.as_deref()
                                == Some("quiet")
                    })
                    .then_some(g.armor[4])
                    .flatten()
            });
            let quiet_charm = quiet_charm
                .filter(|stack| world.is_none_or(|world| world.charm_can_pay(*stack, "quiet")));
            out.push(crate::server::PlayerCtx {
                id: *id,
                pos: g.pos,
                spawn: g.pos,
                attackable: true,
                aggro_mod: if quiet_charm.is_some() {
                    -crate::implements::QUIET_CHARM_AGGRO_REDUCTION
                } else {
                    0.0
                },
                quiet_charm,
            });
        }
        out
    }

    #[cfg(test)]
    pub(crate) fn set_initial_view_distance_for_test(&mut self, chunks: i32) {
        self.initial_view_dist = chunks.clamp(2, MAX_GUEST_VIEW_DIST as i32);
    }

    #[cfg(test)]
    pub fn player_ctxs(
        &self,
        host: Option<crate::server::PlayerCtx>,
    ) -> Vec<crate::server::PlayerCtx> {
        self.player_ctxs_impl(None, host)
    }
}
