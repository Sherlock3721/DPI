use crate::types::{BedConfig, LayoutPosition};

/// Vypočítá absolutní pozice pro multiplexní uspořádání sklíček na podložce.
/// Algoritmus začíná vlevo vzadu (bed_min_x + start_offset_x)
/// a skládá sklíčka směrem doprava (X+) a dozadu (Y+).
pub fn get_layout_positions(
    count: usize,
    slide_w: f64,
    slide_h: f64,
    spacing: f64,
    prime_active: bool,
    prime_glass_type: Option<&str>,
    bed: &BedConfig,
) -> Vec<LayoutPosition> {
    let mut positions = Vec::new();

    let mut curr_y = bed.offset_y;
    let mut curr_x = bed.min_x + bed.offset_x;

    let p_w = match prime_glass_type {
        Some("vzorkové") => slide_w,
        _ => 76.0,
    };
    let p_h = match prime_glass_type {
        Some("vzorkové") => slide_h,
        _ => 26.0,
    };

    // Šířka aktuálního sloupce (první sloupec může být širší kvůli odplivovému sklu)
    let mut current_col_w = if prime_active {
        slide_w.max(p_w)
    } else {
        slide_w
    };

    if prime_active {
        positions.push(LayoutPosition {
            x: curr_x,
            y: curr_y,
            width: p_w,
            height: p_h,
            is_prime: true,
        });
        curr_y += p_h + spacing;
    }

    for _ in 0..count {
        // Pokud sklíčko přeteče maximální výšku podložky, posuneme se doprava na nový sloupec
        if curr_y + slide_h > bed.max_y && !positions.is_empty() {
            curr_x += current_col_w + spacing;
            current_col_w = slide_w; // Další sloupce již nemají prime sklíčko
            curr_y = bed.offset_y;
        }

        // Sklíčko se nevejde ani po přesunu sloupce — netiskneme nic dalšího
        if curr_y + slide_h > bed.max_y {
            break;
        }

        // Pokud bychom přetekli přes pravou hranici, zastavíme rozložení
        if curr_x + slide_w > bed.max_x {
            break;
        }

        positions.push(LayoutPosition {
            x: curr_x,
            y: curr_y,
            width: slide_w,
            height: slide_h,
            is_prime: false,
        });
        curr_y += slide_h + spacing;
    }

    positions
}

// ─── Kompletní aktualizace layoutu (jediné volání z frontendu) ───────────────

use crate::gcode::generate_prime_preview;
use crate::geometry::fit_transforms_to_layout;
use crate::path_processing::process_substrate_paths;
use crate::types::{ProcessParams, SliceParams, SlideOverride, SubstratePaths, Transform};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Výsledek `update_layout` — kompletní data pro překreslení plátna.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutUpdateResult {
    pub positions: Vec<LayoutPosition>,
    pub transforms: Vec<Transform>,
    pub paths: Vec<SubstratePaths>,
    pub prime_path: Option<SubstratePaths>,
    /// Počet vzorků po omezení kapacitou podložky.
    pub final_sample_count: usize,
    /// Zapečená měřítka zarovnaná na final_sample_count (nová sklíčka = 1.0).
    pub baked_scales: Vec<f64>,
}

/// Kompletní přepočet layoutu: kapacita podložky → zpracování drah všech
/// sklíček → pozice → přizpůsobení transformací → náhled odplivu.
///
/// Nahrazuje sekvenci `calculate_slide_layout(100…)` + N× `process_paths` +
/// `recalculate_layout` + `get_prime_preview` v `projectStore.doUpdateLayout`,
/// tedy jediný IPC roundtrip místo 3 + N.
#[allow(clippy::too_many_arguments)]
pub fn update_layout(
    params: &ProcessParams,
    overrides: &HashMap<String, SlideOverride>,
    raw_paths: Option<&SubstratePaths>,
    auto_scale: bool,
    baked_scales: &[f64],
    old_positions: &[LayoutPosition],
    current_transforms: &[Transform],
    bed: &BedConfig,
    multi_spacing: f64,
) -> LayoutUpdateResult {
    let prime_glass_type = overrides.get("-1").and_then(|o| o.glass_type.as_deref());
    let nozzle_diam = if params.nozzle_diam > 0.0 { params.nozzle_diam } else { 0.4 };

    // 1. Kapacita podložky — kolik vzorků se maximálně vejde
    let capacity = get_layout_positions(
        100,
        params.slide_w,
        params.slide_h,
        multi_spacing,
        params.prime_active,
        prime_glass_type,
        bed,
    )
    .iter()
    .filter(|p| !p.is_prime)
    .count()
    .max(1);
    let final_sample_count = params.sample_count.min(capacity);

    // 2. Zpracování drah pro každé sklíčko (zachovává zapečené měřítko a rotaci)
    let mut paths: Vec<SubstratePaths> = Vec::new();
    if let Some(raw) = raw_paths {
        for i in 0..final_sample_count {
            let ovr = overrides.get(&i.to_string());
            let infill_style = ovr
                .and_then(|o| o.infill_style.as_deref())
                .filter(|s| !s.is_empty())
                .unwrap_or(&params.infill_style)
                .to_string();
            let infill_val = ovr.and_then(|o| o.infill_val).unwrap_or(params.infill_val);
            let infill_type = ovr
                .and_then(|o| o.infill_type.clone())
                .unwrap_or_else(|| params.infill_type.clone());
            let current_rot = current_transforms.get(i).map(|t| t.rotation).unwrap_or(0.0);
            let slice = SliceParams {
                slide_w: params.slide_w,
                slide_h: params.slide_h,
                margin: 2.0,
                auto_scale,
                infill_style,
                infill_val,
                infill_type,
                infill_angle: params.infill_angle + current_rot,
                nozzle_diam,
                user_scale: baked_scales.get(i).copied().unwrap_or(1.0),
            };
            paths.push(process_substrate_paths(raw, &slice));
        }
    }

    // 3. Nové pozice a přizpůsobené transformace
    let positions = get_layout_positions(
        final_sample_count,
        params.slide_w,
        params.slide_h,
        multi_spacing,
        params.prime_active,
        prime_glass_type,
        bed,
    );
    let old_non_prime: Vec<LayoutPosition> =
        old_positions.iter().filter(|p| !p.is_prime).copied().collect();
    let transforms = fit_transforms_to_layout(
        &positions,
        &old_non_prime,
        current_transforms,
        &paths,
        nozzle_diam,
    );

    // 4. Náhled odplivové pozice
    let prime_path = if params.prime_active {
        positions
            .iter()
            .find(|p| p.is_prime)
            .map(|p| generate_prime_preview(p, params, overrides.get("-1")))
    } else {
        None
    };

    // 5. Zapečená měřítka zarovnaná na nový počet sklíček
    let baked_scales_out = (0..final_sample_count)
        .map(|i| baked_scales.get(i).copied().unwrap_or(1.0))
        .collect();

    LayoutUpdateResult {
        positions,
        transforms,
        paths,
        prime_path,
        final_sample_count,
        baked_scales: baked_scales_out,
    }
}
