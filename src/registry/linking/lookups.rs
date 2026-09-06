//! Qualified cross-reference rules shared by the ordered linking passes.

use crate::registry::{BlockId, Ingredient, ItemId, Registry, qualify};

pub(super) fn lookup_item(reg: &Registry, modid: &str, name: &str) -> Option<ItemId> {
    reg.item_id(&qualify(modid, name))
        .or_else(|| reg.item_id(name))
}

pub(super) fn lookup_block(reg: &Registry, modid: &str, name: &str) -> Option<BlockId> {
    reg.block_id(&qualify(modid, name))
        .or_else(|| reg.block_id(name))
}

pub(super) fn resolve_ing(reg: &Registry, modid: &str, name: &str) -> Option<Ingredient> {
    if let Some(tag) = name.strip_prefix('#') {
        reg.tags
            .get(&qualify(modid, tag))
            .filter(|items| !items.is_empty())
            .map(|items| Ingredient::Any(items.clone()))
    } else {
        lookup_item(reg, modid, name).map(Ingredient::One)
    }
}

/// Resolve a (possibly bare) piece reference to its qualified name if the
/// piece is registered. Used only by the pool/assembly resolver after all
/// pieces are loaded.
pub(super) fn lookup_piece(reg: &Registry, modid: &str, name: &str) -> Option<String> {
    let id = qualify(modid, name);
    reg.pieces.iter().any(|p| p.name == id).then_some(id)
}

pub(super) fn parse_direction4(name: &str) -> Option<crate::planet::Direction4> {
    use crate::planet::Direction4;
    match name {
        "east" => Some(Direction4::East),
        "north" => Some(Direction4::North),
        "west" => Some(Direction4::West),
        "south" => Some(Direction4::South),
        _ => None,
    }
}
