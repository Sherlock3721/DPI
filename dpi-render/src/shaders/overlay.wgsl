// Přejezdy trysky (LineList) a kužely trysek. Předchází globals.wgsl.

@vertex
fn vs_travel(@location(0) p: vec3<f32>) -> @builtin(position) vec4<f32> {
    return clip_pos(p);
}

@fragment
fn fs_travel() -> @location(0) vec4<f32> {
    // #94a3b8
    return vec4<f32>(0.580, 0.639, 0.722, 0.55);
}

const NOZ_SEGS: u32 = 24u;
const NOZ_HEIGHT: f32 = 6.0;
const NOZ_RADIUS: f32 = 1.5;

struct NozzleOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

// Kužel se špičkou v xyz trysky, otevřený nahoru. Instance 0 = reálná, 1 = náhledová.
// Vrcholy: NOZ_SEGS trojúhelníků pláště + NOZ_SEGS trojúhelníků víka.
@vertex
fn vs_nozzle(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> NozzleOut {
    var o: NozzleOut;
    var nz = g.nozzle_real;
    // #ef4444
    var base = vec3<f32>(0.937, 0.267, 0.267);
    if (ii == 1u) {
        nz = g.nozzle_preview;
        // #22d3ee
        base = vec3<f32>(0.133, 0.827, 0.933);
    }
    if (nz.w < 0.5) {
        o.pos = HIDDEN;
        o.color = vec4<f32>(0.0);
        return o;
    }
    let tri = vi / 3u;
    let corner = vi % 3u;
    let seg = tri % NOZ_SEGS;
    let top = tri >= NOZ_SEGS;
    let a0 = 6.2831853 * f32(seg) / f32(NOZ_SEGS);
    let a1 = 6.2831853 * f32(seg + 1u) / f32(NOZ_SEGS);
    let tip = nz.xyz;
    let r0 = tip + vec3<f32>(NOZ_RADIUS * cos(a0), NOZ_RADIUS * sin(a0), NOZ_HEIGHT);
    let r1 = tip + vec3<f32>(NOZ_RADIUS * cos(a1), NOZ_RADIUS * sin(a1), NOZ_HEIGHT);
    var p: vec3<f32>;
    var n: vec3<f32>;
    if (top) {
        // Víko: (střed, r0, r1) — CCW shora
        let c = tip + vec3<f32>(0.0, 0.0, NOZ_HEIGHT);
        p = select(select(r1, r0, corner == 1u), c, corner == 0u);
        n = vec3<f32>(0.0, 0.0, 1.0);
    } else {
        // Plášť: (špička, r1, r0) — normála ven a dolů
        p = select(select(r0, r1, corner == 1u), tip, corner == 0u);
        let am = 0.5 * (a0 + a1);
        n = normalize(vec3<f32>(NOZ_HEIGHT * cos(am), NOZ_HEIGHT * sin(am), -NOZ_RADIUS));
    }
    o.pos = clip_pos(p);
    let k = 0.6 + 0.5 * max(dot(n, g.light.xyz), 0.0);
    o.color = vec4<f32>(base * k, 0.6);
    return o;
}

@fragment
fn fs_nozzle(in: NozzleOut) -> @location(0) vec4<f32> {
    return in.color;
}
