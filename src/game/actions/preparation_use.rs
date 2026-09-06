//! Preparation use in the ordered graphical action pipeline.

use super::ActionFrame;
use crate::game::Game;

impl Game {
    pub(in crate::game) fn interact_preparation_use(&mut self, frame: &ActionFrame) -> bool {
        let reg = &frame.reg;
        let hit = &frame.hit;
        // Stable preparation bottles are not generic food. Their saved dose
        // identity decides the bounded application and the host performs the
        // water/Current/status transaction.
        let preparation_application =
            self.inventory.slots[self.input.hotbar_sel].and_then(|stack| {
                (stack.arcane_id != 0).then_some(())?;
                let item_name = &reg.item(stack.item).name;
                reg.preparations
                    .values()
                    .find(|definition| definition.output_item == *item_name)
                    .map(|definition| definition.application)
            });
        if self.input.right_held
            && self.input.action_cooldown <= 0.0
            && let Some(application) = preparation_application
        {
            let adjacent_slot = (self.input.hotbar_sel + 1) % crate::inventory::HOTBAR_SLOTS;
            let adjacent_id = self.inventory.slots[adjacent_slot]
                .filter(|stack| stack.count == 1)
                .map_or(0, |stack| stack.arcane_id);
            let target = match application {
                crate::alchemy::ApplicationKind::Drink => crate::alchemy::AlchemyTarget::SelfActor,
                crate::alchemy::ApplicationKind::Plot => {
                    let Some(hit) = &hit else {
                        self.toast("Aim Root Wash at one plot or rooting bed.".into());
                        return true;
                    };
                    crate::alchemy::AlchemyTarget::Plot(hit.block)
                }
                crate::alchemy::ApplicationKind::Wash => {
                    if let Some(hit) = &hit {
                        crate::alchemy::AlchemyTarget::Surface(hit.block)
                    } else if adjacent_id != 0 {
                        crate::alchemy::AlchemyTarget::Item(adjacent_id)
                    } else {
                        self.toast(
                            "Aim Ashlace Wash at a small surface, or carry one stable tool immediately right of it."
                                .into(),
                        );
                        return true;
                    }
                }
                crate::alchemy::ApplicationKind::Coat => {
                    if adjacent_id == 0 {
                        self.toast(
                            "Carry one stable botanical specimen immediately right of the Frostlace jar."
                                .into(),
                        );
                        return true;
                    }
                    crate::alchemy::AlchemyTarget::Item(adjacent_id)
                }
            };
            self.use_selected_preparation(target);
            self.input.right_held = false;
            self.input.action_cooldown = 0.3;
            return true;
        }

        false
    }
}
