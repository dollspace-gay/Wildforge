//! Entry graphical guest adapter.

use super::RemoteFlow;
use crate::atlas;
use crate::client_session::ContentMap;
use crate::game::Game;
use crate::game::Remote;
use crate::game::navigation::Screen;
use crate::net;
use crate::world::ReplicaWorld;
use std::path::PathBuf;
use std::sync::Arc;

impl Game {
    pub(in crate::game) fn remote_entry_message(
        &mut self,
        r: &mut Remote,
        message: net::S2C,
    ) -> RemoteFlow {
        match message {
            net::S2C::ModFiles(files) => {
                let cache = PathBuf::from("saves/.remote/mods");
                match r.session.install_content(&cache, files) {
                    Ok(registry) => self.content.reg = registry,
                    Err(error) => {
                        self.multiplayer.join_status = format!("FAILED: {error}").to_uppercase();
                        return RemoteFlow::Abort;
                    }
                }
                let mut atlas = atlas::build_atlas(
                    &self.content.reg.tex_files,
                    &atlas::pack_chain(&self.active_pack_id()),
                    &self.content.reg.tex_names,
                );
                let season = self
                    .runtime
                    .view()
                    .season_at_surface(self.player.pos.surface());
                atlas::season_tint(&mut atlas.color, atlas.px, season);
                self.presentation.atlas_season = season;
                self.content.pack_warnings = atlas.warnings;
                self.renderer.set_atlas(
                    &atlas.color,
                    &atlas.material,
                    &atlas.normal,
                    atlas.px,
                    atlas.interior_base,
                    &atlas.layer_params,
                );
                self.toast("Synced the host's mods.".to_string());
            }
            net::S2C::Welcome {
                seed,
                mode,
                time,
                ire,
                palette,
                items,
                your_id,
                your_role,
                roster,
                spawn: _,
                world_name,
                player_state,
            } => {
                let mut world = ReplicaWorld::new(seed, self.content.reg.clone(), ire);
                world.configure_entry(mode.clone(), time);
                self.gen_pool = None; // chunks come by wire
                self.mesh_pool = match crate::game::mesh_jobs::MeshPool::new() {
                    Ok(meshes) => Some(meshes),
                    Err(error) => {
                        eprintln!("guest: mesh workers could not start: {error}");
                        self.mesh_pool = None;
                        self.renderer.clear_chunks();
                        self.in_world = false;
                        self.set_screen(Screen::Title);
                        self.toast(format!("Could not enter host world: {error}"));
                        return RemoteFlow::Abort; // local RemoteSession drops and disconnects
                    }
                };
                r.my_id = your_id;
                r.role = your_role;
                if roster
                    .iter()
                    .find(|presence| presence.id == your_id)
                    .is_some_and(|presence| presence.cached_verification)
                {
                    self.toast("ATProto verified from the server's bounded outage cache.".into());
                }
                let content = ContentMap::new(Arc::clone(&self.content.reg), palette, items);
                self.runtime.set_guest(world, time);
                self.renderer.clear_chunks();
                self.apply_remote_player_state(&content, player_state, true);
                self.creative = mode == "creative";
                self.in_world = false;
                r.session.begin(
                    content,
                    world_name,
                    self.player.pos,
                    std::time::Instant::now(),
                );
                r.session.set_roster(roster);
                r.players.clear();
                r.player_positions.clear();
                r.player_held.clear();
                r.player_implement.clear();
                r.player_style.clear();
                r.player_lerp.clear();
                r.mob_lerp.clear();
                r.player_age = 0.0;
                r.mob_age = 0.0;
                r.wants.clear();
                self.multiplayer.join_status = "PREPARING SAFE WORLD ENTRY...".into();
            }
            net::S2C::EntryManifest { spawn, required } => {
                if let Err(error) = r.session.manifest(spawn, required, &self.runtime.view()) {
                    self.multiplayer.join_status = format!("FAILED: {error}").to_uppercase();
                    self.multiplayer.remote = None;
                    return RemoteFlow::Abort;
                }
            }
            net::S2C::EntryProgress { resident, total } => {
                self.multiplayer.join_status =
                    format!("PREPARING SAFE WORLD ENTRY... {resident}/{total}");
            }
            net::S2C::EntryAccepted => {
                let world_name = match r.session.accepted() {
                    Ok(name) => name,
                    Err(error) => {
                        self.multiplayer.join_status = format!("FAILED: {error}").to_uppercase();
                        self.multiplayer.remote = None;
                        return RemoteFlow::Abort;
                    }
                };
                self.in_world = true;
                self.set_screen(Screen::Playing);
                self.multiplayer.join_status.clear();
                self.toast(format!("Joined {}.", world_name.to_uppercase()));
            }
            net::S2C::Refused(why) => {
                r.session.close();
                if self.in_world {
                    // Kicked mid-game: a clean exit, not a broken
                    // half-local world.
                    self.toast(format!("Removed by host: {}", why.detail));
                    self.quit_to_title();
                } else {
                    self.multiplayer.join_status =
                        format!("REFUSED: {}", why.detail).to_uppercase();
                    self.multiplayer.remote = None;
                }
                return RemoteFlow::Abort;
            }
            _ => {}
        }
        RemoteFlow::Continue
    }
}
