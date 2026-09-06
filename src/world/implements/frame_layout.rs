//! Frame layout implements transaction coordination.

use super::apparatus_neighbors;
use super::horizontal_neighbors;
use crate::implements::FrameLayout;
use crate::inventory::ItemStack;
use crate::planet::BlockPos;
use crate::world::BlockEntity;
use crate::world::World;
use std::collections::BTreeSet;
use std::collections::VecDeque;

impl World {
    pub fn binding_frame_layout(&self, pos: BlockPos) -> FrameLayout {
        let mut problems = Vec::new();
        if self
            .reg
            .block(self.get_block_at(pos))
            .interaction
            .as_deref()
            != Some("binding_frame")
        {
            problems.push("The work surface is not a binding frame.".into());
        }
        let neighbors = horizontal_neighbors(pos);
        let mut focus_mounts = 0u8;
        let mut vessels = 0u8;
        let mut conductor_endpoints = 0u8;
        for neighbor in &neighbors {
            match self
                .reg
                .block(self.get_block_at(*neighbor))
                .interaction
                .as_deref()
            {
                Some("focus_mount") => focus_mounts = focus_mounts.saturating_add(1),
                Some("charge_vessel") => vessels = vessels.saturating_add(1),
                Some("arcane_conductor") => {
                    conductor_endpoints = conductor_endpoints.saturating_add(1)
                }
                _ => {}
            }
        }
        if focus_mounts == 0 {
            problems.push("Place a focus mount beside the frame.".into());
        }
        if vessels == 0 {
            problems.push("Place a charge vessel beside the frame.".into());
        }
        if conductor_endpoints == 0 {
            problems.push("Join a conductor endpoint directly to the frame.".into());
        }

        let (network_size, touches_unloaded, network_overflow) = self.conductor_network(pos);
        if touches_unloaded {
            problems.push("The conductor reaches an unloaded boundary; transfer is paused.".into());
        }
        if network_overflow {
            problems.push(format!(
                "The conductor network exceeds its {}-segment local budget.",
                crate::implements::MAX_CONDUCTOR_NETWORK
            ));
        }

        let mut containment = 0u16;
        for du in -2..=2 {
            for dv in -2..=2 {
                if du == 0 && dv == 0 {
                    continue;
                }
                let Some(at) = pos.offset(du, 0, dv) else {
                    continue;
                };
                let definition = self.reg.block(self.get_block_at(at));
                containment = containment.saturating_add(match definition.name.as_str() {
                    "base:still_salt" => 180,
                    "base:hushwood" => 90,
                    _ if definition.interaction.as_deref() == Some("containment") => 150,
                    // Deliberate physical spacing is useful but cannot replace
                    // material containment on its own.
                    _ if self.reg.is_air(self.get_block_at(at)) && du.abs().max(dv.abs()) == 2 => 4,
                    _ => 0,
                });
            }
        }
        containment = containment.min(1_000);
        if containment < 120 {
            problems.push("The frame needs still salt, Hushwood, posts, or more spacing.".into());
        }

        FrameLayout {
            valid: problems.is_empty(),
            focus_mounts,
            vessels,
            conductor_endpoints,
            containment,
            network_size: network_size.min(u8::MAX as usize) as u8,
            touches_unloaded,
            problems,
        }
    }

    pub(super) fn conductor_network(&self, frame: BlockPos) -> (usize, bool, bool) {
        let (visited, touches_unloaded, overflow) = self.conductor_network_positions(frame);
        (visited.len(), touches_unloaded, overflow)
    }

    pub(super) fn conductor_network_positions(
        &self,
        frame: BlockPos,
    ) -> (BTreeSet<BlockPos>, bool, bool) {
        let mut queue = VecDeque::new();
        for pos in horizontal_neighbors(frame) {
            if self
                .reg
                .block(self.get_block_at(pos))
                .interaction
                .as_deref()
                == Some("arcane_conductor")
            {
                queue.push_back(pos);
            }
        }
        let mut visited = BTreeSet::new();
        let mut touches_unloaded = false;
        let mut overflow = false;
        while let Some(pos) = queue.pop_front() {
            if !visited.insert(pos) {
                continue;
            }
            if visited.len() > crate::implements::MAX_CONDUCTOR_NETWORK {
                overflow = true;
                break;
            }
            for next in apparatus_neighbors(pos) {
                if !self.has_chunk(next.chunk()) {
                    touches_unloaded = true;
                    continue;
                }
                if self
                    .reg
                    .block(self.get_block_at(next))
                    .interaction
                    .as_deref()
                    == Some("arcane_conductor")
                    && !visited.contains(&next)
                {
                    queue.push_back(next);
                }
            }
        }
        (visited, touches_unloaded, overflow)
    }

    /// The first deterministic loaded conductor cell that actually touches a
    /// confluence or well. A conductor merely passing through strong Current
    /// is not an extractor, and unloaded network tails never qualify.
    pub(super) fn conductor_place_source(
        &self,
        frame: BlockPos,
    ) -> Option<(
        crate::planet_atlas::AtlasPos,
        crate::arcane_geography::ArcanePlaceType,
    )> {
        let (positions, touches_unloaded, overflow) = self.conductor_network_positions(frame);
        if touches_unloaded || overflow {
            return None;
        }
        let atlas = self.planet_atlas.as_ref()?;
        let geography = self.arcane_geography.as_ref()?;
        positions.into_iter().find_map(|position| {
            let region = atlas.atlas_pos(position.surface());
            let (_, kind, _) = geography.survey(region, true).nearby_place?;
            matches!(
                kind,
                crate::arcane_geography::ArcanePlaceType::Confluence
                    | crate::arcane_geography::ArcanePlaceType::Well
            )
            .then_some((region, kind))
        })
    }

    pub(super) fn adjacent_vessel(
        &self,
        pos: BlockPos,
    ) -> Result<(BlockPos, ItemStack, u64), String> {
        horizontal_neighbors(pos)
            .into_iter()
            .find_map(|at| match self.installations.get(&at) {
                Some(BlockEntity::ChargeVessel(vessel)) => {
                    vessel.vessel.map(|stack| (at, stack, vessel.revision))
                }
                _ => None,
            })
            .ok_or_else(|| "Place a physical charge vessel directly beside the frame.".into())
    }
}
