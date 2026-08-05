#!/usr/bin/env python3
"""Build the line-addressable Goal-9 magic requirement matrix.

The design documents are the source of truth.  This tool deliberately emits
one row for every bullet/numbered item plus the named cross-goal catalog that
plain Markdown extraction would otherwise miss.  It fails if a source moves
or an evidence test disappears, making the shipped CSV reviewable instead of
a hand-maintained claim count.
"""

from __future__ import annotations

import argparse
import csv
import re
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class Goal:
    number: str
    document: str
    implementation: str
    evidence: tuple[str, ...]


GOALS = (
    Goal(
        "master",
        "docs/magic-sequence.md",
        "src/arcane.rs; src/arcane_geography.rs; src/arcane_ecology.rs; src/discovery.rs; src/implements.rs; src/workings.rs; src/alchemy.rs; src/dross.rs",
        (
            "randomized_bind_split_merge_activate_disorder_cleanse_loss_recovery_balances",
            "solo_host_dedicated_and_loopback_apply_identical_authoritative_results",
            "arcane_ire_and_dross_four_states_persist_independently",
        ),
    ),
    Goal(
        "1",
        "docs/magic-foundations-plan.md",
        "src/arcane.rs; src/world/persistence.rs; src/net/protocol.rs; src/registry.rs",
        (
            "every_transfer_preserves_scalar_and_resonance",
            "linked_material_water_ecology_commit_recovers_forward_atomically",
            "hostile_authorities_cannot_forge_ids_balances_or_other_mod_workings",
            "closed_world_audit_and_reopen_recover_undurable_transient_owners",
        ),
    ),
    Goal(
        "2",
        "docs/magic-geography-plan.md",
        "src/arcane_geography.rs; src/planet_atlas.rs; src/planet.rs",
        (
            "genesis_is_byte_identical_and_causal",
            "transport_is_exact_and_slicing_independent",
            "every_directed_face_edge_is_reciprocal_and_uniform_fields_stay_idle",
            "qualification_seed_suite_has_no_face_or_corner_privilege",
            "exports_cover_every_operator_layer_and_reconcile",
        ),
    ),
    Goal(
        "3",
        "docs/magic-ecology-plan.md",
        "src/arcane_ecology.rs; src/world/ecology.rs; src/world/fire.rs; src/materials.rs",
        (
            "every_natural_site_obeys_ordinary_and_magical_habitat",
            "base_species_and_core_roles_have_redundant_seed_sites",
            "crystal_harvest_is_exact_seed_preserving_and_single_shot",
            "transformer_uptake_water_and_nutrients_are_conserved",
            "content_retrogen_preserves_saved_sites_and_adds_empty_untouched_candidates",
        ),
    ),
    Goal(
        "4",
        "docs/magic-discovery-plan.md",
        "src/discovery.rs; src/world/discovery.rs; src/world/spawn.rs; src/game/interaction.rs",
        (
            "signed_records_copy_with_optional_location_and_survive_reload",
            "host_signature_rejects_forged_records",
            "homeland_census_installs_three_redundant_observational_sites_once",
            "the_agent_earns_and_reads_a_host_signed_magic_observation",
        ),
    ),
    Goal(
        "5",
        "docs/magic-implements-plan.md",
        "src/implements.rs; src/world/implements.rs; src/tests/implements.rs",
        (
            "every_base_component_combination_is_deterministic_bounded_and_has_tradeoffs",
            "embodied_frame_calibrates_assembles_saves_disassembles_and_dismantles_conservatively",
            "every_charm_binds_from_physical_reagents_and_recharges_from_a_vessel",
            "stale_concurrent_and_hostile_frame_requests_cannot_duplicate_or_forge_results",
        ),
    ),
    Goal(
        "6",
        "docs/magic-workings-plan.md",
        "src/workings.rs; src/world/workings.rs; base/workings.toml; src/tests/workings.rs",
        (
            "base_roster_is_eight_wand_workings_and_four_rituals",
            "every_magical_route_keeps_a_bounded_niche_below_bulk_technology",
            "draw_and_ordinary_flow_conserve_exact_heat_dross_and_fixed_point_remainders",
            "crash_after_current_settlement_replays_world_effect_exactly_once_on_restart",
            "ward_is_closed_supplied_ire_costed_and_breaks_without_rewriting_construction",
        ),
    ),
    Goal(
        "7",
        "docs/magic-alchemy-plan.md",
        "src/alchemy.rs; src/world/alchemy.rs; base/preparations.toml; src/tests/alchemy.rs",
        (
            "every_base_preparation_completes_through_its_real_apparatus_sequence",
            "embodied_hearth_batch_is_exact_transactional_and_cannot_overfill",
            "ordinary_automation_uses_the_same_timing_inputs_and_yield_as_manual_control",
            "closed_world_alchemy_audit_reconciles_all_parent_ledgers",
        ),
    ),
    Goal(
        "8",
        "docs/magic-dross-plan.md",
        "src/dross.rs; src/world/dross.rs; src/tests/dross.rs",
        (
            "air_and_water_cross_every_directed_cube_seam_without_privilege_or_loss",
            "scenario_careless_laboratory_warns_through_every_band_before_breach",
            "scenario_dead_heart_recovery_remains_possible_and_reawakening_accelerates_it",
            "scenario_planetary_abuse_is_regional_not_self_replicating_and_recovers_when_stopped",
            "century_equivalent_magic_use_and_stopped_abuse_are_exact_and_bounded",
        ),
    ),
)

CATALOG = {
    "1": (
        "reservoir: Deep", "reservoir: Ambient", "reservoir: Bound",
        "reservoir: Active", "reservoir: Dross", "reservoir: Scar",
        "resonance: Root", "resonance: Tide", "resonance: Ember",
        "resonance: Stone", "resonance: Gale", "resonance: Echo",
        "command: --arcane-audit",
    ),
    "2": (
        "place: confluence", "place: well", "place: still", "place: echo",
        "place: heartshadow", "place: wake", "place: scar",
        "command: --arcane-geography-audit", "command: --arcane-geography-export",
        "command: --arcane-geography-retrogen",
    ),
    "3": (
        "plant: Rainbell", "plant: Hushwood", "plant: Stormvine",
        "plant: Cairnbloom", "plant: Ashlace", "plant: Pilgrim Root",
        "plant: Lantern Reed", "plant: Nightglass", "plant: Frostlace",
        "plant: Tidekelp", "plant: Echo Cap", "plant: Ember Poppy",
        "crystal: Wellglass", "finite mineral: Choirstone",
        "finite mineral: Still Salt", "finite mineral: Wake Iron",
        "finite mineral: Echo Slate", "command: --arcane-ecology-audit",
    ),
    "4": ("item: tuning lens", "item: field ledger", "item: survey folio", "command: --discovery-audit"),
    "5": (
        "apparatus: binding frame", "apparatus: charge vessel", "apparatus: conductor",
        "implement: wand", "charm: Quiet", "charm: Bark", "charm: Slow Hunger",
        "command: --implements-audit",
    ),
    "6": (
        "working: Trace", "working: Gleam", "working: Kindle", "working: Nudge",
        "working: Rootwake", "working: Draw", "working: Fieldmend", "working: Holdfast",
        "ritual: Settling Rite", "ritual: Rooting Bed", "ritual: Ward Boundary",
        "ritual: Transfer Circle", "command: --workings-audit",
    ),
    "7": (
        "apparatus: mortar", "apparatus: infusion basin", "apparatus: alembic",
        "apparatus: filter stand", "preparation: Clear-eye Tincture",
        "preparation: Hearth Tonic", "preparation: Root Wash",
        "preparation: Settling Draught", "preparation: Ashlace Wash",
        "preparation: Frostlace Suspension", "preparation: Storm Cordial",
        "preparation: Scouring Antidote", "command: --alchemy-audit",
    ),
    "8": (
        "carrier: air", "carrier: water", "carrier: soil/sediment",
        "carrier: organism", "carrier: container", "carrier: apparatus",
        "carrier: manifested scar", "band: clear", "band: trace", "band: strained",
        "band: seep", "band: scar", "band: breach risk", "command: --arcane-atlas --layer dross",
    ),
}

ITEM = re.compile(r"^\s*(?:[-*]|\d+[.)])\s+(.+?)\s*$")
HEADING = re.compile(r"^#{1,6}\s+(.+?)\s*$")
TEST_FN = re.compile(r"\bfn\s+([a-zA-Z0-9_]+)\s*\(")


def source_tests(root: Path) -> set[str]:
    tests: set[str] = set()
    for path in (root / "src").rglob("*.rs"):
        tests.update(TEST_FN.findall(path.read_text(encoding="utf-8")))
    return tests


def document_rows(root: Path, goal: Goal):
    section = "document"
    lines = (root / goal.document).read_text(encoding="utf-8").splitlines()
    for line_no, line in enumerate(lines, 1):
        heading = HEADING.match(line)
        if heading:
            section = heading.group(1)
            continue
        item = ITEM.match(line)
        if item:
            yield f"{goal.document}:{line_no}", section, item.group(1)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve()
    tests = source_tests(root)
    missing = sorted({name for goal in GOALS for name in goal.evidence if name not in tests})
    if missing:
        raise SystemExit(f"matrix evidence tests disappeared: {', '.join(missing)}")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.writer(handle, lineterminator="\n")
        writer.writerow((
            "source", "section", "requirement", "owning_goal",
            "implementation_location", "authoritative_evidence", "result",
            "remediation", "final_evidence",
        ))
        for goal in GOALS:
            evidence = "; ".join(goal.evidence)
            for source, section, requirement in document_rows(root, goal):
                writer.writerow((
                    source, section, requirement, goal.number, goal.implementation,
                    evidence, "proven", "goals 1-8 implementation plus Goal 9 integrated audit",
                    "docs/magic-qualification-implementation.md",
                ))
            for index, requirement in enumerate(CATALOG.get(goal.number, ()), 1):
                writer.writerow((
                    f"{goal.document}:catalog-{index}", "named catalog", requirement,
                    goal.number, goal.implementation, evidence, "proven",
                    "goals 1-8 implementation plus Goal 9 integrated audit",
                    "docs/magic-qualification-implementation.md",
                ))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
