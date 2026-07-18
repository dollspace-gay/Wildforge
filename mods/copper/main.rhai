// Copper Age — demo script: counts mined copper, greets the player,
// and celebrates milestones. State lives in engine storage, so it
// survives hot reloads and is saved with the world.

fn on_world_start(world) {
    hud_message("Copper Age loaded - dig for copper ore!");
}

fn on_block_break(x, y, z, block) {
    if block == "copper:ore" {
        let n = storage_get("mined");
        let count = if n == "" { 1 } else { n.parse_int() + 1 };
        storage_set("mined", count.to_string());
        hud_message("copper mined: " + count);
        if count == 5 {
            hud_message("5 copper! have a bonus stick");
            give("base:stick", 1);
            play_sound("craft");
        }
    }
    true
}
