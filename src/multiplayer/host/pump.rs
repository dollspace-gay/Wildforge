//! Pump for the authoritative host session.

use super::{EntityPos, HostFx, HostSession, ItemStack, Server, Vec3};

impl HostSession {

    /// Everything the host does per frame: drain guest messages, apply
    /// them authoritatively, stream state back.
    /// `host`: (pos, yaw, sleeping) for a windowed host; None when
    /// running headless (`--server`).
    pub fn pump(
        &mut self,
        server: &mut Server,
        host: Option<(EntityPos, f32, bool, u16, u32)>,
        dt: f32,
    ) -> Vec<HostFx> {
        self.pump_inner(server, host, None, dt)
    }

    pub fn pump_with_host_stack(
        &mut self,
        server: &mut Server,
        host: Option<(EntityPos, f32, bool, Option<ItemStack>, u32)>,
        dt: f32,
    ) -> Vec<HostFx> {
        let visual = host
            .and_then(|(_, _, _, stack, _)| stack)
            .and_then(|stack| server.world.implement_visual(stack));
        let host = host.map(|(pos, yaw, sleeping, stack, style)| {
            (
                pos,
                yaw,
                sleeping,
                stack.map_or(u16::MAX, |stack| stack.item.0),
                style,
            )
        });
        self.pump_inner(server, host, visual, dt)
    }

    pub(super) fn pump_inner(
        &mut self,
        server: &mut Server,
        host: Option<(EntityPos, f32, bool, u16, u32)>,
        host_visual: Option<crate::implements::ImplementVisual>,
        dt: f32,
    ) -> Vec<HostFx> {
        let host_pos = host.map(|(p, _, _, _, _)| p).unwrap_or_else(|| {
            EntityPos::from_local(crate::planet::Face::PosZ, Vec3::new(0.5, 80.0, 0.5))
                .expect("default host position is canonical")
        });
        let host_yaw = host.map(|(_, y, _, _, _)| y).unwrap_or(0.0);
        let host_sleeping = host.map(|(_, _, s, _, _)| s).unwrap_or(false);
        let host_held = host.map(|(_, _, _, h, _)| h).unwrap_or(u16::MAX);
        let host_style = host
            .map(|(_, _, _, _, st)| st)
            .unwrap_or(crate::style::Style::default().pack());
        let mut fx = Vec::new();
        self.pump_events(server, dt, &mut fx);
        self.pump_guest_state(server, dt, &mut fx);
        self.pump_delivery(server);
        self.stream_chunks(server);
        self.stream_snapshots(
            server,
            host.map(|_| (host_pos, host_yaw, host_held, host_style, host_visual)),
            dt,
        );
        self.pump_containers(server, dt);
        self.pump_spoilage(server, dt);
        self.pump_riders(server);
        self.pump_observations(server, dt);
        self.pump_sleep(server, dt, host.is_some(), host_sleeping, &mut fx);
        fx
    }
}
