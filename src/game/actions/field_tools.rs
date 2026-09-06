//! Field tools in the ordered graphical action pipeline.

use crate::audio::BreakMat;
use crate::game::Game;
use crate::game::navigation::Screen;
use crate::net;
use crate::registry;
use crate::registry::ToolKind;
use crate::world;

impl Game {
    /// Read the country at a spot and toast it: the prospector's
    /// verdict, shared by pick strikes and standing survey cairns.
    /// Compass octant of a great-circle bearing (clockwise from local north).
    pub(in crate::game) fn octant_bearing(bearing: Option<f64>) -> &'static str {
        let Some(bearing) = bearing else {
            return "here";
        };
        let names = [
            "north",
            "northeast",
            "east",
            "southeast",
            "south",
            "southwest",
            "west",
            "northwest",
        ];
        names[((bearing.to_degrees() + 22.5).rem_euclid(360.0) / 45.0) as usize]
    }

    /// Commit the sign editor: write the entity (host) or send it
    /// (guest), then return to play.
    pub(in crate::game) fn commit_sign(&mut self, pos: crate::planet::BlockPos) {
        let lines = self.ui_state.sign_lines.clone();
        if let Some(rc) = &self.multiplayer.remote {
            rc.session.send(&net::C2S::SetSign {
                pos,
                lines: lines.clone(),
            });
        }
        // Local worlds and hosts apply directly (the host broadcast
        // happens on the C2S path for guests' own edits).
        if self.multiplayer.remote.is_none() {
            self.runtime
                .local_mut()
                .world
                .insert_block_entity_at(pos, world::BlockEntity::Sign(world::SignState { lines }));
            if let Some(hst) = &mut self.multiplayer.host {
                hst.broadcast_sign_at(pos, &self.ui_state.sign_lines);
            }
        }
        self.set_screen(Screen::Playing);
    }

    pub(in crate::game) fn toast_prospect(&mut self, pos: crate::planet::SurfacePos) {
        let r = self.runtime.view().prospect_at(pos);
        let mut lines: Vec<String> = Vec::new();
        if let Some(province) = &r.province_name {
            if let Some(bedrock) = r.bedrock {
                lines.push(format!("{province}: {} country.", bedrock.label()));
            } else {
                lines.push(province.clone());
            }
        }
        match r.pluton {
            Some(hit) if hit.distance == 0 => lines.push("Granite country underfoot.".into()),
            Some(hit) => lines.push(format!(
                "Granite country ~{} blocks {}.",
                hit.distance,
                Self::octant_bearing(hit.bearing)
            )),
            None => lines.push("No batholith in the pick's reach.".into()),
        }
        if let Some(hit) = r.volcano {
            if hit.distance == 0 {
                lines.push("Volcanic ground — you're standing on it.".into());
            } else {
                lines.push(format!(
                    "Volcanic rock ~{} blocks {}.",
                    hit.distance,
                    Self::octant_bearing(hit.bearing)
                ));
            }
        }
        if let Some(hit) = r.pipe {
            if hit.distance == 0 {
                lines.push("BLUE GROUND. A pipe under this very spot.".into());
            } else {
                lines.push(format!(
                    "Blue ground! A pipe ~{} blocks {}.",
                    hit.distance,
                    Self::octant_bearing(hit.bearing)
                ));
            }
        }
        if let Some(hit) = r.geode {
            if hit.distance == 0 {
                lines.push("A hollow ring underfoot — geode.".into());
            } else {
                lines.push(format!(
                    "A hollow ring ~{} blocks {}.",
                    hit.distance,
                    Self::octant_bearing(hit.bearing)
                ));
            }
        }
        for l in lines {
            self.toast(l);
        }
    }

    /// Break-sound family for a block, from its tool class.
    pub(in crate::game) fn break_mat(&self, b: registry::BlockId) -> BreakMat {
        match self.content.reg.block(b).tool {
            Some(ToolKind::Pickaxe) => BreakMat::Stone,
            Some(ToolKind::Axe) => BreakMat::Wood,
            Some(ToolKind::Shovel) => BreakMat::Soft,
            Some(ToolKind::Hoe) | None => BreakMat::Leafy,
        }
    }
}
