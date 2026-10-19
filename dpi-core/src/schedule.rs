//! Plánování pořadí tisku napříč substráty, objekty a tryskami.
//!
//! Jediné místo, které rozhoduje o POŘADÍ, ve kterém se placementy tisknou.
//! Nerozhoduje o pořadí segmentů uvnitř jednoho objektu (to zůstává na
//! `path_processing::process_substrate_paths` — nearest_neighbor_order /
//! two_opt_improve / stitch_nearby_endpoints) ani o volbě vstupního vrcholu
//! uzavřeného polygonu (`gcode::rotate_closed_path_to_nearest`, řeší se až
//! s živou pozicí hlavy při emisi G-kódu).
//!
//! Sdíleno mezi `gcode::generate_gcode` (skutečná emise G-kódu) a
//! `geometry::compute_preview_segments` (náhled na plátně) — obě MUSÍ
//! procházet objekty stejně, jinak se náhled rozejde se skutečným výstupem.

use crate::gcode::transform_pt;
use crate::path_processing::segs_cross;
use crate::types::{Placement, Point2D};

/// "Pořadí tisku" nastavení.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrintOrder {
    /// Substrát po substrátu — nikdy neproloží objekty ze dvou různých substrátů.
    SubstrateMajor,
    /// Napřed vše, co jde vytisknout aktuální tryskou (napříč všemi substráty),
    /// pak přepnout trysku a pokračovat — minimalizuje počet výměn trysky.
    NozzleMajor,
}

impl PrintOrder {
    pub(crate) fn from_setting(s: &str) -> Self {
        match s {
            "substrate_major" => Self::SubstrateMajor,
            _ => Self::NozzleMajor,
        }
    }
}

/// "Generování & tisk trasy" nastavení.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RouteMode {
    /// Nejbližší soused bez omezení — smí přejíždět přes již vytištěnou linii.
    Unconstrained,
    /// Nejbližší soused, ale přejezd nikdy nesmí křížit již vytištěnou linii
    /// (pokud existuje alternativa; jinak spadne zpět na nejbližšího).
    AvoidCrossing,
}

impl RouteMode {
    /// "fastest" a "cross_allowed" jsou ZÁMĚRNĚ alias téhož algoritmu (potvrzeno
    /// zadáním) — dvě odlišné hodnoty existují jen kvůli tomu, že CustomSelect
    /// dohledává popisek podle hodnoty a sdílená hodnota by druhou položku
    /// vizuálně "opravila" na popisek té první.
    pub(crate) fn from_setting(s: &str) -> Self {
        match s {
            "fastest" | "cross_allowed" => Self::Unconstrained,
            _ => Self::AvoidCrossing,
        }
    }
}

/// Jeden krok naplánovaného tisku.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PrintStep {
    /// Index do `slides` (bez odplivu) — dřívější `measurement_idx`.
    pub measurement_idx: usize,
    /// Index objektu uvnitř `slides[measurement_idx]`.
    pub obj_idx: usize,
    /// Tryska potřebná pro tento krok (override objektu, nebo globální).
    pub nozzle: String,
}

struct Candidate {
    measurement_idx: usize,
    obj_idx: usize,
    nozzle: String,
    /// První bod prvního neprázdného segmentu, absolutní souřadnice.
    entry: Point2D,
    /// Poslední bod posledního neprázdného segmentu, absolutní souřadnice.
    exit: Point2D,
    /// Všechny po sobě jdoucí dvojice bodů (absolutní souřadnice) — pro test křížení.
    segs_abs: Vec<(Point2D, Point2D)>,
    /// (min_x, min_y, max_x, max_y) — broad-phase filtr před přesným testem křížení.
    aabb: (f64, f64, f64, f64),
}

fn build_candidates(slides: &[Vec<Placement>], global_nozzle: &str) -> Vec<Candidate> {
    let mut out = Vec::new();
    for (measurement_idx, row) in slides.iter().enumerate() {
        for (obj_idx, placement) in row.iter().enumerate() {
            if !placement.enabled {
                continue;
            }
            let mut segs_abs = Vec::new();
            let mut entry: Option<Point2D> = None;
            let mut exit: Option<Point2D> = None;
            let (mut min_x, mut min_y, mut max_x, mut max_y) =
                (f64::MAX, f64::MAX, f64::MIN, f64::MIN);

            for seg in &placement.path.segments {
                if seg.points.is_empty() {
                    continue;
                }
                let abs_pts: Vec<Point2D> = seg
                    .points
                    .iter()
                    .map(|p| transform_pt(p.x, p.y, &placement.transform, 0.0))
                    .collect();
                if entry.is_none() {
                    entry = Some(abs_pts[0]);
                }
                exit = abs_pts.last().copied();
                for w in abs_pts.windows(2) {
                    segs_abs.push((w[0], w[1]));
                }
                for p in &abs_pts {
                    min_x = min_x.min(p.x);
                    min_y = min_y.min(p.y);
                    max_x = max_x.max(p.x);
                    max_y = max_y.max(p.y);
                }
            }

            let (Some(entry), Some(exit)) = (entry, exit) else {
                continue; // objekt bez geometrie (samé prázdné segmenty)
            };
            let nozzle = placement
                .nozzle_type
                .clone()
                .unwrap_or_else(|| global_nozzle.to_string());

            out.push(Candidate {
                measurement_idx,
                obj_idx,
                nozzle,
                entry,
                exit,
                segs_abs,
                aabb: (min_x, min_y, max_x, max_y),
            });
        }
    }
    out
}

/// Vrátí pořadí trysek v `candidates`: nasazená tryska první (pokud je mezi
/// nimi zastoupená), pak zbytek podle prvního výskytu v `candidates`.
fn nozzle_visit_order(candidates: &[&Candidate], mounted: &str) -> Vec<String> {
    let mut order = Vec::new();
    if candidates.iter().any(|c| c.nozzle == mounted) {
        order.push(mounted.to_string());
    }
    for c in candidates {
        if !order.contains(&c.nozzle) {
            order.push(c.nozzle.clone());
        }
    }
    order
}

fn aabb_overlap(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
    a.0 <= b.2 && b.0 <= a.2 && a.1 <= b.3 && b.1 <= a.3
}

fn crosses_printed(
    from: Point2D,
    to: Point2D,
    printed: &[(Vec<(Point2D, Point2D)>, (f64, f64, f64, f64))],
) -> bool {
    let travel_aabb = (
        from.x.min(to.x),
        from.y.min(to.y),
        from.x.max(to.x),
        from.y.max(to.y),
    );
    printed.iter().any(|(segs, aabb)| {
        aabb_overlap(travel_aabb, *aabb) && segs.iter().any(|(a, b)| segs_cross(from, to, *a, *b))
    })
}

/// Seřadí `bucket` (kandidáti se stejnou tryskou) metodou nejbližšího souseda
/// z `*cursor`. V režimu `AvoidCrossing` navíc filtruje kandidáty proti
/// `printed`, který roste napříč CELÝM voláním `schedule_print` (ne jen
/// tímto bucketem) — "již vytištěno touto nebo dřívější tryskou v této
/// zakázce". Aktualizuje `*cursor` a `printed` in-place.
fn order_bucket(
    mut remaining: Vec<Candidate>,
    cursor: &mut Point2D,
    printed: &mut Vec<(Vec<(Point2D, Point2D)>, (f64, f64, f64, f64))>,
    route_mode: RouteMode,
) -> Vec<Candidate> {
    let mut ordered = Vec::with_capacity(remaining.len());

    while !remaining.is_empty() {
        let mut by_dist: Vec<(usize, f64)> = remaining
            .iter()
            .enumerate()
            .map(|(i, c)| (i, cursor.distance(&c.entry)))
            .collect();
        by_dist.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let pick_idx = match route_mode {
            RouteMode::Unconstrained => by_dist[0].0,
            RouteMode::AvoidCrossing => by_dist
                .iter()
                .find(|(i, _)| !crosses_printed(*cursor, remaining[*i].entry, printed))
                .map(|(i, _)| *i)
                .unwrap_or(by_dist[0].0), // fallback: všichni by křížili -> vezmi nejbližšího
        };

        let picked = remaining.remove(pick_idx);
        if route_mode == RouteMode::AvoidCrossing {
            printed.push((picked.segs_abs.clone(), picked.aabb));
        }
        *cursor = picked.exit;
        ordered.push(picked);
    }

    ordered
}

fn clone_candidate(c: &Candidate) -> Candidate {
    Candidate {
        measurement_idx: c.measurement_idx,
        obj_idx: c.obj_idx,
        nozzle: c.nozzle.clone(),
        entry: c.entry,
        exit: c.exit,
        segs_abs: c.segs_abs.clone(),
        aabb: c.aabb,
    }
}

/// Seřadí a spustí jeden bucket (skupina kandidátů se stejnou tryskou):
/// pokud se tryska liší od `*mounted`, resetuje `*cursor` (zrcadlí homing po
/// výměně) a aktualizuje `*mounted`; pak zavolá `order_bucket` a výsledek
/// připojí do `result`.
#[allow(clippy::too_many_arguments)]
fn run_bucket(
    bucket: Vec<Candidate>,
    cursor: &mut Point2D,
    mounted: &mut String,
    printed: &mut Vec<(Vec<(Point2D, Point2D)>, (f64, f64, f64, f64))>,
    result: &mut Vec<PrintStep>,
    route_mode: RouteMode,
) {
    let Some(first) = bucket.first() else { return };
    if first.nozzle != *mounted {
        *cursor = Point2D::new(0.0, 0.0);
        *mounted = first.nozzle.clone();
    }
    let ordered = order_bucket(bucket, cursor, printed, route_mode);
    for c in ordered {
        result.push(PrintStep {
            measurement_idx: c.measurement_idx,
            obj_idx: c.obj_idx,
            nozzle: c.nozzle,
        });
    }
}

/// Spočítá kompletní pořadí tisku všech viditelných placementů napříč
/// substráty. Odplivový (prime) vzorek se plánuje mimo tuto funkci (tiskne
/// se vždy jako první, viz `layout::get_layout_positions`).
///
/// Rozhodnutí, kterou tryskou pokračovat (uvnitř substrátu i mezi
/// substráty), se dělá průběžně podle SKUTEČNĚ aktuálně nasazené trysky
/// (`mounted`), ne podle fixní `global_nozzle` — jinak by `SubstrateMajor`
/// u prokládaných trysek mohl vygenerovat VÍC výměn než žádné seskupování
/// vůbec (např. substrát 2 by preferoval "první" trysku job, i když má
/// zrovna nasazenou jinou z konce substrátu 1).
pub(crate) fn schedule_print(
    slides: &[Vec<Placement>],
    global_nozzle: &str,
    print_order: PrintOrder,
    route_mode: RouteMode,
) -> Vec<PrintStep> {
    let candidates = build_candidates(slides, global_nozzle);
    if candidates.is_empty() {
        return Vec::new();
    }

    let mut cursor = Point2D::new(0.0, 0.0);
    let mut mounted = global_nozzle.to_string();
    let mut printed: Vec<(Vec<(Point2D, Point2D)>, (f64, f64, f64, f64))> = Vec::new();
    let mut result = Vec::new();

    match print_order {
        PrintOrder::SubstrateMajor => {
            let max_idx = candidates.iter().map(|c| c.measurement_idx).max().unwrap_or(0);
            for m in 0..=max_idx {
                let refs: Vec<&Candidate> =
                    candidates.iter().filter(|c| c.measurement_idx == m).collect();
                if refs.is_empty() {
                    continue;
                }
                for nozzle in nozzle_visit_order(&refs, &mounted) {
                    let group: Vec<Candidate> = refs
                        .iter()
                        .filter(|c| c.nozzle == nozzle)
                        .map(|c| clone_candidate(c))
                        .collect();
                    if !group.is_empty() {
                        run_bucket(group, &mut cursor, &mut mounted, &mut printed, &mut result, route_mode);
                    }
                }
            }
        }
        PrintOrder::NozzleMajor => {
            let all: Vec<&Candidate> = candidates.iter().collect();
            for nozzle in nozzle_visit_order(&all, &mounted) {
                let group: Vec<Candidate> = all
                    .iter()
                    .filter(|c| c.nozzle == nozzle)
                    .map(|c| clone_candidate(c))
                    .collect();
                if !group.is_empty() {
                    run_bucket(group, &mut cursor, &mut mounted, &mut printed, &mut result, route_mode);
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PathSegment, SubstratePaths, Transform};

    fn identity_transform() -> Transform {
        Transform {
            scale: 1.0,
            rotation: 0.0,
            gui_dx: 0.0,
            gui_dy: 0.0,
            cx: 0.0,
            cy: 0.0,
        }
    }

    fn placement_at(x: f64, y: f64, nozzle: Option<&str>) -> Placement {
        Placement {
            object_id: format!("obj-{x}-{y}"),
            enabled: true,
            transform: identity_transform(),
            baked_scale: 1.0,
            nozzle_type: nozzle.map(|s| s.to_string()),
            nozzle_diam: None,
            path: SubstratePaths::new(vec![PathSegment::new(vec![
                Point2D::new(x, y),
                Point2D::new(x + 1.0, y),
            ])]),
        }
    }

    #[test]
    fn substrate_major_never_interleaves_substrates() {
        let slides = vec![
            vec![placement_at(0.0, 0.0, Some("A")), placement_at(0.0, 10.0, Some("B"))],
            vec![placement_at(0.0, 0.0, Some("B")), placement_at(0.0, 10.0, Some("A"))],
        ];
        let steps = schedule_print(&slides, "A", PrintOrder::SubstrateMajor, RouteMode::Unconstrained);
        assert_eq!(steps.len(), 4);
        // prvni dva kroky patri substratu 0, dalsi dva substratu 1 -- nikdy prolozene
        assert_eq!(steps[0].measurement_idx, 0);
        assert_eq!(steps[1].measurement_idx, 0);
        assert_eq!(steps[2].measurement_idx, 1);
        assert_eq!(steps[3].measurement_idx, 1);
        // uvnitr substratu je tryska seskupena (nasazena "A" je prvni)
        assert_eq!(steps[0].nozzle, "A");
        assert_eq!(steps[1].nozzle, "B");
    }

    fn count_nozzle_changes(steps: &[PrintStep], global_nozzle: &str) -> usize {
        let mut mounted = global_nozzle.to_string();
        let mut changes = 0;
        for s in steps {
            if s.nozzle != mounted {
                changes += 1;
                mounted = s.nozzle.clone();
            }
        }
        changes
    }

    #[test]
    fn substrate_major_grouping_never_exceeds_naive_fixed_order_changes() {
        // Regrese: seskupovani podle trysky UVNITR substratu smi pouzivat jen
        // AKTUALNE nasazenou trysku (ne staticky global_nozzle) - jinak muze
        // u prokladanych trysek napric substraty vyjit VIC vymen nez pri
        // zadnem seskupovani vubec (substrat 2 by preferoval "prvni" trysku
        // jobu, i kdyz ma zrovna nasazenou jinou z konce substratu 1).
        let slides = vec![
            vec![placement_at(0.0, 2.0, None), placement_at(0.0, 20.0, Some("B"))],
            vec![placement_at(0.0, 2.0, Some("B")), placement_at(0.0, 20.0, None)],
        ];
        let naive_changes = count_nozzle_changes(
            &[
                PrintStep { measurement_idx: 0, obj_idx: 0, nozzle: "A".into() },
                PrintStep { measurement_idx: 0, obj_idx: 1, nozzle: "B".into() },
                PrintStep { measurement_idx: 1, obj_idx: 0, nozzle: "B".into() },
                PrintStep { measurement_idx: 1, obj_idx: 1, nozzle: "A".into() },
            ],
            "A",
        );
        let steps = schedule_print(&slides, "A", PrintOrder::SubstrateMajor, RouteMode::Unconstrained);
        assert_eq!(count_nozzle_changes(&steps, "A"), naive_changes);
    }

    #[test]
    fn nozzle_major_minimizes_nozzle_changes() {
        let slides = vec![
            vec![placement_at(0.0, 0.0, Some("A")), placement_at(0.0, 10.0, Some("B"))],
            vec![placement_at(0.0, 0.0, Some("B")), placement_at(0.0, 10.0, Some("A"))],
        ];
        let steps = schedule_print(&slides, "A", PrintOrder::NozzleMajor, RouteMode::Unconstrained);
        assert_eq!(steps.len(), 4);
        // vsechny "A" kroky pred vsemi "B" kroky
        let first_b = steps.iter().position(|s| s.nozzle == "B").unwrap();
        assert!(steps[..first_b].iter().all(|s| s.nozzle == "A"));
        assert!(steps[first_b..].iter().all(|s| s.nozzle == "B"));
    }

    #[test]
    fn unconstrained_picks_strict_nearest_neighbor() {
        let slides = vec![vec![
            placement_at(0.0, 40.0, None),
            placement_at(0.0, 2.0, None),
            placement_at(0.0, 20.0, None),
        ]];
        let steps = schedule_print(&slides, "A", PrintOrder::SubstrateMajor, RouteMode::Unconstrained);
        // od (0,0): nejdriv nejblizsi (obj_idx=1, y=2), pak (obj_idx=2, y=20), pak (obj_idx=0, y=40)
        assert_eq!(steps.iter().map(|s| s.obj_idx).collect::<Vec<_>>(), vec![1, 2, 0]);
    }

    /// Fixtura se skutečným křížením: `wall` je svislá zeď (x=1, y od -1 do 1)
    /// natištěná brzy; `hop` přesune kurzor na stranu počátku (-3,0); `target`
    /// leží za zdí (5,0) — přímá cesta k němu ze (-3,0) protíná zeď v jejím
    /// VNITŘKU (ne jen v dotyku krajního bodu, kterému `segs_cross` záměrně
    /// nerozumí jako křížení). `safe` leží dál, ale bez nutnosti zeď křížit.
    fn wall_fixture(with_safe_detour: bool) -> Vec<Vec<Placement>> {
        let dot = |x: f64, y: f64| Placement {
            object_id: format!("dot-{x}-{y}"),
            enabled: true,
            transform: identity_transform(),
            baked_scale: 1.0,
            nozzle_type: None,
            nozzle_diam: None,
            path: SubstratePaths::new(vec![PathSegment::new(vec![Point2D::new(x, y)])]),
        };
        let seg = |x1: f64, y1: f64, x2: f64, y2: f64| Placement {
            object_id: format!("seg-{x1}-{y1}-{x2}-{y2}"),
            enabled: true,
            transform: identity_transform(),
            baked_scale: 1.0,
            nozzle_type: None,
            nozzle_diam: None,
            path: SubstratePaths::new(vec![PathSegment::new(vec![
                Point2D::new(x1, y1),
                Point2D::new(x2, y2),
            ])]),
        };

        let start = dot(0.0, 0.0);
        let wall = seg(1.0, -1.0, 1.0, 1.0); // zed x=1, y in [-1,1]
        let hop = seg(1.0, 5.0, -3.0, 0.0); // presune kurzor na (-3,0)
        let target = dot(5.0, 0.0); // za zdi - primy prejezd ze (-3,0) ji krizi

        let mut row = vec![start, wall, hop, target];
        if with_safe_detour {
            row.push(dot(-3.0, 20.0)); // dal, ale bez nutnosti zed krizit
        }
        vec![row]
    }

    #[test]
    fn avoid_crossing_prefers_detour_when_direct_path_crosses_printed_wall() {
        let slides = wall_fixture(true);

        let unconstrained =
            schedule_print(&slides, "A", PrintOrder::SubstrateMajor, RouteMode::Unconstrained);
        let avoid_crossing =
            schedule_print(&slides, "A", PrintOrder::SubstrateMajor, RouteMode::AvoidCrossing);

        // obj_idx: 0=start, 1=wall, 2=hop, 3=target, 4=safe
        // Unconstrained jde po prvnich trech krocich primo na blizsi "target" (kriziho zed).
        assert_eq!(
            unconstrained.iter().map(|s| s.obj_idx).collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4]
        );
        // AvoidCrossing odmitne "target" (krizi zed) a napred obslouzi "safe" (obj_idx=4),
        // "target" dotiskne az jako posledni (jedina zbyvajici moznost).
        assert_eq!(
            avoid_crossing.iter().map(|s| s.obj_idx).collect::<Vec<_>>(),
            vec![0, 1, 2, 4, 3]
        );
    }

    #[test]
    fn avoid_crossing_falls_back_when_everything_would_cross() {
        // Bez "safe" alternativy je "target" jedinym zbyvajicim kandidatem a
        // opravdu krizi zed — funkce musi presto vratit vsechny kroky
        // (fallback na nejblizsiho, zadny panic, zadna ztrata objektu).
        let slides = wall_fixture(false);
        let steps = schedule_print(&slides, "A", PrintOrder::SubstrateMajor, RouteMode::AvoidCrossing);
        let mut idxs: Vec<usize> = steps.iter().map(|s| s.obj_idx).collect();
        idxs.sort();
        assert_eq!(idxs, vec![0, 1, 2, 3]);
    }

    #[test]
    fn fastest_and_cross_allowed_are_identical() {
        assert_eq!(RouteMode::from_setting("fastest"), RouteMode::from_setting("cross_allowed"));
        let slides = vec![vec![
            placement_at(0.0, 40.0, None),
            placement_at(0.0, 2.0, None),
            placement_at(0.0, 20.0, None),
        ]];
        let a = schedule_print(&slides, "A", PrintOrder::SubstrateMajor, RouteMode::from_setting("fastest"));
        let b = schedule_print(&slides, "A", PrintOrder::SubstrateMajor, RouteMode::from_setting("cross_allowed"));
        assert_eq!(a, b);
    }

    #[test]
    fn from_setting_defaults() {
        assert_eq!(PrintOrder::from_setting(""), PrintOrder::NozzleMajor);
        assert_eq!(PrintOrder::from_setting("cokoli"), PrintOrder::NozzleMajor);
        assert_eq!(RouteMode::from_setting(""), RouteMode::AvoidCrossing);
        assert_eq!(RouteMode::from_setting("cokoli"), RouteMode::AvoidCrossing);
    }
}
