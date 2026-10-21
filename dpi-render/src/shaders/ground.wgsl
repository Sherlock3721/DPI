// Podložka a podlaha. Předchází globals.wgsl. Bez vertex bufferů (vertex_index).
//
// Podložka je kvádr [0, w] × [0, h] × [−t, 0] (t = g.bed.w): vršek v z = 0
// (výplň, mřížka, okraj), čtyři stínované boční stěny a hrany obou obvodů.
// Podlaha okolo (barva pozadí + mřížka) leží v z = −t; uvnitř půdorysu desky se
// zahazuje (je pod deskou), takže při t = 0 nic nekoliduje a vzhled je jako dřív.

struct GroundOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world: vec3<f32>,
    // 0 = podlaha, 1 = vršek desky
    @location(1) @interpolate(flat) top: u32,
};

// Instance 0 = podlaha (přesah g.bed.z na každou stranu, z = −t), 1 = vršek desky (z = 0).
@vertex
fn vs_ground(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> GroundOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let c = corners[min(vi, 5u)];
    var lo = vec2<f32>(0.0, 0.0);
    var hi = g.bed.xy;
    var z = 0.0;
    if (ii == 0u) {
        let e = g.bed.z;
        lo = vec2<f32>(-e, -e);
        hi = g.bed.xy + vec2<f32>(e, e);
        z = -max(g.bed.w, 0.0);
    }
    var o: GroundOut;
    o.world = vec3<f32>(mix(lo, hi, c), z);
    o.pos = clip_pos(o.world);
    o.top = min(ii, 1u);
    return o;
}

// Pokrytí čar mřížky s krokem `step`: `fw` = fwidth(p / step), poloviční šířka v px.
// Útlum, když jsou čáry v obraze hustší než ~10 px (perspektiva, šikmý pohled) — bez moaré.
fn grid_cover(c: vec2<f32>, fw: vec2<f32>, half_px: f32) -> f32 {
    let f = max(fw, vec2<f32>(1e-6));
    let d = abs(fract(c + 0.5) - 0.5) / f;
    let cover = clamp(vec2<f32>(half_px + 0.5) - d, vec2<f32>(0.0), vec2<f32>(1.0));
    let fade = smoothstep(vec2<f32>(3.0), vec2<f32>(10.0), 1.0 / f);
    let a = cover * fade;
    return max(a.x, a.y);
}

// Kosinus úhlu mezi pohledem a normálou roviny (Z): 0 = rovina z boku (hranou).
fn plane_facing(p: vec3<f32>) -> f32 {
    return abs(view_vec(p).z);
}

@fragment
fn fs_ground(in: GroundOut) -> @location(0) vec4<f32> {
    let p = in.world.xy;
    // Derivace vždy v uniformním toku (před discard)
    let fw = max(fwidth(p), vec2<f32>(1e-6));
    let minor_step = max(g.grid.x, 1e-3);
    let major_step = max(g.grid.y, 1e-3);
    let cmin = p / minor_step;
    let cmaj = p / major_step;
    let fw_min = fwidth(cmin);
    let fw_maj = fwidth(cmaj);

    let size = g.bed.xy;
    let inside = p.x >= 0.0 && p.y >= 0.0 && p.x <= size.x && p.y <= size.y;
    if (in.top == 0u && p.x > 0.0 && p.y > 0.0 && p.x < size.x && p.y < size.y) {
        // Podlaha pod deskou není vidět
        discard;
    }
    // #1e293b uvnitř desky, mimo barva pozadí
    var col = select(g.clear.rgb, vec3<f32>(0.118, 0.161, 0.231), inside);

    // Rovina téměř z boku: mřížku a okraj ztlumit (derivace tam nedávají smysl)
    let facing = smoothstep(0.02, 0.12, plane_facing(in.world));
    if (g.grid.z > 0.5) {
        let a_min = select(0.0, grid_cover(cmin, fw_min, 0.25), g.grid.x > 0.0) * facing;
        let a_maj = select(0.0, grid_cover(cmaj, fw_maj, 0.5), g.grid.y > 0.0) * facing;
        col = mix(col, vec3<f32>(0.141, 0.192, 0.282), a_min); // #243148
        col = mix(col, vec3<f32>(0.200, 0.255, 0.333), a_maj); // #334155
    }

    // Okraj desky ~1,5 px (#64748b) — vnější polovinu kreslí podlaha, vnitřní vršek
    let dx = min(abs(p.x), abs(p.x - size.x)) / fw.x;
    let dy = min(abs(p.y), abs(p.y - size.y)) / fw.y;
    let in_y = p.y >= -fw.y && p.y <= size.y + fw.y;
    let in_x = p.x >= -fw.x && p.x <= size.x + fw.x;
    let d = min(select(1e6, dx, in_y), select(1e6, dy, in_x));
    let a_border = clamp(0.75 + 0.5 - d, 0.0, 1.0) * facing;
    col = mix(col, vec3<f32>(0.392, 0.455, 0.545), a_border);
    return vec4<f32>(col, 1.0);
}

// ── Boční stěny a hrany desky ──

struct SlabOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

// Roh kvádru desky z bitů: x = bit 0, y = bit 1, z = bit 2 (0 → −t, 1 → 0).
fn slab_corner(code: u32) -> vec3<f32> {
    let x = g.bed.x * f32(code & 1u);
    let y = g.bed.y * f32((code >> 1u) & 1u);
    let z = select(-max(g.bed.w, 0.0), 0.0, ((code >> 2u) & 1u) == 1u);
    return vec3<f32>(x, y, z);
}

@vertex
fn vs_slab_walls(@builtin(vertex_index) vi: u32) -> SlabOut {
    // 4 stěny × 4 rohy (CCW zvenku): +X, −X, +Y, −Y
    var faces = array<u32, 16>(
        1u, 3u, 7u, 5u,
        0u, 4u, 6u, 2u,
        2u, 6u, 7u, 3u,
        0u, 1u, 5u, 4u,
    );
    var normals = array<vec3<f32>, 4>(
        vec3<f32>(1.0, 0.0, 0.0),
        vec3<f32>(-1.0, 0.0, 0.0),
        vec3<f32>(0.0, 1.0, 0.0),
        vec3<f32>(0.0, -1.0, 0.0),
    );
    var tri = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
    let face = min(vi / 6u, 3u);
    var o: SlabOut;
    o.world = slab_corner(faces[face * 4u + tri[vi % 6u]]);
    o.pos = clip_pos(o.world);
    o.normal = normals[face];
    return o;
}

@fragment
fn fs_slab_walls(in: SlabOut) -> @location(0) vec4<f32> {
    // O něco světlejší než výplň vršku, ať se stěna od vršku i podlahy odliší
    let base = vec3<f32>(0.176, 0.224, 0.302);
    return vec4<f32>(shade(base, normalize(in.normal), in.world), 1.0);
}

struct EdgeOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_slab_edges(@builtin(vertex_index) vi: u32) -> EdgeOut {
    // 12 hran: spodní obvod, horní obvod, svislé hrany v rozích
    var edges = array<u32, 24>(
        0u, 1u, 1u, 3u, 3u, 2u, 2u, 0u,
        4u, 5u, 5u, 7u, 7u, 6u, 6u, 4u,
        0u, 4u, 1u, 5u, 2u, 6u, 3u, 7u,
    );
    let w = slab_corner(edges[min(vi, 23u)]);
    var p = clip_pos(w);
    // Mírně k oku (depth bias na čáry nepůsobí) — jinak čárkování o vršek/podlahu
    p.z = max(p.z - 4e-6 * p.w, 0.0);
    var o: EdgeOut;
    o.pos = p;
    // Při pohledu shora boky nejsou vidět a okraj kreslí vršek/podlaha → hrany ztlumit,
    // ať pohled shora vypadá stejně jako s plochou deskou
    let a = 1.0 - smoothstep(0.9, 0.99, abs(view_vec(w).z));
    // #64748b
    o.color = vec4<f32>(0.392, 0.455, 0.545, a);
    return o;
}

@fragment
fn fs_slab_edges(in: EdgeOut) -> @location(0) vec4<f32> {
    return in.color;
}
