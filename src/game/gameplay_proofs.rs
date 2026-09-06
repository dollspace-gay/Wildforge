//! Hardware-backed tests of the actual client interaction path. The runner
//! creates a temporary working directory so no personal save or identity is used.

use crate::camera::Camera;
use crate::game::Game;
use crate::inventory::{Inventory, ItemStack};
use crate::physics::Player;
use crate::registry::AIR;
use crate::world::World;
use crate::{raycast, server, world};
use glam::Vec3;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::x11::EventLoopBuilderExtX11;
use winit::window::{Window, WindowId};

#[path = "gameplay_proofs/guest.rs"]
mod guest;

#[derive(Default)]
struct ProofApp {
    failures: Vec<String>,
    ran: bool,
}

impl ApplicationHandler for ProofApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.ran {
            return;
        }
        self.ran = true;
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Wildforge gameplay regression proof")
                        .with_inner_size(PhysicalSize::new(960, 640)),
                )
                .expect("create real game window"),
        );
        let mut game = Game::new(window);
        assert!(
            game.renderer.adapter_hardware,
            "the proof requires a hardware GPU"
        );
        assert_eq!(game.renderer.adapter_backend, "Vulkan");
        for (name, item, held, staged, accepted) in [
            ("whole stack", "base:clay_ball", 32, 0, 32),
            ("remaining appetite", "base:clay_ball", 16, 60, 4),
            ("unwanted goods", "base:cobblestone", 16, 0, 0),
            ("food delivery", "base:bread", 8, 0, 8),
        ] {
            if catch_unwind(AssertUnwindSafe(|| {
                depot_case(&mut game, name, item, held, staged, accepted);
            }))
            .is_err()
            {
                self.failures.push(name.to_string());
            }
        }
        if catch_unwind(AssertUnwindSafe(|| guest::run(&mut game))).is_err() {
            self.failures.push("native guest entry".into());
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn depot_case(game: &mut Game, name: &str, item_name: &str, held: u32, staged: u32, accepted: u32) {
    let reg = game.content.reg.clone();
    assert!(reg.material_errors.is_empty(), "{:?}", reg.material_errors);
    let depot = reg.block_id("proof:depot").expect("fixture depot loaded");
    let item = reg.item_id(item_name).expect("fixture goods exist");
    game.runtime.set_local(server::Server::new(
        World::new(42, PathBuf::from("saves/proof"), reg.clone()),
        0.3,
        5,
    ));
    let pos = crate::planet::BlockPos::of_world(8, 201, 10).unwrap();
    game.runtime.local_mut().world.ensure_chunk(pos.chunk());
    for x in 6..=10 {
        for z in 6..=12 {
            for y in 200..=204 {
                game.runtime
                    .local_mut()
                    .world
                    .set_block_at(crate::planet::BlockPos::of_world(x, y, z).unwrap(), AIR);
            }
        }
    }
    assert!(game.runtime.local_mut().world.place_block_at(pos, depot));
    if staged > 0 {
        assert_eq!(
            game.runtime
                .local_mut()
                .world
                .depot_deposit(pos, &ItemStack::new(&reg, item, staged)),
            staged
        );
    }
    game.player = Player::new_at(
        crate::planet::EntityPos::from_local(crate::planet::Face::PosZ, Vec3::new(8.5, 200.0, 7.5))
            .unwrap(),
    );
    game.camera = Camera::new(game.player.eye().render_pos(), 1.5);
    game.camera.yaw = std::f32::consts::FRAC_PI_2;
    game.camera.pitch = 0.0;
    let hit = raycast::raycast_at(
        &game.runtime.local().world,
        game.player.eye(),
        game.camera.local_forward(),
        game.reach(),
    );
    assert_eq!(
        hit.map(|h| h.block),
        Some(pos),
        "the player's crosshair hits the depot"
    );
    game.inventory = Inventory::new();
    game.inventory.slots[0] = Some(ItemStack::new(&reg, item, held));
    game.input.hotbar_sel = 0;
    game.input.action_cooldown = 0.0;
    game.input.right_held = true;
    game.survival.hunger = 10.0;
    game.content.scripts.kv.borrow_mut().clear();
    // The same method called by the frame loop for mouse input, including
    // the raycast, food interaction gate, and container routing.
    game.interact(1.0 / 30.0);
    let remaining = game.inventory.count_of(item);
    let Some(world::BlockEntity::Depot(state)) = game.runtime.view().block_entity_at(&pos) else {
        panic!("depot remains present");
    };
    let stock: u32 = state
        .storage
        .iter()
        .flatten()
        .filter(|s| s.item == item)
        .map(|s| s.count)
        .sum();
    let standing = game
        .read_player_kv("proof_standing")
        .unwrap_or_else(|| "0".into());
    eprintln!(
        "PROOF depot {name}: before held={held} staged={staged}; after held={remaining} staged={stock} standing={standing}"
    );
    assert_eq!(
        (remaining, stock, standing),
        (
            held - accepted,
            staged + accepted,
            (accepted * 2).to_string()
        ),
        "{name}: only accepted units leave the hand and earn standing"
    );
}

#[test]
#[ignore = "requires a real Vulkan GPU and X11 display; run tools/run_gameplay_proofs.py"]
fn real_client_interactions_and_guest_entry() {
    assert!(
        PathBuf::from("mods/proof/mod.toml").exists(),
        "use the isolated gameplay proof runner"
    );
    let event_loop = EventLoop::builder()
        .with_x11()
        .with_any_thread(true)
        .build()
        .expect("X11 event loop");
    let mut app = ProofApp::default();
    event_loop.run_app(&mut app).expect("run gameplay proof");
    assert!(app.ran, "the game actually ran");
    assert!(
        app.failures.is_empty(),
        "gameplay failures: {:?}",
        app.failures
    );
}
