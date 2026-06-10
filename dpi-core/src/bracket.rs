// Geometrie, SVG a STL export pro "držák" (bracket) — viz BracketExportModal.svelte.
//
// Veškerý výpočet geometrie (cesty, obdélníky, středy děr, rozložení kopií…) je
// soustředěn zde jako jediný zdroj pravdy: stejná data se použijí pro živý
// náhled, SVG export i rasterizaci pro STL export. Frontend pouze zavolá
// `compute_bracket_geometry` / `generate_bracket_svg` / `build_bracket_stl`
// přes wasm bindings (viz wasm.rs) a vykreslí/uloží výsledek.

use std::fmt::Write as FmtWrite;

use serde::{Deserialize, Serialize};

use crate::types::{BedConfig, LayoutPosition};

// Mezera mezi oběma rameny flex L u vnitřního rohu — ramena se téměř, ale
// nikdy zcela nedotýkají (tenký řez = vyšší pružnost spoje).
const FLEX_SPLIT_GAP: f64 = 0.6;

// ─── Vstupní parametry ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BracketParams {
    pub glass_w: f64,
    pub glass_h: f64,
    pub glass_label: String,

    pub left_border_w: f64,
    pub bottom_border_h: f64,
    pub extend_walls: bool,
    pub extend_amount: f64,
    pub fixed_thick_x: f64,
    pub fixed_thick_y: f64,
    pub flex_thick: f64,
    pub flex_gap: f64,
    pub spring_count_x: i32,
    pub spring_count_y: i32,
    pub spring_width: f64,
    pub spring_bends: i32,
    pub spring_gap_mod: f64,
    pub corner_r: f64,
    pub magnet_size: f64,
    pub magnet_shape: String, // "circle" | "square"

    pub multiply_count: usize,
    pub spacing: f64,
    pub bed: BedConfig,
}

// ─── Výstupní geometrie ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PointF {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RectF {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpringGeom {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// Cesta meandru řezů (SVG path data) — připravená k vykreslení/rasterizaci.
    pub path: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CopyOffset {
    pub tx: f64,
    pub ty: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BracketGeometry {
    // Rozměry jedné kopie ("core")
    pub b_w: f64,
    pub b_h: f64,
    pub hole_x: f64,
    pub hole_y: f64,
    pub hole_x2: f64,
    pub hole_y2: f64,

    // Cesty hlavních prvků (SVG path data, lokální souřadnice core)
    pub flex_l_path: String,
    pub fixed_l_path: String,
    pub corner_square_size: f64,
    pub magnet_center: PointF,
    pub effective_magnet_size: f64,

    pub x_springs: Vec<SpringGeom>,
    pub y_springs: Vec<SpringGeom>,

    // Multiplikace a sestava
    pub multiply_positions: Vec<LayoutPosition>,
    pub copy_offsets: Vec<CopyOffset>,
    pub corner_hole_centers: Vec<PointF>,

    pub left_wall_rect: RectF,
    pub bottom_wall_rect: RectF,
    pub wall_extend: f64,
    pub wall_magnet_centers: Vec<PointF>,

    pub assembly_min_x: f64,
    pub assembly_min_y: f64,
    pub assembly_max_x: f64,
    pub assembly_max_y: f64,
    pub assembly_w: f64,
    pub assembly_h: f64,

    /// Skutečný maximální počet kopií, které se vejdou na podložku (odpovídá
    /// počtu pozic, jež by vrátil `grid_layout_positions` pro neomezený vstup).
    /// Frontend to používá jako horní mez pro UI vstup multiplyCount — tento
    /// výpočet proběhne v Rustu, takže Svelte nemusí duplikovat logiku layoutu.
    pub max_multiply: usize,
}

// ─── Pomocné geometrické funkce (cesty řezů a pružin) ─────────────────────────

/// Tloušťka řezů meandru (min. 0.5 mm, aby ji tiskárna s 0.4mm tryskou nezalila,
/// + modifikátor gap_mod pro kompenzaci tolerancí tiskárny).
#[inline]
fn cut_thickness(dim: f64, bends: i32, gap_mod: f64) -> f64 {
    (dim / (bends as f64 * 5.0)).clamp(0.5, 1.2) + gap_mod
}

/// Vodorovný řez ("slot") s jedním zaobleným koncem a rozevřeným ústím do díry.
/// Konec uvnitř materiálu pružiny (rounded_end_is_end=true) má plné zaoblení
/// r = height/2 — tam hrozí iniciace trhlin nejvíc. Konec ústící do okolní díry
/// se navenek rozevře obloukem rf = polovina délky "nohy" pružiny mezi sousedními
/// řezy, čímž se zaoblí (zkosí ven) špičky sousedních noh místo rohu řezu samotného.
fn h_slot(x0: f64, x1: f64, y_top: f64, height: f64, rounded_end_is_end: bool, leg_gap: f64) -> String {
    let r  = (height / 2.0).min((x1 - x0).abs());
    let rf = leg_gap.min(((x1 - x0).abs() - r).max(0.0) * 0.5);
    let y_b = y_top + height;
    if rounded_end_is_end {
        // Zaoblený konec vpravo (x1), rozevřené ústí vlevo (x0).
        format!(
            "M {x0p} {y_top} H {x1r} A {r} {r} 0 0 1 {x1r} {y_b} H {x0p} \
             A {rf} {rf} 0 0 0 {x0} {y_be} V {y_te} A {rf} {rf} 0 0 0 {x0p} {y_top} Z ",
            x0p = x0 + rf, x1r = x1 - r, y_b = y_b, x0 = x0,
            y_be = y_b + rf, y_te = y_top - rf, y_top = y_top,
        )
    } else {
        // Zaoblený konec vlevo (x0), rozevřené ústí vpravo (x1).
        format!(
            "M {x0r} {y_top} H {x1rf} A {rf} {rf} 0 0 0 {x1} {y_te} V {y_be} \
             A {rf} {rf} 0 0 0 {x1rf} {y_b} H {x0r} A {r} {r} 0 0 1 {x0r} {y_top} Z ",
            x0r = x0 + r, x1rf = x1 - rf, x1 = x1,
            y_te = y_top - rf, y_be = y_b + rf, y_b = y_b, y_top = y_top,
        )
    }
}

/// Svislý řez ("slot") — viz `h_slot`, jen otočený o 90°.
fn v_slot(x: f64, y0: f64, y1: f64, width: f64, rounded_end_is_end: bool, leg_gap: f64) -> String {
    let r  = (width / 2.0).min((y1 - y0).abs());
    let rf = leg_gap.min(((y1 - y0).abs() - r).max(0.0) * 0.5);
    let x_r = x + width;
    if rounded_end_is_end {
        // Zaoblený konec dole (y1), rozevřené ústí nahoře (y0).
        format!(
            "M {x} {y0rf} A {rf} {rf} 0 0 0 {xrf} {y0} H {xr_rf} \
             A {rf} {rf} 0 0 0 {xr} {y0rf} V {y1r} A {r} {r} 0 0 1 {x} {y1r} V {y0rf} Z ",
            x = x, y0rf = y0 + rf, xrf = x - rf, xr_rf = x_r + rf,
            xr = x_r, y0 = y0, y1r = y1 - r,
        )
    } else {
        // Zaoblený konec nahoře (y0), rozevřené ústí dole (y1).
        format!(
            "M {x} {y1rf} A {rf} {rf} 0 0 1 {xrf} {y1} H {xr_rf} \
             A {rf} {rf} 0 0 1 {xr} {y1rf} V {y0r} A {r} {r} 0 0 0 {x} {y0r} V {y1rf} Z ",
            x = x, y1rf = y1 - rf, xrf = x - rf, y1 = y1,
            xr_rf = x_r + rf, xr = x_r, y0r = y0 + r,
        )
    }
}

// ─── Pružiny ──────────────────────────────────────────────────────────────────
//
// Osa pružiny (X vs. Y) se předá jako parametr, čímž se eliminuje duplicita
// h_spring_path / v_spring_path. Obě jsou dříve byly totožné — jen s prohozenými
// osami.

/// Osa pružiny pro `spring_path`.
#[derive(Clone, Copy)]
enum SpringAxis { X, Y }

/// Obecná pružina (meandr řezů) — optimalizováno pro 3D tisk: zaoblený konec
/// vždy na straně materiálu pružiny (uvnitř), rozevřené ústí směrem do díry.
///
/// `axis = X` → vodorovné řezy, pružina leží v horizontální zóně.
/// `axis = Y` → svislé řezy, pružina leží ve vertikální zóně.
fn spring_path(x: f64, y: f64, w: f64, h: f64, bends: i32, gap_mod: f64, axis: SpringAxis) -> String {
    if bends <= 0 || w < 1.0 || h < 1.0 {
        return String::new();
    }
    match axis {
        SpringAxis::X => {
            let cut_t = cut_thickness(h, bends, gap_mod);
            let tab_w  = (w * 0.2_f64).max(1.0);
            let leg_h  = if bends > 1 {
                (h - bends as f64 * cut_t) / (bends as f64 - 1.0)
            } else {
                (h - cut_t) / 2.0
            };
            if leg_h < 0.3 && bends > 1 { return String::new(); }
            let mut d = String::new();
            for i in 0..bends {
                let cy = if bends > 1 { y + i as f64 * (cut_t + leg_h) } else { y + leg_h };
                if i % 2 == 0 {
                    d.push_str(&h_slot(x, x + w - tab_w, cy, cut_t, true,  leg_h));
                } else {
                    d.push_str(&h_slot(x + tab_w, x + w, cy, cut_t, false, leg_h));
                }
            }
            d
        }
        SpringAxis::Y => {
            let cut_t = cut_thickness(w, bends, gap_mod);
            let tab_h  = (h * 0.2_f64).max(1.0);
            let leg_w  = if bends > 1 {
                (w - bends as f64 * cut_t) / (bends as f64 - 1.0)
            } else {
                (w - cut_t) / 2.0
            };
            if leg_w < 0.3 && bends > 1 { return String::new(); }
            let mut d = String::new();
            for i in 0..bends {
                let cx = if bends > 1 { x + i as f64 * (cut_t + leg_w) } else { x + leg_w };
                if i % 2 == 0 {
                    d.push_str(&v_slot(cx, y, y + h - tab_h, cut_t, true,  leg_w));
                } else {
                    d.push_str(&v_slot(cx, y + tab_h, y + h, cut_t, false, leg_w));
                }
            }
            d
        }
    }
}

fn magnet_cutout_markup(cx: f64, cy: f64, fill: &str, shape: &str, size: f64) -> String {
    if size <= 0.0 {
        return String::new();
    }
    if shape == "square" {
        format!(
            r#"<rect x="{:.3}" y="{:.3}" width="{:.3}" height="{:.3}" fill="{}"/>"#,
            cx - size / 2.0, cy - size / 2.0, size, size, fill
        )
    } else {
        format!(
            r#"<circle cx="{:.3}" cy="{:.3}" r="{:.3}" fill="{}"/>"#,
            cx, cy, size / 2.0, fill
        )
    }
}

/// Layout pro vizualizaci/export držáku — vychází z mřížky `get_layout_positions`,
/// ALE pořadí plnění je odlišné: 1. sloupec se plní shora dolů, KAŽDÝ další
/// sloupec (2., 3., …) vždy zdola nahoru. Mřížka pozic (řádky) zůstává stejná —
/// mění se jen POŘADÍ, v jakém se do ní vzorky vkládají při přetečení do dalšího sloupce.
fn grid_layout_positions(n: usize, w: f64, h: f64, sp: f64, bed: &BedConfig) -> Vec<LayoutPosition> {
    let mut positions = Vec::new();
    let col_top = bed.offset_y;
    let rows_per_col = (((bed.max_y - col_top + sp) / (h + sp)).floor().max(0.0)) as usize;
    let mut curr_x = bed.min_x + bed.offset_x;
    let mut col = 0usize;
    while positions.len() < n {
        if curr_x + w > bed.max_x || rows_per_col == 0 {
            break;
        }
        let reversed = col > 0;
        let mut r = 0usize;
        while r < rows_per_col && positions.len() < n {
            let row_index = if reversed { rows_per_col - 1 - r } else { r };
            let y = col_top + row_index as f64 * (h + sp);
            positions.push(LayoutPosition { x: curr_x, y, width: w, height: h, is_prime: false });
            r += 1;
        }
        curr_x += w + sp;
        col += 1;
    }
    positions
}

/// Vypočítá kapacitu mřížky (max. počet kopií na podložku) bez omezení n —
/// používá se pro pole `max_multiply` v `BracketGeometry`.
fn grid_max_capacity(w: f64, h: f64, sp: f64, bed: &BedConfig) -> usize {
    let col_top = bed.offset_y;
    let rows_per_col = (((bed.max_y - col_top + sp) / (h + sp)).floor().max(0.0)) as usize;
    if rows_per_col == 0 { return 0; }
    let mut curr_x = bed.min_x + bed.offset_x;
    let mut total = 0usize;
    loop {
        if curr_x + w > bed.max_x { break; }
        total += rows_per_col;
        curr_x += w + sp;
    }
    total.max(1)
}

// ─── Hlavní výpočet geometrie ─────────────────────────────────────────────────

pub fn compute_bracket_geometry(p: &BracketParams) -> BracketGeometry {
    let frame_x = p.spacing;
    let frame_y = p.spacing;

    // === GEOMETRIE JEDNÉ KOPIE ("core") ===
    // Core: x=[0..bW], y=[0..bH]; díra skla: x=[0,glassW], y=[holeY,holeY2].
    let b_w = p.glass_w + frame_x;
    let b_h = frame_y + p.glass_h;
    let hole_x  = 0.0;
    let hole_y  = frame_y;
    let hole_x2 = p.glass_w;
    let hole_y2 = frame_y + p.glass_h;

    // === FLEX L (uvnitř díry skla) ===
    // Dvě samostatná ramena, zkrácená o FLEX_SPLIT_GAP u vnitřního rohu —
    // jejich rohy se téměř, ale nikdy zcela nedotýkají (pružný spoj).
    let flex_l_path = {
        let x0 = hole_x + p.flex_gap;
        let x1 = hole_x2;
        let xi = hole_x2 - p.flex_thick;
        let y0 = hole_y;
        let y1 = hole_y2 - p.flex_gap;
        let yi = hole_y + p.flex_thick;
        let g  = FLEX_SPLIT_GAP;
        format!(
            "M {x0},{y0} H {hx} V {yi} H {x0} Z M {xi},{vy0} H {x1} V {y1} H {xi} Z",
            x0 = x0, y0 = y0, hx = xi - g, yi = yi,
            xi = xi, vy0 = yi + g, x1 = x1, y1 = y1,
        )
    };

    // === PEVNÝ L (podél celého vnějšího top+right okraje core) ===
    // Horní rameno přes celou šířku, pravé přes celou výšku — navazují na
    // sousední kopie/stěny sestavy. Výztužný čtverec ve vnitřním rohu vyplňuje
    // zářez směrem k díře skla, ale nikdy se nedotkne flex L (odečtena clearance
    // flexGap; limit ramene = MAX z osových omezení mínus clearance, celkový
    // limit = MIN přes obě ramena flex L).
    let corner_square_size = {
        let cx = b_w - p.fixed_thick_x;
        let cy = p.fixed_thick_y;
        let xi = hole_x2 - p.flex_thick;
        let g  = FLEX_SPLIT_GAP;
        let e  = p.flex_gap;
        let limit_h_arm = (cx - (xi - g)).max(hole_y - cy) - e;
        let limit_v_arm = (cx - hole_x2).max((hole_y + p.flex_thick + g) - cy) - e;
        limit_h_arm.min(limit_v_arm).max(0.0)
    };
    let fixed_l_path = {
        let base = format!(
            "M 0,0 H {bw} V {bh} H {bwx} V {fty} H 0 Z",
            bw = b_w, bh = b_h, bwx = b_w - p.fixed_thick_x, fty = p.fixed_thick_y,
        );
        if corner_square_size <= 0.0 {
            base
        } else {
            let cx = b_w - p.fixed_thick_x;
            let cy = p.fixed_thick_y;
            let sx = cx - corner_square_size;
            format!(
                "{base} M {sx},{cy} H {cx} V {cye} H {sx} Z",
                base = base, sx = sx, cy = cy, cx = cx, cye = cy + corner_square_size,
            )
        }
    };

    // Střed výztužného čtverce — díra pro magnet. Čtverec leží těsně u vnitřní
    // hrany pevného L: vzdálenost od vnějších hran desky = tloušťka ramene
    // (vpravo fixedThickX, nahoře fixedThickY) + polovina strany čtverce.
    let magnet_center = PointF {
        x: b_w - p.fixed_thick_x - corner_square_size / 2.0,
        y: p.fixed_thick_y + corner_square_size / 2.0,
    };
    // Skutečná velikost díry — nikdy nesmí přesáhnout výztužný čtverec (ten je
    // navržen tak, aby se nikdy nedotkl flex L), proto se ořízne na corner_square_size.
    let effective_magnet_size = p.magnet_size.min(corner_square_size).max(0.0);

    // === PRUŽINY X (horní zóna: mezi dolním okrajem pevného L a horním okrajem flex L) ===
    let x_springs = if p.spring_count_x <= 0 {
        Vec::new()
    } else {
        let zone_h = hole_y - p.fixed_thick_y;
        let avail_w = p.glass_w - p.flex_gap - p.flex_thick;
        if zone_h < 0.5 || avail_w < 1.0 {
            Vec::new()
        } else {
            let n  = p.spring_count_x;
            let sw = p.spring_width.min(avail_w / (n as f64 + 0.5));
            let gap = (avail_w - n as f64 * sw) / (n as f64 + 1.0);
            (0..n)
                .map(|i| {
                    let sx = hole_x + p.flex_gap + gap * (i as f64 + 1.0) + sw * i as f64;
                    let sy = p.fixed_thick_y;
                    SpringGeom {
                        x: sx, y: sy, w: sw, h: zone_h,
                        path: spring_path(sx, sy, sw, zone_h, p.spring_bends, p.spring_gap_mod, SpringAxis::X),
                    }
                })
                .collect()
        }
    };

    // === PRUŽINY Y (pravá zóna: mezi vnitřní hranou pevného L a pravým okrajem flex L) ===
    let y_springs = if p.spring_count_y <= 0 {
        Vec::new()
    } else {
        let zone_w = frame_x - p.fixed_thick_x;
        let y_start = hole_y + p.flex_thick + FLEX_SPLIT_GAP;
        let y_end   = hole_y2 - p.flex_gap;
        let avail_h = y_end - y_start;
        if zone_w < 0.5 || avail_h < 1.0 {
            Vec::new()
        } else {
            let n  = p.spring_count_y;
            let sh = p.spring_width.min(avail_h / (n as f64 + 0.5));
            let gap = (avail_h - n as f64 * sh) / (n as f64 + 1.0);
            (0..n)
                .map(|i| {
                    let sx = hole_x2;
                    let sy = y_start + gap * (i as f64 + 1.0) + sh * i as f64;
                    SpringGeom {
                        x: sx, y: sy, w: zone_w, h: sh,
                        path: spring_path(sx, sy, zone_w, sh, p.spring_bends, p.spring_gap_mod, SpringAxis::Y),
                    }
                })
                .collect()
        }
    };

    // === MULTIPLIKACE ===
    // Skla jsou rozložena stejným algoritmem jako canvas/tisk (get_layout_positions),
    // ale plnění sloupců se po prvním střídá (viz grid_layout_positions).
    let multiply_positions = grid_layout_positions(p.multiply_count, p.glass_w, p.glass_h, p.spacing, &p.bed);
    // Posun každé kopie tak, aby se otvor pro sklo (hole_x, hole_y) překryl s pozicí substrátu.
    let copy_offsets: Vec<CopyOffset> = multiply_positions
        .iter()
        .map(|pos| CopyOffset { tx: pos.x - hole_x, ty: pos.y - hole_y })
        .collect();

    // Středy rohových výsečí (děr) v globálních souřadnicích sestavy —
    // levý dolní roh otvoru pro sklo každé kopie. Výseče se kreslí jako poslední
    // vrstva NAD vším, aby díra pronikla i tam, kde se otvory sousedních kopií
    // nebo okraje pevných stěn překrývají.
    let corner_hole_centers: Vec<PointF> = if p.corner_r > 0.0 {
        copy_offsets.iter().map(|o| PointF { x: o.tx + hole_x, y: o.ty + hole_y2 }).collect()
    } else {
        Vec::new()
    };

    // === SESTAVA A SPOLEČNÉ STĚNY ===
    // Levá/spodní stěna se kreslí jen jednou pro celou sestavu — podél levého
    // okraje prvního sloupce, resp. spodního okraje posledního řádku.
    let core_min_x = copy_offsets.iter().map(|o| o.tx).fold(f64::INFINITY,     f64::min);
    let core_min_y = copy_offsets.iter().map(|o| o.ty).fold(f64::INFINITY,     f64::min);
    let core_max_x = copy_offsets.iter().map(|o| o.tx + b_w).fold(f64::NEG_INFINITY, f64::max);
    let core_max_y = copy_offsets.iter().map(|o| o.ty + b_h).fold(f64::NEG_INFINITY, f64::max);

    let wall_extend = if p.extend_walls { p.extend_amount } else { 0.0 };

    let assembly_min_x = core_min_x - p.left_border_w - wall_extend;
    let assembly_min_y = core_min_y;
    let assembly_max_x = core_max_x;
    let assembly_max_y = core_max_y + p.bottom_border_h + wall_extend;
    let assembly_w = assembly_max_x - assembly_min_x;
    let assembly_h = assembly_max_y - assembly_min_y;

    // Levá stěna sahá až pod spodní stěnu (a naopak) — zajistí spojení v rohu.
    let left_wall_rect   = RectF { x: assembly_min_x, y: assembly_min_y, w: p.left_border_w + wall_extend, h: assembly_h };
    let bottom_wall_rect = RectF { x: assembly_min_x, y: core_max_y,     w: assembly_w, h: p.bottom_border_h + wall_extend };

    // Díry pro magnet ve stěnách sestavy navazují na grid děr v rohových
    // čtvercích pevného L — přímo pod nimi (spodní stěna, stejné X) a přímo
    // vedle nich (levá stěna, stejné Y). Duplicity (shodné X/Y) se sloučí.
    // Roh L (průsečík středních os obou stěn) se přidá jako poslední.
    let wall_magnet_centers: Vec<PointF> = if effective_magnet_size > 0.0 {
        let mut xs: Vec<f64> = copy_offsets.iter().map(|o| o.tx + magnet_center.x).collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        xs.dedup_by(|a, b| (*a - *b).abs() < 1e-9);

        let mut ys: Vec<f64> = copy_offsets.iter().map(|o| o.ty + magnet_center.y).collect();
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        ys.dedup_by(|a, b| (*a - *b).abs() < 1e-9);

        let left_cx   = left_wall_rect.x   + left_wall_rect.w   / 2.0;
        let bottom_cy = bottom_wall_rect.y  + bottom_wall_rect.h / 2.0;

        let mut centers: Vec<PointF> = Vec::with_capacity(ys.len() + xs.len() + 1);
        for &y in &ys { centers.push(PointF { x: left_cx, y }); }
        for &x in &xs { centers.push(PointF { x, y: bottom_cy }); }
        // Průsečík středních os stěn — rohový magnet v L-spoji.
        centers.push(PointF { x: left_cx, y: bottom_cy });
        centers
    } else {
        Vec::new()
    };

    // Maximální kapacita mřížky — Rust ji spočítá jednou, Svelte ji nemusí
    // duplikovat s odlišnou sémantikou.
    let max_multiply = grid_max_capacity(p.glass_w, p.glass_h, p.spacing, &p.bed);

    BracketGeometry {
        b_w, b_h, hole_x, hole_y, hole_x2, hole_y2,
        flex_l_path, fixed_l_path, corner_square_size, magnet_center, effective_magnet_size,
        x_springs, y_springs,
        multiply_positions, copy_offsets, corner_hole_centers,
        left_wall_rect, bottom_wall_rect, wall_extend, wall_magnet_centers,
        assembly_min_x, assembly_min_y, assembly_max_x, assembly_max_y, assembly_w, assembly_h,
        max_multiply,
    }
}

// ─── Export SVG ───────────────────────────────────────────────────────────────

/// Zapíše SVG prvky jedné kopie (core) do `out`. Používá `write!` do stringu
/// místo alokujících `format!` volání v closure.
fn write_bracket_group(out: &mut String, g: &BracketGeometry, p: &BracketParams, id_prefix: &str) {
    let _ = writeln!(out, "    <path d=\"{}\" fill=\"#3b82f6\"/>", g.fixed_l_path);
    let magnet_line = magnet_cutout_markup(g.magnet_center.x, g.magnet_center.y, "white", &p.magnet_shape, g.effective_magnet_size);
    if !magnet_line.is_empty() {
        let _ = writeln!(out, "    {}", magnet_line);
    }
    for (i, s) in g.x_springs.iter().enumerate() {
        let _ = writeln!(out,
            "    <clipPath id=\"{id_prefix}-xspring-clip-{i}\"><rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\"/></clipPath>",
            s.x, s.y, s.w, s.h);
        let _ = writeln!(out,
            "    <rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"#1e40af\"/>",
            s.x, s.y, s.w, s.h);
        let _ = writeln!(out,
            "    <path d=\"{}\" fill=\"white\" clip-path=\"url(#{id_prefix}-xspring-clip-{i})\"/>",
            s.path);
    }
    for (i, s) in g.y_springs.iter().enumerate() {
        let _ = writeln!(out,
            "    <clipPath id=\"{id_prefix}-yspring-clip-{i}\"><rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\"/></clipPath>",
            s.x, s.y, s.w, s.h);
        let _ = writeln!(out,
            "    <rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"#1e40af\"/>",
            s.x, s.y, s.w, s.h);
        let _ = writeln!(out,
            "    <path d=\"{}\" fill=\"white\" clip-path=\"url(#{id_prefix}-yspring-clip-{i})\"/>",
            s.path);
    }
    let _ = writeln!(out, "    <path d=\"{}\" fill=\"#3b82f6\"/>", g.flex_l_path);
}

/// Vygeneruje kompletní SVG export sestavy (stejná geometrie jako náhled).
pub fn generate_bracket_svg(p: &BracketParams) -> String {
    let g  = compute_bracket_geometry(p);
    let ox = g.assembly_min_x;
    let oy = g.assembly_min_y;

    // Odhadovaná velikost výstupu — snižuje počet realokací.
    let capacity = 512
        + g.copy_offsets.len() * (256 + (g.x_springs.len() + g.y_springs.len()) * 256)
        + g.wall_magnet_centers.len() * 64
        + g.corner_hole_centers.len() * 64;
    let mut out = String::with_capacity(capacity);

    let _ = writeln!(out, r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    if p.multiply_count > 1 {
        let _ = writeln!(out,
            "<!-- DPI Bracket: {}, {:.1}×{:.1} mm, {}× vzorek (mezera {:.1} mm) -->",
            p.glass_label, g.assembly_w, g.assembly_h, p.multiply_count, p.spacing);
    } else {
        let _ = writeln!(out,
            "<!-- DPI Bracket: {}, {:.1}×{:.1} mm -->",
            p.glass_label, g.assembly_w, g.assembly_h);
    }
    let _ = writeln!(out,
        r#"<svg width="{:.3}mm" height="{:.3}mm" viewBox="0 0 {:.3} {:.3}" xmlns="http://www.w3.org/2000/svg">"#,
        g.assembly_w, g.assembly_h, g.assembly_w, g.assembly_h);

    let _ = writeln!(out,
        "  <rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"#3b82f6\"/>",
        g.left_wall_rect.x - ox, g.left_wall_rect.y - oy, g.left_wall_rect.w, g.left_wall_rect.h);
    let _ = writeln!(out,
        "  <rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"#3b82f6\"/>",
        g.bottom_wall_rect.x - ox, g.bottom_wall_rect.y - oy, g.bottom_wall_rect.w, g.bottom_wall_rect.h);

    for c in &g.wall_magnet_centers {
        let m = magnet_cutout_markup(c.x - ox, c.y - oy, "white", &p.magnet_shape, g.effective_magnet_size);
        if !m.is_empty() {
            let _ = writeln!(out, "  {}", m);
        }
    }

    for (idx, o) in g.copy_offsets.iter().enumerate() {
        let _ = writeln!(out, r#"  <g transform="translate({:.3},{:.3})">"#, o.tx - ox, o.ty - oy);
        write_bracket_group(&mut out, &g, p, &format!("b{idx}"));
        out.push_str("  </g>\n");
    }

    // Rohové výseče — poslední vrstva NAD vším, aby udělaly díru i tam, kudy
    // procházejí přes okraj/zónu sousední kopie či stěny.
    for c in &g.corner_hole_centers {
        let _ = writeln!(out,
            r#"  <circle cx="{:.3}" cy="{:.3}" r="{:.3}" fill="white"/>"#,
            c.x - ox, c.y - oy, p.corner_r);
    }

    out.push_str("</svg>");
    out
}

// ─── Export STL: rasterizace → greedy meshing → extruze → binární encoding ───
//
// 2D průřez sestavy (vyrastrovaný frontend pomocí canvas — viz drawCrossSectionForSTL
// v BracketExportModal.svelte, protože vyžaduje Path2D pro oblé řezy pružin) se
// předá sem jako bitmapová maska. Vše ostatní — sloučení sousedních plných buněk
// do obdélníků, vytlačení do kvádrů a binární STL encoding — proběhne v Rustu.

#[derive(Debug, Clone, Copy)]
struct MeshRect {
    x0: f64, y0: f64, x1: f64, y1: f64,
}

/// Sloučí sousední plné buňky bitmapy do co největších obdélníků (v mm).
/// Greedy scan-line algoritmus: pro každou nepokrytou plnou buňku rozšíří
/// obdélník doprava co nejdál, pak dolů co nejdál.
fn greedy_mesh_rects(mask: &[u8], cols: usize, rows: usize, cell_size: f64, origin_x: f64, origin_y: f64) -> Vec<MeshRect> {
    // Vec<bool> je kompaktnější než Vec<u8> — 8× méně paměti pro bitmapu.
    let mut visited = vec![false; cols * rows];
    let mut rects   = Vec::new();

    for r in 0..rows {
        for c in 0..cols {
            let idx = r * cols + c;
            if mask[idx] != 1 || visited[idx] { continue; }

            // Rozšíř doprava.
            let mut w = 1usize;
            while c + w < cols && mask[idx + w] == 1 && !visited[idx + w] { w += 1; }

            // Rozšíř dolů.
            let mut h = 1usize;
            'row_scan: while r + h < rows {
                let base = (r + h) * cols + c;
                for k in 0..w {
                    if mask[base + k] != 1 || visited[base + k] { break 'row_scan; }
                }
                h += 1;
            }

            // Označ pokryté buňky.
            for rr in 0..h {
                let base = (r + rr) * cols + c;
                visited[base..base + w].fill(true);
            }

            rects.push(MeshRect {
                x0: origin_x + c as f64         * cell_size,
                y0: origin_y + r as f64         * cell_size,
                x1: origin_x + (c + w) as f64   * cell_size,
                y1: origin_y + (r + h) as f64   * cell_size,
            });
        }
    }
    rects
}

const FLOATS_PER_TRIANGLE: usize = 12;

#[inline]
fn push_quad(out: &mut Vec<f32>, p0: [f32; 3], p1: [f32; 3], p2: [f32; 3], p3: [f32; 3], n: [f32; 3]) {
    out.extend_from_slice(&[n[0], n[1], n[2], p0[0], p0[1], p0[2], p1[0], p1[1], p1[2], p2[0], p2[1], p2[2]]);
    out.extend_from_slice(&[n[0], n[1], n[2], p0[0], p0[1], p0[2], p2[0], p2[1], p2[2], p3[0], p3[1], p3[2]]);
}

/// Vytlačí (extruduje) sloučené obdélníky do kvádrů mezi z0 a z1 a zapíše
/// trojúhelníková data přímo do `out` — nevytváří mezilehlý Vec.
fn extrude_rects_append(out: &mut Vec<f32>, rects: &[MeshRect], z0: f32, z1: f32) {
    out.reserve(rects.len() * 6 * 2 * FLOATS_PER_TRIANGLE);
    for rc in rects {
        let (x0, y0, x1, y1) = (rc.x0 as f32, rc.y0 as f32, rc.x1 as f32, rc.y1 as f32);
        push_quad(out, [x0,y0,z1], [x1,y0,z1], [x1,y1,z1], [x0,y1,z1], [ 0.0, 0.0, 1.0]); // +z
        push_quad(out, [x0,y0,z0], [x0,y1,z0], [x1,y1,z0], [x1,y0,z0], [ 0.0, 0.0,-1.0]); // -z
        push_quad(out, [x0,y0,z0], [x0,y0,z1], [x0,y1,z1], [x0,y1,z0], [-1.0, 0.0, 0.0]); // -x
        push_quad(out, [x1,y0,z0], [x1,y1,z0], [x1,y1,z1], [x1,y0,z1], [ 1.0, 0.0, 0.0]); // +x
        push_quad(out, [x0,y0,z0], [x1,y0,z0], [x1,y0,z1], [x0,y0,z1], [ 0.0,-1.0, 0.0]); // -y
        push_quad(out, [x0,y1,z0], [x0,y1,z1], [x1,y1,z1], [x1,y1,z0], [ 0.0, 1.0, 0.0]); // +y
    }
}

/// Zakóduje ploché trojúhelníkové pole do binárního STL souboru.
/// Na little-endian platformách (x86, ARM, WASM) se float data zkopírují
/// přímo jako raw bytes bez float-po-floatu — O(n) místo O(n×12).
fn encode_binary_stl(triangle_data: &[f32], name: &str) -> Vec<u8> {
    const HEADER: usize = 80;
    const ATTR:   usize = 2;  // attribute byte count per triangle
    let tri_count = triangle_data.len() / FLOATS_PER_TRIANGLE;
    let mut buf = vec![0u8; HEADER + 4 + tri_count * (FLOATS_PER_TRIANGLE * 4 + ATTR)];

    // Záhlaví (max 80 bajtů ASCII).
    let name_bytes = name.as_bytes();
    buf[..name_bytes.len().min(HEADER)].copy_from_slice(&name_bytes[..name_bytes.len().min(HEADER)]);
    buf[HEADER..HEADER + 4].copy_from_slice(&(tri_count as u32).to_le_bytes());

    let mut offset = HEADER + 4;

    #[cfg(target_endian = "little")]
    {
        // SAFETY: f32 nemá žádné neplatné bitové vzory; na LE platformě je
        // paměťová reprezentace totožná s little-endian byte pořadím.
        // Každý trojúhelník: 12 floatů (48 B) + 2 B attribute → 50 B celkem.
        // Přeskakujeme attribute byty (nulové z vec![0u8; ...]) a kopírujeme
        // vždy 12 floatů najednou jako raw slice.
        let float_bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(triangle_data.as_ptr() as *const u8, triangle_data.len() * 4)
        };
        for t in 0..tri_count {
            let src = &float_bytes[t * FLOATS_PER_TRIANGLE * 4..(t + 1) * FLOATS_PER_TRIANGLE * 4];
            buf[offset..offset + FLOATS_PER_TRIANGLE * 4].copy_from_slice(src);
            offset += FLOATS_PER_TRIANGLE * 4 + ATTR; // attribute = 0 (předvyplněno)
        }
    }

    #[cfg(not(target_endian = "little"))]
    {
        // Big-endian fallback — konvertuj float po floatu.
        for t in 0..tri_count {
            let base = t * FLOATS_PER_TRIANGLE;
            for i in 0..FLOATS_PER_TRIANGLE {
                buf[offset..offset + 4].copy_from_slice(&triangle_data[base + i].to_le_bytes());
                offset += 4;
            }
            offset += ATTR;
        }
    }

    buf
}

/// Pouze ta část stěn, o kterou byly rozšířeny (extendWalls/extendAmount) — jen
/// na ní se staví dodatečný blok wallExtraHeight, takže "zarážka" vznikne jen
/// v přesahu, ne na celé ploše základních pevných stěn.
#[inline]
fn is_in_wall_extension_region(x: f64, y: f64, left_wall: &RectF, bottom_wall: &RectF, wall_extend: f64) -> bool {
    if wall_extend <= 0.0 { return false; }
    let in_left   = x >= left_wall.x   && x < left_wall.x   + wall_extend
                 && y >= left_wall.y   && y < left_wall.y   + left_wall.h;
    let in_bottom = y >= bottom_wall.y + bottom_wall.h - wall_extend && y < bottom_wall.y + bottom_wall.h
                 && x >= bottom_wall.x && x < bottom_wall.x + bottom_wall.w;
    in_left || in_bottom
}

/// Sestaví kompletní binární STL z vyrastrované masky průřezu — odvodí masku
/// rozšířené části stěn, provede greedy meshing, extruzi základního tělesa
/// (0..bracket_thickness) a volitelně rozšířené části stěn
/// (bracket_thickness..bracket_thickness+wall_extra_height) a binární encoding.
#[allow(clippy::too_many_arguments)]
pub fn build_bracket_stl(
    mask: &[u8],
    cols: usize,
    rows: usize,
    cell_size: f64,
    origin_x: f64,
    origin_y: f64,
    left_wall_rect: RectF,
    bottom_wall_rect: RectF,
    wall_extend: f64,
    bracket_thickness: f64,
    wall_extra_height: f64,
) -> Vec<u8> {
    let body_rects = greedy_mesh_rects(mask, cols, rows, cell_size, origin_x, origin_y);

    // Trojúhelníkový buffer — body + stěny zapíšeme rovnou do jednoho Vec.
    let mut all_tris: Vec<f32> = Vec::new();
    extrude_rects_append(&mut all_tris, &body_rects, 0.0, bracket_thickness as f32);

    if wall_extra_height > 0.0 && wall_extend > 0.0 {
        let mut wall_mask = vec![0u8; cols * rows];
        for r in 0..rows {
            for c in 0..cols {
                let idx = r * cols + c;
                if mask[idx] == 0 { continue; }
                let mm_x = origin_x + (c as f64 + 0.5) * cell_size;
                let mm_y = origin_y + (r as f64 + 0.5) * cell_size;
                if is_in_wall_extension_region(mm_x, mm_y, &left_wall_rect, &bottom_wall_rect, wall_extend) {
                    wall_mask[idx] = 1;
                }
            }
        }
        let wall_rects = greedy_mesh_rects(&wall_mask, cols, rows, cell_size, origin_x, origin_y);
        extrude_rects_append(&mut all_tris, &wall_rects, bracket_thickness as f32, (bracket_thickness + wall_extra_height) as f32);
    }

    encode_binary_stl(&all_tris, "DPI Bracket")
}
