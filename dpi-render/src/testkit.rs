//! Pomůcky pro testy a benchmark: kamera (stejné konvence jako TS strana —
//! sloupcové matice, clip z ∈ [0, 1]) a syntetické scény. Není součást API.

use crate::protocol::{
    encode_frame, encode_scene, BlobWriter, Dot, FrameHeader, Piece, SlideFrame,
};
use bytemuck::Zeroable;

pub type Mat4 = [f32; 16];

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    [a[0] / l, a[1] / l, a[2] / l]
}

/// a · b (sloupcově).
pub fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut m = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            m[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    m
}

/// Pravotočivý look-at.
pub fn look_at(eye: V3, target: V3, up: V3) -> Mat4 {
    let f = norm(sub(target, eye));
    let s = norm(cross(f, up));
    let u = cross(s, f);
    [
        s[0],
        u[0],
        -f[0],
        0.0, //
        s[1],
        u[1],
        -f[1],
        0.0, //
        s[2],
        u[2],
        -f[2],
        0.0, //
        -dot(s, eye),
        -dot(u, eye),
        dot(f, eye),
        1.0,
    ]
}

/// Perspektiva, z ∈ [0, 1].
pub fn perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (0.5 * fovy).tan();
    let mut m = [0.0; 16];
    m[0] = f / aspect;
    m[5] = f;
    m[10] = far / (near - far);
    m[11] = -1.0;
    m[14] = near * far / (near - far);
    m
}

/// Ortho, z ∈ [0, 1].
pub fn ortho(l: f32, r: f32, b: f32, t: f32, near: f32, far: f32) -> Mat4 {
    let mut m = [0.0; 16];
    m[0] = 2.0 / (r - l);
    m[5] = 2.0 / (t - b);
    m[10] = 1.0 / (near - far);
    m[12] = -(r + l) / (r - l);
    m[13] = -(t + b) / (t - b);
    m[14] = near / (near - far);
    m[15] = 1.0;
    m
}

/// Výchozí hlavička (barvy aplikace, mřížka 1/10 mm, LOD 2 px, čárky 1,5 mm).
pub fn base_header(w: u32, h: u32) -> FrameHeader {
    let mut hd = FrameHeader::zeroed();
    hd.viewport = [w as f32, h as f32, 1.0, 1.0];
    hd.grid = [1.0, 10.0, 1.0, 0.0];
    hd.params = [1.0, 0.0, 2.0, 1.5];
    hd.clear = [0.059, 0.090, 0.165, 1.0];
    hd.flags = [1, 1, 0, 0];
    hd
}

/// Ortho pohled shora na obdélník [x0, x1] × [y0, y1] (zachová poměr stran).
pub fn top_view(w: u32, h: u32, x0: f32, y0: f32, x1: f32, y1: f32) -> FrameHeader {
    let mut hd = base_header(w, h);
    let (cx, cy) = (0.5 * (x0 + x1), 0.5 * (y0 + y1));
    let aspect = w as f32 / h as f32;
    let mut hw = 0.5 * (x1 - x0);
    let mut hh = 0.5 * (y1 - y0);
    if hw / hh > aspect {
        hh = hw / aspect;
    } else {
        hw = hh * aspect;
    }
    let eye = [cx, cy, 500.0];
    let view = look_at(eye, [cx, cy, 0.0], [0.0, 1.0, 0.0]);
    let proj = ortho(-hw, hw, -hh, hh, 1.0, 1000.0);
    hd.view_proj = mat_mul(&proj, &view);
    hd.eye = [eye[0], eye[1], eye[2], 1.0];
    hd.view_dir = [0.0, 0.0, -1.0, 0.0];
    hd.viewport[2] = w as f32 / (2.0 * hw);
    hd
}

/// Perspektiva na `target` ze vzdálenosti `dist`, azimut (0 = od jihu, kladný
/// = od západu… po směru hodin shora) a elevace ve stupních.
pub fn orbit_view(w: u32, h: u32, target: V3, dist: f32, az_deg: f32, el_deg: f32) -> FrameHeader {
    let mut hd = base_header(w, h);
    let (az, el) = (az_deg.to_radians(), el_deg.to_radians());
    let dir = [-az.sin() * el.cos(), -az.cos() * el.cos(), el.sin()];
    let eye = [
        target[0] + dist * dir[0],
        target[1] + dist * dir[1],
        target[2] + dist * dir[2],
    ];
    let fovy = 40f32.to_radians();
    let view = look_at(eye, target, [0.0, 0.0, 1.0]);
    let proj = perspective(fovy, w as f32 / h as f32, 1.0, 2000.0);
    hd.view_proj = mat_mul(&proj, &view);
    let vd = norm(sub(target, eye));
    hd.eye = [eye[0], eye[1], eye[2], 0.0];
    hd.view_dir = [vd[0], vd[1], vd[2], 0.0];
    hd.viewport[2] = h as f32 / (2.0 * dist * (0.5 * fovy).tan());
    hd
}

/// Transformace skla jako `SlideFrame::xf` (θ ve stupních).
pub fn xf(tx: f32, ty: f32, c: [f32; 2], s: [f32; 2], theta_deg: f32) -> [f32; 8] {
    let t = theta_deg.to_radians();
    [tx, ty, c[0], c[1], s[0], s[1], t.cos(), t.sin()]
}

/// svět.xy = (tx + cx, ty + cy) + R·(S·(p − c))
pub fn xf_point(x: &[f32; 8], p: [f32; 2]) -> [f32; 2] {
    let q = [x[4] * (p[0] - x[2]), x[5] * (p[1] - x[3])];
    [
        x[0] + x[2] + x[6] * q[0] - x[7] * q[1],
        x[1] + x[3] + x[7] * q[0] + x[6] * q[1],
    ]
}

/// Skládání drah do `Piece`/`Dot` (jedna dráha = jeden `seg`).
#[derive(Default)]
pub struct PathsBuilder {
    pub pieces: Vec<Piece>,
    pub dots: Vec<Dot>,
    pub n_segs: u32,
}

impl PathsBuilder {
    pub fn path(&mut self, pts: &[[f32; 2]], obj: u32) -> &mut Self {
        let seg = self.n_segs;
        self.n_segs += 1;
        let n = pts.len().saturating_sub(1) as u32;
        let mut cum = 0.0;
        for (j, w) in pts.windows(2).enumerate() {
            self.pieces.push(Piece {
                a: w[0],
                b: w[1],
                seg,
                j: j as u32,
                n_pieces: n,
                obj,
                cum_len: cum,
            });
            cum += ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt();
        }
        self
    }

    pub fn dot(&mut self, p: [f32; 2], obj: u32) -> &mut Self {
        self.dots.push(Dot {
            p,
            seg: self.n_segs,
            obj,
        });
        self.n_segs += 1;
        self
    }
}

pub fn circle(c: [f32; 2], r: f32, n: usize) -> Vec<[f32; 2]> {
    (0..=n)
        .map(|i| {
            let a = std::f32::consts::TAU * (i % n) as f32 / n as f32;
            [c[0] + r * a.cos(), c[1] + r * a.sin()]
        })
        .collect()
}

pub struct TestScene {
    pub scene: Vec<u8>,
    pub slides: Vec<SlideFrame>,
    pub bed: [f32; 2],
    /// Poloha náhledové trysky (svět, špička na vršku housenky).
    pub preview_nozzle: [f32; 3],
}

impl TestScene {
    pub fn frame(&self, header: &FrameHeader) -> Vec<u8> {
        encode_frame(header, &self.slides)
    }
}

/// Malá scéna pro golden testy: deska 120×80, dvě skla (1 a 2 mm), lomená
/// čára, uzavřené smyčky, tečky, duch mimo sklo, částečně vytištěné sklo B.
pub fn golden_scene() -> TestScene {
    let mut blobs = BlobWriter::default();

    // Sklo A (75×25×1) — vše vytištěné
    let mut a = PathsBuilder::default();
    a.path(
        &[
            [3.0, 4.0],
            [9.0, 21.0],
            [15.0, 4.0],
            [21.0, 21.0],
            [27.0, 4.0],
        ],
        0,
    )
    .path(
        &[
            [32.0, 4.0],
            [50.0, 4.0],
            [50.0, 21.0],
            [32.0, 21.0],
            [32.0, 4.0],
        ],
        1,
    )
    .path(&circle([62.0, 12.5], 7.0, 24), 0)
    .dot([41.0, 12.5], 0)
    .dot([71.0, 22.0], 1)
    .path(&[[55.0, 1.5], [75.0, 1.5]], 1);
    let mut a_ghost = PathsBuilder::default();
    a_ghost
        .path(&[[55.0, 1.5], [88.0, 1.5]], 1)
        .path(&[[70.0, 10.0], [84.0, 30.0]], 0);

    // Sklo B (60×28×2), otočené o 15° a zmenšené na 0,9 — rozpracované
    let mut b = PathsBuilder::default();
    let mut serp = Vec::new();
    for (i, y) in [3.0, 6.0, 9.0, 12.0].iter().enumerate() {
        if i % 2 == 0 {
            serp.extend([[6.0, *y], [54.0, *y]]);
        } else {
            serp.extend([[54.0, *y], [6.0, *y]]);
        }
    }
    b.path(&serp, 0)
        .path(&[[20.0, 15.0], [40.0, 15.0], [30.0, 25.0], [20.0, 15.0]], 1)
        .dot([47.0, 20.0], 1)
        .path(&[[44.0, 24.0], [50.0, 26.0], [56.0, 24.0]], 0);

    let pa = blobs.push(&a.pieces);
    let da = blobs.push(&a.dots);
    let pg = blobs.push(&a_ghost.pieces);
    let pb = blobs.push(&b.pieces);
    let db = blobs.push(&b.dots);
    let travels: Vec<[f32; 3]> = vec![
        [35.0, 29.0, 2.0],
        [35.0, 29.0, 5.0],
        [35.0, 29.0, 5.0],
        [40.0, 52.0, 5.0],
        [40.0, 52.0, 5.0],
        [40.0, 52.0, 3.0],
    ];
    let tr = blobs.push(&travels);
    let empty = r#"{"offset":0,"len":0}"#;
    let blob =
        |r: crate::protocol::BlobRef| format!(r#"{{"offset":{},"len":{}}}"#, r.offset, r.len);

    let xf_b = xf(10.0, 45.0, [30.0, 14.0], [0.9, 0.9], 15.0);
    let json = format!(
        r#"{{
        "bed": {{"w": 120, "h": 80}},
        "buffers_add": [
            {{"id": 1, "pieces": {pa}, "dots": {da}, "n_segs": {na}}},
            {{"id": 2, "pieces": {pg}, "dots": {empty}, "n_segs": {ng}}},
            {{"id": 3, "pieces": {pb}, "dots": {db}, "n_segs": {nb}}}
        ],
        "slides": [
            {{"glass": [8, 8, 75, 25, 1.0], "glass_fill": [0.55, 0.75, 0.88, 0.22],
              "glass_edge": [0.70, 0.85, 0.95, 0.85], "paths": 1, "ghost": 2,
              "clip": [8, 8, 83, 33],
              "objects": [{{"w": 1.0, "h": 0.5, "color": [0.376, 0.647, 0.980]}},
                          {{"w": 0.7, "h": 0.35, "color": [0.961, 0.620, 0.043]}}]}},
            {{"glass": [10, 45, 60, 28, 2.0], "glass_fill": [0.55, 0.75, 0.88, 0.22],
              "glass_edge": [0.70, 0.85, 0.95, 0.85], "paths": 3, "ghost": null,
              "clip": [10, 45, 70, 73],
              "objects": [{{"w": 1.2, "h": 0.6, "color": [0.204, 0.827, 0.600]}},
                          {{"w": 0.8, "h": 0.4, "color": [0.957, 0.447, 0.714]}}]}}
        ],
        "travels": {tr}
    }}"#,
        pa = blob(pa),
        da = blob(da),
        pg = blob(pg),
        pb = blob(pb),
        db = blob(db),
        tr = blob(tr),
        na = a.n_segs,
        ng = a_ghost.n_segs,
        nb = b.n_segs,
    );
    let scene = encode_scene(&json, &blobs.data);

    let slides = vec![
        SlideFrame {
            xf: xf(8.0, 8.0, [37.5, 12.5], [1.0, 1.0], 0.0),
            cursor: [0, 0, 0, 0],
            cursor_frac: [0.0; 4],
        },
        // Trojúhelník (dráha 1) tištěný od bodu 1: hotový úsek j=1, j=2 z půlky, j=0 ještě ne
        SlideFrame {
            xf: xf_b,
            cursor: [2, 1, 1, 1],
            cursor_frac: [0.5, 0.0, 0.0, 0.0],
        },
    ];
    // Tryska na konci rozpracovaného úseku j=2: mix((30, 25), (20, 15), 0.5), vršek housenky
    let p = xf_point(&xf_b, [25.0, 20.0]);
    TestScene {
        scene,
        slides,
        bed: [120.0, 80.0],
        preview_nozzle: [p[0], p[1], 2.0 + 0.4],
    }
}

/// Velká syntetická scéna pro benchmark: `n_slides` skel s hadovitými
/// drahami z krátkých úseků (~`pieces_per_slide`) a tečkami.
pub fn bench_scene(n_slides: usize, pieces_per_slide: usize, dots_per_slide: usize) -> TestScene {
    let mut blobs = BlobWriter::default();
    let cols = 4usize;
    let rows = n_slides.div_ceil(cols);
    let (gw, gh, gap) = (75.0f32, 25.0f32, 5.0f32);
    let bed = [
        cols as f32 * (gw + gap) + gap,
        rows as f32 * (gh + gap) + gap,
    ];
    let mut buffers = Vec::new();
    let mut slides_json = Vec::new();
    let mut slides = Vec::new();
    for s in 0..n_slides {
        let mut pb = PathsBuilder::default();
        // Hadovité řádky s jemným vlněním, úseky ~0,25 mm
        let lines = 40usize;
        let per_line = (pieces_per_slide / lines).max(1);
        for l in 0..lines {
            let y = 1.0 + (gh - 2.0) * l as f32 / (lines - 1) as f32;
            let pts: Vec<[f32; 2]> = (0..=per_line)
                .map(|i| {
                    let t = i as f32 / per_line as f32;
                    let x = if l % 2 == 0 {
                        1.0 + t * (gw - 2.0)
                    } else {
                        gw - 1.0 - t * (gw - 2.0)
                    };
                    [x, y + 0.15 * (x * 0.7).sin()]
                })
                .collect();
            pb.path(&pts, (l % 2) as u32);
        }
        for d in 0..dots_per_slide {
            let x = 1.0 + (gw - 2.0) * ((d * 37) % 997) as f32 / 997.0;
            let y = 1.0 + (gh - 2.0) * ((d * 91) % 991) as f32 / 991.0;
            pb.dot([x, y], 1);
        }
        let p = blobs.push(&pb.pieces);
        let d = blobs.push(&pb.dots);
        buffers.push(format!(
            r#"{{"id": {}, "pieces": {{"offset": {}, "len": {}}}, "dots": {{"offset": {}, "len": {}}}, "n_segs": {}}}"#,
            s + 1,
            p.offset,
            p.len,
            d.offset,
            d.len,
            pb.n_segs
        ));
        let gx = gap + (s % cols) as f32 * (gw + gap);
        let gy = gap + (s / cols) as f32 * (gh + gap);
        slides_json.push(format!(
            r#"{{"glass": [{gx}, {gy}, {gw}, {gh}, 1.0], "glass_fill": [0.55, 0.75, 0.88, 0.22],
               "glass_edge": [0.7, 0.85, 0.95, 0.85], "paths": {}, "ghost": null,
               "clip": [{gx}, {gy}, {}, {}],
               "objects": [{{"w": 0.4, "h": 0.2, "color": [0.376, 0.647, 0.98]}},
                           {{"w": 0.4, "h": 0.2, "color": [0.961, 0.62, 0.043]}}]}}"#,
            s + 1,
            gx + gw,
            gy + gh
        ));
        slides.push(SlideFrame {
            xf: xf(gx, gy, [0.0, 0.0], [1.0, 1.0], 0.0),
            cursor: [0, 0, 0, 0],
            cursor_frac: [0.0; 4],
        });
    }
    let json = format!(
        r#"{{"bed": {{"w": {}, "h": {}}}, "buffers_add": [{}], "slides": [{}]}}"#,
        bed[0],
        bed[1],
        buffers.join(","),
        slides_json.join(",")
    );
    TestScene {
        scene: encode_scene(&json, &blobs.data),
        slides,
        bed,
        preview_nozzle: [0.0; 3],
    }
}
