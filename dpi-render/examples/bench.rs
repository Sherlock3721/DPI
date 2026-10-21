//! Benchmark offscreen snímku (render + readback) 1280×800 nad syntetickou
//! scénou ~200k úseků + 20k teček.
//!
//! `cargo run --release -p dpi-render --example bench`
//! (adaptér lze vybrat `WGPU_ADAPTER_NAME=radv`, backend `WGPU_BACKEND=gl`).

use std::time::Instant;

use dpi_render::offscreen::Offscreen;
use dpi_render::testkit::{bench_scene, orbit_view, top_view};

fn main() {
    let mut off = Offscreen::new(false).expect("žádný GPU adaptér");
    println!("adaptér: {:?}", off.adapter());
    let scene = bench_scene(20, 10_000, 1_000);
    let t = Instant::now();
    off.apply_scene(&scene.scene).unwrap();
    println!(
        "apply_scene ({:.1} MB): {:.1} ms",
        scene.scene.len() as f64 / 1e6,
        t.elapsed().as_secs_f64() * 1e3
    );

    let (w, h) = (1280, 800);
    let [bw, bh] = scene.bed;
    let mut cases = Vec::new();
    let mut top = top_view(w, h, -5.0, -5.0, bw + 5.0, bh + 5.0);
    top.params[2] = 0.0;
    cases.push(("shora, celá deska, 3D housenky", top));
    let mut top_lod = top;
    top_lod.params[2] = 1e6;
    cases.push(("shora, celá deska, ploché pásky (LOD)", top_lod));
    let mut persp = orbit_view(w, h, [bw * 0.5, bh * 0.5, 0.0], 260.0, -25.0, 40.0);
    persp.params[2] = 0.0;
    cases.push(("perspektiva, 3D housenky", persp));
    let mut close = orbit_view(w, h, [40.0, 20.0, 1.0], 30.0, -25.0, 40.0);
    close.params[2] = 0.0;
    cases.push(("perspektiva, detail skla, 3D", close));

    for (name, header) in cases {
        let frame = scene.frame(&header);
        for _ in 0..5 {
            off.render_frame(&frame).unwrap();
        }
        let mut times: Vec<f64> = (0..40)
            .map(|_| {
                let t = Instant::now();
                let px = off.render_frame(&frame).unwrap();
                assert_eq!(px.len(), (w * h * 4) as usize);
                t.elapsed().as_secs_f64() * 1e3
            })
            .collect();
        times.sort_by(f64::total_cmp);
        let avg = times.iter().sum::<f64>() / times.len() as f64;
        println!(
            "{name}: průměr {avg:.2} ms, medián {:.2} ms, min {:.2} ms, p95 {:.2} ms",
            times[times.len() / 2],
            times[0],
            times[times.len() * 95 / 100]
        );
    }
}
