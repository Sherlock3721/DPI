use crate::extrusion::ExtrusionCalculator;
use crate::layout::get_layout_positions;
use crate::types::{
    LayoutPosition, MachineConfig, PathSegment, Point2D, ProcessParams, SlideOverride,
    SubstratePaths, Transform,
};
use std::collections::HashMap;

/// Společný generátor cik-cak bodů odplivového (prime) vzoru.
///
/// Vrací posloupnost cílových bodů od startu (x1, y1); příznak u bodu říká,
/// zda jde o krátkou příčnou spojku mezi liniemi (`true`) nebo dlouhou linii.
/// Jediný zdroj pravdy pro náhled (`generate_prime_preview`)
/// i tiskový G-kód (`generate_gcode`).
fn prime_zigzag_waypoints(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    spacing: f64,
) -> Vec<(Point2D, bool)> {
    let mut waypoints: Vec<(Point2D, bool)> = Vec::new();
    if spacing <= 1e-9 || spacing.is_nan() {
        return waypoints; // ochrana proti nekonečné smyčce při nulovém rozestupu
    }
    let mut curr_y = y1;
    let mut direction = 1.0_f64;
    while curr_y <= y2 {
        let target_x = if direction > 0.0 { x2 } else { x1 };
        waypoints.push((Point2D::new(target_x, curr_y), false));
        curr_y += spacing;
        if curr_y <= y2 {
            waypoints.push((Point2D::new(target_x, curr_y), true));
        }
        direction *= -1.0;
    }
    waypoints
}

/// Generuje náhledové dráhy odplivové (prime) pozice pro canvas.
/// Logika odpovídá priming smyčce v `generate_gcode`.
pub fn generate_prime_preview(
    pos: &LayoutPosition,
    params: &ProcessParams,
    prime_override: Option<&SlideOverride>,
) -> SubstratePaths {
    let prime_w = prime_override.and_then(|o| o.slide_w).unwrap_or(15.0);
    let prime_h = prime_override.and_then(|o| o.slide_h).unwrap_or(15.0);

    let infill_type = prime_override
        .and_then(|o| o.infill_type.clone())
        .unwrap_or_else(|| params.infill_type.clone());
    let infill_val = prime_override
        .and_then(|o| o.infill_val)
        .unwrap_or(params.nozzle_diam);

    let spacing = if infill_type == "%" && infill_val > 0.0 {
        params.nozzle_diam / (infill_val / 100.0)
    } else if infill_type == "počet" && infill_val >= 1.0 {
        prime_h / infill_val
    } else if infill_val > 0.0 {
        infill_val
    } else {
        params.nozzle_diam
    };

    let glass_w = match prime_override.and_then(|o| o.glass_type.as_deref()) {
        Some("vzorkové") => params.slide_w,
        _ => pos.width,
    };
    let glass_h = match prime_override.and_then(|o| o.glass_type.as_deref()) {
        Some("vzorkové") => params.slide_h,
        _ => pos.height,
    };

    let cx = glass_w / 2.0;
    let cy = glass_h / 2.0;
    let x1 = cx - prime_w / 2.0;
    let x2 = cx + prime_w / 2.0;
    let y1 = cy - prime_h / 2.0;
    let y2 = cy + prime_h / 2.0;

    let waypoints = prime_zigzag_waypoints(x1, y1, x2, y2, spacing);
    if waypoints.is_empty() {
        return SubstratePaths::new(vec![]);
    }

    let mut points: Vec<Point2D> = Vec::with_capacity(waypoints.len() + 1);
    points.push(Point2D::new(x1, y1));
    points.extend(waypoints.into_iter().map(|(pt, _)| pt));
    SubstratePaths::new(vec![PathSegment::new(points)])
}

// ─── Parsování G-kód řádků (sdílené s tauri backendem) ──────────────────────

/// Vrátí `true` pokud řádek začíná daným příkazem (case-insensitive)
/// a NENÍ delším číselným kódem — tj. "G1 X5" ano, ale "G10" ne.
#[inline]
fn starts_with_cmd(line: &str, cmd: &str) -> bool {
    let l = line.trim_start();
    l.len() >= cmd.len()
        && l[..cmd.len()].eq_ignore_ascii_case(cmd)
        && l.as_bytes().get(cmd.len()).is_none_or(|b| !b.is_ascii_digit())
}

/// Vrátí `true` pokud je řádek lineární pohyb G0/G1 (nikoliv G10, G17…).
#[inline]
pub fn is_linear_move(line: &str) -> bool {
    starts_with_cmd(line, "G0") || starts_with_cmd(line, "G1")
}

/// Vrátí `true` pokud je řádek extruzní pohyb (G1 s parametrem E).
#[inline]
pub fn is_extrusion_move(line: &str) -> bool {
    starts_with_cmd(line, "G1") && parse_axis_value(line, 'E').is_some()
}

/// Extrahuje hodnotu osy (X/Y/Z/E…) z G-kód řádku, case-insensitive,
/// bez alokace paměti. Volající zodpovídá za odstranění komentářů.
#[inline]
pub fn parse_axis_value(line: &str, axis: char) -> Option<f64> {
    let upper = axis.to_ascii_uppercase();
    let lower = axis.to_ascii_lowercase();
    let idx = line.find(|c: char| c == upper || c == lower)?;
    let sub = &line[idx + 1..];
    let end = sub
        .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
        .unwrap_or(sub.len());
    sub[..end].parse().ok()
}

/// Parsuje souřadnice X/Y/Z z G0/G1 řádku.
/// Pro jiné příkazy (včetně G10/G11!) vrací (None, None, None).
pub fn parse_move_axes(line: &str) -> (Option<f64>, Option<f64>, Option<f64>) {
    if !is_linear_move(line) {
        return (None, None, None);
    }
    (
        parse_axis_value(line, 'X'),
        parse_axis_value(line, 'Y'),
        parse_axis_value(line, 'Z'),
    )
}

/// Extrahuje tiskové dráhy z G-kód textu (G0/G1 příkazy) pro vizualizaci.
pub fn parse_gcode_paths(gcode: &str) -> SubstratePaths {
    let mut segments: Vec<PathSegment> = Vec::new();
    let mut current_points: Vec<Point2D> = Vec::new();
    let mut cur_x = 0.0_f64;
    let mut cur_y = 0.0_f64;

    for line in gcode.lines() {
        let clean = line.split(';').next().unwrap_or("").trim();

        let is_g0 = starts_with_cmd(clean, "G0");
        let is_g1 = starts_with_cmd(clean, "G1");
        if !is_g0 && !is_g1 {
            continue;
        }

        let new_x = parse_axis_value(clean, 'X').unwrap_or(cur_x);
        let new_y = parse_axis_value(clean, 'Y').unwrap_or(cur_y);

        if is_g0 {
            if current_points.len() >= 2 {
                segments.push(PathSegment::new(std::mem::take(&mut current_points)));
            } else {
                current_points.clear();
            }
            current_points.push(Point2D::new(new_x, new_y));
        } else {
            if current_points.is_empty() {
                current_points.push(Point2D::new(cur_x, cur_y));
            }
            current_points.push(Point2D::new(new_x, new_y));
        }

        cur_x = new_x;
        cur_y = new_y;
    }

    if current_points.len() >= 2 {
        segments.push(PathSegment::new(current_points));
    }

    SubstratePaths::new(segments)
}

/// Pomocná funkce pro transformaci bodu z lokálních souřadnic sklíčka
/// do absolutních souřadnic tiskové plochy.
fn transform_pt(x_orig: f64, y_orig: f64, t: &Transform, _bed_max_y: f64) -> Point2D {
    let dx = x_orig - t.cx;
    let dy = y_orig - t.cy;

    let dx_scaled = dx * t.scale;
    let dy_scaled = dy * t.scale;

    // Záporná rotace — shodné s tpt() v Canvas2D.svelte (rad = -pRot)
    let rad = (-t.rotation).to_radians();
    let cos_r = rad.cos();
    let sin_r = rad.sin();
    let rx = dx_scaled * cos_r - dy_scaled * sin_r;
    let ry = dx_scaled * sin_r + dy_scaled * cos_r;

    let gui_x = t.gui_dx + t.cx + rx;
    let gui_y = t.gui_dy + t.cy + ry;

    Point2D::new(gui_x, gui_y)
}

/// Z-hop úměrný délce přejezdu — krátké přejezdy dostávají nižší hop.
fn proportional_z_hop(travel_dist: f64, max_hop: f64) -> f64 {
    const NO_HOP_MM: f64 = 0.5;   // pod touto délkou přejezdu hop = 0
    const FULL_HOP_MM: f64 = 5.0; // nad touto délkou přejezdu plný hop
    if travel_dist <= NO_HOP_MM {
        0.0
    } else if travel_dist >= FULL_HOP_MM {
        max_hop
    } else {
        max_hop * (travel_dist - NO_HOP_MM) / (FULL_HOP_MM - NO_HOP_MM)
    }
}

/// Pro uzavřený polygon rotuje body tak, aby první vrchol byl nejblíže aktuální pozici trysky.
/// Otevřené cesty vrací beze změny.
fn rotate_closed_path_to_nearest(
    points: &[Point2D],
    t: &Transform,
    bed_max_y: f64,
    cur_x: f64,
    cur_y: f64,
) -> Vec<Point2D> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let first = points[0];
    let last = *points.last().unwrap();
    if (first.x - last.x).powi(2) + (first.y - last.y).powi(2) > 1e-2 {
        return points.to_vec(); // otevřená cesta
    }
    let n = points.len() - 1; // počet unikátních vrcholů
    let best = (0..n)
        .min_by(|&i, &j| {
            let ai = transform_pt(points[i].x, points[i].y, t, bed_max_y);
            let aj = transform_pt(points[j].x, points[j].y, t, bed_max_y);
            let di = (ai.x - cur_x).hypot(ai.y - cur_y);
            let dj = (aj.x - cur_x).hypot(aj.y - cur_y);
            di.partial_cmp(&dj).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(0);
    if best == 0 {
        return points.to_vec();
    }
    let mut rotated = Vec::with_capacity(points.len());
    rotated.extend_from_slice(&points[best..n]);
    rotated.extend_from_slice(&points[0..best]);
    rotated.push(points[best]);
    rotated
}

/// Připojí G-kód blok do výstupního bufferu a zajistí ukončení novým řádkem.
/// Pokud je blok prázdný, nedělá nic.
#[inline]
fn push_gcode_block(out: &mut String, block: &str) {
    if block.is_empty() {
        return;
    }
    out.push_str(block);
    if !block.ends_with('\n') {
        out.push('\n');
    }
}

/// Hlavní generátor G-kódu pro laboratorní 2D tisk.
/// Vrací trojici: (obsah G-kódu, celková tisková dráha v mm, odhadovaný čas v sekundách).
pub fn generate_gcode(
    slide_paths: &[SubstratePaths],
    params: &ProcessParams,
    transforms: &[Transform],
    slide_overrides: &HashMap<String, SlideOverride>,
    machine: &MachineConfig,
) -> Result<(String, f64, f64), String> {
    let ext_calc = ExtrusionCalculator::new(
        params.filament_diameter,
        params.flow_multiplier,
        Some(machine.calibration_factor),
    );

    // Spočítáme absolutní výšku trysky (včetně tloušťky skla a lokálního Z-offsetu)
    let get_abs_z = |z_off: f64, nz_h: f64, nz_hid: f64, sl_z: f64| -> f64 {
        -machine.block_height + nz_h - nz_hid + sl_z + z_off
    };

    // Najdeme nejnižší potřebnou Z-výšku přes všechna sklíčka a jejich override
    let mut min_needed_z = get_abs_z(
        params.z_offset,
        params.nozzle_height,
        params.nozzle_hidden,
        params.slide_z,
    );

    for m_idx in 0..params.sample_count {
        if let Some(ovr) = slide_overrides.get(&m_idx.to_string()) {
            let loc_z = ovr.z_offset.unwrap_or(params.z_offset);
            let loc_nz_h = ovr.nozzle_height.unwrap_or(params.nozzle_height);
            let val = get_abs_z(loc_z, loc_nz_h, params.nozzle_hidden, params.slide_z);
            if val < min_needed_z {
                min_needed_z = val;
            }
        }
    }

    // Pokud tryska potřebuje jet pod fyzický limit tiskárny (Z < 0), posuneme vše o z_shift
    let z_shift = if min_needed_z < 0.0 {
        min_needed_z.abs() + 1.0
    } else {
        0.0
    };

    // Předalokujeme buffer — průměrný G-kód tiskové dráhy bývá desítky až stovky kB
    let mut result = String::with_capacity(64 * 1024);

    result.push_str("G21 ; Nastaveni jednotek na milimetry\n");
    push_gcode_block(&mut result, &machine.start_gcode);

    if z_shift > 0.0 {
        result.push_str(&format!(
            "; --- VIRTUALNI POSUN Z (SHIFT {z_shift:.2}mm) ---\n\
             G1 Z{:.3} F1000 ; Vyjezd do bezpecne vysky\n\
             G92 Z{:.3} ; Nastaveni posunute nuly\n\n",
            machine.safe_z,
            machine.safe_z + z_shift
        ));
    }

    let mut total_time_sec = 0.0_f64;
    // Bezpečnostní strop teploty podložky — nikdy neposíláme M140/M190
    // nad nakonfigurované (či konzervativní výchozí) maximum.
    let bed_temp = params.bed_temp.clamp(0.0, machine.bed_max_temp.unwrap_or(110.0));
    if bed_temp > 0.0 {
        result.push_str(&format!(
            "M140 S{bed_temp:.0} ; Zacit nahrivat podlozku\n\
             M190 S{bed_temp:.0} ; Pockat na nahrati podlozky\n"
        ));
        total_time_sec += 60.0; // Přibližně 1 minuta na nahřátí
    }

    // Získání rozložení sklíček na podložce
    let prime_glass_type = slide_overrides.get("-1").and_then(|o| o.glass_type.as_deref());
    let positions = get_layout_positions(
        params.sample_count,
        params.slide_w,
        params.slide_h,
        machine.multi_spacing,
        params.prime_active,
        prime_glass_type,
        &machine.bed,
    );

    let mut measurement_idx = 0;
    let mut last_abs_x = 0.0_f64;
    let mut last_abs_y = 0.0_f64;
    let mut total_dist = 0.0_f64;

    for pos in &positions {
        push_gcode_block(&mut result, &machine.loop_start_gcode);

        result.push_str("G90 ; Absolutni souradnice pohybu\nM83 ; Relativni souradnice extruze\n");

        // Načteme lokální override pro dané sklíčko
        let key = if pos.is_prime {
            "-1".to_string()
        } else {
            measurement_idx.to_string()
        };
        let current_overrides = slide_overrides.get(&key);

        let loc_z = current_overrides
            .and_then(|o| o.z_offset)
            .unwrap_or(params.z_offset);
        let loc_nozzle_h = current_overrides
            .and_then(|o| o.nozzle_height)
            .unwrap_or(params.nozzle_height);
        let loc_ext = current_overrides
            .and_then(|o| o.extrusion_rate)
            .unwrap_or(params.extrusion_rate);
        let loc_ext_unit = current_overrides
            .and_then(|o| o.extrusion_unit.as_ref())
            .unwrap_or(&params.extrusion_unit);
        let loc_spd = current_overrides
            .and_then(|o| o.print_speed)
            .unwrap_or(params.print_speed);
        let loc_infill_style = current_overrides
            .and_then(|o| o.infill_style.as_ref())
            .unwrap_or(&params.infill_style);

        let print_z = -machine.block_height + loc_nozzle_h - params.nozzle_hidden
            + params.slide_z
            + loc_z
            + z_shift;

        let e_per_mm = ext_calc.calculate_e_per_mm(loc_ext, loc_ext_unit);

        if pos.is_prime {
            result.push_str("\n; --- VZOREK (ODPLIV) ---\n");
            result.push_str("G92 E0.0 ; Reset extruderu\n");

            // Konfigurovatelný odplivový vzor
            let prime_w = current_overrides.and_then(|o| o.slide_w).unwrap_or(15.0);
            let prime_h = current_overrides.and_then(|o| o.slide_h).unwrap_or(15.0);

            let infill_type = current_overrides
                .and_then(|o| o.infill_type.clone())
                .unwrap_or_else(|| params.infill_type.clone());
            let infill_val = current_overrides
                .and_then(|o| o.infill_val)
                .unwrap_or(params.nozzle_diam);

            // Rozestup odplivových linií — zrcadlí logiku generate_prime_preview
            let prime_infill = if infill_type == "%" && infill_val > 0.0 {
                params.nozzle_diam / (infill_val / 100.0)
            } else if infill_type == "počet" && infill_val >= 1.0 {
                prime_h / infill_val
            } else if infill_val > 0.0 {
                infill_val
            } else {
                params.nozzle_diam
            };

            let cx = pos.x + pos.width / 2.0;
            let cy = pos.y + pos.height / 2.0;
            let x1 = cx - prime_w / 2.0;
            let x2 = cx + prime_w / 2.0;
            let y1 = cy - prime_h / 2.0;
            let y2 = cy + prime_h / 2.0;

            let travel_dist = ((x1 - last_abs_x).powi(2) + (y1 - last_abs_y).powi(2)).sqrt();
            total_time_sec += (travel_dist / 3000.0) * 60.0;

            result.push_str(&format!(
                "G1 Z{:.3} F1000 ; Z-hop pro odpliv\nG0 X{:.3} Y{:.3} F3000\nG1 Z{:.3} F1000\n",
                print_z + machine.z_hop,
                x1,
                y1,
                print_z
            ));

            // Sdílený cik-cak generátor — stejné body jako generate_prime_preview.
            // Dlouhá linie extruduje podle své délky, příčná spojka podle průměru
            // trysky (parita s původní Python implementací).
            let line_dist = (x2 - x1).abs();
            for (pt, is_connector) in prime_zigzag_waypoints(x1, y1, x2, y2, prime_infill) {
                let dist = if is_connector { params.nozzle_diam } else { line_dist };
                result.push_str(&format!(
                    "G1 X{:.3} Y{:.3} E{:.5} F{:.0}\n",
                    pt.x,
                    pt.y,
                    dist * e_per_mm,
                    loc_spd
                ));
                total_dist += dist;
                total_time_sec += (dist / loc_spd) * 60.0;
                last_abs_x = pt.x;
                last_abs_y = pt.y;
            }
            result.push_str(&format!("G0 Z{:.3} F1000\n", print_z + machine.z_hop));
        } else {
            result.push_str(&format!("\n; --- VZOREK {} ---\n", measurement_idx + 1));
            result.push_str("G92 E0.0 ; Reset extruderu\n");

            if measurement_idx < slide_paths.len() {
                let paths = &slide_paths[measurement_idx];
                let transform = if measurement_idx < transforms.len() {
                    transforms[measurement_idx]
                } else {
                    Transform {
                        scale: 1.0,
                        rotation: 0.0,
                        gui_dx: pos.x,
                        gui_dy: pos.y,
                        cx: params.slide_w / 2.0,
                        cy: params.slide_h / 2.0,
                    }
                };

                for segment in &paths.segments {
                    if segment.is_empty() {
                        continue;
                    }

                    // Opt 1: pro uzavřené polygony rotuj počáteční bod k trysce
                    let effective_pts = rotate_closed_path_to_nearest(
                        &segment.points,
                        &transform,
                        machine.bed.max_y,
                        last_abs_x,
                        last_abs_y,
                    );

                    let p0 = effective_pts[0];
                    let abs_p0 = transform_pt(p0.x, p0.y, &transform, machine.bed.max_y);

                    let travel_dist =
                        ((abs_p0.x - last_abs_x).powi(2) + (abs_p0.y - last_abs_y).powi(2)).sqrt();
                    total_time_sec += (travel_dist / 3000.0) * 60.0;
                    last_abs_x = abs_p0.x;
                    last_abs_y = abs_p0.y;

                    // Opt 2: Z-hop úměrný délce přejezdu
                    let hop = proportional_z_hop(travel_dist, machine.z_hop);

                    // Tečkování (Dot Dispensing)
                    if loc_infill_style == "Tečky"
                        && effective_pts.len() == 2
                        && effective_pts[0] == effective_pts[1]
                    {
                        let dot_e = ext_calc.calculate_dot_extrusion(loc_ext, loc_ext_unit);
                        if hop > 0.0 {
                            result.push_str(&format!(
                                "G1 Z{:.3} F1000 ; Z-hop nad bod\n\
                                 G0 X{:.3} Y{:.3} F3000\n\
                                 G1 Z{:.3} F1000 ; Klesnuti k povrchu\n",
                                print_z + hop, abs_p0.x, abs_p0.y, print_z
                            ));
                        } else {
                            result.push_str(&format!(
                                "G0 X{:.3} Y{:.3} F3000\n",
                                abs_p0.x, abs_p0.y
                            ));
                        }
                        result.push_str(&format!(
                            "G1 E{:.5} F300 ; Davkovani kapky\n\
                             G1 Z{:.3} F1000 ; Z-hop po davkovani\n",
                            dot_e,
                            print_z + machine.z_hop
                        ));
                        total_time_sec += 2.0;
                        continue;
                    }

                    // Normální čáry (vektory)
                    if hop > 0.0 {
                        result.push_str(&format!(
                            "G1 Z{:.3} F1000 ; Z-hop pro prejezd\n\
                             G0 X{:.3} Y{:.3} F3000\n\
                             G1 Z{:.3} F1000 ; Sjezd k povrchu\n",
                            print_z + hop, abs_p0.x, abs_p0.y, print_z
                        ));
                    } else {
                        result.push_str(&format!(
                            "G0 X{:.3} Y{:.3} F3000 ; Kratky prejezd bez Z-hopu\n",
                            abs_p0.x, abs_p0.y
                        ));
                    }

                    for window in effective_pts.windows(2) {
                        let pa =
                            transform_pt(window[0].x, window[0].y, &transform, machine.bed.max_y);
                        let pb =
                            transform_pt(window[1].x, window[1].y, &transform, machine.bed.max_y);
                        let dist = ((pb.x - pa.x).powi(2) + (pb.y - pa.y).powi(2)).sqrt();

                        result.push_str(&format!(
                            "G1 X{:.3} Y{:.3} E{:.5} F{:.0}\n",
                            pb.x, pb.y, dist * e_per_mm, loc_spd
                        ));
                        total_dist += dist;
                        total_time_sec += (dist / loc_spd) * 60.0;
                        last_abs_x = pb.x;
                        last_abs_y = pb.y;
                    }
                }

                result.push_str(&format!(
                    "G1 Z{:.3} F1000 ; Zvednuti po tisku sklicka\n",
                    print_z + machine.z_hop
                ));
            }

            measurement_idx += 1;
        }

        push_gcode_block(&mut result, &machine.loop_end_gcode);
    }

    if bed_temp > 0.0 {
        result.push_str("M140 S0 ; Vypnout vyhrivani podlozky\n");
    }
    push_gcode_block(&mut result, &machine.end_gcode);

    Ok((result, total_dist, total_time_sec))
}

// ─── Testy ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_move_axes_basic() {
        let (x, y, z) = parse_move_axes("G1 X100.5 Y50.2 Z2.0 F1500");
        assert_eq!(x, Some(100.5));
        assert_eq!(y, Some(50.2));
        assert_eq!(z, Some(2.0));

        let (x2, y2, z2) = parse_move_axes("g0 x-10.0 y-20.5 z-1.2");
        assert_eq!(x2, Some(-10.0));
        assert_eq!(y2, Some(-20.5));
        assert_eq!(z2, Some(-1.2));
    }

    #[test]
    fn test_parse_move_axes_ignores_non_moves() {
        // G10 (retract) a G17 nesmí být parsovány jako pohyb — regrese
        assert_eq!(parse_move_axes("G10 X5"), (None, None, None));
        assert_eq!(parse_move_axes("G11"), (None, None, None));
        assert_eq!(parse_move_axes("G17"), (None, None, None));
        assert_eq!(parse_move_axes("M117 X stav"), (None, None, None));
    }

    #[test]
    fn test_is_extrusion_move() {
        assert!(is_extrusion_move("G1 X10 Y5 E0.5 F600"));
        assert!(!is_extrusion_move("G1 X10 Y5 F600"));
        assert!(!is_extrusion_move("G0 X10 E5"));
        assert!(!is_extrusion_move("G10 E5"));
    }

    #[test]
    fn test_is_linear_move() {
        assert!(is_linear_move("G0 X1"));
        assert!(is_linear_move("G1 X1"));
        assert!(is_linear_move("  g1 x1"));
        assert!(!is_linear_move("G10"));
        assert!(!is_linear_move("G28"));
    }
}
