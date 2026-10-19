use crate::extrusion::ExtrusionCalculator;
use crate::geometry::clip_paths_to_slide;
use crate::layout::get_layout_positions;
use crate::types::{
    LayoutPosition, MachineConfig, MotionConfig, ObjectOverride, PathSegment, Point2D,
    ProcessParams, SlideOverride, SubstratePaths, Transform,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Značka pro kontrolu vyosení trysky, kterou generátor vloží za homing po
/// každé výměně trysky.
pub const NOZZLE_ALIGN_MARKER: &str = "; DPI_NOZZLE_ALIGN:";

/// Výchozí blok po výměně trysky, pokud ho nastavení nepřepíše.
pub const DEFAULT_NOZZLE_CHANGE_GCODE: &str = "G28 ; Homing po vymene trysky";

pub fn push_nozzle_change(
    out: &mut String,
    nozzle_name: &str,
    nozzle_diam: f64,
    z_shift: f64,
    machine: &MachineConfig,
) {
    let change_block = if machine.nozzle_change_gcode.trim().is_empty() {
        DEFAULT_NOZZLE_CHANGE_GCODE
    } else {
        machine.nozzle_change_gcode.trim_end()
    };

    out.push_str(&format!(
        "\n; --- VYMENA TRYSKY: {nozzle_name} (prumer {nozzle_diam:.2} mm) ---\n\
         G1 Z{:.3} F1000 ; Vyjezd do bezpecne vysky pred vymenou\n\
         M0 Vymente trysku za: {nozzle_name} ({nozzle_diam:.2} mm). Po potvrzeni probehne homing a kontrola vyoseni.\n",
        machine.safe_z + z_shift,
    ));
    push_gcode_block(out, change_block);
    out.push_str(&format!("{NOZZLE_ALIGN_MARKER}{nozzle_name}\n"));

    if z_shift > 0.0 {
        out.push_str(&format!(
            "; Obnoveni virtualniho posunu Z po homingu (SHIFT {z_shift:.2}mm)\n\
             G1 Z{:.3} F1000 ; Vyjezd do bezpecne vysky\n\
             G92 Z{:.3} ; Nastaveni posunute nuly\n",
            machine.safe_z,
            machine.safe_z + z_shift
        ));
    }
    out.push_str("G92 E0.0 ; Reset extruderu po vymene\n");
}

/// Rozestup odplivu: vlastní nastavení odplivu (hodnota a typ vždy v páru),
/// jinak nastavení vzorku — hodnota se nikdy nevykládá jiným typem.
fn prime_infill<'a>(params: &'a ProcessParams, o: Option<&'a SlideOverride>) -> (&'a str, f64) {
    match o.and_then(|o| o.infill_val) {
        Some(v) => (o.and_then(|o| o.infill_type.as_deref()).unwrap_or("mm"), v),
        None => (params.infill_type.as_str(), params.infill_val),
    }
}

/// Nejvyšší počet řad odplivu — ochrana před nesmyslným zadáním.
const PRIME_MAX_ROWS: usize = 10_000;

/// Řady odplivového vzoru v rozsahu [y1, y2]. U "%" a "počet" leží krajní
/// řady na okrajích vzoru (stejně jako výplň objektů), u rozestupu v mm jdou
/// od y1 po celých rozestupech — počet řad se počítá celočíselně, sčítání
/// rozestupů by poslední řadu ztratilo zaokrouhlením.
fn prime_rows(y1: f64, y2: f64, typ: &str, val: f64, nozzle: f64) -> Vec<f64> {
    let h = y2 - y1;
    let mid = vec![(y1 + y2) / 2.0];
    if h.is_nan() || h <= 1e-9 {
        return mid;
    }
    let intervals = match typ {
        "počet" if val >= 1.0 => match val.round() as usize {
            0 | 1 => return mid,
            c => c.min(PRIME_MAX_ROWS) - 1,
        },
        "%" if val > 0.0 => {
            let spacing = nozzle / (val / 100.0);
            if spacing.is_nan() || spacing <= 1e-9 {
                return vec![];
            }
            ((h / spacing - 1e-9).ceil().max(1.0) as usize).min(PRIME_MAX_ROWS)
        }
        _ => {
            let spacing = if val > 0.0 { val } else { nozzle };
            if spacing.is_nan() || spacing <= 1e-9 {
                return vec![]; // ochrana proti nekonečné smyčce při nulovém rozestupu
            }
            let steps = ((h / spacing + 1e-9).floor() as usize).min(PRIME_MAX_ROWS);
            return (0..=steps).map(|k| y1 + k as f64 * spacing).collect();
        }
    };
    (0..=intervals).map(|k| y1 + h * k as f64 / intervals as f64).collect()
}

/// Lomená čára odplivového cik-caku se středem (cx, cy), od levého dolního
/// rohu: linie přes celou šířku střídané příčnými spojkami. Jediný zdroj
/// pravdy pro náhled (`generate_prime_preview`) i tiskový G-kód.
fn prime_pattern(cx: f64, cy: f64, params: &ProcessParams, o: Option<&SlideOverride>) -> Vec<Point2D> {
    let w = o.and_then(|o| o.slide_w).unwrap_or(15.0);
    let h = o.and_then(|o| o.slide_h).unwrap_or(15.0);
    let (typ, val) = prime_infill(params, o);
    let (x1, x2) = (cx - w / 2.0, cx + w / 2.0);
    let rows = prime_rows(cy - h / 2.0, cy + h / 2.0, typ, val, params.nozzle_diam);
    let mut pts = Vec::with_capacity(rows.len() * 2);
    for (k, &y) in rows.iter().enumerate() {
        let (a, b) = if k % 2 == 0 { (x1, x2) } else { (x2, x1) };
        pts.push(Point2D::new(a, y));
        pts.push(Point2D::new(b, y));
    }
    pts
}

/// Generuje náhledové dráhy odplivové (prime) pozice pro canvas.
/// Body jsou shodné s priming smyčkou v `generate_gcode`.
pub fn generate_prime_preview(
    pos: &LayoutPosition,
    params: &ProcessParams,
    prime_override: Option<&SlideOverride>,
) -> SubstratePaths {
    let sample_glass = prime_override.and_then(|o| o.glass_type.as_deref()) == Some("vzorkové");
    let (glass_w, glass_h) =
        if sample_glass { (params.slide_w, params.slide_h) } else { (pos.width, pos.height) };
    let points = prime_pattern(glass_w / 2.0, glass_h / 2.0, params, prime_override);
    if points.len() < 2 {
        return SubstratePaths::new(vec![]);
    }
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
    let idx = line.find([upper, lower])?;
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

/// Jeden úsek G-kódu mezi pauzovacími příkazy (M0/M1/M601).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GcodePauseSegment {
    pub code: String,
    /// Text pauzy z M0/M1 řádku (prázdný řetězec pokud bez textu);
    /// `None` značí poslední úsek za poslední pauzou.
    pub msg: Option<String>,
}

/// Rozdělí G-kód na úseky oddělené pauzovacími příkazy M0/M1/M601.
/// Komentáře (`;…`) se před detekcí odstraňují; M104/M117 apod. pauzy nejsou.
pub fn split_gcode_by_pauses(gcode: &str) -> Vec<GcodePauseSegment> {
    let mut segments: Vec<GcodePauseSegment> = Vec::new();
    let mut current: Vec<&str> = Vec::new();

    for line in gcode.split('\n') {
        let cmd = line.split(';').next().unwrap_or("").trim();
        let is_pause = starts_with_cmd(cmd, "M0")
            || starts_with_cmd(cmd, "M1")
            || cmd.eq_ignore_ascii_case("M601");
        if is_pause {
            let msg = cmd[1..]
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .trim();
            segments.push(GcodePauseSegment {
                code: current.join("\n"),
                msg: Some(msg.to_string()),
            });
            current.clear();
        } else {
            current.push(line);
        }
    }

    segments.push(GcodePauseSegment {
        code: current.join("\n"),
        msg: None,
    });
    segments
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
pub(crate) fn transform_pt(x_orig: f64, y_orig: f64, t: &Transform, _bed_max_y: f64) -> Point2D {
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

/// Pro uzavřený polygon rotuje body tak, aby první vrchol byl nejblíže aktuální pozici trysky.
/// Otevřené cesty vrací beze změny. Druhá hodnota je index původního bodu,
/// kterým dráha začíná (pro mapování tisku na náhled).
pub(crate) fn rotate_closed_path_to_nearest(
    points: &[Point2D],
    t: &Transform,
    bed_max_y: f64,
    cur_x: f64,
    cur_y: f64,
) -> (Vec<Point2D>, usize) {
    if points.len() < 3 {
        return (points.to_vec(), 0);
    }
    let first = points[0];
    let last = *points.last().unwrap();
    if (first.x - last.x).powi(2) + (first.y - last.y).powi(2) > 1e-2 {
        return (points.to_vec(), 0); // otevřená cesta
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
        return (points.to_vec(), 0);
    }
    let mut rotated = Vec::with_capacity(points.len());
    rotated.extend_from_slice(&points[best..n]);
    rotated.extend_from_slice(&points[0..best]);
    rotated.push(points[best]);
    (rotated, best)
}

/// Efektivní tiskové parametry: objekt → sklíčko → globální nastavení.
#[derive(Debug, Clone)]
pub(crate) struct PrintSettings<'a> {
    /// Tloušťka skla pod dráhou (mm) — vlastnost skla, objekt ji nepřepisuje.
    pub glass_z: f64,
    pub z_offset: f64,
    pub nozzle_height: f64,
    pub extrusion_rate: f64,
    pub extrusion_unit: &'a str,
    pub print_speed: f64,
    pub infill_style: &'a str,
}

impl<'a> PrintSettings<'a> {
    /// Nastavení sklíčka (lokální přepis, jinak globální parametry).
    pub fn for_slide(params: &'a ProcessParams, o: Option<&'a SlideOverride>) -> Self {
        PrintSettings {
            glass_z: o.and_then(|o| o.glass_z).unwrap_or(params.slide_z),
            z_offset: o.and_then(|o| o.z_offset).unwrap_or(params.z_offset),
            nozzle_height: o.and_then(|o| o.nozzle_height).unwrap_or(params.nozzle_height),
            extrusion_rate: o.and_then(|o| o.extrusion_rate).unwrap_or(params.extrusion_rate),
            extrusion_unit: o
                .and_then(|o| o.extrusion_unit.as_deref())
                .unwrap_or(&params.extrusion_unit),
            print_speed: o.and_then(|o| o.print_speed).unwrap_or(params.print_speed),
            infill_style: o
                .and_then(|o| o.infill_style.as_deref())
                .unwrap_or(&params.infill_style),
        }
    }

    /// Nastavení dráhy: přepis objektu, pokud ho dráha má, jinak nastavení sklíčka.
    pub fn for_segment(&self, slide: Option<&'a SlideOverride>, seg: &PathSegment) -> Self {
        let obj = seg
            .object
            .and_then(|id| slide.and_then(|s| s.objects.get(&id.to_string())));
        self.with_object(obj)
    }

    pub(crate) fn with_object(&self, o: Option<&'a ObjectOverride>) -> Self {
        let Some(o) = o else { return self.clone() };
        let (extrusion_rate, extrusion_unit) = match o.extrusion_rate {
            Some(r) => (r, o.extrusion_unit.as_deref().unwrap_or("µl/mm")),
            None => (self.extrusion_rate, self.extrusion_unit),
        };
        PrintSettings {
            glass_z: self.glass_z,
            z_offset: o.z_offset.unwrap_or(self.z_offset),
            nozzle_height: o.nozzle_height.unwrap_or(self.nozzle_height),
            extrusion_rate,
            extrusion_unit,
            print_speed: o.print_speed.unwrap_or(self.print_speed),
            infill_style: o
                .infill_style
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or(self.infill_style),
        }
    }
}

/// Je segment tečka (dva shodné body)?
pub(crate) fn is_dot_segment(pts: &[Point2D]) -> bool {
    pts.len() == 2 && pts[0] == pts[1]
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

/// Teoretická Z výška povrchu skla bez uživatelského z_offsetu (mm).
/// Jediný zdroj pravdy pro generátor i kalibrační tok ve frontendu (LeftPanel).
pub fn glass_z_theoretical(
    block_height: f64,
    nozzle_height: f64,
    nozzle_hidden: f64,
    slide_z: f64,
) -> f64 {
    -block_height + nozzle_height - nozzle_hidden + slide_z
}

/// Tisková výška trysky (mm) pro efektivní nastavení dráhy, bez virtuálního posunu Z.
fn print_z_unshifted(block_height: f64, params: &ProcessParams, st: &PrintSettings) -> f64 {
    glass_z_theoretical(block_height, st.nozzle_height, params.nozzle_hidden, st.glass_z)
        + st.z_offset
        + params.z_calibration
}

/// Teoretická Z povrchu skla na první pozici rozložení — tam najíždí kalibrace.
/// S aktivním odplivem je to odplivové sklo, které může mít jinou tloušťku
/// než vzorková skla; korekce z kalibrace pak vyjadřuje jen chybu nuly stroje.
pub fn calibration_glass_z(
    params: &ProcessParams,
    slide_overrides: &HashMap<String, SlideOverride>,
    block_height: f64,
) -> f64 {
    let key = if params.prime_active { "-1" } else { "0" };
    let st = PrintSettings::for_slide(params, slide_overrides.get(key));
    glass_z_theoretical(block_height, st.nozzle_height, params.nozzle_hidden, st.glass_z)
}

/// Virtuální posun Z souřadného systému: pokud nejnižší potřebná výška trysky
/// (minimum přes globální parametry, odpliv i per-slide overrides) klesá pod
/// fyzickou nulu tiskárny, posune se vše o |minimum| + 1 mm.
/// Jediný zdroj pravdy pro `generate_gcode` i kalibrační G92 ve frontendu —
/// `params.z_offset` se očekává v mm.
pub fn compute_z_shift(
    params: &ProcessParams,
    slide_overrides: &HashMap<String, SlideOverride>,
    block_height: f64,
) -> f64 {
    let abs_z = |st: &PrintSettings| print_z_unshifted(block_height, params, st);

    let mut min_needed_z = abs_z(&PrintSettings::for_slide(params, None));
    let prime_key = params.prime_active.then(|| "-1".to_string());
    let keys = (0..params.sample_count).map(|i| i.to_string()).chain(prime_key);
    for key in keys {
        if let Some(ovr) = slide_overrides.get(&key) {
            let slide = PrintSettings::for_slide(params, Some(ovr));
            min_needed_z = min_needed_z.min(abs_z(&slide));
            // Objekty skla mohou mít vlastní výšku — posun musí pokrýt i ty
            for o in ovr.objects.values() {
                min_needed_z = min_needed_z.min(abs_z(&slide.with_object(Some(o))));
            }
        }
    }

    if min_needed_z < 0.0 {
        min_needed_z.abs() + 1.0
    } else {
        0.0
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

    // Pokud tryska potřebuje jet pod fyzický limit tiskárny (Z < 0), posuneme vše o z_shift
    let z_shift = compute_z_shift(params, slide_overrides, machine.block_height);

    let motion = machine.motion();
    let mut head = Head::new(&motion);

    head.line("G21 ; Nastaveni jednotek na milimetry");
    head.block(&machine.start_gcode);

    if z_shift > 0.0 && !machine.skip_z_shift_setup {
        head.line(&format!("; --- VIRTUALNI POSUN Z (SHIFT {z_shift:.2}mm) ---"));
        head.z_to(machine.safe_z, "Vyjezd do bezpecne vysky");
        head.line(&format!("G92 Z{:.3} ; Nastaveni posunute nuly\n", machine.safe_z + z_shift));
        head.z = Some(machine.safe_z + z_shift);
    }

    // Bezpečnostní strop teploty podložky — nikdy neposíláme M140/M190
    // nad nakonfigurované (či konzervativní výchozí) maximum. Vypnuto (záporná
    // hodnota) i cíl 0 °C znamenají pro firmware vypnuté topení — bez příkazů.
    let bed_temp = params.bed_temp.clamp(0.0, machine.bed_max_temp.unwrap_or(110.0));
    if bed_temp > 0.0 {
        head.line(&format!(
            "M140 S{bed_temp:.0} ; Zacit nahrivat podlozku\n\
             M190 S{bed_temp:.0} ; Pockat na nahrati podlozky"
        ));
        head.time += 60.0; // Přibližně 1 minuta na nahřátí
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
    let mut total_dist = 0.0_f64;

    for (pos_idx, pos) in positions.iter().enumerate() {
        head.block(&machine.loop_start_gcode);
        head.line("G90 ; Absolutni souradnice pohybu\nM83 ; Relativni souradnice extruze");

        // Načteme lokální override pro dané sklíčko
        let key = if pos.is_prime {
            "-1".to_string()
        } else {
            measurement_idx.to_string()
        };
        let current_overrides = slide_overrides.get(&key);
        let slide = PrintSettings::for_slide(params, current_overrides);
        let print_z_of = |st: &PrintSettings| -> f64 {
            print_z_unshifted(machine.block_height, params, st) + z_shift
        };

        if pos.is_prime {
            let print_z = print_z_of(&slide);
            let e_per_mm = ext_calc.calculate_e_per_mm(slide.extrusion_rate, slide.extrusion_unit);
            head.line("\n; --- VZOREK (ODPLIV) ---");

            // Konfigurovatelný odplivový vzor — stejné body jako generate_prime_preview.
            // Linie i příčné spojky extrudují podle své skutečné délky.
            let pts = prime_pattern(
                pos.x + pos.width / 2.0,
                pos.y + pos.height / 2.0,
                params,
                current_overrides,
            );
            if pts.len() >= 2 {
                head.line(&format!("; DPI_SEG {pos_idx} 0 0"));
                head.move_to(pts[0], print_z, false);
                for pt in &pts[1..] {
                    total_dist += head.extrude(*pt, e_per_mm, slide.print_speed);
                }
                head.lift_after(print_z, "Zvednuti po odplivu");
            }
        } else {
            head.line(&format!("\n; --- VZOREK {} ---", measurement_idx + 1));

            if measurement_idx < slide_paths.len() {
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
                // Část trasy posunutá mimo sklo se netiskne
                let paths = clip_paths_to_slide(&slide_paths[measurement_idx], &transform, pos);
                let mut last_print_z = print_z_of(&slide);

                for (seg_idx, segment) in paths.segments.iter().enumerate() {
                    if segment.is_empty() {
                        continue;
                    }
                    // Parametry objektu, ke kterému dráha patří (objekt → sklo → globál)
                    let st = slide.for_segment(current_overrides, segment);
                    let print_z = print_z_of(&st);
                    let e_per_mm = ext_calc.calculate_e_per_mm(st.extrusion_rate, st.extrusion_unit);
                    last_print_z = print_z;

                    // Opt 1: pro uzavřené polygony rotuj počáteční bod k trysce
                    // (dráha napojená na spojovací čáru už začíná ve správném bodě)
                    let (effective_pts, start_idx) = if segment.fixed_start {
                        (segment.points.clone(), 0)
                    } else {
                        rotate_closed_path_to_nearest(
                            &segment.points,
                            &transform,
                            machine.bed.max_y,
                            head.x,
                            head.y,
                        )
                    };

                    // Značka pro živý náhled tisku: sklo, dráha, počáteční bod
                    head.line(&format!("; DPI_SEG {pos_idx} {seg_idx} {start_idx}"));

                    // Přesun na začátek: po mokré ploše výplně bez zdvihu, jinak
                    // retrakce, zdvih nad obě výšky, přejezd, sjezd a návrat kapaliny
                    let p0 = effective_pts[0];
                    let abs_p0 = transform_pt(p0.x, p0.y, &transform, machine.bed.max_y);
                    head.move_to(abs_p0, print_z, segment.wet_travel);

                    // Tečkování (Dot Dispensing)
                    if st.infill_style == "Tečky" && is_dot_segment(&effective_pts) {
                        let dot_e = ext_calc.calculate_dot_extrusion(st.extrusion_rate, st.extrusion_unit);
                        head.dispense_dot(dot_e, print_z);
                        continue;
                    }

                    // Normální čáry (vektory)
                    for pt in &effective_pts[1..] {
                        let pb = transform_pt(pt.x, pt.y, &transform, machine.bed.max_y);
                        total_dist += head.extrude(pb, e_per_mm, st.print_speed);
                    }
                }

                head.lift_after(last_print_z, "Zvednuti po tisku sklicka");
            }

            measurement_idx += 1;
        }

        head.block(&machine.loop_end_gcode);
    }

    if bed_temp > 0.0 {
        head.line("M140 S0 ; Vypnout vyhrivani podlozky");
    }
    head.block(&machine.end_gcode);

    Ok((head.out, total_dist, head.time))
}

// ─── Pohyby trysky ───────────────────────────────────────────────────────────

/// Posuv přejezdů a pohybů v Z (mm/min) — G-kód i odhad času.
pub(crate) const TRAVEL_SPEED_MM_MIN: f64 = 3000.0;
pub(crate) const Z_SPEED_MM_MIN: f64 = 1000.0;
/// Posuv E při dávkování kapky (mm/min).
const DOT_FEED_MM_MIN: f64 = 300.0;

impl MotionConfig {
    /// Délka E zpětného nasátí (mm).
    pub(crate) fn retract_e(&self) -> f64 {
        if self.retract_speed > 0.0 {
            self.retract_nl.max(0.0) / 1000.0 * self.calibration_factor
        } else {
            0.0
        }
    }

    fn retract_time(&self) -> f64 {
        let e = self.retract_e();
        if e > 0.0 { e / self.retract_speed * 60.0 } else { 0.0 }
    }

    /// Odhad času přesunu na začátek dráhy (s): po mokré ploše jen přejezd,
    /// jinak retrakce, zdvih, přejezd, sjezd a návrat kapaliny. `lifted` =
    /// tryska je už zvednutá a kapalina stažená (po kapce).
    pub(crate) fn travel_time(&self, dist: f64, wet: bool, lifted: bool) -> f64 {
        let xy = dist / TRAVEL_SPEED_MM_MIN * 60.0;
        if wet {
            return xy;
        }
        let z = self.z_hop.max(0.0) / Z_SPEED_MM_MIN * 60.0;
        let r = self.retract_time();
        if lifted { xy + z + r } else { xy + 2.0 * (z + r) }
    }

    /// Čas kapky (s): dávka, usazení, retrakce a zdvih.
    pub(crate) fn dot_time(&self, dot_e: f64) -> f64 {
        dot_e.abs() / DOT_FEED_MM_MIN * 60.0
            + self.dot_dwell_ms.max(0.0) / 1000.0
            + self.retract_time()
            + self.z_hop.max(0.0) / Z_SPEED_MM_MIN * 60.0
    }
}

/// Stav trysky při psaní G-kódu: poloha, výška, modální posuv, retrakce
/// a odhad času. Výška se sleduje přes celý soubor (i mezi skly), takže se
/// před přejezdem nikdy nesjíždí pod aktuální výšku trysky.
struct Head<'m> {
    out: String,
    motion: &'m MotionConfig,
    retract_e: f64,
    x: f64,
    y: f64,
    /// Výška trysky; None = neznámá (po G28 nebo relativním pohybu v bloku).
    z: Option<f64>,
    /// Modální posuv (Marlin ho sdílí pro G0 i G1); None = neznámý.
    f: Option<f64>,
    /// Kapalina je stažená retrakcí.
    retracted: bool,
    time: f64,
}

impl<'m> Head<'m> {
    fn new(motion: &'m MotionConfig) -> Self {
        Head {
            // Průměrný G-kód tiskové dráhy bývá desítky až stovky kB
            out: String::with_capacity(64 * 1024),
            motion,
            retract_e: motion.retract_e(),
            x: 0.0,
            y: 0.0,
            z: None,
            f: None,
            retracted: false,
            time: 0.0,
        }
    }

    fn line(&mut self, s: &str) {
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn cmd(&mut self, code: String, comment: &str) {
        self.out.push_str(&code);
        if !comment.is_empty() {
            self.out.push_str(" ; ");
            self.out.push_str(comment);
        }
        self.out.push('\n');
    }

    /// Posuv pro příkaz: vypíše se, když se mění (nebo `always`).
    fn feed(&mut self, f: f64, always: bool) -> String {
        let s = if always || self.f != Some(f) { format!(" F{f:.0}") } else { String::new() };
        self.f = Some(f);
        s
    }

    /// Uživatelský blok G-kódu: připojí ho a převezme z něj polohu (Z z G0/G1
    /// a G92, po G28 nebo v relativním režimu neznámá). Posuv po něm není známý.
    fn block(&mut self, block: &str) {
        if block.is_empty() {
            return;
        }
        push_gcode_block(&mut self.out, block);
        self.f = None;
        let mut relative = false;
        for raw in block.lines() {
            let l = raw.split(';').next().unwrap_or("").trim();
            if starts_with_cmd(l, "G91") {
                relative = true;
                self.z = None;
            } else if starts_with_cmd(l, "G90") {
                relative = false;
            } else if starts_with_cmd(l, "G28") {
                self.z = None;
            } else if starts_with_cmd(l, "G92") {
                if let Some(z) = parse_axis_value(l, 'Z') {
                    self.z = Some(z);
                }
            } else if is_linear_move(l) && !relative {
                let (x, y, z) = parse_move_axes(l);
                self.x = x.unwrap_or(self.x);
                self.y = y.unwrap_or(self.y);
                if z.is_some() {
                    self.z = z;
                }
            } else if is_linear_move(l) && parse_axis_value(l, 'Z').is_some() {
                self.z = None;
            }
        }
    }

    /// Svislý pohyb na výšku `z` (vynechá se, je-li tam tryska už).
    fn z_to(&mut self, z: f64, comment: &str) {
        if self.z.is_some_and(|c| (c - z).abs() < 1e-6) {
            return;
        }
        let f = self.feed(Z_SPEED_MM_MIN, true);
        self.cmd(format!("G1 Z{z:.3}{f}"), comment);
        self.time += self.z.map_or(0.0, |c| (c - z).abs()) / Z_SPEED_MM_MIN * 60.0;
        self.z = Some(z);
    }

    fn travel(&mut self, p: Point2D, comment: &str) {
        let f = self.feed(TRAVEL_SPEED_MM_MIN, true);
        self.cmd(format!("G0 X{:.3} Y{:.3}{f}", p.x, p.y), comment);
        self.time += (p.x - self.x).hypot(p.y - self.y) / TRAVEL_SPEED_MM_MIN * 60.0;
        (self.x, self.y) = (p.x, p.y);
    }

    /// Tiskový pohyb do `p`; vrací jeho délku. Posuv se vypíše jen při změně.
    fn extrude(&mut self, p: Point2D, e_per_mm: f64, speed: f64) -> f64 {
        let dist = (p.x - self.x).hypot(p.y - self.y);
        let f = self.feed(speed, false);
        self.cmd(format!("G1 X{:.3} Y{:.3} E{:.5}{f}", p.x, p.y, dist * e_per_mm), "");
        if speed > 0.0 {
            self.time += dist / speed * 60.0;
        }
        (self.x, self.y) = (p.x, p.y);
        dist
    }

    fn extrude_only(&mut self, e: f64, feed: f64, comment: &str) {
        let f = self.feed(feed, true);
        self.cmd(format!("G1 E{e:.5}{f}"), comment);
        self.time += e.abs() / feed * 60.0;
    }

    /// Zpětné nasátí kapaliny před zdvihem — kapka nevisí z trysky při přejezdu.
    fn retract(&mut self) {
        if self.retract_e > 0.0 && !self.retracted {
            self.extrude_only(-self.retract_e, self.motion.retract_speed, "Zpetne nasati");
            self.retracted = true;
        }
    }

    fn unretract(&mut self) {
        if self.retracted {
            self.extrude_only(self.retract_e, self.motion.retract_speed, "Navrat kapaliny");
            self.retracted = false;
        }
    }

    /// Přesun na začátek dráhy s tiskovou výškou `print_z`. Po mokré ploše
    /// výplně (`wet`) se jede v tiskové výšce; jinak retrakce, zdvih nad
    /// aktuální i cílovou výšku (nikdy se před přejezdem nesjíždí — tryska by
    /// narazila do tlustšího skla nebo sjela do čerstvé kapky), přejezd,
    /// sjezd k povrchu a návrat kapaliny.
    fn move_to(&mut self, p: Point2D, print_z: f64, wet: bool) {
        let dist = (p.x - self.x).hypot(p.y - self.y);
        let printing_height = !self.retracted && self.z.is_some_and(|z| (z - print_z).abs() < 1e-6);
        if printing_height && (wet || dist <= 1e-6) {
            // Dráha navazuje přesně na předchozí, nebo vede po mokré ploše
            if dist > 1e-6 {
                self.travel(p, "Prejezd po mokre plose");
            }
            return;
        }
        self.retract();
        let hop_z = print_z + self.motion.z_hop.max(0.0);
        let lift = self.z.map_or(hop_z, |z| z.max(hop_z));
        self.z_to(lift, "Z-hop pro prejezd");
        if dist > 1e-6 {
            self.travel(p, "");
        }
        self.z_to(print_z, "Sjezd k povrchu");
        self.unretract();
    }

    /// Kapka: dávka, usazení, retrakce a zdvih.
    fn dispense_dot(&mut self, dot_e: f64, print_z: f64) {
        self.extrude_only(dot_e, DOT_FEED_MM_MIN, "Davkovani kapky");
        let dwell = self.motion.dot_dwell_ms.max(0.0);
        if dwell > 0.0 {
            self.cmd(format!("G4 P{dwell:.0}"), "Usazeni kapky");
            self.time += dwell / 1000.0;
        }
        self.retract();
        self.z_to(print_z + self.motion.z_hop.max(0.0), "Z-hop po davkovani");
    }

    /// Zvednutí po dokončení skla / odplivu: retrakce a zdvih nad tiskovou výšku.
    fn lift_after(&mut self, print_z: f64, comment: &str) {
        self.retract();
        let hop_z = print_z + self.motion.z_hop.max(0.0);
        let lift = self.z.map_or(hop_z, |z| z.max(hop_z));
        self.z_to(lift, comment);
    }
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

    #[test]
    fn test_split_gcode_by_pauses_basic() {
        let segs =
            split_gcode_by_pauses("G28\nM1 Vloz sklicko\nG0 X5\nM601\nG1 X10\n; konec");
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].code, "G28");
        assert_eq!(segs[0].msg.as_deref(), Some("Vloz sklicko"));
        assert_eq!(segs[1].code, "G0 X5");
        assert_eq!(segs[1].msg.as_deref(), Some(""));
        assert_eq!(segs[2].code, "G1 X10\n; konec");
        assert_eq!(segs[2].msg, None);
    }

    #[test]
    fn test_split_gcode_by_pauses_ignores_lookalikes() {
        // M104/M117/M01 nejsou pauzy; M0 s komentářem ano
        let segs = split_gcode_by_pauses("G28\nM104 S200\nM117 hlaska\nM01");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].msg, None);

        let segs2 = split_gcode_by_pauses("m0 ; jen komentar\nG1 X1");
        assert_eq!(segs2.len(), 2);
        assert_eq!(segs2[0].msg.as_deref(), Some(""));
        assert_eq!(segs2[1].code, "G1 X1");
    }

    #[test]
    fn test_split_gcode_by_pauses_no_pauses() {
        let segs = split_gcode_by_pauses("G28\nG1 X5");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].code, "G28\nG1 X5");
        assert_eq!(segs[0].msg, None);
    }
}
