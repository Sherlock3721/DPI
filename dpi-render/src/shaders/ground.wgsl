// Podlaha v z = 0: deska (výplň), okolí (barva pozadí), mřížka a okraj desky.
// Předchází globals.wgsl. Bez vertex bufferů (vertex_index 0..6).

struct GroundOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world: vec2<f32>,
};

@vertex
fn vs_ground(@builtin(vertex_index) vi: u32) -> GroundOut {
    let e = g.bed.z;
    let lo = vec2<f32>(-e, -e);
    let hi = g.bed.xy + vec2<f32>(e, e);
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let c = corners[min(vi, 5u)];
    var o: GroundOut;
    o.world = mix(lo, hi, c);
    o.pos = clip_pos(vec3<f32>(o.world, 0.0));
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

@fragment
fn fs_ground(in: GroundOut) -> @location(0) vec4<f32> {
    let p = in.world;
    // Derivace vždy v uniformním toku
    let fw = max(fwidth(p), vec2<f32>(1e-6));
    let minor_step = max(g.grid.x, 1e-3);
    let major_step = max(g.grid.y, 1e-3);
    let cmin = p / minor_step;
    let cmaj = p / major_step;
    let fw_min = fwidth(cmin);
    let fw_maj = fwidth(cmaj);

    let size = g.bed.xy;
    let inside = p.x >= 0.0 && p.y >= 0.0 && p.x <= size.x && p.y <= size.y;
    // #1e293b uvnitř desky, mimo barva pozadí
    var col = select(g.clear.rgb, vec3<f32>(0.118, 0.161, 0.231), inside);

    if (g.grid.z > 0.5) {
        let a_min = select(0.0, grid_cover(cmin, fw_min, 0.25), g.grid.x > 0.0);
        let a_maj = select(0.0, grid_cover(cmaj, fw_maj, 0.5), g.grid.y > 0.0);
        col = mix(col, vec3<f32>(0.141, 0.192, 0.282), a_min); // #243148
        col = mix(col, vec3<f32>(0.200, 0.255, 0.333), a_maj); // #334155
    }

    // Okraj desky ~1,5 px (#64748b)
    let dx = min(abs(p.x), abs(p.x - size.x)) / fw.x;
    let dy = min(abs(p.y), abs(p.y - size.y)) / fw.y;
    let in_y = p.y >= -fw.y && p.y <= size.y + fw.y;
    let in_x = p.x >= -fw.x && p.x <= size.x + fw.x;
    let d = min(select(1e6, dx, in_y), select(1e6, dy, in_x));
    let a_border = clamp(0.75 + 0.5 - d, 0.0, 1.0);
    col = mix(col, vec3<f32>(0.392, 0.455, 0.545), a_border);
    return vec4<f32>(col, 1.0);
}
