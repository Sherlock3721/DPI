// Data skla (skupina 1, dynamické offsety) — zrcadlí `renderer::SlideStatic`
// a `renderer::SlideDyn`. Logika kurzoru zrcadlí `cursor.rs` (testovaná verze).

struct SlideStatic {
    // x, y, šířka, výška skla (svět)
    glass: vec4<f32>,
    // tloušťka, počet objektů, počet drah (n_segs bufferu drah), rezerva
    info: vec4<f32>,
    // obdélník ořezu x0, y0, x1, y1
    clip: vec4<f32>,
    fill: vec4<f32>,
    edge: vec4<f32>,
    rsv0: vec4<f32>,
    rsv1: vec4<f32>,
    rsv2: vec4<f32>,
    // objekt i: [2i] = (šířka, výška, 0, 0), [2i + 1] = (r, g, b, 0); MAX_OBJ = 128
    objs: array<vec4<f32>, 256>,
};

struct SlideDyn {
    // tx, ty, cx, cy
    xf0: vec4<f32>,
    // sx, sy, cos θ, sin θ
    xf1: vec4<f32>,
    // režim, aktuální dráha, start_idx, tištěný úsek
    cursor: vec4<i32>,
    // frac, rezerva ×3
    frac: vec4<f32>,
};

@group(1) @binding(0) var<uniform> ss: SlideStatic;
@group(1) @binding(1) var<uniform> sd: SlideDyn;

// svět.xy = (tx + cx, ty + cy) + R(θ)·(S·(p − c))
fn to_world(p: vec2<f32>) -> vec2<f32> {
    let c = sd.xf0.zw;
    let q = sd.xf1.xy * (p - c);
    let cs = sd.xf1.z;
    let sn = sd.xf1.w;
    return sd.xf0.xy + c + vec2<f32>(cs * q.x - sn * q.y, sn * q.x + cs * q.y);
}

fn obj_slot(o: u32) -> u32 {
    let n = u32(max(ss.info.y, 1.0));
    return 2u * min(o, n - 1u);
}

// (šířka, výška) housenky objektu
fn obj_size(o: u32) -> vec2<f32> {
    return ss.objs[obj_slot(o)].xy;
}

fn obj_color(o: u32) -> vec3<f32> {
    return ss.objs[obj_slot(o) + 1u].xyz;
}

fn outside_clip(p: vec2<f32>) -> bool {
    return p.x < ss.clip.x || p.y < ss.clip.y || p.x > ss.clip.z || p.y > ss.clip.w;
}

// Barevná škála „pořadí tisku“: modrá → azurová → zelená → žlutá → oranžová.
fn order_ramp(t: f32) -> vec3<f32> {
    var stops = array<vec3<f32>, 5>(
        vec3<f32>(0.231, 0.510, 0.965),
        vec3<f32>(0.024, 0.714, 0.831),
        vec3<f32>(0.133, 0.773, 0.369),
        vec3<f32>(0.918, 0.702, 0.031),
        vec3<f32>(0.976, 0.451, 0.086),
    );
    let x = clamp(t, 0.0, 1.0) * 4.0;
    let i = min(u32(x), 3u);
    return mix(stops[i], stops[i + 1u], x - f32(i));
}

// Barva úseku: objekt, nebo škála podle (seg + k / n_pieces) / n_segs.
fn seg_color(o: u32, seg: u32, k: u32, n_pieces: u32) -> vec3<f32> {
    if (g.params.y > 0.5) {
        let t = (f32(seg) + f32(k) / f32(max(n_pieces, 1u))) / max(ss.info.z, 1.0);
        return order_ramp(t);
    }
    return obj_color(o);
}

struct PieceVis {
    visible: bool,
    end_frac: f32,
    k: u32,
    first: bool,
};

// Zrcadlo `cursor::piece_vis`.
fn piece_vis(seg: u32, j: u32, n_pieces: u32) -> PieceVis {
    var r = PieceVis(true, 1.0, j, j == 0u);
    let mode = sd.cursor.x;
    if (mode == 1) {
        r.visible = false;
        return r;
    }
    if (mode != 2) {
        return r;
    }
    let s = i32(seg);
    let cur = sd.cursor.y;
    if (s < cur) {
        return r;
    }
    if (s > cur) {
        r.visible = false;
        return r;
    }
    let cp = sd.cursor.w;
    if (cp < 0) {
        r.visible = false;
        return r;
    }
    let n = max(n_pieces, 1u);
    let start = u32(max(sd.cursor.z, 0)) % n;
    let k = (j % n + n - start) % n;
    r.k = k;
    r.first = r.first || k == 0u;
    let ki = i32(k);
    if (ki < cp) {
        return r;
    }
    if (ki == cp) {
        r.end_frac = clamp(sd.frac.x, 0.0, 1.0);
        return r;
    }
    r.visible = false;
    return r;
}

// Zrcadlo `cursor::dot_visible`.
fn dot_visible(seg: u32) -> bool {
    let mode = sd.cursor.x;
    if (mode == 1) {
        return false;
    }
    if (mode != 2) {
        return true;
    }
    let s = i32(seg);
    return s < sd.cursor.y || (s == sd.cursor.y && sd.cursor.w >= 0);
}
