use serde::Serialize;
use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

// ─── Datové typy ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct PrinterStatus {
    pub is_connected: bool,
    pub is_printing: bool,
    pub is_paused: bool,
    pub current_x: f64,
    pub current_y: f64,
    pub current_z: f64,
    pub temp_extruder: f64,
    pub temp_bed: f64,
    pub progress: usize,
    pub total_dist: f64,
    pub time_remaining: f64,
}

impl Default for PrinterStatus {
    fn default() -> Self {
        Self {
            is_connected: false,
            is_printing: false,
            is_paused: false,
            current_x: 0.0,
            current_y: 0.0,
            current_z: 0.0,
            temp_extruder: 0.0,
            temp_bed: 0.0,
            progress: 0,
            total_dist: 0.0,
            time_remaining: 0.0,
        }
    }
}

pub enum PrinterCommand {
    StartPrint {
        gcode: String,
        total_dist: f64,
        total_time: f64,
    },
    Pause,
    Resume,
    /// Obnoví tisk po app-side pauze (M1/M0 modal) — neposílá M602 tiskárně.
    AppResume,
    SendManual {
        gcode: String,
    },
    SendManualBlocking {
        gcode: String,
        reply: Sender<Result<(), String>>,
    },
    Stop {
        /// Parkovací pozice po nouzovém zastavení — odvozená z konfigurace
        /// podložky (bed_min_x, bed_max_y), ne hardcoded.
        park_x: f64,
        park_y: f64,
    },
}

pub struct PrinterManager {
    pub status: Arc<Mutex<PrinterStatus>>,
    pub tx: Mutex<Option<Sender<PrinterCommand>>>,
}

impl Default for PrinterManager {
    fn default() -> Self {
        Self {
            status: Arc::new(Mutex::new(PrinterStatus::default())),
            tx: Mutex::new(None),
        }
    }
}

impl PrinterManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn send(&self, cmd: PrinterCommand) -> Result<(), String> {
        let guard = self.tx.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref sender) = *guard {
            sender.send(cmd).map_err(|e| e.to_string())?;
            Ok(())
        } else {
            Err("Tiskárna není připojena.".to_string())
        }
    }
}

// ─── Pomocné parsery ──────────────────────────────────────────────────────────

/// Pomocný parser teplot z Marlin telemetry řádků (T:xxx.x B:xxx.x).
pub fn parse_temperatures(line: &str) -> Option<(f64, f64)> {
    let t_start = line.find("T:")?;
    let sub_t = &line[t_start + 2..];
    let t_end = sub_t.find(' ').unwrap_or(sub_t.len());
    let t_part = sub_t[..t_end].split('/').next()?.trim();
    let temp_e: f64 = t_part.parse().ok()?;

    let b_start = line.find("B:")?;
    let sub_b = &line[b_start + 2..];
    let b_end = sub_b.find(' ').unwrap_or(sub_b.len());
    let b_part = sub_b[..b_end].split('/').next()?.trim();
    let temp_b: f64 = b_part.parse().ok()?;

    Some((temp_e, temp_b))
}

/// Parser souřadnic trysky z G0/G1 příkazů — sdílený s dpi-core.
/// Na rozdíl od dřívější lokální verze správně ignoruje G10/G11 (retract).
#[inline]
pub fn parse_gcode_pos(line: &str) -> (Option<f64>, Option<f64>, Option<f64>) {
    dpi_core::parse_move_axes(line)
}

/// Detekuje, zda řádek odpovědi tiskárny obsahuje potvrzení příkazu.
/// Marlin/Prusa posílají "ok" na začátku řádku (někdy s dalšími daty).
/// Tolerujeme také "ok" kdekoliv v řádku pro starší firmware.
#[inline]
fn line_is_ok(line: &str) -> bool {
    let lower = line.trim().to_lowercase();
    lower.starts_with("ok") || lower.contains(" ok ") || lower == "ok"
}

/// Detekuje, zda řádek odpovědi patří verifikační sekvenci M115
/// (firmware identification nebo první "ok").
#[inline]
fn line_is_verification(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.contains("ok")
        || lower.contains("marlin")
        || lower.contains("prusa")
        || lower.contains("cap:")
        || lower.contains("start")
        || lower.contains("echo:")
        || lower.contains("firmware")
}

// ─── Automatické nalezení tiskárny ───────────────────────────────────────────

/// Pokusí se ověřit konkrétní port jako tiskárnu:
/// 1. Otevře port se zadanou baudrate
/// 2. Počká 2.5 s na DTR reset (jako Python verze)
/// 3. Odešle M115
/// 4. Čeká max. 3 s na verifikační odpověď
///
/// Vrátí otevřený `Box<dyn serialport::SerialPort>` nebo chybu.
pub fn try_connect_port(
    port_name: &str,
    baudrate: u32,
) -> Result<Box<dyn serialport::SerialPort>, String> {
    let mut port = serialport::new(port_name, baudrate)
        .timeout(Duration::from_millis(100))
        .open()
        .map_err(|e| format!("Nelze otevřít {port_name}: {e}"))?;

    // POSIX při otevření TTY nastaví DTR automaticky, Windows ne — desky
    // s nativním USB CDC (Prusa Buddy, 32U4…) bez DTR vůbec nevysílají.
    // Chybu ignorujeme: některé adaptéry DTR nepodporují.
    let _ = port.write_data_terminal_ready(true);

    // Počkáme na restart desky po DTR resetu (jako Python: time.sleep(2.5))
    thread::sleep(Duration::from_millis(2500));

    // Vyčistíme vstupní buffer
    let _ = port.clear(serialport::ClearBuffer::Input);

    // Pošleme M115 pro identifikaci firmware
    port.write_all(b"\nM115\n")
        .map_err(|e| format!("Chyba zápisu na {port_name}: {e}"))?;
    let _ = port.flush();

    // Čekáme max. 3 sekundy na verifikační odpověď
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];

    while Instant::now() < deadline {
        match port.read(&mut byte) {
            Ok(1) => {
                if byte[0] == b'\n' {
                    let line = String::from_utf8_lossy(&buf).to_string();
                    buf.clear();
                    if line_is_verification(&line) {
                        // Úspěšně ověřeno — vrátíme port
                        return Ok(port);
                    }
                } else {
                    buf.push(byte[0]);
                }
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                // Timeout čtení — pokračujeme ve čtení
            }
            Err(_) => {
                thread::sleep(Duration::from_millis(50));
            }
        }
    }

    Err(format!("Tiskárna na portu {port_name} neodpovídá"))
}

/// Automaticky projde dostupné sériové porty a vrátí název a otevřený port
/// prvního zařízení, které odpoví jako Marlin/Prusa tiskárna.
pub fn auto_find_printer(
    baudrate: u32,
) -> Result<(String, Box<dyn serialport::SerialPort>), String> {
    let available =
        serialport::available_ports().map_err(|e| format!("Chyba při získávání portů: {e}"))?;

    if available.is_empty() {
        return Err("Žádné sériové porty nenalezeny".to_string());
    }

    // Bluetooth COM porty (fantomové porty na Windows) umí při otevírání
    // blokovat desítky sekund — přeskakujeme je. USB porty zkoušíme jako
    // první, ostatní (PCI, neznámé) až po nich.
    let (usb, other): (Vec<_>, Vec<_>) = available
        .into_iter()
        .filter(|p| !matches!(p.port_type, serialport::SerialPortType::BluetoothPort))
        .partition(|p| matches!(p.port_type, serialport::SerialPortType::UsbPort(_)));

    if usb.is_empty() && other.is_empty() {
        return Err("Žádné použitelné sériové porty (nalezeny pouze Bluetooth)".to_string());
    }

    let mut last_error = String::new();
    for port_info in usb.into_iter().chain(other) {
        let name = port_info.port_name.clone();
        match try_connect_port(&name, baudrate) {
            Ok(port) => return Ok((name, port)),
            Err(e) => {
                last_error = e;
            }
        }
    }

    Err(format!(
        "Žádná tiskárna nenalezena. Poslední chyba: {last_error}"
    ))
}

// ─── Sdílené obsluhy příkazů (volané z hlavní i wait_ok smyčky) ──────────────

type Port = Box<dyn serialport::SerialPort>;

fn lock_status(arc: &Arc<Mutex<PrinterStatus>>) -> std::sync::MutexGuard<'_, PrinterStatus> {
    arc.lock().unwrap_or_else(|e| e.into_inner())
}

/// Pozastaví tisk (M601). Vrátí `true` pokud byl příkaz skutečně odeslán
/// (tj. tiskárna nebyla už pozastavená) — volající pak ví, že přijde "ok".
fn handle_pause(
    port: &mut Port,
    status_arc: &Arc<Mutex<PrinterStatus>>,
    app_handle: &AppHandle,
    pause_start: &mut Option<Instant>,
) -> bool {
    let mut status = lock_status(status_arc);
    if status.is_paused {
        return false;
    }
    status.is_paused = true;
    *pause_start = Some(Instant::now());
    let _ = port.write_all(b"M601\n");
    let _ = port.flush();
    app_handle.emit("printer-status-changed", status.clone()).ok();
    true
}

/// Obnoví tisk. `send_m602 = false` pro app-side pauzu (M1/M0 modal).
/// Vrátí `true` pokud bylo odesláno M602 (přijde "ok").
fn handle_resume(
    port: &mut Port,
    status_arc: &Arc<Mutex<PrinterStatus>>,
    app_handle: &AppHandle,
    pause_start: &mut Option<Instant>,
    paused_duration: &mut Duration,
    send_m602: bool,
) -> bool {
    let mut status = lock_status(status_arc);
    if !status.is_paused {
        return false;
    }
    status.is_paused = false;
    if let Some(ps) = pause_start.take() {
        *paused_duration += ps.elapsed();
    }
    if send_m602 {
        let _ = port.write_all(b"M602\n");
        let _ = port.flush();
    }
    app_handle.emit("printer-status-changed", status.clone()).ok();
    send_m602
}

/// Nouzové zastavení tisku: vyčistí frontu, pošle M410 a odjede na parkovací pozici.
#[allow(clippy::too_many_arguments)]
fn handle_stop(
    port: &mut Port,
    status_arc: &Arc<Mutex<PrinterStatus>>,
    app_handle: &AppHandle,
    gcode_queue: &mut Vec<String>,
    dist_per_line: &mut Vec<f64>,
    queue_idx: &mut usize,
    dist_sent: &mut f64,
    pause_start: &mut Option<Instant>,
    park_x: f64,
    park_y: f64,
) {
    gcode_queue.clear();
    dist_per_line.clear();
    *queue_idx = 0;
    *dist_sent = 0.0;
    *pause_start = None;

    // Nouzový stop a bezpečný odjezd trysky (stejně jako Python)
    let _ = port.clear(serialport::ClearBuffer::Output);
    let _ = port.clear(serialport::ClearBuffer::Input);
    let _ = port.write_all(b"M410\n"); // Okamžité zastavení
    let _ = port.flush();
    thread::sleep(Duration::from_millis(500));
    let safe_stop = format!("G91\nG0 Z15 F1000\nG90\nG0 X{park_x:.1} Y{park_y:.1} F5000\n");
    let _ = port.write_all(safe_stop.as_bytes());
    let _ = port.flush();

    let mut status = lock_status(status_arc);
    status.is_printing = false;
    status.is_paused = false;
    status.progress = 0;
    app_handle.emit("printer-status-changed", status.clone()).ok();
}

/// Maximální čekací doba na "ok" odpověď tiskárny (zahrnuje nahřívání podložky apod.).
const OK_TIMEOUT_SECS: u64 = 600;

/// Timeout pro blokující manuální příkazy (G28 s mesh levelingem může trvat minuty).
const BLOCKING_OK_TIMEOUT_SECS: u64 = 300;

// ─── Komunikační smyčka ───────────────────────────────────────────────────────

/// Zprávy ze čtecího vlákna do řídicího vlákna.
enum ReaderMessage {
    /// Tiskárna potvrdila příkaz (odpověď "ok").
    Ok,
    /// Tiskárna zaslala telemetrii teplot.
    Temperatures(f64, f64),
}

/// Spustí neblokující smyčku sériové komunikace ve dvou vláknech:
/// - **řídicí vlákno**: zpracovává příkazy z Tauri a posílá G-kód na port,
/// - **čtecí vlákno**: čte odpovědi tiskárny a předává je přes kanál.
///
/// Port musí být **již otevřený a ověřený** (viz `try_connect_port`).
pub fn spawn_comms_loop(
    port_name: String,
    port: Box<dyn serialport::SerialPort>,
    manager: Arc<PrinterManager>,
    app_handle: AppHandle,
) -> Result<Sender<PrinterCommand>, String> {
    let (cmd_tx, cmd_rx): (Sender<PrinterCommand>, Receiver<PrinterCommand>) = channel();
    let (reader_tx, reader_rx): (Sender<ReaderMessage>, Receiver<ReaderMessage>) = channel();

    let status_arc = Arc::clone(&manager.status);

    // Označíme tiskárnu jako připojenou
    {
        let mut status = status_arc.lock().unwrap_or_else(|e| e.into_inner());
        status.is_connected = true;
        app_handle
            .emit("printer-status-changed", status.clone())
            .ok();
    }

    // ── Čtecí vlákno: pouze čte ze sériového portu a předává zprávy ──────────
    let reader_port = port
        .try_clone()
        .map_err(|e| format!("Chyba klonování portu {port_name}: {e}"))?;
    let app_handle_reader = app_handle.clone();
    let status_arc_reader = Arc::clone(&manager.status);
    thread::spawn(move || {
        let mut reader = BufReader::new(reader_port);
        let mut line = String::new();
        loop {
            {
                let status = status_arc_reader.lock().unwrap_or_else(|e| e.into_inner());
                if !status.is_connected {
                    break;
                }
            }

            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break, // EOF — port zavřen
                Ok(_) => {
                    let _ = app_handle_reader.emit("serial-rx", line.clone());
                    // Teploty — zkontrolujeme DŘÍV než "ok", protože
                    // Marlin posílá "ok T:205.2 /0.0 B:60.1 /60.0" v jednom řádku
                    if line.contains("T:") && line.contains("B:") {
                        if let Some((te, tb)) = parse_temperatures(&line) {
                            if reader_tx.send(ReaderMessage::Temperatures(te, tb)).is_err() {
                                break;
                            }
                            // Řádek s teplotami může zároveň obsahovat "ok" —
                            // pošleme i Ok zprávu, aby se ping-pong neblokoval
                            if line_is_ok(&line) && reader_tx.send(ReaderMessage::Ok).is_err() {
                                break;
                            }
                            continue;
                        }
                    }
                    if line_is_ok(&line) && reader_tx.send(ReaderMessage::Ok).is_err() {
                        break;
                    }
                }
                Err(_) => {
                    // Timeout nebo jiná chyba čtení — pokračujeme
                }
            }
        }
    });

    // ── Řídicí vlákno: zpracovává příkazy a řídí tisk ────────────────────────
    let status_arc2 = Arc::clone(&manager.status);
    thread::spawn(move || {
        let mut serial_port = port;
        let mut gcode_queue: Vec<String> = Vec::new();
        let mut dist_per_line: Vec<f64> = Vec::new();
        let mut queue_idx: usize = 0;
        let mut print_total_dist = 0.0_f64;
        let mut dist_sent = 0.0_f64;
        let mut print_total_time = 0.0_f64;
        let mut print_start_time = Instant::now();
        let mut paused_duration = Duration::ZERO;
        let mut pause_start: Option<Instant> = None;
        let mut last_temp_query = Instant::now();

        loop {
            // 1. Zpracování příchozích příkazů z Tauri
            if let Ok(cmd) = cmd_rx.try_recv() {
                match cmd {
                    PrinterCommand::StartPrint {
                        gcode,
                        total_dist,
                        total_time,
                    } => {
                        // Ignorujeme StartPrint pokud tisk již probíhá
                        let already_printing = {
                            let s = status_arc2.lock().unwrap_or_else(|e| e.into_inner());
                            s.is_printing
                        };
                        if already_printing {
                            continue;
                        }

                        // Sestavíme frontu G-kódu, zachováme markery a převedeme M1/M0/M601
                        gcode_queue.clear();
                        for raw_line in gcode.lines() {
                            let clean = raw_line.split(';').next().unwrap_or("").trim();
                            if clean.is_empty() {
                                if raw_line.contains("; LIVE_ADJUST") {
                                    gcode_queue.push("; LIVE_ADJUST".to_string());
                                }
                                continue;
                            }
                            let upper = clean.to_uppercase();
                            let is_m0 = upper == "M0" || upper.starts_with("M0 ");
                            let is_m1 = upper == "M1" || upper.starts_with("M1 ");
                            let is_m601 = upper == "M601";
                            if is_m0 || is_m1 || is_m601 {
                                let msg = clean.splitn(2, ' ').nth(1).unwrap_or("").trim();
                                let msg = if msg.is_empty() { "Stiskněte pro pokračování" } else { msg };
                                gcode_queue.push(format!("; APP_PAUSE:{msg}"));
                            } else {
                                gcode_queue.push(clean.to_string());
                            }
                        }

                        // Pre-výpočet vzdálenosti extruze pro každý řádek fronty.
                        // Slouží k výpočtu progress% podle ujeté vzdálenosti tisku (ne počtu řádků).
                        dist_per_line.clear();
                        dist_per_line.reserve(gcode_queue.len());
                        {
                            let mut cur_x = 0.0_f64;
                            let mut cur_y = 0.0_f64;
                            for line in &gcode_queue {
                                let (ox, oy, _) = parse_gcode_pos(line);
                                let new_x = ox.unwrap_or(cur_x);
                                let new_y = oy.unwrap_or(cur_y);
                                let is_extrusion = dpi_core::is_extrusion_move(line);
                                let d = if is_extrusion {
                                    ((new_x - cur_x).powi(2) + (new_y - cur_y).powi(2)).sqrt()
                                } else {
                                    0.0
                                };
                                dist_per_line.push(d);
                                if ox.is_some() { cur_x = new_x; }
                                if oy.is_some() { cur_y = new_y; }
                            }
                        }

                        queue_idx = 0;
                        print_total_dist = total_dist;
                        dist_sent = 0.0;
                        print_total_time = total_time;
                        print_start_time = Instant::now();
                        paused_duration = Duration::ZERO;
                        pause_start = None;

                        let mut status = status_arc2.lock().unwrap_or_else(|e| e.into_inner());
                        status.is_printing = true;
                        status.is_paused = false;
                        status.progress = 0;
                        status.total_dist = total_dist;
                        status.time_remaining = total_time;
                        app_handle
                            .emit("printer-status-changed", status.clone())
                            .ok();
                    }

                    PrinterCommand::Pause => {
                        handle_pause(&mut serial_port, &status_arc2, &app_handle, &mut pause_start);
                    }

                    PrinterCommand::Resume => {
                        handle_resume(
                            &mut serial_port,
                            &status_arc2,
                            &app_handle,
                            &mut pause_start,
                            &mut paused_duration,
                            true,
                        );
                    }

                    PrinterCommand::AppResume => {
                        handle_resume(
                            &mut serial_port,
                            &status_arc2,
                            &app_handle,
                            &mut pause_start,
                            &mut paused_duration,
                            false,
                        );
                    }

                    PrinterCommand::SendManual { gcode } => {
                        let mut status = status_arc2.lock().unwrap_or_else(|e| e.into_inner());
                        let (nx, ny, nz) = parse_gcode_pos(&gcode);
                        if let Some(val) = nx {
                            status.current_x = val;
                        }
                        if let Some(val) = ny {
                            status.current_y = val;
                        }
                        if let Some(val) = nz {
                            status.current_z = val;
                        }

                        let mut buf = gcode.clone();
                        buf.push('\n');
                        let _ = serial_port.write_all(buf.as_bytes());
                        let _ = serial_port.flush();
                        app_handle
                            .emit("printer-status-changed", status.clone())
                            .ok();
                    }

                    PrinterCommand::SendManualBlocking { gcode, reply } => {
                        let already_printing = {
                            let s = status_arc2.lock().unwrap_or_else(|e| e.into_inner());
                            s.is_printing
                        };
                        if already_printing {
                            let _ = reply.send(Err("Tiskárna právě tiskne.".to_string()));
                            continue;
                        }

                        let mut error: Option<String> = None;
                        'blocking: for raw_line in gcode.lines() {
                            let clean =
                                raw_line.split(';').next().unwrap_or("").trim().to_string();
                            if clean.is_empty() {
                                continue;
                            }

                            let (nx, ny, nz) = parse_gcode_pos(&clean);
                            {
                                let mut status =
                                    status_arc2.lock().unwrap_or_else(|e| e.into_inner());
                                if let Some(v) = nx {
                                    status.current_x = v;
                                }
                                if let Some(v) = ny {
                                    status.current_y = v;
                                }
                                if let Some(v) = nz {
                                    status.current_z = v;
                                }
                            }

                            let mut buf = clean.clone();
                            buf.push('\n');
                            if serial_port.write_all(buf.as_bytes()).is_err()
                                || serial_port.flush().is_err()
                            {
                                error = Some("Chyba zápisu na sériový port.".to_string());
                                break 'blocking;
                            }

                            let deadline =
                                Instant::now() + Duration::from_secs(BLOCKING_OK_TIMEOUT_SECS);
                            let mut got_ok = false;
                            while Instant::now() < deadline {
                                match reader_rx.recv_timeout(Duration::from_millis(5)) {
                                    Ok(msg) => {
                                        match msg {
                                            ReaderMessage::Ok => got_ok = true,
                                            ReaderMessage::Temperatures(te, tb) => {
                                                let mut status = status_arc2
                                                    .lock()
                                                    .unwrap_or_else(|e| e.into_inner());
                                                status.temp_extruder = te;
                                                status.temp_bed = tb;
                                                let _ = app_handle.emit("printer-status-changed", status.clone());
                                            }
                                        }
                                        while let Ok(msg2) = reader_rx.try_recv() {
                                            match msg2 {
                                                ReaderMessage::Ok => got_ok = true,
                                                ReaderMessage::Temperatures(te, tb) => {
                                                    let mut status = status_arc2
                                                        .lock()
                                                        .unwrap_or_else(|e| e.into_inner());
                                                    status.temp_extruder = te;
                                                    status.temp_bed = tb;
                                                    let _ = app_handle.emit("printer-status-changed", status.clone());
                                                }
                                            }
                                        }
                                    }
                                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                                }
                                if got_ok {
                                    break;
                                }
                            }

                            if !got_ok {
                                error = Some(format!(
                                    "Timeout při čekání na potvrzení příkazu: {clean}"
                                ));
                                break 'blocking;
                            }
                        }

                        {
                            let status =
                                status_arc2.lock().unwrap_or_else(|e| e.into_inner());
                            app_handle.emit("printer-status-changed", status.clone()).ok();
                        }
                        match error {
                            Some(e) => {
                                let _ = reply.send(Err(e));
                            }
                            None => {
                                let _ = reply.send(Ok(()));
                            }
                        }
                    }

                    PrinterCommand::Stop { park_x, park_y } => {
                        handle_stop(
                            &mut serial_port,
                            &status_arc2,
                            &app_handle,
                            &mut gcode_queue,
                            &mut dist_per_line,
                            &mut queue_idx,
                            &mut dist_sent,
                            &mut pause_start,
                            park_x,
                            park_y,
                        );
                    }
                }
            }

            // 2. Kontrola stavu spojení (ukončit smyčku při odpojení)
            {
                let status = status_arc2.lock().unwrap_or_else(|e| e.into_inner());
                if !status.is_connected {
                    break;
                }
            }

            // 3. Zpracování zpráv ze čtecího vlákna (teploty) mimo tisk
            while let Ok(msg) = reader_rx.try_recv() {
                match msg {
                    ReaderMessage::Temperatures(te, tb) => {
                        let mut status = status_arc2.lock().unwrap_or_else(|e| e.into_inner());
                        status.temp_extruder = te;
                        status.temp_bed = tb;
                        app_handle
                            .emit("printer-status-changed", status.clone())
                            .ok();
                    }
                    ReaderMessage::Ok => {
                        // Ok zprávy mimo ping-pong cyklus ignorujeme
                    }
                }
            }

            // 4. Aktivní tisk z fronty
            let (is_printing, is_paused) = {
                let status = status_arc2.lock().unwrap_or_else(|e| e.into_inner());
                (status.is_printing, status.is_paused)
            };

            if is_printing && !is_paused {
                if queue_idx < gcode_queue.len() {
                    let next_line = gcode_queue[queue_idx].clone();

                    // LIVE_ADJUST marker — pozastavíme tisk a vyžádáme si kalibraci
                    if next_line == "; LIVE_ADJUST" {
                        let mut status = status_arc2.lock().unwrap_or_else(|e| e.into_inner());
                        status.is_paused = true;
                        pause_start = Some(Instant::now());
                        let _ = app_handle.emit("live-adjust-requested", ());
                        app_handle
                            .emit("printer-status-changed", status.clone())
                            .ok();
                        queue_idx += 1;
                        continue;
                    }

                    // APP_PAUSE marker (M1/M0/M601) — zobrazíme hlášku v aplikaci
                    if let Some(msg) = next_line.strip_prefix("; APP_PAUSE:") {
                        let mut status = status_arc2.lock().unwrap_or_else(|e| e.into_inner());
                        status.is_paused = true;
                        pause_start = Some(Instant::now());
                        app_handle.emit("app-pause-requested", msg.to_string()).ok();
                        app_handle
                            .emit("printer-status-changed", status.clone())
                            .ok();
                        queue_idx += 1;
                        continue;
                    }

                    // Aktualizace souřadnic trysky z odesílaného G-kódu
                    {
                        let (nx, ny, nz) = parse_gcode_pos(&next_line);
                        let mut status = status_arc2.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(val) = nx {
                            status.current_x = val;
                        }
                        if let Some(val) = ny {
                            status.current_y = val;
                        }
                        if let Some(val) = nz {
                            status.current_z = val;
                        }
                    }

                    // Odeslání řádku na tiskárnu
                    let mut buf = next_line.clone();
                    buf.push('\n');
                    if serial_port.write_all(buf.as_bytes()).is_ok() {
                        let _ = serial_port.flush();

                        // ── Ping-pong: čekání na odpověď "ok" od tiskárny ────────────
                        let ping_start = Instant::now();
                        let mut ok_received = false;
                        let mut stop_requested = false;
                        // Příkazy vstříknuté během čekání (manuální G-kód, M601/M602)
                        // vygenerují vlastní "ok" — ta nesmí být započtena jako
                        // potvrzení aktuálního řádku fronty.
                        let mut extra_oks_needed = 0usize;
                        let consume_ok = |extra: &mut usize, ok_flag: &mut bool| {
                            if *extra > 0 {
                                *extra -= 1;
                            } else {
                                *ok_flag = true;
                            }
                        };

                        'wait_ok: while ping_start.elapsed() < Duration::from_secs(OK_TIMEOUT_SECS) {
                            // Čteme zprávy z tiskárny s timeoutem (odstraní stuttering způsobený thread::sleep)
                            match reader_rx.recv_timeout(Duration::from_millis(5)) {
                                Ok(msg) => {
                                    match msg {
                                        ReaderMessage::Ok => consume_ok(&mut extra_oks_needed, &mut ok_received),
                                        ReaderMessage::Temperatures(te, tb) => {
                                            let mut status =
                                                status_arc2.lock().unwrap_or_else(|e| e.into_inner());
                                            status.temp_extruder = te;
                                            status.temp_bed = tb;
                                            let _ = app_handle.emit("printer-status-changed", status.clone());
                                        }
                                    }
                                    while let Ok(msg2) = reader_rx.try_recv() {
                                        match msg2 {
                                            ReaderMessage::Ok => consume_ok(&mut extra_oks_needed, &mut ok_received),
                                            ReaderMessage::Temperatures(te, tb) => {
                                                let mut status =
                                                    status_arc2.lock().unwrap_or_else(|e| e.into_inner());
                                                status.temp_extruder = te;
                                                status.temp_bed = tb;
                                                let _ = app_handle.emit("printer-status-changed", status.clone());
                                            }
                                        }
                                    }
                                }
                                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break 'wait_ok,
                            }

                            if ok_received {
                                break 'wait_ok;
                            }

                            // Zpracujeme příkazy z Tauri během čekání na "ok"
                            // OPRAVA: zpracujeme ALL příkazy, nejen Stop
                            while let Ok(cmd) = cmd_rx.try_recv() {
                                match cmd {
                                    PrinterCommand::Stop { park_x, park_y } => {
                                        handle_stop(
                                            &mut serial_port,
                                            &status_arc2,
                                            &app_handle,
                                            &mut gcode_queue,
                                            &mut dist_per_line,
                                            &mut queue_idx,
                                            &mut dist_sent,
                                            &mut pause_start,
                                            park_x,
                                            park_y,
                                        );
                                        stop_requested = true;
                                        break 'wait_ok;
                                    }
                                    PrinterCommand::Pause => {
                                        if handle_pause(
                                            &mut serial_port,
                                            &status_arc2,
                                            &app_handle,
                                            &mut pause_start,
                                        ) {
                                            extra_oks_needed += 1;
                                        }
                                    }
                                    PrinterCommand::Resume => {
                                        if handle_resume(
                                            &mut serial_port,
                                            &status_arc2,
                                            &app_handle,
                                            &mut pause_start,
                                            &mut paused_duration,
                                            true,
                                        ) {
                                            extra_oks_needed += 1;
                                        }
                                    }
                                    PrinterCommand::AppResume => {
                                        handle_resume(
                                            &mut serial_port,
                                            &status_arc2,
                                            &app_handle,
                                            &mut pause_start,
                                            &mut paused_duration,
                                            false,
                                        );
                                    }
                                    PrinterCommand::SendManual { gcode } => {
                                        // Manuální příkazy během tisku povolíme — každý
                                        // neprázdný řádek vyvolá vlastní "ok"
                                        let mut buf2 = gcode.clone();
                                        buf2.push('\n');
                                        let _ = serial_port.write_all(buf2.as_bytes());
                                        let _ = serial_port.flush();
                                        extra_oks_needed +=
                                            gcode.lines().filter(|l| !l.trim().is_empty()).count().max(1);
                                    }
                                    PrinterCommand::StartPrint { .. } => {
                                        // Nový tisk během tisku ignorujeme
                                    }
                                    PrinterCommand::SendManualBlocking { reply, .. } => {
                                        // Blokující příkaz během tisku odmítneme
                                        let _ = reply.send(Err("Tiskárna právě tiskne.".to_string()));
                                    }
                                }
                            }
                        }

                        if stop_requested {
                            continue; // fronta vyčištěna, smyčka skončí sama
                        }

                        if ok_received {
                            // Přičteme vzdálenost právě dokončeného řádku
                            dist_sent += dist_per_line.get(queue_idx).copied().unwrap_or(0.0);
                            queue_idx += 1;

                            // Aktualizace postupu tisku a odhadovaného zbývajícího času
                            let mut status = status_arc2.lock().unwrap_or_else(|e| e.into_inner());
                            let progress = if print_total_dist > 0.0 {
                                ((dist_sent / print_total_dist) * 100.0).min(100.0) as usize
                            } else {
                                ((queue_idx as f64 / gcode_queue.len().max(1) as f64) * 100.0) as usize
                            };
                            status.progress = progress;

                            // Odečteme akumulovanou dobu pauzy (jako Python: start_time += pause_duration)
                            let elapsed =
                                print_start_time.elapsed().saturating_sub(paused_duration);
                            status.time_remaining = if progress > 0 {
                                let total_est = (elapsed.as_secs_f64() / progress as f64) * 100.0;
                                (total_est - elapsed.as_secs_f64()).max(0.0)
                            } else {
                                print_total_time
                            };
                            app_handle
                                .emit("printer-status-changed", status.clone())
                                .ok();
                        } else {
                            // Vypršel timeout 30 s — tisk přerušíme
                            let mut status = status_arc2.lock().unwrap_or_else(|e| e.into_inner());
                            status.is_printing = false;
                            app_handle
                                .emit("printer-status-changed", status.clone())
                                .ok();
                        }
                    } else {
                        // Chyba zápisu na sériový port — tisk přerušíme
                        let mut status = status_arc2.lock().unwrap_or_else(|e| e.into_inner());
                        status.is_printing = false;
                        app_handle
                            .emit("printer-status-changed", status.clone())
                            .ok();
                    }
                } else {
                    // Konec fronty — tisk úspěšně dokončen
                    let mut status = status_arc2.lock().unwrap_or_else(|e| e.into_inner());
                    status.is_printing = false;
                    status.progress = 100;
                    status.time_remaining = 0.0;
                    app_handle
                        .emit("printer-status-changed", status.clone())
                        .ok();
                }
            }

            // 5. Periodické dotazování na teploty v klidovém stavu (každé 2 sekundy)
            if !is_printing && last_temp_query.elapsed() > Duration::from_secs(2) {
                let _ = serial_port.write_all(b"M105\n");
                let _ = serial_port.flush();
                last_temp_query = Instant::now();
            }

            // Drobná úspora CPU
            thread::sleep(Duration::from_millis(10));
        }
    });

    Ok(cmd_tx)
}

// ─── Testy ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_temperatures() {
        // Marlin telemetrie
        let line = "ok T:205.2 /0.0 B:60.1 /60.0";
        assert_eq!(parse_temperatures(line), Some((205.2, 60.1)));

        // Prusa telemetrie
        let line2 = "ok T:23.5 /0.0 B:24.0 /0.0 @:0 B@:0";
        assert_eq!(parse_temperatures(line2), Some((23.5, 24.0)));

        // Neplatný řádek
        let line3 = "ok tiskarna pripravena";
        assert_eq!(parse_temperatures(line3), None);
    }

    #[test]
    fn test_parse_gcode_pos() {
        let (x, y, z) = parse_gcode_pos("G1 X100.5 Y50.2 Z2.0 F1500");
        assert_eq!(x, Some(100.5));
        assert_eq!(y, Some(50.2));
        assert_eq!(z, Some(2.0));

        // Záporné souřadnice
        let (x2, y2, z2) = parse_gcode_pos("G0 X-10.0 Y-20.5 Z-1.2");
        assert_eq!(x2, Some(-10.0));
        assert_eq!(y2, Some(-20.5));
        assert_eq!(z2, Some(-1.2));
    }

    #[test]
    fn test_line_is_ok() {
        assert!(line_is_ok("ok"));
        assert!(line_is_ok("ok P15 B3"));
        assert!(line_is_ok("ok T:205.2 /0.0 B:60.1 /60.0"));
        assert!(line_is_ok("  ok  "));
        assert!(!line_is_ok("echo: busy processing"));
        assert!(!line_is_ok("T:205.2 B:60.1"));
    }

    #[test]
    fn test_line_is_verification() {
        assert!(line_is_verification("FIRMWARE_NAME:Marlin 2.0"));
        assert!(line_is_verification("ok T:23.5"));
        assert!(line_is_verification("start"));
        assert!(line_is_verification("echo: M115"));
        assert!(line_is_verification("CAP:AUTOREPORT_TEMP:1"));
    }

    #[test]
    fn test_printer_manager_default() {
        let mgr = PrinterManager::default();
        let status = mgr.status.lock().unwrap();
        assert!(!status.is_connected);
        assert!(!status.is_printing);
    }
}
