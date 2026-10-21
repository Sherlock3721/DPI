// Skla: průhledný kvádr [x, y, 0]–[x + w, y + h, tloušťka] a jeho hrany.
// Předchází globals.wgsl + slide.wgsl. Bez vertex bufferů (vertex_index).

struct GlassOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

// Roh jednotkové krychle z bitů: x = bit 0, y = bit 1, z = bit 2.
fn box_corner(code: u32, z_bottom: f32) -> vec3<f32> {
    let g0 = ss.glass;
    let x = g0.x + g0.z * f32(code & 1u);
    let y = g0.y + g0.w * f32((code >> 1u) & 1u);
    let z = select(z_bottom, ss.info.x, ((code >> 2u) & 1u) == 1u);
    return vec3<f32>(x, y, z);
}

@vertex
fn vs_box(@builtin(vertex_index) vi: u32) -> GlassOut {
    // 6 stěn × 4 rohy (CCW zvenku): +Z, −Z, +X, −X, +Y, −Y
    var faces = array<u32, 24>(
        4u, 5u, 7u, 6u,
        0u, 2u, 3u, 1u,
        1u, 3u, 7u, 5u,
        0u, 4u, 6u, 2u,
        2u, 6u, 7u, 3u,
        0u, 1u, 5u, 4u,
    );
    var normals = array<vec3<f32>, 6>(
        vec3<f32>(0.0, 0.0, 1.0),
        vec3<f32>(0.0, 0.0, -1.0),
        vec3<f32>(1.0, 0.0, 0.0),
        vec3<f32>(-1.0, 0.0, 0.0),
        vec3<f32>(0.0, 1.0, 0.0),
        vec3<f32>(0.0, -1.0, 0.0),
    );
    var tri = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
    let face = min(vi / 6u, 5u);
    let corner = faces[face * 4u + tri[vi % 6u]];
    var o: GlassOut;
    o.pos = clip_pos(box_corner(corner, 0.0));
    // Mírné stínování stěn, ať je tloušťka skla čitelná
    let n = normals[face];
    let k = 0.8 + 0.2 * max(dot(n, g.light.xyz), 0.0);
    o.color = vec4<f32>(ss.fill.rgb * k, ss.fill.a);
    return o;
}

@vertex
fn vs_edges(@builtin(vertex_index) vi: u32) -> GlassOut {
    // 12 hran: spodní čtverec, horní čtverec, svislé hrany
    var edges = array<u32, 24>(
        0u, 1u, 1u, 3u, 3u, 2u, 2u, 0u,
        4u, 5u, 5u, 7u, 7u, 6u, 6u, 4u,
        0u, 4u, 1u, 5u, 2u, 6u, 3u, 7u,
    );
    var o: GlassOut;
    // Spodní hrany kousek nad podlahou a všechny hrany mírně k oku v clip prostoru
    // (depth bias na čáry nepůsobí; bez toho softwarové rasterizéry čárkují spodní
    // hranu o podlahu)
    var p = clip_pos(box_corner(edges[min(vi, 23u)], 0.01));
    p.z = max(p.z - 4e-6 * p.w, 0.0);
    o.pos = p;
    o.color = ss.edge;
    return o;
}

@fragment
fn fs_glass(in: GlassOut) -> @location(0) vec4<f32> {
    return in.color;
}
