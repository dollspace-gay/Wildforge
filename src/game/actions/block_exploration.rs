//! Block exploration in the ordered graphical action pipeline.

use crate::game::Game;
use crate::game::navigation::Screen;
use crate::net;
use crate::physics::Player;
use crate::raycast;
use crate::world;

impl Game {
    pub(in crate::game) fn use_dungeon_entry_block(
        &mut self,
        h: &raycast::PlanetHit,
        s: &str,
    ) -> bool {
        self.input.action_cooldown = 0.5;
        self.input.right_held = false;
        let name = s.trim_start_matches("dungeon_entry:").to_string();
        if let Some(rc) = &self.multiplayer.remote {
            rc.session.send(&net::C2S::DungeonUse {
                pos: h.block,
                kind: 0,
            });
            return true;
        }
        match self
            .runtime
            .local_mut()
            .world
            .enter_dungeon(0, self.player.pos, &name)
        {
            Some(spawn) => {
                self.player = Player::new_at(spawn);
                self.toast("The dark takes you. The door is behind you.".to_string());
            }
            None => self.toast("The way below does not answer.".to_string()),
        }
        true
    }
    pub(in crate::game) fn use_dungeon_exit_block(&mut self, h: &raycast::PlanetHit) -> bool {
        self.input.action_cooldown = 0.5;
        self.input.right_held = false;
        if let Some(rc) = &self.multiplayer.remote {
            rc.session.send(&net::C2S::DungeonUse {
                pos: h.block,
                kind: 1,
            });
            return true;
        }
        match self
            .runtime
            .local_mut()
            .world
            .exit_dungeon(0, self.player.pos)
        {
            Some(back) => {
                self.player = Player::new_at(back);
                self.toast("Daylight again. The deep forgets you.".to_string());
            }
            None => self.toast("This door leads nowhere.".to_string()),
        }
        true
    }
    pub(in crate::game) fn use_dungeon_checkpoint_block(&mut self, h: &raycast::PlanetHit) -> bool {
        self.input.action_cooldown = 0.5;
        self.input.right_held = false;
        if let Some(rc) = &self.multiplayer.remote {
            rc.session.send(&net::C2S::DungeonUse {
                pos: h.block,
                kind: 2,
            });
            return true;
        }
        self.runtime
            .local_mut()
            .world
            .set_dungeon_checkpoint(self.player.pos);
        self.toast("The shrine remembers you.".to_string());
        true
    }
    pub(in crate::game) fn use_chest_block(&mut self, h: &raycast::PlanetHit) -> bool {
        self.input.action_cooldown = 0.3;
        if let Some(rc) = &self.multiplayer.remote {
            rc.session.send(&net::C2S::OpenContainer { pos: h.block });
            return true;
        }
        let e = self
            .runtime
            .local_mut()
            .world
            .ensure_block_entity_at(h.block, world::BlockEntity::Chest(Default::default()));
        if let world::BlockEntity::Chest(c) = e
            && c.wild_owned
        {
            c.wild_owned = false;
            self.runtime
                .local_mut()
                .world
                .add_ire_at_surface(h.block.surface(), 1.0);
            self.toast("The wild keeps its trophies.".to_string());
        }
        self.set_screen(Screen::Chest(h.block));
        true
    }
    pub(in crate::game) fn use_folio_block(&mut self, h: &raycast::PlanetHit) -> bool {
        self.input.action_cooldown = 0.35;
        self.input.right_held = false;
        self.open_discovery_folio(h.block);
        true
    }
    pub(in crate::game) fn use_writing_block(&mut self, h: &raycast::PlanetHit) -> bool {
        self.input.action_cooldown = 0.35;
        self.input.right_held = false;
        self.copy_at_writing_surface(h.block);
        true
    }
    pub(in crate::game) fn use_laboratory_block(&mut self, h: &raycast::PlanetHit) -> bool {
        self.input.action_cooldown = 0.35;
        self.input.right_held = false;
        self.exchange_discovery_apparatus_item(h.block);
        true
    }
    pub(in crate::game) fn use_lens_block(&mut self, h: &raycast::PlanetHit) -> bool {
        self.input.action_cooldown = 0.35;
        self.input.right_held = false;
        self.assemble_tuning_lens(h.block);
        true
    }
    pub(in crate::game) fn use_binding_frame_block(&mut self, h: &raycast::PlanetHit) -> bool {
        self.input.action_cooldown = 0.35;
        self.input.right_held = false;
        self.operate_binding_frame(h.block);
        true
    }
    pub(in crate::game) fn use_alchemy_block(&mut self, h: &raycast::PlanetHit) -> bool {
        self.input.action_cooldown = 0.25;
        self.input.right_held = false;
        self.operate_alchemy_contextual(h.block);
        true
    }
}
