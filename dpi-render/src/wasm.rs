//! Wasm hostitel: WebGL2 přímo v plátně (wgpu GL backend).
//!
//! Plocha (surface) je `Rgba8Unorm` (bez sRGB převodu), takže barvy odpovídají
//! nativnímu offscreen výstupu. Velikost bufferu plátna (`canvas.width/height`)
//! nastavuje renderer podle `FrameHeader::viewport` — CSS velikost řeší frontend.

use std::sync::{Arc, Mutex};

use wasm_bindgen::prelude::*;

use crate::protocol::FrameHeader;
use crate::renderer::{device_limits, pick_formats, AdapterSummary, Renderer};

fn js_err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

#[wasm_bindgen]
pub struct WasmViewport {
    _instance: wgpu::Instance,
    _adapter: wgpu::Adapter,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    summary_json: String,
    errors: Arc<Mutex<Option<String>>>,
}

#[wasm_bindgen]
impl WasmViewport {
    /// Vytvoří WebGL2 kontext nad plátnem (plochu před adaptérem — WebGL to vyžaduje).
    pub async fn create(canvas: web_sys::HtmlCanvasElement) -> Result<WasmViewport, JsValue> {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = wgpu::Backends::GL;
        let instance = wgpu::Instance::new(desc);
        let (w, h) = (canvas.width().max(1), canvas.height().max(1));
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(js_err)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
                apply_limit_buckets: false,
            })
            .await
            .map_err(js_err)?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("dpi-render"),
                required_features: wgpu::Features::empty(),
                required_limits: device_limits(&adapter),
                experimental_features: Default::default(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: Default::default(),
            })
            .await
            .map_err(js_err)?;

        let errors: Arc<Mutex<Option<String>>> = Arc::default();
        let slot = errors.clone();
        device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
            if let Ok(mut s) = slot.lock() {
                s.get_or_insert_with(|| e.to_string());
            }
        }));

        let caps = surface.get_capabilities(&adapter);
        // Ne-sRGB formát → hodnoty se zapisují beze změny jako nativně
        let format = [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Bgra8Unorm,
        ]
        .into_iter()
        .find(|f| caps.formats.contains(f))
        .or_else(|| caps.formats.first().copied())
        .ok_or_else(|| js_err("viewport: plocha nepodporuje žádný formát"))?;
        let mut config = surface
            .get_default_config(&adapter, w, h)
            .ok_or_else(|| js_err("viewport: plocha není s adaptérem kompatibilní"))?;
        config.format = format;
        if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque) {
            config.alpha_mode = wgpu::CompositeAlphaMode::Opaque;
        }
        surface.configure(&device, &config);

        let info = adapter.get_info();
        let cpu = info.device_type == wgpu::DeviceType::Cpu;
        let (depth, samples) = pick_formats(&adapter, format, true);
        let renderer = Renderer::new_with_depth(&device, &queue, format, samples, cpu, depth);
        let summary_json =
            serde_json::to_string(&AdapterSummary::new(&info, samples)).map_err(js_err)?;
        Ok(WasmViewport {
            _instance: instance,
            _adapter: adapter,
            surface,
            device,
            queue,
            config,
            renderer,
            summary_json,
            errors,
        })
    }

    /// JSON stejného tvaru jako nativní `AdapterSummary`.
    pub fn adapter_info(&self) -> String {
        self.summary_json.clone()
    }

    pub fn apply_scene(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.renderer
            .apply_scene(&self.device, &self.queue, bytes)
            .map_err(js_err)?;
        self.take_error()
    }

    /// Vyrenderuje snímek do plátna; při změně `header.viewport` přenastaví plochu.
    pub fn render(&mut self, frame: &[u8]) -> Result<(), JsValue> {
        let hs = std::mem::size_of::<FrameHeader>();
        let raw = frame
            .get(..hs)
            .ok_or_else(|| js_err("snímek: kratší než hlavička"))?;
        let header: FrameHeader = bytemuck::pod_read_unaligned(raw);
        let (w, h) = header
            .target_size()
            .ok_or_else(|| js_err("snímek: nulová velikost cíle"))?;
        let max = self.device.limits().max_texture_dimension_2d;
        let (w, h) = (w.min(max), h.min(max));
        if (w, h) != (self.config.width, self.config.height) {
            self.config.width = w;
            self.config.height = h;
            self.surface.configure(&self.device, &self.config);
        }
        let st = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                // Přenastavit a snímek vynechat (frontend pošle další)
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(())
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(js_err("viewport: chyba validace plochy"))
            }
        };
        let view = st.texture.create_view(&Default::default());
        let res = self
            .renderer
            .render(&self.device, &self.queue, frame, &view);
        drop(view);
        self.queue.present(st);
        res.map_err(js_err)?;
        self.take_error()
    }

    fn take_error(&self) -> Result<(), JsValue> {
        match self.errors.lock().ok().and_then(|mut s| s.take()) {
            Some(e) => Err(js_err(format!("viewport: {e}"))),
            None => Ok(()),
        }
    }
}
