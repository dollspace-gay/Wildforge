//! Room light: what colour the light in here has become.
//!
//! Not a bounce solver. This asks one question, once, from where the player is
//! standing — *of the light landing on surfaces around me, what colour is it?*
//! — and skews the ambient by the answer. A room whose sunbeam falls on red
//! plaster picks up red everywhere; move the beam onto blue and the room turns.
//!
//! That is deliberately one colour for the whole room. Bounced light is very
//! low frequency, and the machinery that gives it spatial variation — a lattice
//! of probes, per-pixel visibility, interpolation between them — costs orders
//! of magnitude more and buys detail that reads as blotches rather than as
//! light. A single probe cannot flicker, cannot leak through a wall, and cannot
//! disagree with itself between neighbouring pixels.
//!
//! Cost is deliberately flat. A fixed number of rays is refreshed per frame
//! whatever the scene contains, because an effect whose cost varies frame to
//! frame reads as stutter even when its average is cheap.

use glam::Vec3;

use crate::planet::{self, EntityPos, Face};
use crate::raycast;
use crate::registry::BlockId;
use crate::sky::{self, SkyParams};
use crate::world::TerrainRead;

/// One ray, cast in the eye's local chart frame.
///
/// `origin`, `dir`, and the returned positions are all centred-local
/// coordinates on `face` — the flat, Y-up frame the whole probe is written in.
/// Within a face this is the exact Cartesian voxel grid the estimate used to
/// march directly; the only new thing is that `raycast_at` walks it in planet
/// coordinates, so it keeps working across chunk borders and up to the cube
/// edges.
///
/// KNOWN LIMIT. Near a face seam — within a probe's reach of a cube edge — a
/// ray can cross onto a neighbouring chart, whose coordinates do not share this
/// frame. Such a hit still stops the ray (so a wall never leaks light) but is
/// reported with a zero normal, and every caller drops a zero-normal surface.
/// That is the price of doing bounce in a single flat chart; at 8192-block
/// faces it costs a strip a few blocks wide along the seams and nothing at all
/// anywhere a player actually stands.
struct LocalHit {
    /// Centre of the hit cell, centred-local.
    center: Vec3,
    /// Outward face normal in centred-local, or zero if the ray crossed a seam.
    normal: Vec3,
    id: BlockId,
}

fn cast_local(
    world: &(impl TerrainRead + ?Sized),
    face: Face,
    origin: Vec3,
    dir: Vec3,
    reach: f32,
) -> Option<LocalHit> {
    let ep = EntityPos::from_local(face, origin).ok()?;
    let hit = raycast::raycast_at(world, ep, dir, reach)?;
    let id = world.get_block_at(hit.block);
    let half = planet::FACE_BLOCKS as f32 * 0.5;
    let center = Vec3::new(
        hit.block.u() as f32 + 0.5 - half,
        hit.block.y() as f32 + 0.5,
        hit.block.v() as f32 + 0.5 - half,
    );
    // The normal is the step from the hit cell to the cell the ray came in
    // through — one unit along a single axis, as long as both stayed on our
    // chart. If either landed on another face the difference is meaningless
    // here, so the surface is disowned rather than mislabelled.
    let normal = if hit.block.face() == face && hit.adjacent.face() == face {
        Vec3::new(
            hit.adjacent.u() as f32 - hit.block.u() as f32,
            hit.adjacent.y() as f32 - hit.block.y() as f32,
            hit.adjacent.v() as f32 - hit.block.v() as f32,
        )
    } else {
        Vec3::ZERO
    };
    Some(LocalHit { center, normal, id })
}

/// Directions in the sample set. Fixed for the life of the process — never
/// rotated, never reshuffled. Numerous, because these are cast from the eye and
/// so are the one part of the estimate that still moves with the player: what
/// they mostly report is sky found through an opening, and when only a couple
/// of them fit through it, losing one as you walk takes a sixth of the room's
/// light with it. A sample set that moves gives a different answer
/// every frame, and that disagreement is what sparkles; a fixed one is merely
/// biased, and a fixed bias holds still.
const DIRS: usize = 512;
/// How many of those are re-marched each frame. The rest keep last frame's
/// answer, so the estimate is always complete and the per-frame cost is a
/// constant that does not care what the rays hit.
const PER_FRAME: usize = 48;
/// How far a ray looks, in blocks. Past this it is treated as having escaped.
const REACH: f32 = 40.0;
/// Whether the tint carries a direction as well as a colour. Off: the band-1
/// terms turn over as you walk past a light and take a third of the answer with
/// them, and AO already does the job of making geometry read.
const DIRECTIONAL: bool = false;
/// How strongly the room's lit colour tints its ambient. A free parameter:
/// this is a statistic being used as a light, so nothing fixes it but the eye.
const LIT_GAIN: f32 = 8.0;
/// Frames between full sweeps of the lattice.
const LIT_EVERY: u32 = 4;
/// Spacing of the world lattice of candidate surfaces, in blocks.
const LATTICE: f32 = 1.0;
/// How far a lit surface can be and still light you.
const RANGE: f32 = 18.0;
/// Width of the fade at that edge, so nothing enters or leaves abruptly.
const FADE: f32 = 5.0;
/// How far above a column the search for its surface starts.
const PROBE_UP: f32 = 20.0;
/// How far to look toward the sun when deciding whether a surface is lit.
const SUN_REACH: f32 = 96.0;
/// Seconds for the estimate to close most of the way to a new answer. The room
/// is being asked what colour its light is, and that is not a question whose
/// answer should change faster than the eye adapts.
const SETTLE: f32 = 0.5;

/// One probe's estimate of the light in the room, as four RGB spherical-harmonic
/// coefficients (L0 plus the three L1 terms), cosine-convolved to a diffuse
/// light multiplier — the same convention `sky::project` uses, so the shader
/// evaluates it the same way.
pub struct RoomLight {
    dirs: Vec<Vec3>,
    /// Radiance last seen along each direction. Persistent, so the estimate is
    /// whole even though only a slice of it is refreshed each frame.
    rad: Vec<Vec3>,
    next: usize,
    sh: [Vec3; 4],
    /// Diagnostics: how many sample directions currently carry any radiance.
    pub lit: usize,
    pub stats: (u32, u32, u32, u32),
    /// Last full sweep of the sunlit lattice, reused between sweeps.
    lit_sh: [Vec3; 4],
    /// How much sun is landing nearby, 0..1, independent of surface colour.
    pub intensity: f32,
    tick: u32,
}

impl Default for RoomLight {
    fn default() -> Self {
        Self::new()
    }
}

impl RoomLight {
    pub fn new() -> RoomLight {
        // Evenly spread over the sphere. The same construction `sky::project`
        // uses, and for the same reason: no clumping, no preferred axis.
        let golden = std::f32::consts::PI * (3.0 - 5.0f32.sqrt());
        let dirs = (0..DIRS)
            .map(|i| {
                let y = 1.0 - 2.0 * (i as f32 + 0.5) / DIRS as f32;
                let r = (1.0 - y * y).max(0.0).sqrt();
                let t = golden * i as f32;
                Vec3::new(t.cos() * r, y, t.sin() * r)
            })
            .collect();
        RoomLight {
            dirs,
            rad: vec![Vec3::ZERO; DIRS],
            next: 0,
            sh: [Vec3::ZERO; 4],
            lit: 0,
            stats: (0, 0, 0, 0),
            lit_sh: [Vec3::ZERO; 4],
            intensity: 0.0,
            tick: 0,
        }
    }

    /// The current estimate, ready to hand to the shader.
    pub fn sh(&self) -> [Vec3; 4] {
        self.sh
    }

    /// Re-march a slice of the sample set and rebuild the estimate.
    ///
    /// `albedo` is the per-block mean albedo table — the colour each surface
    /// hands back as its share of the bounced light.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        world: &(impl TerrainRead + ?Sized),
        albedo: &[[u8; 3]],
        eye: Vec3,
        sun_dir: Vec3,
        sun_true: Vec3,
        sun_col: Vec3,
        sky_params: &SkyParams,
        sun_ref: f32,
        dt: f32,
        face: Face,
    ) {
        for _ in 0..PER_FRAME {
            let i = self.next;
            self.next = (self.next + 1) % DIRS;
            self.rad[i] = self.sample_dir(world, eye, self.dirs[i], sky_params, face);
        }
        // Rebuild from the whole set every frame: it is 128 multiply-adds, and
        // doing it wholesale means the estimate never depends on which slice
        // happened to be refreshed.
        self.lit = self.rad.iter().filter(|r| r.max_element() > 1e-6).count();
        let mut sh = [Vec3::ZERO; 4];
        for (d, r) in self.dirs.iter().zip(self.rad.iter()) {
            sh[0] += *r * 0.282095;
            sh[1] += *r * (0.488603 * d.y);
            sh[2] += *r * (0.488603 * d.z);
            sh[3] += *r * (0.488603 * d.x);
        }
        // Monte-Carlo weight over the sphere, then the cosine convolution that
        // turns radiance into a diffuse multiplier (band 0 = 1, band 1 = 2/3).
        let w = 4.0 * std::f32::consts::PI / DIRS as f32;
        for c in sh.iter_mut() {
            *c *= w;
        }
        // The sunlit surfaces, gathered from the light's side, already carry
        // their own solid angle and so need no Monte-Carlo weight.
        //
        // Not every frame. The lattice belongs to the world, so its answer is
        // the same answer a few frames running unless the sun or the blocks
        // move — which is exactly what buys the right to recompute it rarely.
        // The settle below spreads the step out so the change is never seen.
        self.tick = self.tick.wrapping_add(1);
        if self.tick.is_multiple_of(LIT_EVERY) {
            self.lit_sh = self.sun_lit(
                world, albedo, eye, sun_dir, sun_true, sun_col, sun_ref, face,
            );
        }
        let lit = self.lit_sh;
        for (c, l) in sh.iter_mut().zip(lit.iter()) {
            *c += *l;
        }
        // Cosine convolution: band 0 keeps its weight, band 1 takes two thirds.
        for c in sh.iter_mut().skip(1) {
            *c *= 2.0 / 3.0;
        }
        if !DIRECTIONAL {
            // Flat, on purpose. The band-1 terms say which way the light came
            // from, and that direction turns over as you walk past a sunbeam —
            // a wall that was catching the positive lobe starts catching the
            // negative one, and since it is a third of the answer the room
            // visibly brightens as you cross. It is also the part nothing
            // depends on: corner occlusion is what makes the geometry read, and
            // it does that from the surfaces themselves rather than from where
            // the light happened to be. So the room gets a colour and no
            // opinion about direction.
            for c in sh.iter_mut().skip(1) {
                *c = Vec3::ZERO;
            }
        }
        // Ease toward the new answer rather than snapping to it. Every part of
        // this estimate moves when the player does — which surfaces the beam
        // samples land on, which of them are visible — and applied raw that
        // reads as the walls flashing as you walk. Framerate-independent, so
        // the settle takes the same half second however fast the frames come.
        let k = 1.0 - (-dt / SETTLE).exp();
        for (cur, new) in self.sh.iter_mut().zip(sh.iter()) {
            *cur += (*new - *cur) * k;
        }
    }

    /// What one direction brings back: nothing if it meets a surface, the
    /// sky's radiance if it meets none.
    fn sample_dir(
        &self,
        world: &(impl TerrainRead + ?Sized),
        eye: Vec3,
        dir: Vec3,
        sky_params: &SkyParams,
        face: Face,
    ) -> Vec3 {
        if cast_local(world, face, eye, dir, REACH).is_some() {
            // A surface — but sunlit surfaces are deliberately NOT collected
            // here. Hunting for a sunbeam by casting rays out from the viewer
            // and hoping one lands on it does not work: measured in the demo
            // room, two of a hundred and twenty-eight directions returned
            // anything, and both of those were rays that escaped through the
            // window to open sky. A bright patch a couple of blocks across
            // subtends almost nothing from across a room. `sun_lit` below
            // starts from the light instead, where the patch is impossible to
            // miss. This pass keeps only the sky, so the two never count the
            // same light twice.
            Vec3::ZERO
        } else {
            sky::radiance(dir, sky_params)
        }
    }

    /// Light-side sampling over a lattice that belongs to the world, not to
    /// the player.
    ///
    /// Candidate surfaces are the columns of a fixed world lattice: their
    /// positions, whether the sun reaches them, and what colour they hand back
    /// are all properties of the world alone. Walking cannot change any of
    /// them. What walking changes is only which ones are close enough to
    /// count — and each fades in and out over a few blocks, so no sample ever
    /// arrives or leaves abruptly.
    ///
    /// That is the whole difference from sampling around the viewer. A pattern
    /// hung off the eye resamples every point on every step, so a handful of
    /// hits becomes a different handful and the room changes colour as you
    /// walk — the light appearing to follow the player, which is precisely
    /// what it must not do.
    #[allow(clippy::too_many_arguments)]
    fn sun_lit(
        &mut self,
        world: &(impl TerrainRead + ?Sized),
        albedo: &[[u8; 3]],
        eye: Vec3,
        sun_dir: Vec3,
        sun_true: Vec3,
        sun_col: Vec3,
        sun_ref: f32,
        face: Face,
    ) -> [Vec3; 4] {
        let mut sh = [Vec3::ZERO; 4];
        let mut weight_all = 0.0f32;
        let mut lit_flux = 0.0f32;
        let (mut hits, mut faced, mut sunny, mut seen) = (0, 0, 0, 0);
        if sun_col.max_element() <= 1e-5 {
            return sh;
        }
        // Lattice points are absolute world positions; the centre only decides
        // which of them get visited.
        let cx = (eye.x / LATTICE).round() * LATTICE;
        let cz = (eye.z / LATTICE).round() * LATTICE;
        // Snapped, so ordinary walking (which barely changes height) does not
        // move where the search for a surface begins.
        // At the eye, never above it. Anywhere higher is inside the ceiling
        // when you are indoors, and a ray that starts inside a solid cell has
        // crossed no face — it reports that cell as its own neighbour, the
        // normal comes out zero, and every column is discarded. Rounding down
        // keeps the start in the air the player is already standing in.
        let top = (eye.y / LATTICE).floor() * LATTICE;
        let steps = (RANGE / LATTICE) as i32;
        for i in -steps..=steps {
            for j in -steps..=steps {
                let wx = cx + i as f32 * LATTICE;
                let wz = cz + j as f32 * LATTICE;
                // Straight down to whatever surface holds this column.
                //
                // KNOWN LIMIT, and the next thing to fix here: probing downward
                // can only ever find floors and other upward-facing surfaces.
                // The moment a sunbeam climbs off the floor onto a wall — which
                // is what happens as the sun gets low, and what happens in any
                // ordinary room with a window — every lit column disappears at
                // once and the room's light falls off a cliff. Nothing fades,
                // because there is nothing left being sampled.
                //
                // The fix is to drop the lattice along -sun_dir instead of
                // along -Y. Then whatever the sun strikes first is found by
                // construction, wall or floor or ceiling, for the same one ray
                // per lattice point. Straight down was a shortcut taken because
                // the demo room's beam happens to land on the floor.
                //
                // The wrinkle is that such a lattice lies in a plane
                // perpendicular to the sun, so it turns as the sun does. The
                // sun moves slowly enough that per-frame drift is negligible
                // and SETTLE covers it — but that is the thing to check, since
                // a sample set that moves under the player is the artifact this
                // whole approach exists to avoid.
                // Offset to the middle of the cell. The lattice puts every
                // coordinate on a whole number, and a ray launched from an exact
                // block corner straight down the y axis crosses three boundaries
                // at once — the march has no first face to report and hands back
                // its own starting cell as the hit.
                let Some(hit) = cast_local(
                    world,
                    face,
                    Vec3::new(wx + 0.5, top + 0.5, wz + 0.5),
                    -Vec3::Y,
                    PROBE_UP * 2.0,
                ) else {
                    continue;
                };
                hits += 1;
                let n = hit.normal;
                // A column that began inside rock, or whose surface fell across
                // a chart seam: no face was crossed here, so no surface to
                // speak of.
                if n == Vec3::ZERO {
                    continue;
                }
                // How squarely the sun strikes, from its TRUE direction. The
                // lighting sun is held a little above the horizon so its shadow
                // map never degenerates, which also means its cosine bottoms
                // out around a third and an evening room never dims. The real
                // sun lies down flat and its cosine goes to nothing, which is
                // the whole reason a low sun lights a floor so weakly.
                let cos_sun = n.dot(sun_true);
                if cos_sun <= 0.0 {
                    continue;
                }
                let face_pos = hit.center + n * 0.51;
                let to_eye = eye - face_pos;
                let d = to_eye.length();
                if !(0.5..=RANGE).contains(&d) {
                    continue;
                }
                // Fade at the edge of range, so a surface entering or leaving
                // the set does it over several blocks instead of popping.
                let fade = 1.0 - smoothstep(RANGE - FADE, RANGE, d);
                if fade <= 0.001 {
                    continue;
                }
                let dir = to_eye / d;
                faced += 1;
                // Every in-range surface counts toward the total, lit or not.
                // Dividing by this at the end turns the sum into "the average
                // colour of what is lit, times how much of the room is lit" —
                // a statistic about the room, which is what was asked for.
                weight_all += fade;
                // Where the beam falls is still asked of the clamped direction,
                // so the lit patch agrees with the one the eye can see.
                if cast_local(world, face, face_pos, sun_dir, SUN_REACH).is_some() {
                    continue;
                }
                sunny += 1;
                // How much sun is landing here, with no reference to what
                // colour the surface returns. Kept apart from the tint on
                // purpose: a beam sliding off blue plaster onto white triples
                // the light coming back without a scrap more arriving, and a
                // room should not brighten because the floor changed colour.
                lit_flux += cos_sun * fade;
                // Can the viewer see it? The only term that moves with the eye.
                if cast_local(world, face, face_pos, dir, d - 0.05).is_some() {
                    continue;
                }
                seen += 1;
                let a = albedo.get(hit.id.0 as usize).copied().unwrap_or([0; 3]);
                let a = Vec3::new(a[0] as f32, a[1] as f32, a[2] as f32) / 255.0;
                // No inverse-square, and no view cosine. Those belong to
                // irradiance arriving at a point, and a point is exactly what
                // this must not be: measured at the eye, the answer climbs by
                // half as you walk one block nearer the sunbeam, and since one
                // number tints the whole room the room brightens when you
                // approach the light. What is wanted instead is a property of
                // the room — how much of it the sun is on, and what colour that
                // is — which no amount of walking changes.
                let l = a * sun_col * (cos_sun * fade);
                let g = -dir;
                sh[0] += l * 0.282095;
                sh[1] += l * (0.488603 * g.y);
                sh[2] += l * (0.488603 * g.z);
                sh[3] += l * (0.488603 * g.x);
            }
        }
        self.stats = (hits, faced, sunny, seen);
        if weight_all > 1e-4 {
            for c in sh.iter_mut() {
                *c *= LIT_GAIN / weight_all;
            }
            // How much sun is landing nearby: the fraction of ground it is on,
            // weighted by how squarely it strikes, and by how bright the sun
            // itself is — that last term is what makes an evening room dim as
            // the sun reddens and fades, rather than only as its beam narrows.
            // Measured against a noon sun, so the sun-strength knob does not
            // change what "fully lit" means.
            let sun = sun_col.max_element() / sun_ref.max(1e-3);
            self.intensity = (lit_flux / weight_all * sun).clamp(0.0, 1.0);
        } else {
            self.intensity = 0.0;
        }
        sh
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
