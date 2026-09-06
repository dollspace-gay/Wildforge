//! One native device and content identity shared by every campaign capture.

use super::CaptureMetadata;

pub(super) struct NativeIdentity {
    adapter: String,
    backend: String,
    pub(super) content_hash: String,
}

impl NativeIdentity {
    pub(super) fn from_capture(metadata: &CaptureMetadata) -> Result<Self, String> {
        let render = &metadata.render;
        if !render.hardware
            || !matches!(render.backend.as_str(), "Vulkan" | "Dx12")
            || !render
                .adapter
                .ends_with(&format!("[{}, DiscreteGpu]", render.backend))
            || !super::valid_hex(&metadata.world.atlas_content_hash, 16)
        {
            return Err(
                "campaign foundation requires a native discrete GPU and content identity".into(),
            );
        }
        Ok(Self {
            adapter: render.adapter.clone(),
            backend: render.backend.clone(),
            content_hash: metadata.world.atlas_content_hash.clone(),
        })
    }

    pub(super) fn matches(&self, metadata: &CaptureMetadata) -> bool {
        metadata.render.hardware
            && metadata.render.adapter == self.adapter
            && metadata.render.backend == self.backend
            && metadata.world.atlas_content_hash == self.content_hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visual_capture::CaptureSidecar;

    fn reference() -> CaptureMetadata {
        let capture: CaptureSidecar = toml::from_str(include_str!(
            "../../screenshots/visual-polish-foundation-a.capture.toml"
        ))
        .unwrap();
        capture.metadata
    }

    #[test]
    fn accepts_recorded_native_devices_without_fixing_a_hardware_generation() {
        let mut capture = reference();
        for (adapter, backend) in [
            ("Test discrete adapter [Vulkan, DiscreteGpu]", "Vulkan"),
            ("Test discrete adapter [Dx12, DiscreteGpu]", "Dx12"),
        ] {
            capture.render.adapter = adapter.into();
            capture.render.backend = backend.into();
            assert!(
                NativeIdentity::from_capture(&capture)
                    .unwrap()
                    .matches(&capture)
            );
        }
    }

    #[test]
    fn rejects_software_rendering_and_mixed_capture_identities() {
        let capture = reference();
        let native = NativeIdentity::from_capture(&capture).unwrap();
        let mut changed = capture.clone();
        changed.render.hardware = false;
        assert!(NativeIdentity::from_capture(&changed).is_err());
        assert!(!native.matches(&changed));
        changed = capture.clone();
        changed.render.adapter = "Software adapter [Vulkan, Cpu]".into();
        assert!(NativeIdentity::from_capture(&changed).is_err());
        assert!(!native.matches(&changed));
        changed = capture.clone();
        changed.render.backend = "Gl".into();
        assert!(NativeIdentity::from_capture(&changed).is_err());
        assert!(!native.matches(&changed));
        changed = capture;
        changed.world.atlas_content_hash = "0000000000000000".into();
        assert!(!native.matches(&changed));
    }
}
