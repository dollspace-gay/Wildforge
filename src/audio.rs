//! Procedurally synthesized sound effects — no audio files.
//! Degrades to silence if no output device is available.

use rodio::Sink;
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

/// Footstep surfaces (mapped from the block underfoot).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StepMat {
    Stone,
    Wood,
    Soft,
    Leafy,
    Snow,
    Loose,
}

/// Map a block to its footstep surface: name-based specials first,
/// then the break-material fallback.
pub fn step_mat(name: &str, fallback: BreakMat) -> StepMat {
    if name.contains("snow") {
        return StepMat::Snow;
    }
    if name.contains("sand") || name.contains("gravel") {
        return StepMat::Loose;
    }
    match fallback {
        BreakMat::Stone => StepMat::Stone,
        BreakMat::Wood => StepMat::Wood,
        BreakMat::Soft => StepMat::Soft,
        BreakMat::Leafy => StepMat::Leafy,
    }
}

/// The collection ramp: consecutive pickups climb in near-semitone
/// steps, capped at +7, so a harvest becomes a rising melody.
pub fn pickup_pitch(streak: u32) -> f32 {
    2.0f32.powf(streak.min(7) as f32 / 12.0)
}

#[derive(Clone, Copy, Debug)]
pub enum Sfx {
    Break(BreakMat),
    Place,
    /// Pickup chime at a pitch (1.0 = base; see `pickup_pitch`).
    Pickup2(f32),
    Pickup,
    /// A footstep on a surface, at a varied pitch.
    Step(StepMat, f32),
    /// Heavy landing thump.
    Thud,
    /// Anvil spark ring.
    Spark,
    /// One quern turn: stone on stone.
    Grind,
    /// A hungry stomach's complaint.
    Rumble,
    /// A nearby warden shifts its weight (pitched per species).
    Presence(f32),
    Click,
    Hurt,
    Craft,
    Splash,
    /// Animal hit/death thumps, pitched per species (1.0 = deer-sized).
    MobHurt(f32),
    MobDeath(f32),
    /// Warden bolt cast/whoosh.
    Bolt(f32),
    /// Distant thunder, delayed after the flash.
    Thunder,
}

/// Looping background weather beds.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Ambience {
    Rain,
    Storm,
    /// Overcast wind: the forecast you can hear.
    Wind,
    /// The night bed; `true` = calm (crickets), `false` = wrathful hush.
    Night(bool),
}

pub struct Audio {
    _stream: OutputStream,
    handle: OutputStreamHandle,
    pub volume: f32,
    ambience: std::cell::RefCell<Option<(Ambience, Sink)>>,
}

impl Audio {
    pub fn new(volume: f32) -> Option<Audio> {
        match OutputStream::try_default() {
            Ok((stream, handle)) => Some(Audio {
                _stream: stream,
                handle,
                volume,
                ambience: std::cell::RefCell::new(None),
            }),
            Err(e) => {
                eprintln!("audio: no output device ({e}); running silent");
                None
            }
        }
    }

    /// Swap the looping weather bed (None fades it out). Idempotent
    /// per kind — call every frame with the desired state.
    pub fn set_ambience(&self, want: Option<Ambience>) {
        let mut cur = self.ambience.borrow_mut();
        match (want, cur.as_ref().map(|(k, _)| *k)) {
            (Some(w), Some(c)) if w == c => {}
            (None, None) => {}
            (want, _) => {
                *cur = None; // dropping the sink stops the old loop
                if let Some(kind) = want {
                    if self.volume <= 0.0 {
                        return;
                    }
                    if let Ok(sink) = Sink::try_new(&self.handle) {
                        let samples = ambience_loop(kind);
                        let src = SamplesBuffer::new(1, RATE, samples)
                            .repeat_infinite()
                            .amplify(self.volume * 0.35);
                        sink.append(src);
                        *cur = Some((kind, sink));
                    }
                }
            }
        }
    }

    pub fn play(&self, sfx: Sfx) {
        self.play_vol(sfx, 1.0);
    }

    pub fn play_vol(&self, sfx: Sfx, vol: f32) {
        if self.volume <= 0.0 || vol <= 0.0 {
            return;
        }
        let samples = synth(sfx);
        let src = SamplesBuffer::new(1, RATE, samples).amplify(self.volume * vol.min(1.0));
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
        Sfx::Thunder => burst(1.4, 240.0, 48.0, 0.5, 0.9, 137),
        Sfx::Pickup2(p) => chirp(0.12, 420.0 * p, 1000.0 * p),
        Sfx::Step(m, p) => match m {
            StepMat::Stone => burst(0.07, 1100.0 * p, 0.0, 0.0, 1.6, 141),
            StepMat::Wood => burst(0.08, 640.0 * p, 130.0 * p, 0.3, 1.8, 143),
            StepMat::Soft => burst(0.09, 700.0 * p, 0.0, 0.0, 1.2, 145),
            StepMat::Leafy => burst(0.10, 2600.0 * p, 0.0, 0.0, 1.0, 147),
            StepMat::Snow => burst(0.11, 1900.0 * p, 0.0, 0.0, 0.9, 149),
            StepMat::Loose => burst(0.10, 1400.0 * p, 0.0, 0.0, 1.1, 151),
        },
        Sfx::Thud => burst(0.18, 260.0, 70.0, 0.5, 2.2, 153),
        Sfx::Spark => burst(0.12, 3200.0, 2400.0, 0.6, 2.6, 157),
        Sfx::Grind => burst(0.30, 480.0, 90.0, 0.25, 0.8, 163),
        Sfx::Rumble => burst(0.35, 180.0, 55.0, 0.6, 0.6, 167),
        Sfx::Presence(p) => burst(0.25, 420.0 * p, 75.0 * p, 0.5, 0.7, 173),
        Sfx::MobHurt(p) => burst(0.16, 320.0 * p, 110.0 * p, 0.5, 2.0, 121),
        Sfx::MobDeath(p) => burst(0.34, 240.0 * p, 55.0 * p, 0.7, 1.4, 122),
        Sfx::Bolt(p) => chirp(0.14, 900.0 * p, 300.0 * p),
        Sfx::Craft => burst(0.12, 600.0, 200.0, 0.5, 2.5, 99),
        Sfx::Splash => burst(0.30, 1200.0, 0.0, 0.0, 1.2, 111),
    }
}

/// A seamless few seconds of weather noise for the ambience sink.
/// Rain is hissy; storms sit lower with slow surges.
fn ambience_loop(kind: Ambience) -> Vec<f32> {
    let dur = 4.0f32;
    let n = (dur * RATE as f32) as usize;
    let mut rng = Rng(if kind == Ambience::Rain { 4242 } else { 8484 });
    let (cutoff, gain) = match kind {
        Ambience::Rain => (2400.0, 0.5),
        Ambience::Storm => (900.0, 0.8),
        Ambience::Wind => (500.0, 0.55),
        Ambience::Night(_) => (700.0, 0.30),
    };
    let alpha = (cutoff / RATE as f32).min(1.0);
    let mut lp = 0.0f32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / n as f32;
        lp += alpha * (rng.next() - lp);
        // A slow surge cycle keeps it from reading as flat static; the
        // cycle completes over the loop so the seam is inaudible.
        let surge = 1.0 + 0.35 * (t * std::f32::consts::TAU).sin();
        out.push(lp * gain * surge);
    }
    // A wrathful night is just the low wash: the crickets have gone
    // quiet, and you notice.
    if kind == Ambience::Night(true) {
        // Crickets: short resonant chirps in a steady rhythm.
        let chirp_hz = 4300.0;
        for c in 0..14 {
            let start = (c as f32 * 0.29 * RATE as f32) as usize;
            for j in 0..(0.035 * RATE as f32) as usize {
                let k = start + j;
                if k >= n {
                    break;
                }
                let env = (1.0 - j as f32 / (0.035 * RATE as f32)).max(0.0);
                out[k] +=
                    (j as f32 / RATE as f32 * chirp_hz * std::f32::consts::TAU).sin() * env * 0.10;
            }
        }
    }
    out
}
