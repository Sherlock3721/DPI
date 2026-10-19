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
use crate::geometry::fit_transform_to_position;
use crate::path_processing::process_substrate_paths;
use crate::types::{
    Placement, PlacementState, ProcessParams, SceneObject, SliceParams, SlideOverride,
    SubstratePaths,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Výsledek `update_layout` — kompletní data pro překreslení plátna.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutUpdateResult {
    pub positions: Vec<LayoutPosition>,
    /// Umístění objektů: `slides[i][k]` patří k `objects[k]` — každé sklíčko nese
    /// položku pro každý objekt, skryté mají `enabled == false`.
    pub slides: Vec<Vec<Placement>>,
    pub prime_path: Option<SubstratePaths>,
    /// Počet vzorků po omezení kapacitou podložky.
    pub final_sample_count: usize,
}

/// Kompletní přepočet layoutu: kapacita podložky → zpracování drah všech
/// objektů na všech sklíčkách → pozice → přizpůsobení transformací → náhled
/// odplivu. Jediný IPC roundtrip pro celé překreslení plátna.
///
/// Objekt, který na sklíčku ještě umístění nemá (nově přidaný objekt nebo nově
/// přidaný substrát), ho zdědí od prvního sklíčka, které ho má — proto se každý
/// import automaticky objeví na všech sklech se stejným relativním posunem.
#[allow(clippy::too_many_arguments)]
pub fn update_layout(
    params: &ProcessParams,
    overrides: &HashMap<String, SlideOverride>,
    objects: &[SceneObject],
    slides: &[Vec<PlacementState>],
    old_positions: &[LayoutPosition],
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

    // 2. Nové pozice sklíček
    let positions = get_layout_positions(
        final_sample_count,
        params.slide_w,
        params.slide_h,
        multi_spacing,
        params.prime_active,
        prime_glass_type,
        bed,
    );
    let non_prime: Vec<LayoutPosition> =
        positions.iter().filter(|p| !p.is_prime).copied().collect();
    let old_non_prime: Vec<LayoutPosition> =
        old_positions.iter().filter(|p| !p.is_prime).copied().collect();

    // Pro dědění umístění: první sklíčko, kde je objekt VIDITELNÝ (jinak první,
    // které ho vůbec zná). Nový substrát tak objekt dostane zobrazený, i když
    // je na prvním sklíčku zrovna skrytý.
    let template_of = |object_id: &str| -> Option<(usize, &PlacementState)> {
        let find = |want_enabled: bool| {
            slides.iter().enumerate().find_map(|(i, row)| {
                row.iter()
                    .find(|p| p.object_id == object_id && (!want_enabled || p.enabled))
                    .map(|p| (i, p))
            })
        };
        find(true).or_else(|| find(false))
    };

    // 3. Zpracování drah pro každé sklíčko × objekt
    let mut out_slides: Vec<Vec<Placement>> = Vec::with_capacity(final_sample_count);
    for i in 0..final_sample_count {
        let pos = match non_prime.get(i) {
            Some(p) => p,
            None => break,
        };
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

        let mut row: Vec<Placement> = Vec::with_capacity(objects.len());
        for obj in objects {
            // Stav umístění: vlastní → zděděný od prvního sklíčka s objektem → nový
            let own = slides
                .get(i)
                .and_then(|r| r.iter().find(|p| p.object_id == obj.id));
            let (src_slide_idx, state) = match own {
                Some(p) => (Some(i), Some(p)),
                None => match template_of(&obj.id) {
                    Some((j, p)) => (Some(j), Some(p)),
                    None => (None, None),
                },
            };

            let enabled = state.map(|s| s.enabled).unwrap_or(true);
            let baked_scale = state.map(|s| s.baked_scale).unwrap_or(1.0);
            let nozzle_type = state.and_then(|s| s.nozzle_type.clone());
            let nozzle_override = state.and_then(|s| s.nozzle_diam);
            let eff_nozzle = nozzle_override.filter(|d| *d > 0.0).unwrap_or(nozzle_diam);
            let rotation = state.map(|s| s.transform.rotation).unwrap_or(0.0);

            // Skrytý objekt trasu nepotřebuje — výplň se přepočítá, až se zapne
            let path = if enabled {
                let slice = SliceParams {
                    slide_w: params.slide_w,
                    slide_h: params.slide_h,
                    margin: 2.0,
                    auto_scale: obj.auto_scale,
                    infill_style: infill_style.clone(),
                    infill_val,
                    infill_type: infill_type.clone(),
                    infill_angle: params.infill_angle + rotation,
                    nozzle_diam: eff_nozzle,
                    user_scale: baked_scale,
                    frame: Some(obj.frame),
                };
                process_substrate_paths(&obj.raw, &slice)
            } else {
                SubstratePaths::new(vec![])
            };

            // Relativní offset se počítá vůči sklíčku, ze kterého stav pochází —
            // vždy vůči jeho PŘEDCHOZÍ pozici, protože v ní byl transform zapsán.
            let old_pos = src_slide_idx.and_then(|j| old_non_prime.get(j));
            let transform = fit_transform_to_position(
                pos,
                old_pos,
                state.map(|s| &s.transform),
                Some(&path),
                eff_nozzle,
            );

            row.push(Placement {
                object_id: obj.id.clone(),
                enabled,
                transform,
                baked_scale,
                nozzle_type,
                nozzle_diam: nozzle_override,
                path,
            });
        }
        out_slides.push(row);
    }

    // 4. Náhled odplivové pozice
    let prime_path = if params.prime_active {
        positions
            .iter()
            .find(|p| p.is_prime)
            .map(|p| generate_prime_preview(p, params, overrides.get("-1")))
    } else {
        None
    };

    LayoutUpdateResult {
        positions,
        slides: out_slides,
        prime_path,
        final_sample_count,
    }
}
