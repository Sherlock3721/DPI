use crate::types::{LayoutPosition, SubstratePaths, Transform};

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
/// Pro každé sklíčko zachová relativní offset v rámci sklíčka (pokud se geometrie vejde),
/// jinak resetuje pozici na střed sklíčka (zachová rotaci a měřítko).
///
/// Odpovídá smyčce `doUpdateLayout` v `src/stores/projectStore.ts` (řádky 491–572).
///
/// # Parametry
/// - `new_positions` — nové pozice sklíček (výstup `get_layout_positions`)
/// - `old_non_prime_positions` — předchozí pozice (bez prime) pro výpočet relativního offsetu
/// - `current_transforms` — aktuální transformace pro každé vzorkové sklíčko
/// - `current_paths` — zpracované dráhy (výstup `process_substrate_paths`) pro každé sklíčko
/// - `nozzle_diam` — průměr trysky v mm (použit jako inset při kontrole přesahu)
pub fn fit_transforms_to_layout(
    new_positions: &[LayoutPosition],
    old_non_prime_positions: &[LayoutPosition],
    current_transforms: &[Transform],
    current_paths: &[SubstratePaths],
    nozzle_diam: f64,
) -> Vec<Transform> {
    let r = nozzle_diam / 2.0;
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

                // Zkontrolujeme, zda se geometrie vejde do nového sklíčka s insetem trysky
                let fits = match current_paths.get(sample_idx) {
                    Some(paths) => match bbox_of_paths(paths) {
                        Some((mn_x, mx_x, mn_y, mx_y)) => {
                            let (wmin_x, wmax_x, wmin_y, wmax_y) =
                                compute_world_aabb(&candidate, mn_x, mx_x, mn_y, mx_y);
                            wmin_x >= pos.x + r
                                && wmax_x <= pos.x + pos.width - r
                                && wmin_y >= pos.y + r
                                && wmax_y <= pos.y + pos.height - r
                        }
                        None => true, // prázdné dráhy — vejdou se vždy
                    },
                    None => true, // žádné dráhy pro toto sklíčko
                };

                if fits {
                    candidate
                } else {
                    // Geometrie přesahuje — resetujeme pozici na střed, zachováme škálování/rotaci
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
