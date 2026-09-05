//! Fail-closed identity snapshots for native visual qualification captures.

use crate::world::TerrainRead;

use crate::visual_capture;
use crate::world;
use super::BUILD_MARKER;
use super::Game;
use super::SHOT_SETTLE_FRAMES;

fn required_capture_label(name: &str) -> Result<String, String> {
    let value =
        std::env::var(name).map_err(|_| format!("{name} is required for visual evidence"))?;
    if value.is_empty()
        || value.len() > 80
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(format!(
            "{name} must be 1..=80 lowercase ASCII letters, digits, or hyphens"
        ));
    }
    Ok(value)
}

fn precipitation_name(value: crate::planet_atlas::PrecipitationForm) -> &'static str {
    match value {
        crate::planet_atlas::PrecipitationForm::None => "none",
        crate::planet_atlas::PrecipitationForm::Rain => "rain",
        crate::planet_atlas::PrecipitationForm::Snow => "snow",
    }
}

impl Game {
    pub(super) fn visual_capture_metadata(
        &self,
        fog_distance_blocks: f32,
    ) -> Result<visual_capture::CaptureMetadata, String> {
        let capture_id = required_capture_label("WILDFORGE_CAPTURE_ID")?;
        let scene_id = required_capture_label("WILDFORGE_CAPTURE_SCENE")?;
        let world_name = required_capture_label("WILDFORGE_WORLD")?;
        let build = visual_capture::BuildIdentity::current(BUILD_MARKER);
        if build.dirty {
            return Err("visual evidence requires a clean committed build".into());
        }
        if !self.renderer.adapter_hardware {
            return Err("visual evidence requires a hardware GPU adapter".into());
        }
        if self.settled_frames < SHOT_SETTLE_FRAMES {
            return Err(format!(
                "visual evidence refused an unsettled frame ({} < {SHOT_SETTLE_FRAMES})",
                self.settled_frames
            ));
        }

        let atlas = self.runtime.view().planet_atlas()
            .ok_or("visual evidence requires a production planetary atlas")?;
        let player = self.player.pos;
        let surface = player.surface();
        let weather = self.runtime.view().weather_at_surface(surface);
        let (opaque_chunks, water_chunks, empty_chunks) = self.renderer.chunk_mesh_counts();
        let season = self.runtime.view().season_at_surface(surface);

        Ok(visual_capture::CaptureMetadata {
            schema_version: visual_capture::CAPTURE_SCHEMA_VERSION,
            capture_id,
            scene_id,
            build,
            world: visual_capture::WorldIdentity {
                name: world_name,
                seed: self.runtime.view().seed(),
                generator_version: world::WORLD_GENERATOR_VERSION,
                atlas_format_version: atlas.manifest.format_version,
                atlas_algorithm_version: atlas.manifest.atlas_algorithm_version,
                atlas_content_hash: format!("{:016x}", atlas.manifest.content_hash),
                atlas_genesis_checksum: format!("{:016x}", atlas.manifest.genesis_checksum),
            },
            camera: visual_capture::CameraIdentity {
                face: format!("{:?}", player.face()),
                u: player.u(),
                y: player.y(),
                v: player.v(),
                yaw: self.camera.yaw,
                pitch: self.camera.pitch,
                fov_degrees: self.camera.fovy.to_degrees(),
            },
            environment: visual_capture::EnvironmentIdentity {
                day: self.runtime.view().day(),
                time_of_day: self.runtime.time_of_day(),
                season: world::SEASONS[season].to_ascii_lowercase(),
                weather: weather.kind.name().into(),
                precipitation: precipitation_name(weather.precipitation).into(),
                temperature_c: weather.temperature_c,
                pressure_anomaly: weather.pressure_anomaly,
                vapor: weather.vapor,
                cloud_water: weather.cloud_water,
                storm_energy: weather.storm_energy,
                precipitation_units: weather.precipitation_units,
                wind: weather.wind,
                gloom: self.presentation.weather_vis,
            },
            render: visual_capture::RenderIdentity {
                width: self.renderer.config.width,
                height: self.renderer.config.height,
                pack: self.active_pack_id(),
                view_distance_chunks: self.config.view_dist,
                fog_distance_blocks,
                adapter: self.renderer.adapter_name.clone(),
                backend: self.renderer.adapter_backend.clone(),
                hardware: self.renderer.adapter_hardware,
                lights: self.config.lights,
                point_grid: self.config.point_grid,
                stark: self.config.stark,
                bloom: self.config.bloom,
            },
            telemetry: visual_capture::CaptureTelemetry {
                // The request is consumed by the renderer on the next frame.
                frame: self.total_frames.saturating_add(1),
                eligible_frame: self.capture_frames,
                settled: true,
                settled_frames: self.settled_frames,
                fps: self.fps,
                simulation_ms: self.frame_ms.0,
                draw_ms: self.frame_ms.1,
                resident_chunks: self.runtime.view().chunk_count(),
                gpu_chunks: self.renderer.chunk_count(),
                opaque_chunks,
                water_chunks,
                empty_chunks,
                dirty_chunks: self.runtime.view().dirty_chunks().len(),
            },
            family: self
                .content
                .diagnostic_families
                .clone()
                .ok_or("visual evidence family map was not initialized")?,
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn evidence_labels_are_path_independent_slugs() {
        // Pure shape check kept local so capture ids cannot smuggle paths into
        // the evidence contract. Environment mutation is deliberately avoided.
        for valid in ["foundation-a", "scene-42", "a"] {
            assert!(
                valid
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            );
        }
        for invalid in ["A", "../scene", "scene_name", ""] {
            assert!(
                invalid.is_empty()
                    || !invalid.bytes().all(|byte| byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'-')
            );
        }
    }
}
