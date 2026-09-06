//! Per-frame client update, scene assembly, and renderer submission.

use super::Game;
use super::SHOT_FIXED_DT;
use super::input::KeysDown;
use super::navigation::Screen;
use crate::mesher;
use crate::mobs;
use glam::Vec3;
use std::time::Instant;

/// Local authority pauses for solo play, but a windowed host keeps serving
/// guests. Remote guests never enter this path at all.
fn local_sim_should_advance(paused: bool, hosting: bool) -> bool {
    !paused || hosting
}

fn movement_axes(keys: &KeysDown) -> (f32, f32) {
    static AUTO_MOVE: std::sync::OnceLock<Option<(f32, f32)>> = std::sync::OnceLock::new();
    if let Some(axes) = *AUTO_MOVE.get_or_init(|| {
        std::env::var("WILDFORGE_AUTO_MOVE").ok().and_then(|value| {
            match value.to_ascii_uppercase().as_str() {
                "A" => Some((0.0, -1.0)),
                "D" => Some((0.0, 1.0)),
                "W" => Some((1.0, 0.0)),
                "S" => Some((-1.0, 0.0)),
                _ => None,
            }
        })
    }) {
        return axes;
    }
    (
        (keys.w as i32 - keys.s as i32) as f32,
        (keys.d as i32 - keys.a as i32) as f32,
    )
}

const VIEWMODEL_SLEEVE_MIN: Vec3 = Vec3::new(-0.055, -0.09, -0.46);
const VIEWMODEL_SLEEVE_MAX: Vec3 = Vec3::new(0.055, 0.02, 0.10);
const VIEWMODEL_HAND_MIN: Vec3 = Vec3::new(-0.055, -0.09, 0.10);
const VIEWMODEL_HAND_MAX: Vec3 = Vec3::new(0.055, 0.02, 0.26);

/// How far in front of the camera to stand the paper doll so its body
/// covers exactly `target_px` of screen height. The UI rect is fixed
/// pixels and the projection is not, so this has to be solved per frame
/// rather than picked once: a constant only ever framed one resolution.
///
/// The doll stands along world Y while the screen's vertical runs along
/// camera up, so a pitched camera foreshortens it — pulling in by the
/// same cosine keeps the framing whichever way the player was looking.
pub(super) fn portrait_depth(fovy: f32, pitch: f32, screen_h: f32, target_px: f32) -> f32 {
    let lean = pitch.cos().abs().max(0.25);
    mobs::HUMANOID_HEIGHT * lean * (screen_h * 0.5)
        / ((fovy * 0.5).tan() * target_px.max(1.0)).max(0.001)
}

/// How much brighter direct sun is than the old parity-with-sky default.
/// `WILDFORGE_SUN` overrides it.
fn sun_scale() -> f32 {
    use std::sync::OnceLock;
    static S: OnceLock<f32> = OnceLock::new();
    *S.get_or_init(|| {
        std::env::var("WILDFORGE_SUN")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|v: &f32| *v > 0.0 && *v <= 100.0)
            .unwrap_or(30.0)
    })
}

pub(super) struct LightingFrame {
    daylight: f32,
    sun_dir: Vec3,
    sun_dir_true: Vec3,
    sun_col: Vec3,
    amb_col: Vec3,
    gloom: f32,
    sh_ambient: [Vec3; 9],
    local_weather: Option<crate::planet_atlas::LocalWeatherSample>,
}

pub(super) struct SelectionFrame {
    playing: bool,
    outline: Option<crate::planet::BlockPos>,
    outline_color: [f32; 3],
    underwater: bool,
    fog: f32,
}

pub(super) struct Geometry {
    vertices: Vec<mesher::Vertex>,
    indices: Vec<u32>,
}

impl Game {
    /// Dev: WILDFORGE_LOOK pins yaw,pitch (applied at spawn, and every
    /// frame during WILDFORGE_SHOT runs — synthetic WSLg mouse events
    /// drift the camera over long headless warmups otherwise).
    pub(super) fn apply_look_env(&mut self) {
        if let Ok(l) = std::env::var("WILDFORGE_LOOK")
            && let Some((y, p)) = l.split_once(',')
            && let (Ok(y), Ok(p)) = (y.trim().parse::<f32>(), p.trim().parse::<f32>())
        {
            self.camera.yaw = y;
            self.camera.pitch = p;
        }
    }

    fn begin_frame(&mut self) -> (Instant, f32, bool) {
        let now = Instant::now();
        let dt = if self.auto_shot.is_some() {
            SHOT_FIXED_DT
        } else {
            (now - self.last_frame).as_secs_f32().min(0.05)
        };
        self.last_frame = now;
        self.time_abs += dt;

        let paused = self.ui_state.screen == Screen::Paused || !self.in_world;
        if !paused {
            self.combat
                .tick(dt, self.stamina_max(), self.stamina_regen());
            self.input.action_cooldown = (self.input.action_cooldown - dt).max(0.0);
            self.input.attack_cooldown = (self.input.attack_cooldown - dt).max(0.0);
            self.input.scroll_cooldown = (self.input.scroll_cooldown - dt).max(0.0);
            // The hand-hitch holds the swing at its peak on a connect.
            if self.presentation.hitch > 0.0 {
                self.presentation.hitch -= dt;
            } else {
                self.presentation.swing = (self.presentation.swing - dt / 0.3).max(0.0);
            }
            let hv = Vec3::new(self.player.vel.x, 0.0, self.player.vel.z).length();
            if self.player.on_ground {
                self.presentation.hand_bob += hv.min(6.0) * dt * 1.6;
            }
        }

        (now, dt, paused)
    }

    pub(super) fn update(&mut self) {
        let (now, dt, paused) = self.begin_frame();

        self.advance_feedback(dt, paused);

        self.advance_session_authority(dt, paused);

        // Native qualification runs may warm for hundreds of authoritative
        // ticks while a view-12 horizon fills. The one-shot setup override in
        // `run_demos` is not enough: live planetary weather can advance to a
        // different category before the accepted frame. Reapply only for an
        // automated capture, after simulation and immediately before lighting
        // samples the world. Ordinary play keeps the normal gradual weather.
        if self.auto_shot.is_some()
            && let Ok(requested) = std::env::var("WILDFORGE_WEATHER")
        {
            self.runtime.force_local_weather(&requested);
            self.presentation.weather_vis = match requested.as_str() {
                "overcast" => 0.4,
                "precip" | "rain" | "snow" => 0.55,
                "storm" => 0.7,
                _ => 0.0,
            };
        }

        self.refresh_content_and_toasts(dt);

        self.advance_player(dt, paused);

        self.build_and_render_frame(dt, now);
    }

    // ---------- UI layout ----------

    pub(super) const SLOT: f32 = super::inventory_panel::SLOT;
}

#[cfg(test)]
mod characterization {
    use glam::Vec3;

    use super::{
        KeysDown, VIEWMODEL_HAND_MIN, VIEWMODEL_SLEEVE_MAX, local_sim_should_advance, movement_axes,
    };

    #[test]
    fn pausing_stops_solo_sim_but_not_a_windowed_host() {
        assert!(!local_sim_should_advance(true, false));
        assert!(local_sim_should_advance(true, true));
        assert!(local_sim_should_advance(false, false));
    }

    #[test]
    fn bare_viewmodel_skin_meets_the_sleeve() {
        assert_eq!(VIEWMODEL_SLEEVE_MAX.z, VIEWMODEL_HAND_MIN.z);
    }

    #[test]
    fn d_key_moves_the_player_to_screen_right() {
        let eye =
            crate::planet::EntityPos::new(crate::planet::Face::PosZ, 1616.5, 82.0, 3312.5).unwrap();
        let mut camera = crate::camera::Camera::new(Vec3::ZERO, 16.0 / 9.0);
        camera.follow_planet(eye);
        camera.yaw = -std::f32::consts::FRAC_PI_2;

        let keys = KeysDown {
            d: true,
            ..KeysDown::default()
        };
        let (forward, strafe) = movement_axes(&keys);
        let ahead = eye
            .translated(camera.local_flat_forward() * 12.0)
            .unwrap()
            .pos;
        let moved = eye
            .translated(
                camera.local_flat_forward() * (12.0 + forward) + camera.local_right() * strafe,
            )
            .unwrap()
            .pos;
        let ahead_clip = camera.view_proj() * (ahead.render_pos() - camera.pos).extend(1.0);
        let moved_clip = camera.view_proj() * (moved.render_pos() - camera.pos).extend(1.0);
        assert!(
            moved_clip.x / moved_clip.w > ahead_clip.x / ahead_clip.w,
            "D projected left: key-to-strafe sign and camera-right sign cancel"
        );

        let keys = KeysDown {
            a: true,
            ..KeysDown::default()
        };
        let (forward, strafe) = movement_axes(&keys);
        let moved = eye
            .translated(
                camera.local_flat_forward() * (12.0 + forward) + camera.local_right() * strafe,
            )
            .unwrap()
            .pos;
        let moved_clip = camera.view_proj() * (moved.render_pos() - camera.pos).extend(1.0);
        assert!(
            moved_clip.x / moved_clip.w < ahead_clip.x / ahead_clip.w,
            "A projected right: key-to-strafe sign and camera-right sign cancel"
        );
    }
}

#[cfg(test)]
mod portrait_tests {
    use super::portrait_depth;
    use crate::mobs::HUMANOID_HEIGHT;

    /// How many pixels tall the doll actually lands, given the depth
    /// the framing solver picked. This is the projection the renderer
    /// applies, run backwards.
    fn projected_px(fovy: f32, pitch: f32, screen_h: f32, target_px: f32) -> f32 {
        let depth = portrait_depth(fovy, pitch, screen_h, target_px);
        let half_world = (fovy * 0.5).tan() * depth;
        HUMANOID_HEIGHT * pitch.cos().abs() * (screen_h * 0.5) / half_world
    }

    /// The doll is world geometry in a fixed-pixel UI rect, so a
    /// hardcoded depth framed exactly one resolution. At 720p and up
    /// the head was clipped by the name plate and the feet by the
    /// storage rows, leaving a torso that read as "no sprite at all".
    #[test]
    fn the_paper_doll_fills_its_frame_at_any_resolution() {
        let fovy = 75f32.to_radians();
        let target = 132.0;
        for screen_h in [480.0, 720.0, 1080.0, 1440.0, 2160.0] {
            let got = projected_px(fovy, 0.0, screen_h, target);
            assert!(
                (got - target).abs() < 0.5,
                "{screen_h}p framed the body at {got:.1}px, wanted {target}"
            );
        }
    }

    /// The body stands along world Y while the screen's vertical runs
    /// along camera up, so looking up or down foreshortens it. The
    /// solver pulls in by the same cosine.
    #[test]
    fn the_framing_survives_the_camera_being_pitched() {
        let fovy = 75f32.to_radians();
        let target = 132.0;
        for pitch in [-1.2f32, -0.6, 0.0, 0.6, 1.2] {
            let got = projected_px(fovy, pitch, 1080.0, target);
            assert!(
                (got - target).abs() < 0.5,
                "pitch {pitch} framed the body at {got:.1}px, wanted {target}"
            );
        }
        // Straight up or down would divide the doll away entirely; the
        // clamp keeps it a sane size instead of infinitely close.
        assert!(portrait_depth(fovy, std::f32::consts::FRAC_PI_2, 1080.0, target).is_finite());
    }

    /// Different fields of view, same frame.
    #[test]
    fn the_framing_survives_a_different_field_of_view() {
        for fov in [60f32, 75.0, 100.0] {
            let got = projected_px(fov.to_radians(), 0.0, 1080.0, 132.0);
            assert!((got - 132.0).abs() < 0.5, "fov {fov} gave {got:.1}px");
        }
    }
}

mod adaptation;
mod authority;
mod content_refresh;
mod feedback;
mod lighting;
mod player_motion;
mod point_lights;
mod post_render;
mod render;
mod scene_hand;
mod scene_members;
mod scene_overlay;
mod scene_precipitation;
mod scene_stations;
mod selection;
mod ui_effects;
mod viewmodel;
