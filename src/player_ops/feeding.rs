//! Animal feeding eligibility and its visible tame/breed transition.

use crate::mobs::Mob;
use crate::registry::{AnimalDef, ItemId};

#[derive(Clone, Copy, Debug)]
pub(crate) struct FeedPlan {
    food: ItemId,
    tame: bool,
    breed: bool,
}

impl FeedPlan {
    /// Inventory spending and transport reach are admitted by the adapter.
    pub(crate) fn prepare(definition: &AnimalDef, mob: &Mob, held: Option<ItemId>) -> Option<Self> {
        let food = definition.breed_food?;
        let breed = mob.breed_cd <= 0.0 && !mob.fed;
        let tame = !mob.tamed;
        let eligible =
            !definition.hostile && mob.growth >= 1.0 && held == Some(food) && (breed || tame);
        if !eligible {
            return None;
        }
        Some(Self { food, tame, breed })
    }

    pub(crate) fn food(self) -> ItemId {
        self.food
    }

    /// Returns whether this meal completed taming. The same state transition
    /// supplies guest prediction until the next authoritative mob snapshot.
    pub(crate) fn apply(self, mob: &mut Mob) -> bool {
        let now_tamed = self.tame && mob.feed_tame();
        if self.breed {
            mob.fed = true;
        }
        mob.calm = 30.0;
        now_tamed
    }
}
