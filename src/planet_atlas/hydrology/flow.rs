//! Ordered drainage traversal, discharge accumulation, and channel order.

use crate::planet::geodesic_distance;
use crate::planet_atlas::{AtlasError, AtlasGrid, AtlasPos, CLIMATE_SEASONS, GeometryCell};
use std::collections::BinaryHeap;

pub(super) fn topological_order(receiver: &[u32]) -> Result<Vec<usize>, AtlasError> {
    let mut upstream = vec![0u32; receiver.len()];
    for &next in receiver {
        if next != u32::MAX {
            upstream[next as usize] += 1;
        }
    }
    let mut ready = BinaryHeap::new();
    for (index, count) in upstream.iter().enumerate() {
        if *count == 0 {
            ready.push(std::cmp::Reverse(index));
        }
    }
    let mut order = Vec::with_capacity(receiver.len());
    while let Some(std::cmp::Reverse(index)) = ready.pop() {
        order.push(index);
        let next = receiver[index];
        if next == u32::MAX {
            continue;
        }
        let slot = &mut upstream[next as usize];
        *slot -= 1;
        if *slot == 0 {
            ready.push(std::cmp::Reverse(next as usize));
        }
    }
    if order.len() != receiver.len() {
        return Err(AtlasError::Corrupt(
            "priority drainage contains an undeclared directed cycle".into(),
        ));
    }
    Ok(order)
}

pub(super) struct FlowAccumulation {
    pub(super) order: Vec<usize>,
    pub(super) annual: Vec<f64>,
    pub(super) seasonal: Vec<[f64; CLIMATE_SEASONS]>,
    pub(super) area: Vec<f64>,
}

pub(super) fn accumulate_flow(
    receiver: &[u32],
    geometry: &AtlasGrid<GeometryCell>,
    runoff: &[f32],
    seasonal_runoff: &[[f64; CLIMATE_SEASONS]],
) -> Result<FlowAccumulation, AtlasError> {
    let order = topological_order(receiver)?;
    let mut annual = vec![0.0f64; receiver.len()];
    let mut seasonal = vec![[0.0f64; CLIMATE_SEASONS]; receiver.len()];
    let mut area = vec![0.0f64; receiver.len()];
    for index in 0..receiver.len() {
        let cell_area = f64::from(geometry.values()[index].physical_area);
        area[index] = cell_area;
        annual[index] = f64::from(runoff[index]) * cell_area / 1000.0;
        for (accumulated, local) in seasonal[index].iter_mut().zip(seasonal_runoff[index]) {
            *accumulated = local * cell_area / 1000.0;
        }
    }
    for &index in &order {
        let next = receiver[index];
        if next == u32::MAX {
            continue;
        }
        let next = next as usize;
        annual[next] += annual[index];
        area[next] += area[index];
        let upstream = seasonal[index];
        for (downstream, upstream) in seasonal[next].iter_mut().zip(upstream) {
            *downstream += upstream;
        }
    }
    Ok(FlowAccumulation {
        order,
        annual,
        seasonal,
        area,
    })
}

pub(super) fn edge_distance(side: u16, a: usize, b: usize) -> f64 {
    geodesic_distance(
        AtlasPos::from_index(a, side)
            .expect("atlas index")
            .center(side),
        AtlasPos::from_index(b, side)
            .expect("atlas index")
            .center(side),
    )
    .max(1.0)
}

pub(super) fn receiver_path_reaches(
    start: u32,
    receiver: &[u32],
    targets: &std::collections::BTreeSet<usize>,
) -> bool {
    let mut next = start;
    for _ in 0..receiver.len() {
        if next == u32::MAX {
            return false;
        }
        let index = next as usize;
        if targets.contains(&index) {
            return true;
        }
        next = receiver[index];
    }
    // An existing cycle is never a safe outlet. The normal priority-flood
    // graph is acyclic; this conservative answer also keeps lake solving
    // from hiding corruption if that invariant is ever broken earlier.
    true
}

pub(super) fn stream_orders(receiver: &[u32], channel: &[bool]) -> Result<Vec<u8>, AtlasError> {
    let order = topological_order(receiver)?;
    let mut order_value = vec![0u8; receiver.len()];
    let mut largest_upstream = vec![0u8; receiver.len()];
    let mut largest_count = vec![0u8; receiver.len()];
    for &index in &order {
        if channel[index] {
            let inherited = largest_upstream[index];
            order_value[index] = if inherited == 0 {
                1
            } else if largest_count[index] >= 2 {
                inherited.saturating_add(1)
            } else {
                inherited
            };
        }
        let next = receiver[index];
        if next == u32::MAX || !channel[index] {
            continue;
        }
        let next = next as usize;
        let value = order_value[index];
        if value > largest_upstream[next] {
            largest_upstream[next] = value;
            largest_count[next] = 1;
        } else if value == largest_upstream[next] {
            largest_count[next] = largest_count[next].saturating_add(1);
        }
    }
    Ok(order_value)
}
