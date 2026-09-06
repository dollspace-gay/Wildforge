//! Cosmetic state, deterministic cosmetic variation, particles, and gait.

use crate::inventory::HOTBAR_SLOTS;
use crate::{config, lights, particles, world};
use glam::Vec3;

/// Cosmetic animation, particles, transient feedback, and light selection.
pub(super) struct PresentationState {
    /// Top of the view-distance slider on this machine, resolved once at
    /// startup from available memory. A setting that cannot be honoured is
    /// worse than one that is not offered.
    pub(super) max_view_dist: i32,
    /// Region-whisper bookkeeping: the cell we're in, and cells
    /// already whispered this session.
    pub(super) last_ire_cell: Option<world::RegionCell>,
    pub(super) whispered_cells: std::collections::HashSet<world::RegionCell>,
    /// Qualitative magical signatures already presented this session. The
    /// same ordinary condition does not toast on every atlas-cell crossing.
    pub(super) arcane_signs: std::collections::HashSet<String>,
    pub(super) swing: f32,
    pub(super) hand_bob: f32,
    pub(super) weather_vis: f32,
    pub(super) lightning: f32,
    pub(super) thunder_delay: f32,
    pub(super) atlas_season: usize,
    pub(super) juice: bool,
    pub(super) rng: u32,
    pub(super) pool: particles::Pool,
    pub(super) step_accum: f32,
    pub(super) mob_strides: std::collections::HashMap<u32, f32>,
    pub(super) remote_strides: std::collections::HashMap<u32, (crate::planet::EntityPos, f32)>,
    pub(super) ui_flies: Vec<(u16, (f32, f32), usize, f32)>,
    pub(super) slot_pulse: [f32; HOTBAR_SLOTS],
    pub(super) pickup_streak: (u32, f32),
    pub(super) screen_age: f32,
    /// Countdown to the next ambient speck (songbird, dragonfly).
    pub(super) ambient_timer: f32,
    pub(super) sel_bounce: f32,
    pub(super) press_dip: f32,
    pub(super) hitch: f32,
    pub(super) nudge: (Vec3, f32),
    pub(super) presence_timer: f32,
    pub(super) hunger_timer: f32,
    pub(super) demo_burst: Option<(Vec3, u16)>,
    pub(super) toasts: Vec<(String, f32)>,
    pub(super) lights: lights::Director,
    pub(super) player_gait: std::collections::HashMap<u32, (Vec3, f32)>,
    pub(super) demo_lights: Vec<lights::DynLight>,
    /// Last host-authored active cue by stable working id. Dedicated guests
    /// refresh this bounded presentation cache once per second; local play
    /// reads the authoritative state directly.
    pub(super) working_cues: std::collections::HashMap<u64, (crate::workings::WorkingCue, f32)>,
}

impl PresentationState {
    pub(super) fn new() -> Self {
        Self {
            max_view_dist: config::max_view_dist_for_memory(),
            last_ire_cell: None,
            whispered_cells: std::collections::HashSet::new(),
            arcane_signs: std::collections::HashSet::new(),
            swing: 0.0,
            hand_bob: 0.0,
            weather_vis: 0.0,
            lightning: 0.0,
            thunder_delay: -1.0,
            atlas_season: 1,
            juice: std::env::var("WILDFORGE_JUICE")
                .map(|v| v != "0")
                .unwrap_or(true),
            rng: 0x9e3779b9,
            pool: particles::Pool::default(),
            step_accum: 0.0,
            mob_strides: Default::default(),
            remote_strides: Default::default(),
            ui_flies: Vec::new(),
            slot_pulse: [0.0; HOTBAR_SLOTS],
            pickup_streak: (0, 0.0),
            screen_age: 1.0,
            ambient_timer: 3.0,
            sel_bounce: 1.0,
            press_dip: 0.0,
            hitch: 0.0,
            nudge: (Vec3::ZERO, 0.0),
            presence_timer: 0.0,
            hunger_timer: 0.0,
            demo_burst: None,
            toasts: Vec::new(),
            lights: lights::Director::new(),
            player_gait: Default::default(),
            demo_lights: Vec::new(),
            working_cues: Default::default(),
        }
    }

    pub(super) fn vary(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
        0.9 + ((self.rng >> 8) as f32 / (1 << 24) as f32) * 0.2
    }
}

impl PresentationState {
    /// Debris burst from a block's own texture (breaks, hits).
    pub(super) fn burst(&mut self, at: Vec3, tile: u16, n: usize, speed: f32) {
        if !self.juice {
            return;
        }
        let mut r = self.rng;
        self.pool.burst(at, tile, n, speed, &mut r);
        self.rng = r;
    }

    /// A soft ground puff (landings, grinding).
    pub(super) fn puff(&mut self, at: Vec3, tile: u16, n: usize) {
        if !self.juice {
            return;
        }
        let mut r = self.rng;
        self.pool.puff(at, tile, n, &mut r);
        self.rng = r;
    }

    /// Advance a remote player's walk phase from their motion.
    pub(super) fn gait_for(&mut self, id: u32, pos: Vec3, dt: f32) -> (f32, f32) {
        let e = self.player_gait.entry(id).or_insert((pos, 0.0));
        let hspeed = Vec3::new(pos.x - e.0.x, 0.0, pos.z - e.0.z).length() / dt.max(0.001);
        e.0 = pos;
        let amp = (hspeed / 3.5).clamp(0.0, 1.0);
        e.1 += hspeed * dt * 3.2;
        (e.1, amp)
    }
}
