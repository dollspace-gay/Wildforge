//! Post render in the graphical frame pipeline.

use crate::game::BUILD_MARKER;
use crate::game::Game;
use crate::game::SHOT_MAX_FRAMES;
use crate::game::SHOT_SETTLE_FRAMES;
use crate::game::advance_capture_clock;
use crate::game::navigation::Screen;
use crate::visual_capture;
use crate::world::TerrainRead;
use std::time::Instant;

impl Game {
    pub(in crate::game) fn finish_rendered_frame(&mut self, fog: f32, now: Instant) {
        // Headless verification: WILDFORGE_SHOT=path.ppm captures a frame once
        // the world is meshed, then exits.
        self.total_frames += 1;
        if let Some(path) = self.auto_shot.clone() {
            let forced: Option<u64> = std::env::var("WILDFORGE_SHOT_FRAME")
                .ok()
                .and_then(|v| v.parse().ok());
            let minimum: u64 = std::env::var("WILDFORGE_SHOT_MIN_FRAME")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            // Direct world/join captures must not mistake a responsive
            // pre-entry loading screen for a settled world. World opening is
            // intentionally asynchronous now, so `chunk_work_pending()` can
            // be zero before the background entry task hands over its first
            // playable session.
            let awaiting_direct_entry = !self.in_world
                && (std::env::var_os("WILDFORGE_WORLD").is_some()
                    || std::env::var_os("WILDFORGE_JOIN").is_some());
            self.capture_frames = advance_capture_clock(self.capture_frames, awaiting_direct_entry);
            if !awaiting_direct_entry && self.chunk_work_pending() == 0 {
                self.settled_frames += 1;
            } else {
                self.settled_frames = 0;
            }
            if self.settled_frames == SHOT_SETTLE_FRAMES && visual_capture::evidence_enabled() {
                eprintln!("visual evidence: initial chunk uploads settled");
            }
            // A minimum frame keeps the ordinary settled-world requirement.
            // It is useful when the visual under test needs simulation warmup;
            // unlike SHOT_FRAME, it never captures half-meshed terrain.
            let ready = !awaiting_direct_entry
                && self.capture_frames >= minimum
                && match forced {
                    Some(frame) => self.capture_frames >= frame,
                    None => {
                        self.settled_frames >= SHOT_SETTLE_FRAMES
                            || self.capture_frames >= SHOT_MAX_FRAMES
                    }
                };
            match self.shot_at {
                Some(at) if self.total_frames > at + 1 => std::process::exit(0),
                None if ready => {
                    let player = self.player.pos;
                    let surface = crate::planet::SurfacePos::new(
                        player.face(),
                        player.u().floor() as u16,
                        player.v().floor() as u16,
                    )
                    .expect("canonical player has a valid capture column");
                    let column_top = self.runtime.view().surface_height_at(surface);
                    let (opaque_chunks, water_chunks, empty_chunks) =
                        self.renderer.chunk_mesh_counts();
                    eprintln!(
                        "capture at frame {} (eligible {}, {}), fps {}, sim {:.2}ms draw {:.2}ms; player {:?} {:.1},{:.1},{:.1}, column top {}; chunks resident {}, gpu {}, opaque {}, water {}, empty {}, dirty {}",
                        self.total_frames,
                        self.capture_frames,
                        if forced.is_some() {
                            "forced frame".to_string()
                        } else if self.settled_frames >= SHOT_SETTLE_FRAMES {
                            format!("world settled {} frames", self.settled_frames)
                        } else {
                            "TIMED OUT, world still changing".to_string()
                        },
                        self.fps,
                        self.frame_ms.0,
                        self.frame_ms.1,
                        player.face(),
                        player.u(),
                        player.y(),
                        player.v(),
                        column_top,
                        self.runtime.view().chunk_count(),
                        self.renderer.chunk_count(),
                        opaque_chunks,
                        water_chunks,
                        empty_chunks,
                        self.runtime.view().dirty_chunks().len(),
                    );
                    if visual_capture::evidence_enabled() {
                        let metadata = self
                            .visual_capture_metadata(fog)
                            .unwrap_or_else(|error| panic!("visual evidence refused: {error}"));
                        self.renderer.pending_capture_metadata = Some(metadata);
                    }
                    self.renderer.pending_screenshot = Some(path);
                    self.shot_at = Some(self.total_frames);
                }
                _ => {}
            }
        }

        // Window-title HUD.
        self.frames += 1;
        if (now - self.last_title).as_secs_f32() > 0.5 {
            self.fps = (self.frames as f32 / (now - self.last_title).as_secs_f32()) as u32;
            self.frames = 0;
            self.last_title = now;
            let p = self.player.pos;
            let biome = if self.in_world {
                format!(
                    " | {}",
                    p.block()
                        .map(|at| self.runtime.view().biome_here_at(at.surface()).name())
                        .unwrap_or("Beyond the world")
                )
            } else {
                String::new()
            };
            let key_probe = match (self.input.keys.a, self.input.keys.d) {
                (true, false) => " | KEY A",
                (false, true) => " | KEY D",
                (true, true) => " | KEYS A+D",
                (false, false) => "",
            };
            self.window.set_title(&format!(
                "Wildforge {BUILD_MARKER} — {} fps (sim {:.1}ms, draw {:.1}ms, {}) | XYZ {:.1} / {:.1} / {:.1}{biome}{key_probe}{}",
                self.fps,
                self.frame_ms.0,
                self.frame_ms.1,
                self.renderer.adapter_name,
                p.x,
                p.y,
                p.z,
                if self.input.captured() || self.ui_state.screen != Screen::Playing {
                    ""
                } else {
                    "  [click to capture mouse]"
                },
            ));
        }
    }
}
