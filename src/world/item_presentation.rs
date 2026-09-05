//! Shared item observations from registry data and authorized charge readings.

use crate::implements::{ImplementKind, ImplementVisual};
use crate::inventory::ItemStack;
use crate::registry::Registry;

pub(super) fn charm_can_pay(
    registry: &Registry, stack: ItemStack, kind: &str, current: Option<u64>,
) -> bool {
        let definition = registry.item(stack.item);
        let Some(charm) = &definition.charm_def else {
            return false;
        };
        charm.effect.id() == kind
            && stack.arcane_id != 0
            && current
                .is_some_and(|units| {
                    crate::implements::usable_charge(units) >= charm.charge_per_trigger
                })
}

pub(super) fn implement_visual(
    registry: &Registry, kind: &ImplementKind, current: Option<u64>,
) -> Option<ImplementVisual> {
        let ImplementKind::Wand { parts, resolved } = kind else {
            return None;
        };
        // Saved component ids outlive content packs. A removed mod part keeps
        // its manifest/stat identity, while rendering falls back by physical
        // role instead of making the entire held model disappear.
        let item = |name: &str, fallback: &str| {
            registry
                .item_id(name)
                .or_else(|| registry.item_id(fallback))
                .map(|item| item.0)
        };
        let usable = current
            .map(crate::implements::usable_charge)
            .unwrap_or(0);
        let charge_band = crate::implements::charge_band(
            usable.saturating_add(crate::implements::STRUCTURAL_SPARK_UNITS),
            resolved.capacity,
        );
        let focus_shape = match parts.focus.as_str() {
            "base:echo_slate" => 1,
            "base:choirstone" => 2,
            "base:wake_iron" => 3,
            "base:pilgrim_root_cutting" => 4,
            _ => {
                1 + (parts.focus.bytes().fold(0u32, |hash, byte| {
                    hash.wrapping_mul(16777619) ^ u32::from(byte)
                }) % 4) as u8
            }
        };
        Some(crate::implements::ImplementVisual {
            body: item(&parts.body, "base:seasoned_wand_body")?,
            reservoir: item(&parts.reservoir, "base:ritual_rod_socket")?,
            focus: item(&parts.focus, "base:echo_slate")?,
            binding: item(&parts.binding, "base:bronze_wand_binding")?,
            focus_shape,
            charge_band,
        })
}
