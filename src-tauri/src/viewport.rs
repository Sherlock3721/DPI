//! 3D náhled tiskové plochy — nativní offscreen renderer (`dpi-render`) na
//! vlastním vlákně.
//!
//! Frontend posílá binární pakety (scéna / snímek, viz `dpi_render::protocol`)
//! jako raw tělo IPC a dostává RGBA8 pixely snímku (šířka × výška × 4,
//! řádek 0 = nahoře) jako raw odpověď. GPU zařízení žije jen na render
//! vlákně (vzniká líně při prvním `viewport_init`), příkazy chodí přes kanál.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;

use dpi_render::offscreen::Offscreen;
use tauri::ipc::{InvokeBody, Request, Response};
use tokio::sync::oneshot;

type Reply<T> = oneshot::Sender<Result<T, String>>;

enum Cmd {
    Init {
        force_fallback: bool,
        reply: Reply<serde_json::Value>,
    },
    Scene {
        bytes: Vec<u8>,
        reply: Reply<()>,
    },
    Frame {
        bytes: Vec<u8>,
        reply: Reply<Vec<u8>>,
    },
}

/// Odesílač příkazů běžícího render vlákna (None = vlákno neběží).
static RENDER: Mutex<Option<Sender<Cmd>>> = Mutex::new(None);

const NOT_INIT: &str = "viewport: renderer není inicializovaný (viewport_init)";

fn render_thread(rx: Receiver<Cmd>) {
    // (renderer, force_fallback, se kterým vznikl)
    let mut state: Option<(Offscreen, bool)> = None;
    // Konec kanálu (viewport_dispose zahodí odesílač) = konec vlákna, drop uvolní GPU
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::Init {
                force_fallback,
                reply,
            } => {
                if state.as_ref().is_some_and(|(_, f)| *f != force_fallback) {
                    state = None;
                }
                if state.is_none() {
                    match Offscreen::new(force_fallback) {
                        Ok(o) => state = Some((o, force_fallback)),
                        Err(e) => {
                            let _ = reply.send(Err(e));
                            continue;
                        }
                    }
                }
                let summary = state
                    .as_ref()
                    .map(|(o, _)| serde_json::to_value(o.adapter()).map_err(|e| e.to_string()))
                    .unwrap_or_else(|| Err(NOT_INIT.into()));
                let _ = reply.send(summary);
            }
            Cmd::Scene { bytes, reply } => {
                let r = match state.as_mut() {
                    Some((o, _)) => o.apply_scene(&bytes),
                    None => Err(NOT_INIT.into()),
                };
                let _ = reply.send(r);
            }
            Cmd::Frame { bytes, reply } => {
                let r = match state.as_mut() {
                    Some((o, _)) => o.render_frame(&bytes),
                    None => Err(NOT_INIT.into()),
                };
                let _ = reply.send(r);
            }
        }
    }
}

/// Pošle příkaz render vláknu; `spawn` = případně vlákno nastartuje.
fn send(cmd: Cmd, spawn: bool) -> Result<(), String> {
    let mut guard = RENDER
        .lock()
        .map_err(|_| "viewport: zámek je poškozený".to_string())?;
    if guard.is_none() {
        if !spawn {
            return Err(NOT_INIT.into());
        }
        let (tx, rx) = channel();
        std::thread::Builder::new()
            .name("dpi-viewport".into())
            .spawn(move || render_thread(rx))
            .map_err(|e| format!("viewport: nelze spustit render vlákno: {e}"))?;
        *guard = Some(tx);
    }
    let tx = guard.as_ref().expect("odesílač nastaven výše");
    if tx.send(cmd).is_err() {
        // Vlákno skončilo — příště se spustí znovu
        *guard = None;
        return Err("viewport: render vlákno neběží".into());
    }
    Ok(())
}

async fn call<T>(make: impl FnOnce(Reply<T>) -> Cmd, spawn: bool) -> Result<T, String> {
    let (tx, rx) = oneshot::channel();
    send(make(tx), spawn)?;
    rx.await
        .map_err(|_| "viewport: render vlákno neodpovědělo".to_string())?
}

fn raw_body(request: &Request<'_>) -> Result<Vec<u8>, String> {
    match request.body() {
        InvokeBody::Raw(bytes) => Ok(bytes.clone()),
        InvokeBody::Json(_) => {
            Err("viewport: očekávané binární tělo (Uint8Array/ArrayBuffer)".into())
        }
    }
}

/// Spustí renderer (líně) a vrátí souhrn adaptéru `{name, backend, device_type, driver, cpu, sample_count}`.
/// Jiné `force_fallback` než při posledním volání renderer vytvoří znovu (scénu je pak nutné poslat znovu).
#[tauri::command]
pub async fn viewport_init(force_fallback: bool) -> Result<serde_json::Value, String> {
    call(
        |reply| Cmd::Init {
            force_fallback,
            reply,
        },
        true,
    )
    .await
}

/// Scénový paket (raw tělo).
#[tauri::command]
pub async fn viewport_scene(request: Request<'_>) -> Result<(), String> {
    let bytes = raw_body(&request)?;
    call(|reply| Cmd::Scene { bytes, reply }, false).await
}

/// Paket snímku (raw tělo) → RGBA8 pixely (raw odpověď).
#[tauri::command]
pub async fn viewport_frame(request: Request<'_>) -> Result<Response, String> {
    let bytes = raw_body(&request)?;
    let rgba = call(|reply| Cmd::Frame { bytes, reply }, false).await?;
    Ok(Response::new(rgba))
}

/// Ukončí render vlákno a uvolní GPU zařízení (další `viewport_init` ho vytvoří znovu).
#[tauri::command]
pub async fn viewport_dispose() -> Result<(), String> {
    if let Ok(mut guard) = RENDER.lock() {
        guard.take();
    }
    Ok(())
}
