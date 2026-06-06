use crate::types::{PathSegment, Point2D, SliceParams, SubstratePaths};
use std::collections::HashMap;

// ─── Geometrické helpery ──────────────────────────────────────────────────────

struct RawGeometry {
    points: Vec<Point2D>,
    is_closed: bool,
    /// None = řídit se infill_style; Some(v) = explicitní příznak z SVG/DXF
    is_filled: Option<bool>,
    /// None = řídit se infill_style; Some(v) = explicitní příznak z SVG/DXF
    has_stroke: Option<bool>,
}

fn bbox_of_points<'a>(iter: impl Iterator<Item = &'a Point2D>) -> Option<(f64, f64, f64, f64)> {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut has = false;
    for p in iter {
        if p.x < min_x { min_x = p.x; }
        if p.x > max_x { max_x = p.x; }
        if p.y < min_y { min_y = p.y; }
        if p.y > max_y { max_y = p.y; }
        has = true;
    }
    if has { Some((min_x, max_x, min_y, max_y)) } else { None }
}

fn bbox_of_paths(segments: &[PathSegment]) -> Option<(f64, f64, f64, f64)> {
    bbox_of_points(segments.iter().flat_map(|s| s.points.iter()))
}

fn bbox_of_poly(pts: &[Point2D]) -> Option<(f64, f64, f64, f64)> {
    bbox_of_points(pts.iter())
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

/// Vzdálenost bodu od přímky (ne úsečky) definované body v a w.
fn point_to_line_dist(p: Point2D, v: Point2D, w: Point2D) -> f64 {
    let dx = w.x - v.x;
    let dy = w.y - v.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-12 {
        return (p.x - v.x).hypot(p.y - v.y);
    }
    ((p.x - v.x) * dy - (p.y - v.y) * dx).abs() / len_sq.sqrt()
}

/// Ramer-Douglas-Peucker simplifikace cesty.
/// Zachová tvar s maximální odchylkou `epsilon` mm a redukuje počet bodů.
fn douglas_peucker(pts: &[Point2D], epsilon: f64) -> Vec<Point2D> {
    if pts.len() <= 2 {
        return pts.to_vec();
    }
    let first = pts[0];
    let last = *pts.last().unwrap();
    let (max_dist, max_idx) = pts[1..pts.len() - 1]
        .iter()
        .enumerate()
        .map(|(i, p)| (point_to_line_dist(*p, first, last), i + 1))
        .fold((0.0f64, 0usize), |(md, mi), (d, i)| {
            if d > md { (d, i) } else { (md, mi) }
        });
    if max_dist > epsilon {
        let mut left = douglas_peucker(&pts[..=max_idx], epsilon);
        let right = douglas_peucker(&pts[max_idx..], epsilon);
        left.extend_from_slice(&right[1..]);
        left
    } else {
        vec![first, last]
    }
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

    // Pre-compute centroidy všech polygonů jednou — každý centroid se pak O(1) indexuje.
    let centroids: Vec<Option<Point2D>> = rotated_polygons
        .iter()
        .map(|poly| {
            let n = poly.len() as f64;
            if n < 1.0 {
                return None;
            }
            let cx = poly.iter().map(|p| p.x).sum::<f64>() / n;
            let cy = poly.iter().map(|p| p.y).sum::<f64>() / n;
            Some(Point2D::new(cx, cy))
        })
        .collect();

    // Polygon je díra, pokud jeho těžiště leží uvnitř lichého počtu ostatních polygonů.
    let hole_flags: Vec<bool> = (0..rotated_polygons.len())
        .map(|i| {
            let Some(c) = centroids[i] else { return false };
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

    // Seskupení scanline segmentů podle Y řady; klíč = (y × 10000).round() jako i64
    // aby se předešlo string-formátovacím alokacím v hot loopě.
    #[inline]
    fn y_key(v: f64) -> i64 { (v * 10_000.0).round() as i64 }

    let mut by_y: HashMap<i64, Vec<(Point2D, Point2D)>> = HashMap::new();
    for &(p1, p2) in &lines {
        by_y.entry(y_key(p1.y)).or_default().push((p1, p2));
    }

    let mut sorted_ys: Vec<i64> = by_y.keys().copied().collect();
    sorted_ys.sort_unstable();

    let mut all_pts: Vec<Point2D> = Vec::new();
    let mut current_pt: Option<Point2D> = None;
    let mut going_right = true;

    for key in &sorted_ys {
        let Some(raw) = by_y.get(key) else { continue };

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
                is_filled: seg.is_filled,
                has_stroke: seg.has_stroke,
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

    // 5. Douglas-Peucker simplifikace — redukuje počet bodů v křivkách (oblouky, kružnice z DXF/SVG)
    //    Tolerance 0.01 mm je pod rozlišením jakékoliv laboratorní tiskárny.
    const DP_EPSILON: f64 = 0.01;
    for obj in &mut objects {
        if obj.points.len() > 2 {
            obj.points = douglas_peucker(&obj.points, DP_EPSILON);
        }
    }

    // Seřadit objekty podle těžiště: zleva dola → doprava nahoru.
    // Určuje pořadí perimetrů i infill komponent.
    objects.sort_by(|a, b| {
        let cx =
            |pts: &[Point2D]| pts.iter().map(|p| p.x).sum::<f64>() / pts.len().max(1) as f64;
        let cy =
            |pts: &[Point2D]| pts.iter().map(|p| p.y).sum::<f64>() / pts.len().max(1) as f64;
        cx(&a.points)
            .partial_cmp(&cx(&b.points))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(cy(&a.points).partial_cmp(&cy(&b.points)).unwrap_or(std::cmp::Ordering::Equal))
    });

    // Pro "počet": rozestup se počítá per-komponent z výšky bbox; zde jen placeholder.
    let infill_spacing_base = if p.infill_type == "%" && p.infill_val > 0.0 {
        p.nozzle_diam / (p.infill_val / 100.0)
    } else if p.infill_type == "počet" {
        0.0
    } else {
        p.infill_val
    };

    let mut final_segs: Vec<PathSegment> = Vec::new();

    // infill_style = master rozhodnutí co vykreslit; SVG is_filled/has_stroke = brána (podmínka)
    let style_wants_fill = p.infill_style != "Okraje";

    // 6. Perimetry a otevřené cesty — v pořadí z souboru, bez přeřazování
    // Pro "počet" na otevřených cestách: fallback 1 mm (počet nedává smysl bez bbox)
    let open_path_spacing =
        infill_spacing_base.max(if p.infill_type == "počet" { 1.0 } else { 0.0 });
    // Otevřené cesty — uzavřené objekty se zpracují společně s výplní v komponentní smyčce
    for obj in &objects {
        if obj.points.is_empty() || obj.is_closed {
            continue;
        }
        if !obj.has_stroke.unwrap_or(true) {
            continue;
        }
        if p.infill_style == "Tečky" {
            let dots = interpolate_along_path(&obj.points, open_path_spacing);
            for d in dots {
                final_segs.push(PathSegment::new(vec![d, d]));
            }
        } else {
            final_segs.push(PathSegment::new(obj.points.clone()));
        }
    }

    // 8. Globální infill pro uzavřené polygony (s Union-Find seskupením)
    // Zahrnujeme VŠECHNY uzavřené polygony (i ty bez výplně fungují jako díry při even-odd rule).
    let closed_polys: Vec<Vec<Point2D>> = objects
        .iter()
        .filter(|o| o.is_closed && o.points.len() >= 3)
        .map(|o| o.points.clone())
        .collect();

    // Pro každý polygon: chce výplň = infill_style ji chce A objekt má fill (None = zpětná kompatibilita)
    let closed_poly_wants_fill: Vec<bool> = objects
        .iter()
        .filter(|o| o.is_closed && o.points.len() >= 3)
        .map(|o| style_wants_fill && o.is_filled.unwrap_or(true))
        .collect();

    // Pro každý polygon: má okraj (perimetr), který se skutečně kreslí
    let closed_poly_has_stroke: Vec<bool> = objects
        .iter()
        .filter(|o| o.is_closed && o.points.len() >= 3)
        .map(|o| o.has_stroke.unwrap_or(true))
        .collect();

    // Pro každý polygon: má být nakreslen okraj (plná draw_outline logika)
    let closed_draw_outline: Vec<bool> = objects
        .iter()
        .filter(|o| o.is_closed && o.points.len() >= 3)
        .map(|o| {
            let has_stroke = o.has_stroke.unwrap_or(true);
            let has_fill = o.is_filled.unwrap_or(true);
            has_stroke
                && match p.infill_style.as_str() {
                    "Okraje + Výplň" | "Okraje" => true,
                    "Had" | "Tečky" => !has_fill,
                    "Mřížka" => true,
                    _ => false,
                }
        })
        .collect();

    let infill_needed = !closed_polys.is_empty()
        && closed_poly_wants_fill.iter().any(|&f| f)
        && (infill_spacing_base > 0.0 || (p.infill_type == "počet" && p.infill_val >= 1.0));
    let mut infill_segs: Vec<PathSegment> = Vec::new();
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
        let mut component_wants_fill: HashMap<usize, bool> = HashMap::new();
        let mut component_has_stroke: HashMap<usize, bool> = HashMap::new();
        for (i, poly) in closed_polys.iter().enumerate() {
            let root = uf_find(&mut parent, i);
            components.entry(root).or_default().push(poly);
            if closed_poly_wants_fill[i] {
                *component_wants_fill.entry(root).or_insert(false) = true;
            }
            if closed_poly_has_stroke[i] {
                *component_has_stroke.entry(root).or_insert(false) = true;
            }
        }

        // Perimetry per komponenta — zgroupovat okraje uzavřených objektů ke svým komponentám
        let mut component_perimeters: HashMap<usize, Vec<PathSegment>> = HashMap::new();
        for (i, obj) in objects
            .iter()
            .filter(|o| o.is_closed && o.points.len() >= 3)
            .enumerate()
        {
            if !closed_draw_outline[i] {
                continue;
            }
            let root = uf_find(&mut parent, i);
            if p.infill_style == "Tečky" {
                let dots = interpolate_along_path(&obj.points, open_path_spacing);
                component_perimeters
                    .entry(root)
                    .or_default()
                    .extend(dots.iter().map(|&d| PathSegment::new(vec![d, d])));
            } else {
                component_perimeters
                    .entry(root)
                    .or_default()
                    .push(PathSegment::new(obj.points.clone()));
            }
        }

        // Seřadit komponenty podle těžiště jejich polygonů: zleva dola → doprava nahoru
        let mut sorted_roots: Vec<usize> = components.keys().cloned().collect();
        sorted_roots.sort_by(|&ra, &rb| {
            let centroid = |root: usize| -> (f64, f64) {
                let comp = &components[&root];
                let pts: Vec<_> = comp.iter().flat_map(|p| p.iter()).collect();
                if pts.is_empty() {
                    return (0.0, 0.0);
                }
                let n = pts.len() as f64;
                (
                    pts.iter().map(|p| p.x).sum::<f64>() / n,
                    pts.iter().map(|p| p.y).sum::<f64>() / n,
                )
            };
            let (ax, ay) = centroid(ra);
            let (bx, by) = centroid(rb);
            ax.partial_cmp(&bx)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(ay.partial_cmp(&by).unwrap_or(std::cmp::Ordering::Equal))
        });

        for &root in &sorted_roots {
            if !component_wants_fill.get(&root).copied().unwrap_or(false) {
                // Žádná výplň — přidat jen perimetry (pokud existují) a pokračovat
                if let Some(perims) = component_perimeters.get(&root) {
                    infill_segs.extend(nearest_neighbor_order(perims.clone()));
                }
                continue;
            }
            let comp = components[&root].as_slice();
            let mut comp_segs: Vec<PathSegment> = Vec::new();

            // Inset infill od okraje o polovinu průměru trysky — jen pro styly, kde se kreslí
            // perimetr zároveň s výplní, aby se trasy nepřekrývaly.
            let apply_inset = matches!(p.infill_style.as_str(), "Okraje + Výplň" | "Mřížka")
                && component_has_stroke.get(&root).copied().unwrap_or(false);
            let inset = if apply_inset { p.nozzle_diam } else { 0.0 };

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
                            comp_segs.push(PathSegment::new(vec![
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
                        let x_left = xs[j] + inset;
                        let x_right = xs[j + 1] - inset;
                        if x_left < x_right {
                            lines.push((Point2D::new(x_left, y_f), Point2D::new(x_right, y_f)));
                        }
                        j += 2;
                    }
                    y_f += infill_spacing;
                }

                if p.infill_style == "Had" {
                    comp_segs.extend(snake_infill(lines, &rotated, p.infill_angle));
                } else if p.infill_style == "Mřížka" {
                    // Horizontální linky — boustrofedon, přímo do infill_segs (NN by pořadí rozbil)
                    for (idx, (p1, p2)) in lines.iter().enumerate() {
                        let (a, b) = if idx % 2 == 0 { (*p1, *p2) } else { (*p2, *p1) };
                        infill_segs.push(PathSegment::new(vec![
                            rotate_pt(a, p.infill_angle, 0.0, 0.0),
                            rotate_pt(b, p.infill_angle, 0.0, 0.0),
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
                    let mut v_idx = 0usize;
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
                            let x_left = xs[j] + inset;
                            let x_right = xs[j + 1] - inset;
                            if x_left < x_right {
                                // Boustrofedon: sudé řady vlevo→vpravo, liché vpravo→vlevo
                                let (xl, xr) =
                                    if v_idx % 2 == 0 { (x_left, x_right) } else { (x_right, x_left) };
                                // Z rotated_90 prostoru zpět do světa: R(+90°) pak R(+infill_angle)
                                let r1 = rotate_pt(
                                    Point2D::new(xl, y_v),
                                    90.0 + p.infill_angle,
                                    0.0,
                                    0.0,
                                );
                                let r2 = rotate_pt(
                                    Point2D::new(xr, y_v),
                                    90.0 + p.infill_angle,
                                    0.0,
                                    0.0,
                                );
                                infill_segs.push(PathSegment::new(vec![r1, r2]));
                                v_idx += 1;
                            }
                            j += 2;
                        }
                        y_v += infill_spacing_v;
                    }
                } else {
                    // Boustrofedon přímo do infill_segs — NN by přirozené Y-pořadí rozbil
                    for (idx, (p1, p2)) in lines.into_iter().enumerate() {
                        let (a, b) = if idx % 2 == 0 { (p1, p2) } else { (p2, p1) };
                        infill_segs.push(PathSegment::new(vec![
                            rotate_pt(a, p.infill_angle, 0.0, 0.0),
                            rotate_pt(b, p.infill_angle, 0.0, 0.0),
                        ]));
                    }
                }
            }
            // Per-komponentní NN + 2-opt: každá komponenta optimalizována jako celek
            infill_segs.extend(two_opt_improve(nearest_neighbor_order(comp_segs)));
            // Perimetry rovnou po výplni (vždy poslední v rámci komponenty)
            if let Some(perims) = component_perimeters.get(&root) {
                infill_segs.extend(nearest_neighbor_order(perims.clone()));
            }
        }

    }
    // Záložní perimetry uzavřených objektů pro případ bez výplně (styl "Okraje" apod.)
    if !infill_needed {
        for obj in objects.iter().filter(|o| o.is_closed && !o.points.is_empty()) {
            let has_stroke = obj.has_stroke.unwrap_or(true);
            let has_fill = obj.is_filled.unwrap_or(true);
            let draw_outline = has_stroke
                && match p.infill_style.as_str() {
                    "Okraje + Výplň" | "Okraje" => true,
                    "Had" | "Tečky" => !has_fill,
                    "Mřížka" => true,
                    _ => false,
                };
            if !draw_outline {
                continue;
            }
            if p.infill_style == "Tečky" {
                let dots = interpolate_along_path(&obj.points, open_path_spacing);
                for d in dots {
                    final_segs.push(PathSegment::new(vec![d, d]));
                }
            } else {
                final_segs.push(PathSegment::new(obj.points.clone()));
            }
        }
    }

    // Infill: stitching (NN + 2-opt jsou již provedeny per-komponentně)
    let optimized_infill = stitch_nearby_endpoints(infill_segs, p.nozzle_diam);
    // Perimetry a otevřené cesty: jen NN (jsou již kontinuální, stitching by je mohl porušit)
    let optimized_outlines = nearest_neighbor_order(final_segs);
    // Perimetr jako poslední — zapečetí výplň a zabrání vytékání kapaliny ven
    let mut combined = optimized_infill;
    combined.extend(optimized_outlines);
    SubstratePaths::new(combined)
}

/// Hladová optimalizace pořadí segmentů metodou nejbližšího souseda s prostorovým gridem.
/// Místo O(n²) prohledávání všech segmentů používá 2D mřížku a expandující Čebyšev-prstence —
/// v průměru O(n·√n) pro rovnoměrná data, v praxi blízko O(n).
/// Otevřené segmenty (ne tečky, ne uzavřené polygony) lze pro lepší napojení obrátit.
fn nearest_neighbor_order(segs: Vec<PathSegment>) -> Vec<PathSegment> {
    if segs.len() <= 1 {
        return segs;
    }
    let n = segs.len();

    // ── Bounding box všech endpointů ──────────────────────────────────────────
    let (mut min_x, mut max_x, mut min_y, mut max_y) =
        (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for seg in &segs {
        for pt in seg.points.first().into_iter().chain(seg.points.last()) {
            min_x = min_x.min(pt.x);
            max_x = max_x.max(pt.x);
            min_y = min_y.min(pt.y);
            max_y = max_y.max(pt.y);
        }
    }
    let span = (max_x - min_x).max(max_y - min_y).max(1.0);
    // Velikost buňky: bbox / √n → průměrně ~1 segment na buňku
    let cell_size = (span / (n as f64).sqrt()).max(1e-6);

    let to_cell = |x: f64, y: f64| -> (i32, i32) {
        (
            ((x - min_x) / cell_size).floor() as i32,
            ((y - min_y) / cell_size).floor() as i32,
        )
    };

    // ── Sestavení gridu: buňka → indexy segmentů s endpointem v té buňce ─────
    let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for i in 0..n {
        let pts = &segs[i].points;
        if pts.is_empty() {
            continue;
        }
        let first = pts[0];
        let last = *pts.last().unwrap();
        let is_dot = pts.len() == 2
            && (first.x - last.x).abs() < 1e-9
            && (first.y - last.y).abs() < 1e-9;
        let closed = is_path_closed(pts);

        let c1 = to_cell(first.x, first.y);
        grid.entry(c1).or_default().push(i);
        // Konec reverzibilního segmentu registrovat zvlášť (jiná buňka)
        if !is_dot && !closed {
            let c2 = to_cell(last.x, last.y);
            if c2 != c1 {
                grid.entry(c2).or_default().push(i);
            }
        }
    }

    // ── Hladový NN s expandujícím prstenem ────────────────────────────────────
    let mut used = vec![false; n];
    let mut result = Vec::with_capacity(n);
    let mut cur = Point2D::new(0.0, 0.0);
    let max_ring = (n as f64).sqrt() as i32 + 2;

    for _ in 0..n {
        let (cx, cy) = to_cell(cur.x, cur.y);
        let mut best_i = n;
        let mut best_dist = f64::INFINITY;
        let mut best_rev = false;

        'rings: for ring in 0..=max_ring {
            // Minimální možná L2 vzdálenost od cur k buňkám v prstenci ring:
            // buňky leží ≥ (ring-1)*cell_size od hranice cur_cell → konzervativní bound.
            if best_i < n && ring > 1 && (ring - 1) as f64 * cell_size > best_dist {
                break 'rings;
            }

            // Výčet buněk v Čebyšev-prstenci r (vzdálenost přesně r v ∞-normě)
            let r = ring;
            let cells_iter: Box<dyn Iterator<Item = (i32, i32)>> = if r == 0 {
                Box::new(std::iter::once((cx, cy)))
            } else {
                // Horní a dolní řada + levý a pravý sloupec (bez rohů duplicitně)
                let top = ((-r)..=r).map(move |d| (cx + d, cy - r));
                let bot = ((-r)..=r).map(move |d| (cx + d, cy + r));
                let left = (-(r - 1)..=(r - 1)).map(move |d| (cx - r, cy + d));
                let right = (-(r - 1)..=(r - 1)).map(move |d| (cx + r, cy + d));
                Box::new(top.chain(bot).chain(left).chain(right))
            };

            for cell in cells_iter {
                let Some(indices) = grid.get(&cell) else { continue };
                for &i in indices {
                    if used[i] || segs[i].points.is_empty() {
                        continue;
                    }
                    let pts = &segs[i].points;
                    let first = pts[0];
                    let last = *pts.last().unwrap();
                    let is_dot = pts.len() == 2
                        && (first.x - last.x).abs() < 1e-9
                        && (first.y - last.y).abs() < 1e-9;
                    let closed = is_path_closed(pts);

                    let d_fwd = (first.x - cur.x).hypot(first.y - cur.y);
                    if d_fwd < best_dist {
                        best_dist = d_fwd;
                        best_i = i;
                        best_rev = false;
                    }
                    if !is_dot && !closed {
                        let d_rev = (last.x - cur.x).hypot(last.y - cur.y);
                        if d_rev < best_dist {
                            best_dist = d_rev;
                            best_i = i;
                            best_rev = true;
                        }
                    }
                }
            }
        }

        if best_i == n {
            break;
        }

        used[best_i] = true;
        let mut seg = segs[best_i].clone();
        if best_rev {
            seg.points.reverse();
        }
        if let Some(last_pt) = seg.points.last() {
            cur = *last_pt;
        }
        result.push(seg);
    }

    // Přidej případné přeskočené prázdné segmenty
    for i in 0..n {
        if !used[i] {
            result.push(segs[i].clone());
        }
    }

    result
}

// ─── 2-opt optimalizace ───────────────────────────────────────────────────────

fn seg_start(segs: &[PathSegment], idx: usize, rev: bool) -> Point2D {
    if rev {
        *segs[idx].points.last().unwrap()
    } else {
        segs[idx].points[0]
    }
}

fn seg_end(segs: &[PathSegment], idx: usize, rev: bool) -> Point2D {
    if rev {
        segs[idx].points[0]
    } else {
        *segs[idx].points.last().unwrap()
    }
}

fn seg_can_rev(segs: &[PathSegment], idx: usize) -> bool {
    let pts = &segs[idx].points;
    if pts.len() < 2 {
        return false;
    }
    let first = pts[0];
    let last = *pts.last().unwrap();
    let is_dot = pts.len() == 2
        && (first.x - last.x).abs() < 1e-9
        && (first.y - last.y).abs() < 1e-9;
    !is_dot && !is_path_closed(pts)
}

/// 2-opt zlepšení pořadí segmentů po `nearest_neighbor_order`.
/// Iterativně zkouší přehodit dvojice hran — pokud prohození sníží celkovou délku
/// přejezdů, provede ho. Přeskočeno pro n > 500 (výkonnostní guard).
fn two_opt_improve(segs: Vec<PathSegment>) -> Vec<PathSegment> {
    const MAX_SEGS: usize = 500;
    let n = segs.len();
    if n <= 2 || n > MAX_SEGS {
        return segs;
    }

    // tour[k] = (index původního segmentu, je_obrácen)
    let mut tour: Vec<(usize, bool)> = (0..n).map(|i| (i, false)).collect();

    let mut improved = true;
    while improved {
        improved = false;
        for i in 0..n - 1 {
            for j in i + 1..n {
                let (si, ri) = tour[i];
                let (si1, ri1) = tour[i + 1];
                let (sj, rj) = tour[j];

                let end_i = seg_end(&segs, si, ri);
                let start_i1 = seg_start(&segs, si1, ri1);
                let end_j = seg_end(&segs, sj, rj);

                // Délka přejezdu i→i+1 a j→j+1 před swapem
                let d_old = (end_i.x - start_i1.x).hypot(end_i.y - start_i1.y);
                // Délka přejezdu i→j a i+1→j+1 po reverzi bloku [i+1..=j]
                let d_new = (end_i.x - end_j.x).hypot(end_i.y - end_j.y);

                let (d_old_j, d_new_j) = if j + 1 < n {
                    let (sj1, rj1) = tour[j + 1];
                    let start_j1 = seg_start(&segs, sj1, rj1);
                    (
                        (end_j.x - start_j1.x).hypot(end_j.y - start_j1.y),
                        (start_i1.x - start_j1.x).hypot(start_i1.y - start_j1.y),
                    )
                } else {
                    (0.0, 0.0)
                };

                if (d_old + d_old_j) - (d_new + d_new_j) > 1e-9 {
                    // Reverze pořadí bloku [i+1..=j] a překlop orientace reverzibilních segmentů
                    tour[i + 1..=j].reverse();
                    for k in i + 1..=j {
                        let (idx, rev) = tour[k];
                        if seg_can_rev(&segs, idx) {
                            tour[k] = (idx, !rev);
                        }
                    }
                    improved = true;
                }
            }
        }
    }

    tour.iter()
        .map(|&(idx, rev)| {
            let mut seg = segs[idx].clone();
            if rev {
                seg.points.reverse();
            }
            seg
        })
        .collect()
}

// ─── Path stitching ───────────────────────────────────────────────────────────

/// Spojování blízkých koncových bodů — redukuje počet zdvihů trysky.
/// Pokud je vzdálenost konce seg[i] od začátku seg[i+1] menší než `threshold`,
/// segmenty se sloučí do jedné kontinuální dráhy s krátkým tiskovým přejezdem.
/// Nepřipojuje tečky ani uzavřené polygony.
fn stitch_nearby_endpoints(segs: Vec<PathSegment>, threshold: f64) -> Vec<PathSegment> {
    if segs.len() <= 1 || threshold <= 0.0 {
        return segs;
    }

    let is_dot = |pts: &[Point2D]| -> bool {
        pts.len() == 2
            && (pts[0].x - pts[1].x).abs() < 1e-9
            && (pts[0].y - pts[1].y).abs() < 1e-9
    };

    let mut result: Vec<PathSegment> = Vec::with_capacity(segs.len());
    let mut iter = segs.into_iter();
    let mut current = match iter.next() {
        Some(s) => s,
        None => return result,
    };

    for next in iter {
        if current.points.is_empty() || next.points.is_empty() {
            result.push(current);
            current = next;
            continue;
        }

        let cur_end = *current.points.last().unwrap();
        let next_start = next.points[0];
        let dist = (cur_end.x - next_start.x).hypot(cur_end.y - next_start.y);

        let can_stitch = dist < threshold
            && !is_dot(&current.points)
            && !is_dot(&next.points)
            && !is_path_closed(&current.points)
            && !is_path_closed(&next.points);

        if can_stitch {
            if dist > 1e-6 {
                current.points.push(next_start);
            }
            current.points.extend_from_slice(&next.points[1..]);
        } else {
            result.push(current);
            current = next;
        }
    }

    result.push(current);
    result
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
                    has_stroke: None,
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
