//! Item cards and held models consume only authorized observations.

use super::{Source, WorldView};
use crate::implements::{ApparatusCue, ImplementVisual};
use crate::inventory::ItemStack;
use crate::planet::EntityPos;
use crate::world::{TerrainRead, item_presentation};

impl WorldView<'_> {
    pub(crate) fn inspectable_item_current(&self, id: u64) -> Option<u64> {
        match self.source {
            Source::Authority(world) => world.inspectable_item_current(id),
            Source::Replica(world) => world.inspectable_item_current(id),
        }
    }
    pub(crate) fn implement_tooltip(&self, stack: ItemStack, exact: bool) -> Vec<String> {
        match self.source {
            Source::Authority(world) => world.implement_tooltip(stack, exact),
            Source::Replica(world) => world.implement_tooltip(stack, exact),
        }
    }
    pub(crate) fn implement_visual(&self, stack: ItemStack) -> Option<ImplementVisual> {
        match self.source {
            Source::Authority(world) => world.implement_visual(stack),
            Source::Replica(world) => world.implement_visual(stack),
        }
    }
    pub(crate) fn charm_can_pay(&self, stack: ItemStack, kind: &str) -> bool {
        item_presentation::charm_can_pay(self.registry(), stack, kind, self.inspectable_item_current(stack.arcane_id))
    }
    pub(crate) fn preparation_tooltip(&self, stack: ItemStack, has_lens: bool) -> Vec<String> {
        match self.source {
            Source::Authority(world) => world.preparation_tooltip(stack, has_lens),
            // The private preparation-container ledger has never been streamed.
            Source::Replica(_) => Vec::new(),
        }
    }
    pub(crate) fn apparatus_cues_near(&self, observer: EntityPos, radius: f32) -> Vec<ApparatusCue> {
        match self.source {
            Source::Authority(world) => world.apparatus_cues_near(observer, radius),
            Source::Replica(world) => world.apparatus_cues_near(observer, radius),
        }
    }
}

impl<'a> WorldView<'a> {
    pub(crate) fn alchemy_apparatus_at(&self, position: crate::planet::BlockPos) -> Option<&'a crate::alchemy::AlchemyApparatusState> {
        match self.source {
            Source::Authority(world) => world.alchemy_state()?.apparatus.get(&position),
            Source::Replica(_) => None,
        }
    }

    pub(crate) fn ordinary_alchemy_job_at(&self, position: crate::planet::BlockPos) -> Option<&'a crate::alchemy::OrdinaryProcessJob> {
        match self.source {
            Source::Authority(world) => world.alchemy_state()?.ordinary_jobs.get(&position),
            Source::Replica(_) => None,
        }
    }

    pub(crate) fn idle_apparatus_near(&self, position: crate::planet::BlockPos, kind: crate::alchemy::ApparatusKind) -> Option<crate::planet::BlockPos> {
        let Source::Authority(world) = self.source else { return None };
        world.alchemy_state()?.apparatus.iter()
            .filter(|(_, apparatus)| apparatus.kind == kind && apparatus.batch.is_none())
            .map(|(candidate, _)| *candidate)
            .filter(|candidate| candidate.face() == position.face()
                && i32::from(candidate.u()).abs_diff(i32::from(position.u()))
                    + i32::from(candidate.y()).abs_diff(i32::from(position.y()))
                    + i32::from(candidate.v()).abs_diff(i32::from(position.v())) <= 4)
            .min()
    }
}
