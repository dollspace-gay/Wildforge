//! Shared separator ticking over the physical BlockStore contract.

use crate::world::BlockEntity;
use crate::world::multiblock::BlockStore;
use crate::machines::MachineHandler;
use crate::world::SEPARATE_SECS;

pub(in crate::world) fn tick_separator_machines<B: BlockStore>(store: &mut B, dt: f32) {
    let keys: Vec<B::Pos> = store
        .block_entities()
        .iter()
        .filter(|(_, e)| {
            matches!(e, BlockEntity::Multiblock(m)
                if m.kind.handler(store.reg()) == Some(MachineHandler::Separator))
        })
        .map(|(k, _)| *k)
        .collect();
    for pos in keys {
        let Some(BlockEntity::Multiblock(mut sp)) = store.block_entities_mut().remove(&pos) else {
            continue;
        };
        let def = store.reg().machine(sp.kind).cloned();
        let working = sp.powder >= 1 && sp.separator_fuel >= 1;
        if !working {
            sp.progress = 0.0;
        } else {
            sp.progress += dt;
            if sp.progress >= SEPARATE_SECS {
                sp.progress = 0.0;
                sp.powder -= 1;
                sp.separator_fuel -= 1;
                sp.neodymium += 1;
                sp.cerium += 2;
            }
        }
        let want = if working {
            def.as_ref()
                .and_then(|def| def.mouth_lit.clone())
                .unwrap_or_else(|| "base:separator_lit".to_string())
        } else {
            def.as_ref()
                .map(|def| def.mouth.clone())
                .unwrap_or_else(|| "base:separator".to_string())
        };
        if Some(store.get_block(pos)) != store.reg().block_id(&want) {
            store.swap_block_keep_entity(pos, &want);
        }
        store
            .block_entities_mut()
            .insert(pos, BlockEntity::Multiblock(sp));
    }
}
