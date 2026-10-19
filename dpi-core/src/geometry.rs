use crate::extrusion::ExtrusionCalculator;
use crate::gcode::{is_dot_segment, rotate_closed_path_to_nearest, transform_pt, PrintSettings};
use crate::types::{
    LayoutPosition, MotionConfig, PathSegment, Point2D, PreviewDistResult, PreviewSegData,
    ProcessParams, SlideOverride, SubstratePaths, Transform,
};
use std::borrow::Cow;
use std::collections::HashMap;

// ─── World-space AABB ─────────────────────────────────────────────────────────

/// Vypočítá world-space AABB transformované tiskové trasy.
///
/// Odpovídá TS funkci `computeWorldAABB` v `src/lib/geometry.ts`.
/// Rotace je negována — canvas používá `scale(zoom, -zoom)` (Y-inverze),
/// což efektivně obrací směr rotace v zobrazovacích souřadnicích.
///
/// Vrátí `(min_x, max_x, min_y, max_y)` v absolutních souřadnicích tiskové plochy.
pub fn compute_world_aabb(
    t: &Transform,
    raw_min_x: f64,
    raw_max_x: f64,
    raw_min_y: f64,
    raw_max_y: f64,
) -> (f64, f64, f64, f64) {
    // Aplikace měřítka vůči středu transformace
    let s_min_x = t.cx + (raw_min_x - t.cx) * t.scale;
    let s_max_x = t.cx + (raw_max_x - t.cx) * t.scale;
    let s_min_y = t.cy + (raw_min_y - t.cy) * t.scale;
    let s_max_y = t.cy + (raw_max_y - t.cy) * t.scale;

    // Střed transformace v absolutních souřadnicích
    let wcx = t.gui_dx + t.cx;
    let wcy = t.gui_dy + t.cy;

    // Negace rotace odpovídá Y-inverzi canvas souřadnic
    let rad = (-t.rotation).to_radians();
    let cr = rad.cos();
    let sr = rad.sin();

    let corners = [
        (t.gui_dx + s_min_x, t.gui_dy + s_min_y),
        (t.gui_dx + s_max_x, t.gui_dy + s_min_y),
        (t.gui_dx + s_min_x, t.gui_dy + s_max_y),
        (t.gui_dx + s_max_x, t.gui_dy + s_max_y),
    ];

    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for (lx, ly) in corners {
        let dx = lx - wcx;
        let dy = ly - wcy;
        let wx = wcx + dx * cr - dy * sr;
        let wy = wcy + dx * sr + dy * cr;
        if wx < min_x {
            min_x = wx;
        }
        if wx > max_x {
            max_x = wx;
        }
        if wy < min_y {
            min_y = wy;
        }
        if wy > max_y {
            max_y = wy;
        }
    }

    (min_x, max_x, min_y, max_y)
}

// ─── Bounding box drah ────────────────────────────────────────────────────────

/// Extrahuje raw bounding box z drah (min_x, max_x, min_y, max_y) v lokálních souřadnicích.
/// Vrátí `None` pokud paths neobsahuje žádné body.
pub fn bbox_of_paths(paths: &SubstratePaths) -> Option<(f64, f64, f64, f64)> {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut has = false;
    for seg in &paths.segments {
        for p in &seg.points {
            if p.x < min_x {
                min_x = p.x;
            }
            if p.x > max_x {
                max_x = p.x;
            }
            if p.y < min_y {
                min_y = p.y;
            }
            if p.y > max_y {
                max_y = p.y;
            }
            has = true;
        }
    }
    if has {
        Some((min_x, max_x, min_y, max_y))
    } else {
        None
    }
}

// ─── Přizpůsobení transformací layoutu ───────────────────────────────────────

/// Přizpůsobí transformace nové sadě layoutových pozic.
///
/// Pro každé sklíčko zachová relativní offset v rámci sklíčka, pokud trasa se sklem
/// aspoň zčásti překrývá (část mimo sklo se při tisku ořízne, viz
/// `clip_paths_to_slide`), jinak resetuje pozici na střed sklíčka (zachová rotaci
/// a měřítko).
///
/// Odpovídá smyčce `doUpdateLayout` v `src/stores/projectStore.ts` (řádky 491–572).
///
/// # Parametry
/// - `new_positions` — nové pozice sklíček (výstup `get_layout_positions`)
/// - `old_non_prime_positions` — předchozí pozice (bez prime) pro výpočet relativního offsetu
/// - `current_transforms` — aktuální transformace pro každé vzorkové sklíčko
/// - `current_paths` — zpracované dráhy (výstup `process_substrate_paths`) pro každé sklíčko
/// - `_nozzle_diam` — průměr trysky v mm (dřív inset kontroly přesahu; zachováno kvůli API)
pub fn fit_transforms_to_layout(
    new_positions: &[LayoutPosition],
    old_non_prime_positions: &[LayoutPosition],
    current_transforms: &[Transform],
    current_paths: &[SubstratePaths],
    _nozzle_diam: f64,
) -> Vec<Transform> {
    let mut result = Vec::new();
    let mut sample_idx: usize = 0;

    for pos in new_positions {
        if pos.is_prime {
            continue;
        }

        let old_t = current_transforms.get(sample_idx);
        let old_pos = old_non_prime_positions.get(sample_idx);

        let new_t = match (old_t, old_pos) {
            (Some(old_t), Some(old_pos)) => {
                // Zachováme relativní offset uvnitř sklíčka
                let rel_dx = old_t.gui_dx - old_pos.x;
                let rel_dy = old_t.gui_dy - old_pos.y;

                let candidate = Transform {
                    cx: pos.width / 2.0,
                    cy: pos.height / 2.0,
                    gui_dx: pos.x + rel_dx,
                    gui_dy: pos.y + rel_dy,
                    scale: old_t.scale,
                    rotation: old_t.rotation,
                };

                // Trasa musí sklo aspoň zčásti překrývat (přesah se ořízne)
                let fits = match current_paths.get(sample_idx) {
                    Some(paths) => match bbox_of_paths(paths) {
                        Some((mn_x, mx_x, mn_y, mx_y)) => {
                            let (wmin_x, wmax_x, wmin_y, wmax_y) =
                                compute_world_aabb(&candidate, mn_x, mx_x, mn_y, mx_y);
                            wmax_x > pos.x
                                && wmin_x < pos.x + pos.width
                                && wmax_y > pos.y
                                && wmin_y < pos.y + pos.height
                        }
                        None => true, // prázdné dráhy — vejdou se vždy
                    },
                    None => true, // žádné dráhy pro toto sklíčko
                };

                if fits {
                    candidate
                } else {
                    // Trasa by byla celá mimo sklo — reset na střed, zachováme škálování/rotaci
                    Transform {
                        cx: pos.width / 2.0,
                        cy: pos.height / 2.0,
                        gui_dx: pos.x,
                        gui_dy: pos.y,
                        scale: old_t.scale,
                        rotation: old_t.rotation,
                    }
                }
            }
            // Nové sklíčko bez předchozí transformace — výchozí hodnoty
            _ => Transform {
                cx: pos.width / 2.0,
                cy: pos.height / 2.0,
                gui_dx: pos.x,
                gui_dy: pos.y,
                scale: 1.0,
                rotation: 0.0,
            },
        };

        result.push(new_t);
        sample_idx += 1;
    }

    result
}

// ─── Předpočítání vzdáleností pro náhled tisku ───────────────────────────────

/// Předpočítá vzdálenosti a časovou osu tisku pro náhled. Pořadí, start
/// uzavřených drah, rychlosti (objekt → sklíčko → globál), přejezdy se zdvihem
/// a retrakcí i kapky odpovídají `generate_gcode` (`motion`), takže náhled
/// běží v reálném čase tisku.
pub fn compute_preview_segments(
    positions: &[LayoutPosition],
    paths: &[SubstratePaths],
    transforms: &[Transform],
    prime_path: Option<&SubstratePaths>,
    params: &ProcessParams,
    overrides: &HashMap<String, SlideOverride>,
    motion: &MotionConfig,
) -> PreviewDistResult {
    let mut segs: Vec<PreviewSegData> = Vec::new();
    let mut cum_dist = 0.0_f64;
    let mut time = 0.0_f64;
    let mut last = Point2D::new(0.0, 0.0);
    // Tryska je zvednutá a kapalina stažená (po kapce, odplivu nebo skle)
    let mut lifted = false;
    let mut sample_idx = 0usize;
    let dist = |a: Point2D, b: Point2D| (b.x - a.x).hypot(b.y - a.y);
    let ext_calc = ExtrusionCalculator::new(
        params.filament_diameter,
        params.flow_multiplier,
        Some(motion.calibration_factor),
    );

    for (slide_idx, pos) in positions.iter().enumerate() {
        if pos.is_prime {
            let Some(prime) = prime_path else { continue };
            let st = PrintSettings::for_slide(params, overrides.get("-1"));
            for (seg_idx, seg) in prime.segments.iter().enumerate() {
                if seg.points.is_empty() {
                    continue;
                }
                // Odpliv se kreslí bez transformace: world = poloha skla + lokální bod
                let world: Vec<Point2D> = seg
                    .points
                    .iter()
                    .map(|p| Point2D::new(pos.x + p.x, pos.y + p.y))
                    .collect();
                let travel_from = last;
                time += motion.travel_time(dist(last, world[0]), false, lifted);
                let (mut seg_dist, mut seg_time) = (0.0, 0.0);
                let mut point_dists = vec![0.0];
                let mut point_times = vec![0.0];
                for w in world.windows(2) {
                    let d = dist(w[0], w[1]);
                    seg_dist += d;
                    seg_time += d / st.print_speed * 60.0;
                    point_dists.push(seg_dist);
                    point_times.push(seg_time);
                }
                segs.push(PreviewSegData {
                    slide_idx,
                    seg_idx,
                    path_start_dist: cum_dist,
                    point_dists,
                    seg_dist,
                    start_idx: 0,
                    start_time: time,
                    point_times,
                    travel_from,
                });
                time += seg_time;
                cum_dist += seg_dist.max(0.001);
                last = *world.last().unwrap();
            }
            // Zvednutí po odplivu
            lifted = true;
            continue;
        }

        let tidx = sample_idx;
        sample_idx += 1;
        let (Some(path_data), Some(t)) = (paths.get(tidx), transforms.get(tidx)) else {
            continue;
        };
        let slide_ovr = overrides.get(&tidx.to_string());
        let slide = PrintSettings::for_slide(params, slide_ovr);
        // Část trasy mimo sklo se netiskne — stejný ořez jako generate_gcode
        let path_data = clip_paths_to_slide(path_data, t, pos);

        for (seg_idx, seg) in path_data.segments.iter().enumerate() {
            if seg.points.is_empty() {
                continue;
            }
            let st = slide.for_segment(slide_ovr, seg);
            let (pts, start_idx) = if seg.fixed_start {
                (seg.points.clone(), 0)
            } else {
                rotate_closed_path_to_nearest(&seg.points, t, 0.0, last.x, last.y)
            };
            let world: Vec<Point2D> = pts.iter().map(|p| transform_pt(p.x, p.y, t, 0.0)).collect();
            let travel_from = last;
            let d = dist(last, world[0]);
            // Navazující dráha (bez přejezdu) netrvá nic, jinak stejný přesun jako G-kód
            if d > 1e-6 || lifted {
                time += motion.travel_time(d, seg.wet_travel && !lifted, lifted);
            }

            let (mut seg_dist, mut seg_time) = (0.0, 0.0);
            let mut point_dists = vec![0.0];
            let mut point_times = vec![0.0];
            let is_dot = st.infill_style == "Tečky" && is_dot_segment(&pts);
            lifted = is_dot;
            if is_dot {
                let dot_e = ext_calc.calculate_dot_extrusion(st.extrusion_rate, st.extrusion_unit);
                seg_time = motion.dot_time(dot_e);
                point_dists.push(0.0);
                point_times.push(seg_time);
            } else {
                for w in world.windows(2) {
                    let d = dist(w[0], w[1]);
                    seg_dist += d;
                    seg_time += d / st.print_speed * 60.0;
                    point_dists.push(seg_dist);
                    point_times.push(seg_time);
                }
            }
            segs.push(PreviewSegData {
                slide_idx,
                seg_idx,
                path_start_dist: cum_dist,
                point_dists,
                seg_dist,
                start_idx,
                start_time: time,
                point_times,
                travel_from,
            });
            time += seg_time;
            cum_dist += seg_dist.max(0.001);
            last = *world.last().unwrap();
        }
        // Zvednutí po tisku skla
        lifted = true;
    }

    PreviewDistResult {
        segs,
        total_dist: cum_dist,
        total_time: time,
    }
}

// ─── Ořez drah na sklo ─────────────────────────────────────────────────────────

/// Tolerance hranice skla (mm) — bod přesně na hraně je uvnitř.
const CLIP_EPS: f64 = 1e-6;

/// Obdélník skla ve world souřadnicích: (min_x, max_x, min_y, max_y).
type ClipRect = (f64, f64, f64, f64);

/// Ořízne dráhy sklíčka na obdélník skla — část trasy posunutá mimo sklo se
/// netiskne. Ořez probíhá ve world souřadnicích, výsledné body zůstávají
/// v lokálních souřadnicích dráhy (transformace je lineární, takže interpolace
/// podle parametru z world prostoru je přesná). Dráhy celé uvnitř skla se
/// vrací beze změny.
///
/// Jediný zdroj pravdy pro G-kód, časovou osu náhledu i vykreslení v plátně —
/// indexy drah (značky `DPI_SEG`) se tak shodují všude.
pub fn clip_paths_to_slide<'a>(
    paths: &'a SubstratePaths,
    t: &Transform,
    pos: &LayoutPosition,
) -> Cow<'a, SubstratePaths> {
    let rect: ClipRect = (pos.x, pos.x + pos.width, pos.y, pos.y + pos.height);
    let world = |p: &Point2D| transform_pt(p.x, p.y, t, 0.0);
    let all_inside = paths
        .segments
        .iter()
        .all(|s| s.points.iter().all(|p| in_rect(world(p), rect)));
    if all_inside {
        return Cow::Borrowed(paths);
    }

    let mut segments = Vec::with_capacity(paths.segments.len());
    for seg in &paths.segments {
        for (k, points) in clip_polyline(&seg.points, &world, rect).into_iter().enumerate() {
            // Kus, který nezačíná původním začátkem dráhy, navazuje přejezdem
            // přes okraj skla, ne po mokré ploše
            let wet_travel = seg.wet_travel && k == 0 && seg.points.first() == points.first();
            segments.push(PathSegment {
                points,
                is_filled: seg.is_filled,
                has_stroke: seg.has_stroke,
                group: seg.group,
                object: seg.object,
                hole: seg.hole,
                anchors: Vec::new(),
                snake: seg.snake,
                // Oříznutá uzavřená dráha už není uzavřená — začíná na hraně skla
                fixed_start: seg.fixed_start,
                wet_travel,
            });
        }
    }
    Cow::Owned(SubstratePaths {
        segments,
        objects: paths.objects.clone(),
        deleted_objects: paths.deleted_objects.clone(),
        object_transforms: paths.object_transforms.clone(),
    })
}

fn in_rect(p: Point2D, (x0, x1, y0, y1): ClipRect) -> bool {
    p.x >= x0 - CLIP_EPS && p.x <= x1 + CLIP_EPS && p.y >= y0 - CLIP_EPS && p.y <= y1 + CLIP_EPS
}

/// Liang–Barsky: rozsah parametru úsečky a→b uvnitř obdélníku, None = celá mimo.
fn clip_edge(a: Point2D, b: Point2D, (x0, x1, y0, y1): ClipRect) -> Option<(f64, f64)> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    for (p, q) in [(-dx, a.x - x0), (dx, x1 - a.x), (-dy, a.y - y0), (dy, y1 - a.y)] {
        if p.abs() < 1e-12 {
            if q < -CLIP_EPS {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                if r > t1 {
                    return None;
                }
                t0 = t0.max(r);
            } else {
                if r < t0 {
                    return None;
                }
                t1 = t1.min(r);
            }
        }
    }
    Some((t0, t1))
}

/// Části lomené čáry uvnitř obdélníku (lokální souřadnice). Uzavřená dráha,
/// která začíná uvnitř, se v místě začátku spojí, aby nevznikl zbytečný přejezd.
fn clip_polyline(
    pts: &[Point2D],
    world: &impl Fn(&Point2D) -> Point2D,
    rect: ClipRect,
) -> Vec<Vec<Point2D>> {
    let lerp = |a: Point2D, b: Point2D, t: f64| Point2D::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
    if pts.len() < 2 {
        return match pts.first() {
            Some(p) if in_rect(world(p), rect) => vec![pts.to_vec()],
            _ => vec![],
        };
    }
    let w: Vec<Point2D> = pts.iter().map(world).collect();
    let is_dot = is_dot_segment(pts);

    let mut runs: Vec<Vec<Point2D>> = Vec::new();
    let mut cur: Vec<Point2D> = Vec::new();
    let flush = |cur: &mut Vec<Point2D>, runs: &mut Vec<Vec<Point2D>>| {
        let run = std::mem::take(cur);
        let has_length = run.windows(2).any(|p| p[0] != p[1]);
        if run.len() >= 2 && (has_length || is_dot) {
            runs.push(run);
        }
    };
    for i in 0..pts.len() - 1 {
        match clip_edge(w[i], w[i + 1], rect) {
            None => flush(&mut cur, &mut runs),
            Some((t0, t1)) => {
                // Vstup do skla uprostřed úsečky (nebo první úsečka) začíná nový úsek
                if cur.is_empty() || t0 > 1e-9 {
                    flush(&mut cur, &mut runs);
                    cur.push(lerp(pts[i], pts[i + 1], t0));
                }
                cur.push(lerp(pts[i], pts[i + 1], t1));
                if t1 < 1.0 - 1e-9 {
                    flush(&mut cur, &mut runs);
                }
            }
        }
    }
    flush(&mut cur, &mut runs);

    let closed = pts.len() > 2 && pts[0] == pts[pts.len() - 1];
    if closed && runs.len() >= 2 && in_rect(w[0], rect) {
        let first = runs.remove(0);
        if let Some(last) = runs.last_mut() {
            last.extend(first.into_iter().skip(1));
        }
    }
    runs
}

// ─── Kontrola přesahu trasy ────────────────────────────────────────────────────

/// Vrátí `true` pokud některá z transformovaných tras přesahuje svou pozici sklíčka
/// s insetem `nozzle_diam / 2`.
///
/// Nahrazuje smyčky v `handleNozzleDiamGrew` v `App.svelte`.
/// `paths`, `transforms` a `non_prime_positions` musí být seřazeny shodně — index 0 odpovídá prvnímu vzorkovému sklíčku.
pub fn check_paths_overflow(
    paths: &[SubstratePaths],
    transforms: &[Transform],
    non_prime_positions: &[LayoutPosition],
    nozzle_diam: f64,
) -> bool {
    let r = nozzle_diam / 2.0;
    for ((path, transform), pos) in paths.iter().zip(transforms.iter()).zip(non_prime_positions.iter()) {
        if let Some((mn_x, mx_x, mn_y, mx_y)) = bbox_of_paths(path) {
            let (wmin_x, wmax_x, wmin_y, wmax_y) =
                compute_world_aabb(transform, mn_x, mx_x, mn_y, mx_y);
            if wmin_x < pos.x + r
                || wmax_x > pos.x + pos.width - r
                || wmin_y < pos.y + r
                || wmax_y > pos.y + pos.height - r
            {
                return true;
            }
        }
    }
    false
}

// ─── Testy ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PathSegment, Point2D};

    fn make_transform(gui_dx: f64, gui_dy: f64, scale: f64, rotation: f64) -> Transform {
        Transform {
            gui_dx,
            gui_dy,
            scale,
            rotation,
            cx: 38.0,
            cy: 13.0,
        }
    }

    fn make_paths(pts: &[(f64, f64)]) -> SubstratePaths {
        SubstratePaths::new(vec![PathSegment::new(
            pts.iter().map(|&(x, y)| Point2D::new(x, y)).collect(),
        )])
    }

    #[test]
    fn test_bbox_of_paths_basic() {
        let paths = make_paths(&[(1.0, 2.0), (5.0, 3.0), (3.0, 7.0)]);
        let bbox = bbox_of_paths(&paths);
        assert!(bbox.is_some());
        let (mn_x, mx_x, mn_y, mx_y) = bbox.unwrap();
        assert!((mn_x - 1.0).abs() < 1e-9);
        assert!((mx_x - 5.0).abs() < 1e-9);
        assert!((mn_y - 2.0).abs() < 1e-9);
        assert!((mx_y - 7.0).abs() < 1e-9);
    }

    #[test]
    fn test_bbox_of_paths_empty() {
        let paths = SubstratePaths::new(vec![]);
        assert!(bbox_of_paths(&paths).is_none());
    }

    #[test]
    fn test_compute_world_aabb_no_rotation() {
        // Bez rotace a s identity scale: AABB musí odpovídat raw bbox + offset
        let t = make_transform(10.0, 5.0, 1.0, 0.0);
        let (mn_x, mx_x, mn_y, mx_y) = compute_world_aabb(&t, 0.0, 20.0, 0.0, 10.0);
        // gui_dx=10, cx=38 → world_cx=48; raw_min_x=0 → s_min_x=cx+(0-cx)*1=0 → gui_dx+0=10
        // Pro scale=1 a no rotation: výsledek = raw + (gui_dx, gui_dy)
        assert!((mn_x - 10.0).abs() < 1e-6, "min_x={mn_x}");
        assert!((mx_x - 30.0).abs() < 1e-6, "max_x={mx_x}");
        assert!((mn_y - 5.0).abs() < 1e-6, "min_y={mn_y}");
        assert!((mx_y - 15.0).abs() < 1e-6, "max_y={mx_y}");
    }

    #[test]
    fn test_fit_transforms_preserves_offset_when_fits() {
        let old_pos = LayoutPosition {
            x: 0.0,
            y: 0.0,
            width: 76.0,
            height: 26.0,
            is_prime: false,
        };
        let new_pos = LayoutPosition {
            x: 80.0,
            y: 0.0,
            width: 76.0,
            height: 26.0,
            is_prime: false,
        };
        let old_t = Transform {
            gui_dx: 5.0, // rel_dx = 5 - 0 = 5
            gui_dy: 2.0, // rel_dy = 2 - 0 = 2
            cx: 38.0,
            cy: 13.0,
            scale: 1.0,
            rotation: 0.0,
        };
        // Malá geometrie uprostřed sklíčka — vejde se s relativním offsetem
        let paths = make_paths(&[(35.0, 11.0), (41.0, 15.0)]);

        let result = fit_transforms_to_layout(
            &[new_pos],
            &[old_pos],
            &[old_t],
            &[paths],
            0.4,
        );

        assert_eq!(result.len(), 1);
        // Relativní offset se zachová: new gui_dx = 80 + 5 = 85
        assert!((result[0].gui_dx - 85.0).abs() < 1e-6, "gui_dx={}", result[0].gui_dx);
        assert!((result[0].gui_dy - 2.0).abs() < 1e-6, "gui_dy={}", result[0].gui_dy);
    }

    #[test]
    fn test_fit_transforms_resets_when_outside() {
        let old_pos = LayoutPosition {
            x: 0.0,
            y: 0.0,
            width: 76.0,
            height: 26.0,
            is_prime: false,
        };
        let new_pos = LayoutPosition {
            x: 80.0,
            y: 0.0,
            width: 20.0, // užší sklíčko — se starým offsetem by trasa ležela celá mimo
            height: 26.0,
            is_prime: false,
        };
        let old_t = Transform {
            gui_dx: 30.0,
            gui_dy: 0.0,
            cx: 38.0,
            cy: 13.0,
            scale: 1.0,
            rotation: 0.0,
        };
        // Geometrie šířky 60mm — nevejde se do sklíčka 20mm
        let paths = make_paths(&[(5.0, 5.0), (65.0, 20.0)]);

        let result = fit_transforms_to_layout(
            &[new_pos],
            &[old_pos],
            &[old_t],
            &[paths],
            0.4,
        );

        assert_eq!(result.len(), 1);
        // Reset na střed nové pozice
        assert!((result[0].gui_dx - 80.0).abs() < 1e-6, "gui_dx={}", result[0].gui_dx);
        assert!((result[0].gui_dy - 0.0).abs() < 1e-6, "gui_dy={}", result[0].gui_dy);
        // Rotace a měřítko se zachovají
        assert!((result[0].scale - 1.0).abs() < 1e-6);
        assert!((result[0].rotation - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_fit_transforms_skips_prime() {
        let prime_pos = LayoutPosition {
            x: 0.0,
            y: 0.0,
            width: 76.0,
            height: 26.0,
            is_prime: true,
        };
        let sample_pos = LayoutPosition {
            x: 80.0,
            y: 0.0,
            width: 76.0,
            height: 26.0,
            is_prime: false,
        };

        let result = fit_transforms_to_layout(
            &[prime_pos, sample_pos],
            &[],
            &[],
            &[],
            0.4,
        );

        // Prime pozice se přeskočí → 1 transform pro sample_pos
        assert_eq!(result.len(), 1);
        assert!((result[0].gui_dx - 80.0).abs() < 1e-6);
    }
}
