use crate::regions::{self, Bbox, Contour, ScanHit};
use crate::types::{ObjectTransform, PathObject, PathSegment, Point2D, SliceParams, SubstratePaths};
use std::collections::{HashMap, HashSet};

// ─── Geometrické helpery ──────────────────────────────────────────────────────

struct RawGeometry {
    points: Vec<Point2D>,
    is_closed: bool,
    /// None = řídit se infill_style; Some(v) = explicitní příznak z SVG/DXF
    is_filled: Option<bool>,
    /// None = řídit se infill_style; Some(v) = explicitní příznak z SVG/DXF
    has_stroke: Option<bool>,
    /// Zdrojový prvek (even-odd skupina výplně)
    group: Option<u32>,
    /// Index ve vstupních segmentech — stabilní id objektu
    src: usize,
    /// Tvar je díra (odečítá se z výplně ostatních prvků)
    hole: bool,
    /// Konce čáry přichycené k objektům: (konec, id objektu, bod přichycení)
    anchors: Vec<(u8, u32, Point2D)>,
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

// ─── Řady výplně ──────────────────────────────────────────────────────────────

/// Odsazení krajních řad dovnitř tvaru, aby řada neležela přesně na hraně
/// (průsečíky s vodorovnou hranou by byly degenerované).
const EDGE_ROW_EPS: f64 = 1e-4;

/// Rozmístění řad výplně v rozsahu tvaru.
#[derive(Clone, Copy)]
enum RowSpec {
    /// Pevný rozestup v mm — řady začínají půl rozestupu od okraje.
    Step(f64),
    /// Rozestup z procent — krajní řady na okrajích, rozestup se rovnoměrně
    /// zmenší, aby se vešel celý počet mezer.
    Fit(f64),
    /// Přesný počet řad — krajní na okrajích (jediná řada uprostřed).
    Count(usize),
}

/// Polohy řad výplně v rozsahu [min, max]. U `Fit` a `Count` leží krajní řady
/// na okrajích (zmenšených o `margin`) — výplň zachová rozměry i poměr stran tvaru.
fn row_positions(min: f64, max: f64, spec: RowSpec, margin: f64) -> Vec<f64> {
    if let RowSpec::Step(spacing) = spec {
        let mut rows = Vec::new();
        let mut y = min + spacing / 2.0;
        while y < max {
            rows.push(y);
            y += spacing;
        }
        return rows;
    }
    let (lo, hi) = (min + margin + EDGE_ROW_EPS, max - margin - EDGE_ROW_EPS);
    if hi - lo <= 1e-9 {
        return if max > min { vec![(min + max) / 2.0] } else { vec![] };
    }
    let n = match spec {
        RowSpec::Fit(spacing) => ((hi - lo) / spacing - 1e-9).ceil().max(1.0) as usize,
        RowSpec::Count(c) if c <= 1 => return vec![(lo + hi) / 2.0],
        RowSpec::Count(c) => c - 1,
        RowSpec::Step(_) => unreachable!(),
    };
    (0..=n).map(|k| lo + (hi - lo) * k as f64 / n as f64).collect()
}

// ─── Had-infill (cik-cak / drátový odpor) ────────────────────────────────────

/// Úsek řádku uvnitř výplně.
struct ScanSpan {
    row: usize,
    y: f64,
    /// [levý, pravý] konec úseku
    ends: [ScanHit; 2],
}

/// Vrcholy obrysu polygonu mezi dvěma průsečíky ve směru hran (bez krajních bodů).
fn outline_between(poly: &[Point2D], from: ScanHit, to: ScanHit) -> Vec<Point2D> {
    let n = poly.len();
    let mut steps = (to.edge + n - from.edge) % n;
    if steps == 0 && to.t <= from.t {
        steps = n;
    }
    (1..=steps).map(|k| poly[(from.edge + k) % n]).collect()
}

/// Konec úseku podél obrysu: (hrana, parametr na hraně, konec úseku).
/// Konec úseku = index_úseku * 2 + strana (0 = levý, 1 = pravý); None =
/// průsečík uvnitř výplně (překryv tvarů), který spojky po obrysu blokuje.
type AlongHit = (usize, f64, Option<usize>);

/// Úseky řádků a všechny průsečíky seřazené podél obrysů.
fn scan_spans(contours: &[Contour], rows: &[f64]) -> (Vec<ScanSpan>, Vec<Vec<AlongHit>>) {
    let mut spans: Vec<ScanSpan> = Vec::new();
    let mut along: Vec<Vec<AlongHit>> = vec![Vec::new(); contours.len()];
    let (mut hits, mut row_spans) = (Vec::new(), Vec::new());
    for (row, &y) in rows.iter().enumerate() {
        regions::row_hits(contours, y, &mut hits);
        regions::row_spans(contours, &hits, &mut row_spans);
        let first = spans.len();
        spans.extend(row_spans.iter().map(|&ends| ScanSpan { row, y, ends }));
        for h in &hits {
            // Konec úseku = index_úseku * 2 + strana (0 = levý, 1 = pravý)
            let end = (first * 2..spans.len() * 2).find(|&e| {
                let eh = spans[e / 2].ends[e % 2];
                eh.poly == h.poly && eh.edge == h.edge
            });
            along[h.poly].push((h.edge, h.t, end));
        }
    }
    for list in &mut along {
        list.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        });
    }
    (spans, along)
}

/// Generuje výplň typu had (cik-cak) pro jeden objekt. Úseky sousedních řádků
/// se spojují jen po obrysu mezi dvěma po sobě jdoucími průsečíky, takže
/// tisková dráha nikdy nevede přes prázdné místo ani přes díru. Spojka mezi
/// řádky k a k+1 leží vpravo pro sudé k a vlevo pro liché k — každý konec úseku
/// má nejvýš jednu spojku a dráha se v řádcích střídá tam a zpět. Kde spojka
/// po obrysu neexistuje (tvar se kolem díry větví), začíná nová dráha a mezi
/// drahami se přejíždí bez extruze.
fn snake_infill(contours: &[Contour], rows: &[f64], infill_angle: f64) -> Vec<PathSegment> {
    // 1. Úseky řádků a všechny průsečíky seřazené podél obrysů; průsečíky
    //    uvnitř výplně (překryv tvarů) blokují spojky stejně jako konce úseků.
    let (spans, along) = scan_spans(contours, rows);
    let end_count = spans.len() * 2;
    let hit_of = |end: usize| spans[end / 2].ends[end % 2];
    let pt_of = |end: usize| Point2D::new(hit_of(end).x, spans[end / 2].y);

    // 2. Spojky po obrysu mezi sousedními řádky.
    //    link[konec] = (druhý konec, index spojky, spojka vede od tohoto konce)
    let mut connectors: Vec<Vec<Point2D>> = Vec::new();
    let mut link: Vec<Option<(usize, usize, bool)>> = vec![None; end_count];
    for (pi, list) in along.iter().enumerate() {
        let m = list.len();
        if m < 2 {
            continue;
        }
        for j in 0..m {
            let (Some(a), Some(b)) = (list[j].2, list[(j + 1) % m].2) else { continue };
            let (row_a, row_b) = (spans[a / 2].row, spans[b / 2].row);
            let right_side = a % 2 == 1;
            if b % 2 != a % 2 || row_a.abs_diff(row_b) != 1 {
                continue;
            }
            // Cik-cak: vpravo mezi řádky (sudý, lichý), vlevo mezi (lichý, sudý)
            if right_side != (row_a.min(row_b) % 2 == 0) {
                continue;
            }
            if link[a].is_some() || link[b].is_some() {
                continue;
            }
            connectors.push(outline_between(&contours[pi].pts, hit_of(a), hit_of(b)));
            let c = connectors.len() - 1;
            link[a] = Some((b, c, true));
            link[b] = Some((a, c, false));
        }
    }

    // 3. Skládání drah úsek → spojka → úsek… od volných konců; co zbyde,
    //    tvoří uzavřené řetězce a ty se rozpojí na prvním úseku.
    let mut visited = vec![false; spans.len()];
    let mut out: Vec<PathSegment> = Vec::new();
    let free_ends = (0..end_count).filter(|&end| link[end].is_none());
    for start in free_ends.chain(0..end_count) {
        if visited[start / 2] {
            continue;
        }
        let mut pts = vec![pt_of(start)];
        let mut entry = start;
        loop {
            visited[entry / 2] = true;
            let exit = entry ^ 1;
            pts.push(pt_of(exit));
            let Some((next, c, forward)) = link[exit] else { break };
            if visited[next / 2] {
                break;
            }
            if forward {
                pts.extend_from_slice(&connectors[c]);
            } else {
                pts.extend(connectors[c].iter().rev());
            }
            pts.push(pt_of(next));
            entry = next;
        }
        pts.dedup_by(|a, b| (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9);
        out.push(PathSegment::new(
            pts.into_iter()
                .map(|p| rotate_pt(p, infill_angle, 0.0, 0.0))
                .collect(),
        ));
    }
    out
}

/// Had objektu v tiskovém pořadí. `start` = roh obálky objektu (`world_bb`,
/// souřadnice sklíčka), ve kterém had začíná; jinak se začátek volí sám.
/// Had začíná vždy na kraji první nebo poslední řady — do ostatních rohů vede
/// zrcadlená varianta (spojky řad na opačných stranách).
fn snake_paths(
    rotated: &[Contour],
    world_bb: &Bbox,
    rows: &[f64],
    angle: f64,
    start: Option<&str>,
) -> Vec<PathSegment> {
    let mark = |segs: Vec<PathSegment>| -> Vec<PathSegment> {
        segs.into_iter().map(|s| PathSegment { snake: true, ..s }).collect()
    };
    let corner = match start {
        Some("bl") => Point2D::new(world_bb.min_x, world_bb.min_y),
        Some("br") => Point2D::new(world_bb.max_x, world_bb.min_y),
        Some("tl") => Point2D::new(world_bb.min_x, world_bb.max_y),
        Some("tr") => Point2D::new(world_bb.max_x, world_bb.max_y),
        _ => {
            return mark(two_opt_improve(nearest_neighbor_order(snake_infill(rotated, rows, angle))))
        }
    };
    let variant = |mirror: bool| -> Vec<PathSegment> {
        let segs = if mirror {
            let flip = |p: &Point2D| Point2D::new(-p.x, p.y);
            let flipped: Vec<Contour> = rotated
                .iter()
                .map(|c| {
                    // Zrcadlení obrací orientaci obrysu — pořadí bodů se vrací zpět
                    let pts: Vec<Point2D> = c.pts.iter().rev().map(flip).collect();
                    Contour::new(pts, c.group, c.src, c.stroke).with_hole(c.hole)
                })
                .collect();
            snake_infill(&flipped, rows, 0.0)
                .into_iter()
                .map(|s| {
                    PathSegment::new(
                        s.points.iter().map(|p| rotate_pt(flip(p), angle, 0.0, 0.0)).collect(),
                    )
                })
                .collect()
        } else {
            snake_infill(rotated, rows, angle)
        };
        let mut segs = two_opt_improve(nearest_neighbor_order_from(segs, corner));
        // Jediná dráha se v NN neotáčí — začátek k rohu
        if let Some(first) = segs.first_mut() {
            let (a, b) = (first.points[0], *first.points.last().unwrap());
            if b.distance(&corner) < a.distance(&corner) {
                first.points.reverse();
            }
        }
        segs
    };
    let dist = |v: &[PathSegment]| v.first().map_or(f64::INFINITY, |s| s.points[0].distance(&corner));
    let (a, b) = (variant(false), variant(true));
    mark(if dist(&b) + 1e-9 < dist(&a) { b } else { a })
}

/// Výplň "Jedna linka": celý objekt jedinou souvislou dráhou, která nikde
/// nekříží ani se nedotkne sama sebe. Jde-li objekt projít hadem v jednom
/// kuse, použije se had. Jinak se řady spárují do pásů (0–1, 2–3, …) a obvod
/// každé souvislé části pásu — dolní úseky zleva doprava, horní zprava doleva,
/// mezi nimi obrys — je uzavřená smyčka. V mezeře mezi sousedními pásy jdou
/// horní úseky jednoho a dolní úseky druhého proti sobě: smyčky se tam spojí
/// mezerou v obou řadách a dvěma příčkami (pokrytí po kostře, STC). V pásu leží
/// jen obrys a v mezeře mezi pásy jen příčky, takže se dráha nemůže křížit.
/// Hotová smyčka se rozpojí na obrysovém spoji. Nesouvislé části (a části,
/// které příčkou spojit nejde) zůstanou samostatnými drahami.
fn oneline_paths(world: &[Contour], rotated: &[Contour], rows: &[f64], angle: f64) -> Vec<PathSegment> {
    let snake = snake_infill(rotated, rows, angle);
    if snake.len() <= 1 {
        return snake.into_iter().map(|s| PathSegment { snake: true, ..s }).collect();
    }
    let (spans, along) = scan_spans(rotated, rows);
    let n = spans.len();
    let row_of = |s: usize| spans[s].row;

    // 1. Spojky po obrysu uvnitř pásů. Každý konec úseku má po obrysu jednoho
    //    souseda nad řadou a jednoho pod ní — v pásu leží právě jeden z nich.
    //    link[konec] = (druhý konec, body obrysu od tohoto konce)
    let mut link: Vec<Option<(usize, Vec<Point2D>)>> = vec![None; 2 * n];
    for (pi, list) in along.iter().enumerate() {
        let m = list.len();
        if m < 2 {
            continue;
        }
        let poly = &rotated[pi].pts;
        for j in 0..m {
            let (Some(a), Some(b)) = (list[j].2, list[(j + 1) % m].2) else { continue };
            let (ha, hb) = (spans[a / 2].ends[a % 2], spans[b / 2].ends[b % 2]);
            let pts = outline_between(poly, ha, hb);
            let (ra, rb) = (row_of(a / 2), row_of(b / 2));
            let strip_lo = if ra != rb {
                ra.min(rb)
            } else {
                // Obrys se vrací do stejné řady — nad ní, nebo pod ní?
                let y = spans[a / 2].y;
                match pts.iter().find(|p| (p.y - y).abs() > 1e-9) {
                    Some(p) if p.y > y => ra,
                    Some(_) if ra > 0 => ra - 1,
                    _ => continue,
                }
            };
            // Pás k tvoří řady 2k a 2k+1 — mezera nad sudou řadou
            if !strip_lo.is_multiple_of(2) || link[a].is_some() || link[b].is_some() || a == b {
                continue;
            }
            link[b] = Some((a, pts.iter().rev().copied().collect()));
            link[a] = Some((b, pts));
        }
    }

    // 2. Smyčky (u porušených obrysů cesty) z úseků a spojek. Směr: úseky
    //    sudých řad zleva doprava — liché pak vedou zprava doleva.
    //    entry[úsek] = konec, kterým se do úseku vstupuje
    let mut comp = vec![usize::MAX; n];
    let mut entry = vec![0usize; n];
    let mut is_path: Vec<bool> = Vec::new();
    let mut starts: Vec<usize> = Vec::new();
    let order: Vec<usize> = (0..n).filter(|&s| row_of(s) % 2 == 0).chain((0..n).filter(|&s| row_of(s) % 2 == 1)).collect();
    for &s0 in &order {
        if comp[s0] != usize::MAX {
            continue;
        }
        let c = is_path.len();
        // Úsek liché řady jako první (smyčka bez sudých řad) se prochází zprava
        let mut e0 = if row_of(s0) % 2 == 0 { 0 } else { 1 };
        // Cesta: dojít proti směru k jejímu začátku
        let mut open = false;
        let (mut s, mut e) = (s0, e0);
        for _ in 0..n {
            let Some((prev_exit, _)) = &link[2 * s + e] else {
                open = true;
                break;
            };
            let p = prev_exit / 2;
            if p == s0 {
                break;
            }
            (s, e) = (p, 1 - prev_exit % 2);
        }
        if open {
            e0 = e;
        }
        let first = if open { s } else { s0 };
        let (mut s, mut e) = (first, e0);
        loop {
            comp[s] = c;
            entry[s] = e;
            let exit = 2 * s + (1 - e);
            match &link[exit] {
                Some((next, _)) if comp[next / 2] == usize::MAX => (s, e) = (next / 2, next % 2),
                _ => break,
            }
        }
        is_path.push(open);
        starts.push(first);
    }

    // 3. Spojení smyček příčkami v mezerách mezi pásy (řady 2k+1 a 2k+2).
    //    Mezera g v obou úsecích, příčky spojují stejnolehlé okraje mezer —
    //    svisle, kde se úseky překrývají, jinak šikmo (rovnoběžně).
    //    cuts[úsek] = (od, do, druhý úsek, posun mezery ve druhém úseku).
    let spacing = rows.windows(2).map(|w| w[1] - w[0]).fold(f64::INFINITY, f64::min);
    let spacing = if spacing.is_finite() { spacing } else { 1.0 };
    let geo = regions::FillGeodesics::new(world, Vec::new());
    let back = |p: Point2D| rotate_pt(p, angle, 0.0, 0.0);
    let mut parent: Vec<usize> = (0..is_path.len()).collect();
    let mut cuts: Vec<Vec<(f64, f64, usize, f64)>> = vec![Vec::new(); n];
    // Příčky v každé mezeře mezi pásy — nové se s nimi nesmí křížit
    let mut rungs: Vec<Vec<(Point2D, Point2D)>> = vec![Vec::new(); rows.len()];
    let span_x = |s: usize| (spans[s].ends[0].x, spans[s].ends[1].x);
    let free = |cuts: &[(f64, f64, usize, f64)], lo: f64, hi: f64| cuts.iter().all(|&(a, b, _, _)| hi < a || lo > b);
    let mut by_row: Vec<Vec<usize>> = vec![Vec::new(); rows.len()];
    for s in 0..n {
        by_row[row_of(s)].push(s);
    }
    for r in (1..rows.len().saturating_sub(1)).step_by(2) {
        for &t in &by_row[r] {
            for &u in &by_row[r + 1] {
                let (ct, cu) = (regions::uf_find(&mut parent, comp[t]), regions::uf_find(&mut parent, comp[u]));
                // Dvě cesty spojením nevzniknou jedna; úseky musí jít proti sobě
                if ct == cu || (is_path[ct] && is_path[cu]) || entry[t] == entry[u] {
                    continue;
                }
                let ((tl, tr), (ul, ur)) = (span_x(t), span_x(u));
                let (lo, hi) = (tl.max(ul), tr.min(ur));
                // Středy mezer (v t, v u): v překryvu svisle, jinak u bližších konců
                let mut g = spacing.min((tr - tl) / 2.0).min((ur - ul) / 2.0);
                if hi - lo < 2.0 * g && hi - lo >= spacing * 0.1 {
                    g = (hi - lo) / 2.0;
                }
                if g < spacing * 0.05 {
                    continue;
                }
                let margin = g / 4.0;
                let e = margin + g / 2.0;
                let candidates = if hi - lo >= 2.0 * g {
                    vec![(lo + e, lo + e), (hi - e, hi - e), ((lo + hi) / 2.0, (lo + hi) / 2.0)]
                } else if hi - lo >= spacing * 0.1 {
                    vec![((lo + hi) / 2.0, (lo + hi) / 2.0)]
                } else if ur < tl + g {
                    vec![(tl + e, ur - e)]
                } else {
                    vec![(tr - e, ul + e)]
                };
                let (yt, yu) = (spans[t].y, spans[u].y);
                let fits = |xa: f64, xb: f64, cuts: &[Vec<(f64, f64, usize, f64)>], rungs: &[(Point2D, Point2D)]| {
                    free(&cuts[t], xa - g / 2.0 - margin, xa + g / 2.0 + margin)
                        && free(&cuts[u], xb - g / 2.0 - margin, xb + g / 2.0 + margin)
                        && [-g / 2.0, g / 2.0].iter().all(|&o| {
                            let (p, q) = (Point2D::new(xa + o, yt), Point2D::new(xb + o, yu));
                            let (pw, qw) = (back(p), back(q));
                            geo.visible(pw, qw)
                                && !geo.touches_vertex(pw, qw)
                                && rungs.iter().all(|&(c, d)| !segments_touch(p, q, c, d))
                        })
                };
                let Some((xa, xb)) = candidates.into_iter().find(|&(xa, xb)| fits(xa, xb, &cuts, &rungs[r]))
                else {
                    continue;
                };
                cuts[t].push((xa - g / 2.0, xa + g / 2.0, u, xb - xa));
                cuts[u].push((xb - g / 2.0, xb + g / 2.0, t, xa - xb));
                for o in [-g / 2.0, g / 2.0] {
                    rungs[r].push((Point2D::new(xa + o, yt), Point2D::new(xb + o, yu)));
                }
                parent[cu] = ct;
                is_path[ct] |= is_path[cu];
            }
        }
    }

    // 4. Průchod: po úseku k nejbližší mezeře (příčka do druhého úseku), nebo
    //    ke konci úseku (spojka po obrysu do dalšího úseku pásu).
    let mut out: Vec<PathSegment> = Vec::new();
    let mut done = vec![false; is_path.len()];
    // Začátek: cesta začíná svým volným koncem, smyčka nejnižším úsekem
    let firsts: Vec<(usize, usize)> =
        starts.iter().enumerate().map(|(c, &s)| (regions::uf_find(&mut parent, c), s)).collect();
    // Drobek na špičce (úsek užší než mezera pro příčky), který se nepřipojil,
    // by byl samostatnou dráhou — jeho plochu pokryje sousední řada
    let mut row_len = vec![0.0; is_path.len()];
    for sp in 0..n {
        row_len[regions::uf_find(&mut parent, comp[sp])] += span_x(sp).1 - span_x(sp).0;
    }
    let roots = (0..is_path.len()).filter(|&c| regions::uf_find(&mut parent, c) == c).count();
    for &(root, s0) in &firsts {
        if done[root] || (roots > 1 && row_len[root] < spacing / 2.0) {
            continue;
        }
        let s0 = if is_path[root] {
            // Volný začátek cesty patří některé ze sloučených složek
            match firsts.iter().find(|&&(r, s)| r == root && link[2 * s + entry[s]].is_none()) {
                Some(&(_, s)) => s,
                None => s0,
            }
        } else {
            s0
        };
        done[root] = true;
        let mut pts: Vec<Point2D> = Vec::new();
        let (mut s, mut x) = (s0, span_x(s0).0);
        if entry[s] == 1 {
            x = span_x(s).1;
        }
        pts.push(Point2D::new(x, spans[s].y));
        for _ in 0..4 * n + 4 * cuts.iter().map(Vec::len).sum::<usize>() + 4 {
            let right = entry[s] == 0;
            let next_cut = cuts[s]
                .iter()
                .filter(|c| if right { c.0 > x + 1e-9 } else { c.1 < x - 1e-9 })
                .min_by(|a, b| {
                    let (da, db) = if right { (a.0, b.0) } else { (-a.1, -b.1) };
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                })
                .copied();
            if let Some((lo, hi, other, dx)) = next_cut {
                x = if right { lo } else { hi };
                pts.push(Point2D::new(x, spans[s].y));
                s = other;
                x += dx;
                pts.push(Point2D::new(x, spans[s].y));
                continue;
            }
            let exit = 2 * s + (1 - entry[s]);
            pts.push(Point2D::new(spans[s].ends[exit % 2].x, spans[s].y));
            let Some((next, conn)) = &link[exit] else { break };
            let t = next / 2;
            // Smyčka se uzavírá — poslední spojka po obrysu se netiskne
            if t == s0 && *next == 2 * s0 + entry[s0] {
                break;
            }
            pts.extend_from_slice(conn);
            s = t;
            x = spans[s].ends[next % 2].x;
            pts.push(Point2D::new(x, spans[s].y));
        }
        pts.dedup_by(|a, b| (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9);
        out.push(PathSegment {
            snake: true,
            ..PathSegment::new(pts.into_iter().map(back).collect())
        });
    }
    out
}

// ─── Výplň a okraje objektu ──────────────────────────────────────────────────

/// Efektivní nastavení výplně objektu: přepis objektu, jinak nastavení sklíčka.
struct InfillSettings<'a> {
    style: &'a str,
    val: f64,
    typ: &'a str,
    /// Začátek hada v rohu obálky objektu ("bl", "br", "tl", "tr"); None = automaticky.
    snake_start: Option<&'a str>,
}

fn infill_for(p: &SliceParams, id: u32) -> InfillSettings<'_> {
    let o = p.object_overrides.get(&id.to_string());
    let style = o
        .and_then(|o| o.infill_style.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or(&p.infill_style);
    let (val, typ) = match o.and_then(|o| o.infill_val) {
        Some(v) => (v, o.and_then(|o| o.infill_type.as_deref()).unwrap_or(&p.infill_type)),
        None => (p.infill_val, p.infill_type.as_str()),
    };
    let snake_start = o
        .and_then(|o| o.snake_start.as_deref())
        .filter(|c| matches!(*c, "bl" | "br" | "tl" | "tr"));
    InfillSettings { style, val, typ, snake_start }
}

/// Rozestup řádků výplně; pro "počet" se dopočítá z výšky objektu (zde 0).
fn spacing_base(s: &InfillSettings, nozzle_diam: f64) -> f64 {
    if s.typ == "%" && s.val > 0.0 {
        nozzle_diam / (s.val / 100.0)
    } else if s.typ == "počet" {
        0.0
    } else {
        s.val
    }
}

/// Rozestup teček na čarách a okrajích ("počet" nedává bez výšky smysl → 1 mm).
fn path_dot_spacing(s: &InfillSettings, nozzle_diam: f64) -> f64 {
    spacing_base(s, nozzle_diam).max(if s.typ == "počet" { 1.0 } else { 0.0 })
}

/// Kreslí styl okraj? SVG tah je navíc podmínkou (řeší volající).
fn style_draws_outline(style: &str, filled: bool) -> bool {
    match style {
        "Okraje + Výplň" | "Okraje" | "Mřížka" => true,
        "Had" | "Tečky" | "Jedna linka" => !filled,
        _ => false,
    }
}

fn bbox_union(contours: &[Contour]) -> Bbox {
    contours
        .iter()
        .map(|c| c.bbox)
        .reduce(|a, b| a.union(&b))
        .expect("objekt má aspoň jeden obrys")
}

/// Úseky řad výplně (levý, pravý konec) v otočeném prostoru obrysů — pro
/// každou řadu zleva doprava.
fn scan_rows(contours: &[Contour], rows: &[f64], inset: f64) -> Vec<Vec<(Point2D, Point2D)>> {
    let (mut hits, mut spans) = (Vec::new(), Vec::new());
    rows.iter()
        .map(|&y| {
            regions::row_hits(contours, y, &mut hits);
            regions::row_spans(contours, &hits, &mut spans);
            spans
                .iter()
                .filter_map(|[l, r]| {
                    let (x_left, x_right) = (l.x + inset, r.x - inset);
                    (x_left < x_right).then(|| (Point2D::new(x_left, y), Point2D::new(x_right, y)))
                })
                .collect()
        })
        .collect()
}

/// Boustrofedon přes úseky řad. Úseky se skládají do řetězců — každý další
/// úsek leží v sousední řadě a překrývá se s předchozím — takže oblast vedle
/// díry nebo výběžek konkávního tvaru se vyplní celý, než tryska přejede jinam
/// (dřívější střídání směru podle pořadí úseku vedlo přes díru dvakrát na řadu).
/// Řetězce se řadí hladově od bodu `start`, každý lze projít odspodu i shora;
/// úsek začíná koncem bližším konci předchozího. Vrací úseky v tiskovém pořadí.
fn chain_rows(rows: &[Vec<(Point2D, Point2D)>], start: Point2D) -> Vec<(Point2D, Point2D)> {
    // 1. Řetězce: k úseku se přidává nejvíc překrývající se volný úsek další řady
    let mut used: Vec<Vec<bool>> = rows.iter().map(|r| vec![false; r.len()]).collect();
    let mut chains: Vec<Vec<(usize, usize)>> = Vec::new();
    for r in 0..rows.len() {
        for i in 0..rows[r].len() {
            if used[r][i] {
                continue;
            }
            used[r][i] = true;
            let mut chain = vec![(r, i)];
            let (mut cr, mut ci) = (r, i);
            while cr + 1 < rows.len() {
                let (a, b) = rows[cr][ci];
                let next = rows[cr + 1]
                    .iter()
                    .enumerate()
                    .filter(|&(j, _)| !used[cr + 1][j])
                    .map(|(j, &(c, d))| (j, b.x.min(d.x) - a.x.max(c.x)))
                    .filter(|&(_, overlap)| overlap > 0.0)
                    .max_by(|x, y| x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal));
                let Some((j, _)) = next else { break };
                used[cr + 1][j] = true;
                chain.push((cr + 1, j));
                (cr, ci) = (cr + 1, j);
            }
            chains.push(chain);
        }
    }

    // 2. Pořadí řetězců a směr úseků: hladově od aktuální polohy trysky
    let mut out = Vec::with_capacity(rows.iter().map(Vec::len).sum());
    let mut cur = start;
    let mut left: Vec<usize> = (0..chains.len()).collect();
    while !left.is_empty() {
        let mut best = (f64::INFINITY, 0usize, false);
        for (k, &c) in left.iter().enumerate() {
            for rev in [false, true] {
                let &(r, i) = if rev { chains[c].last() } else { chains[c].first() }.unwrap();
                let (a, b) = rows[r][i];
                let d = a.distance(&cur).min(b.distance(&cur));
                if d < best.0 - 1e-9 {
                    best = (d, k, rev);
                }
            }
        }
        let c = left.remove(best.1);
        let steps: Box<dyn Iterator<Item = &(usize, usize)>> =
            if best.2 { Box::new(chains[c].iter().rev()) } else { Box::new(chains[c].iter()) };
        for &(r, i) in steps {
            let (a, b) = rows[r][i];
            let (a, b) = if b.distance(&cur) < a.distance(&cur) { (b, a) } else { (a, b) };
            out.push((a, b));
            cur = b;
        }
    }
    out
}

/// Výplň jednoho objektu (obrysy ve světových souřadnicích sklíčka), již
/// v tiskovém pořadí.
fn object_infill(
    world: &[Contour],
    s: &InfillSettings,
    p: &SliceParams,
    inset: f64,
) -> Vec<PathSegment> {
    let angle = p.infill_angle;
    let rotated: Vec<Contour> = world.iter().map(|c| c.rotated(-angle)).collect();
    let bb = bbox_union(&rotated);
    // "počet" = přesný počet řad přes celou výšku objektu (krajní na okrajích)
    let count = (s.typ == "počet" && s.val >= 1.0).then(|| s.val.round().max(1.0) as usize);
    let spacing = spacing_base(s, p.nozzle_diam);
    if count.is_none() && (spacing.is_nan() || spacing <= 0.0) {
        return vec![];
    }
    let back = |pt: Point2D| rotate_pt(pt, angle, 0.0, 0.0);
    // Rozestup z procent i počet: krajní řady na okrajích tvaru (zachová rozměry)
    let fit = s.typ == "%" || count.is_some();
    let margin = if fit { inset } else { 0.0 };
    let spec = match count {
        Some(n) => RowSpec::Count(n),
        None if fit => RowSpec::Fit(spacing),
        None => RowSpec::Step(spacing),
    };
    let bottom_left = Point2D::new(bb.min_x, bb.min_y);

    match s.style {
        "Tečky" => {
            let (w, h) = (bb.max_x - bb.min_x, bb.max_y - bb.min_y);
            // Rozteč v řadě: u počtu stejná jako mezi řadami (čtvercová mřížka
            // přes celý tvar), jediná řada dostane rozteč rovnou výšce objektu
            let pitch = match count {
                Some(n) if n > 1 => (h / (n - 1) as f64).max(1e-6),
                Some(_) => h.max(w / 2.0).max(1e-6),
                None => spacing,
            };
            // Ochrana před přetížením: max 100 000 bodů (jako Python verze)
            let (spec_y, pitch) = if (w / pitch + 1.0) * (h / pitch + 1.0) > 100_000.0 {
                let d = (w * h / 100_000.0).sqrt();
                (if fit { RowSpec::Fit(d) } else { RowSpec::Step(d) }, d)
            } else {
                (spec, pitch)
            };
            let mut dots = Vec::new();
            let mut reverse = false;
            let (mut hits, mut spans) = (Vec::new(), Vec::new());
            for y in row_positions(bb.min_y, bb.max_y, spec_y, 0.0) {
                let mut xs: Vec<f64> = if fit {
                    // Krajní tečky na okrajích každého úseku řady — zachová rozměry
                    // i u oblých tvarů (mřížka obálky by je u kruhu ztratila)
                    regions::row_hits(&rotated, y, &mut hits);
                    regions::row_spans(&rotated, &hits, &mut spans);
                    spans
                        .iter()
                        .flat_map(|[l, r]| {
                            let len = r.x - l.x;
                            let n = if len <= pitch / 2.0 { 0 } else { (len / pitch - 1e-9).ceil().max(1.0) as usize };
                            let (x0, x1) = (l.x + EDGE_ROW_EPS, r.x - EDGE_ROW_EPS);
                            (0..=n).map(move |k| {
                                if n == 0 { (l.x + r.x) / 2.0 } else { x0 + (x1 - x0) * k as f64 / n as f64 }
                            })
                        })
                        .collect()
                } else {
                    // Pevný rozestup: pravidelná mřížka zarovnaná přes celý objekt
                    row_positions(bb.min_x, bb.max_x, RowSpec::Step(pitch), 0.0)
                        .into_iter()
                        .filter(|&x| regions::in_fill(Point2D::new(x, y), &rotated))
                        .collect()
                };
                if reverse {
                    xs.reverse();
                }
                dots.extend(xs.into_iter().map(|x| {
                    let pt = back(Point2D::new(x, y));
                    PathSegment::new(vec![pt, pt])
                }));
                reverse = !reverse;
            }
            two_opt_improve(nearest_neighbor_order(dots))
        }
        "Jedna linka" => oneline_paths(
            world,
            &rotated,
            &row_positions(bb.min_y, bb.max_y, spec, margin),
            angle,
        ),
        "Had" => snake_paths(
            &rotated,
            &bbox_union(world),
            &row_positions(bb.min_y, bb.max_y, spec, margin),
            angle,
            s.snake_start,
        ),
        style => {
            let rows = row_positions(bb.min_y, bb.max_y, spec, margin);
            let lines = chain_rows(&scan_rows(&rotated, &rows, inset), bottom_left);
            let exit = lines.last().map(|&(_, b)| back(b));
            let mut segs: Vec<PathSegment> =
                lines.into_iter().map(|(a, b)| PathSegment::new(vec![back(a), back(b)])).collect();
            if style == "Mřížka" {
                // Svislé linky: skenovat obrysy otočené o dalších -90°; navazují
                // tam, kde skončily vodorovné
                let rotated_90: Vec<Contour> =
                    world.iter().map(|c| c.rotated(-(angle + 90.0))).collect();
                let bb90 = bbox_union(&rotated_90);
                let back_90 = |pt: Point2D| rotate_pt(pt, 90.0 + angle, 0.0, 0.0);
                let start_90 = exit
                    .map(|e| rotate_pt(e, -(angle + 90.0), 0.0, 0.0))
                    .unwrap_or(Point2D::new(bb90.min_x, bb90.min_y));
                let rows_v = row_positions(bb90.min_y, bb90.max_y, spec, margin);
                segs.extend(
                    chain_rows(&scan_rows(&rotated_90, &rows_v, inset), start_90)
                        .into_iter()
                        .map(|(a, b)| PathSegment::new(vec![back_90(a), back_90(b)])),
                );
            }
            segs
        }
    }
}

// ─── Hlavní funkce ────────────────────────────────────────────────────────────

/// Port TypeScript `processSubstratePaths()` do Rustu.
/// Normalizuje, škáluje a centruje dráhy, rozloží je na objekty a každému
/// vygeneruje výplň a okraj podle jeho nastavení (objekt → sklíčko).
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
    let norm = |pt: Point2D| Point2D::new(pt.x - min_x, height - (pt.y - min_y));
    let mut objects: Vec<RawGeometry> = raw
        .segments
        .iter()
        .enumerate()
        .map(|(src, seg)| {
            let pts: Vec<Point2D> = seg.points.iter().map(|&pt| norm(pt)).collect();
            let closed = is_path_closed(&pts);
            RawGeometry {
                is_closed: closed,
                points: pts,
                is_filled: seg.is_filled,
                has_stroke: seg.has_stroke,
                group: seg.group,
                src,
                hole: seg.hole.unwrap_or(false),
                anchors: seg
                    .anchors
                    .iter()
                    .map(|a| (a.end, a.object, norm(Point2D::new(a.x, a.y))))
                    .collect(),
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
            for pt in obj.points.iter_mut().chain(obj.anchors.iter_mut().map(|a| &mut a.2)) {
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
        for pt in obj.points.iter_mut().chain(obj.anchors.iter_mut().map(|a| &mut a.2)) {
            pt.x += off_x;
            pt.y += off_y;
        }
    }

    // 3b. Objekty se určí na neupravené geometrii — úprava objektu (posun do
    //     jiného objektu apod.) tak nemění id ani nastavení objektů.
    let is_fill_shape =
        |o: &RawGeometry| o.is_closed && o.points.len() >= 3 && (o.hole || o.is_filled.unwrap_or(true));
    let object_of = object_ids(&objects, &is_fill_shape);

    // 3c. Úpravy objektů (posun, měřítko, rotace kolem středu obálky objektu).
    //     Vlastní úprava sklíčka má přednost před sdílenou z kresby.
    let mut base_bb: HashMap<u32, Bbox> = HashMap::new();
    for obj in &objects {
        let (Some(&id), false) = (object_of.get(&obj.src), obj.points.is_empty()) else { continue };
        let bb = Bbox::of(&obj.points);
        base_bb.entry(id).and_modify(|b| *b = b.union(&bb)).or_insert(bb);
    }
    let xform_of = |id: u32| -> Option<ObjectTransform> {
        let key = id.to_string();
        p.object_transforms
            .get(&key)
            .or_else(|| raw.object_transforms.get(&key))
            .copied()
            .filter(|t| !t.is_identity())
    };
    let moved = |id: u32, pt: Point2D| -> Point2D {
        match (xform_of(id), base_bb.get(&id)) {
            (Some(t), Some(b)) => t.apply(pt, Point2D::new((b.min_x + b.max_x) / 2.0, (b.min_y + b.max_y) / 2.0)),
            _ => pt,
        }
    };
    for obj in &mut objects {
        if let Some(&id) = object_of.get(&obj.src) {
            if xform_of(id).is_some() {
                for pt in &mut obj.points {
                    *pt = moved(id, *pt);
                }
            }
        }
        // Konce čáry přichycené k trase objektu sledují jeho úpravu
        for &(end, target, a) in &obj.anchors {
            if obj.points.is_empty() || !base_bb.contains_key(&target) {
                continue;
            }
            let k = if end == 0 { 0 } else { obj.points.len() - 1 };
            obj.points[k] = moved(target, a);
        }
    }

    // 4. Uživatelské měřítko kolem středu sklíčka (slide_w/2, slide_h/2) —
    //    shodné s Canvas2D transform_pt, kde cx = slide_w/2, cy = slide_h/2.
    //    Jiný střed způsoboval vizuální skok při baking resetu scale → 1.0.
    //    Osa Y může mít vlastní měřítko (škálování s odemčeným poměrem stran).
    let us = p.user_scale;
    let us_y = p.user_scale_y.unwrap_or(us);
    if (us - 1.0).abs() > 1e-6 || (us_y - 1.0).abs() > 1e-6 {
        let cx = p.slide_w / 2.0;
        let cy = p.slide_h / 2.0;
        for obj in &mut objects {
            for pt in &mut obj.points {
                pt.x = cx + (pt.x - cx) * us;
                pt.y = cy + (pt.y - cy) * us_y;
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
    let centroid =
        |pts: &[Point2D]| -> (f64, f64) {
            let n = pts.len().max(1) as f64;
            (
                pts.iter().map(|p| p.x).sum::<f64>() / n,
                pts.iter().map(|p| p.y).sum::<f64>() / n,
            )
        };
    let cmp_centroids = |a: (f64, f64), b: (f64, f64)| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    };
    objects.sort_by(|a, b| cmp_centroids(centroid(&a.points), centroid(&b.points)));

    let nozzle = p.nozzle_diam;
    let mut objects_out: Vec<PathObject> = Vec::new();
    let frame_of = |id: u32| base_bb.get(&id).map(|b| [b.min_x, b.max_x, b.min_y, b.max_y]);

    // 6. Vyplněné tvary → obrysy výplně, rozložené na objekty (ostrůvky).
    //    Prvky se sjednocují, uvnitř prvku platí even-odd (díry).
    let mut group_keys: Vec<Option<u32>> = Vec::new();

    // Smazané objekty (z tohoto sklíčka nebo ze sdílené kresby) se vyřadí celé.
    // Obálka kresby výše je spočítaná i s nimi, takže se zbytek neposune.
    let deleted: HashSet<u32> =
        p.deleted_objects.iter().chain(&raw.deleted_objects).copied().collect();
    if !deleted.is_empty() {
        objects.retain(|o| !object_of.get(&o.src).is_some_and(|id| deleted.contains(id)));
    }
    let contours: Vec<Contour> = objects
        .iter()
        .filter(|o| is_fill_shape(o))
        .map(|o| {
            let g = match group_keys.iter().position(|k| *k == o.group) {
                Some(i) => i,
                None => {
                    group_keys.push(o.group);
                    group_keys.len() - 1
                }
            };
            Contour::new(o.points.clone(), g, o.src, o.has_stroke.unwrap_or(true)).with_hole(o.hole)
        })
        .collect();
    // Objekty = ostrůvky v rámci jednoho prvku (každé písmeno, každý nakreslený
    // tvar), určené výše. Tiskové oblasti = ostrůvky sjednocení přes prvky:
    // překrývající se objekty se tisknou jako jedna dráha, v seznamu ale zůstávají
    // samostatné.
    let mut object_islands: Vec<(u32, Vec<usize>)> = Vec::new();
    for (ci, c) in contours.iter().enumerate() {
        let Some(&id) = object_of.get(&c.src) else { continue };
        match object_islands.iter_mut().find(|(oid, _)| *oid == id) {
            Some((_, members)) => members.push(ci),
            None => object_islands.push((id, vec![ci])),
        }
    }
    let mut islands = regions::find_islands(&contours);
    islands.sort_by(|a, b| {
        let pts_of = |members: &[usize]| -> Vec<Point2D> {
            members.iter().flat_map(|&m| contours[m].pts.iter().copied()).collect()
        };
        cmp_centroids(centroid(&pts_of(&a.members)), centroid(&pts_of(&b.members)))
    });

    // Tisková oblast každého objektu: přes společný obrys, jinak (objekt celý
    // uvnitř jiného) podle oblasti, ve které leží.
    let mut island_of_contour: Vec<Option<usize>> = vec![None; contours.len()];
    for (k, isl) in islands.iter().enumerate() {
        for &m in &isl.members {
            island_of_contour[m] = Some(k);
        }
    }
    let object_island: Vec<(u32, Option<usize>, bool)> = object_islands
        .iter()
        .map(|(id, members)| {
            match members.iter().find_map(|&m| island_of_contour[m]) {
                Some(k) => (*id, Some(k), true),
                None => {
                    let probe = contours[members[0]].pts[0];
                    let k = islands.iter().position(|isl| {
                        let cs: Vec<Contour> =
                            isl.members.iter().map(|&m| contours[m].clone()).collect();
                        regions::in_fill(probe, &cs)
                    });
                    (*id, k, false)
                }
            }
        })
        .collect();
    let is_hole_object = |id: u32| objects.iter().any(|o| o.src == id as usize && o.hole);
    // Nastavení sloučené dráhy určuje nejstarší (nedírový) objekt, který tvoří její okraj
    let primary_of = |k: usize| -> Option<u32> {
        let ids = |boundary_only: bool| {
            object_island
                .iter()
                .filter(move |&&(id, ik, boundary)| {
                    ik == Some(k) && (boundary || !boundary_only) && !is_hole_object(id)
                })
                .map(|&(id, _, _)| id)
                .min()
        };
        ids(true).or_else(|| ids(false))
    };

    // Tiskové jednotky: dráhy jedné tiskové oblasti nebo jedné čáry. Pořadí tisku
    // určuje pořadí objektů, jednotky spojené čárou se tisknou za sebou.
    let mut units: Vec<PrintUnit> = Vec::new();
    let mut unit_of_object: HashMap<u32, usize> = HashMap::new();
    for (k, island) in islands.iter().enumerate() {
        let primary = primary_of(k).unwrap_or(island.id);
        let members: Vec<Contour> =
            island.members.iter().map(|&m| contours[m].clone()).collect();
        // Sloučený tvar: pracuje se s hranicí sjednocení, aby výplň i okraj šly
        // po skutečném obrysu výsledného tvaru (jedna trasa přes překryv).
        let (world, exact_outline) = if island.merged {
            match regions::union_loops(&members) {
                Some(loops) if !loops.is_empty() => (loops, true),
                _ => (members, false),
            }
        } else {
            (members, true)
        };
        let s = infill_for(p, primary);
        let has_stroke = world.iter().any(|c| c.stroke);

        // Výplň (styl "Okraje" ji nechce). Inset od okraje o průměr trysky jen tam,
        // kde se okraj tiskne zároveň s výplní, aby se trasy nepřekrývaly.
        let mut segs = Vec::new();
        if s.style != "Okraje" {
            let inset = if matches!(s.style, "Okraje + Výplň" | "Mřížka") && has_stroke {
                nozzle
            } else {
                0.0
            };
            segs = object_infill(&world, &s, p, inset);
        }

        // Okraj — u sloučených tvarů jen hranice sjednocení. Kreslí se až po
        // výplni: zapečetí ji a zabrání vytékání kapaliny ven.
        let mut perims: Vec<PathSegment> = Vec::new();
        if style_draws_outline(s.style, true) {
            for (j, c) in world.iter().enumerate() {
                if !c.stroke {
                    continue;
                }
                let pieces = if exact_outline {
                    vec![c.pts.clone()]
                } else {
                    regions::boundary_pieces(&world, j)
                };
                for piece in pieces {
                    if s.style == "Tečky" {
                        perims.extend(
                            interpolate_along_path(&piece, path_dot_spacing(&s, nozzle))
                                .into_iter()
                                .map(|d| PathSegment::new(vec![d, d])),
                        );
                    } else {
                        perims.push(PathSegment::new(piece));
                    }
                }
            }
        }
        // Výplň s okrajem nejde tisknout pozpátku (okraj musí být poslední);
        // had s pevně zvoleným začátkem také ne
        // Spojitá výplň: přejezd uvnitř tvaru vede po ploše, kterou kapalina
        // pokryje — tryska se nemusí zvedat
        let wet_fill = !segs.is_empty() && matches!(s.style, "Výplň" | "Okraje + Výplň" | "Had");
        let reversible = (segs.is_empty() || perims.is_empty())
            && !(s.style == "Had" && s.snake_start.is_some());
        segs.extend(nearest_neighbor_order(perims));
        let segs: Vec<PathSegment> = segs
            .into_iter()
            .map(|mut seg| {
                seg.object = Some(primary);
                seg
            })
            .collect();

        // Stitching; had jen uvnitř výplně, jinak by spojka tiskla přes mezeru
        // (NN + 2-opt už proběhly per objekt).
        let segs = stitch_nearby_endpoints(segs, nozzle, |cur, next| {
            if s.style != "Had" {
                return true;
            }
            let (a, b) = (*cur.points.last().unwrap(), next.points[0]);
            regions::in_fill(Point2D::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0), &contours)
        });
        if segs.is_empty() {
            continue;
        }
        let members: Vec<u32> = object_island
            .iter()
            .filter(|o| o.1 == Some(k))
            .map(|o| o.0)
            .collect();
        for &m in &members {
            unit_of_object.insert(m, units.len());
        }
        units.push(PrintUnit {
            segs,
            members: members.into_iter().filter(|&m| !is_hole_object(m)).collect(),
            id: primary,
            reversible,
            links: Vec::new(),
            wet_fill,
        });
    }
    for (id, members) in &object_islands {
        let own: Vec<Contour> = members.iter().map(|&m| contours[m].clone()).collect();
        let bb = bbox_union(&own);
        let hole = is_hole_object(*id);
        // Díra, která nic neprotíná, leží v žádné oblasti — hledá se přes průnik bboxů
        let island = object_island.iter().find(|o| o.0 == *id).and_then(|o| o.1).or_else(|| {
            hole.then(|| {
                islands.iter().position(|isl| {
                    let ib = bbox_union(&isl.members.iter().map(|&m| contours[m].clone()).collect::<Vec<_>>());
                    ib.min_x <= bb.max_x && bb.min_x <= ib.max_x && ib.min_y <= bb.max_y && bb.min_y <= ib.max_y
                })
            })
            .flatten()
        });
        let merged_into = island.and_then(primary_of).filter(|primary| primary != id);
        objects_out.push(PathObject {
            id: *id,
            bounds: [bb.min_x, bb.max_x, bb.min_y, bb.max_y],
            filled: true,
            merged_into,
            hole,
            element: objects.iter().find(|o| o.src == *id as usize).and_then(|o| o.group),
            base_bounds: frame_of(*id),
            xform: xform_of(*id),
            sources: own.iter().map(|c| c.src as u32).collect(),
        });
    }

    // 7. Čáry a neplněné uzavřené tvary — každý je samostatný objekt. Konec
    //    přichycený k objektu se po úpravě objektu znovu napojí na jeho trasu
    //    (výplň se přegenerovala, bod přichycení nemusí přesně ležet na ní).
    let line_points = |id: u32| -> Option<&[Point2D]> {
        objects
            .iter()
            .find(|o| o.src == id as usize && !is_fill_shape(o))
            .map(|o| o.points.as_slice())
    };
    let mut line_units: Vec<PrintUnit> = Vec::new();
    for obj in &objects {
        if obj.points.is_empty() || is_fill_shape(obj) {
            continue;
        }
        let id = obj.src as u32;
        let s = infill_for(p, id);
        let stroke = obj.has_stroke.unwrap_or(true);
        let draw = stroke && (!obj.is_closed || style_draws_outline(s.style, false));
        if !draw {
            continue;
        }
        let mut pts = obj.points.clone();
        let mut links = Vec::new();
        for &(end, target, _) in &obj.anchors {
            let trasa: Vec<&[Point2D]> = match unit_of_object.get(&target) {
                Some(&u) => units[u].segs.iter().map(|s| s.points.as_slice()).collect(),
                None => line_points(target).into_iter().collect(),
            };
            if trasa.is_empty() || target == id {
                continue;
            }
            let k = if end == 0 { 0 } else { pts.len() - 1 };
            if xform_of(target).is_some() {
                pts[k] = resnap(pts[k], &trasa, 2.0 * nozzle);
            }
            links.push(target);
        }
        let tag = |points: Vec<Point2D>| PathSegment {
            object: Some(id),
            ..PathSegment::new(points)
        };
        let segs = if s.style == "Tečky" {
            interpolate_along_path(&pts, path_dot_spacing(&s, nozzle))
                .into_iter()
                .map(|d| tag(vec![d, d]))
                .collect()
        } else {
            vec![tag(pts.clone())]
        };
        let bb = Bbox::of(&pts);
        objects_out.push(PathObject {
            id,
            bounds: [bb.min_x, bb.max_x, bb.min_y, bb.max_y],
            filled: false,
            merged_into: None,
            hole: false,
            element: obj.group,
            base_bounds: frame_of(id),
            xform: xform_of(id),
            sources: vec![id],
        });
        line_units.push(PrintUnit { segs, members: vec![id], id, reversible: true, links, wet_fill: false });
    }
    for u in line_units {
        unit_of_object.insert(u.id, units.len());
        units.push(u);
    }

    // 8. Pořadí tisku: objekty v pořadí seznamu (lokální pořadí sklíčka, jinak
    //    podle id); objekty spojené čarou se tisknou za sebou nejkratší trasou.
    let order_key = |id: u32| -> (usize, u32) {
        match p.object_order.iter().position(|&o| o == id) {
            Some(i) => (i, 0),
            None => (usize::MAX, id),
        }
    };
    let combined = order_print_units(&units, &unit_of_object, &order_key, &contours);
    objects_out.sort_by_key(|o| order_key(o.id));
    SubstratePaths {
        segments: combined,
        objects: objects_out,
        deleted_objects: Vec::new(),
        object_transforms: HashMap::new(),
    }
}

/// Id objektu každého zdrojového segmentu. Objekt je ostrůvek v rámci prvku
/// (vyplněné tvary) nebo samostatná čára; id = nejmenší index zdrojového
/// segmentu vnějšího obrysu, takže je stabilní při přidávání dalších tvarů.
fn object_ids(
    objects: &[RawGeometry],
    is_fill_shape: &impl Fn(&RawGeometry) -> bool,
) -> HashMap<usize, u32> {
    let mut out = HashMap::new();
    let fills: Vec<&RawGeometry> = objects.iter().filter(|o| is_fill_shape(o)).collect();
    let mut groups: Vec<Option<u32>> = Vec::new();
    for o in &fills {
        if !groups.contains(&o.group) {
            groups.push(o.group);
        }
    }
    for g in groups {
        let subset: Vec<Contour> = fills
            .iter()
            .filter(|o| o.group == g)
            .map(|o| Contour::new(o.points.clone(), 0, o.src, o.has_stroke.unwrap_or(true)))
            .collect();
        for isl in regions::find_islands(&subset) {
            for &m in &isl.members {
                out.insert(subset[m].src, isl.id);
            }
        }
    }
    for o in objects {
        if !is_fill_shape(o) {
            out.insert(o.src, o.src as u32);
        }
    }
    out
}

// ─── Pořadí tisku a spojovací čáry ───────────────────────────────────────────

/// Dráhy jedné tiskové oblasti (objekt, případně sloučené objekty) nebo čáry.
struct PrintUnit {
    /// Dráhy ve vnitřním pořadí tisku (výplň, pak okraj).
    segs: Vec<PathSegment>,
    /// Objekty, jejichž pořadí určuje pořadí jednotky (bez děr).
    members: Vec<u32>,
    id: u32,
    /// Jednotku lze tisknout pozpátku (obrácené pořadí i směr drah).
    reversible: bool,
    /// Objekty, ke kterým je čára přichycená magnetem.
    links: Vec<u32>,
    /// Spojitá výplň — přejezdy uvnitř výplně se označí jako mokré.
    wet_fill: bool,
}

/// Krok tisku jednotky: dráha, směr a u uzavřené dráhy počáteční vrchol.
#[derive(Clone, Copy)]
struct Step {
    idx: usize,
    reverse: bool,
    rot: usize,
    fixed: bool,
}

/// Tolerance shody bodů napojení (mm).
const JOIN_EPS: f64 = 5e-3;

/// Vrchol uzavřené dráhy shodný s bodem `t`.
fn vertex_at(pts: &[Point2D], t: Point2D) -> Option<usize> {
    (0..pts.len() - 1).find(|&k| pts[k].distance(&t) < JOIN_EPS)
}

fn nearest_vertex(pts: &[Point2D], t: Point2D) -> usize {
    (0..pts.len() - 1)
        .min_by(|&a, &b| {
            pts[a].distance(&t).partial_cmp(&pts[b].distance(&t)).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(0)
}

/// Uzavřená dráha začínající vrcholem `k`.
fn rotate_closed(pts: &[Point2D], k: usize) -> Vec<Point2D> {
    let n = pts.len() - 1;
    let mut out = Vec::with_capacity(pts.len());
    out.extend_from_slice(&pts[k..n]);
    out.extend_from_slice(&pts[..k]);
    out.push(pts[k]);
    out
}

/// Kde se do jednotky vstoupí, přijíždí-li tryska z `from`.
fn unit_entry(u: &PrintUnit, rev: bool, from: Point2D) -> Point2D {
    let seg = if rev { u.segs.last() } else { u.segs.first() };
    let pts = &seg.expect("jednotka má dráhy").points;
    if is_path_closed(pts) {
        pts[vertex_at(pts, from).unwrap_or_else(|| nearest_vertex(pts, from))]
    } else if rev {
        *pts.last().unwrap()
    } else {
        pts[0]
    }
}

/// Průchod jednotkou z bodu `from`: (kroky, vstup, výstup). Uzavřená dráha
/// začíná tam, kam tryska přijela; poslední uzavřená dráha jinak končí v bodě
/// napojení další jednotky (`targets`), aby tisk pokračoval bez přejezdu.
fn plan_unit(u: &PrintUnit, rev: bool, from: Point2D, targets: &[Point2D]) -> (Vec<Step>, Point2D, Point2D) {
    let n = u.segs.len();
    let mut steps = Vec::with_capacity(n);
    let mut cur = from;
    let mut entry = None;
    for i in 0..n {
        let idx = if rev { n - 1 - i } else { i };
        let pts = &u.segs[idx].points;
        let (mut rot, mut fixed) = (0, false);
        let (first, last) = if is_path_closed(pts) {
            let exit = if i + 1 == n { targets.iter().find_map(|&t| vertex_at(pts, t)) } else { None };
            (rot, fixed) = match (vertex_at(pts, cur), exit) {
                (Some(k), _) | (None, Some(k)) => (k, true),
                (None, None) => (nearest_vertex(pts, cur), false),
            };
            (pts[rot], pts[rot])
        } else if rev {
            (*pts.last().unwrap(), pts[0])
        } else {
            (pts[0], *pts.last().unwrap())
        };
        entry.get_or_insert(first);
        steps.push(Step { idx, reverse: rev, rot, fixed });
        cur = last;
    }
    (steps, entry.unwrap_or(from), cur)
}

/// Dráhy jednotky v tiskovém pořadí. U spojité výplně se označí dráhy, na
/// které se přejíždí jen přes plochu výplně (`fill` = obrysy výplně sklíčka).
fn materialize(u: &PrintUnit, steps: &[Step], fill: &[Contour], out: &mut Vec<PathSegment>) {
    let mut prev_end: Option<Point2D> = None;
    for st in steps {
        let mut seg = u.segs[st.idx].clone();
        if is_path_closed(&seg.points) {
            if st.rot != 0 {
                seg.points = rotate_closed(&seg.points, st.rot);
            }
            seg.fixed_start = st.fixed;
        }
        if st.reverse {
            seg.points.reverse();
        }
        if let (true, Some(a), Some(&b)) = (u.wet_fill, prev_end, seg.points.first()) {
            seg.wet_travel = travel_inside_fill(fill, a, b);
        }
        prev_end = seg.points.last().copied().or(prev_end);
        out.push(seg);
    }
}

/// Vede přejezd a → b jen přes výplň (včetně jejího okraje)? Vzorkuje se po
/// 0,1 mm; bod do 0,02 mm od obrysu se počítá jako uvnitř (konce drah leží
/// na obrysu).
fn travel_inside_fill(fill: &[Contour], a: Point2D, b: Point2D) -> bool {
    const STEP: f64 = 0.1;
    const EDGE_TOL: f64 = 0.02;
    let n = (a.distance(&b) / STEP).ceil().max(1.0) as usize;
    (0..=n).all(|k| {
        let t = k as f64 / n as f64;
        let p = Point2D::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
        regions::in_fill(p, fill)
            || fill.iter().any(|c| {
                (0..c.pts.len()).any(|i| {
                    point_to_segment_dist(p, c.pts[i], c.pts[(i + 1) % c.pts.len()]) < EDGE_TOL
                })
            })
    })
}

/// Vloží konce spojovacích čar jako vrcholy do uzavřených drah, na jejichž
/// hraně leží — uzavřená dráha pak může v bodě napojení začít i skončit.
fn insert_join_vertices(units: &mut [PrintUnit], comp: &[usize]) {
    let ends: Vec<Point2D> = comp
        .iter()
        .filter(|&&u| !units[u].links.is_empty())
        .flat_map(|&u| &units[u].segs)
        .filter(|s| !is_path_closed(&s.points) && !s.points.is_empty())
        .flat_map(|s| [s.points[0], *s.points.last().unwrap()])
        .collect();
    for &u in comp {
        for seg in &mut units[u].segs {
            if !is_path_closed(&seg.points) {
                continue;
            }
            for &e in &ends {
                let pts = &seg.points;
                if vertex_at(pts, e).is_some() {
                    continue;
                }
                let hit = pts.windows(2).position(|w| point_to_segment_dist(e, w[0], w[1]) < 0.02);
                if let Some(k) = hit {
                    seg.points.insert(k + 1, e);
                }
            }
        }
    }
}

/// Protínají se nebo dotýkají úsečky a–b a c–d?
fn segments_touch(a: Point2D, b: Point2D, c: Point2D, d: Point2D) -> bool {
    const EPS: f64 = 1e-9;
    let cross = |o: Point2D, p: Point2D, q: Point2D| (p.x - o.x) * (q.y - o.y) - (p.y - o.y) * (q.x - o.x);
    let opposite = |u: f64, v: f64| (u > EPS && v < -EPS) || (u < -EPS && v > EPS);
    if opposite(cross(c, d, a), cross(c, d, b)) && opposite(cross(a, b, c), cross(a, b, d)) {
        return true;
    }
    point_to_segment_dist(a, c, d) < EPS
        || point_to_segment_dist(b, c, d) < EPS
        || point_to_segment_dist(c, a, b) < EPS
        || point_to_segment_dist(d, a, b) < EPS
}

fn point_to_segment_dist(p: Point2D, a: Point2D, b: Point2D) -> f64 {
    p.distance(&closest_on_segment(p, a, b))
}

fn closest_on_segment(p: Point2D, a: Point2D, b: Point2D) -> Point2D {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-18 {
        return a;
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0);
    Point2D::new(a.x + dx * t, a.y + dy * t)
}

/// Napojí bod `q` na trasu objektu: leží-li na ní, zůstane; jinak se přichytí ke
/// konci otevřené dráhy v okruhu `r` (tam jde tisk plynule napojit), k vrcholu
/// v okruhu `r / 2`, nebo k nejbližšímu místu trasy.
fn resnap(q: Point2D, trasa: &[&[Point2D]], r: f64) -> Point2D {
    let mut nearest = (f64::INFINITY, q);
    for pts in trasa {
        for w in pts.windows(2) {
            let c = closest_on_segment(q, w[0], w[1]);
            let d = c.distance(&q);
            if d < nearest.0 {
                nearest = (d, c);
            }
        }
        if pts.len() == 1 && pts[0].distance(&q) < nearest.0 {
            nearest = (pts[0].distance(&q), pts[0]);
        }
    }
    if nearest.0 < 0.02 {
        return q;
    }
    let closest_of = |cands: &mut dyn Iterator<Item = Point2D>, lim: f64| {
        cands
            .map(|c| (c.distance(&q), c))
            .filter(|(d, _)| *d < lim)
            .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(_, c)| c)
    };
    let mut ends = trasa
        .iter()
        .filter(|pts| !pts.is_empty() && !is_path_closed(pts))
        .flat_map(|pts| [pts[0], pts[pts.len() - 1]]);
    let mut verts = trasa.iter().flat_map(|pts| pts.iter().copied());
    closest_of(&mut ends, r)
        .or_else(|| closest_of(&mut verts, r / 2.0))
        .unwrap_or(nearest.1)
}

/// Seřadí tiskové jednotky: komponenty spojené čarami v pořadí jejich prvního
/// objektu, uvnitř komponenty nejkratší trasa (hladově z každého možného
/// začátku, vybere se nejkratší celkový přejezd).
fn order_print_units(
    units: &[PrintUnit],
    unit_of_object: &HashMap<u32, usize>,
    order_key: &impl Fn(u32) -> (usize, u32),
    fill: &[Contour],
) -> Vec<PathSegment> {
    let n = units.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for (u, unit) in units.iter().enumerate() {
        for target in &unit.links {
            if let Some(&v) = unit_of_object.get(target) {
                let (a, b) = (find(&mut parent, u), find(&mut parent, v));
                parent[a] = b;
            }
        }
    }
    let unit_key = |u: usize| -> (usize, u32) {
        let unit = &units[u];
        unit.members.iter().map(|&m| order_key(m)).min().unwrap_or_else(|| order_key(unit.id))
    };
    let mut comps: Vec<Vec<usize>> = Vec::new();
    let mut comp_of_root: HashMap<usize, usize> = HashMap::new();
    for u in 0..n {
        let r = find(&mut parent, u);
        let c = *comp_of_root.entry(r).or_insert_with(|| {
            comps.push(Vec::new());
            comps.len() - 1
        });
        comps[c].push(u);
    }
    for comp in &mut comps {
        comp.sort_by_key(|&u| unit_key(u));
    }
    comps.sort_by_key(|comp| unit_key(comp[0]));

    let mut units_mut: Vec<PrintUnit> = Vec::new();
    let units: &[PrintUnit] = if comps.iter().any(|c| c.len() > 1) {
        units_mut.extend(units.iter().map(|u| PrintUnit {
            segs: u.segs.clone(),
            members: u.members.clone(),
            id: u.id,
            reversible: u.reversible,
            links: u.links.clone(),
            wet_fill: u.wet_fill,
        }));
        for comp in comps.iter().filter(|c| c.len() > 1) {
            insert_join_vertices(&mut units_mut, comp);
        }
        &units_mut
    } else {
        units
    };

    let options = |u: usize| -> &'static [bool] { if units[u].reversible { &[false, true] } else { &[false] } };
    let mut out: Vec<PathSegment> = Vec::new();
    let mut cur = Point2D::new(0.0, 0.0);
    for comp in &comps {
        // Body, kde lze vstoupit do ostatních jednotek komponenty (otevřené dráhy)
        let entries: Vec<Vec<Point2D>> = comp
            .iter()
            .map(|&u| {
                let unit = &units[u];
                let mut v = Vec::new();
                for &rev in options(u) {
                    let seg = if rev { unit.segs.last() } else { unit.segs.first() };
                    if let Some(seg) = seg.filter(|s| !is_path_closed(&s.points)) {
                        v.push(if rev { *seg.points.last().unwrap() } else { seg.points[0] });
                    }
                }
                v
            })
            .collect();
        let targets: Vec<Vec<Point2D>> = (0..comp.len())
            .map(|i| (0..comp.len()).filter(|&j| j != i).flat_map(|j| entries[j].clone()).collect())
            .collect();

        let run = |first: Option<(usize, bool)>| -> (f64, Vec<(usize, Vec<Step>)>, Point2D) {
            let mut left: Vec<usize> = (0..comp.len()).collect();
            let (mut pos, mut cost) = (cur, 0.0);
            let mut plan = Vec::with_capacity(comp.len());
            let mut forced = first;
            while !left.is_empty() {
                let (li, rev) = forced.take().unwrap_or_else(|| {
                    let mut best = (f64::INFINITY, 0usize, false);
                    for (k, &i) in left.iter().enumerate() {
                        for &rev in options(comp[i]) {
                            let d = unit_entry(&units[comp[i]], rev, pos).distance(&pos);
                            if d < best.0 - 1e-9 {
                                best = (d, k, rev);
                            }
                        }
                    }
                    (left[best.1], best.2)
                });
                let (steps, entry, exit) = plan_unit(&units[comp[li]], rev, pos, &targets[li]);
                cost += entry.distance(&pos);
                pos = exit;
                plan.push((comp[li], steps));
                left.retain(|&x| x != li);
            }
            (cost, plan, pos)
        };

        let (_, plan, end) = if comp.len() == 1 || comp.len() > 12 {
            run(None)
        } else {
            (0..comp.len())
                .flat_map(|i| options(comp[i]).iter().map(move |&rev| (i, rev)))
                .map(|start| run(Some(start)))
                .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
                .expect("komponenta má jednotky")
        };
        for (u, steps) in &plan {
            materialize(&units[*u], steps, fill, &mut out);
        }
        cur = end;
    }
    out
}

/// Hladová optimalizace pořadí segmentů metodou nejbližšího souseda s prostorovým gridem.
/// Místo O(n²) prohledávání všech segmentů používá 2D mřížku a expandující Čebyšev-prstence —
/// v průměru O(n·√n) pro rovnoměrná data, v praxi blízko O(n).
/// Otevřené segmenty (ne tečky, ne uzavřené polygony) lze pro lepší napojení obrátit.
fn nearest_neighbor_order(segs: Vec<PathSegment>) -> Vec<PathSegment> {
    nearest_neighbor_order_from(segs, Point2D::new(0.0, 0.0))
}

/// `nearest_neighbor_order` s tryskou začínající v bodě `start`.
fn nearest_neighbor_order_from(segs: Vec<PathSegment>, start: Point2D) -> Vec<PathSegment> {
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
    for (i, seg) in segs.iter().enumerate().take(n) {
        let pts = &seg.points;
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
    let mut cur = start;
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
                    for entry in &mut tour[i + 1..=j] {
                        let (idx, rev) = *entry;
                        if seg_can_rev(&segs, idx) {
                            *entry = (idx, !rev);
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
/// Nepřipojuje tečky ani uzavřené polygony; `can_join` může spojku dál zakázat.
fn stitch_nearby_endpoints(
    segs: Vec<PathSegment>,
    threshold: f64,
    can_join: impl Fn(&PathSegment, &PathSegment) -> bool,
) -> Vec<PathSegment> {
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

        // Tolerance: řady s rozestupem přesně rovným prahu se spojí vždy,
        // ne podle zaokrouhlení
        let can_stitch = dist < threshold + 1e-6
            && !is_dot(&current.points)
            && !is_dot(&next.points)
            && !is_path_closed(&current.points)
            && !is_path_closed(&next.points)
            && can_join(&current, &next);

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
                    is_filled: Some(true),
                    ..PathSegment::new(pts)
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

// ─── Union-Find a rozdělení importu na objekty ────────────────────────────────

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

/// Rozdělí surový import na samostatné objekty. Segmenty, jejichž bounding boxy
/// se překrývají, patří do téhož objektu (union-find) — mezikruží nebo tvar
/// s dírou tak zůstane pohromadě a výplň pro něj dál funguje podle even-odd
/// pravidla.
pub fn split_into_objects(raw: &SubstratePaths) -> Vec<SubstratePaths> {
    let segs: Vec<&PathSegment> = raw.segments.iter().filter(|s| !s.is_empty()).collect();
    let n = segs.len();
    if n == 0 {
        return vec![];
    }

    let boxes: Vec<(f64, f64, f64, f64)> = segs
        .iter()
        .map(|s| bbox_of_points(s.points.iter()).unwrap_or((0.0, 0.0, 0.0, 0.0)))
        .collect();

    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        boxes[a]
            .0
            .partial_cmp(&boxes[b].0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut parent: Vec<usize> = (0..n).collect();
    let mut active: Vec<usize> = Vec::new();
    for &i in &order {
        let (ax1, _, ay1, ay2) = boxes[i];
        active.retain(|&j| boxes[j].1 >= ax1);
        for &j in &active {
            let (_, _, by1, by2) = boxes[j];
            if !(ay2 < by1 || ay1 > by2) {
                uf_union(&mut parent, i, j);
            }
        }
        active.push(i);
    }

    let mut groups: HashMap<usize, Vec<PathSegment>> = HashMap::new();
    for (i, seg) in segs.iter().enumerate() {
        let root = uf_find(&mut parent, i);
        groups.entry(root).or_default().push((*seg).clone());
    }

    let mut objects: Vec<SubstratePaths> = groups.into_values().map(SubstratePaths::new).collect();
    objects.sort_by(|a, b| {
        let c = |o: &SubstratePaths| -> (f64, f64) {
            let pts: Vec<&Point2D> = o.segments.iter().flat_map(|s| s.points.iter()).collect();
            let k = pts.len().max(1) as f64;
            (
                pts.iter().map(|p| p.x).sum::<f64>() / k,
                pts.iter().map(|p| p.y).sum::<f64>() / k,
            )
        };
        let (ax, ay) = c(a);
        let (bx, by) = c(b);
        ax.partial_cmp(&bx)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(ay.partial_cmp(&by).unwrap_or(std::cmp::Ordering::Equal))
    });
    objects
}

pub(crate) fn segs_cross(p1: Point2D, p2: Point2D, p3: Point2D, p4: Point2D) -> bool {
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

// ─── Testy ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regions::point_in_polygon;
    use crate::types::SliceParams;

    fn square_segment(size: f64) -> PathSegment {
        PathSegment::new(vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(size, 0.0),
            Point2D::new(size, size),
            Point2D::new(0.0, size),
            Point2D::new(0.0, 0.0),
        ])
    }

    fn default_params() -> SliceParams {
        SliceParams {
            slide_w: 25.0,
            slide_h: 75.0,
            margin: 2.0,
            auto_scale: false,
            infill_style: "Okraje".to_string(),
            infill_val: 1.0,
            infill_type: "mm".to_string(),
            infill_angle: 0.0,
            nozzle_diam: 0.4,
            user_scale: 1.0,
            object_overrides: HashMap::new(),
            user_scale_y: None,
            deleted_objects: Vec::new(),
            object_transforms: HashMap::new(),
            object_order: Vec::new(),
        }
    }

    fn bbox(paths: &SubstratePaths) -> (f64, f64, f64, f64) {
        bbox_of_paths(&paths.segments).expect("neprázdné dráhy")
    }

    #[test]
    fn test_is_path_closed() {
        let closed = square_segment(10.0);
        assert!(is_path_closed(&closed.points));

        let open = vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)];
        assert!(!is_path_closed(&open));
    }

    #[test]
    fn test_point_in_polygon() {
        let sq = square_segment(10.0).points;
        assert!(point_in_polygon(Point2D::new(5.0, 5.0), &sq));
        assert!(!point_in_polygon(Point2D::new(15.0, 5.0), &sq));
        assert!(!point_in_polygon(Point2D::new(-1.0, -1.0), &sq));
    }

    #[test]
    fn test_douglas_peucker_collinear() {
        // Kolineární body se zjednoduší na koncové
        let pts: Vec<Point2D> = (0..=10).map(|i| Point2D::new(i as f64, 0.0)).collect();
        let simplified = douglas_peucker(&pts, 0.01);
        assert_eq!(simplified.len(), 2);
        assert_eq!(simplified[0].x, 0.0);
        assert_eq!(simplified[1].x, 10.0);

        // Výrazný roh musí zůstat
        let corner = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(5.0, 5.0),
            Point2D::new(10.0, 0.0),
        ];
        assert_eq!(douglas_peucker(&corner, 0.01).len(), 3);
    }

    #[test]
    fn test_interpolate_along_path() {
        // Úsečka 10 mm, rozestup 2 mm → body 0,2,4,6,8,10
        let line = vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)];
        let pts = interpolate_along_path(&line, 2.0);
        assert_eq!(pts.len(), 6);
        assert!((pts[1].x - 2.0).abs() < 1e-9);
        assert!((pts.last().unwrap().x - 10.0).abs() < 1e-9);
    }

    #[test]
    fn test_process_empty_input() {
        let out = process_substrate_paths(&SubstratePaths::new(vec![]), &default_params());
        assert!(out.segments.is_empty());
    }

    #[test]
    fn test_process_centers_on_slide() {
        // Čtverec 10×10 bez auto-scale se vycentruje na sklíčku 25×75
        let raw = SubstratePaths::new(vec![square_segment(10.0)]);
        let out = process_substrate_paths(&raw, &default_params());
        assert!(!out.segments.is_empty());
        let (min_x, max_x, min_y, max_y) = bbox(&out);
        assert!(((min_x + max_x) / 2.0 - 12.5).abs() < 1e-6, "střed X");
        assert!(((min_y + max_y) / 2.0 - 37.5).abs() < 1e-6, "střed Y");
        assert!((max_x - min_x - 10.0).abs() < 1e-6, "šířka beze změny");
    }

    #[test]
    fn test_process_auto_scale_fits_margin() {
        // Čtverec 100×100 s auto-scale se vejde do 25−2·2 = 21 mm
        let raw = SubstratePaths::new(vec![square_segment(100.0)]);
        let mut p = default_params();
        p.auto_scale = true;
        let out = process_substrate_paths(&raw, &p);
        let (min_x, max_x, min_y, max_y) = bbox(&out);
        assert!(max_x - min_x <= 21.0 + 1e-6);
        assert!(min_x >= 2.0 - 1e-6 && max_x <= 23.0 + 1e-6);
        assert!(min_y >= 2.0 - 1e-6 && max_y <= 73.0 + 1e-6);
    }

    #[test]
    fn test_process_user_scale() {
        // user_scale 0.5 zmenší objekt na polovinu
        let raw = SubstratePaths::new(vec![square_segment(10.0)]);
        let mut p = default_params();
        p.user_scale = 0.5;
        let out = process_substrate_paths(&raw, &p);
        let (min_x, max_x, _, _) = bbox(&out);
        assert!((max_x - min_x - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_process_fill_adds_paths() {
        // "Okraje + Výplň" musí vygenerovat víc drah než samotné "Okraje"
        let raw = SubstratePaths::new(vec![square_segment(20.0)]);
        let mut edges = default_params();
        edges.infill_style = "Okraje".to_string();
        let out_edges = process_substrate_paths(&raw, &edges);

        let mut fill = default_params();
        fill.infill_style = "Okraje + Výplň".to_string();
        let out_fill = process_substrate_paths(&raw, &fill);

        let count_pts = |s: &SubstratePaths| -> usize {
            s.segments.iter().map(|seg| seg.points.len()).sum()
        };
        assert!(count_pts(&out_fill) > count_pts(&out_edges));
    }

    // ─── Had ─────────────────────────────────────────────────────────────────

    fn closed(pts: &[(f64, f64)]) -> Vec<Point2D> {
        let mut v: Vec<Point2D> = pts.iter().map(|&(x, y)| Point2D::new(x, y)).collect();
        v.push(v[0]);
        v
    }

    /// C-tvar otevřený doprava a samostatný ostrůvek v jeho ústí (bboxy se překrývají).
    fn c_with_island() -> Vec<Vec<Point2D>> {
        vec![
            closed(&[
                (0.0, 0.0), (10.0, 0.0), (10.0, 2.0), (2.0, 2.0),
                (2.0, 8.0), (10.0, 8.0), (10.0, 10.0), (0.0, 10.0),
            ]),
            closed(&[(4.0, 4.0), (8.0, 4.0), (8.0, 6.0), (4.0, 6.0)]),
        ]
    }

    /// Čtverec se čtvercovou dírou.
    fn ring() -> Vec<Vec<Point2D>> {
        vec![
            closed(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]),
            closed(&[(3.0, 3.0), (7.0, 3.0), (7.0, 7.0), (3.0, 7.0)]),
        ]
    }

    /// Hřeben se třemi zuby dolů (jako písmeno „m“).
    fn comb() -> Vec<Vec<Point2D>> {
        vec![closed(&[
            (0.0, 0.0), (2.0, 0.0), (2.0, 6.0), (4.0, 6.0), (4.0, 0.0), (6.0, 0.0),
            (6.0, 6.0), (8.0, 6.0), (8.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0),
        ])]
    }

    /// Had pro polygony ve světových souřadnicích s daným úhlem výplně
    /// (polygony tvoří jednu even-odd skupinu, jako u DXF).
    fn run_snake(polys: &[Vec<Point2D>], spacing: f64, angle: f64) -> Vec<PathSegment> {
        let rotated: Vec<Contour> = polys
            .iter()
            .enumerate()
            .map(|(i, p)| Contour::new(p.clone(), 0, i, false).rotated(-angle))
            .collect();
        let bb = bbox_union(&rotated);
        snake_infill(&rotated, &row_positions(bb.min_y, bb.max_y, RowSpec::Step(spacing), 0.0), angle)
    }

    fn dist_to_segment(p: Point2D, a: Point2D, b: Point2D) -> f64 {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len_sq = dx * dx + dy * dy;
        let t = if len_sq < 1e-18 {
            0.0
        } else {
            (((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq).clamp(0.0, 1.0)
        };
        (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
    }

    /// Bod leží ve výplni (even-odd) nebo na jejím obrysu.
    fn in_fill(pt: Point2D, polys: &[Vec<Point2D>]) -> bool {
        let on_outline = polys
            .iter()
            .any(|poly| poly.windows(2).any(|w| dist_to_segment(pt, w[0], w[1]) < 1e-6));
        on_outline || polys.iter().filter(|poly| point_in_polygon(pt, poly)).count() % 2 == 1
    }

    #[test]
    fn test_snake_never_prints_outside_fill() {
        for (name, polys) in [("C + ostrůvek", c_with_island()), ("prstenec", ring()), ("hřeben", comb())] {
            for angle in [0.0, 30.0, 90.0, 135.0] {
                let out = run_snake(&polys, 0.5, angle);
                assert!(!out.is_empty(), "{name} {angle}°: žádná dráha");
                for w in out.iter().flat_map(|seg| seg.points.windows(2)) {
                    for k in 1..10 {
                        let f = k as f64 / 10.0;
                        let pt = Point2D::new(
                            w[0].x + (w[1].x - w[0].x) * f,
                            w[0].y + (w[1].y - w[0].y) * f,
                        );
                        assert!(
                            in_fill(pt, &polys),
                            "{name} {angle}°: tisk mimo výplň v ({:.3}, {:.3})",
                            pt.x,
                            pt.y
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_snake_covers_every_row() {
        for (name, polys) in [("C + ostrůvek", c_with_island()), ("prstenec", ring()), ("hřeben", comb())] {
            let out = run_snake(&polys, 0.5, 0.0);
            let mut y = 0.25;
            while y < 10.0 {
                let mut xs: Vec<f64> = polys
                    .iter()
                    .flat_map(|poly| poly.windows(2))
                    .filter(|w| (w[0].y <= y && w[1].y > y) || (w[1].y <= y && w[0].y > y))
                    .map(|w| w[0].x + (y - w[0].y) * (w[1].x - w[0].x) / (w[1].y - w[0].y))
                    .collect();
                xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
                for pair in xs.chunks_exact(2) {
                    let mid = Point2D::new((pair[0] + pair[1]) / 2.0, y);
                    let printed = out
                        .iter()
                        .flat_map(|seg| seg.points.windows(2))
                        .any(|w| dist_to_segment(mid, w[0], w[1]) < 1e-6);
                    assert!(printed, "{name}: chybí úsek řádku y={y} kolem x={:.2}", mid.x);
                }
                y += 0.5;
            }
        }
    }

    #[test]
    fn test_snake_path_count() {
        // Konvexní tvar = jediná souvislá dráha
        let square = vec![closed(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)])];
        for angle in [0.0, 45.0] {
            assert_eq!(run_snake(&square, 0.5, angle).len(), 1, "čtverec {angle}°");
        }
        // Rozvětvení kolem díry / zubů dělí výplň jen na pár drah, ne po řádcích
        assert_eq!(run_snake(&ring(), 0.5, 0.0).len(), 2, "prstenec");
        assert_eq!(run_snake(&comb(), 0.5, 0.0).len(), 3, "hřeben");
        assert_eq!(run_snake(&c_with_island(), 0.5, 0.0).len(), 2, "C + ostrůvek");
    }

    /// Jedna linka pro polygony ve světových souřadnicích (jedna even-odd skupina).
    fn run_oneline(polys: &[Vec<Point2D>], spacing: f64, angle: f64) -> Vec<PathSegment> {
        let world: Vec<Contour> =
            polys.iter().enumerate().map(|(i, p)| Contour::new(p.clone(), 0, i, false)).collect();
        let rotated: Vec<Contour> = world.iter().map(|c| c.rotated(-angle)).collect();
        let bb = bbox_union(&rotated);
        oneline_paths(&world, &rotated, &row_positions(bb.min_y, bb.max_y, RowSpec::Step(spacing), 0.0), angle)
    }

    /// Hvězda s pěti cípy — konkávní tvar, kusy hada se spojují přes vnitřní rohy.
    fn star() -> Vec<Vec<Point2D>> {
        let pts: Vec<(f64, f64)> = (0..10)
            .map(|k| {
                let a = k as f64 * std::f64::consts::PI / 5.0;
                let r = if k % 2 == 0 { 5.0 } else { 2.0 };
                (5.0 + r * a.cos(), 5.0 + r * a.sin())
            })
            .collect();
        vec![closed(&pts)]
    }

    /// Písmeno „U“ — dvě ramena nad společným dnem.
    fn u_shape() -> Vec<Vec<Point2D>> {
        vec![closed(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (7.0, 10.0), (7.0, 3.0), (3.0, 3.0), (3.0, 10.0), (0.0, 10.0)])]
    }

    /// Vzdálenost úseček (0 = protínají se nebo dotýkají).
    fn seg_seg_dist(a: Point2D, b: Point2D, c: Point2D, d: Point2D) -> f64 {
        let cross = |o: Point2D, p: Point2D, q: Point2D| (p.x - o.x) * (q.y - o.y) - (p.y - o.y) * (q.x - o.x);
        let (d1, d2, d3, d4) = (cross(c, d, a), cross(c, d, b), cross(a, b, c), cross(a, b, d));
        // Kolineární úseky (stejná řada) mají součiny jen na úrovni zaokrouhlení
        let opposite = |u: f64, v: f64| (u > 1e-12 && v < -1e-12) || (u < -1e-12 && v > 1e-12);
        if opposite(d1, d2) && opposite(d3, d4) {
            return 0.0;
        }
        dist_to_segment(a, c, d).min(dist_to_segment(b, c, d)).min(dist_to_segment(c, a, b)).min(dist_to_segment(d, a, b))
    }

    #[test]
    fn test_oneline_single_path_without_crossing() {
        let shapes = [("prstenec", ring()), ("hřeben", comb()), ("hvězda", star()), ("U", u_shape())];
        for (name, polys) in shapes {
            for angle in [0.0, 30.0, 90.0, 135.0] {
                let out = run_oneline(&polys, 0.5, angle);
                assert_eq!(out.len(), 1, "{name} {angle}°: souvislý tvar = jediná dráha");
                let segs: Vec<(Point2D, Point2D)> = out[0].points.windows(2).map(|w| (w[0], w[1])).collect();
                for &(p, q) in &segs {
                    for k in 0..=20 {
                        let f = k as f64 / 20.0;
                        let pt = Point2D::new(p.x + (q.x - p.x) * f, p.y + (q.y - p.y) * f);
                        assert!(in_fill(pt, &polys), "{name} {angle}°: tisk mimo výplň v ({:.3}, {:.3})", pt.x, pt.y);
                    }
                }
                // Dráha se nikde nekříží ani nedotkne sama sebe
                for i in 0..segs.len() {
                    for j in i + 2..segs.len() {
                        let d = seg_seg_dist(segs[i].0, segs[i].1, segs[j].0, segs[j].1);
                        assert!(
                            d > 1e-6,
                            "{name} {angle}°: úseky {i} a {j} se kříží u ({:.3}, {:.3})",
                            segs[i].0.x,
                            segs[i].0.y
                        );
                    }
                }
                // Pokryje všechny řady výplně (mezery pro příčky mají šířku rozestupu)
                let rotated: Vec<Contour> =
                    polys.iter().enumerate().map(|(i, p)| Contour::new(p.clone(), 0, i, false).rotated(-angle)).collect();
                let bb = bbox_union(&rotated);
                let rows = row_positions(bb.min_y, bb.max_y, RowSpec::Step(0.5), 0.0);
                for (a, b) in scan_rows(&rotated, &rows, 0.0).into_iter().flatten() {
                    for k in 0..=10 {
                        let f = k as f64 / 10.0;
                        let pt = rotate_pt(Point2D::new(a.x + (b.x - a.x) * f, a.y), angle, 0.0, 0.0);
                        assert!(
                            segs.iter().any(|&(p, q)| dist_to_segment(pt, p, q) < 0.25 + 1e-6),
                            "{name} {angle}°: nepokryto kolem ({:.2}, {:.2})",
                            pt.x,
                            pt.y
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_oneline_many_angles_one_path() {
        // Číslice „8“: obdélník se dvěma dírami nad sebou
        let eight = vec![
            closed(&[(0.0, 0.0), (6.0, 0.0), (6.0, 11.0), (0.0, 11.0)]),
            closed(&[(2.0, 2.0), (4.0, 2.0), (4.0, 4.5), (2.0, 4.5)]),
            closed(&[(1.5, 6.5), (4.5, 6.5), (4.5, 9.0), (1.5, 9.0)]),
        ];
        let shapes = [("prstenec", ring()), ("hřeben", comb()), ("hvězda", star()), ("U", u_shape()), ("8", eight)];
        let mut fails = Vec::new();
        for (name, polys) in &shapes {
            for sp in [0.3, 0.5, 0.7] {
                for k in 0..26 {
                    let angle = k as f64 * 7.0;
                    let out = run_oneline(polys, sp, angle);
                    for path in &out {
                        let segs: Vec<(Point2D, Point2D)> = path.points.windows(2).map(|w| (w[0], w[1])).collect();
                        for i in 0..segs.len() {
                            for j in i + 2..segs.len() {
                                let d = seg_seg_dist(segs[i].0, segs[i].1, segs[j].0, segs[j].1);
                                assert!(d > 1e-6, "{name} {sp} mm {angle}°: úseky {i} a {j} se kříží");
                            }
                        }
                    }
                    if out.len() != 1 {
                        fails.push((name.to_string(), sp, angle, out.len()));
                    }
                }
            }
        }
        assert!(fails.is_empty(), "souvislý tvar musí být jedinou drahou: {fails:?}");
    }

    #[test]
    fn test_oneline_disconnected_parts_split() {
        // Ostrůvek uvnitř C nemá s C spojení výplní — tiskne se zvlášť, ne přes mezeru
        let out = run_oneline(&c_with_island(), 0.5, 0.0);
        assert_eq!(out.len(), 2);
    }

    // ─── Objekty a sjednocení prvků ──────────────────────────────────────────

    fn element(pts: &[(f64, f64)], group: u32, stroke: bool) -> PathSegment {
        PathSegment {
            is_filled: Some(true),
            has_stroke: Some(stroke),
            group: Some(group),
            ..PathSegment::new(closed(pts))
        }
    }

    fn rect_el(x0: f64, y0: f64, x1: f64, y1: f64, group: u32) -> PathSegment {
        element(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], group, true)
    }

    /// Sklíčko velikosti bboxu vstupu → bez centrovacího posunu; Y se jen překlopí.
    fn params_for(style: &str, w: f64, h: f64) -> SliceParams {
        SliceParams {
            slide_w: w,
            slide_h: h,
            infill_style: style.to_string(),
            infill_val: 0.5,
            ..default_params()
        }
    }

    fn lines_at(out: &SubstratePaths, y: f64) -> Vec<(f64, f64)> {
        out.segments
            .iter()
            .filter(|s| s.points.len() == 2 && (s.points[0].y - y).abs() < 0.26 && (s.points[1].y - y).abs() < 0.26)
            .map(|s| (s.points[0].x.min(s.points[1].x), s.points[0].x.max(s.points[1].x)))
            .collect()
    }

    #[test]
    fn test_overlapping_elements_merge() {
        // Dva překrývající se obdélníky (různé prvky) = jeden objekt bez díry v překryvu
        let raw = SubstratePaths::new(vec![
            rect_el(0.0, 0.0, 10.0, 5.0, 0),
            rect_el(5.0, 2.0, 15.0, 8.0, 1),
        ]);
        let out = process_substrate_paths(&raw, &params_for("Okraje + Výplň", 15.0, 8.0));
        // V seznamu dva objekty, tisknou se ale jako jedna oblast s nastavením prvního
        assert_eq!(out.objects.len(), 2);
        assert_eq!(out.objects[1].merged_into, Some(0));
        assert!(out.segments.iter().all(|s| s.object == Some(0)));
        // Po překlopení Y je překryv v y ∈ (3, 6): řádek jde celou šířkou sjednocení
        let row = lines_at(&out, 4.75);
        assert!(
            row.iter().any(|&(l, r)| l < 1.0 && r > 14.0),
            "řádek přes překryv musí být souvislý: {row:?}"
        );
        // Okraj nesmí vést vnitřkem překryvu (obrys B uvnitř A a naopak)
        let interior = [Point2D::new(7.5, 4.5), Point2D::new(10.0, 4.5), Point2D::new(5.0, 4.0)];
        for seg in out.segments.iter().filter(|s| s.points.len() > 2) {
            for w in seg.points.windows(2) {
                for c in interior {
                    assert!(
                        dist_to_segment(c, w[0], w[1]) > 0.2,
                        "okraj prochází vnitřkem sloučeného tvaru u ({}, {})",
                        c.x,
                        c.y
                    );
                }
            }
        }
    }

    #[test]
    fn test_hole_inside_element_is_kept() {
        // Obrys a díra v jednom prvku (písmeno „O“) — díra zůstane
        let raw = SubstratePaths::new(vec![
            rect_el(0.0, 0.0, 10.0, 10.0, 0),
            rect_el(3.0, 3.0, 7.0, 7.0, 0),
        ]);
        let out = process_substrate_paths(&raw, &params_for("Had", 10.0, 10.0));
        assert_eq!(out.objects.len(), 1);
        let hole_center = Point2D::new(5.0, 5.0);
        for w in out.segments.iter().flat_map(|s| s.points.windows(2)) {
            assert!(dist_to_segment(hole_center, w[0], w[1]) > 1.5, "dráha vede dírou");
        }
    }

    #[test]
    fn test_shape_inside_other_element_merges() {
        // Menší tvar celý uvnitř jiného prvku nevytvoří díru ani vlastní objekt
        let raw = SubstratePaths::new(vec![
            rect_el(0.0, 0.0, 10.0, 10.0, 0),
            rect_el(3.0, 3.0, 7.0, 7.0, 1),
        ]);
        let out = process_substrate_paths(&raw, &params_for("Okraje + Výplň", 10.0, 10.0));
        assert_eq!(out.objects.len(), 2);
        assert_eq!(out.objects[1].merged_into, Some(0), "vnitřní tvar patří k vnějšímu");
        let row = lines_at(&out, 5.25);
        assert!(row.iter().any(|&(l, r)| l < 1.0 && r > 9.0), "střed musí být vyplněn: {row:?}");
    }

    #[test]
    fn test_drawn_hole_is_subtracted() {
        // Tvar označený jako díra se odečte i z jiného prvku; je to samostatný objekt
        let mut hole = rect_el(3.0, 3.0, 7.0, 7.0, 1);
        hole.hole = Some(true);
        let raw = SubstratePaths::new(vec![rect_el(0.0, 0.0, 10.0, 10.0, 0), hole]);
        let out = process_substrate_paths(&raw, &params_for("Had", 10.0, 10.0));
        assert_eq!(out.objects.len(), 2);
        assert!(out.objects[1].hole && !out.objects[0].hole);
        let center = Point2D::new(5.0, 5.0);
        for w in out.segments.iter().flat_map(|s| s.points.windows(2)) {
            assert!(dist_to_segment(center, w[0], w[1]) > 1.5, "dráha vede dírou");
        }
        assert!(out.segments.iter().all(|s| s.object == Some(0)), "díra se netiskne");
    }

    #[test]
    fn test_deleted_object_and_hole() {
        let mut hole = rect_el(3.0, 3.0, 7.0, 7.0, 1);
        hole.hole = Some(true);
        let raw = SubstratePaths::new(vec![rect_el(0.0, 0.0, 10.0, 10.0, 0), hole, rect_el(12.0, 0.0, 16.0, 4.0, 2)]);
        let mut p = params_for("Výplň", 16.0, 10.0);
        // Smazaná díra: tvar se vyplní i uprostřed
        p.deleted_objects = vec![1];
        let out = process_substrate_paths(&raw, &p);
        assert_eq!(out.objects.iter().map(|o| o.id).collect::<Vec<_>>(), vec![0, 2]);
        let row = lines_at(&out, 5.25);
        assert!(row.iter().any(|&(l, r)| l < 1.0 && r > 9.0), "díra zmizela: {row:?}");
        // Smazaný objekt: netiskne se a není v seznamu
        p.deleted_objects = vec![2];
        let out = process_substrate_paths(&raw, &p);
        assert_eq!(out.objects.iter().map(|o| o.id).collect::<Vec<_>>(), vec![0, 1]);
        assert!(out.segments.iter().all(|s| s.object == Some(0)));
    }

    #[test]
    fn test_object_ids_and_overrides() {
        // Dva oddělené objekty; druhému objekt-přepis vypne výplň
        let raw = SubstratePaths::new(vec![
            rect_el(0.0, 0.0, 4.0, 4.0, 0),
            rect_el(6.0, 0.0, 10.0, 4.0, 1),
        ]);
        let mut p = params_for("Okraje + Výplň", 10.0, 4.0);
        p.object_overrides.insert(
            "1".to_string(),
            crate::types::ObjectOverride { infill_style: Some("Okraje".into()), ..Default::default() },
        );
        let out = process_substrate_paths(&raw, &p);
        let ids: Vec<u32> = out.objects.iter().map(|o| o.id).collect();
        assert_eq!(ids, vec![0, 1], "id = index zdrojového segmentu");
        let count = |id: u32| out.segments.iter().filter(|s| s.object == Some(id)).count();
        assert!(count(0) > 2, "objekt 0 má okraj i výplň");
        assert_eq!(count(1), 1, "objekt 1 má jen okraj");
        assert!(out.segments.iter().all(|s| s.object.is_some()), "každá dráha patří objektu");
    }

    #[test]
    fn test_parse_dxf_line() {
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0.0\n20\n0.0\n11\n10.0\n21\n5.0\n0\nENDSEC\n0\nEOF\n";
        let out = parse_dxf(dxf);
        assert_eq!(out.segments.len(), 1);
        let pts = &out.segments[0].points;
        assert_eq!(pts.len(), 2);
        assert!((pts[0].x - 0.0).abs() < 1e-9 && (pts[1].x - 10.0).abs() < 1e-9);
        assert!((pts[1].y - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_dxf_insunits_inches() {
        // $INSUNITS = 1 (palce) → souřadnice ×25.4
        let dxf = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n1\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0.0\n20\n0.0\n11\n1.0\n21\n0.0\n0\nENDSEC\n0\nEOF\n";
        let out = parse_dxf(dxf);
        assert_eq!(out.segments.len(), 1);
        assert!((out.segments[0].points[1].x - 25.4).abs() < 1e-9);
    }

    #[test]
    fn test_parse_dxf_garbage() {
        assert!(parse_dxf("toto není dxf").segments.is_empty());
        assert!(parse_dxf("").segments.is_empty());
    }
}
