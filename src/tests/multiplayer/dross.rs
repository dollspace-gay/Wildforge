//! Dedicated simulation-event delivery through a real admitted QUIC guest.

use crate::agent::Agent;
use crate::tests::fixtures::TestHost;

#[test]
fn dedicated_runtime_delivers_regional_dross_forecasts_and_breaches() {
    let host = TestHost::start_with_discovery("dedicated-dross-events");
    let mut agent = Agent::connect_for_test(host.addr, "DROSS OBSERVER").expect("guest joins");
    agent.pump_for(0.5);
    let region = host.with(|session, sim| {
        let guest = session.guests.values().next().expect("guest admitted");
        assert!(guest.is_active(), "guest has acknowledged its terrain");
        sim.world
            .planet_atlas()
            .expect("discovery fixture has a planetary atlas")
            .atlas_pos(guest.pos.surface())
    });
    let cues = [
        crate::dross::DrossCue {
            region,
            kind: crate::dross::DrossCueKind::BreachForecast,
        },
        crate::dross::DrossCue {
            region,
            kind: crate::dross::DrossCueKind::Breach {
                activity: Some(crate::dross::ScarActivityHandler::Shear),
            },
        },
    ];
    host.with(|session, sim| {
        for cue in cues {
            crate::dedicated::dispatch_sim_event(session, sim, crate::server::SimEvent::Dross(cue));
        }
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        agent.pump(0.02);
        if agent
            .events
            .iter()
            .any(|event| event.contains("dross event") && event.contains("DROSS BREACH FORECAST"))
            && agent
                .events
                .iter()
                .any(|event| event.contains("dross event") && event.contains("field shear"))
        {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!(
        "dedicated forecasts and breaches did not reach the guest: {:?}",
        agent.events
    );
}
