// Společná data snímku (skupina 0) — zrcadlí `renderer::Globals`.
// Barvy jsou sRGB hodnoty a zapisují se beze změny (cíl Rgba8Unorm).

struct Globals {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    view_dir: vec4<f32>,
    // šířka, výška (px), px na mm, DPR
    viewport: vec4<f32>,
    // minor krok, major krok, zobrazit, rezerva
    grid: vec4<f32>,
    // převýšení Z, režim barvy, práh LOD (px), perioda čárkování duchů (mm)
    params: vec4<f32>,
    nozzle_real: vec4<f32>,
    nozzle_preview: vec4<f32>,
    clear: vec4<f32>,
    // šířka a výška desky, přesah podlahy, rezerva
    bed: vec4<f32>,
    // směr KE světlu (normalizovaný)
    light: vec4<f32>,
    misc: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;

// Poloha mimo clip prostor — skrytá instance zkolabuje sem (nulová plocha, ořízne se).
const HIDDEN: vec4<f32> = vec4<f32>(2.0, 2.0, 2.0, 1.0);

fn clip_pos(p: vec3<f32>) -> vec4<f32> {
    return g.view_proj * vec4<f32>(p, 1.0);
}

// Jednotkový směr od bodu k oku (ortho = proti směru pohledu).
fn view_vec(p: vec3<f32>) -> vec3<f32> {
    if (g.eye.w > 0.5) {
        return -g.view_dir.xyz;
    }
    let d = g.eye.xyz - p;
    let l = length(d);
    if (l < 1e-6) {
        return -g.view_dir.xyz;
    }
    return d / l;
}

// Osvětlení: ambient + Lambert (světlo přes rameno pozorovatele) + slabý odlesk a okraj.
// Vršek housenky při pohledu shora zůstane blízko základní barvy.
fn shade(base: vec3<f32>, n: vec3<f32>, p: vec3<f32>) -> vec3<f32> {
    let l = g.light.xyz;
    let v = view_vec(p);
    let diff = max(dot(n, l), 0.0);
    let hv = normalize(l + v);
    let spec = pow(max(dot(n, hv), 0.0), 48.0) * 0.10;
    let rim = pow(1.0 - clamp(dot(n, v), 0.0, 1.0), 3.0) * 0.10;
    return base * (0.42 + 0.62 * diff) + vec3<f32>(spec + rim);
}
