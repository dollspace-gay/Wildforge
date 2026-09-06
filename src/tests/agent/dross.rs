//! Dross scenarios.

use super::*;

#[test]
fn the_agent_perceives_and_excavates_dross_without_private_provenance() {
    let host = TestHost::start_with_discovery("agent-dross-parity");
    let mut agent = Agent::connect_for_test(host.addr, "REMEDIATOR").expect("agent joins");
    agent.pump_for(0.5);
    let guest_id = host.with(|session, _| *session.guests.keys().next().unwrap());
    let (scar_pos, scar_id, scar_region) = host.with(|session, sim| {
        let actor_pos = session.guests[&guest_id]
            .pos
            .block()
            .expect("agent stands on the test stage");
        let scar_pos = actor_pos.offset(2, 0, 0).unwrap();
        let atlas = sim.world.planet_atlas().unwrap().clone();
        let region = atlas.atlas_pos(scar_pos.surface());
        let ledger = sim.world.arcane_ledger.as_mut().unwrap();
        let scar_id = ledger.allocate_scar_id().unwrap();
        let mut current = ledger
            .account(&crate::arcane::ArcaneOwner::Deep)
            .unwrap()
            .current
            .clone();
        let current = current
            .take_units(
                32,
                crate::arcane::BASE_RESONANCES
                    .into_iter()
                    .map(str::to_string),
            )
            .unwrap();
        let transaction_id = ledger.system_transaction_id().unwrap();
        ledger
            .commit(crate::arcane::ArcaneTransaction::transfer(
                transaction_id,
                crate::arcane::ArcaneOwner::Deep,
                ledger.version_of(&crate::arcane::ArcaneOwner::Deep),
                crate::arcane::ArcaneOwner::Scar(scar_id),
                ledger.version_of(&crate::arcane::ArcaneOwner::Scar(scar_id)),
                current,
                crate::arcane::ArcaneAuthority::System,
                "agent dross parity fixture",
            ))
            .unwrap();
        let step = sim
            .world
            .arcane_geography
            .as_ref()
            .unwrap()
            .dynamic
            .dross_state
            .completed_steps;
        let kind = crate::dross::ScarKind::BrokenSymmetry;
        let content_id = kind.block_id().to_string();
        let geography = sim.world.arcane_geography.as_mut().unwrap();
        geography.dynamic.dross_state.cells[region.index(atlas.side())].band =
            crate::dross::DrossBand::Scar;
        geography.dynamic.dross_state.scars.insert(
            scar_id,
            crate::dross::ScarSite {
                id: scar_id,
                region,
                kind,
                content_id,
                site_slot: 0,
                created_step: step,
                last_changed_step: step,
                breach_count: 0,
                materialized_at: Some(scar_pos),
                resolved_step: None,
                actor_hint: Some([0xcc; 16]),
                installation_hint: Some(707),
                provenance: crate::dross::DrossProvenance {
                    entries: vec![crate::dross::DrossContribution {
                        actor: Some([0xcc; 16]),
                        installation_id: Some(707),
                        source_class: "private fixture laboratory".into(),
                        units: 32,
                        first_step: step,
                        last_step: step,
                        confidence_permille: 1_000,
                    }],
                    unknown_units: 0,
                },
            },
        );
        geography
            .dynamic
            .dross_state
            .materialized
            .insert(scar_pos, scar_id);
        geography.mark_scar(&atlas, region);
        let block = sim.world.reg.block_id(kind.block_id()).unwrap();
        sim.world.set_block_at(scar_pos, block);
        (scar_pos, scar_id, region)
    });

    assert!(
        pump_until(&mut agent, std::time::Duration::from_secs(15), |agent| {
            agent.world.get_block_at(scar_pos) != crate::registry::AIR
                && agent.events.iter().any(|event| {
                    event.contains("environmental dross band 4") && event.contains("pattern 4")
                })
        }),
        "the authoritative Scar cue/block never reached the agent; events: {:?}",
        agent.events
    );
    assert!(agent.events.iter().all(|event| {
        !event.contains("private fixture laboratory")
            && !event.contains("707")
            && !event.contains("cc, cc")
    }));
    host.with(|session, sim| {
        session.broadcast_dross_cue(
            &sim.world,
            crate::dross::DrossCue {
                region: scar_region,
                kind: crate::dross::DrossCueKind::Breach {
                    activity: Some(crate::dross::ScarActivityHandler::Shear),
                },
            },
        );
    });
    assert!(
        pump_until(&mut agent, std::time::Duration::from_secs(15), |agent| {
            agent.events.iter().any(|event| {
                event.contains("dross event")
                    && event.contains("field shear")
                    && event.contains("existing burden")
            })
        }),
        "the public breach activity never reached the agent; events: {:?}",
        agent.events
    );
    assert!(agent.events.iter().all(|event| {
        !event.contains("private fixture laboratory")
            && !event.contains("707")
            && !event.contains("cc, cc")
    }));
    agent
        .break_block_at(scar_pos)
        .expect("ordinary agent Break excavates the host-owned scar");
    let fragment = agent.reg.item_id("base:scar_fragment").unwrap();
    assert!(
        pump_until(&mut agent, std::time::Duration::from_secs(15), |agent| {
            agent
                .inventory
                .slots
                .iter()
                .flatten()
                .any(|stack| stack.item == fragment && stack.arcane_id != 0)
        }),
        "the exact scar fragment never reached the agent"
    );
    host.with(|_, sim| {
        let state = &sim
            .world
            .arcane_geography
            .as_ref()
            .unwrap()
            .dynamic
            .dross_state;
        assert!(state.scars[&scar_id].resolved_step.is_some());
        assert!(!state.materialized.contains_key(&scar_pos));
        assert!(
            sim.world
                .arcane_ledger
                .as_ref()
                .unwrap()
                .account(&crate::arcane::ArcaneOwner::Scar(scar_id))
                .is_none_or(|account| account.current.is_empty())
        );
    });
}
