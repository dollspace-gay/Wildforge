//! Lighting in the graphical frame pipeline.

use super::{LightingFrame, sun_scale};
use crate::audio;
use crate::audio::Sfx;
use crate::game::Game;
use crate::game::navigation::Screen;
use glam::Vec3;

impl Game {
    pub(in crate::game) fn prepare_frame_lighting(&mut self, dt: f32) -> LightingFrame {
        let local_up = self.camera.up();
        let sun_dir_true = if self.in_world {
            self.runtime.view().sun_direction().as_vec3()
        } else {
            self.camera
                .world_vector(Vec3::new(0.0, 1.0, 0.45))
                .normalize()
        };
        let elev = sun_dir_true.dot(local_up);
        // Near-black floor: night is now carried by the moon (below), not a flat
        // ambient, so a new-moon night goes genuinely dark while a full moon
        // stays navigable. Torch light is unaffected (its own vertex channel).
        // This is the render brightness only; the sim's own daylight() (mob
        // spawns etc.) keeps its 0.12 floor untouched.
        let daylight = if self.in_world {
            (elev * 2.5 + 0.5).clamp(0.02, 1.0)
        } else {
            1.0
        };
        let day_sky = [0.55, 0.75, 0.95];
        let night_sky = [0.02, 0.03, 0.08];
        let f = daylight;
        self.renderer.sky_color = [
            night_sky[0] + (day_sky[0] - night_sky[0]) * f,
            night_sky[1] + (day_sky[1] - night_sky[1]) * f,
            night_sky[2] + (day_sky[2] - night_sky[2]) * f,
        ];

        // Sun for directional lighting. The sun arcs east->west over the day
        // (noon at time 0.25); we keep it a touch above the horizon while up so
        // shadows never degenerate. Warm direct light, cool sky-ambient fill,
        // both faded by `daylight` so night is lit only by the moonlit floor
        // and torches.
        // Warm sun, clamped just over the horizon so its shadow never
        // degenerates while it's the active light.
        let sun_tangent = (sun_dir_true - local_up * elev).normalize_or_zero();
        let warm_sun_dir = (sun_tangent + local_up * elev.max(0.05)).normalize();
        let sun_vis = elev.clamp(0.0, 1.0).sqrt(); // 0 below horizon
        // Golden hour: the sun's hue warms from near-white at noon to deep
        // orange as it nears the horizon.
        let noon = Vec3::new(1.0, 0.96, 0.86);
        let horizon = Vec3::new(1.0, 0.54, 0.26);
        // Direct sun against skylight. Real sun is one to two orders of
        // magnitude above the sky; ours sat at roughly parity, which is why
        // shadows filled in flat and a window out-lit its own sunbeam. Raising
        // it is only meaningful now that the composite tone-maps instead of
        // clamping — the range has somewhere to go.
        let warm_sun_col = horizon.lerp(noon, sun_vis) * (0.64 * sun_vis * sun_scale());
        let mut amb_col = Vec3::new(0.60, 0.68, 0.82) * (0.42 * daylight);

        // Moon: rides the anti-solar arc (up while the sun is down), cold and
        // dim, its strength set by the deterministic lunar phase — a new moon is
        // near-dark, a full moon lights the night. Clamped like the sun so its
        // shadow holds up.
        let illum = if self.in_world {
            self.runtime.view().moon_illumination()
        } else {
            0.0
        };
        let moon_elev = -elev;
        let moon_tangent = -sun_tangent;
        let moon_dir = (moon_tangent + local_up * moon_elev.max(0.05)).normalize();
        let moon_vis = moon_elev.clamp(0.0, 1.0).sqrt() * illum;
        // A strong, distinctly cold key so full-moon-lit faces clearly read as
        // lit — paired with a near-nothing fill (below) so shadows stay genuinely
        // dark. High contrast, a real directional light, not a flat ambient lift.
        let moon_col = Vec3::new(0.45, 0.60, 1.0) * (0.42 * moon_vis);
        // Barely any cold fill, folded into the sky ambient below: enough that a
        // full moon's shadowed faces read cold-dark rather than dead black, but
        // not enough to wash out the shadows.
        let moon_fill = Vec3::new(0.015, 0.022, 0.05) * moon_vis;

        // Surface lighting uses whichever body is dominant as the single
        // directional light, so the shadow map follows it for free: warm sun
        // while it's up, cold moon once it sets. Both intensities have faded to
        // ~zero at the crossover, so the swap is invisible. (The sky gradient
        // tracks the true sun via `sun_dir_true`, independently.)
        let sun_dir = if elev > 0.0 { warm_sun_dir } else { moon_dir };
        let mut sun_col = if elev > 0.0 { warm_sun_col } else { moon_col };

        // Weather gloom: fronts dim the direct sun hard and the ambient
        // gently, gray the sky, and pull the fog in. Lerped over ~10 s
        // so transitions read as skies changing, not a light switch.
        let local_weather = self.in_world.then(|| {
            self.runtime
                .view()
                .weather_at_surface(self.player.pos.surface())
        });
        let gloom_target = if let Some(weather) = local_weather {
            match weather.kind {
                crate::planet_atlas::LocalWeather::Clear => 0.0,
                crate::planet_atlas::LocalWeather::Overcast => 0.4,
                crate::planet_atlas::LocalWeather::Precipitation => 0.55,
                crate::planet_atlas::LocalWeather::Storm => 0.7,
            }
        } else {
            0.0
        };
        self.presentation.weather_vis +=
            (gloom_target - self.presentation.weather_vis) * (dt / 10.0).min(1.0);
        let gloom = self.presentation.weather_vis;
        sun_col *= 1.0 - gloom;
        amb_col *= 1.0 - gloom * 0.45;
        let gray = [0.36 * f, 0.39 * f, 0.44 * f];
        let mix = (gloom * 1.4).min(1.0);
        for (c, g) in self.renderer.sky_color.iter_mut().zip(gray) {
            *c += (g - *c) * mix;
        }

        // Storms flash: two frames of borrowed noon, thunder later.
        let mut daylight = daylight;
        if local_weather
            .is_some_and(|weather| weather.kind == crate::planet_atlas::LocalWeather::Storm)
        {
            self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
            if ((self.rng >> 8) as f32 / (1 << 24) as f32) < dt / 25.0 {
                self.presentation.lightning = 0.12;
                self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
                self.presentation.thunder_delay =
                    0.5 + ((self.rng >> 8) as f32 / (1 << 24) as f32) * 2.5;
            }
        }
        if self.presentation.lightning > 0.0 {
            self.presentation.lightning -= dt;
            daylight = 1.0;
            self.renderer.sky_color = [0.85, 0.88, 0.95];
            sun_col = Vec3::new(0.9, 0.92, 1.0);
        }
        if self.presentation.thunder_delay >= 0.0 {
            self.presentation.thunder_delay -= dt;
            if self.presentation.thunder_delay < 0.0 {
                self.sfx(Sfx::Thunder);
            }
        }

        // Project the finished sky into SH ambient — the colored, directional
        // fill light — from the same values that drive the visible dome.
        let sky_params = crate::sky::SkyParams {
            sun_dir: sun_dir_true,
            up: self.camera.up(),
            gloom,
            overcast: Vec3::from_array(self.renderer.sky_color),
            moon_fill,
        };
        let sh_ambient = crate::sky::project(&sky_params);
        // And ask the room what colour its light has become. One probe, from
        // where the player is standing — see bounce.rs for why it is only one.
        //
        // The probe works in the player's local chart — the flat, Y-up frame
        // the whole estimate is written in — so the render-space eye and sun
        // are expressed there first, through the same tangent basis the camera
        // uses. Its answer is a colour and an amount, with no direction (see
        // bounce.rs), and that rotation leaves both untouched, so nothing
        // downstream needs to know which chart it was measured in.
        let eye = self.player.eye();
        let lf = crate::planet::local_frame(eye.surface_point());
        let (east, up, north) = (lf.east.as_vec3(), lf.up.as_vec3(), lf.north.as_vec3());
        let to_chart = |d: Vec3| Vec3::new(d.dot(east), d.dot(up), d.dot(north));
        let sun_true_chart = to_chart(sun_dir_true);
        let warm_sun_chart = to_chart(warm_sun_dir);
        let sky_chart = crate::sky::SkyParams {
            sun_dir: sun_true_chart,
            up: Vec3::Y,
            gloom,
            overcast: Vec3::from_array(self.renderer.sky_color),
            moon_fill,
        };
        self.room_light.update(
            &self.runtime.view(),
            &self.block_albedo,
            eye.local(),
            warm_sun_chart,
            sun_true_chart,
            warm_sun_col,
            &sky_chart,
            // What the sun's brightness would be straight overhead, so
            // "fully lit" means the same thing at any sun-strength setting.
            0.64 * sun_scale(),
            dt,
            eye.face(),
        );
        if std::env::var("WILDFORGE_DEBUG").is_ok() && self.total_frames.is_multiple_of(60) {
            let sh = self.room_light.sh();
            eprintln!(
                "room L0 {:.4},{:.4},{:.4}  L1y {:.4},{:.4},{:.4}  nonzero {}/128  cols {:?}  intensity {:.3}",
                sh[0].x,
                sh[0].y,
                sh[0].z,
                sh[1].x,
                sh[1].y,
                sh[1].z,
                self.room_light.lit,
                self.room_light.stats,
                self.room_light.intensity
            );
        }

        // Weather wins while it is audible; in fair conditions nearby living
        // Current supplies its own restrained harmonic bed.
        let ecology_ambience = self
            .runtime
            .view()
            .perceived_arcane_ecology_at(self.player.pos.surface(), self.scan_range());
        if let Some(a) = &self.audio {
            let want = if self.ui_state.screen == Screen::Paused {
                // The pause menu holds the world's breath: no rain,
                // no wind, no crickets until you come back.
                None
            } else if local_weather.is_some_and(|weather| {
                weather.precipitation == crate::planet_atlas::PrecipitationForm::Rain
            }) {
                Some(
                    if local_weather.is_some_and(|weather| {
                        weather.kind == crate::planet_atlas::LocalWeather::Storm
                    }) {
                        audio::Ambience::Storm
                    } else {
                        audio::Ambience::Rain
                    },
                )
            } else if self.in_world
                && self.presentation.juice
                && local_weather.is_some_and(|weather| {
                    weather.kind == crate::planet_atlas::LocalWeather::Overcast
                })
            {
                // Wind is the forecast: every rain passes through it.
                Some(audio::Ambience::Wind)
            } else if self.in_world
                && self.presentation.juice
                && let Some(observation) = &ecology_ambience
            {
                Some(audio::Ambience::Current(observation.damped))
            } else if self.in_world && self.presentation.juice && daylight < 0.25 {
                // The night bed: crickets while the wild is calm; a low
                // hush once it turns wrathful. The ire meter, diegetic.
                Some(audio::Ambience::Night(
                    // The night bed reads the land underfoot: crickets
                    // in tended country, the wrathful hush where the
                    // ground remembers (legible escalation, stage 2).
                    self.runtime
                        .view()
                        .ire_tier_at_surface(self.player.pos.surface())
                        < 2,
                ))
            } else {
                None
            };
            a.set_ambience(want);
        }
        // Ambient is the engine's stark<->accessible knob. Dev override:
        // WILDFORGE_AMBIENT="r,g,b" pins a flat ambient (crush it to make
        // point-light contrast legible in tests). Applied last so it wins over
        // weather gloom.
        if let Ok(s) = std::env::var("WILDFORGE_AMBIENT") {
            let v: Vec<f32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
            if v.len() == 3 {
                amb_col = Vec3::new(v[0], v[1], v[2]);
            }
        }

        LightingFrame {
            daylight,
            sun_dir,
            sun_dir_true,
            sun_col,
            amb_col,
            gloom,
            sh_ambient,
            local_weather,
        }
    }
}
