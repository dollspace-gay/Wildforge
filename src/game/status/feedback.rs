//! Feedback graphical status adapter.

use crate::audio::Sfx;
use crate::game::Game;

impl Game {
    pub(in crate::game) fn present_dross_cue(&mut self, cue: crate::dross::DrossCue) {
        match cue.kind {
            crate::dross::DrossCueKind::BreachForecast => self.sfx(Sfx::DrossWarning(5)),
            crate::dross::DrossCueKind::Breach { activity } => self.sfx(Sfx::DrossBreach(
                activity.unwrap_or(crate::dross::ScarActivityHandler::Shear),
            )),
        }
        self.toast(cue.accessible_text().to_string());
    }

    pub(in crate::game) fn toast(&mut self, msg: String) {
        self.presentation.toasts.push((msg, 4.0));
        if self.presentation.toasts.len() > 5 {
            self.presentation.toasts.remove(0);
        }
    }

    /// Minimal capture/stamp interaction (spec Part 1.4): a `!` chat line is
    /// a world command instead of chat. Everything except `stamp` runs on
    /// the world directly; `stamp` additionally draws on the player's own
    /// inventory and creative flag.
    pub(in crate::game) fn template_command(&mut self, line: &str) -> Vec<String> {
        use crate::world::multiblock::Rotation;
        use crate::world::template::{parse_block_pos, parse_rot};

        let line = line.trim_start_matches(['!', '/']);
        let mut tokens = line.split_whitespace();
        let Some(cmd) = tokens.next().map(str::to_ascii_lowercase) else {
            return vec!["templates: capture | ghost | cost | list | drop | cancel | stamp | spawn | despawn".into()];
        };
        if cmd != "stamp" {
            return self.runtime.local_mut().world.template_command(line);
        }
        let Some(name) = tokens.next() else {
            return vec!["usage: stamp <name> <face> <u> <y> <v> [rot]".into()];
        };
        let Some(pos) = parse_block_pos(&mut tokens) else {
            return vec!["stamp: bad anchor — <face> <u> <y> <v> [rot]".into()];
        };
        let rot = parse_rot(tokens.next()).unwrap_or(Rotation::R0);
        let Some(t) = self.runtime.local().world.template(name).cloned() else {
            return vec![format!("no template named {name}")];
        };
        match self.runtime.local_mut().world.stamp_instant(
            &t,
            pos,
            rot,
            &mut self.inventory,
            self.creative,
        ) {
            Ok(msg) => vec![msg],
            Err(error) => vec![format!("stamp {name}: {error}")],
        }
    }
}
