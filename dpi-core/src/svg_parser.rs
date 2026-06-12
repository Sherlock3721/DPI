use crate::types::{PathSegment, Point2D, SubstratePaths};
use roxmltree::Document;

// ─── Veřejné API ─────────────────────────────────────────────────────────────

pub fn parse_svg(svg_text: &str, fineness: f64) -> SubstratePaths {
    let mut segments: Vec<PathSegment> = Vec::new();

    let doc = match Document::parse(svg_text) {
        Ok(d) => d,
        Err(_) => return SubstratePaths::new(segments),
    };

    let root = doc.root_element();
    let mut svg_to_mm = 1.0_f64;

    // Výpočet převodního koeficientu z atributů kořenového <svg>
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

    // Průchod všemi elementy dokumentu
    for node in doc.descendants() {
        if !node.is_element() {
            continue;
        }
        match node.tag_name().name() {
            "line" => {
                let x1 = attr_f64(&node, "x1");
                let y1 = attr_f64(&node, "y1");
                let x2 = attr_f64(&node, "x2");
                let y2 = attr_f64(&node, "y2");
                segments.push(PathSegment {
                    points: vec![Point2D::new(x1, y1), Point2D::new(x2, y2)],
                    is_filled: Some(false),
                    has_stroke: Some(has_stroke(&node)),
                });
            }
            "rect" => {
                let x = attr_f64(&node, "x");
                let y = attr_f64(&node, "y");
                let w = attr_f64(&node, "width");
                let h = attr_f64(&node, "height");
                if w <= 0.0 || h <= 0.0 {
                    continue;
                }
                let filled = has_fill(&node);
                let rx_raw = attr_f64(&node, "rx");
                let ry_raw = attr_f64(&node, "ry");
                let mut rx = if rx_raw > 0.0 { rx_raw } else { ry_raw };
                let mut ry = if ry_raw > 0.0 { ry_raw } else { rx_raw };
                rx = rx.min(w / 2.0);
                ry = ry.min(h / 2.0);

                let stroke = has_stroke(&node);
                if rx <= 0.0 || ry <= 0.0 {
                    let pts = vec![
                        Point2D::new(x, y),
                        Point2D::new(x + w, y),
                        Point2D::new(x + w, y + h),
                        Point2D::new(x, y + h),
                        Point2D::new(x, y),
                    ];
                    segments.push(PathSegment {
                        points: pts,
                        is_filled: Some(filled),
                        has_stroke: Some(stroke),
                    });
                } else {
                    let steps = (8.0 * fineness).round().max(3.0) as usize;
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
                    segments.push(PathSegment {
                        points: pts,
                        is_filled: Some(filled),
                        has_stroke: Some(stroke),
                    });
                }
            }
            "circle" => {
                let cx = attr_f64(&node, "cx");
                let cy = attr_f64(&node, "cy");
                let r = attr_f64(&node, "r");
                if r <= 0.0 {
                    continue;
                }
                let steps = (32.0 * fineness).round().max(4.0) as usize;
                let pts: Vec<Point2D> = (0..=steps)
                    .map(|j| {
                        let theta = (j as f64 / steps as f64) * std::f64::consts::TAU;
                        Point2D::new(cx + r * theta.cos(), cy + r * theta.sin())
                    })
                    .collect();
                segments.push(PathSegment {
                    points: pts,
                    is_filled: Some(has_fill(&node)),
                    has_stroke: Some(has_stroke(&node)),
                });
            }
            "ellipse" => {
                let cx = attr_f64(&node, "cx");
                let cy = attr_f64(&node, "cy");
                let rx = attr_f64(&node, "rx");
                let ry = attr_f64(&node, "ry");
                if rx <= 0.0 || ry <= 0.0 {
                    continue;
                }
                let steps = (32.0 * fineness).round().max(4.0) as usize;
                let pts: Vec<Point2D> = (0..=steps)
                    .map(|j| {
                        let theta = (j as f64 / steps as f64) * std::f64::consts::TAU;
                        Point2D::new(cx + rx * theta.cos(), cy + ry * theta.sin())
                    })
                    .collect();
                segments.push(PathSegment {
                    points: pts,
                    is_filled: Some(has_fill(&node)),
                    has_stroke: Some(has_stroke(&node)),
                });
            }
            "polyline" | "polygon" => {
                let is_polygon = node.tag_name().name() == "polygon";
                let points_attr = node.attribute("points").unwrap_or("");
                let mut pts: Vec<Point2D> = Vec::new();
                for pair in points_attr.split_whitespace() {
                    let parts: Vec<&str> = pair.split(',').collect();
                    if parts.len() == 2 {
                        if let (Ok(px), Ok(py)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>())
                        {
                            pts.push(Point2D::new(px, py));
                        }
                    }
                }
                if pts.len() < 2 {
                    continue;
                }
                if is_polygon {
                    if let Some(first) = pts.first().cloned() {
                        pts.push(first);
                    }
                }
                let filled = if is_polygon { Some(has_fill(&node)) } else { Some(false) };
                let stroke = Some(has_stroke(&node));
                segments.push(PathSegment {
                    points: pts,
                    is_filled: filled,
                    has_stroke: stroke,
                });
            }
            "path" => {
                let elem_filled = has_fill(&node);
                let elem_stroke = has_stroke(&node);
                let d = node.attribute("d").unwrap_or("");
                let mut path_segs = parse_path_d(d, fineness);
                for seg in &mut path_segs {
                    let closed = is_path_closed(&seg.points);
                    seg.is_filled = Some(if closed { elem_filled } else { false });
                    seg.has_stroke = Some(elem_stroke);
                }
                segments.extend(path_segs);
            }
            _ => {}
        }
    }

    // Převod jednotek na mm
    if (svg_to_mm - 1.0).abs() > 1e-9 {
        for seg in &mut segments {
            for pt in &mut seg.points {
                pt.x *= svg_to_mm;
                pt.y *= svg_to_mm;
            }
        }
    }

    SubstratePaths::new(segments)
}

// ─── Pomocné funkce ───────────────────────────────────────────────────────────

fn attr_f64(node: &roxmltree::Node, name: &str) -> f64 {
    node.attribute(name)
        .and_then(|v| v.parse().ok())
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

fn has_fill(node: &roxmltree::Node) -> bool {
    if let Some(style) = node.attribute("style") {
        for part in style.split(';') {
            let part = part.trim().to_lowercase();
            if let Some(rest) = part.strip_prefix("fill") {
                let rest = rest.trim_start();
                if let Some(val) = rest.strip_prefix(':') {
                    let val = val.trim();
                    if !val.is_empty() {
                        return val != "none";
                    }
                }
            }
        }
    }
    if let Some(fill) = node.attribute("fill") {
        return fill.to_lowercase() != "none";
    }
    true // SVG výchozí výplň je černá
}

fn has_stroke(node: &roxmltree::Node) -> bool {
    if let Some(style) = node.attribute("style") {
        for part in style.split(';') {
            let part = part.trim().to_lowercase();
            // Jen "stroke:" — ne "stroke-width:", "stroke-dasharray:" apod.
            if part.starts_with("stroke:") || part.starts_with("stroke :") {
                if let Some(val) = part.split_once(':').map(|x| x.1) {
                    let val = val.trim();
                    if !val.is_empty() {
                        return val != "none";
                    }
                }
            }
        }
    }
    if let Some(stroke) = node.attribute("stroke") {
        return stroke.to_lowercase() != "none";
    }
    false // SVG výchozí tah je none
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
}
