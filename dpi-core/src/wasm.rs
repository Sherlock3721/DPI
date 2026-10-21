use wasm_bindgen::prelude::*;

use crate::bracket::{build_bracket_stl, compute_bracket_geometry, generate_bracket_svg, BracketParams, RectF};
use crate::gcode::print_heights;
use crate::geometry::{clip_paths_to_slide, compute_world_aabb};
use crate::path_processing::process_substrate_paths;
use crate::types::{LayoutPosition, ProcessParams, SliceParams, SlideOverride, SubstratePaths, Transform};
use std::collections::HashMap;

fn make_transform(gui_dx: f64, gui_dy: f64, scale: f64, rotation_deg: f64, cx: f64, cy: f64) -> Transform {
    Transform { gui_dx, gui_dy, scale, rotation: rotation_deg, cx, cy }
}

/// Vypočítá world-space AABB transformované trasy.
/// Vrátí Float64Array [min_x, max_x, min_y, max_y].
#[wasm_bindgen]
pub fn wasm_world_aabb(
    gui_dx: f64, gui_dy: f64, scale: f64, rotation_deg: f64, cx: f64, cy: f64,
    mn_x: f64, mx_x: f64, mn_y: f64, mx_y: f64,
) -> Box<[f64]> {
    let t = make_transform(gui_dx, gui_dy, scale, rotation_deg, cx, cy);
    let (min_x, max_x, min_y, max_y) = compute_world_aabb(&t, mn_x, mx_x, mn_y, mx_y);
    Box::new([min_x, max_x, min_y, max_y])
}

/// Minimální překryv trasy se sklem (mm) — trasu nejde odsunout celou mimo sklo.
const MIN_OVERLAP_MM: f64 = 1.0;

/// Vypočítá nové gui_dx/gui_dy tak, aby trasa sklíčko aspoň zčásti překrývala
/// (MIN_OVERLAP_MM). Přesah mimo sklo je dovolený — při tisku se ořízne
/// (`clip_paths_to_slide`). Vrátí Float64Array [new_gui_dx, new_gui_dy].
#[wasm_bindgen]
pub fn wasm_clamp_gui_xy(
    gui_dx: f64, gui_dy: f64, scale: f64, rotation_deg: f64, cx: f64, cy: f64,
    pos_x: f64, pos_y: f64, pos_w: f64, pos_h: f64,
    mn_x: f64, mx_x: f64, mn_y: f64, mx_y: f64,
    _nozzle_diam: f64,
) -> Box<[f64]> {
    let t = make_transform(gui_dx, gui_dy, scale, rotation_deg, cx, cy);
    let (min_x, max_x, min_y, max_y) = compute_world_aabb(&t, mn_x, mx_x, mn_y, mx_y);
    let mx = MIN_OVERLAP_MM.min(pos_w / 2.0);
    let my = MIN_OVERLAP_MM.min(pos_h / 2.0);
    let mut new_dx = gui_dx;
    let mut new_dy = gui_dy;
    if max_x < pos_x + mx { new_dx += pos_x + mx - max_x; }
    if min_x > pos_x + pos_w - mx { new_dx -= min_x - (pos_x + pos_w - mx); }
    if max_y < pos_y + my { new_dy += pos_y + my - max_y; }
    if min_y > pos_y + pos_h - my { new_dy -= min_y - (pos_y + pos_h - my); }
    Box::new([new_dx, new_dy])
}

/// Ořízne dráhy sklíčka na sklo (viz `clip_paths_to_slide`) — plátno kreslí
/// stejné dráhy jako G-kód, aby seděly indexy živého náhledu tisku.
/// Vstup i výstup jsou JSON `SubstratePaths`.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn wasm_clip_paths(
    paths_json: &str,
    gui_dx: f64, gui_dy: f64, scale: f64, rotation_deg: f64, cx: f64, cy: f64,
    pos_x: f64, pos_y: f64, pos_w: f64, pos_h: f64,
) -> Result<String, JsValue> {
    let paths: SubstratePaths = serde_json::from_str(paths_json)
        .map_err(|e| JsValue::from_str(&format!("Neplatné dráhy: {e}")))?;
    let t = make_transform(gui_dx, gui_dy, scale, rotation_deg, cx, cy);
    let pos = LayoutPosition { x: pos_x, y: pos_y, width: pos_w, height: pos_h, is_prime: false };
    serde_json::to_string(clip_paths_to_slide(&paths, &t, &pos).as_ref())
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Zpracuje zdrojové dráhy pro jedno sklíčko (`process_substrate_paths`) —
/// živý náhled úpravy objektu v plátně bez IPC. Vstup: JSON `SubstratePaths`
/// a `SliceParams`, výstup JSON `SubstratePaths`.
#[wasm_bindgen]
pub fn wasm_process_paths(raw_json: &str, slice_json: &str) -> Result<String, JsValue> {
    let raw: SubstratePaths = serde_json::from_str(raw_json)
        .map_err(|e| JsValue::from_str(&format!("Neplatné dráhy: {e}")))?;
    let slice: SliceParams = serde_json::from_str(slice_json)
        .map_err(|e| JsValue::from_str(&format!("Neplatné parametry: {e}")))?;
    serde_json::to_string(&process_substrate_paths(&raw, &slice))
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Binárním hledáním najde maximální faktor rotace ∈ [0,1] při němž se trasa
/// (po přizpůsobení pozice) vejde do sklíčka s insetem nozzle_diam/2.
/// Odpovídá funkci `maxFitFactor` v Canvas2D.svelte.
#[wasm_bindgen]
pub fn wasm_max_fit_rotation_factor(
    start_gui_dx: f64, start_gui_dy: f64, scale: f64, cx: f64, cy: f64,
    start_rotation: f64, delta_deg: f64,
    pos_x: f64, pos_y: f64, pos_w: f64, pos_h: f64,
    mn_x: f64, mx_x: f64, mn_y: f64, mx_y: f64,
    nozzle_diam: f64,
) -> f64 {
    let nd = nozzle_diam / 2.0;

    let fits = |f: f64| -> bool {
        let rot = ((start_rotation + f * delta_deg) % 360.0 + 360.0) % 360.0;
        let mut t = make_transform(start_gui_dx, start_gui_dy, scale, rot, cx, cy);
        // Clamp pozice
        let (mn, mx, mny, mxy) = compute_world_aabb(&t, mn_x, mx_x, mn_y, mx_y);
        if mn < pos_x + nd { t.gui_dx += pos_x + nd - mn; }
        if mx > pos_x + pos_w - nd { t.gui_dx -= mx - (pos_x + pos_w - nd); }
        if mny < pos_y + nd { t.gui_dy += pos_y + nd - mny; }
        if mxy > pos_y + pos_h - nd { t.gui_dy -= mxy - (pos_y + pos_h - nd); }
        // Zkontroluj po clampování
        let (wmin_x, wmax_x, wmin_y, wmax_y) = compute_world_aabb(&t, mn_x, mx_x, mn_y, mx_y);
        wmin_x >= pos_x + nd
            && wmax_x <= pos_x + pos_w - nd
            && wmin_y >= pos_y + nd
            && wmax_y <= pos_y + pos_h - nd
    };

    if fits(1.0) { return 1.0; }
    if !fits(0.0) { return 0.0; }
    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;
    for _ in 0..12 {
        let mid = (lo + hi) / 2.0;
        if fits(mid) { lo = mid; } else { hi = mid; }
    }
    lo
}

/// Binárním hledáním najde maximální měřítko ≤ wanted při němž trasa vejde do sklíčka
/// s anchor-based pozicí (anchor zůstane na místě).
/// Odpovídá funkci `maxFitScale` v Canvas2D.svelte.
#[wasm_bindgen]
pub fn wasm_max_fit_scale(
    start_gui_dx: f64, start_gui_dy: f64, start_scale: f64, start_rotation: f64, cx: f64, cy: f64,
    anc_x: f64, anc_y: f64, wanted_scale: f64,
    pos_x: f64, pos_y: f64, pos_w: f64, pos_h: f64,
    mn_x: f64, mx_x: f64, mn_y: f64, mx_y: f64,
    nozzle_diam: f64,
) -> f64 {
    // Zmenšování nikdy nepotřebuje clamp — trasa se jen smrskne k anchoru
    if wanted_scale <= start_scale {
        return f64::max(0.05, wanted_scale);
    }
    let nd = nozzle_diam / 2.0;
    let eps = 1e-4_f64;

    let fits = |s: f64| -> bool {
        let ratio = s / start_scale;
        let dx = anc_x - cx - ratio * (anc_x - start_gui_dx - cx);
        let dy = anc_y - cy - ratio * (anc_y - start_gui_dy - cy);
        let t = make_transform(dx, dy, s, start_rotation, cx, cy);
        let (min_x, max_x, min_y, max_y) = compute_world_aabb(&t, mn_x, mx_x, mn_y, mx_y);
        min_x >= pos_x + nd - eps
            && max_x <= pos_x + pos_w - nd + eps
            && min_y >= pos_y + nd - eps
            && max_y <= pos_y + pos_h - nd + eps
    };

    if fits(wanted_scale) { return wanted_scale; }
    let mut lo = 0.05_f64;
    let mut hi = wanted_scale;
    if !fits(lo) {
        lo = start_scale;
        if !fits(lo) { return lo; }
    }
    for _ in 0..12 {
        let mid = (lo + hi) / 2.0;
        if fits(mid) { lo = mid; } else { hi = mid; }
    }
    lo
}

// ─── Export držáku (bracket) ──────────────────────────────────────────────────
// Geometrie, SVG i STL pipeline žijí v `bracket.rs` — sem patří jen tenké
// wasm-bindgen obálky. Parametry/výsledky komplexních tvarů se přenášejí jako
// JSON (BracketParams/BracketGeometry derivují Serialize/Deserialize), aby se
// nemusely táhnout desítky jednotlivých argumentů přes hranici JS↔WASM.

fn parse_bracket_params(params_json: &str) -> Result<BracketParams, JsValue> {
    serde_json::from_str(params_json).map_err(|e| JsValue::from_str(&format!("Neplatné parametry držáku: {e}")))
}

/// Vypočítá kompletní geometrii držáku (cesty, obdélníky, středy děr, layout…)
/// — jediný zdroj pravdy pro živý náhled i export. Vrací JSON serializovanou
/// `BracketGeometry`.
#[wasm_bindgen]
pub fn wasm_bracket_geometry(params_json: &str) -> Result<String, JsValue> {
    let params = parse_bracket_params(params_json)?;
    let geometry = compute_bracket_geometry(&params);
    serde_json::to_string(&geometry).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Vygeneruje kompletní SVG export sestavy (stejná geometrie jako náhled).
#[wasm_bindgen]
pub fn wasm_bracket_svg(params_json: &str) -> Result<String, JsValue> {
    let params = parse_bracket_params(params_json)?;
    Ok(generate_bracket_svg(&params))
}

/// Sestaví binární STL z vyrastrované bitmapové masky průřezu (frontend ji
/// vyrastruje přes canvas/Path2D — viz drawCrossSectionForSTL, protože to
/// vyžaduje Path2D pro oblé řezy pružin). Odvození masky rozšířené části stěn,
/// greedy meshing, extruze i binární encoding proběhnou zde v Rustu.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen]
pub fn wasm_bracket_stl(
    mask: &[u8],
    cols: usize,
    rows: usize,
    cell_size: f64,
    origin_x: f64,
    origin_y: f64,
    left_wall_x: f64, left_wall_y: f64, left_wall_w: f64, left_wall_h: f64,
    bottom_wall_x: f64, bottom_wall_y: f64, bottom_wall_w: f64, bottom_wall_h: f64,
    wall_extend: f64,
    bracket_thickness: f64,
    wall_extra_height: f64,
) -> Vec<u8> {
    let left_wall_rect = RectF { x: left_wall_x, y: left_wall_y, w: left_wall_w, h: left_wall_h };
    let bottom_wall_rect = RectF { x: bottom_wall_x, y: bottom_wall_y, w: bottom_wall_w, h: bottom_wall_h };
    build_bracket_stl(
        mask, cols, rows, cell_size, origin_x, origin_y,
        left_wall_rect, bottom_wall_rect, wall_extend,
        bracket_thickness, wall_extra_height,
    )
}

/// Výšky pro 3D náhled (tloušťka skla, mezera, průřez housenky) — viz
/// `gcode::print_heights`. Vstup JSON `ProcessParams` (z_offset v mm) a mapa
/// `SlideOverride`, výstup JSON `{ "-1" | "0".. : SlideHeights }`.
#[wasm_bindgen]
pub fn wasm_print_heights(params_json: &str, overrides_json: &str, calibration_factor: f64) -> Result<String, JsValue> {
    let params: ProcessParams = serde_json::from_str(params_json)
        .map_err(|e| JsValue::from_str(&format!("Neplatné parametry: {e}")))?;
    let overrides: HashMap<String, SlideOverride> = serde_json::from_str(overrides_json)
        .map_err(|e| JsValue::from_str(&format!("Neplatné přepisy: {e}")))?;
    serde_json::to_string(&print_heights(&params, &overrides, calibration_factor))
        .map_err(|e| JsValue::from_str(&e.to_string()))
}
