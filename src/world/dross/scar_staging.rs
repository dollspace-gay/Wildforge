//! Scar staging dross transaction coordination.

use crate::planet::BlockPos;
use crate::world::World;

impl World {
    pub(in crate::world) fn owns_dross_scar_block(&self, pos: BlockPos) -> bool {
        self.arcane_geography.as_ref().is_some_and(|geography| {
            geography
                .dynamic
                .dross_state
                .materialized
                .contains_key(&pos)
        })
    }

    pub(super) fn stage_resolved_scar(
        &mut self,
        pos: BlockPos,
    ) -> Result<
        (
            u64,
            crate::dross::ScarSite,
            std::collections::VecDeque<crate::dross::DrossEvent>,
            u64,
            crate::arcane_geography::ArcaneSiteCatalog,
        ),
        String,
    > {
        let geography = self
            .arcane_geography
            .as_mut()
            .ok_or("The finite magical geography is unavailable.")?;
        let scar_id = geography
            .dynamic
            .dross_state
            .materialized
            .get(&pos)
            .copied()
            .ok_or("That block is not a live dross scar.")?;
        let old_site = geography
            .dynamic
            .dross_state
            .scars
            .get(&scar_id)
            .cloned()
            .ok_or("The scar's persistent site is missing.")?;
        let old_events = geography.dynamic.dross_state.events.clone();
        let old_sequence = geography.dynamic.dross_state.event_sequence;
        let old_catalog = geography.catalog.clone();
        let step = geography.dynamic.dross_state.completed_steps;
        let site = geography
            .dynamic
            .dross_state
            .scars
            .get_mut(&scar_id)
            .expect("scar site was checked");
        site.materialized_at = None;
        site.resolved_step = Some(step);
        site.last_changed_step = step;
        geography.dynamic.dross_state.materialized.remove(&pos);
        geography.dynamic.dross_state.record(
            step,
            old_site.region,
            crate::dross::DrossEventKind::ScarExcavated { scar_id },
        );
        let region_still_scarred = geography
            .dynamic
            .dross_state
            .scars
            .values()
            .any(|site| site.region == old_site.region && site.resolved_step.is_none());
        if !region_still_scarred
            && let Some(place) = geography.catalog.sites.iter_mut().find(|place| {
                place.kind == crate::arcane_geography::ArcanePlaceType::Scar
                    && place.center == old_site.region
            })
        {
            place.active = false;
        }
        Ok((scar_id, old_site, old_events, old_sequence, old_catalog))
    }

    pub(super) fn rollback_resolved_scar(
        &mut self,
        pos: BlockPos,
        scar_id: u64,
        old_site: crate::dross::ScarSite,
        old_events: std::collections::VecDeque<crate::dross::DrossEvent>,
        old_sequence: u64,
        old_catalog: crate::arcane_geography::ArcaneSiteCatalog,
    ) {
        if let Some(geography) = self.arcane_geography.as_mut() {
            geography
                .dynamic
                .dross_state
                .scars
                .insert(scar_id, old_site);
            geography
                .dynamic
                .dross_state
                .materialized
                .insert(pos, scar_id);
            geography.dynamic.dross_state.events = old_events;
            geography.dynamic.dross_state.event_sequence = old_sequence;
            geography.catalog = old_catalog;
        }
    }
}
