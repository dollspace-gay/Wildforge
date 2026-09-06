//! Per-provider registration owns texture allocation and deferred cross-references.

#[path = "register_blocks.rs"]
mod blocks;
#[path = "register_items.rs"]
mod items;
#[path = "register_references.rs"]
mod references;
#[path = "textures.rs"]
mod textures;

use super::pending::PendingContent;
use crate::registry::{Registry, schema::RawMod};
use textures::Textures;

#[derive(Default)]
struct Registration {
    textures: Textures,
    pending: PendingContent,
}

pub(in crate::registry) fn register(reg: &mut Registry, raws: &[RawMod]) -> PendingContent {
    let mut registration = Registration::default();
    for raw in raws {
        if raw.info.id.is_empty() {
            continue;
        }
        let mut errs = Vec::new();
        registration.blocks(reg, raw, &mut errs);
        registration.items(reg, raw, &mut errs);
        registration.references(raw, &mut errs);
        let mut info = raw.info.clone();
        if !errs.is_empty() {
            info.error = Some(errs.join("; "));
        }
        reg.mods.push(info);
    }
    registration.textures.publish(reg);
    registration.pending
}
