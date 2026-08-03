//! Client-side point-light direction: decides which emitters get real
//! shadow-casting lights each frame (docs/point-lights-integration-plan.md).
//! Pure presentation — the simulation reads the flood-fill, never this.

use std::collections::HashMap;

use glam::Vec3;

use crate::chunk::ChunkPos;
use crate::planet::{BlockPos, block_to_render};
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
    Block(BlockPos),
    Held,
    Mob(u32),
    /// Another player's carried light (by player id).
    RemoteHeld(u32),
    /// A host-authored temporary Gleam working.
    Working(u64),
    /// Dev demo hooks.
    Demo(u32),
}

/// A block that emits light, as collected during chunk meshing.
#[derive(Clone, Copy, Debug)]
pub struct Emitter {
    pub pos: BlockPos,
    /// Per-channel emission 0..15 (BlockDef.light_rgb).
    pub rgb: [u8; 3],
    /// Peak level 0..15 (BlockDef.light_emit).
    pub emit: u8,
}

fn emitter_render_pos(pos: BlockPos) -> Vec3 {
    block_to_render(pos.surface().center(), f64::from(pos.y()) + 0.5).as_vec3()
}

/// A dynamic light the game hands the director each frame.
#[derive(Clone, Copy, Debug)]
pub struct DynLight {
    pub key: Key,
    pub pos: Vec3,
    pub color: Vec3,
    pub range: f32,
}

/// Soft-shadow source size (world units) for real lights. A torch flame is
/// small, so a tight radius gives a soft-but-crisp penumbra; 0 is a hard
/// point. Dev override: WILDFORGE_LIGHT_RADIUS.
fn soft_radius() -> f32 {
    use std::sync::OnceLock;
    static R: OnceLock<f32> = OnceLock::new();
    *R.get_or_init(|| {
        std::env::var("WILDFORGE_LIGHT_RADIUS")
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0.15)
    })
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
        pos: emitter_render_pos(e.pos),
        range: emit + 2.0,
        color: color * intensity,
        key: 0,
        epoch: 0,
        shadows: true,
        suppress: (suppress_scale, emit),
        radius: soft_radius(),
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
        let center = block_to_render(
            crate::planet::SurfacePoint::new(
                cpos.face(),
                f64::from(cpos.u()) * 16.0 + 8.0,
                f64::from(cpos.v()) * 16.0 + 8.0,
            )
            .expect("chunk center is on its face"),
            128.0,
        )
        .as_vec3();
        // Statics hold a slot; dynamics (the held torch, glowing mobs)
        // live in last_pos. Both kinds of cube go stale when the world
        // near them remeshes — a held light that skipped this showed
        // shadows of walls that were no longer there until its owner
        // walked far enough to move the anchor.
        let keys: Vec<Key> = self
            .slots
            .iter()
            .flatten()
            .copied()
            .chain(self.last_pos.keys().copied())
            .collect();
        for key in keys {
            let Some((p, range)) = self.light_pos(key) else {
                continue;
            };
            if p.distance(center) <= range + 130.0 {
                *self.epochs.entry(key).or_insert(0) += 1;
            }
        }
    }

    /// Position + range of an active light, for invalidation tests.
    fn light_pos(&self, key: Key) -> Option<(Vec3, f32)> {
        match key {
            Key::Block(pos) => {
                let e = self.emitter_at(pos)?;
                Some((emitter_render_pos(pos), e.emit as f32 + 2.0))
            }
            // Dynamic lights re-render on movement anyway; use their last
            // known position for chunk-edit invalidation.
            _ => self.last_pos.get(&key).map(|p| (*p, 24.0)),
        }
    }

    fn emitter_at(&self, pos: BlockPos) -> Option<Emitter> {
        let cpos = pos.chunk();
        self.chunk_emitters
            .get(&cpos)?
            .iter()
            .find(|e| e.pos == pos)
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
        let mut candidates: Vec<(Key, f32)> = Vec::new();
        for list in self.chunk_emitters.values() {
            for e in list {
                let p = emitter_render_pos(e.pos);
                let d = p.distance(cam);
                if d < PROMOTE_RANGE {
                    let key = Key::Block(e.pos);
                    candidates.push((key, e.emit as f32 / (1.0 + d)));
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
            let Key::Block(pos) = key else { continue };
            let Some(e) = self.emitter_at(*pos) else {
                continue;
            };
            // Flames breathe; cool lights hold steady.
            let f = if e.rgb[0] > e.rgb[2] {
                let phase = (u32::from(pos.u()).wrapping_mul(31)
                    ^ u32::from(pos.y()).wrapping_mul(17)
                    ^ u32::from(pos.v())
                    ^ (pos.face() as u32).wrapping_mul(101)) as f32;
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
                radius: soft_radius(),
            });
        }
        out
    }
}

/// Pack a Key into the u64 the renderer caches on.
fn key_bits(k: Key) -> u64 {
    match k {
        Key::Block(pos) => {
            ((pos.face() as u64) << 48)
                | (u64::from(pos.u()) << 32)
                | (u64::from(pos.y()) << 16)
                | u64::from(pos.v())
        }
        Key::Held => 1 << 62,
        Key::Mob(id) => (2 << 62) | id as u64,
        Key::RemoteHeld(id) => (3 << 62) | ((id as u64) << 20),
        Key::Working(id) => (3 << 62) | (1 << 61) | (id & ((1 << 61) - 1)),
        Key::Demo(id) => (3 << 62) | id as u64,
    }
}
