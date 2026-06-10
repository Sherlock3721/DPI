pub mod comms;
pub mod sync;

use comms::{
    auto_find_printer, spawn_comms_loop, try_connect_port, PrinterCommand, PrinterManager,
    PrinterStatus,
};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

#[cfg(target_os = "linux")]
use webkit2gtk::{
    glib::Cast, PermissionRequestExt, UserMediaPermissionRequest, WebViewExt,
};

// ─── Datové typy ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct GCodeResponse {
    gcode: String,
    total_dist: f64,
    total_time: f64,
}

// ─── G-kód příkazy ───────────────────────────────────────────────────────────

#[tauri::command]
fn generate_gcode_job(
    slide_paths: Vec<dpi_core::SubstratePaths>,
    params: dpi_core::ProcessParams,
    transforms: Vec<dpi_core::Transform>,
    slide_overrides: std::collections::HashMap<String, dpi_core::SlideOverride>,
    machine: dpi_core::MachineConfig,
) -> Result<GCodeResponse, String> {
    let (gcode, total_dist, total_time) = dpi_core::generate_gcode(
        &slide_paths,
        &params,
        &transforms,
        &slide_overrides,
        &machine,
    )?;
    Ok(GCodeResponse {
        gcode,
        total_dist,
        total_time,
    })
}

// ─── Tiskové příkazy (COMMS) ──────────────────────────────────────────────────

#[tauri::command]
fn get_ports() -> Vec<String> {
    match serialport::available_ports() {
        Ok(ports) => ports.into_iter().map(|p| p.port_name).collect(),
        Err(_) => Vec::new(),
    }
}

/// Sdílená logika odpojení — volána ze sync i async kontextu.
fn do_disconnect(manager: &Arc<PrinterManager>, app_handle: &AppHandle) {
    let mut guard = manager.tx.lock().unwrap_or_else(|e| e.into_inner());
    *guard = None; // Přeruší kanál → ukončí komunikační smyčku na pozadí
    let mut status = manager.status.lock().unwrap_or_else(|e| e.into_inner());
    status.is_connected = false;
    status.is_printing = false;
    status.is_paused = false;
    app_handle.emit("printer-status-changed", status.clone()).ok();
}

/// Blokující část připojení — spouštěna ve spawn_blocking.
fn do_connect(
    port_name: String,
    baudrate: u32,
    manager: Arc<PrinterManager>,
    app_handle: AppHandle,
) -> Result<PrinterStatus, String> {
    do_disconnect(&manager, &app_handle);
    let port = try_connect_port(&port_name, baudrate)?;
    let tx = spawn_comms_loop(port_name, port, Arc::clone(&manager), app_handle)?;
    let mut guard = manager.tx.lock().unwrap_or_else(|e| e.into_inner());
    *guard = Some(tx);
    Ok(manager.status.lock().unwrap_or_else(|e| e.into_inner()).clone())
}

#[tauri::command]
async fn connect_printer(
    port_name: String,
    baudrate: u32,
    manager: State<'_, Arc<PrinterManager>>,
    app_handle: AppHandle,
) -> Result<PrinterStatus, String> {
    let manager = Arc::clone(manager.inner());
    tokio::task::spawn_blocking(move || do_connect(port_name, baudrate, manager, app_handle))
        .await
        .map_err(|e| e.to_string())?
}

/// Automaticky projde všechny dostupné sériové porty, ověří M115
/// a připojí první nalezené tiskárnu. Odpovídá Python `connect_printer(manual_port=None)`.
#[tauri::command]
async fn auto_connect_printer(
    baudrate: u32,
    manager: State<'_, Arc<PrinterManager>>,
    app_handle: AppHandle,
) -> Result<PrinterStatus, String> {
    let manager = Arc::clone(manager.inner());
    tokio::task::spawn_blocking(move || {
        do_disconnect(&manager, &app_handle);
        let (port_name, port) = auto_find_printer(baudrate)?;
        let tx = spawn_comms_loop(port_name, port, Arc::clone(&manager), app_handle)?;
        let mut guard = manager.tx.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(tx);
        Ok(manager.status.lock().unwrap_or_else(|e| e.into_inner()).clone())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn disconnect_printer(
    manager: State<'_, Arc<PrinterManager>>,
    app_handle: AppHandle,
) -> Result<(), String> {
    do_disconnect(manager.inner(), &app_handle);
    Ok(())
}

#[tauri::command]
fn start_print_job(
    gcode: String,
    total_dist: f64,
    total_time: f64,
    manager: State<'_, Arc<PrinterManager>>,
) -> Result<(), String> {
    manager.send(PrinterCommand::StartPrint {
        gcode,
        total_dist,
        total_time,
    })
}

#[tauri::command]
fn pause_print_job(manager: State<'_, Arc<PrinterManager>>) -> Result<(), String> {
    manager.send(PrinterCommand::Pause)
}

#[tauri::command]
fn resume_print_job(manager: State<'_, Arc<PrinterManager>>) -> Result<(), String> {
    manager.send(PrinterCommand::Resume)
}

#[tauri::command]
fn resume_app_pause_job(manager: State<'_, Arc<PrinterManager>>) -> Result<(), String> {
    manager.send(PrinterCommand::AppResume)
}

/// Odvodí bezpečnou parkovací pozici po nouzovém zastavení z uložené
/// konfigurace podložky. Při chybě čtení nastavení vrací konzervativní default.
fn read_park_position(app_handle: &AppHandle) -> (f64, f64) {
    const DEFAULT: (f64, f64) = (0.0, 200.0);
    let Ok(dir) = get_app_config_dir(app_handle) else {
        return DEFAULT;
    };
    let Ok(content) = std::fs::read_to_string(dir.join("settings.json")) else {
        return DEFAULT;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) else {
        return DEFAULT;
    };
    let min_x = v.get("bed_min_x").and_then(|x| x.as_f64()).unwrap_or(0.0);
    let max_y = v.get("bed_max_y").and_then(|x| x.as_f64()).unwrap_or(210.0);
    (min_x.max(0.0), (max_y - 10.0).max(0.0))
}

#[tauri::command]
fn stop_print_job(
    manager: State<'_, Arc<PrinterManager>>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let (park_x, park_y) = read_park_position(&app_handle);
    manager.send(PrinterCommand::Stop { park_x, park_y })
}

#[tauri::command]
fn send_manual_gcode(gcode: String, manager: State<'_, Arc<PrinterManager>>) -> Result<(), String> {
    manager.send(PrinterCommand::SendManual { gcode })
}

#[tauri::command]
async fn send_manual_gcode_blocking(
    gcode: String,
    manager: State<'_, Arc<PrinterManager>>,
) -> Result<(), String> {
    let (reply_tx, reply_rx) = std::sync::mpsc::channel::<Result<(), String>>();
    manager.send(PrinterCommand::SendManualBlocking { gcode, reply: reply_tx })?;
    // recv() blokuje OS vlákno — přesuneme do spawn_blocking aby JS Promise nezmrazilo UI
    tokio::task::spawn_blocking(move || {
        reply_rx.recv().map_err(|_| "Komunikační vlákno selhalo.".to_string())?
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Kompletní přepočet layoutu v jediném volání: kapacita podložky, zpracování
/// drah všech sklíček, pozice, přizpůsobení transformací a náhled odplivu.
/// Nahrazuje sekvenci calculate_layout + N× process_paths + recalculate_layout
/// + get_prime_preview ve frontendu.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn update_layout(
    params: dpi_core::ProcessParams,
    overrides: std::collections::HashMap<String, dpi_core::SlideOverride>,
    raw_paths: Option<dpi_core::SubstratePaths>,
    auto_scale: bool,
    baked_scales: Vec<f64>,
    old_positions: Vec<dpi_core::LayoutPosition>,
    current_transforms: Vec<dpi_core::Transform>,
    bed: dpi_core::BedConfig,
    multi_spacing: f64,
) -> dpi_core::LayoutUpdateResult {
    dpi_core::update_layout(
        &params,
        &overrides,
        raw_paths.as_ref(),
        auto_scale,
        &baked_scales,
        &old_positions,
        &current_transforms,
        &bed,
        multi_spacing,
    )
}

// ─── Metadata a export ───────────────────────────────────────────────────────

#[tauri::command]
fn build_gcode_metadata_header(meta: dpi_core::GCodeMetadata) -> String {
    dpi_core::serialize_metadata(&meta)
}

#[tauri::command]
fn parse_gcode_metadata(gcode_text: String) -> Option<dpi_core::GCodeMetadata> {
    dpi_core::deserialize_metadata(&gcode_text)
}

#[tauri::command]
fn parse_gcode_file_paths(gcode_text: String) -> dpi_core::SubstratePaths {
    dpi_core::parse_gcode_paths(&gcode_text)
}

#[tauri::command]
fn generate_csv_protocol(
    params: dpi_core::ProcessParams,
    overrides: std::collections::HashMap<String, dpi_core::SlideOverride>,
    total_dist: f64,
    total_time: f64,
    selected_glass: String,
    app_version: String,
    date_str: String,
) -> String {
    dpi_core::build_csv_protocol(
        &params,
        &overrides,
        total_dist,
        total_time,
        &selected_glass,
        &app_version,
        &date_str,
    )
}

// ─── Geometrie a preview ─────────────────────────────────────────────────────

/// Předpočítá kumulativní vzdálenosti segmentů pro náhled průběhu tisku.
/// Nahrazuje `recomputePreviewDist()` v `Canvas2D.svelte`.
#[tauri::command]
fn compute_preview_segments(
    positions: Vec<dpi_core::LayoutPosition>,
    paths: Vec<dpi_core::SubstratePaths>,
    transforms: Vec<dpi_core::Transform>,
    prime_path: Option<dpi_core::SubstratePaths>,
) -> dpi_core::PreviewDistResult {
    dpi_core::compute_preview_segments(&positions, &paths, &transforms, prime_path.as_ref())
}

/// Vrátí `true` pokud některá z transformovaných tras přesahuje okraj sklíčka s insetem trysky.
/// Nahrazuje smyčky v `handleNozzleDiamGrew` v `App.svelte`.
#[tauri::command]
fn check_paths_overflow(
    paths: Vec<dpi_core::SubstratePaths>,
    transforms: Vec<dpi_core::Transform>,
    non_prime_positions: Vec<dpi_core::LayoutPosition>,
    nozzle_diam: f64,
) -> bool {
    dpi_core::check_paths_overflow(&paths, &transforms, &non_prime_positions, nozzle_diam)
}

// ─── Zpracování vektorových drah ──────────────────────────────────────────────

#[tauri::command]
fn process_paths(
    raw_paths: dpi_core::SubstratePaths,
    slice_params: dpi_core::SliceParams,
) -> dpi_core::SubstratePaths {
    dpi_core::process_substrate_paths(&raw_paths, &slice_params)
}

#[tauri::command]
fn parse_dxf_file(dxf_text: String) -> dpi_core::SubstratePaths {
    dpi_core::parse_dxf(&dxf_text)
}

#[tauri::command]
fn parse_svg_file(svg_text: String, fineness: f64) -> dpi_core::SubstratePaths {
    dpi_core::parse_svg(&svg_text, fineness)
}

// ─── Nastavení ───────────────────────────────────────────────────────────────

/// Výchozí nastavení vestavěné do binárky — viz src-tauri/default_settings.json.
/// include_str! zaručí, že se soubor zvaliduje při kompilaci (existence)
/// a nemíchá se JSON s Rust kódem.
const DEFAULT_SETTINGS_JSON: &str = include_str!("../default_settings.json");

/// Vrátí cestu ke konfiguračnímu adresáři aplikace (dle platformy).
/// Pokud adresář neexistuje, vytvoří ho.
fn get_app_config_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("Nelze získat konfigurační adresář: {e}"))?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Nelze vytvořit konfigurační adresář: {e}"))?;
    }
    Ok(dir)
}

#[tauri::command]
fn get_settings(app_handle: AppHandle) -> Result<serde_json::Value, String> {
    let config_dir = get_app_config_dir(&app_handle)?;
    let settings_path = config_dir.join("settings.json");

    if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;
        return serde_json::from_str(&content).map_err(|e| e.to_string());
    }

    // Záložní výchozí nastavení vestavěné v binárce
    serde_json::from_str(DEFAULT_SETTINGS_JSON).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_settings(settings: serde_json::Value, app_handle: AppHandle) -> Result<(), String> {
    let config_dir = get_app_config_dir(&app_handle)?;
    let path = config_dir.join("settings.json");
    let content = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_feedback(data: serde_json::Value, app_handle: AppHandle) -> Result<(), String> {
    let config_dir = get_app_config_dir(&app_handle)?;
    let path = config_dir.join("feedback_log.json");

    let mut logs: Vec<serde_json::Value> = if path.exists() {
        let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    };

    logs.push(data);
    let content = serde_json::to_string_pretty(&logs).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

// ─── Vstupní bod Tauri ───────────────────────────────────────────────────────

pub static START_TIME: OnceLock<u128> = OnceLock::new();

#[tauri::command]
fn get_startup_time() -> u128 {
    let start = *START_TIME.get().unwrap_or(&0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    now.saturating_sub(start)
}

#[tauri::command]
fn get_local_ip() -> String {
    if let Ok(ip) = local_ip_address::local_ip() {
        ip.to_string()
    } else {
        "127.0.0.1".to_string()
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = START_TIME.set(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    );
    let manager = Arc::new(PrinterManager::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(manager)
        .invoke_handler(tauri::generate_handler![
            compute_preview_segments,
            check_paths_overflow,
            update_layout,
            generate_gcode_job,
            process_paths,
            parse_dxf_file,
            parse_svg_file,
            build_gcode_metadata_header,
            parse_gcode_metadata,
            parse_gcode_file_paths,
            generate_csv_protocol,
            get_ports,
            connect_printer,
            auto_connect_printer,
            disconnect_printer,
            start_print_job,
            pause_print_job,
            resume_print_job,
            resume_app_pause_job,
            stop_print_job,
            send_manual_gcode,
            send_manual_gcode_blocking,
            get_settings,
            save_settings,
            save_feedback,
            get_startup_time,
            get_local_ip,
        ])
        .setup(|app| {
            // Spusť WebSocket server pro sdílení stavu na pozadí
            tauri::async_runtime::spawn(async move {
                sync::start_ws_server().await;
            });

            #[cfg(target_os = "linux")]
            {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.with_webview(|webview| {
                        let webview_inner = webview.inner();
                        webview_inner.connect_permission_request(move |_webview, request| {
                            // Povolujeme pouze přístup ke kameře/mikrofonu (náhled tisku);
                            // vše ostatní (geolokace, notifikace…) zamítáme.
                            if request.dynamic_cast_ref::<UserMediaPermissionRequest>().is_some() {
                                request.allow();
                            } else {
                                request.deny();
                            }
                            true
                        });
                    });
                }
            }

            // Stejná politika pro Windows (WebView2): kamera/mikrofon povolit,
            // vše ostatní zamítnout — bez handleru WebView2 zobrazuje vlastní
            // prompt, případně getUserMedia tiše zamítne.
            #[cfg(target_os = "windows")]
            {
                use webview2_com::Microsoft::Web::WebView2::Win32::{
                    COREWEBVIEW2_PERMISSION_KIND_CAMERA, COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
                    COREWEBVIEW2_PERMISSION_STATE_ALLOW, COREWEBVIEW2_PERMISSION_STATE_DENY,
                };
                use webview2_com::PermissionRequestedEventHandler;

                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.with_webview(|webview| unsafe {
                        let Ok(core) = webview.controller().CoreWebView2() else {
                            return;
                        };
                        let handler = PermissionRequestedEventHandler::create(Box::new(
                            |_sender, args| {
                                if let Some(args) = args {
                                    let mut kind = COREWEBVIEW2_PERMISSION_KIND_CAMERA;
                                    args.PermissionKind(&mut kind)?;
                                    let state = if kind == COREWEBVIEW2_PERMISSION_KIND_CAMERA
                                        || kind == COREWEBVIEW2_PERMISSION_KIND_MICROPHONE
                                    {
                                        COREWEBVIEW2_PERMISSION_STATE_ALLOW
                                    } else {
                                        COREWEBVIEW2_PERMISSION_STATE_DENY
                                    };
                                    args.SetState(state)?;
                                }
                                Ok(())
                            },
                        ));
                        let mut token = Default::default();
                        let _ = core.add_PermissionRequested(&handler, &mut token);
                    });
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Chyba při spuštění Tauri aplikace");
}
