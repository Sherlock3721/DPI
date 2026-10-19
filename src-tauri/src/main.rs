#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Snížení priority PipeWire backendu pro WebKit GStreamer a preferování v4l2.
    // Řeší chybový spam: handle_format_change: assertion 'gst_caps_is_fixed (pwsrc->caps)' failed
    // set_var je deprecated od Rust 1.81 (není thread-safe), ale zde je voláno před
    // spuštěním jakéhokoli vlákna — použití je tedy bezpečné.
    #[allow(deprecated)]
    std::env::set_var("GST_PLUGIN_FEATURE_RANK", "v4l2src:MAX,pipewiresrc:0");

    #[cfg(gst_videorate_fix)]
    install_gst_videorate_fix();

    dpi_app::run();
}

/// Obejití zamrzání náhledu kamery ve WebKitGTK s GStreamerem >= 1.28
/// (viz src-tauri/gst/dpivideoratefix.c): tracer vložený při buildu se zapíše
/// do cache a zaregistruje přes GST_PLUGIN_PATH + GST_TRACERS — proměnné
/// dědí procesy WebKitu, které kameru snímají.
#[cfg(gst_videorate_fix)]
fn install_gst_videorate_fix() {
    use std::path::PathBuf;

    const PLUGIN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/libgstdpivideoratefix.so"));

    let Some(cache) = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
    else {
        return;
    };
    let dir = cache.join("droplet-printing-interface").join("gst-plugins");
    let file = dir.join("libgstdpivideoratefix.so");
    if std::fs::read(&file).ok().as_deref() != Some(PLUGIN) {
        let written = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&file, PLUGIN));
        if let Err(e) = written {
            eprintln!("Obejití zamrzání kamery nelze nainstalovat ({}): {e}", file.display());
            return;
        }
    }

    let join = |var: &str, value: &str, sep: &str| match std::env::var(var) {
        Ok(cur) if !cur.is_empty() => format!("{value}{sep}{cur}"),
        _ => value.to_string(),
    };
    let plugin_path = join("GST_PLUGIN_PATH", &dir.to_string_lossy(), ":");
    let tracers = join("GST_TRACERS", "dpivideoratefix", ";");
    // Stejně jako výše: voláno před spuštěním jakéhokoli vlákna
    #[allow(deprecated)]
    {
        std::env::set_var("GST_PLUGIN_PATH", plugin_path);
        std::env::set_var("GST_TRACERS", tracers);
    }
}
