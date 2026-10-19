use crate::types::{PathSegment, Point2D, SubstratePaths};
use roxmltree::{Document, Node};
use std::collections::HashMap;
use std::rc::Rc;

// ─── Veřejné API ─────────────────────────────────────────────────────────────

pub fn parse_svg(svg_text: &str, fineness: f64) -> SubstratePaths {
    let doc = match Document::parse(svg_text) {
        Ok(d) => d,
        Err(_) => return SubstratePaths::new(Vec::new()),
    };

    let root = doc.root_element();
    let root_mat = root_matrix(&root);

    // Index id → element. Cíl <use> může ležet kdekoli — i v <defs>, i až za
    // odkazem (Inkscape klony odkazují dopředu i dozadu).
    let mut by_id: HashMap<&str, Node> = HashMap::new();
    for node in doc.descendants() {
        if !node.is_element() {
            continue;
        }
        if let Some(id) = node.attribute("id") {
            by_id.entry(id).or_insert(node);
        }
    }

    let mut collector = Collector {
        fineness,
        by_id,
        cache: HashMap::new(),
        stack: Vec::new(),
        cycles: 0,
    };

    // Sběr geometrie v uživatelských jednotkách dokumentu…
    let local = collector.collect(root, 0, false);

    // …a jeden závěrečný převod do mm (+ dořešení zděděného stylu).
    let segments = local
        .into_iter()
        .filter(|seg| seg.points.len() >= 2)
        .map(|seg| PathSegment {
            points: seg.points.into_iter().map(|p| root_mat.apply(p)).collect(),
            is_filled: Some(seg.fill.unwrap_or(true)),     // výchozí výplň SVG je černá
            has_stroke: Some(seg.stroke.unwrap_or(false)), // výchozí tah SVG je none
        })
        .collect();

    SubstratePaths::new(segments)
}

// ─── Průchod dokumentem ──────────────────────────────────────────────────────

/// Pojistka proti patologicky zanořeným klonům.
const MAX_DEPTH: usize = 64;

/// Segment v souřadnicích předka. `None` u `fill`/`stroke` znamená „dědit výš“ —
/// dořeší se cestou nahoru, takže klon se stylem řídí podle místa vložení
/// (`<use>`), ne podle definice. `Some` je naopak konečné (např. otevřená cesta
/// se nikdy nevyplňuje).
struct LocalSeg {
    points: Vec<Point2D>,
    fill: Option<bool>,
    stroke: Option<bool>,
}

/// Segment, který nemůže mít výplň (úsečka, polyline, otevřený podúsek cesty).
fn open_seg(points: Vec<Point2D>) -> LocalSeg {
    LocalSeg {
        points,
        fill: Some(false),
        stroke: None,
    }
}

/// Uzavřený obrys — výplň i tah se berou ze stylu (vlastního nebo zděděného).
fn area_seg(points: Vec<Point2D>) -> LocalSeg {
    LocalSeg {
        points,
        fill: None,
        stroke: None,
    }
}

fn apply_mat(segs: &mut [LocalSeg], m: &Mat) {
    for seg in segs {
        for pt in &mut seg.points {
            *pt = m.apply(*pt);
        }
    }
}

struct Collector<'a, 'i> {
    fineness: f64,
    by_id: HashMap<&'a str, Node<'a, 'i>>,
    /// Hotová geometrie cílů `<use>`, klíčovaná podle id. Tessellace (bézier,
    /// oblouky, kružnice) proběhne pro každý cíl právě jednou — každý další klon
    /// je pak už jen kopie bodů + afinní transformace. To je ta optimalizace,
    /// kterou klonování dovoluje: N klonů stojí N×O(bodů), ne N×O(parsování).
    cache: HashMap<&'a str, Rc<Vec<LocalSeg>>>,
    /// Id právě rozbalovaných cílů — detekce cyklu `<use>` → … → `<use>`.
    stack: Vec<&'a str>,
    /// Počet přerušených cyklů — geometrie zkrácená cyklem se nesmí cachovat,
    /// na jiném místě dokumentu by se rozbalila jinak.
    cycles: usize,
}

impl<'a, 'i> Collector<'a, 'i> {
    /// Geometrie podstromu v souřadnicích *předka* uzlu — tedy včetně vlastního
    /// `transform` a s doplněným vlastním `fill`/`stroke`.
    ///
    /// `referenced` = uzel je cílem `<use>`; jen tak se vykreslí i obsah
    /// kontejnerů, které se samy nevykreslují (`<defs>`, `<symbol>`, …).
    fn collect(&mut self, node: Node<'a, 'i>, depth: usize, referenced: bool) -> Vec<LocalSeg> {
        if depth > MAX_DEPTH || is_display_none(&node) {
            return Vec::new();
        }

        let mut segs = match node.tag_name().name() {
            "g" | "a" | "svg" | "switch" => self.collect_children(node, depth),
            // Nevykreslované kontejnery — obsah se objeví jen přes <use>
            "defs" | "symbol" | "marker" | "clipPath" | "mask" | "pattern" if referenced => {
                self.collect_children(node, depth)
            }
            "use" => self.expand_use(node, depth),
            "line" | "rect" | "circle" | "ellipse" | "polyline" | "polygon" | "path" => {
                self.shape(node)
            }
            _ => Vec::new(),
        };

        if segs.is_empty() {
            return segs;
        }

        if let Some(t) = node.attribute("transform") {
            let m = parse_transform(t);
            if !m.is_identity() {
                apply_mat(&mut segs, &m);
            }
        }

        // Dědičnost stylu zdola nahoru: vlastní hodnota doplní jen to, co
        // podstrom nechal nevyplněné (prezentační atribut potomka tak vyhrává
        // nad stylem předka — stejně jako v CSS kaskádě).
        let fill = paint_flag(&node, "fill");
        let stroke = paint_flag(&node, "stroke");
        if fill.is_some() || stroke.is_some() {
            for seg in &mut segs {
                if seg.fill.is_none() {
                    seg.fill = fill;
                }
                if seg.stroke.is_none() {
                    seg.stroke = stroke;
                }
            }
        }

        segs
    }

    fn collect_children(&mut self, node: Node<'a, 'i>, depth: usize) -> Vec<LocalSeg> {
        let mut out: Vec<LocalSeg> = Vec::new();
        for child in node.children() {
            if child.is_element() {
                out.extend(self.collect(child, depth + 1, false));
            }
        }
        out
    }

    /// Klon `<use>`: `transform` × `translate(x,y)` × geometrie cíle.
    fn expand_use(&mut self, node: Node<'a, 'i>, depth: usize) -> Vec<LocalSeg> {
        let href = node
            .attribute(("http://www.w3.org/1999/xlink", "href"))
            .or_else(|| node.attribute("href"))
            .unwrap_or("")
            .trim();
        let id = match href.strip_prefix('#') {
            Some(id) if !id.is_empty() => id,
            _ => return Vec::new(),
        };
        if self.stack.contains(&id) {
            self.cycles += 1;
            return Vec::new(); // cyklický odkaz — klon sebe sama
        }
        let target = match self.by_id.get(id) {
            Some(n) => *n,
            None => return Vec::new(), // visící odkaz
        };

        let geom = self.geometry_of(id, target, depth);
        let mut segs: Vec<LocalSeg> = geom
            .iter()
            .map(|s| LocalSeg {
                points: s.points.clone(),
                fill: s.fill,
                stroke: s.stroke,
            })
            .collect();

        let x = attr_f64(&node, "x");
        let y = attr_f64(&node, "y");
        if x != 0.0 || y != 0.0 {
            apply_mat(&mut segs, &Mat::translate(x, y));
        }
        segs
    }

    /// Geometrie cíle `<use>` — z cache, nebo se jednou natessellluje a uloží.
    fn geometry_of(
        &mut self,
        id: &'a str,
        target: Node<'a, 'i>,
        depth: usize,
    ) -> Rc<Vec<LocalSeg>> {
        if let Some(cached) = self.cache.get(id) {
            return Rc::clone(cached);
        }
        let cycles_before = self.cycles;
        self.stack.push(id);
        let segs = self.collect(target, depth + 1, true);
        self.stack.pop();
        let rc = Rc::new(segs);
        if self.cycles == cycles_before {
            self.cache.insert(id, Rc::clone(&rc));
        }
        rc
    }

    /// Tessellace jednoho tvaru do polyline v jeho vlastních souřadnicích.
    fn shape(&self, node: Node<'a, 'i>) -> Vec<LocalSeg> {
        match node.tag_name().name() {
            "line" => {
                let x1 = attr_f64(&node, "x1");
                let y1 = attr_f64(&node, "y1");
                let x2 = attr_f64(&node, "x2");
                let y2 = attr_f64(&node, "y2");
                vec![open_seg(vec![Point2D::new(x1, y1), Point2D::new(x2, y2)])]
            }
            "rect" => {
                let x = attr_f64(&node, "x");
                let y = attr_f64(&node, "y");
                let w = attr_f64(&node, "width");
                let h = attr_f64(&node, "height");
                if w <= 0.0 || h <= 0.0 {
                    return Vec::new();
                }
                let rx_raw = attr_f64(&node, "rx");
                let ry_raw = attr_f64(&node, "ry");
                let mut rx = if rx_raw > 0.0 { rx_raw } else { ry_raw };
                let mut ry = if ry_raw > 0.0 { ry_raw } else { rx_raw };
                rx = rx.min(w / 2.0);
                ry = ry.min(h / 2.0);

                if rx <= 0.0 || ry <= 0.0 {
                    return vec![area_seg(vec![
                        Point2D::new(x, y),
                        Point2D::new(x + w, y),
                        Point2D::new(x + w, y + h),
                        Point2D::new(x, y + h),
                        Point2D::new(x, y),
                    ])];
                }

                let steps = (8.0 * self.fineness).round().max(3.0) as usize;
                let mut pts: Vec<Point2D> = Vec::new();
                add_arc_pts(
                    &mut pts,
                    x + w - rx,
                    y + ry,
                    rx,
                    ry,
                    -std::f64::consts::FRAC_PI_2,
                    steps,
                );
                add_arc_pts(&mut pts, x + w - rx, y + h - ry, rx, ry, 0.0, steps);
                add_arc_pts(
                    &mut pts,
                    x + rx,
                    y + h - ry,
                    rx,
                    ry,
                    std::f64::consts::FRAC_PI_2,
                    steps,
                );
                add_arc_pts(
                    &mut pts,
                    x + rx,
                    y + ry,
                    rx,
                    ry,
                    std::f64::consts::PI,
                    steps,
                );
                if let Some(first) = pts.first().cloned() {
                    pts.push(first);
                }
                vec![area_seg(pts)]
            }
            "circle" => {
                let cx = attr_f64(&node, "cx");
                let cy = attr_f64(&node, "cy");
                let r = attr_f64(&node, "r");
                if r <= 0.0 {
                    return Vec::new();
                }
                let steps = (32.0 * self.fineness).round().max(4.0) as usize;
                vec![area_seg(
                    (0..=steps)
                        .map(|j| {
                            let theta = (j as f64 / steps as f64) * std::f64::consts::TAU;
                            Point2D::new(cx + r * theta.cos(), cy + r * theta.sin())
                        })
                        .collect(),
                )]
            }
            "ellipse" => {
                let cx = attr_f64(&node, "cx");
                let cy = attr_f64(&node, "cy");
                let rx = attr_f64(&node, "rx");
                let ry = attr_f64(&node, "ry");
                if rx <= 0.0 || ry <= 0.0 {
                    return Vec::new();
                }
                let steps = (32.0 * self.fineness).round().max(4.0) as usize;
                vec![area_seg(
                    (0..=steps)
                        .map(|j| {
                            let theta = (j as f64 / steps as f64) * std::f64::consts::TAU;
                            Point2D::new(cx + rx * theta.cos(), cy + ry * theta.sin())
                        })
                        .collect(),
                )]
            }
            "polyline" | "polygon" => {
                let is_polygon = node.tag_name().name() == "polygon";
                let points_attr = node.attribute("points").unwrap_or("");
                let mut nums = points_attr
                    .split(|c: char| c.is_whitespace() || c == ',')
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| s.parse::<f64>().ok());
                let mut pts: Vec<Point2D> = Vec::new();
                while let (Some(px), Some(py)) = (nums.next(), nums.next()) {
                    pts.push(Point2D::new(px, py));
                }
                if pts.len() < 2 {
                    return Vec::new();
                }
                if is_polygon {
                    if let Some(first) = pts.first().cloned() {
                        pts.push(first);
                    }
                    vec![area_seg(pts)]
                } else {
                    vec![open_seg(pts)]
                }
            }
            "path" => {
                let d = node.attribute("d").unwrap_or("");
                parse_path_d(d, self.fineness)
                    .into_iter()
                    .map(|seg| {
                        if is_path_closed(&seg.points) {
                            area_seg(seg.points)
                        } else {
                            open_seg(seg.points)
                        }
                    })
                    .collect()
            }
            _ => Vec::new(),
        }
    }
}

// ─── Afinní transformace ─────────────────────────────────────────────────────

/// Matice SVG `[a c e; b d f; 0 0 1]`.
#[derive(Debug, Clone, Copy)]
struct Mat {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Mat {
    const IDENTITY: Mat = Mat {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    fn translate(tx: f64, ty: f64) -> Mat {
        Mat {
            e: tx,
            f: ty,
            ..Mat::IDENTITY
        }
    }

    fn scale(sx: f64, sy: f64) -> Mat {
        Mat {
            a: sx,
            d: sy,
            ..Mat::IDENTITY
        }
    }

    fn rotate(deg: f64) -> Mat {
        let (sin, cos) = deg.to_radians().sin_cos();
        Mat {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: 0.0,
            f: 0.0,
        }
    }

    fn skew_x(deg: f64) -> Mat {
        Mat {
            c: deg.to_radians().tan(),
            ..Mat::IDENTITY
        }
    }

    fn skew_y(deg: f64) -> Mat {
        Mat {
            b: deg.to_radians().tan(),
            ..Mat::IDENTITY
        }
    }

    /// `self ∘ other` — nejdřív se aplikuje `other`, pak `self`.
    fn mul(&self, o: &Mat) -> Mat {
        Mat {
            a: self.a * o.a + self.c * o.b,
            b: self.b * o.a + self.d * o.b,
            c: self.a * o.c + self.c * o.d,
            d: self.b * o.c + self.d * o.d,
            e: self.a * o.e + self.c * o.f + self.e,
            f: self.b * o.e + self.d * o.f + self.f,
        }
    }

    fn apply(&self, p: Point2D) -> Point2D {
        Point2D::new(
            self.a * p.x + self.c * p.y + self.e,
            self.b * p.x + self.d * p.y + self.f,
        )
    }

    fn is_identity(&self) -> bool {
        (self.a - 1.0).abs() < 1e-12
            && self.b.abs() < 1e-12
            && self.c.abs() < 1e-12
            && (self.d - 1.0).abs() < 1e-12
            && self.e.abs() < 1e-12
            && self.f.abs() < 1e-12
    }
}

/// Parsuje atribut `transform`. Funkce se skládají zleva doprava —
/// nejlevější je nejvíc vnější.
fn parse_transform(s: &str) -> Mat {
    let mut m = Mat::IDENTITY;
    let mut rest = s;

    while let Some(open) = rest.find('(') {
        let name: String = rest[..open]
            .chars()
            .filter(|c| c.is_ascii_alphabetic())
            .collect::<String>()
            .to_lowercase();
        let close = match rest[open + 1..].find(')') {
            Some(i) => open + 1 + i,
            None => break,
        };
        let args: Vec<f64> = rest[open + 1..close]
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|t| !t.is_empty())
            .filter_map(|t| t.parse::<f64>().ok())
            .collect();
        rest = &rest[close + 1..];

        let step = match (name.as_str(), args.len()) {
            ("matrix", 6) => Some(Mat {
                a: args[0],
                b: args[1],
                c: args[2],
                d: args[3],
                e: args[4],
                f: args[5],
            }),
            ("translate", 1) => Some(Mat::translate(args[0], 0.0)),
            ("translate", _) => Some(Mat::translate(args[0], args[1])),
            ("scale", 1) => Some(Mat::scale(args[0], args[0])),
            ("scale", _) => Some(Mat::scale(args[0], args[1])),
            ("rotate", 1) => Some(Mat::rotate(args[0])),
            ("rotate", n) if n >= 3 => {
                let (cx, cy) = (args[1], args[2]);
                Some(
                    Mat::translate(cx, cy)
                        .mul(&Mat::rotate(args[0]))
                        .mul(&Mat::translate(-cx, -cy)),
                )
            }
            ("skewx", 1) => Some(Mat::skew_x(args[0])),
            ("skewy", 1) => Some(Mat::skew_y(args[0])),
            _ => None,
        };
        if let Some(step) = step {
            m = m.mul(&step);
        }
    }

    m
}

/// Matice dokumentu: uživatelské jednotky (s ohledem na `viewBox`) → mm.
fn root_matrix(root: &Node) -> Mat {
    let mut svg_to_mm = 1.0_f64;
    let mut vb_x = 0.0_f64;
    let mut vb_y = 0.0_f64;

    if root.tag_name().name() == "svg" {
        let width_attr = root.attribute("width").unwrap_or("");
        let height_attr = root.attribute("height").unwrap_or("");
        let viewbox = root.attribute("viewBox").unwrap_or("");

        let width_mm = parse_unit_to_mm(width_attr);
        let height_mm = parse_unit_to_mm(height_attr);

        if !viewbox.is_empty() {
            let parts: Vec<f64> = viewbox
                .trim()
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|s| !s.is_empty())
                .filter_map(|s| s.parse().ok())
                .collect();
            if parts.len() == 4 {
                let vb_w = parts[2];
                let vb_h = parts[3];
                vb_x = parts[0];
                vb_y = parts[1];
                if vb_w > 0.0 {
                    if let Some(wmm) = width_mm {
                        svg_to_mm = wmm / vb_w;
                    } else if let Some(hmm) = height_mm {
                        if vb_h > 0.0 {
                            svg_to_mm = hmm / vb_h;
                        }
                    } else {
                        svg_to_mm = 25.4 / 96.0;
                    }
                }
            }
        } else if !width_attr.is_empty() {
            if let Some(wmm) = width_mm {
                let w_val = parse_numeric_part(width_attr);
                if w_val > 0.0 {
                    svg_to_mm = wmm / w_val;
                }
            }
        }
    }

    Mat::scale(svg_to_mm, svg_to_mm).mul(&Mat::translate(-vb_x, -vb_y))
}

// ─── Pomocné funkce ───────────────────────────────────────────────────────────

fn attr_f64(node: &Node, name: &str) -> f64 {
    node.attribute(name)
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0.0)
}

fn parse_numeric_part(s: &str) -> f64 {
    let num_str: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    num_str.parse().unwrap_or(0.0)
}

fn parse_unit_to_mm(val_str: &str) -> Option<f64> {
    let val: f64 = val_str
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect::<String>()
        .parse()
        .ok()?;
    if val.is_nan() {
        return None;
    }
    let unit: String = val_str
        .chars()
        .skip_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || c.is_whitespace())
        .collect::<String>()
        .trim()
        .to_lowercase();
    let mm = match unit.as_str() {
        "mm" => val,
        "cm" => val * 10.0,
        "in" => val * 25.4,
        "pt" => val * (25.4 / 72.0),
        "pc" => val * (25.4 / 6.0),
        _ => val * (25.4 / 96.0), // px nebo bez jednotky
    };
    Some(mm)
}

/// Hodnota CSS vlastnosti z atributu `style` — přesná shoda názvu,
/// takže `fill-opacity` se neplete s `fill`.
fn style_prop<'s>(style: &'s str, name: &str) -> Option<&'s str> {
    for part in style.split(';') {
        if let Some((key, val)) = part.split_once(':') {
            if key.trim().eq_ignore_ascii_case(name) {
                let val = val.trim();
                if !val.is_empty() {
                    return Some(val);
                }
            }
        }
    }
    None
}

/// Zda element kreslí danou barvu (`fill`/`stroke`):
/// `Some(true)` = kreslí, `Some(false)` = `none`, `None` = neurčeno → dědí se.
fn paint_flag(node: &Node, name: &str) -> Option<bool> {
    let raw = node
        .attribute("style")
        .and_then(|style| style_prop(style, name))
        .or_else(|| node.attribute(name))?;
    let val = raw.trim();
    if val.is_empty() || val.eq_ignore_ascii_case("inherit") {
        return None;
    }
    Some(!val.eq_ignore_ascii_case("none"))
}

fn is_display_none(node: &Node) -> bool {
    let raw = node
        .attribute("style")
        .and_then(|style| style_prop(style, "display"))
        .or_else(|| node.attribute("display"));
    matches!(raw, Some(v) if v.trim().eq_ignore_ascii_case("none"))
}

fn is_path_closed(pts: &[Point2D]) -> bool {
    if pts.len() < 3 {
        return false;
    }
    let p1 = &pts[0];
    let p2 = &pts[pts.len() - 1];
    (p1.x - p2.x).powi(2) + (p1.y - p2.y).powi(2) < 1e-2
}

fn add_arc_pts(
    pts: &mut Vec<Point2D>,
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    start_angle: f64,
    steps: usize,
) {
    for s in 0..=steps {
        let a = start_angle + (s as f64 / steps as f64) * std::f64::consts::FRAC_PI_2;
        pts.push(Point2D::new(cx + rx * a.cos(), cy + ry * a.sin()));
    }
}

// ─── Tokenizér path d ────────────────────────────────────────────────────────

#[derive(Debug)]
enum PathToken {
    Cmd(char),
    Num(f64),
}

fn tokenize_path(d: &str) -> Vec<PathToken> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = d.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if "MLHVCSQTAZmlhvcsqtaz".contains(c) {
            tokens.push(PathToken::Cmd(c));
            i += 1;
        } else if c.is_ascii_digit() || c == '-' || c == '.' {
            let start = i;
            if c == '-' {
                i += 1;
            }
            let mut has_dot = false;
            while i < chars.len() {
                let ch = chars[i];
                if ch.is_ascii_digit() {
                    i += 1;
                } else if ch == '.' && !has_dot {
                    has_dot = true;
                    i += 1;
                } else {
                    break;
                }
            }
            // exponent
            if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                i += 1;
                if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
                    i += 1;
                }
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let s: String = chars[start..i].iter().collect();
            if let Ok(v) = s.parse::<f64>() {
                tokens.push(PathToken::Num(v));
            }
        } else {
            i += 1;
        }
    }
    tokens
}

// ─── Parser path d ───────────────────────────────────────────────────────────

fn parse_path_d(d: &str, fineness: f64) -> Vec<PathSegment> {
    let bezier_steps = (20.0 * fineness).round().max(3.0) as usize;
    let arc_steps = (30.0 * fineness).round().max(4.0) as usize;

    let tokens = tokenize_path(d);
    let mut segments: Vec<PathSegment> = Vec::new();
    let mut current_pts: Vec<Point2D> = Vec::new();

    let mut cx = 0.0_f64;
    let mut cy = 0.0_f64;
    let mut start_x = 0.0_f64;
    let mut start_y = 0.0_f64;
    let mut last_ctrl_x = 0.0_f64;
    let mut last_ctrl_y = 0.0_f64;
    let mut last_cmd = ' ';

    let mut ti = 0usize;

    let nums = |ti: &mut usize, count: usize| -> Vec<f64> {
        let mut out = Vec::with_capacity(count);
        while out.len() < count && *ti < tokens.len() {
            match &tokens[*ti] {
                PathToken::Num(v) => {
                    out.push(*v);
                    *ti += 1;
                }
                PathToken::Cmd(_) => break,
            }
        }
        out
    };

    while ti < tokens.len() {
        let cmd = match &tokens[ti] {
            PathToken::Cmd(c) => {
                let c = *c;
                ti += 1;
                c
            }
            PathToken::Num(_) => {
                ti += 1;
                continue;
            }
        };

        match cmd {
            'M' | 'm' => {
                if current_pts.len() >= 2 {
                    segments.push(PathSegment::new(std::mem::take(&mut current_pts)));
                } else {
                    current_pts.clear();
                }
                let a = nums(&mut ti, 2);
                if a.len() == 2 {
                    if cmd == 'm' && !segments.is_empty() {
                        cx += a[0];
                        cy += a[1];
                    } else {
                        cx = a[0];
                        cy = a[1];
                    }
                    start_x = cx;
                    start_y = cy;
                    current_pts.push(Point2D::new(cx, cy));
                    loop {
                        let a = nums(&mut ti, 2);
                        if a.len() < 2 {
                            break;
                        }
                        if cmd == 'm' {
                            cx += a[0];
                            cy += a[1];
                        } else {
                            cx = a[0];
                            cy = a[1];
                        }
                        current_pts.push(Point2D::new(cx, cy));
                    }
                }
            }
            'L' | 'l' => loop {
                let a = nums(&mut ti, 2);
                if a.len() < 2 {
                    break;
                }
                if cmd == 'l' {
                    cx += a[0];
                    cy += a[1];
                } else {
                    cx = a[0];
                    cy = a[1];
                }
                current_pts.push(Point2D::new(cx, cy));
            },
            'H' | 'h' => loop {
                let a = nums(&mut ti, 1);
                if a.is_empty() {
                    break;
                }
                if cmd == 'h' {
                    cx += a[0];
                } else {
                    cx = a[0];
                }
                current_pts.push(Point2D::new(cx, cy));
            },
            'V' | 'v' => loop {
                let a = nums(&mut ti, 1);
                if a.is_empty() {
                    break;
                }
                if cmd == 'v' {
                    cy += a[0];
                } else {
                    cy = a[0];
                }
                current_pts.push(Point2D::new(cx, cy));
            },
            'C' | 'c' => loop {
                let a = nums(&mut ti, 6);
                if a.len() < 6 {
                    break;
                }
                let (mut x1, mut y1, mut x2, mut y2, mut dx, mut dy) =
                    (a[0], a[1], a[2], a[3], a[4], a[5]);
                if cmd == 'c' {
                    x1 += cx;
                    y1 += cy;
                    x2 += cx;
                    y2 += cy;
                    dx += cx;
                    dy += cy;
                }
                for s in 1..=bezier_steps {
                    let t = s as f64 / bezier_steps as f64;
                    let mt = 1.0 - t;
                    current_pts.push(Point2D::new(
                        mt.powi(3) * cx
                            + 3.0 * mt.powi(2) * t * x1
                            + 3.0 * mt * t.powi(2) * x2
                            + t.powi(3) * dx,
                        mt.powi(3) * cy
                            + 3.0 * mt.powi(2) * t * y1
                            + 3.0 * mt * t.powi(2) * y2
                            + t.powi(3) * dy,
                    ));
                }
                cx = dx;
                cy = dy;
                last_ctrl_x = x2;
                last_ctrl_y = y2;
            },
            'S' | 's' => loop {
                let a = nums(&mut ti, 4);
                if a.len() < 4 {
                    break;
                }
                let (mut x2, mut y2, mut dx, mut dy) = (a[0], a[1], a[2], a[3]);
                if cmd == 's' {
                    x2 += cx;
                    y2 += cy;
                    dx += cx;
                    dy += cy;
                }
                let (x1, y1) = if "CcSs".contains(last_cmd) {
                    (2.0 * cx - last_ctrl_x, 2.0 * cy - last_ctrl_y)
                } else {
                    (cx, cy)
                };
                for s in 1..=bezier_steps {
                    let t = s as f64 / bezier_steps as f64;
                    let mt = 1.0 - t;
                    current_pts.push(Point2D::new(
                        mt.powi(3) * cx
                            + 3.0 * mt.powi(2) * t * x1
                            + 3.0 * mt * t.powi(2) * x2
                            + t.powi(3) * dx,
                        mt.powi(3) * cy
                            + 3.0 * mt.powi(2) * t * y1
                            + 3.0 * mt * t.powi(2) * y2
                            + t.powi(3) * dy,
                    ));
                }
                cx = dx;
                cy = dy;
                last_ctrl_x = x2;
                last_ctrl_y = y2;
            },
            'Q' | 'q' => loop {
                let a = nums(&mut ti, 4);
                if a.len() < 4 {
                    break;
                }
                let (mut x1, mut y1, mut dx, mut dy) = (a[0], a[1], a[2], a[3]);
                if cmd == 'q' {
                    x1 += cx;
                    y1 += cy;
                    dx += cx;
                    dy += cy;
                }
                for s in 1..=bezier_steps {
                    let t = s as f64 / bezier_steps as f64;
                    let mt = 1.0 - t;
                    current_pts.push(Point2D::new(
                        mt.powi(2) * cx + 2.0 * mt * t * x1 + t.powi(2) * dx,
                        mt.powi(2) * cy + 2.0 * mt * t * y1 + t.powi(2) * dy,
                    ));
                }
                cx = dx;
                cy = dy;
                last_ctrl_x = x1;
                last_ctrl_y = y1;
            },
            'T' | 't' => loop {
                let a = nums(&mut ti, 2);
                if a.len() < 2 {
                    break;
                }
                let (mut dx, mut dy) = (a[0], a[1]);
                if cmd == 't' {
                    dx += cx;
                    dy += cy;
                }
                let (x1, y1) = if "QqTt".contains(last_cmd) {
                    (2.0 * cx - last_ctrl_x, 2.0 * cy - last_ctrl_y)
                } else {
                    (cx, cy)
                };
                for s in 1..=bezier_steps {
                    let t = s as f64 / bezier_steps as f64;
                    let mt = 1.0 - t;
                    current_pts.push(Point2D::new(
                        mt.powi(2) * cx + 2.0 * mt * t * x1 + t.powi(2) * dx,
                        mt.powi(2) * cy + 2.0 * mt * t * y1 + t.powi(2) * dy,
                    ));
                }
                cx = dx;
                cy = dy;
                last_ctrl_x = x1;
                last_ctrl_y = y1;
            },
            'A' | 'a' => loop {
                let a = nums(&mut ti, 7);
                if a.len() < 7 {
                    break;
                }
                let rx_a = a[0].abs();
                let ry_a = a[1].abs();
                let x_rot = a[2];
                let large_arc = a[3] != 0.0;
                let sweep = a[4] != 0.0;
                let (mut dx, mut dy) = (a[5], a[6]);
                if cmd == 'a' {
                    dx += cx;
                    dy += cy;
                }
                if rx_a == 0.0 || ry_a == 0.0 {
                    current_pts.push(Point2D::new(dx, dy));
                } else {
                    let arc_pts = approximate_arc(
                        cx, cy, rx_a, ry_a, x_rot, large_arc, sweep, dx, dy, arc_steps,
                    );
                    current_pts.extend(arc_pts);
                }
                cx = dx;
                cy = dy;
            },
            'Z' | 'z' => {
                cx = start_x;
                cy = start_y;
                current_pts.push(Point2D::new(cx, cy));
                if current_pts.len() >= 2 {
                    segments.push(PathSegment::new(std::mem::take(&mut current_pts)));
                } else {
                    current_pts.clear();
                }
            }
            _ => {}
        }

        last_cmd = cmd;
    }

    if current_pts.len() >= 2 {
        segments.push(PathSegment::new(current_pts));
    }
    segments
}

// ─── Aproximace SVG arc ───────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn approximate_arc(
    x1: f64,
    y1: f64,
    rx: f64,
    ry: f64,
    phi: f64,
    large_arc: bool,
    sweep: bool,
    x2: f64,
    y2: f64,
    steps: usize,
) -> Vec<Point2D> {
    let phi_rad = phi.to_radians();
    let cos_phi = phi_rad.cos();
    let sin_phi = phi_rad.sin();

    let dx2 = (x1 - x2) / 2.0;
    let dy2 = (y1 - y2) / 2.0;
    let x1p = cos_phi * dx2 + sin_phi * dy2;
    let y1p = -sin_phi * dx2 + cos_phi * dy2;

    let mut rx = rx;
    let mut ry = ry;
    let mut rx_sq = rx * rx;
    let mut ry_sq = ry * ry;
    let x1p_sq = x1p * x1p;
    let y1p_sq = y1p * y1p;

    let lambda = x1p_sq / rx_sq + y1p_sq / ry_sq;
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
        rx_sq = rx * rx;
        ry_sq = ry * ry;
    }

    let sign = if large_arc == sweep {
        -1.0_f64
    } else {
        1.0_f64
    };
    let sq = ((rx_sq * ry_sq - rx_sq * y1p_sq - ry_sq * x1p_sq)
        / (rx_sq * y1p_sq + ry_sq * x1p_sq))
        .max(0.0);
    let coef = sign * sq.sqrt();
    let cxp = coef * rx * y1p / ry;
    let cyp = -coef * ry * x1p / rx;
    let cx = cos_phi * cxp - sin_phi * cyp + (x1 + x2) / 2.0;
    let cy = sin_phi * cxp + cos_phi * cyp + (y1 + y2) / 2.0;

    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| -> f64 {
        let dot = ux * vx + uy * vy;
        let len = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        let mut ang = (dot / len).clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            ang = -ang;
        }
        ang
    };

    let ux = (x1p - cxp) / rx;
    let uy = (y1p - cyp) / ry;
    let vx = (-x1p - cxp) / rx;
    let vy = (-y1p - cyp) / ry;

    let theta1 = angle(1.0, 0.0, ux, uy);
    let mut d_theta = angle(ux, uy, vx, vy);
    if !sweep && d_theta > 0.0 {
        d_theta -= std::f64::consts::TAU;
    } else if sweep && d_theta < 0.0 {
        d_theta += std::f64::consts::TAU;
    }

    (1..=steps)
        .map(|s| {
            let th = theta1 + d_theta * (s as f64 / steps as f64);
            Point2D::new(
                cos_phi * rx * th.cos() - sin_phi * ry * th.sin() + cx,
                sin_phi * rx * th.cos() + cos_phi * ry * th.sin() + cy,
            )
        })
        .collect()
}

// ─── Testy ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Point2D;

    fn bbox(paths: &SubstratePaths) -> (f64, f64, f64, f64) {
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for seg in &paths.segments {
            for p in &seg.points {
                min_x = min_x.min(p.x);
                max_x = max_x.max(p.x);
                min_y = min_y.min(p.y);
                max_y = max_y.max(p.y);
            }
        }
        (min_x, max_x, min_y, max_y)
    }

    fn is_closed(pts: &[Point2D]) -> bool {
        let a = pts.first().unwrap();
        let b = pts.last().unwrap();
        (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6
    }

    #[test]
    fn test_parse_invalid_svg() {
        assert!(parse_svg("tohle není xml", 1.0).segments.is_empty());
        assert!(parse_svg("<svg></svg>", 1.0).segments.is_empty());
    }

    #[test]
    fn test_parse_rect() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><rect x="5" y="5" width="10" height="20" fill="black"/></svg>"#;
        let out = parse_svg(svg, 1.0);
        assert_eq!(out.segments.len(), 1);
        let seg = &out.segments[0];
        assert!(is_closed(&seg.points));
        assert_eq!(seg.is_filled, Some(true));
        let (min_x, max_x, min_y, max_y) = bbox(&out);
        assert!((max_x - min_x - 10.0).abs() < 1e-6);
        assert!((max_y - min_y - 20.0).abs() < 1e-6);
        assert!((min_x - 5.0).abs() < 1e-6 && (min_y - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_parse_rect_fill_none() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><rect width="10" height="10" fill="none" stroke="black"/></svg>"#;
        let out = parse_svg(svg, 1.0);
        assert_eq!(out.segments.len(), 1);
        assert_eq!(out.segments[0].is_filled, Some(false));
    }

    #[test]
    fn test_parse_path_closed_triangle() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0 L10 0 L5 8 Z"/></svg>"#;
        let out = parse_svg(svg, 1.0);
        assert_eq!(out.segments.len(), 1);
        let pts = &out.segments[0].points;
        assert!(pts.len() >= 4);
        assert!(is_closed(pts));
    }

    #[test]
    fn test_parse_circle_fineness() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><circle cx="10" cy="10" r="5"/></svg>"#;
        let coarse = parse_svg(svg, 0.5);
        let fine = parse_svg(svg, 2.0);
        assert_eq!(coarse.segments.len(), 1);
        assert_eq!(fine.segments.len(), 1);
        // Vyšší jemnost → více bodů na oblouku
        assert!(fine.segments[0].points.len() > coarse.segments[0].points.len());
        // Průměr 10 v obou osách
        let (min_x, max_x, min_y, max_y) = bbox(&fine);
        assert!((max_x - min_x - 10.0).abs() < 0.1);
        assert!((max_y - min_y - 10.0).abs() < 0.1);
    }

    #[test]
    fn test_parse_polyline_open_polygon_closed() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
            <polyline points="0,0 10,0 10,10"/>
            <polygon points="20,0 30,0 25,8"/>
        </svg>"#;
        let out = parse_svg(svg, 1.0);
        assert_eq!(out.segments.len(), 2);
        assert!(!is_closed(&out.segments[0].points), "polyline je otevřená");
        assert!(is_closed(&out.segments[1].points), "polygon je uzavřený");
        assert_eq!(out.segments[0].is_filled, Some(false));
    }

    #[test]
    fn test_viewbox_mm_scaling() {
        // width=50mm, viewBox 0 0 100 100 → 1 jednotka = 0.5 mm
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="50mm" height="50mm" viewBox="0 0 100 100"><rect width="100" height="100"/></svg>"#;
        let out = parse_svg(svg, 1.0);
        let (min_x, max_x, _, _) = bbox(&out);
        assert!((max_x - min_x - 50.0).abs() < 1e-6);
    }

    #[test]
    fn test_path_curve_fineness() {
        // Kubická Bézier křivka — jemnost zvyšuje počet bodů
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0 C 0 10, 10 10, 10 0"/></svg>"#;
        let coarse = parse_svg(svg, 0.5);
        let fine = parse_svg(svg, 3.0);
        assert!(!coarse.segments.is_empty() && !fine.segments.is_empty());
        assert!(fine.segments[0].points.len() > coarse.segments[0].points.len());
    }

    // ─── Klony <use> a transformace ──────────────────────────────────────────

    #[test]
    fn test_group_transform_applies() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100">
            <g transform="translate(50,5)"><rect width="10" height="10"/></g>
        </svg>"##;
        let out = parse_svg(svg, 1.0);
        assert_eq!(out.segments.len(), 1);
        let (min_x, max_x, min_y, max_y) = bbox(&out);
        assert!((min_x - 50.0).abs() < 1e-6 && (max_x - 60.0).abs() < 1e-6);
        assert!((min_y - 5.0).abs() < 1e-6 && (max_y - 15.0).abs() < 1e-6);
    }

    #[test]
    fn test_nested_transform_composition() {
        // Vnější translate se skládá s vnitřním scale (nejlevější = nejvíc vnější)
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100">
            <g transform="translate(10,10) scale(2)"><rect width="5" height="5"/></g>
        </svg>"##;
        let out = parse_svg(svg, 1.0);
        let (min_x, max_x, _, _) = bbox(&out);
        assert!((min_x - 10.0).abs() < 1e-6, "min_x = {min_x}");
        assert!((max_x - 20.0).abs() < 1e-6, "max_x = {max_x}");
    }

    #[test]
    fn test_use_clone_is_imported() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"
                         width="100mm" height="100mm" viewBox="0 0 100 100">
            <g id="g1"><rect width="10" height="10"/></g>
            <use xlink:href="#g1" transform="translate(50,0)"/>
        </svg>"##;
        let out = parse_svg(svg, 1.0);
        assert_eq!(out.segments.len(), 2, "originál + klon");
        let (min_x, max_x, _, _) = bbox(&out);
        assert!((min_x - 0.0).abs() < 1e-6 && (max_x - 60.0).abs() < 1e-6);
    }

    #[test]
    fn test_use_href_without_xlink_and_xy_offset() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100">
            <defs><rect id="r" width="10" height="10"/></defs>
            <use href="#r" x="20" y="30"/>
        </svg>"##;
        let out = parse_svg(svg, 1.0);
        assert_eq!(out.segments.len(), 1);
        let (min_x, _, min_y, _) = bbox(&out);
        assert!((min_x - 20.0).abs() < 1e-6 && (min_y - 30.0).abs() < 1e-6);
    }

    #[test]
    fn test_use_rotate_around_center() {
        // rotate(180, 50, 50) překlopí obdélník z levého dolního do pravého horního kvadrantu
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"
                         width="100mm" height="100mm" viewBox="0 0 100 100">
            <g id="g1"><rect x="0" y="0" width="20" height="10"/></g>
            <use xlink:href="#g1" transform="rotate(180,50,50)"/>
        </svg>"##;
        let out = parse_svg(svg, 1.0);
        assert_eq!(out.segments.len(), 2);
        let clone = &out.segments[1];
        let (mut mn_x, mut mx_x, mut mn_y, mut mx_y) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for p in &clone.points {
            mn_x = mn_x.min(p.x);
            mx_x = mx_x.max(p.x);
            mn_y = mn_y.min(p.y);
            mx_y = mx_y.max(p.y);
        }
        assert!((mn_x - 80.0).abs() < 1e-6 && (mx_x - 100.0).abs() < 1e-6, "x [{mn_x},{mx_x}]");
        assert!((mn_y - 90.0).abs() < 1e-6 && (mx_y - 100.0).abs() < 1e-6, "y [{mn_y},{mx_y}]");
    }

    #[test]
    fn test_defs_render_only_through_use() {
        let hidden = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100">
            <defs><rect id="r" width="10" height="10"/></defs>
            <clipPath id="c"><rect width="50" height="50"/></clipPath>
        </svg>"##;
        assert!(parse_svg(hidden, 1.0).segments.is_empty(), "<defs>/<clipPath> se nekreslí");

        let symbol = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100">
            <symbol id="s"><circle cx="5" cy="5" r="5"/></symbol>
            <use href="#s"/><use href="#s" x="20"/>
        </svg>"##;
        assert_eq!(parse_svg(symbol, 1.0).segments.len(), 2, "<symbol> přes <use> ano");
    }

    #[test]
    fn test_nested_use_and_shared_target() {
        // #cell se tessellluje jednou, použije se 4× (1 originál + 2 v #pair + 2 v klonu #pair)
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100">
            <g id="cell"><rect width="5" height="5"/></g>
            <g id="pair"><use href="#cell"/><use href="#cell" x="10"/></g>
            <use href="#pair" y="20"/>
        </svg>"##;
        let out = parse_svg(svg, 1.0);
        assert_eq!(out.segments.len(), 5);
        let (_, max_x, _, max_y) = bbox(&out);
        assert!((max_x - 15.0).abs() < 1e-6, "max_x = {max_x}");
        assert!((max_y - 25.0).abs() < 1e-6, "max_y = {max_y}");
    }

    #[test]
    fn test_use_cycle_and_dangling_href() {
        // Cyklus se musí ukončit, nikoli zacyklit
        let cyclic = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10">
            <g id="a"><use href="#b"/><rect width="2" height="2"/></g>
            <g id="b"><use href="#a"/></g>
        </svg>"##;
        let out = parse_svg(cyclic, 1.0);
        assert!(!out.segments.is_empty(), "reálná geometrie se načte i s cyklem");

        let dangling = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10">
            <use href="#neexistuje"/><use/>
        </svg>"##;
        assert!(parse_svg(dangling, 1.0).segments.is_empty());
    }

    #[test]
    fn test_style_inherits_from_group_and_use_site() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100">
            <defs><rect id="r" width="10" height="10"/></defs>
            <g style="fill:none;stroke:#000000">
                <rect width="10" height="10"/>
                <use href="#r" x="20"/>
            </g>
        </svg>"##;
        let out = parse_svg(svg, 1.0);
        assert_eq!(out.segments.len(), 2);
        for seg in &out.segments {
            assert_eq!(seg.is_filled, Some(false), "fill:none se dědí i do klonu");
            assert_eq!(seg.has_stroke, Some(true), "stroke se dědí i do klonu");
        }
    }

    #[test]
    fn test_own_paint_wins_over_inherited() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100">
            <g fill="none"><rect width="10" height="10" fill="black"/></g>
        </svg>"##;
        let out = parse_svg(svg, 1.0);
        assert_eq!(out.segments[0].is_filled, Some(true));
    }

    #[test]
    fn test_display_none_skipped() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100">
            <g style="display:none"><rect width="10" height="10"/></g>
            <g style="display:inline"><rect width="10" height="10"/></g>
            <rect width="10" height="10" display="none"/>
        </svg>"##;
        assert_eq!(parse_svg(svg, 1.0).segments.len(), 1);
    }

    #[test]
    fn test_viewbox_origin_offset() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="50mm" height="50mm" viewBox="10 20 50 50">
            <rect x="10" y="20" width="10" height="10"/>
        </svg>"##;
        let out = parse_svg(svg, 1.0);
        let (min_x, _, min_y, _) = bbox(&out);
        assert!(min_x.abs() < 1e-6 && min_y.abs() < 1e-6, "viewBox origin = počátek");
    }

    #[test]
    fn test_polyline_space_separated_points() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100">
            <polyline points="0 0 10 0 10 10"/>
        </svg>"##;
        let out = parse_svg(svg, 1.0);
        assert_eq!(out.segments.len(), 1);
        assert_eq!(out.segments[0].points.len(), 3);
    }

    #[test]
    fn test_inkscape_clone_layout_roundtrip() {
        // Struktura jako v reálném souboru: skupina + její klon přes rotate(180)
        // a čtyři klony jednoho rohu.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"
                         width="100mm" height="100mm" viewBox="0 0 100 100">
          <use x="0" y="0" xlink:href="#g2" id="use3" transform="rotate(180,50,50)" style="display:inline"/>
          <g id="g2">
            <path d="M 40,60 V 90" style="fill:none;stroke:#000000"/>
            <ellipse cx="40" cy="95" rx="2" ry="2" style="fill:#000000;stroke:#000000"/>
          </g>
          <g id="g3" style="display:inline">
            <path id="corner" d="m 90,80 v 10 h -10" style="fill:none;stroke:#000000"/>
            <use xlink:href="#corner" transform="rotate(90,50,50)"/>
            <use xlink:href="#corner" transform="rotate(-90,50,50)"/>
            <use xlink:href="#corner" transform="rotate(180,50,50)"/>
          </g>
        </svg>"##;
        let out = parse_svg(svg, 1.0);
        // 2 (g2) + 2 (klon g2) + 1 (roh) + 3 (klony rohu) = 8 segmentů
        assert_eq!(out.segments.len(), 8, "všechny klony se načetly");
        // Rohy pokrývají všechny čtyři strany → bbox 10…90 v obou osách
        let (min_x, max_x, min_y, max_y) = bbox(&out);
        assert!((min_x - 10.0).abs() < 1e-6, "min_x = {min_x}");
        assert!((max_x - 90.0).abs() < 1e-6, "max_x = {max_x}");
        assert!((min_y - 3.0).abs() < 1e-6, "min_y = {min_y}");
        assert!((max_y - 97.0).abs() < 1e-6, "max_y = {max_y}");
        // Elipsa z originálu je vyplněná, cesta ne
        assert!(out.segments.iter().any(|s| s.is_filled == Some(true)));
        assert!(out.segments.iter().any(|s| s.is_filled == Some(false) && s.has_stroke == Some(true)));
    }
}
