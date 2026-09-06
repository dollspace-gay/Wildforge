//! Replication scenarios.

use super::*;

#[test]
fn guest_weather_matches_the_host_across_a_face_seam() {
    let atlas = Arc::new(climate(404, 8));
    let reg = base_reg();
    let mut host = World::new_with_atlas(
        404,
        tmp_dir("climate-host-seam"),
        reg.clone(),
        atlas.clone(),
    );
    host.force_local_weather("storm");
    let source = SurfacePos::new(Face::PosZ, 8_191, 4_096).unwrap();
    let crossed = crate::planet::step4(source, Direction4::East).pos;
    let center = atlas.atlas_pos(source);
    let mut positions = vec![center];
    positions.extend(center.neighbors8(atlas.side()));
    positions.sort();
    positions.dedup();
    let cells = positions
        .into_iter()
        .map(|pos| {
            let center = pos.center(atlas.side());
            let surface = SurfacePos::new(
                center.face,
                center.u.floor() as u16,
                center.v.floor() as u16,
            )
            .unwrap();
            (pos, host.weather_at_surface(surface))
        })
        .collect();
    let mut guest = ReplicaWorld::new(404, reg, 0.0);
    guest.observations_mut().set_weather(atlas.side(), cells);
    assert_eq!(
        guest.weather_at_surface(source).kind,
        host.weather_at_surface(source).kind
    );
    assert_eq!(
        guest.weather_at_surface(crossed).kind,
        host.weather_at_surface(crossed).kind
    );
    assert_eq!(guest.weather_at_surface(crossed).kind, LocalWeather::Storm);
}
