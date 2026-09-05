//! Plate sites, craton fields, Euler motion, and boundary pair classification.

use glam::DVec3;
use noise::{NoiseFn, Perlin};
use crate::planet_atlas::{AtlasGrid, GeometryCell, mix64};
use super::{PlateRecord, CratonRecord, DetailedBoundary, LLOYD_PASSES};
use super::geometry::{dvec, arr, unit_from_hash, fibonacci_sites, nearest_two};

pub(super) fn plate_sites(seed: u32, geometry: &AtlasGrid<GeometryCell>) -> Vec<PlateRecord> {
    let count = 16 + (mix64(u64::from(seed) ^ 0x504c_4154_4553) % 7) as usize;
    let mut sites = fibonacci_sites(count, seed, 0x504c_4154_4553, 0.055);
    for _ in 0..LLOYD_PASSES {
        let mut sums = vec![DVec3::ZERO; count];
        for cell in geometry.values() {
            let point = dvec(cell.unit_direction);
            let nearest = nearest_two(&sites, point).0;
            sums[nearest] += point * f64::from(cell.physical_area);
        }
        for (site, sum) in sites.iter_mut().zip(sums) {
            if sum.length_squared() > 0.0 {
                *site = sum.normalize();
            }
        }
    }
    sites
        .into_iter()
        .enumerate()
        .map(|(index, site)| {
            let pole = unit_from_hash(seed, 0x4555_4c45_5200 ^ index as u64);
            let speed_hash = mix64(u64::from(seed) ^ 0x5350_4545_4400 ^ index as u64);
            PlateRecord {
                id: index as u16,
                site_unit: arr(site),
                euler_pole: arr(pole),
                angular_speed: 0.25 + (speed_hash as f32 / u64::MAX as f32) * 0.95,
            }
        })
        .collect()
}

pub(super) fn plate_assignments(plates: &[PlateRecord], geometry: &AtlasGrid<GeometryCell>) -> Vec<u16> {
    let sites: Vec<_> = plates.iter().map(|plate| dvec(plate.site_unit)).collect();
    geometry
        .values()
        .iter()
        .map(|cell| nearest_two(&sites, dvec(cell.unit_direction)).0 as u16)
        .collect()
}

pub(super) fn craton_field(
    seed: u32,
    attempt: u8,
    geometry: &AtlasGrid<GeometryCell>,
) -> (Vec<u16>, Vec<u16>, Vec<u16>, Vec<CratonRecord>) {
    let count = 6 + (mix64(u64::from(seed) ^ u64::from(attempt) ^ 0x4352_4154_4f4e) % 4) as usize;
    let centers = fibonacci_sites(
        count,
        seed ^ u32::from(attempt).wrapping_mul(0x9e37_79b9),
        0x4352_4154_4f4e,
        0.045,
    );
    let noise = Perlin::new(seed ^ u32::from(attempt).wrapping_mul(0x85eb_ca6b) ^ 0xacc3_710a);
    let mut records = Vec::with_capacity(count);
    let mut nuclei_by_group = Vec::with_capacity(count);
    for (index, center) in centers.iter().copied().enumerate() {
        let mut nuclei = [[0.0; 3]; 3];
        for (nucleus_index, slot) in nuclei.iter_mut().enumerate() {
            let random = unit_from_hash(
                seed,
                0x4e55_434c_4555 ^ u64::from(attempt) ^ (index as u64) << 8 ^ nucleus_index as u64,
            );
            let tangent = (random - center * random.dot(center)).normalize_or_zero();
            let angle = if nucleus_index == 0 {
                0.0
            } else {
                0.08 + nucleus_index as f64 * 0.035
            };
            *slot = arr((center * angle.cos() + tangent * angle.sin()).normalize());
        }
        nuclei_by_group.push(nuclei);
        let radius =
            0.46 + (mix64(u64::from(seed) ^ index as u64 ^ 0x7261_6469_7573) % 45) as f32 / 1000.0;
        records.push(CratonRecord {
            id: index as u16 + 1,
            center_unit: arr(center),
            nuclei,
            radius_radians: radius,
            interior_age_myr: 2_700
                + (mix64(u64::from(seed) ^ index as u64 ^ 0x0061_6765) % 900) as u16,
        });
    }

    let mut fraction = Vec::with_capacity(geometry.len());
    let mut ids = Vec::with_capacity(geometry.len());
    let mut ages = Vec::with_capacity(geometry.len());
    for cell in geometry.values() {
        let point = dvec(cell.unit_direction);
        let n = noise.get([
            point.x * 3.7 + 11.0,
            point.y * 3.7 - 7.0,
            point.z * 3.7 + 3.0,
        ]);
        let mut best = (0.0f64, 0usize);
        for (group, record) in records.iter().enumerate() {
            let distance = nuclei_by_group[group]
                .iter()
                .map(|nucleus| dvec(*nucleus).dot(point).clamp(-1.0, 1.0).acos())
                .fold(f64::INFINITY, f64::min);
            let radius = f64::from(record.radius_radians) + n * 0.055;
            let value = ((radius - distance) / 0.14 + 0.5).clamp(0.0, 1.0);
            let value = value * value * (3.0 - 2.0 * value);
            if value > best.0 {
                best = (value, group);
            }
        }
        fraction.push((best.0 * 65_535.0).round() as u16);
        if best.0 > 0.04 {
            ids.push(records[best.1].id);
            let margin = 1.0 - best.0;
            ages.push(
                (f32::from(records[best.1].interior_age_myr) * (1.0 - margin as f32 * 0.72))
                    .max(180.0) as u16,
            );
        } else {
            ids.push(0);
            ages.push(0);
        }
    }
    (fraction, ids, ages, records)
}

pub(super) fn plate_velocity(plate: &PlateRecord, point: DVec3) -> DVec3 {
    dvec(plate.euler_pole).cross(point) * f64::from(plate.angular_speed)
}

pub(super) fn classify_pair(
    a_plate: u16,
    b_plate: u16,
    a_continental: bool,
    b_continental: bool,
    point: DVec3,
    plates: &[PlateRecord],
) -> (DetailedBoundary, f32, DVec3) {
    let b_site = dvec(plates[usize::from(b_plate)].site_unit);
    let normal = (b_site - point * b_site.dot(point)).normalize_or_zero();
    let relative = plate_velocity(&plates[usize::from(a_plate)], point)
        - plate_velocity(&plates[usize::from(b_plate)], point);
    let convergence = relative.dot(normal) as f32;
    let tangent = point.cross(normal).normalize_or_zero();
    let shear = relative.dot(tangent).abs() as f32;
    let detail = if convergence > 0.10 {
        match (a_continental, b_continental) {
            (true, true) => DetailedBoundary::ContinentalCollision,
            (false, false) => DetailedBoundary::OceanOceanSubduction,
            _ => DetailedBoundary::OceanContinentSubduction,
        }
    } else if convergence < -0.10 {
        if a_continental && b_continental {
            DetailedBoundary::ContinentalRift
        } else {
            DetailedBoundary::OceanRidge
        }
    } else if shear > 0.09 {
        DetailedBoundary::Transform
    } else {
        DetailedBoundary::PassiveWeak
    };
    (detail, convergence.abs().max(shear), tangent)
}

#[cfg(test)]
pub(crate) fn classify_pair_rotation_probe(
    a_plate: u16,
    b_plate: u16,
    a_continental: bool,
    b_continental: bool,
    point: DVec3,
    plates: &[PlateRecord],
) -> (DetailedBoundary, f32) {
    let (detail, strength, _) = classify_pair(
        a_plate,
        b_plate,
        a_continental,
        b_continental,
        point,
        plates,
    );
    (detail, strength)
}
