//! Magic feedback in the ordered graphical action pipeline.

use crate::audio::Sfx;
use crate::game::Game;
use crate::registry::ItemId;

impl Game {
    /// Present an authoritative implement event without learning exact charge
    /// or provenance. `wire_items` is present for a guest receiving host item
    /// ids; a windowed host passes `None` because its visual already names the
    /// local registry.
    pub(in crate::game) fn present_implement_activation(
        &mut self,
        pos: crate::planet::EntityPos,
        cue: crate::implements::ImplementCue,
        visual: Option<crate::implements::ImplementVisual>,
        wire_items: Option<&[Option<ItemId>]>,
    ) {
        let sound = match cue {
            crate::implements::ImplementCue::Use => Sfx::ImplementUse,
            crate::implements::ImplementCue::Transfer => Sfx::ImplementTransfer,
            crate::implements::ImplementCue::Strain => Sfx::ImplementStrain,
            crate::implements::ImplementCue::Empty => Sfx::ImplementEmpty,
            crate::implements::ImplementCue::Failure => Sfx::ImplementFailure,
        };
        self.sfx(sound);
        if !self.presentation.juice {
            return;
        }
        let focus_item = visual.and_then(|visual| {
            wire_items.map_or_else(
                || {
                    self.content
                        .reg
                        .items
                        .get(visual.focus as usize)
                        .map(|_| ItemId(visual.focus))
                },
                |map| map.get(visual.focus as usize).copied().flatten(),
            )
        });
        let tile = focus_item
            .map(|item| self.content.reg.item(item).icon)
            .unwrap_or_else(|| {
                *crate::atlas::builtin_slots()
                    .get("ember")
                    .unwrap_or(&crate::atlas::UNKNOWN_SLOT)
            });
        let count = match cue {
            crate::implements::ImplementCue::Transfer => 10,
            crate::implements::ImplementCue::Failure => 18,
            crate::implements::ImplementCue::Use | crate::implements::ImplementCue::Strain => 6,
            crate::implements::ImplementCue::Empty => 2,
        };
        self.presentation.burst(pos.render_pos(), tile, count, 1.4);
    }

    pub(in crate::game) fn present_working_cue(&mut self, cue: crate::workings::WorkingCue) {
        use crate::workings::WorkingCueKind;
        if matches!(
            cue.kind,
            WorkingCueKind::Complete | WorkingCueKind::Cancel | WorkingCueKind::Refuse
        ) {
            self.presentation.working_cues.remove(&cue.stable_id);
        } else {
            self.presentation
                .working_cues
                .insert(cue.stable_id, (cue.clone(), self.time_abs));
        }
        self.presentation.swing = 1.0;
        self.sfx(
            if cue.warning_band >= 2
                || matches!(cue.kind, WorkingCueKind::Strain | WorkingCueKind::Overload)
            {
                Sfx::WorkingStrain(cue.warning_band)
            } else if matches!(cue.kind, WorkingCueKind::Refuse) {
                Sfx::ImplementFailure
            } else {
                Sfx::ImplementUse
            },
        );
        if !self.presentation.juice {
            return;
        }
        let source_tile = *crate::atlas::builtin_slots()
            .get("ember")
            .unwrap_or(&crate::atlas::UNKNOWN_SLOT);
        let target_tile = *crate::atlas::builtin_slots()
            .get("water")
            .unwrap_or(&crate::atlas::UNKNOWN_SLOT);
        let path_tile = *crate::atlas::builtin_slots()
            .get("stone")
            .unwrap_or(&crate::atlas::UNKNOWN_SLOT);
        self.presentation.burst(
            cue.source.entity_center().render_pos(),
            source_tile,
            4,
            0.75,
        );
        if let Some(target) = cue.path.last().copied() {
            self.presentation.burst(
                target.entity_center().render_pos(),
                target_tile,
                if cue.warning_band >= 2 { 10 } else { 6 },
                1.0,
            );
        }
        let inner = cue.path.len().saturating_sub(2);
        let stride = inner.div_ceil(8).max(1);
        for pos in cue.path.iter().skip(1).take(inner).step_by(stride) {
            self.presentation.burst(
                pos.entity_center().render_pos(),
                path_tile,
                if cue.warning_band >= 2 { 3 } else { 1 },
                0.28,
            );
        }
    }

    pub(in crate::game) fn present_alchemy_cue(&mut self, cue: crate::alchemy::AlchemyCue) {
        use crate::alchemy::AlchemyCueKind;
        self.presentation.swing = 1.0;
        self.sfx(match cue.kind {
            AlchemyCueKind::Grind => Sfx::Grind,
            AlchemyCueKind::Bubble | AlchemyCueKind::Drip | AlchemyCueKind::Pour => Sfx::Splash,
            AlchemyCueKind::Filter | AlchemyCueKind::Clean => Sfx::Click,
            AlchemyCueKind::Leak
            | AlchemyCueKind::Overcharge
            | AlchemyCueKind::Spoil
            | AlchemyCueKind::Pulse => Sfx::ImplementStrain,
            AlchemyCueKind::Drink | AlchemyCueKind::Apply => Sfx::Pickup,
        });
        if self.presentation.juice {
            let tile_name = if cue.color[0] > cue.color[1] && cue.color[0] > cue.color[2] {
                "ember"
            } else if cue.color[2] > cue.color[0] {
                "water"
            } else {
                "plant"
            };
            let tile = *crate::atlas::builtin_slots()
                .get(tile_name)
                .unwrap_or(&crate::atlas::UNKNOWN_SLOT);
            let count = 3 + usize::from(cue.intensity) / 20;
            self.presentation.burst(
                cue.pos.entity_center().render_pos(),
                tile,
                count.min(18),
                0.9,
            );
        }
        self.toast(cue.message);
    }
}
