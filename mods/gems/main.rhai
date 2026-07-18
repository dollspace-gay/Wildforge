// Gemstones example mod: milestone toasts via persistent storage.

fn on_world_start(world) {
    hud_message("Gemstones loaded - rubies hide deep below y24");
}

fn on_block_break(x, y, z, block) {
    if block == "gems:ruby_ore" {
        let n = storage_get("found");
        let count = if n == "" { 1 } else { n.parse_int() + 1 };
        storage_set("found", count.to_string());
        hud_message("rubies found: " + count);
        play_sound("craft");
    }
    true
}
