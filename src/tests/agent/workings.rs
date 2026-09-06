//! Workings scenarios.

use super::*;

#[test]
fn the_agent_runs_and_cancels_a_real_working_through_the_dedicated_host() {
    use crate::implements::FrameAction;
    use crate::workings::{WorkingApparatus, WorkingIntent, WorkingPhase, WorkingTargetIntent};

    let host = TestHost::start_with_discovery("agent-workings");
    let mut agent = Agent::connect_for_test(host.addr, "PRACTITIONER")
        .expect("practitioner joins as an ordinary guest");
    agent.pump_for(0.5);
    let guest_id = host.with(|session, _| *session.guests.keys().next().unwrap());
    let aim = crate::agent::cell_of(agent.player.pos)
        .expect("agent occupies the stage")
        .offset(2, 0, 0)
        .unwrap();

    let wand_id = host.with(|_, sim| {
        let (frame, _) = crate::tests::implements::install_frame_fixture(&mut sim.world);
        let (_, revision) = crate::tests::implements::assemble_fixture_wand(&mut sim.world, frame);
        let mut transfer = Inventory::new();
        sim.world
            .operate_binding_frame(
                frame,
                &mut transfer,
                0,
                FrameAction::ExchangeSelected,
                Some(revision),
                "agent workings integration fixture",
            )
            .unwrap();
        let wand = transfer.slots[0]
            .take()
            .expect("the assembled wand leaves the physical frame");
        let wand_id = wand.arcane_id;
        assert_ne!(wand_id, 0);
        sim.world.queue_give(guest_id, wand);

        let lens_item = sim.world.reg.item_id("base:tuning_lens").unwrap();
        let mut lens = ItemStack::new(&sim.world.reg, lens_item, 1);
        sim.world
            .bind_arcane_stack_at(aim, &mut lens, "agent workings integration fixture")
            .unwrap();
        sim.world
            .record_external_stack(lens, "agent workings integration fixture")
            .unwrap();
        sim.world.queue_give(guest_id, lens);
        wand_id
    });

    for _ in 0..120 {
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
        if has("base:bound_wand") && has("base:tuning_lens") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        agent.aim_working_at(aim).unwrap().contains("aiming"),
        "the ordinary agent aiming action remains available"
    );

    let started = agent
        .start_working(
            "base:trace",
            WorkingTargetIntent::None,
            Some("base:bound_wand"),
            false,
        )
        .expect("the dedicated host accepts the same Trace start as local play");
    assert!(started.contains("settle"), "{started}");
    let first_id = host.with(|_, sim| {
        let transaction = sim
            .world
            .workings_state
            .as_ref()
            .unwrap()
            .active
            .values()
            .find(|transaction| {
                matches!(
                    &transaction.apparatus,
                    WorkingApparatus::Wand { instance_id, .. } if *instance_id == wand_id
                )
            })
            .expect("the host owns the agent's active transaction");
        assert_eq!(transaction.definition.id, "base:trace");
        assert_eq!(transaction.actor_label, "PRACTITIONER");
        assert_eq!(transaction.phase, WorkingPhase::Charging);
        transaction.id
    });

    agent.pump_for(crate::workings::MIN_WAND_SETTLE_SECONDS + 0.2);
    let held = agent
        .continue_working(WorkingIntent::Hold)
        .expect("the agent advances the host-owned channel");
    assert!(held.contains("Trace"), "{held}");
    host.with(|_, sim| {
        assert_eq!(
            sim.world.workings_state.as_ref().unwrap().active[&first_id].phase,
            WorkingPhase::Active
        );
    });
    assert!(
        pump_until(&mut agent, std::time::Duration::from_secs(10), |agent| {
            agent.events.iter().any(|event| {
                event.contains("working cue")
                    && event.contains("base:trace")
                    && event.contains("Active")
            })
        }),
        "the reliable Active working cue never reached the agent; recent events: {:?}",
        agent.events
    );

    let released = agent
        .continue_working(WorkingIntent::Release)
        .expect("the agent releases through the ordinary completion path");
    assert!(released.contains("completes"), "{released}");
    host.with(|_, sim| {
        let state = sim.world.workings_state.as_ref().unwrap();
        assert!(!state.active.contains_key(&first_id));
        let event = state
            .history
            .iter()
            .find(|event| event.id == first_id)
            .unwrap();
        assert_eq!(event.working_id, "base:trace");
        assert_eq!(event.outcome, "completed");
    });

    // Prove cancellation is not merely exposed by the agent schema: begin a
    // second real reservation after recovery and settle it through the host.
    agent.pump_for(crate::workings::WAND_RECOVERY_SECONDS + 0.2);
    agent
        .start_working(
            "base:trace",
            WorkingTargetIntent::None,
            Some("base:bound_wand"),
            false,
        )
        .expect("the agent starts a second host-owned reservation");
    let second_id = host.with(|_, sim| {
        sim.world
            .workings_state
            .as_ref()
            .unwrap()
            .active
            .values()
            .find(|transaction| {
                matches!(
                    &transaction.apparatus,
                    WorkingApparatus::Wand { instance_id, .. } if *instance_id == wand_id
                )
            })
            .unwrap()
            .id
    });
    let cancelled = agent
        .continue_working(WorkingIntent::Cancel)
        .expect("the agent cancels through the ordinary settlement path");
    assert!(cancelled.contains("cancel"), "{cancelled}");
    host.with(|_, sim| {
        let state = sim.world.workings_state.as_ref().unwrap();
        assert!(!state.active.contains_key(&second_id));
        let event = state
            .history
            .iter()
            .find(|event| event.id == second_id)
            .unwrap();
        assert_eq!(event.outcome, "cancelled");
        assert!(state.history.iter().any(|event| {
            event.id == first_id && event.working_id == "base:trace" && event.outcome == "completed"
        }));
    });
}
