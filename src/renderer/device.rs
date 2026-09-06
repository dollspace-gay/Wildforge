//! Surface-compatible hardware selection and configured device creation.

use std::sync::Arc;
use winit::window::Window;

pub(super) struct DeviceState {
    pub(super) surface: wgpu::Surface<'static>,
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) config: wgpu::SurfaceConfiguration,
    pub(super) adapter_name: String,
    pub(super) adapter_backend: String,
    pub(super) adapter_hardware: bool,
}

fn adapter_priority(info: &wgpu::AdapterInfo) -> u8 {
    let device = match info.device_type {
        wgpu::DeviceType::DiscreteGpu => 4,
        wgpu::DeviceType::IntegratedGpu => 3,
        wgpu::DeviceType::VirtualGpu => 2,
        wgpu::DeviceType::Other => 1,
        wgpu::DeviceType::Cpu => 0,
    };
    let backend = match info.backend {
        wgpu::Backend::Dx12 | wgpu::Backend::Metal => 4,
        wgpu::Backend::Vulkan => 3,
        wgpu::Backend::Gl => 2,
        wgpu::Backend::BrowserWebGpu => 1,
        wgpu::Backend::Noop => 0,
    };
    device * 8 + backend
}

impl DeviceState {
    pub(super) async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let instance_descriptor = wgpu::InstanceDescriptor::from_env_or_default();
        let enabled_backends = instance_descriptor.backends;
        let instance = wgpu::Instance::new(&instance_descriptor);
        let surface = instance.create_surface(window).expect("create surface");
        let mut available = Vec::new();
        let adapter = instance
            .enumerate_adapters(enabled_backends)
            .into_iter()
            .filter(|adapter| adapter.is_surface_supported(&surface))
            .inspect(|adapter| available.push(adapter.get_info()))
            .filter(|adapter| adapter.get_info().device_type != wgpu::DeviceType::Cpu)
            .max_by_key(|adapter| adapter_priority(&adapter.get_info()))
            .unwrap_or_else(|| {
                let listed = available
                    .iter()
                    .map(|info| {
                        format!(
                            "{} [{:?}, {:?}]",
                            info.name, info.backend, info.device_type
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                panic!(
                    "no surface-compatible GPU adapter found; refusing CPU software rendering (available: {listed})"
                )
            });
        let info = adapter.get_info();
        let adapter_name = format!("{} [{:?}, {:?}]", info.name, info.backend, info.device_type);
        let adapter_backend = format!("{:?}", info.backend);
        let adapter_hardware = info.device_type != wgpu::DeviceType::Cpu;
        eprintln!("renderer: using {adapter_name}");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults()
                    .using_resolution(adapter.limits()),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await
            .expect("request device");

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            // Some accelerated presentation paths (notably Mesa's D3D12
            // driver under WSLg) expose swapchain images only as render
            // targets. Screenshots use their own copyable render target, so
            // presentation never needs COPY_SRC support.
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        Self {
            surface,
            device,
            queue,
            config,
            adapter_name,
            adapter_backend,
            adapter_hardware,
        }
    }
}
