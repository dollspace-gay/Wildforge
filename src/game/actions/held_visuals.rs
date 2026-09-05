//! Held visuals in the ordered graphical action pipeline.

use crate::game::Game;
use crate::world::TerrainRead;
use crate::atlas;
use crate::inventory::ItemStack;
use crate::mobs;
use crate::registry::ItemId;
use crate::style;
use glam::Vec3;

impl Game {
    /// The tile set dressing a humanoid for a given style.
    pub(in crate::game) fn humanoid_art(st: style::Style) -> mobs::HumanoidArt {
        let b = |n: &str| *atlas::builtin_slots().get(n).unwrap_or(&0);
        mobs::HumanoidArt {
            skin: style::skin_tile(&st),
            face: style::face_tile(&st),
            hair: style::hair_tile(&st),
            hair_front: style::hair_front_tile(&st).unwrap_or(0),
            hair_top: style::hair_top_tile(&st),
            beard: style::beard_tile(&st),
            shirt: style::shirt_tile(&st),
            trousers: style::trouser_tile(&st),
            boot: b("player_boot"),
            long_hair: st.hair_style == 3,
            skirt: st.legwear == 1,
            build: st.build,
        }
    }

    /// How a held item renders in a remote hand.
    pub(in crate::game) fn held_art(&self, item: Option<ItemId>) -> mobs::HeldArt {
        let Some(item) = item else {
            return mobs::HeldArt::None;
        };
        let def = self.content.reg.item(item);
        match def.places {
            Some(b) if !self.content.reg.block(b).cross => {
                mobs::HeldArt::Cube(self.content.reg.block(b).tiles)
            }
            _ => mobs::HeldArt::Sprite(def.icon),
        }
    }

    pub(in crate::game) fn held_art_stack(&self, stack: Option<ItemStack>) -> mobs::HeldArt {
        let Some(stack) = stack else {
            return mobs::HeldArt::None;
        };
        if let Some(visual) = self.runtime.view().implement_visual(stack) {
            return self.held_art_implement(visual, |wire| Some(ItemId(wire)));
        }
        self.held_art(Some(stack.item))
    }

    pub(in crate::game) fn held_art_implement(
        &self,
        visual: crate::implements::ImplementVisual,
        mut map: impl FnMut(u16) -> Option<ItemId>,
    ) -> mobs::HeldArt {
        let icon = |item: Option<ItemId>| {
            item.map(|item| self.content.reg.item(item).icon)
                .unwrap_or(crate::atlas::UNKNOWN_SLOT)
        };
        mobs::HeldArt::Wand {
            body: icon(map(visual.body)),
            reservoir: icon(map(visual.reservoir)),
            focus: icon(map(visual.focus)),
            binding: icon(map(visual.binding)),
            focus_shape: visual.focus_shape,
            charge_band: visual.charge_band.min(3),
        }
    }

    /// The carried light of a held item, if any: an explicit item glow,
    /// or derived from the light of the block it places (torches).
    pub(in crate::game) fn held_glow(&self, item: ItemId) -> Option<(Vec3, f32)> {
        let def = self.content.reg.item(item);
        if let Some(g) = def.glow {
            return Some((Vec3::from(g), 14.0));
        }
        let b = def.places?;
        let bd = self.content.reg.block(b);
        if bd.light_emit == 0 {
            return None;
        }
        let emit = bd.light_emit.max(1) as f32;
        let color = Vec3::new(
            bd.light_rgb[0] as f32 / emit,
            bd.light_rgb[1] as f32 / emit,
            bd.light_rgb[2] as f32 / emit,
        );
        Some((color * 1.8 * (emit / 14.0), emit + 2.0))
    }

    pub(in crate::game) fn implement_glow(
        &self,
        visual: crate::implements::ImplementVisual,
    ) -> Option<(Vec3, f32)> {
        let band = visual.charge_band.min(3);
        if band == 0 {
            return None;
        }
        let strength = f32::from(band) / 3.0;
        Some((
            Vec3::new(0.32, 0.58, 0.95) * (0.32 + strength * 0.48),
            3.5 + strength * 3.5,
        ))
    }
}
