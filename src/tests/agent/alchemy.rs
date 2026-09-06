//! Alchemy scenarios.

use super::*;

#[test]
fn the_agent_runs_an_ordinary_alchemy_job_through_the_host() {
    use crate::alchemy::ApparatusAction;

    let host = TestHost::start_with_discovery("agent-alchemy");
    let mut agent = Agent::connect_for_test(host.addr, "APOTHECARY")
        .expect("apothecary joins as an ordinary guest");
    agent.pump_for(0.5);
    let guest_id = host.with(|session, _| *session.guests.keys().next().unwrap());
    let mortar = crate::agent::cell_of(agent.player.pos)
        .expect("agent occupies the stage")
        .offset(2, 0, 0)
        .unwrap();
    host.with(|_, sim| {
        sim.world.set_block_at(
            mortar,
            sim.world.reg.block_id("base:alchemy_mortar").unwrap(),
        );
        let seeds = ItemStack::new(
            &sim.world.reg,
            sim.world.reg.item_id("base:wheat_seeds").unwrap(),
            4,
        );
        sim.world
            .record_external_stack(seeds, "agent alchemy integration fixture")
            .unwrap();
        sim.world.queue_give(guest_id, seeds);
    });
    let seed_item = agent.reg.item_id("base:wheat_seeds").unwrap();
    assert!(
        pump_until(&mut agent, std::time::Duration::from_secs(10), |agent| {
            agent
                .inventory
                .slots
                .iter()
                .flatten()
                .any(|stack| stack.item == seed_item && stack.count == 4)
        }),
        "the authoritative seed stack never reached the alchemy agent"
    );

    let inspected = agent
        .operate_alchemy(mortar, ApparatusAction::Inspect, None)
        .expect("the agent inspects through the ordinary station request");
    assert!(inspected.contains("apparatus is inspected"), "{inspected}");
    let started = agent
        .operate_alchemy(
            mortar,
            ApparatusAction::PressOil { seed_slot: 0 },
            Some("base:wheat_seeds"),
        )
        .expect("the host reserves the agent's measured oil process");
    assert!(started.contains("begun"), "{started}");
    host.with(|_, sim| {
        let due = sim.world.alchemy_state().unwrap().ordinary_jobs[&mortar].due_tick;
        sim.freeze_clock = true;
        sim.world.set_simulation_clock((due + 1) as f64 / 20.0);
    });
    let collected = agent
        .operate_alchemy(mortar, ApparatusAction::PressOil { seed_slot: 0 }, None)
        .unwrap_or_else(|error| {
            panic!(
                "the same host action did not collect the finished physical carrier: {error}; recent agent events: {:?}",
                agent.events
            )
        });
    assert!(collected.contains("measured finite carrier"), "{collected}");
    let oil_item = agent.reg.item_id("base:plant_oil").unwrap();
    assert!(
        pump_until(&mut agent, std::time::Duration::from_secs(10), |agent| {
            agent
                .inventory
                .slots
                .iter()
                .flatten()
                .filter(|stack| stack.item == oil_item)
                .map(|stack| stack.count)
                .sum::<u32>()
                == 4
        }),
        "the agent never received the host-authored oil yield"
    );
    host.with(|session, sim| {
        assert!(
            !sim.world
                .alchemy_state()
                .unwrap()
                .ordinary_jobs
                .contains_key(&mortar)
        );
        assert_eq!(
            session.guests[&guest_id]
                .inventory
                .slots
                .iter()
                .flatten()
                .filter(|stack| sim.world.reg.item(stack.item).name == "base:plant_oil")
                .map(|stack| stack.count)
                .sum::<u32>(),
            4
        );
    });
}
