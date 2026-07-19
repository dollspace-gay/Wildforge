//! Client-side point-light direction: decides which emitters get real
//! shadow-casting lights each frame (docs/point-lights-integration-plan.md).
//! Pure presentation — the simulation reads the flood-fill, never this.

use std::collections::HashMap;

use glam::Vec3;

use crate::chunk::ChunkPos;
use crate::renderer::PointLight;

/// Total light slots (matches the renderer/shader MAX_PT_LIGHTS).
pub const MAX_LIGHTS: usize = 8;
/// Slots reserved for dynamic lights (held + glowing mobs).
pub const MAX_DYNAMIC: usize = 3;
/// How far from the camera a static emitter can be promoted.
pub const PROMOTE_RANGE: f32 = 48.0;
/// A challenger must beat an incumbent's score by this factor.
const HYSTERESIS: f32 = 1.25;

/// Stable identity for a light across frames. Static emitters key on
/// their cell; dynamic lights on what they are.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Key {
    Block(i32, i32, i32),
    Held,
    Mob(u32),
    /// Another player's carried light (by player id).
    RemoteHeld(u32),
    /// Dev demo hooks.
    Demo(u32),
}

/// A block that emits light, as collected during chunk meshing.
#[derive(Clone, Copy, Debug)]
pub struct Emitter {
    pub pos: (i32, i32, i32),
    /// Per-channel emission 0..15 (BlockDef.light_rgb).
    pub rgb: [u8; 3],
    /// Peak level 0..15 (BlockDef.light_emit).
    pub emit: u8,
}

/// A dynamic light the game hands the director each frame.
#[derive(Clone, Copy, Debug)]
pub struct DynLight {
    pub key: Key,
    pub pos: Vec3,
    pub color: Vec3,
    pub range: f32,
}

/// The derived render parameters for a promoted emitter.
fn static_light(e: &Emitter, flicker: f32) -> PointLight {
    let emit = e.emit.max(1) as f32;
    // Hue-normalized color (peak channel = 1) at an intensity that blares
    // at arm's length and dies by ~range.
    let color = Vec3::new(
        e.rgb[0] as f32 / emit,
        e.rgb[1] as f32 / emit,
        e.rgb[2] as f32 / emit,
    );
    let intensity = 1.8 * (emit / 14.0) * flicker;
    // Render-side flood suppression: the flood value this emitter reaches
    // a fragment with is ~ (rgb_ch - d)/15 along open paths. Cancel that
    // from the rendered torch term so the hard direct light reads.
    let suppress_scale = emit / (15.0 * intensity.max(0.01));
    PointLight {
        pos: Vec3::new(
            e.pos.0 as f32 + 0.5,
            e.pos.1 as f32 + 0.5,
            e.pos.2 as f32 + 0.5,
        ),
        range: emit + 2.0,
        color: color * intensity,
        key: 0,
        epoch: 0,
        shadows: true,
        suppress: (suppress_scale, emit),
    }
}

/// Slot assignment with hysteresis: incumbents keep their slot (and so
/// their cached cube map) unless a challenger clearly beats them.
/// Pure function — unit-tested directly.
pub fn promote(slots: &[Option<Key>], candidates: &[(Key, f32)], n: usize) -> Vec<Option<Key>> {
    let score: HashMap<Key, f32> = candidates.iter().copied().collect();
    let mut out: Vec<Option<Key>> = slots
        .iter()
        .map(|s| s.filter(|k| score.contains_key(k)))
        .collect();
    out.resize(n, None);
    out.truncate(n);

    let mut challengers: Vec<(Key, f32)> = candidates
        .iter()
        .filter(|(k, _)| !out.contains(&Some(*k)))
        .copied()
        .collect();
    challengers.sort_by(|a, b| b.1.total_cmp(&a.1));

    for (k, s) in challengers {
        if let Some(slot) = out.iter().position(|s| s.is_none()) {
            out[slot] = Some(k);
            continue;
        }
        // Weakest incumbent, if this challenger beats it decisively.
        let weakest = out
            .iter()
            .enumerate()
            .filter_map(|(i, o)| o.map(|k| (i, score.get(&k).copied().unwrap_or(0.0))))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, ws)) = weakest
            && s > ws * HYSTERESIS
        {
            out[i] = Some(k);
        }
    }
    out
}

/// Slow two-sine flame flicker in [1-amp, 1+amp], phased per key.
pub fn flicker(t: f32, phase: f32, amp: f32) -> f32 {
    let n = 0.6 * (t * 7.3 + phase).sin() + 0.4 * (t * 13.7 + phase * 2.0).sin();
    1.0 + amp * n
}

pub struct Director {
    /// Per-chunk emitter lists, refreshed whenever a chunk re-meshes.
    chunk_emitters: HashMap<ChunkPos, Vec<Emitter>>,
    /// Current slot assignment (index = renderer slot = cube layer group).
    slots: Vec<Option<Key>>,
    /// Cache-busting revision per active light key.
    epochs: HashMap<Key, u64>,
    /// Last position a dynamic light was rendered at (movement bumps epoch).
    last_pos: HashMap<Key, Vec3>,
    /// Advances with real time; drives the flame flicker.
    pub clock: f32,
}

impl Director {
    pub fn new() -> Director {
        Director {
            chunk_emitters: HashMap::new(),
            slots: vec![None; MAX_LIGHTS],
            epochs: HashMap::new(),
            last_pos: HashMap::new(),
            clock: 0.0,
        }
    }

    /// A chunk finished (re)meshing: replace its emitter list and
    /// invalidate the cube maps of every active light that can see it.
    pub fn chunk_meshed(&mut self, pos: ChunkPos, emitters: Vec<Emitter>) {
        self.chunk_emitters.insert(pos, emitters);
        self.invalidate_near_chunk(pos);
    }

    pub fn chunk_dropped(&mut self, pos: ChunkPos) {
        self.chunk_emitters.remove(&pos);
    }

    fn invalidate_near_chunk(&mut self, cpos: ChunkPos) {
        let min = Vec3::new(cpos.x as f32 * 16.0, 0.0, cpos.z as f32 * 16.0);
        let max = min + Vec3::new(16.0, 256.0, 16.0);
        for key in self.slots.iter().flatten() {
            let Some(p) = self.light_pos(*key) else {
                continue;
            };
            let (p, range) = p;
            if p.distance(p.clamp(min, max)) <= range {
                *self.epochs.entry(*key).or_insert(0) += 1;
            }
        }
    }

    /// Position + range of an active light, for invalidation tests.
    fn light_pos(&self, key: Key) -> Option<(Vec3, f32)> {
        match key {
            Key::Block(x, y, z) => {
                let e = self.emitter_at(x, y, z)?;
                Some((
                    Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5),
                    e.emit as f32 + 2.0,
                ))
            }
            // Dynamic lights re-render on movement anyway; use their last
            // known position for chunk-edit invalidation.
            _ => self.last_pos.get(&key).map(|p| (*p, 24.0)),
        }
    }

    fn emitter_at(&self, x: i32, y: i32, z: i32) -> Option<Emitter> {
        let cpos = ChunkPos::of_world(x, z);
        self.chunk_emitters
            .get(&cpos)?
            .iter()
            .find(|e| e.pos == (x, y, z))
            .copied()
    }

    /// One frame of direction: promote static emitters near the camera,
    /// append the dynamic lights, and return the renderer's light list.
    pub fn frame(
        &mut self,
        cam: Vec3,
        dynamic: &[DynLight],
        dt: f32,
        shadows: bool,
    ) -> Vec<PointLight> {
        self.clock += dt;

        // Static candidates: every emitter within promotion range.
        let ccx = (cam.x / 16.0).floor() as i32;
        let ccz = (cam.z / 16.0).floor() as i32;
        let r = (PROMOTE_RANGE / 16.0).ceil() as i32;
        let mut candidates: Vec<(Key, f32)> = Vec::new();
        for cx in (ccx - r)..=(ccx + r) {
            for cz in (ccz - r)..=(ccz + r) {
                let Some(list) = self.chunk_emitters.get(&ChunkPos { x: cx, z: cz }) else {
                    continue;
                };
                for e in list {
                    let p = Vec3::new(
                        e.pos.0 as f32 + 0.5,
                        e.pos.1 as f32 + 0.5,
                        e.pos.2 as f32 + 0.5,
                    );
                    let d = p.distance(cam);
                    if d < PROMOTE_RANGE {
                        let key = Key::Block(e.pos.0, e.pos.1, e.pos.2);
                        candidates.push((key, e.emit as f32 / (1.0 + d)));
                    }
                }
            }
        }
        let n_static = MAX_LIGHTS - dynamic.len().min(MAX_DYNAMIC);
        self.slots = promote(&self.slots, &candidates, n_static);

        // Drop bookkeeping for lights that fell out of every slot.
        let mut active: Vec<Key> = self.slots.iter().flatten().copied().collect();
        active.extend(dynamic.iter().take(MAX_DYNAMIC).map(|d| d.key));
        self.epochs.retain(|k, _| active.contains(k));
        self.last_pos.retain(|k, _| active.contains(k));

        let mut out = Vec::with_capacity(MAX_LIGHTS);
        for slot in &self.slots {
            let Some(key) = slot else { continue };
            let Key::Block(x, y, z) = key else { continue };
            let Some(e) = self.emitter_at(*x, *y, *z) else {
                continue;
            };
            // Flames breathe; cool lights hold steady.
            let f = if e.rgb[0] > e.rgb[2] {
                let phase = (x.wrapping_mul(31) ^ y.wrapping_mul(17) ^ z) as f32;
                flicker(self.clock, phase, 0.08)
            } else {
                1.0
            };
            let mut l = static_light(&e, f);
            l.key = key_bits(*key);
            l.epoch = self.epochs.get(key).copied().unwrap_or(0);
            l.shadows = shadows;
            out.push(l);
        }
        for d in dynamic.iter().take(MAX_DYNAMIC) {
            // Movement invalidates the cube; standing still keeps it.
            let moved = self
                .last_pos
                .get(&d.key)
                .is_none_or(|p| p.distance(d.pos) > 0.15);
            if moved {
                *self.epochs.entry(d.key).or_insert(0) += 1;
                self.last_pos.insert(d.key, d.pos);
            }
            let rendered = self.last_pos.get(&d.key).copied().unwrap_or(d.pos);
            let f = if d.color.x > d.color.z {
                flicker(self.clock, key_bits(d.key) as f32 % 64.0, 0.08)
            } else {
                1.0
            };
            out.push(PointLight {
                pos: rendered,
                range: d.range,
                color: d.color * f,
                key: key_bits(d.key),
                epoch: self.epochs.get(&d.key).copied().unwrap_or(0),
                shadows,
                suppress: (0.0, 1.0), // dynamic lights aren't in the flood-fill
            });
        }
        out
    }
}

/// Pack a Key into the u64 the renderer caches on.
fn key_bits(k: Key) -> u64 {
    match k {
        Key::Block(x, y, z) => {
            ((x as u32 as u64) << 40) ^ ((y as u32 as u64) << 20) ^ (z as u32 as u64)
        }
        Key::Held => 1 << 62,
        Key::Mob(id) => (2 << 62) | id as u64,
        Key::RemoteHeld(id) => (3 << 62) | ((id as u64) << 20),
        Key::Demo(id) => (3 << 62) | id as u64,
    }
}
