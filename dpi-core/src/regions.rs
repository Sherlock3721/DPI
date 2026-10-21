//! Oblasti výplně a objekty.
//!
//! Výplň je sjednocení zdrojových prvků (SVG elementů); uvnitř jednoho prvku
//! platí even-odd, takže díry písmen zůstanou zachované. Objekty jsou souvislé
//! ostrůvky této výplně — tvary, které se překrývají nebo dotýkají, splynou
//! v jeden objekt s jednou trasou.

use crate::types::Point2D;
use std::cmp::Ordering;

/// Vzdálenost, pod kterou se obrysy považují za dotýkající se (mm).
const TOUCH_EPS: f64 = 1e-6;

#[derive(Debug, Clone, Copy)]
pub struct Bbox {
    pub min_x: f64,
    pub max_x: f64,
    pub min_y: f64,
    pub max_y: f64,
}

impl Bbox {
    pub fn of(pts: &[Point2D]) -> Bbox {
        let mut b = Bbox {
            min_x: f64::INFINITY,
            max_x: f64::NEG_INFINITY,
            min_y: f64::INFINITY,
            max_y: f64::NEG_INFINITY,
        };
        for p in pts {
            b.min_x = b.min_x.min(p.x);
            b.max_x = b.max_x.max(p.x);
            b.min_y = b.min_y.min(p.y);
            b.max_y = b.max_y.max(p.y);
        }
        b
    }

    pub fn union(&self, o: &Bbox) -> Bbox {
        Bbox {
            min_x: self.min_x.min(o.min_x),
            max_x: self.max_x.max(o.max_x),
            min_y: self.min_y.min(o.min_y),
            max_y: self.max_y.max(o.max_y),
        }
    }

    fn overlaps(&self, o: &Bbox, eps: f64) -> bool {
        self.min_x <= o.max_x + eps
            && o.min_x <= self.max_x + eps
            && self.min_y <= o.max_y + eps
            && o.min_y <= self.max_y + eps
    }

    fn contains(&self, p: Point2D) -> bool {
        p.x >= self.min_x && p.x <= self.max_x && p.y >= self.min_y && p.y <= self.max_y
    }
}

/// Uzavřený obrys vyplněného tvaru.
#[derive(Debug, Clone)]
pub struct Contour {
    pub pts: Vec<Point2D>,
    /// Even-odd skupina (zdrojový prvek); různé skupiny se sjednocují.
    pub group: usize,
    /// Index zdrojového segmentu — stabilní klíč objektu.
    pub src: usize,
    /// Prvek má tah (okraj se smí kreslit).
    pub stroke: bool,
    /// Obrys díry — jeho skupina se z výplně odečítá.
    pub hole: bool,
    pub bbox: Bbox,
}

impl Contour {
    pub fn new(pts: Vec<Point2D>, group: usize, src: usize, stroke: bool) -> Contour {
        let bbox = Bbox::of(&pts);
        Contour { pts, group, src, stroke, hole: false, bbox }
    }

    pub fn with_hole(mut self, hole: bool) -> Contour {
        self.hole = hole;
        self
    }

    /// Kopie otočená kolem počátku o `angle_deg`.
    pub fn rotated(&self, angle_deg: f64) -> Contour {
        let (sin_a, cos_a) = angle_deg.to_radians().sin_cos();
        let pts = self
            .pts
            .iter()
            .map(|p| Point2D::new(p.x * cos_a - p.y * sin_a, p.x * sin_a + p.y * cos_a))
            .collect();
        Contour::new(pts, self.group, self.src, self.stroke).with_hole(self.hole)
    }

    fn edge(&self, i: usize) -> (Point2D, Point2D) {
        (self.pts[i], self.pts[(i + 1) % self.pts.len()])
    }
}

pub fn point_in_polygon(pt: Point2D, poly: &[Point2D]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (pi, pj) = (poly[i], poly[j]);
        if ((pi.y > pt.y) != (pj.y > pt.y))
            && (pt.x < (pj.x - pi.x) * (pt.y - pi.y) / (pj.y - pi.y) + pi.x)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

pub fn signed_area(poly: &[Point2D]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f64>()
        / 2.0
}

/// Skupiny s lichou paritou (uvnitř) — zvlášť plné a děrové.
#[derive(Default)]
struct Parity {
    odd: Vec<(usize, bool)>,
    fill: usize,
    holes: usize,
}

impl Parity {
    /// Přepne paritu skupiny; vrací, zda je bod nyní ve výplni.
    fn toggle(&mut self, group: usize, hole: bool) -> bool {
        match self.odd.iter().position(|&(g, _)| g == group) {
            Some(i) => {
                self.odd.swap_remove(i);
                if hole { self.holes -= 1 } else { self.fill -= 1 }
            }
            None => {
                self.odd.push((group, hole));
                if hole { self.holes += 1 } else { self.fill += 1 }
            }
        }
        self.filled()
    }

    fn filled(&self) -> bool {
        self.fill > 0 && self.holes == 0
    }
}

/// Leží bod ve výplni? Uvnitř skupiny even-odd, mezi skupinami sjednocení,
/// děrové skupiny se odečítají.
pub fn in_fill(p: Point2D, contours: &[Contour]) -> bool {
    let mut parity = Parity::default();
    for c in contours {
        if c.bbox.contains(p) && point_in_polygon(p, &c.pts) {
            parity.toggle(c.group, c.hole);
        }
    }
    parity.filled()
}

// ─── Řádkové skenování ───────────────────────────────────────────────────────

/// Průsečík řádku s hranou obrysu. Hrana a parametr `t` určují pořadí
/// průsečíků podél obrysu.
#[derive(Debug, Clone, Copy)]
pub struct ScanHit {
    pub x: f64,
    pub poly: usize,
    pub edge: usize,
    pub t: f64,
}

/// Průsečíky řádku `y` se všemi obrysy (half-open pravidlo), seřazené podle x.
pub fn row_hits(contours: &[Contour], y: f64, out: &mut Vec<ScanHit>) {
    out.clear();
    for (pi, c) in contours.iter().enumerate() {
        if y < c.bbox.min_y || y > c.bbox.max_y {
            continue;
        }
        for i in 0..c.pts.len() {
            let (p1, p2) = c.edge(i);
            if (p1.y <= y && p2.y > y) || (p2.y <= y && p1.y > y) {
                let t = (y - p1.y) / (p2.y - p1.y);
                out.push(ScanHit { x: p1.x + t * (p2.x - p1.x), poly: pi, edge: i, t });
            }
        }
    }
    out.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal));
}

/// Úseky řádku uvnitř výplně. Zametání přes průsečíky: skupina při každém
/// průsečíku mění paritu a úsek trvá, dokud je uvnitř aspoň jedna plná
/// skupina a žádná děrová. Navazující úseky (dotýkající se tvary) se slijí.
pub fn row_spans(contours: &[Contour], hits: &[ScanHit], out: &mut Vec<[ScanHit; 2]>) {
    out.clear();
    let mut parity = Parity::default();
    let mut start: Option<ScanHit> = None;
    for h in hits {
        let c = &contours[h.poly];
        let filled = parity.toggle(c.group, c.hole);
        match (start, filled) {
            (None, true) => start = Some(*h),
            (Some(s), false) => {
                start = None;
                match out.last_mut() {
                    Some(last) if s.x - last[1].x <= 1e-9 => last[1] = *h,
                    _ if h.x - s.x > 1e-9 => out.push([s, *h]),
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

// ─── Objekty (ostrůvky výplně) ──────────────────────────────────────────────

/// Souvislý ostrůvek výplně — jeden objekt.
#[derive(Debug, Clone)]
pub struct Island {
    /// Stabilní id objektu: nejmenší index zdrojového segmentu vnějšího obrysu.
    pub id: u32,
    /// Indexy obrysů ostrůvku (vnější obrysy, díry a obrysy, se kterými splývají).
    pub members: Vec<usize>,
    /// Ostrůvek vznikl sloučením tvarů (překryv/dotyk nebo více prvků) —
    /// okraj je pak jen hranice sjednocení, ne celé obrysy.
    pub merged: bool,
}

fn dist_point_segment(p: Point2D, a: Point2D, b: Point2D) -> (f64, f64) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len_sq = dx * dx + dy * dy;
    let t = if len_sq < 1e-24 {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq).clamp(0.0, 1.0)
    };
    ((p.x - a.x - t * dx).hypot(p.y - a.y - t * dy), t)
}

/// Parametr `t` na úsečce a→b, kde ji úsečka c→d protíná, a parametry
/// koncových bodů c, d, které na a→b leží (dotyk, společná hrana).
fn split_params(a: Point2D, b: Point2D, c: Point2D, d: Point2D, ts: &mut Vec<f64>) {
    let (rx, ry) = (b.x - a.x, b.y - a.y);
    let (sx, sy) = (d.x - c.x, d.y - c.y);
    let denom = rx * sy - ry * sx;
    if denom.abs() > 1e-15 {
        let (qx, qy) = (c.x - a.x, c.y - a.y);
        let t = (qx * sy - qy * sx) / denom;
        let u = (qx * ry - qy * rx) / denom;
        if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
            ts.push(t);
        }
    }
    for q in [c, d] {
        let (dist, t) = dist_point_segment(q, a, b);
        if dist < TOUCH_EPS {
            ts.push(t);
        }
    }
}

fn segments_touch(a: Point2D, b: Point2D, c: Point2D, d: Point2D) -> bool {
    let mut ts = Vec::new();
    split_params(a, b, c, d, &mut ts);
    !ts.is_empty() || dist_point_segment(a, c, d).0 < TOUCH_EPS || dist_point_segment(b, c, d).0 < TOUCH_EPS
}

fn contours_touch(p: &Contour, q: &Contour) -> bool {
    for i in 0..p.pts.len() {
        let (a, b) = p.edge(i);
        let eb = Bbox::of(&[a, b]);
        if !eb.overlaps(&q.bbox, TOUCH_EPS) {
            continue;
        }
        for j in 0..q.pts.len() {
            let (c, d) = q.edge(j);
            if eb.overlaps(&Bbox::of(&[c, d]), TOUCH_EPS) && segments_touch(a, b, c, d) {
                return true;
            }
        }
    }
    false
}

/// Bod hrany posunutý o `eps` na obě strany: (vnitřní strana obrysu, vnější strana).
fn edge_probes(a: Point2D, b: Point2D, frac: f64, ccw: bool) -> Option<(Point2D, Point2D)> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx.hypot(dy);
    if len < 1e-9 {
        return None;
    }
    let eps = (len * 1e-3).min(1e-4);
    // Levá normála hrany; u obrysu proti směru hodinových ručiček míří dovnitř
    let s = if ccw { 1.0 } else { -1.0 };
    let (nx, ny) = (-dy / len * s * eps, dx / len * s * eps);
    let m = Point2D::new(a.x + dx * frac, a.y + dy * frac);
    Some((Point2D::new(m.x + nx, m.y + ny), Point2D::new(m.x - nx, m.y - ny)))
}

/// Zda je obrys (někde) vnější hranicí výplně, resp. hranicí díry.
fn classify(c: &Contour, all: &[Contour]) -> (bool, bool) {
    let area = signed_area(&c.pts);
    if area.abs() < 1e-12 {
        return (false, false);
    }
    let n = c.pts.len();
    let (mut outer, mut hole) = (false, false);
    for i in (0..n).step_by((n / 16).max(1)) {
        let (a, b) = c.edge(i);
        let Some((p_in, p_out)) = edge_probes(a, b, 0.5, area > 0.0) else { continue };
        match (in_fill(p_in, all), in_fill(p_out, all)) {
            (true, false) => outer = true,
            (false, true) => hole = true,
            _ => {}
        }
    }
    (outer, hole)
}

pub(crate) fn uf_find(parent: &mut [usize], i: usize) -> usize {
    let mut r = i;
    while parent[r] != r {
        r = parent[r];
    }
    let mut k = i;
    while parent[k] != r {
        let next = parent[k];
        parent[k] = r;
        k = next;
    }
    r
}

/// Rozloží výplň na objekty: obrysy, které se protínají nebo dotýkají, splývají;
/// díry patří k nejmenšímu vnějšímu obrysu, který je obklopuje. Obrysy, které
/// hranici výplně netvoří (tvar celý uvnitř jiného), se vynechají.
pub fn find_islands(contours: &[Contour]) -> Vec<Island> {
    let n = contours.len();
    let mut parent: Vec<usize> = (0..n).collect();
    let mut touched = vec![false; n];
    for i in 0..n {
        for j in i + 1..n {
            if contours[i].bbox.overlaps(&contours[j].bbox, TOUCH_EPS)
                && contours_touch(&contours[i], &contours[j])
            {
                let (ri, rj) = (uf_find(&mut parent, i), uf_find(&mut parent, j));
                parent[ri] = rj;
                touched[i] = true;
                touched[j] = true;
            }
        }
    }

    let kinds: Vec<(bool, bool)> = contours.iter().map(|c| classify(c, contours)).collect();
    let areas: Vec<f64> = contours.iter().map(|c| signed_area(&c.pts).abs()).collect();
    for i in 0..n {
        let (outer, hole) = kinds[i];
        if outer || !hole {
            continue;
        }
        let probe = contours[i].pts[0];
        let container = (0..n)
            .filter(|&j| {
                j != i
                    && kinds[j].0
                    && areas[j] > areas[i]
                    && contours[j].bbox.contains(probe)
                    && point_in_polygon(probe, &contours[j].pts)
            })
            .min_by(|&a, &b| areas[a].partial_cmp(&areas[b]).unwrap_or(Ordering::Equal));
        if let Some(j) = container {
            let (ri, rj) = (uf_find(&mut parent, i), uf_find(&mut parent, j));
            parent[ri] = rj;
        }
    }

    let mut by_root: Vec<(usize, Vec<usize>)> = Vec::new();
    for i in 0..n {
        let r = uf_find(&mut parent, i);
        match by_root.iter_mut().find(|(root, _)| *root == r) {
            Some((_, members)) => members.push(i),
            None => by_root.push((r, vec![i])),
        }
    }

    by_root
        .into_iter()
        .filter_map(|(_, members)| {
            let id = members
                .iter()
                .filter(|&&m| kinds[m].0)
                .map(|&m| contours[m].src)
                .min()? as u32;
            let first_group = contours[members[0]].group;
            let merged = members
                .iter()
                .any(|&m| touched[m] || contours[m].group != first_group);
            Some(Island { id, members, merged })
        })
        .collect()
}

/// Rozdělí obrys `ci` v průsečících s ostatními obrysy na úseky a každý
/// klasifikuje: Some(true) = výplň vlevo (ve směru úseku), Some(false) = výplň
/// vpravo, None = úsek není hranicí výplně (leží uvnitř překryvu nebo mimo).
fn split_contour(contours: &[Contour], ci: usize) -> Vec<(Point2D, Point2D, Option<bool>)> {
    let c = &contours[ci];
    let mut out = Vec::new();
    for i in 0..c.pts.len() {
        let (a, b) = c.edge(i);
        if (b.x - a.x).hypot(b.y - a.y) < 1e-12 {
            continue;
        }
        let eb = Bbox::of(&[a, b]);
        let mut ts = vec![0.0, 1.0];
        for (oi, o) in contours.iter().enumerate() {
            if oi == ci || !eb.overlaps(&o.bbox, TOUCH_EPS) {
                continue;
            }
            for j in 0..o.pts.len() {
                let (p, q) = o.edge(j);
                split_params(a, b, p, q, &mut ts);
            }
        }
        ts.sort_by(|x, y| x.partial_cmp(y).unwrap_or(Ordering::Equal));
        ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
        for w in ts.windows(2) {
            let p0 = Point2D::new(a.x + (b.x - a.x) * w[0], a.y + (b.y - a.y) * w[0]);
            let p1 = Point2D::new(a.x + (b.x - a.x) * w[1], a.y + (b.y - a.y) * w[1]);
            let Some((left, right)) = edge_probes(p0, p1, 0.5, true) else { continue };
            let side = match (in_fill(left, contours), in_fill(right, contours)) {
                (true, false) => Some(true),
                (false, true) => Some(false),
                _ => None,
            };
            out.push((p0, p1, side));
        }
    }
    out
}

/// Části obrysu `ci`, které leží na hranici výplně (okraj sloučeného tvaru —
/// úseky uvnitř překryvu se vynechají). Nerozdělený obrys se vrací celý.
pub fn boundary_pieces(contours: &[Contour], ci: usize) -> Vec<Vec<Point2D>> {
    let pieces = split_contour(contours, ci);
    if pieces.iter().all(|&(_, _, side)| side.is_some()) {
        return vec![contours[ci].pts.clone()];
    }

    let mut out: Vec<Vec<Point2D>> = Vec::new();
    let mut current: Vec<Point2D> = Vec::new();
    for &(p0, p1, side) in &pieces {
        if side.is_none() {
            if current.len() >= 2 {
                out.push(std::mem::take(&mut current));
            }
            current.clear();
            continue;
        }
        if current.is_empty() {
            current.push(p0);
        }
        current.push(p1);
    }
    if current.len() >= 2 {
        // Okraj procházející začátkem obrysu: navázat na první kus
        let wraps = pieces.first().is_some_and(|f| f.2.is_some()) && !out.is_empty();
        if wraps {
            let first = out.remove(0);
            current.extend_from_slice(&first[1..]);
        }
        out.push(current);
    }
    out
}

/// Hranice sjednocení obrysů jako uzavřené smyčky orientované výplní vlevo
/// (vnější obrysy proti směru hodinových ručiček, díry po směru). Smyčky se
/// nepřekrývají, takže tvoří jedinou even-odd skupinu. None, pokud se hranici
/// nepodaří uzavřít (numericky nejednoznačné dotyky).
pub fn union_loops(contours: &[Contour]) -> Option<Vec<Contour>> {
    const MATCH_EPS: f64 = 1e-6;
    let stroke = contours.iter().any(|c| c.stroke);
    let src = contours.iter().map(|c| c.src).min()?;

    let mut pieces: Vec<(Point2D, Point2D)> = Vec::new();
    for ci in 0..contours.len() {
        for (p0, p1, side) in split_contour(contours, ci) {
            match side {
                Some(true) => pieces.push((p0, p1)),
                Some(false) => pieces.push((p1, p0)),
                None => {}
            }
        }
    }
    if pieces.is_empty() {
        return None;
    }

    // Mřížka začátků úseků pro rychlé hledání navazujícího úseku
    let cell = |p: Point2D| ((p.x / 1e-3).floor() as i64, (p.y / 1e-3).floor() as i64);
    let mut grid: std::collections::HashMap<(i64, i64), Vec<usize>> =
        std::collections::HashMap::new();
    for (i, &(p0, _)) in pieces.iter().enumerate() {
        grid.entry(cell(p0)).or_default().push(i);
    }
    let mut used = vec![false; pieces.len()];
    let next_from = |p: Point2D, used: &[bool]| -> Option<usize> {
        let (cx, cy) = cell(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(list) = grid.get(&(cx + dx, cy + dy)) {
                    for &j in list {
                        let q = pieces[j].0;
                        if !used[j] && (q.x - p.x).hypot(q.y - p.y) < MATCH_EPS {
                            return Some(j);
                        }
                    }
                }
            }
        }
        None
    };

    let mut loops = Vec::new();
    for i in 0..pieces.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let start = pieces[i].0;
        let mut pts = vec![start, pieces[i].1];
        loop {
            let end = *pts.last().unwrap();
            if (end.x - start.x).hypot(end.y - start.y) < MATCH_EPS && pts.len() > 2 {
                *pts.last_mut().unwrap() = start;
                break;
            }
            let j = next_from(end, &used)?;
            used[j] = true;
            pts.push(pieces[j].1);
        }
        if pts.len() >= 4 {
            loops.push(Contour::new(pts, 0, src, stroke));
        }
    }
    Some(loops)
}

// ─── Kompenzace šířky stopy ─────────────────────────────────────────────────

/// Mřížka hran obrysů pro dotaz na vzdálenost bodu od nejbližší hrany
/// (do vzdálenosti `range`).
struct EdgeGrid {
    cell: f64,
    edges: Vec<(Point2D, Point2D)>,
    cells: std::collections::HashMap<(i64, i64), Vec<usize>>,
}

impl EdgeGrid {
    fn new(contours: &[Contour], range: f64) -> Self {
        let cell = range.max(1e-3);
        let mut grid = EdgeGrid { cell, edges: Vec::new(), cells: Default::default() };
        for c in contours {
            for i in 0..c.pts.len() {
                let (a, b) = c.edge(i);
                let id = grid.edges.len();
                grid.edges.push((a, b));
                // Vzorky po hraně s krokem buňky — dotaz prohledá okolí 5×5
                let steps = (a.distance(&b) / cell).ceil().max(1.0) as usize;
                for k in 0..=steps {
                    let t = k as f64 / steps as f64;
                    let key = grid.key(Point2D::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
                    let list = grid.cells.entry(key).or_default();
                    if list.last() != Some(&id) {
                        list.push(id);
                    }
                }
            }
        }
        grid
    }

    fn key(&self, p: Point2D) -> (i64, i64) {
        ((p.x / self.cell).floor() as i64, (p.y / self.cell).floor() as i64)
    }

    /// Vzdálenost k nejbližší hraně, nejvýš ~`range` (dál = nekonečno).
    fn dist(&self, p: Point2D) -> f64 {
        let (cx, cy) = self.key(p);
        let mut best = f64::INFINITY;
        for dx in -2..=2 {
            for dy in -2..=2 {
                for &e in self.cells.get(&(cx + dx, cy + dy)).into_iter().flatten() {
                    let (a, b) = self.edges[e];
                    best = best.min(dist_to_segment(p, a, b));
                }
            }
        }
        best
    }
}

fn dist_to_segment(p: Point2D, a: Point2D, b: Point2D) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-18 {
        return p.distance(&a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0);
    p.distance(&Point2D::new(a.x + t * dx, a.y + t * dy))
}

/// Obrysy oblasti zúžené o `r`: každá hranice (vnější i díry) se posune o `r`
/// dovnitř výplně. Tryska pak jede `r` od okraje a vytištěný okraj housenky
/// leží na okraji tvaru. Vypouklé rohy zůstanou ostré (průsečík posunutých
/// hran), vnitřní rohy se zaoblí. None, pokud zúžení tvar zdeformuje (část
/// užší než 2·r, ostrá špička) — volající pak použije nezúžený tvar.
pub fn inset_region(contours: &[Contour], r: f64) -> Option<Vec<Contour>> {
    if r <= 0.0 || r.is_nan() {
        return Some(contours.to_vec());
    }
    const ARC_STEP: f64 = std::f64::consts::PI / 12.0; // 15°
    let grid = EdgeGrid::new(contours, r);
    let mut out = Vec::with_capacity(contours.len());
    for c in contours {
        let mut pts: Vec<Point2D> = Vec::with_capacity(c.pts.len());
        for &p in &c.pts {
            if pts.last().is_none_or(|q| q.distance(&p) > 1e-9) {
                pts.push(p);
            }
        }
        while pts.len() > 1 && pts[0].distance(pts.last().unwrap()) <= 1e-9 {
            pts.pop();
        }
        let n = pts.len();
        if n < 3 {
            return None;
        }
        let dir = |i: usize| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let l = a.distance(&b);
            ((b.x - a.x) / l, (b.y - a.y) / l)
        };

        // Strana výplně: zkouška vlevo a vpravo od středu nejdelší hrany
        let len = |i: usize| pts[i].distance(&pts[(i + 1) % n]);
        let k = (0..n).max_by(|&i, &j| len(i).partial_cmp(&len(j)).unwrap_or(Ordering::Equal))?;
        let (dx, dy) = dir(k);
        let mid = Point2D::new((pts[k].x + pts[(k + 1) % n].x) / 2.0, (pts[k].y + pts[(k + 1) % n].y) / 2.0);
        let eps = (len(k) * 0.25).min(1e-3);
        let probe = |s: f64| in_fill(Point2D::new(mid.x - dy * eps * s, mid.y + dx * eps * s), contours);
        let side = match (probe(1.0), probe(-1.0)) {
            (true, false) => 1.0,
            (false, true) => -1.0,
            _ => return None,
        };
        // Normála hrany směrem do výplně
        let normal = |i: usize| {
            let (dx, dy) = dir(i);
            (-dy * side, dx * side)
        };

        let mut q: Vec<Point2D> = Vec::with_capacity(n + n / 4);
        // Rozsah bodů každého vrcholu — hrana mezi koncem vrcholu i a začátkem
        // vrcholu i+1 je posunutá hrana i
        let mut span: Vec<(usize, usize)> = Vec::with_capacity(n);
        for i in 0..n {
            let (p, prev) = (pts[i], (i + n - 1) % n);
            let (n1, n2) = (normal(prev), normal(i));
            let (d1, d2) = (dir(prev), dir(i));
            let convex = (d1.0 * d2.1 - d1.1 * d2.0) * side >= 0.0;
            let cos_t = (n1.0 * n2.0 + n1.1 * n2.1).clamp(-1.0, 1.0);
            let theta = cos_t.acos();
            let first = q.len();
            if convex || theta < ARC_STEP {
                // Průsečík posunutých hran; příliš ostrá špička se nezúží
                if 1.0 + cos_t < 5e-3 {
                    return None;
                }
                let f = r / (1.0 + cos_t);
                q.push(Point2D::new(p.x + (n1.0 + n2.0) * f, p.y + (n1.1 + n2.1) * f));
            } else {
                let a1 = n1.1.atan2(n1.0);
                let mut delta = n2.1.atan2(n2.0) - a1;
                if delta > std::f64::consts::PI {
                    delta -= 2.0 * std::f64::consts::PI;
                } else if delta < -std::f64::consts::PI {
                    delta += 2.0 * std::f64::consts::PI;
                }
                let steps = (delta.abs() / ARC_STEP).ceil().max(1.0) as usize;
                for j in 0..=steps {
                    let a = a1 + delta * j as f64 / steps as f64;
                    q.push(Point2D::new(p.x + r * a.cos(), p.y + r * a.sin()));
                }
            }
            span.push((first, q.len() - 1));
        }

        // Kontrola: posunuté hrany míří stejně jako původní (jinak se tvar
        // převrátil) a body i středy hran leží aspoň r od všech okrajů
        let min_d = 0.98 * r;
        for i in 0..n {
            let (a, b) = (q[span[i].1], q[span[(i + 1) % n].0]);
            let (dx, dy) = dir(i);
            if (b.x - a.x) * dx + (b.y - a.y) * dy <= 0.0 {
                return None;
            }
        }
        for j in 0..q.len() {
            let (a, b) = (q[j], q[(j + 1) % q.len()]);
            let m = Point2D::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
            if grid.dist(a) < min_d || grid.dist(m) < min_d {
                return None;
            }
        }
        if !in_fill(q[0], contours) {
            return None;
        }
        q.push(q[0]);
        out.push(Contour::new(q, c.group, c.src, c.stroke).with_hole(c.hole));
    }
    Some(out)
}

// ─── Nejkratší cesty uvnitř výplně ──────────────────────────────────────────

/// Nejkratší cesty mezi body (terminály) vedoucí jen výplní nebo po její
/// hranici. Lomí se jen ve vrcholech, kde výplň svírá úhel větší než 180°
/// (vnitřní rohy obrysu, vrcholy děr); viditelnost se počítá líně a ukládá.
pub struct FillGeodesics<'a> {
    contours: &'a [Contour],
    edges: Vec<(Point2D, Point2D, Bbox)>,
    terminals: Vec<Point2D>,
    bends: Vec<Point2D>,
    /// Viditelnost ohyb–ohyb a terminál–ohyb: 0 = nezjištěno, 1 = ano, 2 = ne
    bend_vis: Vec<u8>,
    term_vis: Vec<u8>,
}

impl<'a> FillGeodesics<'a> {
    pub fn new(contours: &'a [Contour], terminals: Vec<Point2D>) -> Self {
        let edges: Vec<_> = contours
            .iter()
            .flat_map(|c| (0..c.pts.len()).map(move |i| c.edge(i)))
            .filter(|(a, b)| a.distance(b) > 1e-12)
            .map(|(a, b)| (a, b, Bbox::of(&[a, b])))
            .collect();
        let mut bends = Vec::new();
        for c in contours {
            let n = c.pts.len();
            for i in 0..n {
                let (prev, v, next) = (c.pts[(i + n - 1) % n], c.pts[i], c.pts[(i + 1) % n]);
                let (l1, l2) = (prev.distance(&v), next.distance(&v));
                if l1 < 1e-9 || l2 < 1e-9 {
                    continue;
                }
                // Osa menšího úhlu mezi hranami; leží-li bod na ní mimo výplň,
                // výplň svírá ve vrcholu úhel větší než 180°
                let (bx, by) = ((prev.x - v.x) / l1 + (next.x - v.x) / l2, (prev.y - v.y) / l1 + (next.y - v.y) / l2);
                let bl = bx.hypot(by);
                if bl < 1e-6 {
                    continue;
                }
                let eps = (l1.min(l2) * 1e-2).min(1e-4);
                if !in_fill(Point2D::new(v.x + bx / bl * eps, v.y + by / bl * eps), contours) {
                    bends.push(v);
                }
            }
        }
        let (r, t) = (bends.len(), terminals.len());
        FillGeodesics { contours, edges, terminals, bends, bend_vis: vec![0; r * r], term_vis: vec![0; t * r] }
    }

    /// Leží úsečka a→b celá ve výplni nebo na její hranici?
    pub fn visible(&self, a: Point2D, b: Point2D) -> bool {
        let sb = Bbox::of(&[a, b]);
        let mut ts = vec![0.0, 1.0];
        for (c, d, eb) in &self.edges {
            if sb.overlaps(eb, TOUCH_EPS) {
                split_params(a, b, *c, *d, &mut ts);
            }
        }
        ts.sort_by(|x, y| x.partial_cmp(y).unwrap_or(Ordering::Equal));
        ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
        ts.windows(2).all(|w| {
            let t = (w[0] + w[1]) / 2.0;
            let m = Point2D::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
            in_fill(m, self.contours)
                || self.edges.iter().any(|(c, d, eb)| {
                    eb.overlaps(&Bbox::of(&[m]), TOUCH_EPS) && dist_point_segment(m, *c, *d).0 < TOUCH_EPS
                })
        })
    }

    /// Prochází úsečka a→b (mimo své konce) vrcholem obrysu? Bod, ve kterém
    /// se části výplně jen dotýkají, není průchod.
    pub fn touches_vertex(&self, a: Point2D, b: Point2D) -> bool {
        let sb = Bbox::of(&[a, b]);
        self.edges.iter().any(|(c, _, eb)| {
            sb.overlaps(eb, TOUCH_EPS) && {
                let (d, t) = dist_point_segment(*c, a, b);
                d < TOUCH_EPS && t > 1e-9 && t < 1.0 - 1e-9
            }
        })
    }

    fn bend_visible(&mut self, u: usize, w: usize) -> bool {
        let k = u.min(w) * self.bends.len() + u.max(w);
        if self.bend_vis[k] == 0 {
            self.bend_vis[k] = if self.visible(self.bends[u], self.bends[w]) { 1 } else { 2 };
        }
        self.bend_vis[k] == 1
    }

    fn term_visible(&mut self, t: usize, v: usize) -> bool {
        let k = t * self.bends.len() + v;
        if self.term_vis[k] == 0 {
            self.term_vis[k] = if self.visible(self.terminals[t], self.bends[v]) { 1 } else { 2 };
        }
        self.term_vis[k] == 1
    }

    /// Nejkratší cesta výplní z terminálu `s` do terminálu `t`: (délka, body
    /// včetně koncových). Nedosažitelný cíl dostane přímou úsečku a délku ∞.
    ///
    /// Cesta délky L leží celá v elipse |a−v| + |v−b| ≤ L, proto se A* pouští
    /// jen přes ohyby v elipse; když se cesta v mezi nenajde, mez se zdvojí.
    /// Výsledek je přesný a krátké spojky projdou jen pár vrcholů.
    pub fn path(&mut self, s: usize, t: usize) -> (f64, Vec<Point2D>) {
        let (a, b) = (self.terminals[s], self.terminals[t]);
        if self.visible(a, b) {
            return (a.distance(&b), vec![a, b]);
        }
        let r = self.bends.len();
        let mut limit = a.distance(&b) * 1.5 + 1.0;
        loop {
            let cand: Vec<usize> =
                (0..r).filter(|&v| a.distance(&self.bends[v]) + self.bends[v].distance(&b) <= limit).collect();
            let c = cand.len();
            let mut g = vec![f64::INFINITY; c];
            let mut prev = vec![usize::MAX; c];
            let mut done = vec![false; c];
            for (k, &v) in cand.iter().enumerate() {
                if self.term_visible(s, v) {
                    g[k] = a.distance(&self.bends[v]);
                }
            }
            let h = |s: &Self, k: usize| s.bends[cand[k]].distance(&b);
            let mut found: Option<(f64, usize)> = None;
            while let Some(k) = (0..c)
                .filter(|&k| !done[k] && g[k].is_finite())
                .min_by(|&x, &y| (g[x] + h(self, x)).partial_cmp(&(g[y] + h(self, y))).unwrap_or(Ordering::Equal))
            {
                // A*: první vybraný uzel s odhadem nad nejlepší nalezenou délkou končí
                if found.is_some_and(|(len, _)| g[k] + h(self, k) >= len) || g[k] + h(self, k) > limit {
                    break;
                }
                done[k] = true;
                let (v, pv) = (cand[k], self.bends[cand[k]]);
                let d = g[k] + pv.distance(&b);
                if found.is_none_or(|(len, _)| d < len) && self.term_visible(t, v) {
                    found = Some((d, k));
                }
                for w in 0..c {
                    if done[w] {
                        continue;
                    }
                    let d = g[k] + pv.distance(&self.bends[cand[w]]);
                    if d < g[w] && self.bend_visible(v, cand[w]) {
                        g[w] = d;
                        prev[w] = k;
                    }
                }
            }
            match found {
                Some((len, last)) if len <= limit => {
                    let mut pts = vec![b];
                    let mut k = last;
                    while k != usize::MAX {
                        pts.push(self.bends[cand[k]]);
                        k = prev[k];
                    }
                    pts.push(a);
                    pts.reverse();
                    return (len, pts);
                }
                _ if c == r => return (f64::INFINITY, vec![a, b]),
                _ => limit *= 2.0,
            }
        }
    }
}
