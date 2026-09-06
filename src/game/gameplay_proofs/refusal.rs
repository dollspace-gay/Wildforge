//! Unsupported guest use must finish before spending or creating local state.

use super::Game;
use crate::inventory::{Inventory, ItemStack};
use crate::raycast;
use crate::tests::fixtures::TestHost;

pub(super) fn prepare(host: &TestHost) {
    host.with(|_, server| {
        let water = server.world.reg.water_for_volume(8);
        for x in -1..=1 {
            for z in 2..=4 {
                server
                    .world
                    .set_block_at(crate::planet::BlockPos::of_world(x, 200, z).unwrap(), water);
            }
        }
    });
}

pub(super) fn run(game: &mut Game) {
    let boat = game.content.reg.item_id("base:boat").unwrap();
    game.inventory = Inventory::new();
    game.inventory.slots[0] = Some(ItemStack::new(&game.content.reg, boat, 1));
    game.input.hotbar_sel = 0;
    game.input.action_cooldown = 0.0;
    game.input.right_held = true;
    let angles = (game.camera.yaw, game.camera.pitch);
    game.camera.yaw = std::f32::consts::FRAC_PI_2;
    game.camera.pitch = -0.6;
    assert!(
        raycast::raycast_water_at(
            &game.runtime.view(),
            game.player.eye(),
            game.camera.local_forward(),
            game.reach(),
        )
        .is_some(),
        "the native guest must actually target water"
    );
    let before = game.inventory.slots[0];
    // Ordinary mouse interaction arbitration, including targeting and held use.
    game.interact(1.0 / 30.0);
    assert_eq!(game.inventory.slots[0], before, "refusal retains the boat");
    assert!(!game.input.right_held, "refusal consumes the action");
    assert!(
        game.presentation
            .toasts
            .iter()
            .any(|(text, _)| { text == "This action is not supported in multiplayer yet." })
    );
    assert!(game.runtime.is_guest());
    assert!(game.gen_pool.is_none());
    (game.camera.yaw, game.camera.pitch) = angles;
    game.inventory = Inventory::new();
}
