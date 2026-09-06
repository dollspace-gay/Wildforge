//! Script commands in the ordered graphical action pipeline.

use crate::audio::Sfx;
use crate::game::Game;
use crate::game::navigation::Screen;
use crate::inventory::ItemStack;
use crate::mobs;
use crate::script;
use crate::world;

impl Game {
    /// Apply world mutations queued by scripts during the last dispatch.
    pub(in crate::game) fn apply_script_cmds(&mut self) {
        let reg = self.content.reg.clone();
        for cmd in self.content.scripts.take_cmds() {
            if self.runtime.is_guest() && cmd.requires_authority() {
                eprintln!("scripts: guest world mutation rejected; commands execute on the host");
                continue;
            }
            match cmd {
                script::Cmd::SetBlock(pos, name) => {
                    if let Some(b) = reg.block_id(&name) {
                        if reg.block(b).arcane_ecology.is_some() {
                            eprintln!(
                                "arcane ecology: script placement of {name} rejected; lifecycle sites are engine-owned"
                            );
                            continue;
                        }
                        if reg.block(b).name.starts_with("base:scar_")
                            || reg
                                .block(b)
                                .observation
                                .as_ref()
                                .is_some_and(|observation| {
                                    observation
                                        .categories
                                        .iter()
                                        .any(|category| category == "scar")
                                })
                        {
                            eprintln!(
                                "dross: script placement of {name} rejected; scar manifestations are ledger-owned"
                            );
                            continue;
                        }
                        self.runtime.local_mut().world.set_block_authored_at(
                            pos,
                            b,
                            "mod script world event",
                        );
                    }
                }
                script::Cmd::Give(name, n) => {
                    if let Some(item) = reg.item_id(&name) {
                        let item_definition = &reg.items[item.0 as usize];
                        let ecology_product = item_definition.arcane_ecology.is_some()
                            || item_definition
                                .places
                                .is_some_and(|block| reg.block(block).arcane_ecology.is_some())
                            || reg.blocks.iter().any(|block| {
                                block.arcane_ecology.is_some()
                                    && block.drops.is_some_and(|(drop, _)| drop == item)
                            });
                        if ecology_product {
                            eprintln!(
                                "arcane ecology: script give of {name} rejected; growth and harvest are authoritative"
                            );
                            continue;
                        }
                        let mut stack = ItemStack::new(&reg, item, n);
                        if reg.item(item).arcane.is_some() {
                            if n != 1 {
                                eprintln!("arcane: script give rejected a charged stack of {n}");
                                continue;
                            }
                            let Some(at) = self.player.pos.block() else {
                                continue;
                            };
                            if let Err(error) = self.runtime.local_mut().world.bind_arcane_stack_at(
                                at,
                                &mut stack,
                                "mod script discovery",
                            ) {
                                eprintln!("arcane: script give rejected: {error}");
                                continue;
                            }
                        }
                        if let Some(ledger) = &mut self.runtime.local_mut().world.material_ledger
                            && let Err(error) =
                                ledger.record_external_stack(&reg, stack, "mod script give")
                        {
                            eprintln!("materials: script give accounting failed: {error}");
                        }
                        let left = self.inventory.add_stack(&reg, stack);
                        if left > 0 {
                            self.drop_stack(ItemStack {
                                count: left,
                                ..stack
                            });
                        }
                    }
                }
                script::Cmd::Hud(msg) => self.toast(msg),
                script::Cmd::OpenScreen(name) => {
                    // Capability E11: scripts open mod screens by qualified
                    // id; an unknown id says so instead of silently failing.
                    match reg.screen_by_name(&name).map(Screen::Mod) {
                        Some(screen) => self.set_screen(screen),
                        None => self.toast(format!("No screen named {name}.")),
                    }
                }
                script::Cmd::SpawnAnimal(name, pos) => {
                    if let Some(si) = reg.animal_id(&name)
                        && self.runtime.view().mob_count() < world::MOB_CAP
                    {
                        let mut m = mobs::Mob::new_at(si, pos, 0.0);
                        m.health = reg.animals[si].health;
                        self.runtime.local_mut().world.spawn_mob(m);
                    }
                }
                script::Cmd::SpawnNpc(name, pos) => {
                    if let Some(ni) = reg.npc_id(&name)
                        && self.runtime.view().mob_count() < world::MOB_CAP
                        && self.runtime.local().world.npc_count() < world::NPC_CAP
                    {
                        self.runtime.local_mut().world.spawn_npc_at(ni, pos);
                    }
                }
                script::Cmd::QuestProgress {
                    quest_id,
                    objective,
                    n,
                } => {
                    if !objective.is_empty() {
                        self.quest_progress_apply(&quest_id, &objective, n);
                    }
                }
                script::Cmd::QuestAccept(quest_id) => {
                    if let Err(error) = self.quest_accept(&quest_id) {
                        self.toast(error);
                    }
                }
                script::Cmd::ArcaneMoveWorking {
                    mod_id,
                    from,
                    to,
                    resonance,
                    units,
                    reason,
                } => {
                    let result = self
                        .runtime
                        .local_mut()
                        .world
                        .arcane_ledger
                        .as_mut()
                        .ok_or_else(|| "world has no arcane ledger".to_string())
                        .and_then(|ledger| {
                            ledger
                                .mod_working_transfer(
                                    &mod_id,
                                    from,
                                    to,
                                    crate::arcane::Current::single(resonance, units),
                                    &reason,
                                )
                                .map(|_| ())
                                .map_err(|error| error.to_string())
                        });
                    if let Err(error) = result {
                        eprintln!("[mod:{mod_id}] arcane transaction rejected: {error}");
                    }
                }
                script::Cmd::Sound(name) => {
                    let sfx = match name.as_str() {
                        "click" => Some(Sfx::Click),
                        "place" => Some(Sfx::Place),
                        "pickup" => Some(Sfx::Pickup),
                        "hurt" => Some(Sfx::Hurt),
                        "craft" => Some(Sfx::Craft),
                        "splash" => Some(Sfx::Splash),
                        _ => None,
                    };
                    if let Some(s) = sfx {
                        self.sfx(s);
                    }
                }
            }
        }
    }
}
