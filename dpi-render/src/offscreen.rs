//! Nativní offscreen hostitel: vlastní zařízení, renderuje do textury
//! a vrací RGBA8 pixely (řádek 0 = horní okraj). Používá ho src-tauri
//! (render vlákno) a testy.

use std::sync::{Arc, Mutex};

pub use crate::renderer::AdapterSummary;
use crate::renderer::{device_limits, pick_formats, Renderer};

/// Formát cílové textury (sRGB hodnoty se zapisují beze změny).
pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[derive(Debug, Clone, Copy)]
pub struct OffscreenOptions {
    /// Jen softwarový (fallback) adaptér.
    pub force_fallback: bool,
    /// MSAA 4×, pokud ho adaptér umí (na CPU adaptéru vždy vypnuto).
    pub msaa: bool,
    /// Na CPU adaptéru kreslit housenky jako ploché pásky (rychlost).
    pub cpu_ribbons: bool,
}

impl Default for OffscreenOptions {
    fn default() -> Self {
        Self {
            force_fallback: false,
            msaa: true,
            cpu_ribbons: true,
        }
    }
}

struct Target {
    size: (u32, u32),
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    readback: wgpu::Buffer,
    padded_row: u32,
}

pub struct Offscreen {
    _instance: wgpu::Instance,
    _adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    summary: AdapterSummary,
    target: Option<Target>,
    /// Chyby validace z wgpu (bez handleru by wgpu panikařilo → s panic=abort pád aplikace).
    errors: Arc<Mutex<Option<String>>>,
}

fn pick_adapter(instance: &wgpu::Instance, force_fallback: bool) -> Result<wgpu::Adapter, String> {
    // WGPU_ADAPTER_NAME (podřetězec názvu) — vlastní výběr, util helper při neshodě panikaří
    if let Ok(name) = std::env::var("WGPU_ADAPTER_NAME") {
        let name = name.to_lowercase();
        let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
        if let Some(a) = adapters
            .into_iter()
            .find(|a| a.get_info().name.to_lowercase().contains(&name))
        {
            return Ok(a);
        }
    }
    let request = |fallback: bool| {
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: fallback,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
    };
    if !force_fallback {
        if let Ok(a) = request(false) {
            return Ok(a);
        }
    }
    request(true).map_err(|e| format!("viewport: žádný GPU adaptér ({e})"))
}

impl Offscreen {
    /// Výchozí volby (MSAA, ploché pásky na CPU adaptéru).
    pub fn new(force_fallback: bool) -> Result<Self, String> {
        Self::with_options(OffscreenOptions {
            force_fallback,
            ..Default::default()
        })
    }

    pub fn with_options(opts: OffscreenOptions) -> Result<Self, String> {
        // WGPU_BACKEND / WGPU_DEBUG apod. z prostředí
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pick_adapter(&instance, opts.force_fallback)?;
        let info = adapter.get_info();
        let cpu = info.device_type == wgpu::DeviceType::Cpu;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("dpi-render"),
            required_features: wgpu::Features::empty(),
            required_limits: device_limits(&adapter),
            experimental_features: Default::default(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: Default::default(),
        }))
        .map_err(|e| format!("viewport: request_device: {e}"))?;

        let errors: Arc<Mutex<Option<String>>> = Arc::default();
        let slot = errors.clone();
        device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
            eprintln!("[dpi-render] wgpu: {e}");
            if let Ok(mut s) = slot.lock() {
                s.get_or_insert_with(|| e.to_string());
            }
        }));

        let (depth, samples) = pick_formats(&adapter, COLOR_FORMAT, opts.msaa);
        let renderer = Renderer::new_with_depth(
            &device,
            &queue,
            COLOR_FORMAT,
            samples,
            cpu && opts.cpu_ribbons,
            depth,
        );
        let summary = AdapterSummary::new(&info, samples);
        Ok(Self {
            _instance: instance,
            _adapter: adapter,
            device,
            queue,
            renderer,
            summary,
            target: None,
            errors,
        })
    }

    pub fn adapter(&self) -> &AdapterSummary {
        &self.summary
    }

    fn take_error(&self) -> Result<(), String> {
        match self.errors.lock().ok().and_then(|mut s| s.take()) {
            Some(e) => Err(format!("viewport: {e}")),
            None => Ok(()),
        }
    }

    pub fn apply_scene(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.renderer
            .apply_scene(&self.device, &self.queue, bytes)?;
        self.take_error()
    }

    fn ensure_target(&mut self, size: (u32, u32)) {
        if self.target.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dpi offscreen"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let padded_row = (size.0 * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dpi readback"),
            size: padded_row as u64 * size.1 as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.target = Some(Target {
            size,
            texture,
            view,
            readback,
            padded_row,
        });
    }

    /// Vyrenderuje snímek a vrátí těsně zabalené RGBA8 (šířka × výška × 4, řádek 0 = nahoře).
    pub fn render_frame(&mut self, frame_bytes: &[u8]) -> Result<Vec<u8>, String> {
        let header = protocol_header(frame_bytes)?;
        let size = header.target_size().ok_or("snímek: nulová velikost cíle")?;
        let max = self.device.limits().max_texture_dimension_2d;
        if size.0 > max || size.1 > max {
            return Err(format!(
                "snímek: {}×{} přesahuje limit {max} px",
                size.0, size.1
            ));
        }
        self.ensure_target(size);
        let t = self.target.as_ref().expect("cíl vytvořen výše");

        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("dpi offscreen"),
            });
        self.renderer
            .encode(&self.device, &self.queue, frame_bytes, &t.view, &mut enc)?;
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &t.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &t.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(t.padded_row),
                    rows_per_image: Some(size.1),
                },
            },
            wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([enc.finish()]);

        let slice = t.readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| format!("viewport: poll: {e}"))?;
        match rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(format!("viewport: map: {e}")),
            Err(_) => return Err("viewport: map se nedokončil".into()),
        }
        let row = size.0 as usize * 4;
        let mut out = Vec::with_capacity(row * size.1 as usize);
        {
            let data = slice
                .get_mapped_range()
                .map_err(|e| format!("viewport: {e}"))?;
            if t.padded_row as usize == row {
                out.extend_from_slice(&data);
            } else {
                for r in data.chunks_exact(t.padded_row as usize) {
                    out.extend_from_slice(&r[..row]);
                }
            }
        }
        t.readback.unmap();
        self.take_error()?;
        Ok(out)
    }
}

fn protocol_header(bytes: &[u8]) -> Result<crate::protocol::FrameHeader, String> {
    let hs = std::mem::size_of::<crate::protocol::FrameHeader>();
    let raw = bytes.get(..hs).ok_or("snímek: kratší než hlavička")?;
    Ok(bytemuck::pod_read_unaligned(raw))
}
