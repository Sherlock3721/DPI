//! Golden testy rendereru: malá syntetická scéna, offscreen bez MSAA
//! (determinismus napříč GPU), porovnání s PNG v tests/golden/ s tolerancí.
//!
//! `DPI_RENDER_BLESS=1 cargo test -p dpi-render --test golden` přepíše reference.
//! Vyrenderované obrázky se vždy ukládají do `target/dpi-render-test/`
//! (včetně větších náhledů `*_big.png`, které se neporovnávají).

use std::path::{Path, PathBuf};

use dpi_render::offscreen::{Offscreen, OffscreenOptions};
use dpi_render::protocol::FrameHeader;
use dpi_render::testkit::{golden_scene, orbit_view, top_view, TestScene};

/// Max. rozdíl kanálu, který se ještě nepočítá jako odlišný pixel.
const CHANNEL_TOL: u8 = 8;
/// Max. podíl odlišných pixelů.
const PIXEL_TOL: f64 = 0.01;

fn out_dir() -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../target"));
    let dir = target.join("dpi-render-test");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) {
    let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut enc = png::Encoder::new(file, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(rgba).unwrap();
}

fn read_png(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).ok()?));
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    buf.truncate(info.buffer_size());
    Some((info.width, info.height, buf))
}

/// Pohledy testu: (název, hlavička). `big` = náhled pro oči, neporovnává se.
fn views(s: &TestScene, w: u32, h: u32) -> Vec<(&'static str, FrameHeader)> {
    // Neceločíselné posuny: hrany skel (1px čáry) nesmí ležet přesně na hranici pixelů,
    // jinak je rasterizace čar závislá na implementaci
    let mut top = top_view(w, h, -4.13, -4.07, s.bed[0] + 3.87, s.bed[1] + 3.93);
    top.nozzle_real = [62.0, 30.0, 5.0, 1.0];
    let mut oblique = orbit_view(w, h, [60.0, 40.0, 0.0], 150.0, -30.0, 38.0);
    oblique.nozzle_real = [62.0, 30.0, 5.0, 1.0];
    oblique.nozzle_preview = [
        s.preview_nozzle[0],
        s.preview_nozzle[1],
        s.preview_nozzle[2],
        1.0,
    ];
    // Detail rozpracovaného skla B: barvy podle pořadí tisku, větší převýšení
    let mut close = orbit_view(w, h, [40.0, 60.0, 2.0], 52.0, 20.0, 42.0);
    close.params = [2.0, 1.0, 2.0, 1.5];
    // převýšení 2 → vršek housenky (h = 0,4) o 0,4 mm výš
    close.nozzle_preview = [
        s.preview_nozzle[0],
        s.preview_nozzle[1],
        s.preview_nozzle[2] + 0.4,
        1.0,
    ];
    // Ortho shora na sklo A: kontrola vzhledu „2D plánu“ a pásků (LOD) proti 3D
    let mut top_a = top_view(w, h, 6.11, 6.07, 86.11, 36.07);
    top_a.grid = [1.0, 10.0, 1.0, 0.0];
    vec![
        ("top", top),
        ("oblique", oblique),
        ("closeup", close),
        ("top_slide_a", top_a),
    ]
}

#[test]
fn golden_images() {
    let opts = OffscreenOptions {
        force_fallback: false,
        msaa: false,
        cpu_ribbons: false,
    };
    let mut off = match Offscreen::with_options(opts) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("golden: přeskočeno — žádný adaptér ({e})");
            return;
        }
    };
    eprintln!("golden: adaptér {:?}", off.adapter());
    let scene = golden_scene();
    off.apply_scene(&scene.scene).unwrap();

    let bless = std::env::var("DPI_RENDER_BLESS").is_ok_and(|v| v == "1");
    let out = out_dir();
    let mut failures = Vec::new();
    let (w, h) = (320u32, 200u32);
    for (name, header) in views(&scene, w, h) {
        let rgba = off.render_frame(&scene.frame(&header)).unwrap();
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        write_png(&out.join(format!("{name}.png")), w, h, &rgba);
        let golden = golden_dir().join(format!("{name}.png"));
        if bless {
            std::fs::create_dir_all(golden_dir()).unwrap();
            write_png(&golden, w, h, &rgba);
            continue;
        }
        let Some((gw, gh, gold)) = read_png(&golden) else {
            failures.push(format!(
                "{name}: chybí reference {} (spusťte s DPI_RENDER_BLESS=1)",
                golden.display()
            ));
            continue;
        };
        if (gw, gh) != (w, h) {
            failures.push(format!("{name}: velikost {gw}×{gh} ≠ {w}×{h}"));
            continue;
        }
        let mut diff_px = 0usize;
        let mut max_diff = 0u8;
        let mut diff_img = vec![0u8; rgba.len()];
        for (i, (a, b)) in rgba.chunks_exact(4).zip(gold.chunks_exact(4)).enumerate() {
            let d = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap();
            max_diff = max_diff.max(d);
            if d > CHANNEL_TOL {
                diff_px += 1;
                diff_img[i * 4..i * 4 + 4].copy_from_slice(&[255, 0, 255, 255]);
            } else {
                diff_img[i * 4..i * 4 + 4].copy_from_slice(&[a[0] / 3, a[1] / 3, a[2] / 3, 255]);
            }
        }
        let ratio = diff_px as f64 / (w * h) as f64;
        eprintln!(
            "golden {name}: odlišných pixelů {diff_px} ({:.3} %), max rozdíl {max_diff}",
            ratio * 100.0
        );
        if ratio > PIXEL_TOL {
            write_png(&out.join(format!("{name}_diff.png")), w, h, &diff_img);
            failures.push(format!(
                "{name}: {:.2} % odlišných pixelů (limit {:.0} %)",
                ratio * 100.0,
                PIXEL_TOL * 100.0
            ));
        }
    }

    // Větší náhledy s MSAA podle adaptéru (jen pro vizuální kontrolu)
    if let Ok(mut big) = Offscreen::new(false) {
        big.apply_scene(&scene.scene).unwrap();
        let (bw, bh) = (960u32, 600u32);
        for (name, header) in views(&scene, bw, bh) {
            let rgba = big.render_frame(&scene.frame(&header)).unwrap();
            write_png(&out.join(format!("{name}_big.png")), bw, bh, &rgba);
        }
    }
    eprintln!("golden: obrázky v {}", out.display());
    assert!(failures.is_empty(), "golden: {}", failures.join("; "));
}

#[test]
fn readback_is_top_down_and_tight() {
    // Nepárná šířka → bytes_per_row s paddingem; horní polovina obrazu = +Y světa
    let Ok(mut off) = Offscreen::with_options(OffscreenOptions {
        msaa: false,
        ..Default::default()
    }) else {
        eprintln!("readback: přeskočeno — žádný adaptér");
        return;
    };
    let scene = golden_scene();
    off.apply_scene(&scene.scene).unwrap();
    let (w, h) = (101u32, 67u32);
    // Pohled shora jen na horní okraj desky: dole v obraze deska, nahoře okolí
    let mut header = top_view(w, h, 0.0, 70.0, 40.0, 90.0);
    header.grid[2] = 0.0;
    let rgba = off.render_frame(&scene.frame(&header)).unwrap();
    assert_eq!(rgba.len(), (w * h * 4) as usize);
    let px = |x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2]]
    };
    let clear = [15u8, 23, 42]; // header.clear (0.059, 0.090, 0.165)
    let near = |a: [u8; 3], b: [u8; 3]| a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 2);
    assert!(
        near(px(w / 2, 2), clear),
        "horní řádek má být mimo desku: {:?}",
        px(w / 2, 2)
    );
    assert!(
        !near(px(w / 2, h - 3), clear),
        "dolní řádek má být deska: {:?}",
        px(w / 2, h - 3)
    );
    // Opakovaný snímek jiné velikosti (znovuvytvoření cílů) a zpět
    let header2 = top_view(64, 64, 0.0, 0.0, 120.0, 80.0);
    assert_eq!(
        off.render_frame(&scene.frame(&header2)).unwrap().len(),
        64 * 64 * 4
    );
    assert_eq!(off.render_frame(&scene.frame(&header)).unwrap(), rgba);
}

#[test]
fn bad_packets_are_errors_not_panics() {
    let Ok(mut off) = Offscreen::with_options(OffscreenOptions {
        msaa: false,
        ..Default::default()
    }) else {
        return;
    };
    assert!(off.apply_scene(b"nope").is_err());
    assert!(off.render_frame(&[0u8; 10]).is_err());
    // Nulová velikost
    let mut header = top_view(10, 10, 0.0, 0.0, 1.0, 1.0);
    header.viewport[0] = 0.0;
    assert!(off
        .render_frame(&dpi_render::protocol::encode_frame(&header, &[]))
        .is_err());
    // Snímek bez scény (žádná skla) i s více skly, než má scéna, projde
    let header = top_view(16, 16, 0.0, 0.0, 1.0, 1.0);
    assert!(off
        .render_frame(&dpi_render::protocol::encode_frame(&header, &[]))
        .is_ok());
    let scene = golden_scene();
    off.apply_scene(&scene.scene).unwrap();
    let mut slides = scene.slides.clone();
    slides.push(slides[0]);
    assert!(off
        .render_frame(&dpi_render::protocol::encode_frame(&header, &slides))
        .is_ok());
    // Uvolnění bufferu, na který sklo stále odkazuje → sklo se kreslí bez drah
    let free = dpi_render::protocol::encode_scene(r#"{"buffers_free": [1, 3]}"#, &[]);
    off.apply_scene(&free).unwrap();
    assert!(off.render_frame(&scene.frame(&header)).is_ok());
}
