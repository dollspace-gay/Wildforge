//! Procedurally synthesized sound effects — no audio files.
//! Degrades to silence if no output device is available.

use rodio::buffer::SamplesBuffer;
use rodio::{OutputStream, OutputStreamHandle, Source};

const RATE: u32 = 44_100;

/// Break-sound material family, derived from the block's tool class.
#[derive(Clone, Copy, Debug)]
pub enum BreakMat {
    Stone,
    Wood,
    Soft,
    Leafy,
}

#[derive(Clone, Copy, Debug)]
pub enum Sfx {
    Break(BreakMat),
    Place,
    Pickup,
    Click,
    Hurt,
    Craft,
    Splash,
    /// Animal hit/death thumps, pitched per species (1.0 = deer-sized).
    MobHurt(f32),
    MobDeath(f32),
    /// Warden bolt cast/whoosh.
    Bolt(f32),
}

pub struct Audio {
    _stream: OutputStream,
    handle: OutputStreamHandle,
    pub volume: f32,
}

impl Audio {
    pub fn new(volume: f32) -> Option<Audio> {
        match OutputStream::try_default() {
            Ok((stream, handle)) => Some(Audio {
                _stream: stream,
                handle,
                volume,
            }),
            Err(e) => {
                eprintln!("audio: no output device ({e}); running silent");
                None
            }
        }
    }

    pub fn play(&self, sfx: Sfx) {
        if self.volume <= 0.0 {
            return;
        }
        let samples = synth(sfx);
        let src = SamplesBuffer::new(1, RATE, samples).amplify(self.volume);
        let _ = self.handle.play_raw(src.convert_samples());
    }
}

// ---------------- synthesis ----------------

struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.0 >> 8) as f32 / (1 << 24) as f32 * 2.0 - 1.0
    }
}

/// Filtered-noise burst: `cutoff` colors the material, `pitchy` adds a tonal
/// body at `tone_hz`.
fn burst(dur: f32, cutoff: f32, tone_hz: f32, tonal: f32, punch: f32, seed: u32) -> Vec<f32> {
    let n = (dur * RATE as f32) as usize;
    let mut rng = Rng(seed);
    let mut out = Vec::with_capacity(n);
    let alpha = (cutoff / RATE as f32 * std::f32::consts::TAU).min(1.0);
    let mut lp = 0.0f32;
    for i in 0..n {
        let t = i as f32 / RATE as f32;
        let env = (1.0 - t / dur).powf(punch);
        lp += alpha * (rng.next() - lp);
        let tone = (t * tone_hz * std::f32::consts::TAU).sin();
        out.push((lp * (1.0 - tonal) + tone * tonal) * env * 0.5);
    }
    out
}

/// Frequency sweep chirp.
fn chirp(dur: f32, f0: f32, f1: f32) -> Vec<f32> {
    let n = (dur * RATE as f32) as usize;
    let mut out = Vec::with_capacity(n);
    let mut phase = 0.0f32;
    for i in 0..n {
        let t = i as f32 / n as f32;
        let f = f0 + (f1 - f0) * t;
        phase += f * std::f32::consts::TAU / RATE as f32;
        let env = (1.0 - t).powf(1.5) * (t * 40.0).min(1.0);
        out.push(phase.sin() * env * 0.4);
    }
    out
}

fn synth(sfx: Sfx) -> Vec<f32> {
    match sfx {
        Sfx::Break(m) => match m {
            BreakMat::Stone => burst(0.16, 900.0, 100.0, 0.15, 2.0, 11),
            BreakMat::Wood => burst(0.14, 500.0, 170.0, 0.45, 2.5, 22),
            BreakMat::Soft => burst(0.20, 1600.0, 0.0, 0.0, 1.5, 33),
            BreakMat::Leafy => burst(0.18, 3400.0, 0.0, 0.0, 1.2, 44),
        },
        Sfx::Place => burst(0.10, 700.0, 150.0, 0.5, 3.0, 66),
        Sfx::Pickup => chirp(0.12, 420.0, 1000.0),
        Sfx::Click => burst(0.03, 2500.0, 0.0, 0.0, 1.0, 77),
        Sfx::Hurt => burst(0.22, 300.0, 90.0, 0.6, 1.8, 88),
        Sfx::MobHurt(p) => burst(0.16, 320.0 * p, 110.0 * p, 0.5, 2.0, 121),
        Sfx::MobDeath(p) => burst(0.34, 240.0 * p, 55.0 * p, 0.7, 1.4, 122),
        Sfx::Bolt(p) => chirp(0.14, 900.0 * p, 300.0 * p),
        Sfx::Craft => burst(0.12, 600.0, 200.0, 0.5, 2.5, 99),
        Sfx::Splash => burst(0.30, 1200.0, 0.0, 0.0, 1.2, 111),
    }
}
