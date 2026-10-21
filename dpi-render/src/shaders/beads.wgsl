// Housenky (instance `Piece`), kapky (instance `Dot`) a duchové.
// Předchází globals.wgsl + slide.wgsl.

struct PieceIn {
    // šablona: ldir, lperp, lup, druh (viz meshes.rs)
    @location(0) tpl: vec4<f32>,
    @location(1) a: vec2<f32>,
    @location(2) b: vec2<f32>,
    @location(3) seg: u32,
    @location(4) j: u32,
    @location(5) n_pieces: u32,
    @location(6) obj: u32,
    @location(7) cum_len: f32,
};

struct DotIn {
    // šablona: x, y, z na polokouli, plochý (0/1)
    @location(0) tpl: vec4<f32>,
    @location(1) p: vec2<f32>,
    @location(2) seg: u32,
    @location(3) obj: u32,
};

struct BeadOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
};

struct GhostOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world: vec3<f32>,
    // vzdálenost od začátku dráhy (mm) pro čárkování
    @location(1) dash: f32,
};

fn hidden_bead() -> BeadOut {
    var o: BeadOut;
    o.pos = HIDDEN;
    o.world = vec3<f32>(0.0);
    o.normal = vec3<f32>(0.0, 0.0, 1.0);
    o.color = vec3<f32>(0.0);
    return o;
}

// Rám úseku ve světě: počátek a, konec b (zkrácený o frac), jednotkový směr.
struct PieceFrame {
    wa: vec2<f32>,
    wb: vec2<f32>,
    dir: vec2<f32>,
    len: f32,
};

fn piece_frame(a: vec2<f32>, b: vec2<f32>, end_frac: f32) -> PieceFrame {
    var f: PieceFrame;
    f.wa = to_world(a);
    let wb_full = to_world(b);
    let d = wb_full - f.wa;
    f.len = length(d);
    // Nulová délka: libovolný směr, žádné NaN
    f.dir = vec2<f32>(1.0, 0.0);
    if (f.len > 1e-6) {
        f.dir = d / f.len;
    }
    f.wb = mix(f.wa, wb_full, end_frac);
    return f;
}

@vertex
fn vs_piece(v: PieceIn) -> BeadOut {
    let vis = piece_vis(v.seg, v.j, v.n_pieces);
    if (!vis.visible) {
        return hidden_bead();
    }
    let f = piece_frame(v.a, v.b, vis.end_frac);
    let perp = vec2<f32>(-f.dir.y, f.dir.x);
    let size = obj_size(v.obj);
    let r = max(0.5 * size.x, 1e-4);
    let h = max(size.y * g.params.x, 1e-4);
    let z0 = ss.info.x;

    let kind = u32(v.tpl.w + 0.5);
    let place = kind & 3u;
    var l = v.tpl.xyz;
    // Počáteční čepička jen u prvního tištěného úseku (jinak by se na spojích
    // zdvojovala s koncovou čepičkou předchozího úseku)
    if (place == 3u && !vis.first) {
        l = vec3<f32>(0.0);
    }
    var base = f.wa;
    if (place == 1u || place == 2u) {
        base = f.wb;
    }
    let xy = base + (f.dir * l.x + perp * l.y) * r;
    var z = z0 + l.z * h;
    // Normála elipsoidu (r, r, h)
    var n = vec3<f32>(f.dir * (l.x / r) + perp * (l.y / r), l.z / h);
    if ((kind & 4u) != 0u) {
        // Plochý pásek na vršku housenky
        z = z0 + h * 1.02;
        n = vec3<f32>(0.0, 0.0, 1.0);
    }
    if (dot(n, n) < 1e-12) {
        n = vec3<f32>(0.0, 0.0, 1.0);
    }

    var o: BeadOut;
    o.world = vec3<f32>(xy, z);
    o.pos = clip_pos(o.world);
    o.normal = n;
    o.color = seg_color(v.obj, v.seg, vis.k, v.n_pieces);
    return o;
}

@vertex
fn vs_dot(v: DotIn) -> BeadOut {
    if (!dot_visible(v.seg)) {
        return hidden_bead();
    }
    let c = to_world(v.p);
    let size = obj_size(v.obj);
    let r = max(0.5 * size.x, 1e-4);
    let h = max(size.y * g.params.x, 1e-4);
    let z0 = ss.info.x;
    var z = z0 + v.tpl.z * h;
    var n = vec3<f32>(v.tpl.xy / r, v.tpl.z / h);
    if (v.tpl.w > 0.5) {
        z = z0 + h * 1.02;
        n = vec3<f32>(0.0, 0.0, 1.0);
    }
    var o: BeadOut;
    o.world = vec3<f32>(c + v.tpl.xy * r, z);
    o.pos = clip_pos(o.world);
    o.normal = n;
    o.color = seg_color(v.obj, v.seg, 0u, 1u);
    return o;
}

@fragment
fn fs_bead(in: BeadOut) -> @location(0) vec4<f32> {
    // Ořez i u už oříznutých drah (čepičky přesahují hranu skla)
    if (outside_clip(in.world.xy)) {
        discard;
    }
    let n = normalize(in.normal);
    return vec4<f32>(shade(in.color, n, in.world), 1.0);
}

// Duchové: tenký plochý pásek (0,5·w) nad úrovní vršku housenek, jen MIMO ořez, čárkovaně.
@vertex
fn vs_ghost(v: PieceIn) -> GhostOut {
    var o: GhostOut;
    let f = piece_frame(v.a, v.b, 1.0);
    let perp = vec2<f32>(-f.dir.y, f.dir.x);
    let size = obj_size(v.obj);
    let r = max(0.25 * size.x, 1e-4);
    let h = max(size.y * g.params.x, 0.0);
    let kind = u32(v.tpl.w + 0.5);
    let place = kind & 3u;
    var l = v.tpl.xy;
    // Čepičky jen na koncích dráhy (překryvy průhledných čepiček na spojích by tmavly)
    if ((place == 3u && v.j != 0u) || (place == 2u && v.j + 1u < v.n_pieces)) {
        l = vec2<f32>(0.0);
    }
    var base = f.wa;
    if (place == 1u || place == 2u) {
        base = f.wb;
    }
    let xy = base + (f.dir * l.x + perp * l.y) * r;
    o.world = vec3<f32>(xy, ss.info.x + h + 0.01);
    o.pos = clip_pos(o.world);
    // cum_len je v lokálních jednotkách → přepočet měřítkem úseku
    let len_local = length(v.b - v.a);
    var ratio = 0.5 * (abs(sd.xf1.x) + abs(sd.xf1.y));
    if (len_local > 1e-6) {
        ratio = f.len / len_local;
    }
    o.dash = v.cum_len * ratio + dot(xy - f.wa, f.dir);
    return o;
}

@fragment
fn fs_ghost(in: GhostOut) -> @location(0) vec4<f32> {
    if (!outside_clip(in.world.xy)) {
        discard;
    }
    let period = max(g.params.w, 1e-3);
    if (fract(in.dash / period) > 0.5) {
        discard;
    }
    // #f87171
    return vec4<f32>(0.973, 0.443, 0.443, 0.35);
}
