use crate::types::{PathSegment, Point2D, SliceParams, SubstratePaths};
use std::collections::HashMap;

// ─── Geometrické helpery ──────────────────────────────────────────────────────

struct RawGeometry {
    points: Vec<Point2D>,
    is_closed: bool,
}

fn bbox_of_paths(segments: &[PathSegment]) -> Option<(f64, f64, f64, f64)> {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut has = false;
    for seg in segments {
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

fn bbox_of_poly(pts: &[Point2D]) -> Option<(f64, f64, f64, f64)> {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut has = false;
    for p in pts {
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
    if has {
        Some((min_x, max_x, min_y, max_y))
    } else {
        None
    }
}

fn is_path_closed(pts: &[Point2D]) -> bool {
    if pts.len() < 3 {
        return false;
    }
    let p1 = &pts[0];
    let p2 = &pts[pts.len() - 1];
    (p1.x - p2.x).powi(2) + (p1.y - p2.y).powi(2) < 1e-2
}

fn rotate_pt(pt: Point2D, angle_deg: f64, cx: f64, cy: f64) -> Point2D {
    let rad = angle_deg.to_radians();
    let cos_a = rad.cos();
    let sin_a = rad.sin();
    let dx = pt.x - cx;
    let dy = pt.y - cy;
    Point2D::new(cx + dx * cos_a - dy * sin_a, cy + dx * sin_a + dy * cos_a)
}

fn rotate_poly(pts: &[Point2D], angle_deg: f64) -> Vec<Point2D> {
    pts.iter()
        .map(|p| rotate_pt(*p, angle_deg, 0.0, 0.0))
        .collect()
}

fn point_in_polygon(pt: Point2D, poly: &[Point2D]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let xi = poly[i].x;
        let yi = poly[i].y;
        let xj = poly[j].x;
        let yj = poly[j].y;
        if ((yi > pt.y) != (yj > pt.y)) && (pt.x < (xj - xi) * (pt.y - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn interpolate_along_path(points: &[Point2D], spacing: f64) -> Vec<Point2D> {
    if points.len() < 2 || spacing <= 0.0 {
        return vec![];
    }
    let mut dots = Vec::new();
    let mut carry = 0.0_f64;
    for w in points.windows(2) {
        let dx = w[1].x - w[0].x;
        let dy = w[1].y - w[0].y;
        let len = dx.hypot(dy);
        if len == 0.0 {
            continue;
        }
        let mut t = carry;
        while t <= len {
            let r = t / len;
            dots.push(Point2D::new(w[0].x + dx * r, w[0].y + dy * r));
            t += spacing;
        }
        carry = t - len;
    }
    dots
}

fn dist_to_seg_sq(p: Point2D, v: Point2D, w: Point2D) -> f64 {
    let l2 = (v.x - w.x).powi(2) + (v.y - w.y).powi(2);
    if l2 == 0.0 {
        return (p.x - v.x).powi(2) + (p.y - v.y).powi(2);
    }
    let t = ((p.x - v.x) * (w.x - v.x) + (p.y - v.y) * (w.y - v.y)) / l2;
    let t = t.clamp(0.0, 1.0);
    let px = v.x + t * (w.x - v.x);
    let py = v.y + t * (w.y - v.y);
    (p.x - px).powi(2) + (p.y - py).powi(2)
}

/// Spojí dva body po obvodu polygonu (nejkratší cestou).
fn connect_along_perimeter(pa: Point2D, pb: Point2D, polygons: &[Vec<Point2D>]) -> Vec<Point2D> {
    const EPS: f64 = 1e-3;
    let mut pi_a: Option<usize> = None;
    let mut ei_a = 0usize;
    let mut pi_b: Option<usize> = None;
    let mut ei_b = 0usize;

    'outer: for (pi, poly) in polygons.iter().enumerate() {
        for j in 0..poly.len() {
            let v1 = poly[j];
            let v2 = poly[(j + 1) % poly.len()];
            if pi_a.is_none() && dist_to_seg_sq(pa, v1, v2) < EPS {
                pi_a = Some(pi);
                ei_a = j;
            }
            if pi_b.is_none() && dist_to_seg_sq(pb, v1, v2) < EPS {
                pi_b = Some(pi);
                ei_b = j;
            }
            if pi_a.is_some() && pi_b.is_some() {
                break 'outer;
            }
        }
    }

    if let (Some(pia), Some(pib)) = (pi_a, pi_b) {
        if pia == pib {
            if ei_a == ei_b {
                return vec![pa, pb];
            }
            let poly = &polygons[pia];
            let n = poly.len();

            let mut path1 = vec![pa];
            let mut e = ei_a;
            while e != ei_b {
                e = (e + 1) % n;
                path1.push(poly[e]);
            }
            path1.push(pb);

            let mut path2 = vec![pa];
            e = ei_a;
            while e != ei_b {
                path2.push(poly[e]);
                e = (e + n - 1) % n;
            }
            path2.push(pb);

            let len = |p: &[Point2D]| -> f64 {
                p.windows(2)
                    .map(|w| (w[0].x - w[1].x).hypot(w[0].y - w[1].y))
                    .sum()
            };
            return if len(&path1) < len(&path2) {
                path1
            } else {
                path2
            };
        }
    }
    vec![pa, pb]
}

// ─── Union-Find ───────────────────────────────────────────────────────────────

fn uf_find(parent: &mut Vec<usize>, i: usize) -> usize {
    if parent[i] != i {
        parent[i] = uf_find(parent, parent[i]);
    }
    parent[i]
}

fn uf_union(parent: &mut Vec<usize>, i: usize, j: usize) {
    let ri = uf_find(parent, i);
    let rj = uf_find(parent, j);
    if ri != rj {
        parent[ri] = rj;
    }
}

// ─── Had-infill (drátový odpor / wire-resistor pattern) ──────────────────────

fn polygon_signed_area(poly: &[Point2D]) -> f64 {
    let n = poly.len();
    let mut area = 0.0f64;
    for i in 0..n {
        let j = (i + 1) % n;
        area += poly[i].x * poly[j].y - poly[j].x * poly[i].y;
    }
    area / 2.0
}

fn segs_cross(p1: Point2D, p2: Point2D, p3: Point2D, p4: Point2D) -> bool {
    let dx1 = p2.x - p1.x;
    let dy1 = p2.y - p1.y;
    let dx2 = p4.x - p3.x;
    let dy2 = p4.y - p3.y;
    let d = dx1 * dy2 - dy1 * dx2;
    if d.abs() < 1e-12 {
        return false;
    }
    let ox = p3.x - p1.x;
    let oy = p3.y - p1.y;
    let t = (ox * dy2 - oy * dx2) / d;
    let u = (ox * dy1 - oy * dx1) / d;
    t > 1e-9 && t < 1.0 - 1e-9 && u > 1e-9 && u < 1.0 - 1e-9
}

fn line_crosses_hole(pa: Point2D, pb: Point2D, hole_polys: &[&Vec<Point2D>]) -> bool {
    for hole in hole_polys {
        let n = hole.len();
        if (0..n).any(|i| segs_cross(pa, pb, hole[i], hole[(i + 1) % n])) {
            return true;
        }
        if point_in_polygon(pa, hole) || point_in_polygon(pb, hole) {
            return true;
        }
    }
    false
}

/// Generuje trasu typu drátový odpor: řady zpracovávány striktně v pořadí Y,
/// směr se v každé řadě střídá (boustrofedon). Přechody jdou po obvodu polygonu —
/// pokud by přímá spojnice vedla dírou, trasa se odklonuje po vnějším obvodu.
fn snake_infill(
    lines: Vec<(Point2D, Point2D)>,
    rotated_polygons: &[Vec<Point2D>],
    infill_angle: f64,
) -> Vec<PathSegment> {
    if lines.is_empty() {
        return vec![];
    }

    // Polygon je díra, pokud jeho těžiště leží uvnitř jiného polygonu v komponentě.
    let hole_flags: Vec<bool> = (0..rotated_polygons.len())
        .map(|i| {
            let poly = &rotated_polygons[i];
            let n = poly.len() as f64;
            if n < 1.0 {
                return false;
            }
            let cx = poly.iter().map(|p| p.x).sum::<f64>() / n;
            let cy = poly.iter().map(|p| p.y).sum::<f64>() / n;
            let c = Point2D::new(cx, cy);
            rotated_polygons
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .filter(|(_, other)| point_in_polygon(c, other))
                .count()
                % 2
                == 1
        })
        .collect();

    let hole_polys: Vec<&Vec<Point2D>> = rotated_polygons
        .iter()
        .enumerate()
        .filter(|(i, _)| hole_flags[*i])
        .map(|(_, p)| p)
        .collect();

    // Největší non-hole polygon jako záložní trasa obejití díry
    let outer_fallback: Option<Vec<Vec<Point2D>>> = rotated_polygons
        .iter()
        .enumerate()
        .filter(|(i, _)| !hole_flags[*i])
        .max_by(|(_, a), (_, b)| {
            polygon_signed_area(a)
                .abs()
                .partial_cmp(&polygon_signed_area(b).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(_, p)| vec![p.clone()]);

    // Seskupení scanline segmentů podle Y řady
    let mut by_y: HashMap<String, Vec<(Point2D, Point2D)>> = HashMap::new();
    for &(p1, p2) in &lines {
        by_y.entry(format!("{:.4}", p1.y))
            .or_default()
            .push((p1, p2));
    }

    let mut sorted_ys: Vec<f64> = by_y.keys().filter_map(|k| k.parse::<f64>().ok()).collect();
    sorted_ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mut all_pts: Vec<Point2D> = Vec::new();
    let mut current_pt: Option<Point2D> = None;
    let mut going_right = true;

    for y in &sorted_ys {
        let key = format!("{:.4}", y);
        let Some(raw) = by_y.get(&key) else { continue };

        // Normalizace: p1.x ≤ p2.x, seřadit zleva doprava
        let mut row_segs: Vec<(Point2D, Point2D)> = raw
            .iter()
            .map(|&(a, b)| if a.x <= b.x { (a, b) } else { (b, a) })
            .collect();
        row_segs.sort_by(|a, b| {
            a.0.x
                .partial_cmp(&b.0.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Vzor drátového odporu: zprava doleva → obrátit pořadí i orientaci segmentů
        if !going_right {
            row_segs.reverse();
            for seg in &mut row_segs {
                std::mem::swap(&mut seg.0, &mut seg.1);
            }
        }

        for &(seg_start, seg_end) in &row_segs {
            if let Some(cp) = current_pt {
                let conn = connect_along_perimeter(cp, seg_start, rotated_polygons);
                // Záložní ochrana: pokud perimetrové trasování selhalo na přímou linku
                // a ta prochází dírou, odklonovat po vnějším obvodu
                if conn.len() == 2 && line_crosses_hole(cp, seg_start, &hole_polys) {
                    if let Some(ref outer) = outer_fallback {
                        let safe = connect_along_perimeter(cp, seg_start, outer);
                        all_pts.extend_from_slice(&safe[1..]);
                    } else {
                        all_pts.push(seg_start);
                    }
                } else {
                    all_pts.extend_from_slice(&conn[1..]);
                }
            } else {
                all_pts.push(seg_start);
            }
            all_pts.push(seg_end);
            current_pt = Some(seg_end);
        }

        going_right = !going_right;
    }

    if all_pts.is_empty() {
        return vec![];
    }

    vec![PathSegment::new(
        all_pts
            .into_iter()
            .map(|p| rotate_pt(p, infill_angle, 0.0, 0.0))
            .collect(),
    )]
}

// ─── Hlavní funkce ────────────────────────────────────────────────────────────

/// Port TypeScript `processSubstratePaths()` do Rustu.
/// Normalizuje, škáluje, centruje a generuje výplň pro vektorové dráhy.
pub fn process_substrate_paths(raw: &SubstratePaths, p: &SliceParams) -> SubstratePaths {
    if raw.segments.is_empty() {
        return SubstratePaths::new(vec![]);
    }

    let (min_x, max_x, min_y, max_y) = match bbox_of_paths(&raw.segments) {
        Some(b) => b,
        None => return SubstratePaths::new(vec![]),
    };
    let width = max_x - min_x;
    let height = max_y - min_y;

    // 1. Normalizace: posun do (0,0) a překlop Y
    let mut objects: Vec<RawGeometry> = raw
        .segments
        .iter()
        .map(|seg| {
            let pts: Vec<Point2D> = seg
                .points
                .iter()
                .map(|pt| Point2D::new(pt.x - min_x, height - (pt.y - min_y)))
                .collect();
            let closed = is_path_closed(&pts);
            RawGeometry {
                is_closed: closed,
                points: pts,
            }
        })
        .collect();

    let mut cur_w = width;
    let mut cur_h = height;

    // 2. Auto-scale
    let avail_w = p.slide_w - 2.0 * p.margin;
    let avail_h = p.slide_h - 2.0 * p.margin;
    if p.auto_scale && (width > avail_w || height > avail_h) {
        let sf = (avail_w / width).min(avail_h / height);
        for obj in &mut objects {
            for pt in &mut obj.points {
                pt.x *= sf;
                pt.y *= sf;
            }
        }
        cur_w *= sf;
        cur_h *= sf;
    }

    // 3. Vycentrování
    let off_x = (p.slide_w - cur_w) / 2.0;
    let off_y = (p.slide_h - cur_h) / 2.0;
    for obj in &mut objects {
        for pt in &mut obj.points {
            pt.x += off_x;
            pt.y += off_y;
        }
    }

    // 4. Uživatelské měřítko kolem středu sklíčka (slide_w/2, slide_h/2) —
    //    shodné s Canvas2D transform_pt, kde cx = slide_w/2, cy = slide_h/2.
    //    Jiný střed způsoboval vizuální skok při baking resetu scale → 1.0.
    if (p.user_scale - 1.0).abs() > 1e-6 {
        let us = p.user_scale;
        let cx = p.slide_w / 2.0;
        let cy = p.slide_h / 2.0;
        for obj in &mut objects {
            for pt in &mut obj.points {
                pt.x = cx + (pt.x - cx) * us;
                pt.y = cy + (pt.y - cy) * us;
            }
        }
    }

    // Pro "počet": rozestup se počítá per-komponent z výšky bbox; zde jen placeholder.
    let infill_spacing_base = if p.infill_type == "%" && p.infill_val > 0.0 {
        p.nozzle_diam / (p.infill_val / 100.0)
    } else if p.infill_type == "počet" {
        0.0
    } else {
        p.infill_val
    };

    let mut final_segs: Vec<PathSegment> = Vec::new();

    // 6. Perimetry a otevřené cesty — v pořadí z souboru, bez přeřazování
    // Pro "počet" na otevřených cestách: fallback 1 mm (počet nedává smysl bez bbox)
    let open_path_spacing =
        infill_spacing_base.max(if p.infill_type == "počet" { 1.0 } else { 0.0 });
    for obj in &objects {
        if obj.points.is_empty() {
            continue;
        }
        if !obj.is_closed {
            if p.infill_style == "Tečky" {
                let dots = interpolate_along_path(&obj.points, open_path_spacing);
                for d in dots {
                    final_segs.push(PathSegment::new(vec![d, d]));
                }
            } else {
                final_segs.push(PathSegment::new(obj.points.clone()));
            }
        } else if p.infill_style == "Okraje + Výplň" || p.infill_style == "Okraje" {
            final_segs.push(PathSegment::new(obj.points.clone()));
        }
    }

    // 8. Globální infill pro uzavřené polygony (s Union-Find seskupením)
    let closed_polys: Vec<Vec<Point2D>> = objects
        .iter()
        .filter(|o| o.is_closed && o.points.len() >= 3)
        .map(|o| o.points.clone())
        .collect();

    let infill_needed = !closed_polys.is_empty()
        && p.infill_style != "Okraje"
        && (infill_spacing_base > 0.0 || (p.infill_type == "počet" && p.infill_val >= 1.0));
    if infill_needed {
        let n = closed_polys.len();
        let mut parent: Vec<usize> = (0..n).collect();
        let poly_bboxes: Vec<_> = closed_polys
            .iter()
            .map(|poly| bbox_of_poly(poly).unwrap_or((0.0, 0.0, 0.0, 0.0)))
            .collect();

        for i in 0..n {
            for j in (i + 1)..n {
                let (ax1, ax2, ay1, ay2) = poly_bboxes[i];
                let (bx1, bx2, by1, by2) = poly_bboxes[j];
                if !(ax2 < bx1 || ax1 > bx2 || ay2 < by1 || ay1 > by2) {
                    uf_union(&mut parent, i, j);
                }
            }
        }

        let mut components: HashMap<usize, Vec<&Vec<Point2D>>> = HashMap::new();
        for (i, poly) in closed_polys.iter().enumerate() {
            let root = uf_find(&mut parent, i);
            components.entry(root).or_default().push(poly);
        }

        let mut infill_segs: Vec<PathSegment> = Vec::new();

        for comp in components.values() {
            let rotated: Vec<Vec<Point2D>> = comp
                .iter()
                .map(|poly| rotate_poly(poly, -p.infill_angle))
                .collect();

            let (mut mn_x, mut mx_x, mut mn_y, mut mx_y) = (
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
            );
            for poly in &rotated {
                for pt in poly {
                    if pt.x < mn_x {
                        mn_x = pt.x;
                    }
                    if pt.x > mx_x {
                        mx_x = pt.x;
                    }
                    if pt.y < mn_y {
                        mn_y = pt.y;
                    }
                    if pt.y > mx_y {
                        mx_y = pt.y;
                    }
                }
            }

            // Pro "počet": rovnoměrně rozdělit výšku bbox na N řádků
            let infill_spacing = if p.infill_type == "počet" && p.infill_val >= 1.0 {
                (mx_y - mn_y) / p.infill_val
            } else {
                infill_spacing_base
            };

            if p.infill_style == "Tečky" {
                // Ochrana před přetížením: max 100 000 bodů (jako Python verze)
                let dot_spacing = {
                    let estimated =
                        ((mx_x - mn_x) / infill_spacing) * ((mx_y - mn_y) / infill_spacing);
                    if estimated > 100_000.0 {
                        ((mx_x - mn_x) * (mx_y - mn_y) / 100_000.0).sqrt()
                    } else {
                        infill_spacing
                    }
                };
                let mut y_c = mn_y + dot_spacing / 2.0;
                let mut reverse = false;
                while y_c < mx_y {
                    let mut xs: Vec<f64> = {
                        let mut v = Vec::new();
                        let mut x = mn_x + dot_spacing / 2.0;
                        while x < mx_x {
                            v.push(x);
                            x += dot_spacing;
                        }
                        v
                    };
                    if reverse {
                        xs.reverse();
                    }
                    for x in xs {
                        let pt = Point2D::new(x, y_c);
                        let inside = rotated
                            .iter()
                            .filter(|poly| point_in_polygon(pt, poly))
                            .count();
                        if inside % 2 != 0 {
                            infill_segs.push(PathSegment::new(vec![
                                rotate_pt(pt, p.infill_angle, 0.0, 0.0),
                                rotate_pt(pt, p.infill_angle, 0.0, 0.0),
                            ]));
                        }
                    }
                    y_c += dot_spacing;
                    reverse = !reverse;
                }
            } else {
                // Řádkový infill (skenování Y)
                let mut y_f = mn_y + infill_spacing / 2.0;
                let mut lines: Vec<(Point2D, Point2D)> = Vec::new();
                while y_f < mx_y {
                    let mut xs: Vec<f64> = Vec::new();
                    for poly in &rotated {
                        for i in 0..poly.len() {
                            let p1 = poly[i];
                            let p2 = poly[(i + 1) % poly.len()];
                            if (p1.y <= y_f && p2.y > y_f) || (p2.y <= y_f && p1.y > y_f) {
                                let x = p1.x + (y_f - p1.y) * (p2.x - p1.x) / (p2.y - p1.y);
                                xs.push(x);
                            }
                        }
                    }
                    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                    let mut j = 0;
                    while j + 1 < xs.len() {
                        lines.push((Point2D::new(xs[j], y_f), Point2D::new(xs[j + 1], y_f)));
                        j += 2;
                    }
                    y_f += infill_spacing;
                }

                if p.infill_style == "Had" {
                    infill_segs.extend(snake_infill(lines, &rotated, p.infill_angle));
                } else if p.infill_style == "Mřížka" {
                    // Horizontální linky (z `lines`, otočit zpět o +infill_angle)
                    for (p1, p2) in &lines {
                        infill_segs.push(PathSegment::new(vec![
                            rotate_pt(*p1, p.infill_angle, 0.0, 0.0),
                            rotate_pt(*p2, p.infill_angle, 0.0, 0.0),
                        ]));
                    }
                    // Vertikální linky: skenovat polygony otočené o dalších -90°
                    let rotated_90: Vec<Vec<Point2D>> = rotated
                        .iter()
                        .map(|poly| rotate_poly(poly, -90.0))
                        .collect();
                    let (mut mn_x2, mut mx_x2, mut mn_y2, mut mx_y2) = (
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                    );
                    for poly in &rotated_90 {
                        for pt in poly {
                            if pt.x < mn_x2 {
                                mn_x2 = pt.x;
                            }
                            if pt.x > mx_x2 {
                                mx_x2 = pt.x;
                            }
                            if pt.y < mn_y2 {
                                mn_y2 = pt.y;
                            }
                            if pt.y > mx_y2 {
                                mx_y2 = pt.y;
                            }
                        }
                    }
                    let infill_spacing_v = if p.infill_type == "počet" && p.infill_val >= 1.0 {
                        (mx_y2 - mn_y2) / p.infill_val
                    } else {
                        infill_spacing
                    };
                    let mut y_v = mn_y2 + infill_spacing_v / 2.0;
                    while y_v < mx_y2 {
                        let mut xs: Vec<f64> = Vec::new();
                        for poly in &rotated_90 {
                            for i in 0..poly.len() {
                                let q1 = poly[i];
                                let q2 = poly[(i + 1) % poly.len()];
                                if (q1.y <= y_v && q2.y > y_v) || (q2.y <= y_v && q1.y > y_v) {
                                    xs.push(q1.x + (y_v - q1.y) * (q2.x - q1.x) / (q2.y - q1.y));
                                }
                            }
                        }
                        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                        let mut j = 0;
                        while j + 1 < xs.len() {
                            // Z rotated_90 prostoru zpět do světa: R(+90°) pak R(+infill_angle)
                            let r1 = rotate_pt(
                                Point2D::new(xs[j], y_v),
                                90.0 + p.infill_angle,
                                0.0,
                                0.0,
                            );
                            let r2 = rotate_pt(
                                Point2D::new(xs[j + 1], y_v),
                                90.0 + p.infill_angle,
                                0.0,
                                0.0,
                            );
                            infill_segs.push(PathSegment::new(vec![r1, r2]));
                            j += 2;
                        }
                        y_v += infill_spacing_v;
                    }
                } else {
                    for (p1, p2) in lines {
                        infill_segs.push(PathSegment::new(vec![
                            rotate_pt(p1, p.infill_angle, 0.0, 0.0),
                            rotate_pt(p2, p.infill_angle, 0.0, 0.0),
                        ]));
                    }
                }
            }
        }

        final_segs.extend(infill_segs);
    }
    SubstratePaths::new(final_segs)
}

// ─── DXF Parser ───────────────────────────────────────────────────────────────

/// Lehký parser ASCII DXF souborů. Načítá entity LINE a LWPOLYLINE.
/// Vrací souřadnice v milimetrech na základě $INSUNITS z hlavičky.
pub fn parse_dxf(dxf_text: &str) -> SubstratePaths {
    let lines: Vec<&str> = dxf_text.lines().map(str::trim).collect();

    // Detekce jednotek z hlavičky
    let mut dxf_to_mm = 1.0_f64;
    let mut i = 0;
    while i + 1 < lines.len().min(2000) {
        if lines[i] == "9" && lines[i + 1] == "$INSUNITS" {
            let mut k = i + 2;
            while k + 1 < (i + 22).min(lines.len()) {
                if lines[k] == "70" {
                    if let Ok(val) = lines[k + 1].parse::<u32>() {
                        dxf_to_mm = match val {
                            1 => 25.4,
                            4 => 1.0,
                            5 => 10.0,
                            6 => 1000.0,
                            8 => 25.4 * 12.0,
                            11 => 1e-3,
                            _ => 1.0,
                        };
                    }
                    break;
                }
                k += 2;
            }
            break;
        }
        i += 2;
    }

    enum Ent {
        Line {
            x1: f64,
            y1: f64,
            x2: f64,
            y2: f64,
        },
        Poly(Vec<Point2D>),
        Circle {
            cx: f64,
            cy: f64,
            r: f64,
        },
        Arc {
            cx: f64,
            cy: f64,
            r: f64,
            start_angle: f64,
            end_angle: f64,
        },
    }

    let mut entities: Vec<Ent> = Vec::new();
    let mut current: Option<Ent> = None;
    let mut idx = 0;

    while idx + 1 < lines.len() {
        let code = lines[idx].parse::<i32>().unwrap_or(-1);
        let val = lines[idx + 1];
        idx += 2;

        if code == 0 {
            if let Some(ent) = current.take() {
                entities.push(ent);
            }
            current = match val {
                "LINE" => Some(Ent::Line {
                    x1: 0.0,
                    y1: 0.0,
                    x2: 0.0,
                    y2: 0.0,
                }),
                "LWPOLYLINE" => Some(Ent::Poly(Vec::new())),
                "CIRCLE" => Some(Ent::Circle {
                    cx: 0.0,
                    cy: 0.0,
                    r: 0.0,
                }),
                "ARC" => Some(Ent::Arc {
                    cx: 0.0,
                    cy: 0.0,
                    r: 0.0,
                    start_angle: 0.0,
                    end_angle: 360.0,
                }),
                _ => None,
            };
            continue;
        }

        match &mut current {
            Some(Ent::Line { x1, y1, x2, y2 }) => {
                if let Ok(v) = val.parse::<f64>() {
                    match code {
                        10 => *x1 = v,
                        20 => *y1 = v,
                        11 => *x2 = v,
                        21 => *y2 = v,
                        _ => {}
                    }
                }
            }
            Some(Ent::Poly(pts)) => {
                if let Ok(v) = val.parse::<f64>() {
                    match code {
                        10 => pts.push(Point2D::new(v, 0.0)),
                        20 => {
                            if let Some(last) = pts.last_mut() {
                                last.y = v;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Some(Ent::Circle { cx, cy, r }) => {
                if let Ok(v) = val.parse::<f64>() {
                    match code {
                        10 => *cx = v,
                        20 => *cy = v,
                        40 => *r = v,
                        _ => {}
                    }
                }
            }
            Some(Ent::Arc {
                cx,
                cy,
                r,
                start_angle,
                end_angle,
            }) => {
                if let Ok(v) = val.parse::<f64>() {
                    match code {
                        10 => *cx = v,
                        20 => *cy = v,
                        40 => *r = v,
                        50 => *start_angle = v,
                        51 => *end_angle = v,
                        _ => {}
                    }
                }
            }
            None => {}
        }
    }
    if let Some(ent) = current {
        entities.push(ent);
    }

    let mut segments = Vec::new();
    for ent in entities {
        match ent {
            Ent::Line { x1, y1, x2, y2 } => {
                segments.push(PathSegment::new(vec![
                    Point2D::new(x1 * dxf_to_mm, y1 * dxf_to_mm),
                    Point2D::new(x2 * dxf_to_mm, y2 * dxf_to_mm),
                ]));
            }
            Ent::Poly(pts) if pts.len() >= 2 => {
                segments.push(PathSegment::new(
                    pts.into_iter()
                        .map(|p| Point2D::new(p.x * dxf_to_mm, p.y * dxf_to_mm))
                        .collect(),
                ));
            }
            Ent::Circle { cx, cy, r } if r > 0.0 => {
                // Aproximace kružnice 64 úsečkami (stejně jako Python verze)
                const NUM_SEGS: usize = 64;
                let pts: Vec<Point2D> = (0..=NUM_SEGS)
                    .map(|i| {
                        let ang = std::f64::consts::TAU * i as f64 / NUM_SEGS as f64;
                        Point2D::new(
                            (cx + r * ang.cos()) * dxf_to_mm,
                            (cy + r * ang.sin()) * dxf_to_mm,
                        )
                    })
                    .collect();
                segments.push(PathSegment {
                    points: pts,
                    is_filled: Some(true),
                });
            }
            Ent::Arc {
                cx,
                cy,
                r,
                start_angle,
                end_angle,
            } if r > 0.0 => {
                let end_a = if end_angle < start_angle {
                    end_angle + 360.0
                } else {
                    end_angle
                };
                let span = end_a - start_angle;
                let num_segs = ((64.0 * span / 360.0) as usize).max(2);
                let pts: Vec<Point2D> = (0..=num_segs)
                    .map(|i| {
                        let ang = (start_angle + span * i as f64 / num_segs as f64).to_radians();
                        Point2D::new(
                            (cx + r * ang.cos()) * dxf_to_mm,
                            (cy + r * ang.sin()) * dxf_to_mm,
                        )
                    })
                    .collect();
                segments.push(PathSegment::new(pts));
            }
            _ => {}
        }
    }
    SubstratePaths::new(segments)
}
