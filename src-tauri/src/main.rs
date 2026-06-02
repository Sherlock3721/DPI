#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Snížení priority PipeWire backendu pro WebKit GStreamer a preferování v4l2.
    // Řeší chybový spam: handle_format_change: assertion 'gst_caps_is_fixed (pwsrc->caps)' failed
    // set_var je deprecated od Rust 1.81 (není thread-safe), ale zde je voláno před
    // spuštěním jakéhokoli vlákna — použití je tedy bezpečné.
    #[allow(deprecated)]
    std::env::set_var("GST_PLUGIN_FEATURE_RANK", "v4l2src:MAX,pipewiresrc:0");

    dpi_app::run();
}
