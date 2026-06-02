use crate::extrusion::ExtrusionCalculator;
use crate::layout::get_layout_positions;
use crate::types::{
    LayoutPosition, MachineConfig, PathSegment, Point2D, ProcessParams, SlideOverride,
    SubstratePaths, Transform,
};
use std::collections::HashMap;

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

    let cx = pos.width / 2.0;
    let cy = pos.height / 2.0;
    let x1 = cx - prime_w / 2.0;
    let x2 = cx + prime_w / 2.0;
    let y1 = cy - prime_h / 2.0;
    let y2 = cy + prime_h / 2.0;

    let mut points: Vec<Point2D> = Vec::new();
    let mut curr_y = y1;
    let mut direction = 1.0_f64;

    while curr_y <= y2 {
        let target_x = if direction > 0.0 { x2 } else { x1 };
        let src_x = if direction > 0.0 { x1 } else { x2 };
        points.push(Point2D::new(src_x, curr_y));
        points.push(Point2D::new(target_x, curr_y));
        curr_y += spacing;
        if curr_y <= y2 {
            points.push(Point2D::new(target_x, curr_y));
        }
        direction *= -1.0;
    }

    if points.is_empty() {
        SubstratePaths::new(vec![])
    } else {
        SubstratePaths::new(vec![PathSegment::new(points)])
    }
}

/// Extrahuje tiskové dráhy z G-kód textu (G0/G1 příkazy) pro vizualizaci.
pub fn parse_gcode_paths(gcode: &str) -> SubstratePaths {
    let mut segments: Vec<PathSegment> = Vec::new();
    let mut current_points: Vec<Point2D> = Vec::new();
    let mut cur_x = 0.0_f64;
    let mut cur_y = 0.0_f64;

    for line in gcode.lines() {
        let clean = line.split(';').next().unwrap_or("").to_uppercase();
        let clean = clean.trim();

        let is_g0 = clean.starts_with("G0")
            && clean.as_bytes().get(2).map_or(true, |b| !b.is_ascii_digit());
        let is_g1 = clean.starts_with("G1")
            && clean.as_bytes().get(2).map_or(true, |b| !b.is_ascii_digit());
        if !is_g0 && !is_g1 {
            continue;
        }

        let new_x = extract_gcode_coord(clean, 'X').unwrap_or(cur_x);
        let new_y = extract_gcode_coord(clean, 'Y').unwrap_or(cur_y);

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

fn extract_gcode_coord(line: &str, axis: char) -> Option<f64> {
    let pos = line.find(axis)?;
    let rest = &line[pos + 1..];
    let end = rest
        .find(|c: char| c != '-' && c != '.' && !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Pomocná funkce pro transformaci bodu z lokálních souřadnic sklíčka
/// do absolutních souřadnic tiskové plochy.
fn transform_pt(x_orig: f64, y_orig: f64, t: &Transform, _bed_max_y: f64) -> Point2D {
    // Vektor vůči středu transformace (cx, cy)
    let dx = x_orig - t.cx;
    let dy = t.cy - y_orig; // Invertováno pro shodu s GUI (Y+ je dozadu)

    // Aplikace měřítka
    let dx_scaled = dx * t.scale;
    let dy_scaled = dy * t.scale;

    // Aplikace rotace ve stupních
    let rad = t.rotation.to_radians();
    let cos_r = rad.cos();
    let sin_r = rad.sin();
    let rx = dx_scaled * cos_r - dy_scaled * sin_r;
    let ry = dx_scaled * sin_r + dy_scaled * cos_r;

    // Absolutní pozice v GUI souřadnicích
    let gui_x = t.gui_dx + t.cx + rx;
    let gui_y = t.gui_dy + t.cy + ry;

    Point2D::new(gui_x, gui_y)
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
    if params.bed_temp > 0.0 {
        result.push_str(&format!(
            "M140 S{:.0} ; Zacit nahrivat podlozku\n\
             M190 S{:.0} ; Pockat na nahrati podlozky\n",
            params.bed_temp, params.bed_temp
        ));
        total_time_sec += 60.0; // Přibližně 1 minuta na nahřátí
    }

    // Získání rozložení sklíček na podložce
    let positions = get_layout_positions(
        params.sample_count,
        params.slide_w,
        params.slide_h,
        machine.multi_spacing,
        params.prime_active,
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

            let mut curr_y = y1;
            let mut direction = 1.0_f64;
            let mut last_prime_y = y1;
            while curr_y <= y2 {
                let target_x = if direction > 0.0 { x2 } else { x1 };
                let dist = (x2 - x1).abs();
                result.push_str(&format!(
                    "G1 X{:.3} Y{:.3} E{:.5} F{:.0}\n",
                    target_x,
                    curr_y,
                    dist * e_per_mm,
                    loc_spd
                ));
                total_dist += dist;
                total_time_sec += (dist / loc_spd) * 60.0;
                last_prime_y = curr_y;

                curr_y += prime_infill;
                if curr_y <= y2 {
                    result.push_str(&format!(
                        "G1 X{:.3} Y{:.3} E{:.5} F{:.0}\n",
                        target_x,
                        curr_y,
                        params.nozzle_diam * e_per_mm,
                        loc_spd
                    ));
                    total_dist += params.nozzle_diam;
                    total_time_sec += (params.nozzle_diam / loc_spd) * 60.0;
                    last_prime_y = curr_y;
                }
                direction *= -1.0;
                last_abs_x = target_x;
            }
            last_abs_y = last_prime_y;
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

                let mut is_retracted = false;

                for segment in &paths.segments {
                    if segment.is_empty() {
                        continue;
                    }

                    let p0 = segment.points[0];
                    let abs_p0 = transform_pt(p0.x, p0.y, &transform, machine.bed.max_y);

                    // Přejezd k začátku dráhy
                    let travel_dist =
                        ((abs_p0.x - last_abs_x).powi(2) + (abs_p0.y - last_abs_y).powi(2)).sqrt();
                    total_time_sec += (travel_dist / 3000.0) * 60.0;
                    last_abs_x = abs_p0.x;
                    last_abs_y = abs_p0.y;

                    // Tečkování (Dot Dispensing)
                    if loc_infill_style == "Tečky"
                        && segment.points.len() == 2
                        && segment.points[0] == segment.points[1]
                    {
                        let dot_e = ext_calc.calculate_dot_extrusion(loc_ext, loc_ext_unit);
                        result.push_str(&format!(
                            "G1 Z{:.3} F1000 ; Z-hop nad bod\n\
                             G0 X{:.3} Y{:.3} F3000\n\
                             G1 Z{:.3} F1000 ; Klesnuti k povrchu\n\
                             G1 E{:.5} F300 ; Davkovani kapky\n\
                             G1 Z{:.3} F1000 ; Z-hop po davkovani\n",
                            print_z + machine.z_hop,
                            abs_p0.x,
                            abs_p0.y,
                            print_z,
                            dot_e,
                            print_z + machine.z_hop
                        ));
                        total_time_sec += 2.0; // Odhadovaný čas na jednu kapku
                        continue;
                    }

                    // Normální čáry (vektory)
                    result.push_str(&format!(
                        "G1 Z{:.3} F1000 ; Z-hop pro prejezd\n\
                         G0 X{:.3} Y{:.3} F3000\n\
                         G1 Z{:.3} F1000 ; Sjezd k povrchu\n",
                        print_z + machine.z_hop,
                        abs_p0.x,
                        abs_p0.y,
                        print_z
                    ));

                    if is_retracted && machine.retraction > 0.0 {
                        result.push_str(&format!(
                            "G1 E{:.5} F{:.0} ; Deretrakce\n",
                            machine.retraction, machine.retract_speed
                        ));
                        is_retracted = false;
                    }

                    for window in segment.points.windows(2) {
                        let pa =
                            transform_pt(window[0].x, window[0].y, &transform, machine.bed.max_y);
                        let pb =
                            transform_pt(window[1].x, window[1].y, &transform, machine.bed.max_y);
                        let dist = ((pb.x - pa.x).powi(2) + (pb.y - pa.y).powi(2)).sqrt();

                        result.push_str(&format!(
                            "G1 X{:.3} Y{:.3} E{:.5} F{:.0}\n",
                            pb.x,
                            pb.y,
                            dist * e_per_mm,
                            loc_spd
                        ));
                        total_dist += dist;
                        total_time_sec += (dist / loc_spd) * 60.0;
                        last_abs_x = pb.x;
                        last_abs_y = pb.y;
                    }

                    if machine.retraction > 0.0 {
                        result.push_str(&format!(
                            "G1 E{:.5} F{:.0} ; Retrakce\n",
                            -machine.retraction, machine.retract_speed
                        ));
                        is_retracted = true;
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

    if params.bed_temp > 0.0 {
        result.push_str("M140 S0 ; Vypnout vyhrivani podlozky\n");
    }
    push_gcode_block(&mut result, &machine.end_gcode);

    Ok((result, total_dist, total_time_sec))
}
