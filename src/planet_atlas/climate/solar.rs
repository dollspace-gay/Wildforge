//! Shared astronomical geometry with the established manifest axes.

use glam::DVec3;
use super::{AXIAL_TILT_DEGREES, ROTATION_AXIS, PRIME_MERIDIAN, YEAR_DAYS};

#[inline]
pub fn rotation_axis() -> DVec3 {
    DVec3::from_array(ROTATION_AXIS)
}

#[inline]
pub fn prime_meridian() -> DVec3 {
    DVec3::from_array(PRIME_MERIDIAN)
}

/// Solar declination in radians. Day zero is the northern vernal equinox.
pub fn solar_declination(day: f64) -> f64 {
    AXIAL_TILT_DEGREES.to_radians() * (std::f64::consts::TAU * day / f64::from(YEAR_DAYS)).sin()
}

/// Planet-space direction from the planet toward the sun.
pub fn solar_direction(day: f64, time_of_day: f64) -> DVec3 {
    let axis = rotation_axis();
    let prime = prime_meridian();
    let east = axis.cross(prime).normalize();
    let declination = solar_declination(day);
    let hour = std::f64::consts::TAU * (time_of_day - 0.25);
    (axis * declination.sin() + (prime * hour.cos() + east * hour.sin()) * declination.cos())
        .normalize()
}

/// Astronomical daylight duration, including polar day and night.
pub fn day_length_hours(latitude_radians: f64, day: f64) -> f64 {
    let declination = solar_declination(day);
    let cos_hour = -latitude_radians.tan() * declination.tan();
    if cos_hour <= -1.0 {
        24.0
    } else if cos_hour >= 1.0 {
        0.0
    } else {
        24.0 * cos_hour.acos() / std::f64::consts::PI
    }
}

/// Daily-mean top-of-atmosphere solar factor, normalized to 0..=1.
pub fn daily_mean_insolation(latitude_radians: f64, day: f64) -> f64 {
    let declination = solar_declination(day);
    let sin_product = latitude_radians.sin() * declination.sin();
    let cos_product = latitude_radians.cos() * declination.cos();
    let cos_hour = if cos_product.abs() < 1.0e-12 {
        if sin_product > 0.0 { -2.0 } else { 2.0 }
    } else {
        -sin_product / cos_product
    };
    let hour = if cos_hour <= -1.0 {
        std::f64::consts::PI
    } else if cos_hour >= 1.0 {
        0.0
    } else {
        cos_hour.acos()
    };
    ((hour * sin_product + cos_product * hour.sin()) / std::f64::consts::PI).clamp(0.0, 1.0)
}

/// Local astronomical season: spring, summer, autumn, winter.
pub fn local_season(day: u32, latitude_radians: f64) -> usize {
    let northern = ((day / 36) % 4) as usize;
    if latitude_radians < -1.0e-6 {
        (northern + 2) % 4
    } else {
        northern
    }
}

/// Signed latitude and longitude about the manifest axes.
pub fn latitude_longitude(unit: DVec3) -> (f64, f64) {
    let axis = rotation_axis();
    let prime = prime_meridian();
    let east = axis.cross(prime).normalize();
    let latitude = unit.dot(axis).clamp(-1.0, 1.0).asin();
    let planar = (unit - axis * unit.dot(axis)).normalize_or_zero();
    let longitude = planar.dot(east).atan2(planar.dot(prime));
    (latitude, longitude)
}

pub(super) fn geographic_basis(unit: DVec3) -> (DVec3, DVec3) {
    let axis = rotation_axis();
    let mut east = axis.cross(unit);
    if east.length_squared() < 1.0e-12 {
        east = prime_meridian().cross(unit);
    }
    east = east.normalize();
    let north = unit.cross(east).normalize();
    (east, north)
}
