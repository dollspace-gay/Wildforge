//! Implements scenarios.

use super::*;

#[test]
fn the_agent_assembles_a_real_wand_and_its_identity_survives_hosted_death() {
    use crate::implements::FrameAction;

    let host = TestHost::start_with_discovery("agent-implements");
    let mut agent = Agent::connect_for_test(host.addr, "BINDER").expect("binder joins");
    agent.pump_for(0.5);
    let guest_id = host.with(|session, _| *session.guests.keys().next().unwrap());
    let player_cell = crate::agent::cell_of(agent.player.pos).expect("agent occupies stage");
    let frame = player_cell.offset(3, 0, 0).unwrap();
    // Keep the west face of the frame visible from the actor; apparatus on
    // that ray would correctly make the host reject the interaction.
    let vessel = frame.offset(0, 0, -1).unwrap();
    host.with(|_, sim| {
        sim.world
            .set_block_at(frame, sim.world.reg.block_id("base:binding_frame").unwrap());
        sim.world.set_block_at(
            frame.offset(0, 0, 1).unwrap(),
            sim.world.reg.block_id("base:focus_mount").unwrap(),
        );
        sim.world.set_block_at(
            frame.offset(1, 0, 0).unwrap(),
            sim.world.reg.block_id("base:arcane_conductor").unwrap(),
        );
        sim.world.set_block_at(
            frame.offset(0, 0, 2).unwrap(),
            sim.world.reg.block_id("base:containment_post").unwrap(),
        );
        let vessel_stack = ItemStack::new(
            &sim.world.reg,
            sim.world.reg.item_id("base:charge_vessel").unwrap(),
            1,
        );
        assert!(sim.world.place_item_block_at(vessel, vessel_stack));
        assert!(sim.world.binding_frame_layout(frame).valid);
        for name in [
            "base:seasoned_wand_body",
            "base:ritual_rod_socket",
            "base:echo_slate",
            "base:bronze_wand_binding",
        ] {
            let stack = ItemStack::new(&sim.world.reg, sim.world.reg.item_id(name).unwrap(), 1);
            sim.world
                .record_external_stack(stack, "agent implements integration fixture")
                .unwrap();
            sim.world.queue_give(guest_id, stack);
        }
    });
    for _ in 0..120 {
        agent.pump(0.02);
        let have = [
            "base:seasoned_wand_body",
            "base:ritual_rod_socket",
            "base:echo_slate",
            "base:bronze_wand_binding",
        ]
        .into_iter()
        .all(|name| {
            let item = agent.reg.item_id(name).unwrap();
            agent
                .inventory
                .slots
                .iter()
                .flatten()
                .any(|stack| stack.item == item)
        });
        if have {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    agent.pump_for(0.3);
    assert!(
        agent
            .operate_binding_frame(frame, FrameAction::Calibrate, None)
            .unwrap()
            .contains("calibration")
    );
    for name in [
        "base:seasoned_wand_body",
        "base:ritual_rod_socket",
        "base:echo_slate",
        "base:bronze_wand_binding",
    ] {
        let report = agent
            .operate_binding_frame(frame, FrameAction::ExchangeSelected, Some(name))
            .unwrap();
        assert!(report.contains("Mounted"), "{report}");
    }
    let assembled = agent
        .operate_binding_frame(frame, FrameAction::Assemble, None)
        .unwrap();
    assert!(assembled.contains("bound wand"), "{assembled}");
    let retrieved = agent
        .operate_binding_frame(frame, FrameAction::ExchangeSelected, None)
        .unwrap();
    assert!(retrieved.contains("Retrieved"), "{retrieved}");
    let wand_item = agent.reg.item_id("base:bound_wand").unwrap();
    let wand = agent
        .inventory
        .slots
        .iter()
        .flatten()
        .find(|stack| stack.item == wand_item)
        .copied()
        .expect("agent received the authoritative wand");
    assert_ne!(wand.arcane_id, 0);

    host.with(|session, sim| {
        let death_pos = session.guests[&guest_id].pos;
        session.hurt_guest(sim, guest_id, 1_000.0, death_pos);
        assert!(
            sim.world
                .pending_drops()
                .iter()
                .any(|(_, stack)| stack.arcane_id == wand.arcane_id)
        );
        assert!(
            sim.world
                .implements_state
                .as_ref()
                .unwrap()
                .instance(wand.arcane_id)
                .is_some()
        );
        assert!(
            sim.world
                .arcane_ledger
                .as_ref()
                .unwrap()
                .item_current_total(wand.arcane_id)
                .is_some()
        );
    });
    assert!(
        pump_until(&mut agent, std::time::Duration::from_secs(15), |agent| {
            agent
                .inventory
                .slots
                .iter()
                .flatten()
                .any(|stack| stack.item == wand_item && stack.arcane_id == wand.arcane_id)
        }),
        "the ordinary hosted pickup never returned stable wand {} after death; recent agent events: {:?}",
        wand.arcane_id,
        agent.events
    );
}
