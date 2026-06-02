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
use webkit2gtk::{PermissionRequestExt, WebViewExt};

// ─── Datové typy ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct GCodeResponse {
    gcode: String,
    total_dist: f64,
    total_time: f64,
}

// ─── G-kód příkazy ───────────────────────────────────────────────────────────

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn calculate_layout(
    count: usize,
    slide_w: f64,
    slide_h: f64,
    spacing: f64,
    bed_max_x: f64,
    bed_max_y: f64,
    start_offset_x: f64,
    start_offset_y: f64,
    prime_active: bool,
    bed_min_x: f64,
) -> Vec<dpi_core::LayoutPosition> {
    let bed = dpi_core::BedConfig {
        max_x: bed_max_x,
        max_y: bed_max_y,
        min_x: bed_min_x,
        offset_x: start_offset_x,
        offset_y: start_offset_y,
    };
    dpi_core::get_layout_positions(count, slide_w, slide_h, spacing, prime_active, &bed)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn generate_gcode_job(
    slide_paths: Vec<dpi_core::SubstratePaths>,
    params: dpi_core::ProcessParams,
    transforms: Vec<dpi_core::Transform>,
    slide_overrides: std::collections::HashMap<String, dpi_core::SlideOverride>,
    start_gcode: String,
    end_gcode: String,
    loop_start_gcode: String,
    loop_end_gcode: String,
    bed_max_x: f64,
    bed_max_y: f64,
    start_offset_x: f64,
    start_offset_y: f64,
    multi_spacing: f64,
    block_height: f64,
    calibration_factor: f64,
    retraction: f64,
    retract_speed: f64,
    bed_min_x: f64,
    z_hop: f64,
    safe_z: f64,
) -> Result<GCodeResponse, String> {
    let machine = dpi_core::MachineConfig {
        bed: dpi_core::BedConfig {
            max_x: bed_max_x,
            max_y: bed_max_y,
            min_x: bed_min_x,
            offset_x: start_offset_x,
            offset_y: start_offset_y,
        },
        start_gcode,
        end_gcode,
        loop_start_gcode,
        loop_end_gcode,
        multi_spacing,
        block_height,
        calibration_factor,
        retraction,
        retract_speed,
        z_hop,
        safe_z,
    };
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

#[tauri::command]
fn stop_print_job(manager: State<'_, Arc<PrinterManager>>) -> Result<(), String> {
    manager.send(PrinterCommand::Stop)
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

/// Vypočítá nové pozice sklíček A přizpůsobí transformace v jediném Rust volání.
/// Nahrazuje kombinaci `calculate_layout` + TS smyčky pro přizpůsobení transformací.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn recalculate_layout(
    sample_count: usize,
    slide_w: f64,
    slide_h: f64,
    multi_spacing: f64,
    prime_active: bool,
    bed_max_x: f64,
    bed_max_y: f64,
    bed_min_x: f64,
    start_offset_x: f64,
    start_offset_y: f64,
    old_positions: Vec<dpi_core::LayoutPosition>,
    current_transforms: Vec<dpi_core::Transform>,
    current_paths: Vec<dpi_core::SubstratePaths>,
    nozzle_diam: f64,
) -> dpi_core::LayoutWithTransforms {
    let bed = dpi_core::BedConfig {
        max_x: bed_max_x,
        max_y: bed_max_y,
        min_x: bed_min_x,
        offset_x: start_offset_x,
        offset_y: start_offset_y,
    };
    let positions =
        dpi_core::get_layout_positions(sample_count, slide_w, slide_h, multi_spacing, prime_active, &bed);
    let old_non_prime: Vec<dpi_core::LayoutPosition> =
        old_positions.into_iter().filter(|p| !p.is_prime).collect();
    let transforms = dpi_core::fit_transforms_to_layout(
        &positions,
        &old_non_prime,
        &current_transforms,
        &current_paths,
        nozzle_diam,
    );
    dpi_core::LayoutWithTransforms { positions, transforms }
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
fn get_prime_preview(
    pos: dpi_core::LayoutPosition,
    params: dpi_core::ProcessParams,
    prime_override: Option<dpi_core::SlideOverride>,
) -> dpi_core::SubstratePaths {
    dpi_core::generate_prime_preview(&pos, &params, prime_override.as_ref())
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

const DEFAULT_SETTINGS_JSON: &str = r##"{
    "bed_max_x": 250.0,
    "bed_max_y": 210.0,
    "bed_min_x": 0.0,
    "start_offset_x": 18.0,
    "start_offset_y": 11.0,
    "multi_spacing": 5.0,
    "block_height": 34.0,
    "hidden_nozzle_part": 4.0,
    "print_speed": 1500,
    "retraction": 0.0,
    "bed_min_temp": 30,
    "start_gcode": ";FLAVOR:Marlin\n; --- INICIALIZACE TISKÁRNY PRO KAPALINY ---\nM201 X1000 Y1000 Z200 E5000\nM203 X200 Y200 Z12 E120\nM204 S1250 T1250\nM205 X8.00 Y8.00 Z0.40 E4.50\nM205 S0 T0\n\nG90 ; use absolute coordinates\nM83 ; extruder RELATIVE mode\nM302 P1 ; disable cold extrusion checking\nM302 S0 ; always allow extrusion\nM900 K0 ; disable Linear Advance for liquids\n\nG28\nG92 E0.0\n",
    "loop_start_gcode": "",
    "loop_end_gcode": "",
    "end_gcode": "G0 Z30 F1000 ; Zvednuti tiskove hlavy\nG0 X0 Y200 F3000 ; Vysunuti podlozky vpred\nM84 ; Vypnuti motoru\n",
    "sklo_dims": {
        "Laboratorní Sklo (76 x 26 x 1 mm)": [76.0, 26.0, 1.0],
        "FTO (76 x 50 x 1 mm)": [50.0, 76.0, 1.0],
        "Vlastní": [25.0, 25.0, 1.0]
    },
    "default_z_offset": 0.2,
    "default_z_hop": 2.0,
    "safe_z": 20.0,
    "default_speed": 1500,
    "default_infill": 1.0,
    "default_density": 0.05,
    "show_slide_grid": true,
    "nozzle_defs": {
        "Červená": [31.1, 0.3, 4.0, "#ef4444"],
        "Modrá": [31.0, 0.41, 4.0, "#3b82f6"]
    },
    "filament_diameter": 9.5,
    "flow_multiplier": 1.0,
    "calibration_factor": 0.323877,
    "calibration_object_height": 0.1,
    "camera_rotation": 180,
    "z_step": 0.0025,
    "camera_mirror": false,
    "show_bed_axes": true,
    "liquid_density": 1.0
}"##;

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
            calculate_layout,
            recalculate_layout,
            generate_gcode_job,
            process_paths,
            parse_dxf_file,
            parse_svg_file,
            build_gcode_metadata_header,
            parse_gcode_metadata,
            parse_gcode_file_paths,
            get_prime_preview,
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
                            request.allow();
                            true
                        });
                    });
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Chyba při spuštění Tauri aplikace");
}
