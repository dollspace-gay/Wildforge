//! Existing tooltip presentation contracts.

use super::*;

fn reg() -> Registry {
    crate::registry::load(std::path::Path::new("/nonexistent-mods-dir"))
}

fn lines_for(reg: &Registry, name: &str) -> Vec<String> {
    let item = reg.item_id(name).unwrap_or_else(|| panic!("no {name}"));
    item_tooltip_lines(reg, ItemStack::new(reg, item, 1))
        .into_iter()
        .map(|(t, _)| t)
        .collect()
}

/// The complaint that started this: a charm in the pack told you
/// nothing but its name, and only once you had put it in your hand.
#[test]
fn a_charm_says_what_wearing_it_does() {
    let reg = reg();
    for (item, want) in [
        ("base:charm_quiet", "MINDS YOU LESS"),
        ("base:charm_bark", "ARMOUR"),
        ("base:charm_hunger", "HUNGER"),
    ] {
        let lines = lines_for(&reg, item);
        assert!(
            lines.iter().any(|l| l.contains(want)),
            "{item} should explain itself, got {lines:?}"
        );
    }
}

/// Every line is derived, so the numbers can never contradict the
/// mechanic they describe.
#[test]
fn tools_food_and_armour_read_off_their_own_numbers() {
    let reg = reg();
    let pick = lines_for(&reg, "base:stone_pickaxe");
    assert_eq!(pick[0], "STONE PICKAXE");
    assert!(pick.iter().any(|l| l.starts_with("PICKAXE - TIER")));
    assert!(pick.iter().any(|l| l.starts_with("DURABILITY")));

    let potato = lines_for(&reg, "base:potato");
    assert!(potato.iter().any(|l| l.contains("HUNGER")));
    assert!(potato.iter().any(|l| l.contains("VEGETABLE")));
    // Food's `durability` is a freshness clock, not tool wear —
    // calling it "durability" said nothing about the only thing it
    // governs, which is how long you have to eat the thing.
    assert!(
        potato.iter().any(|l| l.starts_with("FRESH:")),
        "food shows its clock as freshness, got {potato:?}"
    );
    assert!(!potato.iter().any(|l| l.starts_with("DURABILITY")));
}

/// The font is a 5x7 uppercase bitmap with no fallback glyph: any
/// character it doesn't know renders as a hole in the sentence.
#[test]
fn every_shipped_item_tooltip_is_renderable_and_titled() {
    let reg = reg();
    for (i, def) in reg.items.iter().enumerate() {
        let stack = ItemStack::new(&reg, crate::registry::ItemId(i as u16), 1);
        let lines = item_tooltip_lines(&reg, stack);
        assert_eq!(
            lines[0].0,
            def.label.to_uppercase(),
            "{} leads with its name",
            def.name
        );
        for (text, _) in &lines {
            // is_ascii() is not the test: '~' is ASCII and draws
            // as a hole. Ask the font itself.
            if let Some(bad) = text.chars().find(|&c| !crate::ui::has_glyph(c)) {
                panic!(
                    "{}: {text:?} contains {bad:?}, which the font draws as a blank",
                    def.name
                );
            }
        }
    }
}

#[test]
fn ecology_tooltips_are_qualitative_and_explain_sequestered_dross() {
    let reg = reg();
    let ashlace = reg.item_id("base:ashlace_tissue").unwrap();
    let lines = item_tooltip_lines_with_current(&reg, ItemStack::new(&reg, ashlace, 1), Some(137))
        .into_iter()
        .map(|(text, _)| text)
        .collect::<Vec<_>>();
    assert!(lines.iter().any(|line| line.contains("BINDS DROSS")));
    assert!(lines.iter().any(|line| line.starts_with("CURRENT: ")));
    assert!(
        lines
            .iter()
            .filter(|line| line.starts_with("CURRENT: "))
            .all(|line| !line.chars().any(|character| character.is_ascii_digit()))
    );
}

#[test]
fn every_preparation_explains_its_target_cost_or_limit() {
    let reg = reg();
    for preparation in reg.preparations.values() {
        let lines = lines_for(&reg, &preparation.output_item);
        assert!(
            lines.iter().any(|line| line.starts_with("USE: ")),
            "{} has no application instruction: {lines:?}",
            preparation.id
        );
        assert!(
            lines.iter().any(|line| {
                line.contains("DOES NOT")
                    || line.contains("BOUNDED")
                    || line.contains("PAID")
                    || line.contains("MOVES")
                    || line.contains("NEVER")
                    || line.contains("DRAIN")
                    || line.contains("THROUGHPUT")
            }),
            "{} hides its principal cost or limit: {lines:?}",
            preparation.id
        );
    }
}

#[test]
fn wand_and_frame_tooltips_publish_the_installed_working_catalogue() {
    let reg = reg();
    let wand = lines_for(&reg, "base:bound_wand");
    for label in [
        "TRACE",
        "GLEAM",
        "KINDLE",
        "NUDGE",
        "ROOTWAKE",
        "DRAW",
        "FIELDMEND",
        "HOLDFAST",
    ] {
        assert!(
            wand.iter().any(|line| line.contains(label)),
            "wand tooltip omitted {label}: {wand:?}"
        );
    }
    assert!(wand.iter().any(|line| line.contains("CTRL + USE")));

    let frame = lines_for(&reg, "base:binding_frame");
    for label in [
        "SETTLING RITE",
        "ROOTING BED",
        "WARD BOUNDARY",
        "TRANSFER CIRCLE",
    ] {
        assert!(
            frame.iter().any(|line| line.contains(label)),
            "binding-frame tooltip omitted {label}: {frame:?}"
        );
    }
}
