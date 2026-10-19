use crate::types::{
    LayoutPosition, Placement, PreviewDistResult, PreviewSegData, SubstratePaths, Transform,
};

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

/// Přizpůsobí jednu transformaci nové pozici sklíčka.
///
/// Zachová relativní offset v rámci sklíčka (pokud se geometrie vejde), jinak
/// resetuje pozici na střed sklíčka a ponechá rotaci i měřítko. Bez předchozího
/// stavu (`old_t`/`old_pos` = `None`) vrátí výchozí transformaci na střed.
///
/// # Parametry
/// - `pos` — nová pozice sklíčka
/// - `old_pos` — předchozí pozice téhož sklíčka (pro výpočet relativního offsetu)
/// - `old_t` — předchozí transformace umístění
/// - `path` — zpracovaná dráha umístění (kontrola přesahu)
/// - `nozzle_diam` — průměr trysky v mm (inset při kontrole přesahu)
pub fn fit_transform_to_position(
    pos: &LayoutPosition,
    old_pos: Option<&LayoutPosition>,
    old_t: Option<&Transform>,
    path: Option<&SubstratePaths>,
    nozzle_diam: f64,
) -> Transform {
    let r = nozzle_diam / 2.0;
    let centered = |scale: f64, rotation: f64| Transform {
        cx: pos.width / 2.0,
        cy: pos.height / 2.0,
        gui_dx: pos.x,
        gui_dy: pos.y,
        scale,
        rotation,
    };

    let (old_t, old_pos) = match (old_t, old_pos) {
        (Some(t), Some(p)) => (t, p),
        // Nové sklíčko bez předchozí transformace — výchozí hodnoty
        _ => return centered(1.0, 0.0),
    };

    // Zachováme relativní offset uvnitř sklíčka
    let candidate = Transform {
        cx: pos.width / 2.0,
        cy: pos.height / 2.0,
        gui_dx: pos.x + (old_t.gui_dx - old_pos.x),
        gui_dy: pos.y + (old_t.gui_dy - old_pos.y),
        scale: old_t.scale,
        rotation: old_t.rotation,
    };

    // Zkontrolujeme, zda se geometrie vejde do nového sklíčka s insetem trysky
    let fits = match path.and_then(bbox_of_paths) {
        Some((mn_x, mx_x, mn_y, mx_y)) => {
            let (wmin_x, wmax_x, wmin_y, wmax_y) =
                compute_world_aabb(&candidate, mn_x, mx_x, mn_y, mx_y);
            wmin_x >= pos.x + r
                && wmax_x <= pos.x + pos.width - r
                && wmin_y >= pos.y + r
                && wmax_y <= pos.y + pos.height - r
        }
        // prázdné dráhy nebo žádné dráhy — vejdou se vždy
        None => true,
    };

    if fits {
        candidate
    } else {
        // Geometrie přesahuje — reset pozice na střed, škálování/rotace zůstávají
        centered(old_t.scale, old_t.rotation)
    }
}

// ─── Hledání volného místa na sklíčku ────────────────────────────────────────

/// Najde posun `(dx, dy)`, po kterém nově přidaná skupina objektů s world AABB
/// `new_aabb` nepřekrývá už umístěné objekty (`occupied`) a vejde se do sklíčka
/// s insetem `nozzle_diam / 2`.
///
/// First-fit po mřížce zleva dolů → doprava nahoru, tedy ve stejném pořadí,
/// v jakém skládá sklíčka `get_layout_positions`. `None` = volné místo není
/// (volající nechá objekt vycentrovaný a spoléhá na ruční posun).
pub fn find_free_offset(
    pos: &LayoutPosition,
    new_aabb: (f64, f64, f64, f64),
    occupied: &[(f64, f64, f64, f64)],
    nozzle_diam: f64,
) -> Option<(f64, f64)> {
    let r = nozzle_diam / 2.0;
    let (mn_x, mx_x, mn_y, mx_y) = new_aabb;
    let w = mx_x - mn_x;
    let h = mx_y - mn_y;

    let avail_w = pos.width - 2.0 * r;
    let avail_h = pos.height - 2.0 * r;
    if w > avail_w || h > avail_h {
        return None;
    }

    // Krok mřížky: dost jemný na pohodlné skládání, dost hrubý na to, aby
    // prohledání zůstalo v řádu tisíců kandidátů i na velkém substrátu.
    let step = 0.5_f64;
    let free = |dx: f64, dy: f64| -> bool {
        let (ax1, ay1) = (mn_x + dx, mn_y + dy);
        let (ax2, ay2) = (mx_x + dx, mx_y + dy);
        !occupied.iter().any(|&(bx1, bx2, by1, by2)| {
            ax1 < bx2 && ax2 > bx1 && ay1 < by2 && ay2 > by1
        })
    };

    let x_steps = ((avail_w - w) / step).floor().max(0.0) as usize;
    let y_steps = ((avail_h - h) / step).floor().max(0.0) as usize;
    for ix in 0..=x_steps {
        for iy in 0..=y_steps {
            let target_x = pos.x + r + ix as f64 * step;
            let target_y = pos.y + r + iy as f64 * step;
            let (dx, dy) = (target_x - mn_x, target_y - mn_y);
            if free(dx, dy) {
                return Some((dx, dy));
            }
        }
    }
    None
}

// ─── Předpočítání vzdáleností pro náhled tisku ───────────────────────────────

/// Předpočítá kumulativní vzdálenosti segmentů pro náhled průběhu tisku.
///
/// Odpovídá funkci `recomputePreviewDist` v `Canvas2D.svelte`.
/// Výsledek se používá v JS k nalezení přesné pozice náhledové trysky podle `printProgress`.
///
/// Pořadí určuje `schedule::schedule_print`, sdílené s G-kód generátorem
/// (`gcode::generate_gcode`) — nelze se rozejít.
pub fn compute_preview_segments(
    positions: &[LayoutPosition],
    slides: &[Vec<Placement>],
    prime_path: Option<&SubstratePaths>,
    global_nozzle: &str,
    print_order: &str,
    route_mode: &str,
) -> PreviewDistResult {
    let mut segs: Vec<PreviewSegData> = Vec::new();
    let mut cum_dist = 0.0_f64;

    // Odpliv je vždy positions[0] (layout::get_layout_positions) a tiskne se
    // jako první, mimo naplánované pořadí.
    let prime_active = positions.first().map(|p| p.is_prime).unwrap_or(false);
    if prime_active {
        if let Some(p) = prime_path {
            push_preview_segs(&mut segs, &mut cum_dist, 0, 0, p, 1.0);
        }
    }
    let pos_offset = if prime_active { 1 } else { 0 };

    let steps = crate::schedule::schedule_print(
        slides,
        global_nozzle,
        crate::schedule::PrintOrder::from_setting(print_order),
        crate::schedule::RouteMode::from_setting(route_mode),
    );
    for step in &steps {
        if let Some(pl) = slides
            .get(step.measurement_idx)
            .and_then(|row| row.get(step.obj_idx))
        {
            push_preview_segs(
                &mut segs,
                &mut cum_dist,
                step.measurement_idx + pos_offset,
                step.obj_idx,
                &pl.path,
                pl.transform.scale,
            );
        }
    }

    PreviewDistResult { segs, total_dist: cum_dist }
}

fn push_preview_segs(
    segs: &mut Vec<PreviewSegData>,
    cum_dist: &mut f64,
    slide_idx: usize,
    obj_idx: usize,
    path: &SubstratePaths,
    scale: f64,
) {
    for (seg_idx, seg) in path.segments.iter().enumerate() {
        let pts = &seg.points;
        if pts.is_empty() {
            continue;
        }
        let mut point_dists = Vec::with_capacity(pts.len());
        point_dists.push(0.0_f64);
        let mut seg_dist = 0.0_f64;
        for j in 1..pts.len() {
            let dx = (pts[j].x - pts[j - 1].x) * scale;
            let dy = (pts[j].y - pts[j - 1].y) * scale;
            seg_dist += (dx * dx + dy * dy).sqrt();
            point_dists.push(seg_dist);
        }
        segs.push(PreviewSegData {
            slide_idx,
            obj_idx,
            seg_idx,
            path_start_dist: *cum_dist,
            point_dists,
            seg_dist,
        });
        *cum_dist += seg_dist.max(0.001);
    }
}

// ─── Kontrola přesahu trasy ────────────────────────────────────────────────────

/// Vrátí `true` pokud některá z transformovaných tras přesahuje svou pozici sklíčka
/// s insetem `nozzle_diam / 2`.
///
/// Nahrazuje smyčky v `handleNozzleDiamGrew` v `App.svelte`.
/// `slides` a `non_prime_positions` musí být seřazeny shodně — index 0 odpovídá prvnímu vzorkovému sklíčku.
/// Objekty s vlastní tryskou se posuzují podle ní, ne podle `nozzle_diam`.
pub fn check_paths_overflow(
    slides: &[Vec<Placement>],
    non_prime_positions: &[LayoutPosition],
    nozzle_diam: f64,
) -> bool {
    for (row, pos) in slides.iter().zip(non_prime_positions.iter()) {
        for pl in row.iter().filter(|p| p.enabled) {
            let r = pl.effective_nozzle_diam(nozzle_diam) / 2.0;
            if let Some((mn_x, mx_x, mn_y, mx_y)) = bbox_of_paths(&pl.path) {
                let (wmin_x, wmax_x, wmin_y, wmax_y) =
                    compute_world_aabb(&pl.transform, mn_x, mx_x, mn_y, mx_y);
                if wmin_x < pos.x + r
                    || wmax_x > pos.x + pos.width - r
                    || wmin_y < pos.y + r
                    || wmax_y > pos.y + pos.height - r
                {
                    return true;
                }
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

        let result =
            fit_transform_to_position(&new_pos, Some(&old_pos), Some(&old_t), Some(&paths), 0.4);

        // Relativní offset se zachová: new gui_dx = 80 + 5 = 85
        assert!((result.gui_dx - 85.0).abs() < 1e-6, "gui_dx={}", result.gui_dx);
        assert!((result.gui_dy - 2.0).abs() < 1e-6, "gui_dy={}", result.gui_dy);
    }

    #[test]
    fn test_fit_transforms_resets_when_overflow() {
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
            width: 20.0, // užší sklíčko — geometrie se nevejde
            height: 26.0,
            is_prime: false,
        };
        let old_t = Transform {
            gui_dx: 0.0,
            gui_dy: 0.0,
            cx: 38.0,
            cy: 13.0,
            scale: 1.0,
            rotation: 0.0,
        };
        // Geometrie šířky 60mm — nevejde se do sklíčka 20mm
        let paths = make_paths(&[(5.0, 5.0), (65.0, 20.0)]);

        let result =
            fit_transform_to_position(&new_pos, Some(&old_pos), Some(&old_t), Some(&paths), 0.4);

        // Reset na střed nové pozice
        assert!((result.gui_dx - 80.0).abs() < 1e-6, "gui_dx={}", result.gui_dx);
        assert!((result.gui_dy - 0.0).abs() < 1e-6, "gui_dy={}", result.gui_dy);
        // Rotace a měřítko se zachovají
        assert!((result.scale - 1.0).abs() < 1e-6);
        assert!((result.rotation - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_fit_transform_without_previous_state_centers() {
        let pos = LayoutPosition {
            x: 80.0,
            y: 0.0,
            width: 76.0,
            height: 26.0,
            is_prime: false,
        };

        let result = fit_transform_to_position(&pos, None, None, None, 0.4);

        assert!((result.gui_dx - 80.0).abs() < 1e-6);
        assert!((result.gui_dy - 0.0).abs() < 1e-6);
        assert!((result.scale - 1.0).abs() < 1e-6);
        assert!((result.cx - 38.0).abs() < 1e-6);
    }

    #[test]
    fn test_find_free_offset_avoids_occupied() {
        let pos = LayoutPosition {
            x: 0.0,
            y: 0.0,
            width: 25.0,
            height: 75.0,
            is_prime: false,
        };
        // Obsazeno celé levé dolní okolí: 0..10 v X, 0..40 v Y
        let occupied = [(0.0, 10.0, 0.0, 40.0)];
        let new_aabb = (0.0, 8.0, 0.0, 8.0);

        let (dx, dy) = find_free_offset(&pos, new_aabb, &occupied, 0.4).expect("volné místo");
        let (ax1, ay1) = (new_aabb.0 + dx, new_aabb.2 + dy);
        let (ax2, ay2) = (new_aabb.1 + dx, new_aabb.3 + dy);

        // Nepřekrývá obsazené
        assert!(!(ax1 < 10.0 && ax2 > 0.0 && ay1 < 40.0 && ay2 > 0.0), "překryv: {ax1}..{ax2}");
        // A vejde se do sklíčka s insetem trysky
        assert!(ax1 >= 0.2 - 1e-9 && ax2 <= 24.8 + 1e-9 && ay1 >= 0.2 - 1e-9 && ay2 <= 74.8 + 1e-9);
    }

    #[test]
    fn test_find_free_offset_none_when_too_big() {
        let pos = LayoutPosition {
            x: 0.0,
            y: 0.0,
            width: 25.0,
            height: 75.0,
            is_prime: false,
        };
        // Objekt širší než sklíčko
        assert!(find_free_offset(&pos, (0.0, 30.0, 0.0, 5.0), &[], 0.4).is_none());
    }
}
