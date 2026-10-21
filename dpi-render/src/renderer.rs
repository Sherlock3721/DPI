//! Renderer nezávislý na hostiteli: pipeline, buffery a stav scény.
//!
//! Hostitel (nativní `offscreen`, nebo `wasm` nad WebGL2) vlastní zařízení
//! a cílovou texturu; renderer do ní zakóduje celý snímek. Vnitřní MSAA
//! a hloubkové textury si drží sám (velikost podle `FrameHeader::viewport`).

use std::collections::HashMap;
use std::mem::size_of;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::meshes::{self, Template};
use crate::protocol::{self, Bed, Dot, FrameHeader, ObjStyle, Piece, Slide, SlideFrame};

/// Max. počet stylů objektů na sklo (index nad limit se ořízne na poslední).
pub const MAX_OBJECTS: usize = 128;

const GLOBALS_WGSL: &str = include_str!("shaders/globals.wgsl");
const SLIDE_WGSL: &str = include_str!("shaders/slide.wgsl");
const BEADS_WGSL: &str = include_str!("shaders/beads.wgsl");
const GLASS_WGSL: &str = include_str!("shaders/glass.wgsl");
const GROUND_WGSL: &str = include_str!("shaders/ground.wgsl");
const OVERLAY_WGSL: &str = include_str!("shaders/overlay.wgsl");

/// Počet vrcholů kužele trysky (viz overlay.wgsl: 24 segmentů × 2 trojúhelníky).
const NOZZLE_VERTICES: u32 = 24 * 2 * 3;

/// Výchozí styl, když sklo nemá žádné objekty.
const DEFAULT_STYLE: ObjStyle = ObjStyle {
    w: 0.4,
    h: 0.2,
    color: [0.6, 0.6, 0.6],
};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [f32; 16],
    eye: [f32; 4],
    view_dir: [f32; 4],
    viewport: [f32; 4],
    grid: [f32; 4],
    params: [f32; 4],
    nozzle_real: [f32; 4],
    nozzle_preview: [f32; 4],
    clear: [f32; 4],
    /// Šířka, výška desky, přesah podlahy, tloušťka podložky.
    bed: [f32; 4],
    light: [f32; 4],
    misc: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct SlideStatic {
    glass: [f32; 4],
    info: [f32; 4],
    clip: [f32; 4],
    fill: [f32; 4],
    edge: [f32; 4],
    rsv: [[f32; 4]; 3],
    objs: [[f32; 4]; 2 * MAX_OBJECTS],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct SlideDyn {
    xf0: [f32; 4],
    xf1: [f32; 4],
    cursor: [i32; 4],
    frac: [f32; 4],
}

/// Souhrn adaptéru pro frontend (stejný JSON z nativního i wasm hostitele).
#[derive(Debug, Clone, serde::Serialize)]
pub struct AdapterSummary {
    pub name: String,
    pub backend: String,
    pub device_type: String,
    pub driver: String,
    /// Softwarový adaptér (lavapipe/llvmpipe/WARP) → plochý LOD a bez MSAA.
    pub cpu: bool,
    /// Použité MSAA (1 = vypnuto).
    pub sample_count: u32,
}

impl AdapterSummary {
    pub fn new(info: &wgpu::AdapterInfo, sample_count: u32) -> Self {
        let driver = format!("{} {}", info.driver, info.driver_info)
            .trim()
            .to_string();
        Self {
            name: info.name.clone(),
            backend: info.backend.to_string(),
            device_type: format!("{:?}", info.device_type),
            driver,
            cpu: info.device_type == wgpu::DeviceType::Cpu,
            sample_count,
        }
    }
}

/// Limity zařízení: WebGL2 základ (stejné chování nativně i na webu),
/// rozlišení textur podle adaptéru (velká okna / HiDPI).
pub fn device_limits(adapter: &wgpu::Adapter) -> wgpu::Limits {
    wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())
}

/// Hloubkový formát a počet vzorků MSAA pro daný adaptér a barevný formát.
pub fn pick_formats(
    adapter: &wgpu::Adapter,
    color_format: wgpu::TextureFormat,
    want_msaa: bool,
) -> (wgpu::TextureFormat, u32) {
    let cpu = adapter.get_info().device_type == wgpu::DeviceType::Cpu;
    let ok = |f: wgpu::TextureFormat, n: u32| {
        let feat = adapter.get_texture_format_features(f);
        feat.allowed_usages
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
            && feat.flags.sample_count_supported(n)
    };
    let color_resolve = adapter
        .get_texture_format_features(color_format)
        .flags
        .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE);
    let msaa = want_msaa && !cpu && color_resolve && ok(color_format, 4);
    for depth in [
        wgpu::TextureFormat::Depth32Float,
        wgpu::TextureFormat::Depth24Plus,
    ] {
        if msaa && ok(depth, 4) {
            return (depth, 4);
        }
    }
    let depth = if ok(wgpu::TextureFormat::Depth32Float, 1) {
        wgpu::TextureFormat::Depth32Float
    } else {
        wgpu::TextureFormat::Depth24Plus
    };
    (depth, 1)
}

/// Geometrie jednoho objektu drah na GPU.
struct GpuPaths {
    pieces: Option<wgpu::Buffer>,
    n_pieces: u32,
    /// Kopie úseků s j == 0 (začátky drah) — jen pro ně se kreslí počáteční čepička.
    starts: Option<wgpu::Buffer>,
    n_starts: u32,
    dots: Option<wgpu::Buffer>,
    n_dots: u32,
    n_segs: u32,
    /// Dráha → (index prvního úseku, počet úseků); u::MAX = dráha bez úseků.
    seg_runs: Vec<(u32, u32)>,
}

impl GpuPaths {
    /// Index úseku, kterým začal tisk aktuální dráhy (uzavřená dráha od start_idx ≠ 0)
    /// — ten potřebuje počáteční čepičku navíc; `None`, když stačí začátky drah.
    fn cursor_first_piece(&self, cursor: [i32; 4]) -> Option<u32> {
        if cursor[0] != crate::cursor::MODE_PARTIAL || cursor[1] < 0 || cursor[3] < 0 {
            return None;
        }
        let &(first, n) = self.seg_runs.get(cursor[1] as usize)?;
        if first == u32::MAX || n == 0 {
            return None;
        }
        let start = cursor[2].max(0) as u32 % n;
        (start != 0)
            .then_some(first + start)
            .filter(|&i| i < self.n_pieces)
    }
}

/// Úseky dráhy jsou v bufferu za sebou (pořadí drah a bodů) → běhy podle `seg`.
fn seg_runs(pieces: &[Piece]) -> Vec<(u32, u32)> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for (i, p) in pieces.iter().enumerate() {
        let s = p.seg as usize;
        if s >= runs.len() {
            if s > 10_000_000 {
                continue; // nesmyslný index, nealokovat
            }
            runs.resize(s + 1, (u32::MAX, 0));
        }
        if runs[s].0 == u32::MAX {
            runs[s].0 = i as u32;
        }
        runs[s].1 += 1;
    }
    runs
}

struct Targets {
    size: (u32, u32),
    msaa: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
}

struct Pipelines {
    ground: wgpu::RenderPipeline,
    slab_walls: wgpu::RenderPipeline,
    slab_edges: wgpu::RenderPipeline,
    bead: wgpu::RenderPipeline,
    dot: wgpu::RenderPipeline,
    ghost: wgpu::RenderPipeline,
    glass: wgpu::RenderPipeline,
    glass_edges: wgpu::RenderPipeline,
    travel: wgpu::RenderPipeline,
    nozzle: wgpu::RenderPipeline,
}

pub struct Renderer {
    color_format: wgpu::TextureFormat,
    depth_format: wgpu::TextureFormat,
    sample_count: u32,
    cpu_adapter: bool,
    pipelines: Pipelines,

    globals_buf: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,

    slide_bgl: wgpu::BindGroupLayout,
    static_stride: u64,
    dyn_stride: u64,
    slide_cap: usize,
    static_buf: wgpu::Buffer,
    dyn_buf: wgpu::Buffer,
    slide_bg: wgpu::BindGroup,

    tpl_vbuf: wgpu::Buffer,
    tpl_ibuf: wgpu::Buffer,
    tpl_bead: Template,
    tpl_bead_start: Template,
    tpl_ribbon: Template,
    tpl_ribbon_start: Template,
    tpl_ribbon_full: Template,
    tpl_dome: Template,
    tpl_disc: Template,

    bed: Bed,
    buffers: HashMap<u32, GpuPaths>,
    slides: Vec<Slide>,
    travels: Option<(wgpu::Buffer, u32)>,
    targets: Option<Targets>,
}

fn align_up(v: u64, a: u64) -> u64 {
    v.div_ceil(a) * a
}

fn shader(device: &wgpu::Device, label: &str, parts: &[&str]) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(parts.join("\n").into()),
    })
}

/// Nastavení jedné pipeline (vše ostatní je společné).
struct PipeDesc<'a> {
    label: &'a str,
    layout: &'a wgpu::PipelineLayout,
    module: &'a wgpu::ShaderModule,
    vs: &'a str,
    fs: &'a str,
    buffers: &'a [Option<wgpu::VertexBufferLayout<'a>>],
    topology: wgpu::PrimitiveTopology,
    cull: Option<wgpu::Face>,
    depth_write: bool,
    depth_compare: wgpu::CompareFunction,
    blend: Option<wgpu::BlendState>,
    bias: wgpu::DepthBiasState,
}

impl Renderer {
    /// Renderer s hloubkou Depth32Float (pro jiný formát `new_with_depth`, viz `pick_formats`).
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        sample_count: u32,
        cpu_adapter: bool,
    ) -> Self {
        Self::new_with_depth(
            device,
            queue,
            color_format,
            sample_count,
            cpu_adapter,
            wgpu::TextureFormat::Depth32Float,
        )
    }

    pub fn new_with_depth(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        sample_count: u32,
        cpu_adapter: bool,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        let sample_count = sample_count.max(1);
        let uniform_entry = |binding: u32, dynamic: bool, size: usize| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic,
                min_binding_size: wgpu::BufferSize::new(size as u64),
            },
            count: None,
        };
        let globals_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dpi globals"),
            entries: &[uniform_entry(0, false, size_of::<Globals>())],
        });
        let slide_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dpi slide"),
            entries: &[
                uniform_entry(0, true, size_of::<SlideStatic>()),
                uniform_entry(1, true, size_of::<SlideDyn>()),
            ],
        });
        let layout_g = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("dpi layout globals"),
            bind_group_layouts: &[Some(&globals_bgl)],
            immediate_size: 0,
        });
        let layout_s = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("dpi layout slide"),
            bind_group_layouts: &[Some(&globals_bgl), Some(&slide_bgl)],
            immediate_size: 0,
        });

        let globals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dpi globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dpi globals"),
            layout: &globals_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buf.as_entire_binding(),
            }],
        });

        let align = device.limits().min_uniform_buffer_offset_alignment.max(1) as u64;
        let static_stride = align_up(size_of::<SlideStatic>() as u64, align);
        let dyn_stride = align_up(size_of::<SlideDyn>() as u64, align);

        let m = meshes::build();
        let tpl_vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("dpi templates"),
            contents: bytemuck::cast_slice(&m.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mut idx = m.indices.clone();
        if idx.len() % 2 == 1 {
            idx.push(0); // zarovnání na 4 bajty
        }
        let tpl_ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("dpi template indices"),
            contents: bytemuck::cast_slice(&idx),
            usage: wgpu::BufferUsages::INDEX,
        });

        // ── Shadery ──
        let m_beads = shader(device, "dpi beads", &[GLOBALS_WGSL, SLIDE_WGSL, BEADS_WGSL]);
        let m_glass = shader(device, "dpi glass", &[GLOBALS_WGSL, SLIDE_WGSL, GLASS_WGSL]);
        let m_ground = shader(device, "dpi ground", &[GLOBALS_WGSL, GROUND_WGSL]);
        let m_overlay = shader(device, "dpi overlay", &[GLOBALS_WGSL, OVERLAY_WGSL]);

        // ── Vertex layouty ──
        let tpl_layout = wgpu::VertexBufferLayout {
            array_stride: 16,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x4],
        };
        let piece_layout = wgpu::VertexBufferLayout {
            array_stride: size_of::<Piece>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                1 => Float32x2, 2 => Float32x2, 3 => Uint32, 4 => Uint32,
                5 => Uint32, 6 => Uint32, 7 => Float32
            ],
        };
        let dot_layout = wgpu::VertexBufferLayout {
            array_stride: size_of::<Dot>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![1 => Float32x2, 2 => Uint32, 3 => Uint32],
        };
        let travel_layout = wgpu::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3],
        };

        let no_bias = wgpu::DepthBiasState::default();
        let alpha = Some(wgpu::BlendState::ALPHA_BLENDING);
        let tri = wgpu::PrimitiveTopology::TriangleList;
        let lines = wgpu::PrimitiveTopology::LineList;
        let back = Some(wgpu::Face::Back);
        use wgpu::CompareFunction::{Less, LessEqual};

        let mk = |d: PipeDesc| -> wgpu::RenderPipeline {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(d.label),
                layout: Some(d.layout),
                vertex: wgpu::VertexState {
                    module: d.module,
                    entry_point: Some(d.vs),
                    compilation_options: Default::default(),
                    buffers: d.buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology: d.topology,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: d.cull,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: depth_format,
                    depth_write_enabled: Some(d.depth_write),
                    depth_compare: Some(d.depth_compare),
                    stencil: Default::default(),
                    bias: d.bias,
                }),
                multisample: wgpu::MultisampleState {
                    count: sample_count,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                fragment: Some(wgpu::FragmentState {
                    module: d.module,
                    entry_point: Some(d.fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: color_format,
                        blend: d.blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };

        let tpl_piece = [Some(tpl_layout.clone()), Some(piece_layout)];
        let tpl_dot = [Some(tpl_layout), Some(dot_layout)];
        let travel_buffers = [Some(travel_layout)];

        let pipelines = Pipelines {
            ground: mk(PipeDesc {
                label: "dpi ground",
                layout: &layout_g,
                module: &m_ground,
                vs: "vs_ground",
                fs: "fs_ground",
                buffers: &[],
                topology: tri,
                cull: None,
                depth_write: true,
                depth_compare: Less,
                blend: None,
                bias: no_bias,
            }),
            slab_walls: mk(PipeDesc {
                label: "dpi bed walls",
                layout: &layout_g,
                module: &m_ground,
                vs: "vs_slab_walls",
                fs: "fs_slab_walls",
                buffers: &[],
                topology: tri,
                cull: back,
                depth_write: true,
                depth_compare: Less,
                blend: None,
                bias: no_bias,
            }),
            slab_edges: mk(PipeDesc {
                label: "dpi bed edges",
                layout: &layout_g,
                module: &m_ground,
                vs: "vs_slab_edges",
                fs: "fs_slab_edges",
                buffers: &[],
                topology: lines,
                cull: None,
                depth_write: false,
                depth_compare: LessEqual,
                blend: alpha,
                bias: no_bias,
            }),
            bead: mk(PipeDesc {
                label: "dpi beads",
                layout: &layout_s,
                module: &m_beads,
                vs: "vs_piece",
                fs: "fs_bead",
                buffers: &tpl_piece,
                topology: tri,
                cull: back,
                depth_write: true,
                depth_compare: Less,
                blend: None,
                bias: no_bias,
            }),
            dot: mk(PipeDesc {
                label: "dpi dots",
                layout: &layout_s,
                module: &m_beads,
                vs: "vs_dot",
                fs: "fs_bead",
                buffers: &tpl_dot,
                topology: tri,
                cull: back,
                depth_write: true,
                depth_compare: Less,
                blend: None,
                bias: no_bias,
            }),
            ghost: mk(PipeDesc {
                label: "dpi ghosts",
                layout: &layout_s,
                module: &m_beads,
                vs: "vs_ghost",
                fs: "fs_ghost",
                buffers: &tpl_piece,
                topology: tri,
                cull: None,
                depth_write: false,
                depth_compare: LessEqual,
                blend: alpha,
                bias: no_bias,
            }),
            glass: mk(PipeDesc {
                label: "dpi glass",
                layout: &layout_s,
                module: &m_glass,
                vs: "vs_box",
                fs: "fs_glass",
                buffers: &[],
                topology: tri,
                cull: back,
                depth_write: false,
                depth_compare: LessEqual,
                blend: alpha,
                // Sklo kousek „za“ spodek housenek (leží přesně v z = tloušťka)
                bias: wgpu::DepthBiasState {
                    constant: 4,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            glass_edges: mk(PipeDesc {
                label: "dpi glass edges",
                layout: &layout_s,
                module: &m_glass,
                vs: "vs_edges",
                fs: "fs_glass",
                buffers: &[],
                topology: lines,
                cull: None,
                depth_write: false,
                depth_compare: LessEqual,
                blend: alpha,
                bias: no_bias,
            }),
            travel: mk(PipeDesc {
                label: "dpi travels",
                layout: &layout_g,
                module: &m_overlay,
                vs: "vs_travel",
                fs: "fs_travel",
                buffers: &travel_buffers,
                topology: lines,
                cull: None,
                depth_write: false,
                depth_compare: LessEqual,
                blend: alpha,
                bias: no_bias,
            }),
            nozzle: mk(PipeDesc {
                label: "dpi nozzles",
                layout: &layout_g,
                module: &m_overlay,
                vs: "vs_nozzle",
                fs: "fs_nozzle",
                buffers: &[],
                topology: tri,
                cull: back,
                depth_write: false,
                depth_compare: LessEqual,
                blend: alpha,
                bias: no_bias,
            }),
        };

        let (static_buf, dyn_buf, slide_bg) =
            Self::slide_buffers(device, &slide_bgl, 1, static_stride, dyn_stride);

        Self {
            color_format,
            depth_format,
            sample_count,
            cpu_adapter,
            pipelines,
            globals_buf,
            globals_bg,
            slide_bgl,
            static_stride,
            dyn_stride,
            slide_cap: 1,
            static_buf,
            dyn_buf,
            slide_bg,
            tpl_vbuf,
            tpl_ibuf,
            tpl_bead: m.bead,
            tpl_bead_start: m.bead_start,
            tpl_ribbon: m.ribbon,
            tpl_ribbon_start: m.ribbon_start,
            tpl_ribbon_full: m.ribbon_full,
            tpl_dome: m.dome,
            tpl_disc: m.disc,
            bed: Bed {
                w: 0.0,
                h: 0.0,
                thickness: 0.0,
            },
            buffers: HashMap::new(),
            slides: Vec::new(),
            travels: None,
            targets: None,
        }
    }

    pub fn color_format(&self) -> wgpu::TextureFormat {
        self.color_format
    }

    pub fn sample_count(&self) -> u32 {
        self.sample_count
    }

    fn slide_buffers(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        cap: usize,
        static_stride: u64,
        dyn_stride: u64,
    ) -> (wgpu::Buffer, wgpu::Buffer, wgpu::BindGroup) {
        let usage = wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST;
        let static_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dpi slide static"),
            size: static_stride * cap as u64,
            usage,
            mapped_at_creation: false,
        });
        let dyn_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dpi slide frame"),
            size: dyn_stride * cap as u64,
            usage,
            mapped_at_creation: false,
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dpi slide"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &static_buf,
                        offset: 0,
                        size: wgpu::BufferSize::new(size_of::<SlideStatic>() as u64),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &dyn_buf,
                        offset: 0,
                        size: wgpu::BufferSize::new(size_of::<SlideDyn>() as u64),
                    }),
                },
            ],
        });
        (static_buf, dyn_buf, bg)
    }

    /// Aplikuje scénový paket (`protocol::parse_scene`). Při chybě zůstává
    /// předchozí stav (kromě bufferů uvolněných před chybou).
    pub fn apply_scene(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bytes: &[u8],
    ) -> Result<(), String> {
        let scene = protocol::parse_scene(bytes)?;
        let p = &scene.packet;

        // Nejdřív načíst všechny bloby (chyba = nic se nezmění)
        let mut added = Vec::with_capacity(p.buffers_add.len());
        for d in &p.buffers_add {
            let pieces: Vec<Piece> = scene.blob(d.pieces)?;
            let dots: Vec<Dot> = scene.blob(d.dots)?;
            added.push((d.id, d.n_segs, pieces, dots));
        }
        let travels: Option<Vec<[f32; 3]>> = match p.travels {
            Some(r) => Some(scene.blob(r)?),
            None => None,
        };

        for id in &p.buffers_free {
            // Drop uvolní GPU paměť, jakmile ji GPU přestane používat
            self.buffers.remove(id);
        }
        let make = |label: &str, data: &[u8]| {
            (!data.is_empty()).then(|| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: data,
                    usage: wgpu::BufferUsages::VERTEX,
                })
            })
        };
        for (id, n_segs, pieces, dots) in added {
            let starts: Vec<Piece> = pieces.iter().filter(|p| p.j == 0).copied().collect();
            let gp = GpuPaths {
                pieces: make("dpi pieces", bytemuck::cast_slice(&pieces)),
                n_pieces: pieces.len() as u32,
                starts: make("dpi path starts", bytemuck::cast_slice(&starts)),
                n_starts: starts.len() as u32,
                dots: make("dpi dots", bytemuck::cast_slice(&dots)),
                n_dots: dots.len() as u32,
                n_segs,
                seg_runs: seg_runs(&pieces),
            };
            self.buffers.insert(id, gp);
        }
        if let Some(mut bed) = p.bed {
            if !(bed.thickness.is_finite() && bed.thickness > 0.0) {
                bed.thickness = 0.0;
            }
            self.bed = bed;
        }
        if let Some(slides) = &p.slides {
            self.slides = slides.clone();
        }
        if let Some(t) = travels {
            let n = (t.len() / 2 * 2) as u32;
            self.travels =
                make("dpi travels", bytemuck::cast_slice(&t[..n as usize])).map(|b| (b, n));
        }

        // Statická data skel (n_segs závisí i na bufferech → vždy znovu)
        if self.slides.len() > self.slide_cap {
            let cap = self.slides.len().next_power_of_two();
            let (s, d, bg) = Self::slide_buffers(
                device,
                &self.slide_bgl,
                cap,
                self.static_stride,
                self.dyn_stride,
            );
            self.static_buf = s;
            self.dyn_buf = d;
            self.slide_bg = bg;
            self.slide_cap = cap;
        }
        if !self.slides.is_empty() {
            let mut data = vec![0u8; self.static_stride as usize * self.slides.len()];
            for (i, s) in self.slides.iter().enumerate() {
                let st = self.slide_static(s);
                let off = i * self.static_stride as usize;
                data[off..off + size_of::<SlideStatic>()].copy_from_slice(bytemuck::bytes_of(&st));
            }
            queue.write_buffer(&self.static_buf, 0, &data);
        }
        Ok(())
    }

    fn slide_static(&self, s: &Slide) -> SlideStatic {
        let mut st = SlideStatic::zeroed();
        let [x, y, w, h, t] = s.glass;
        st.glass = [x, y, w, h];
        let n_segs = s
            .paths
            .and_then(|id| self.buffers.get(&id))
            .map_or(1, |b| b.n_segs);
        let objs: &[ObjStyle] = if s.objects.is_empty() {
            &[DEFAULT_STYLE]
        } else {
            &s.objects
        };
        let n_obj = objs.len().min(MAX_OBJECTS);
        st.info = [t, n_obj as f32, n_segs.max(1) as f32, 0.0];
        st.clip = s.clip;
        st.fill = s.glass_fill;
        st.edge = s.glass_edge;
        for (i, o) in objs.iter().take(n_obj).enumerate() {
            st.objs[2 * i] = [o.w, o.h, 0.0, 0.0];
            st.objs[2 * i + 1] = [o.color[0], o.color[1], o.color[2], 0.0];
        }
        st
    }

    fn ensure_targets(&mut self, device: &wgpu::Device, size: (u32, u32)) {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let extent = wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        };
        let tex = |label: &str, format: wgpu::TextureFormat| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: extent,
                    mip_level_count: 1,
                    sample_count: self.sample_count,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let msaa = (self.sample_count > 1).then(|| tex("dpi msaa", self.color_format));
        let depth = tex("dpi depth", self.depth_format);
        self.targets = Some(Targets { size, msaa, depth });
    }

    /// Zakóduje a odešle celý snímek do `target_view` (jednovzorkový, velikost
    /// = `header.viewport`). Vrací velikost cíle.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame_bytes: &[u8],
        target_view: &wgpu::TextureView,
    ) -> Result<(u32, u32), String> {
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("dpi frame"),
        });
        let size = self.encode(device, queue, frame_bytes, target_view, &mut enc)?;
        queue.submit([enc.finish()]);
        Ok(size)
    }

    /// Jako `render`, ale jen zakóduje do `enc` (hostitel přidá např. kopii pro readback).
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame_bytes: &[u8],
        target_view: &wgpu::TextureView,
        enc: &mut wgpu::CommandEncoder,
    ) -> Result<(u32, u32), String> {
        let frame = protocol::parse_frame(frame_bytes)?;
        let h = frame.header;
        let size = h.target_size().ok_or("snímek: nulová velikost cíle")?;
        self.ensure_targets(device, size);

        let globals = self.globals(&h);
        queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));

        let n = frame.slides.len().min(self.slides.len());
        if n > 0 {
            let mut data = vec![0u8; self.dyn_stride as usize * n];
            for (i, sf) in frame.slides.iter().take(n).enumerate() {
                let d = slide_dyn(sf);
                let off = i * self.dyn_stride as usize;
                data[off..off + size_of::<SlideDyn>()].copy_from_slice(bytemuck::bytes_of(&d));
            }
            queue.write_buffer(&self.dyn_buf, 0, &data);
        }

        let targets = self.targets.as_ref().expect("cíle vytvořené výše");
        let (view, resolve, store) = match &targets.msaa {
            Some(msaa) => (msaa, Some(target_view), wgpu::StoreOp::Discard),
            None => (target_view, None, wgpu::StoreOp::Store),
        };
        let c = h.clear;
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("dpi scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: resolve,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: c[0] as f64,
                        g: c[1] as f64,
                        b: c[2] as f64,
                        a: c[3] as f64,
                    }),
                    store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &targets.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let pl = &self.pipelines;
        pass.set_bind_group(0, &self.globals_bg, &[]);

        // 1. Podlaha (z = −tloušťka) a vršek podložky (z = 0), pak její boky a hrany
        pass.set_pipeline(&pl.ground);
        pass.draw(0..6, 0..2);
        if self.bed.thickness > 0.0 && self.bed.w > 0.0 && self.bed.h > 0.0 {
            pass.set_pipeline(&pl.slab_walls);
            pass.draw(0..24, 0..1);
            pass.set_pipeline(&pl.slab_edges);
            pass.draw(0..24, 0..1);
        }

        let offsets = |i: usize| {
            [
                (i as u64 * self.static_stride) as u32,
                (i as u64 * self.dyn_stride) as u32,
            ]
        };
        let px_per_mm = h.viewport[2];
        let lod_px = h.params[2];
        let ribbon_for = |s: &Slide| {
            if self.cpu_adapter {
                return true;
            }
            let max_w = s.objects.iter().map(|o| o.w).fold(0.0f32, f32::max);
            max_w * px_per_mm < lod_px
        };
        pass.set_vertex_buffer(0, self.tpl_vbuf.slice(..));
        pass.set_index_buffer(self.tpl_ibuf.slice(..), wgpu::IndexFormat::Uint16);

        // Skla mimo zorné pole a skla bez vytištěných drah (režim 1) se nekreslí
        let in_view: Vec<bool> = self
            .slides
            .iter()
            .take(n)
            .map(|s| {
                let max_h = s.objects.iter().map(|o| o.h).fold(0.0f32, f32::max);
                slide_in_view(&h.view_proj, s, max_h * h.params[0].max(0.0))
            })
            .collect();
        let printed =
            |i: usize| in_view[i] && frame.slides[i].cursor[0] != crate::cursor::MODE_NONE;

        // 2. Housenky: tělo + koncová čepička všech úseků, počáteční čepička jen
        //    na začátcích drah (+ první tištěný úsek aktuální uzavřené dráhy)
        pass.set_pipeline(&pl.bead);
        for (i, s) in self.slides.iter().take(n).enumerate() {
            let Some(gp) = s.paths.and_then(|id| self.buffers.get(&id)) else {
                continue;
            };
            let Some(buf) = &gp.pieces else { continue };
            if !printed(i) {
                continue;
            }
            let (body, start) = if ribbon_for(s) {
                (self.tpl_ribbon, self.tpl_ribbon_start)
            } else {
                (self.tpl_bead, self.tpl_bead_start)
            };
            pass.set_bind_group(1, &self.slide_bg, &offsets(i));
            pass.set_vertex_buffer(1, buf.slice(..));
            pass.draw_indexed(body.range(), 0, 0..gp.n_pieces);
            if let Some(starts) = &gp.starts {
                pass.set_vertex_buffer(1, starts.slice(..));
                pass.draw_indexed(start.range(), 0, 0..gp.n_starts);
            }
            if let Some(k) = gp.cursor_first_piece(frame.slides[i].cursor) {
                // Jedna instance přes výřez bufferu (WebGL2 neumí first_instance)
                let stride = size_of::<Piece>() as u64;
                pass.set_vertex_buffer(1, buf.slice(k as u64 * stride..(k as u64 + 1) * stride));
                pass.draw_indexed(start.range(), 0, 0..1);
            }
        }
        // 3. Kapky
        pass.set_pipeline(&pl.dot);
        for (i, s) in self.slides.iter().take(n).enumerate() {
            let Some(gp) = s.paths.and_then(|id| self.buffers.get(&id)) else {
                continue;
            };
            let Some(buf) = &gp.dots else { continue };
            if !printed(i) {
                continue;
            }
            let tpl = if ribbon_for(s) {
                self.tpl_disc
            } else {
                self.tpl_dome
            };
            pass.set_bind_group(1, &self.slide_bg, &offsets(i));
            pass.set_vertex_buffer(1, buf.slice(..));
            pass.draw_indexed(tpl.range(), 0, 0..gp.n_dots);
        }

        // 4. Skla odzadu dopředu
        let mut order: Vec<(usize, f32)> = self
            .slides
            .iter()
            .take(n)
            .enumerate()
            .map(|(i, s)| {
                let [x, y, w, hh, t] = s.glass;
                let c = [x + 0.5 * w, y + 0.5 * hh, 0.5 * t];
                let d = [c[0] - h.eye[0], c[1] - h.eye[1], c[2] - h.eye[2]];
                let key = if h.eye[3] > 0.5 {
                    d[0] * h.view_dir[0] + d[1] * h.view_dir[1] + d[2] * h.view_dir[2]
                } else {
                    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
                };
                (i, key)
            })
            .collect();
        order.retain(|&(i, _)| in_view[i]);
        order.sort_by(|a, b| b.1.total_cmp(&a.1));
        pass.set_pipeline(&pl.glass);
        for &(i, _) in &order {
            pass.set_bind_group(1, &self.slide_bg, &offsets(i));
            pass.draw(0..36, 0..1);
        }
        pass.set_pipeline(&pl.glass_edges);
        for &(i, _) in &order {
            pass.set_bind_group(1, &self.slide_bg, &offsets(i));
            pass.draw(0..24, 0..1);
        }

        // 5. Duchové
        if h.flags[0] == 1 {
            pass.set_pipeline(&pl.ghost);
            for (i, s) in self.slides.iter().take(n).enumerate() {
                let Some(gp) = s.ghost.and_then(|id| self.buffers.get(&id)) else {
                    continue;
                };
                let Some(buf) = &gp.pieces else { continue };
                pass.set_bind_group(1, &self.slide_bg, &offsets(i));
                pass.set_vertex_buffer(1, buf.slice(..));
                pass.draw_indexed(self.tpl_ribbon_full.range(), 0, 0..gp.n_pieces);
            }
        }

        // 6. Přejezdy
        if h.flags[1] == 1 {
            if let Some((buf, count)) = &self.travels {
                pass.set_pipeline(&pl.travel);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..*count, 0..1);
            }
        }

        // 7. Trysky
        if h.nozzle_real[3] == 1.0 || h.nozzle_preview[3] == 1.0 {
            pass.set_pipeline(&pl.nozzle);
            pass.draw(0..NOZZLE_VERTICES, 0..2);
        }
        drop(pass);
        Ok(size)
    }

    fn globals(&self, h: &FrameHeader) -> Globals {
        let ext = 2.0 * self.bed.w.max(self.bed.h).max(1.0);
        let l = light_dir([h.view_dir[0], h.view_dir[1], h.view_dir[2]]);
        Globals {
            view_proj: h.view_proj,
            eye: h.eye,
            view_dir: h.view_dir,
            viewport: h.viewport,
            grid: h.grid,
            params: h.params,
            nozzle_real: h.nozzle_real,
            nozzle_preview: h.nozzle_preview,
            clear: h.clear,
            bed: [self.bed.w, self.bed.h, ext, self.bed.thickness],
            light: [l[0], l[1], l[2], 0.0],
            misc: [0.0; 4],
        }
    }
}

/// Konzervativní test: leží kvádr skla (+ ořez, + výška housenek) aspoň zčásti
/// v zorném poli? (Všech 8 rohů za stejnou rovinou clip prostoru → ne.)
fn slide_in_view(view_proj: &[f32; 16], s: &Slide, bead_h: f32) -> bool {
    let [x, y, w, h, t] = s.glass;
    let c = s.clip;
    let lo = [x.min(c[0]), y.min(c[1]), 0.0];
    let hi = [(x + w).max(c[2]), (y + h).max(c[3]), t + bead_h + 0.1];
    if !lo.iter().chain(hi.iter()).all(|v| v.is_finite()) {
        return true;
    }
    let m = view_proj;
    let mut out = [true; 6];
    for k in 0..8 {
        let p = [
            if k & 1 == 0 { lo[0] } else { hi[0] },
            if k & 2 == 0 { lo[1] } else { hi[1] },
            if k & 4 == 0 { lo[2] } else { hi[2] },
        ];
        let v = |r: usize| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r];
        let (cx, cy, cz, cw) = (v(0), v(1), v(2), v(3));
        let inside = [
            cx >= -cw,
            cx <= cw,
            cy >= -cw,
            cy <= cw,
            cz >= 0.0,
            cz <= cw,
        ];
        for (o, i) in out.iter_mut().zip(inside) {
            *o &= !i;
        }
    }
    !out.iter().any(|&o| o)
}

fn slide_dyn(sf: &SlideFrame) -> SlideDyn {
    let x = sf.xf;
    SlideDyn {
        xf0: [x[0], x[1], x[2], x[3]],
        xf1: [x[4], x[5], x[6], x[7]],
        cursor: sf.cursor,
        frac: sf.cursor_frac,
    }
}

fn normalize3(v: [f32; 3]) -> Option<[f32; 3]> {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (l > 1e-6 && l.is_finite()).then(|| [v[0] / l, v[1] / l, v[2] / l])
}

/// Směr KE světlu „přes rameno“ pozorovatele: proti směru pohledu, nahoru
/// (v rámu kamery) a trochu doleva.
pub(crate) fn light_dir(view_dir: [f32; 3]) -> [f32; 3] {
    let v = normalize3(view_dir).unwrap_or([0.0, 0.0, -1.0]);
    // right = v × Z
    let right = normalize3([v[1], -v[0], 0.0]).unwrap_or([1.0, 0.0, 0.0]);
    // up = right × v
    let up = [
        right[1] * v[2] - right[2] * v[1],
        right[2] * v[0] - right[0] * v[2],
        right[0] * v[1] - right[1] * v[0],
    ];
    let l = [
        -v[0] + 0.45 * up[0] - 0.35 * right[0],
        -v[1] + 0.45 * up[1] - 0.35 * right[1],
        -v[2] + 0.45 * up[2] - 0.35 * right[2],
    ];
    normalize3(l).unwrap_or([0.0, 0.0, 1.0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_layouts() {
        assert_eq!(size_of::<Globals>(), 64 + 11 * 16);
        assert_eq!(size_of::<SlideStatic>(), 8 * 16 + 2 * MAX_OBJECTS * 16);
        assert!(size_of::<SlideStatic>() <= 16 << 10);
        assert_eq!(size_of::<SlideDyn>(), 64);
    }

    #[test]
    fn cursor_first_piece_of_closed_loop() {
        let piece = |seg: u32, j: u32, n: u32| Piece {
            a: [0.0; 2],
            b: [1.0, 0.0],
            seg,
            j,
            n_pieces: n,
            obj: 0,
            cum_len: 0.0,
        };
        // dráha 0: 2 úseky, dráha 1: tečka (bez úseků), dráha 2: uzavřená smyčka 4 úseků
        let pieces = [
            piece(0, 0, 2),
            piece(0, 1, 2),
            piece(2, 0, 4),
            piece(2, 1, 4),
            piece(2, 2, 4),
            piece(2, 3, 4),
        ];
        let runs = seg_runs(&pieces);
        assert_eq!(runs, vec![(0, 2), (u32::MAX, 0), (2, 4)]);
        let gp = GpuPaths {
            pieces: None,
            n_pieces: 6,
            starts: None,
            n_starts: 0,
            dots: None,
            n_dots: 0,
            n_segs: 3,
            seg_runs: runs,
        };
        assert_eq!(gp.cursor_first_piece([2, 2, 3, 0]), Some(5));
        assert_eq!(gp.cursor_first_piece([2, 2, 6, 1]), Some(4)); // start mod n
        assert_eq!(gp.cursor_first_piece([2, 2, 0, 1]), None); // j == 0 kreslí buffer začátků
        assert_eq!(gp.cursor_first_piece([2, 2, 1, -1]), None); // tryska přejíždí
        assert_eq!(gp.cursor_first_piece([2, 1, 1, 0]), None); // tečka
        assert_eq!(gp.cursor_first_piece([2, 9, 1, 0]), None);
        assert_eq!(gp.cursor_first_piece([0, 2, 1, 0]), None);
    }

    #[test]
    fn slide_culling() {
        use crate::testkit::{orbit_view, top_view};
        let slide = Slide {
            glass: [10.0, 10.0, 75.0, 25.0, 1.0],
            glass_fill: [0.0; 4],
            glass_edge: [0.0; 4],
            paths: None,
            ghost: None,
            clip: [10.0, 10.0, 85.0, 35.0],
            objects: vec![],
        };
        assert!(slide_in_view(
            &top_view(100, 100, 0.0, 0.0, 120.0, 80.0).view_proj,
            &slide,
            0.5
        ));
        assert!(slide_in_view(
            &top_view(100, 100, 80.0, 30.0, 90.0, 40.0).view_proj,
            &slide,
            0.5
        ));
        assert!(!slide_in_view(
            &top_view(100, 100, 100.0, 50.0, 120.0, 80.0).view_proj,
            &slide,
            0.5
        ));
        // Kamera v y ≈ 100 hledí k +Y → sklo (y ≤ 35) je za ní
        assert!(!slide_in_view(
            &orbit_view(100, 100, [40.0, 120.0, 0.0], 20.0, 0.0, 10.0).view_proj,
            &slide,
            0.5
        ));
        assert!(slide_in_view(
            &orbit_view(100, 100, [40.0, 20.0, 0.0], 80.0, 0.0, 30.0).view_proj,
            &slide,
            0.5
        ));
    }

    #[test]
    fn light_over_shoulder() {
        // Shora: světlo převážně z +Z, mírně z +Y (nahoře v obraze) a −X (vlevo)
        let l = light_dir([0.0, 0.0, -1.0]);
        assert!(l[2] > 0.8 && l[1] > 0.0 && l[0] < 0.0);
        // Šikmo od jihu k severu
        let l = light_dir([0.0, 0.7, -0.7]);
        assert!(l[1] < 0.0 && l[2] > 0.0);
    }
}
