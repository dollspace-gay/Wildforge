//! Society scenarios.

use super::*;

#[test]
fn four_identity_magic_society_shares_one_unprivileged_host() {
    let host = TestHost::start_with_discovery("agent-magic-society");
    let mut agents = [
        Agent::connect_for_test(host.addr, "SURVEYOR").expect("surveyor joins"),
        Agent::connect_for_test(host.addr, "CULTIVATOR").expect("cultivator joins"),
        Agent::connect_for_test(host.addr, "BINDER").expect("binder joins"),
        Agent::connect_for_test(host.addr, "APOTHECARY").expect("apothecary joins"),
    ];
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while std::time::Instant::now() < deadline {
        for agent in &mut agents {
            agent.pump(0.02);
        }
        if host.with(|session, _| session.guests.len() == 4) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let identities = host.with(|session, _| {
        session
            .guests
            .values()
            .map(|guest| guest.player_id)
            .collect::<std::collections::HashSet<_>>()
    });
    assert_eq!(
        identities.len(),
        4,
        "four roles collapsed onto one identity"
    );
    for (agent, role) in agents.iter().zip([
        "survey records ready",
        "cultivation stock ready",
        "wand workshop ready",
        "waste station ready",
    ]) {
        agent.send(&crate::net::C2S::Chat(role.into()));
    }
    assert!(
        pump_until(
            &mut agents[0],
            std::time::Duration::from_secs(15),
            |agent| {
                [
                    "survey records ready",
                    "cultivation stock ready",
                    "wand workshop ready",
                    "waste station ready",
                ]
                .into_iter()
                .all(|role| agent.events.iter().any(|event| event.contains(role)))
            },
        ),
        "the four physical roles did not share the ordinary society channel: {:?}",
        agents[0].events
    );
    host.with(|session, _| {
        assert!(session.guests.values().all(|guest| {
            guest
                .principals
                .iter()
                .any(|principal| principal == &guest.principal)
        }));
    });
}

#[test]
fn the_agent_earns_and_reads_a_host_signed_magic_observation() {
    let host = TestHost::start_with_discovery("agent-discovery");
    let mut agent = Agent::connect_for_test(host.addr, "OBSERVER").expect("observer joins");
    agent.pump_for(0.5);
    let guest_id = host.with(|session, _| *session.guests.keys().next().unwrap());
    host.with(|session, sim| {
        let at = session.guests[&guest_id]
            .pos
            .block()
            .expect("guest stands on the planetary stage");
        for item_name in ["base:tuning_lens", "base:field_ledger"] {
            let item = sim.world.reg.item_id(item_name).unwrap();
            let mut stack = crate::inventory::ItemStack::new(&sim.world.reg, item, 1);
            if item_name == "base:tuning_lens" {
                sim.world
                    .bind_arcane_stack_at(at, &mut stack, "agent discovery integration fixture")
                    .unwrap();
            }
            sim.world
                .record_external_stack(stack, "agent discovery integration fixture")
                .unwrap();
            sim.world.queue_give(guest_id, stack);
        }
    });
    for _ in 0..100 {
        agent.pump(0.02);
        let has = |name: &str| {
            let item = agent.reg.item_id(name).unwrap();
            agent
                .inventory
                .slots
                .iter()
                .flatten()
                .any(|stack| stack.item == item)
        };
        if has("base:tuning_lens") && has("base:field_ledger") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let report = agent
        .observe_discovery(
            None,
            "base:field_ledger",
            None,
            Some("agent field note".into()),
        )
        .expect("ordinary agent protocol earns a settled observation");
    assert!(
        report.contains("base:local_current_field")
            && report.contains("| region |")
            && report.contains("agent field note"),
        "{report}"
    );
    let ledger = agent
        .read_knowledge("base:field_ledger")
        .expect("the agent reads only its physical ledger");
    assert!(ledger.contains("records 1/32") && ledger.contains("agent field note"));

    host.with(|session, sim| {
        let object_id = session.guests[&guest_id]
            .inventory
            .slots
            .iter()
            .flatten()
            .find(|stack| sim.world.reg.item(stack.item).name == "base:field_ledger")
            .unwrap()
            .arcane_id;
        let records = sim.world.discovery_summaries(object_id, true).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].observer_name, "OBSERVER");
    });
}
