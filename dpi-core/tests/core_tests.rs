use dpi_core::*;
use std::collections::HashMap;

/// Sestaví umístění z dvojic (trasa, transformace) — jedno sklíčko, jeden objekt.
fn slides_of(paths: &[SubstratePaths], transforms: &[Transform]) -> Vec<Vec<Placement>> {
    paths
        .iter()
        .zip(transforms.iter())
        .map(|(p, t)| {
            vec![Placement {
                object_id: "obj-0".to_string(),
                enabled: true,
                transform: *t,
                baked_scale: 1.0,
                nozzle_type: None,
                nozzle_diam: None,
                path: p.clone(),
            }]
        })
        .collect()
}

#[test]
fn test_dxf_circle_arc_parsing() {
    let dxf = "\
0\nSECTION\n2\nENTITIES\n\
0\nCIRCLE\n10\n5.0\n20\n5.0\n40\n3.0\n\
0\nARC\n10\n10.0\n20\n10.0\n40\n2.0\n50\n0.0\n51\n90.0\n\
0\nENDSEC\n0\nEOF\n";

    let paths = parse_dxf(dxf);
    // Kružnice → 65 bodů (0..=64), oblouk → min 2+1 body
    assert_eq!(paths.segments.len(), 2);

    let circle = &paths.segments[0];
    assert_eq!(circle.points.len(), 65); // 64 segmentů + uzavření
    assert_eq!(circle.is_filled, Some(true));
    // Střed (5,5), r=3 → první bod by měl být (8, 5) (0° = (cx+r, cy))
    assert!((circle.points[0].x - 8.0).abs() < 1e-6);
    assert!((circle.points[0].y - 5.0).abs() < 1e-6);

    let arc = &paths.segments[1];
    assert!(arc.points.len() >= 3);
    // Oblouk 0°-90° r=2 ze středu (10,10): start = (12,10), konec = (10,12)
    assert!((arc.points[0].x - 12.0).abs() < 1e-6);
    assert!((arc.points[0].y - 10.0).abs() < 1e-6);
    assert!((arc.points.last().unwrap().x - 10.0).abs() < 1e-3);
    assert!((arc.points.last().unwrap().y - 12.0).abs() < 1e-3);
}

#[test]
fn test_layout_calculation_basic() {
    // Standard Prusa bed dimensions: 250 x 210
    let bed_max_x = 250.0;
    let bed_max_y = 210.0;
    let start_offset_x = 18.0;
    let start_offset_y = 11.0;
    let spacing = 5.0;
    let slide_w = 25.0;
    let slide_h = 75.0;

    // Test without priming, count = 2
    let bed = BedConfig {
        max_x: bed_max_x,
        max_y: bed_max_y,
        min_x: 0.0,
        offset_x: start_offset_x,
        offset_y: start_offset_y,
    };
    let positions = get_layout_positions(2, slide_w, slide_h, spacing, false, None, &bed);

    assert_eq!(positions.len(), 2);

    // First slide: curr_x = min_x + offset_x = 0 + 18 = 18
    assert_eq!(positions[0].x, 18.0);
    assert_eq!(positions[0].y, 11.0);
    assert!(!positions[0].is_prime);

    // Second slide: same column, y2 = 11.0 + 75.0 + 5.0 = 91.0
    assert_eq!(positions[1].x, 18.0);
    assert_eq!(positions[1].y, 91.0);
}

#[test]
fn test_layout_multiplex_column_overflow() {
    let bed_max_x = 250.0;
    let bed_max_y = 210.0;
    let start_offset_x = 18.0;
    let start_offset_y = 11.0;
    let spacing = 5.0;
    let slide_w = 25.0;
    let slide_h = 75.0;

    // With slide height of 75mm, maximum Y of 210mm can fit 2 slides per column:
    // Slide 1 Y: 11 -> 86
    // Slide 2 Y: 91 -> 166
    // Slide 3 would overflow (166 + 5 + 75 = 246 > 210)
    // So Slide 3 must jump to a new column on the right!
    let bed = BedConfig {
        max_x: bed_max_x,
        max_y: bed_max_y,
        min_x: 0.0,
        offset_x: start_offset_x,
        offset_y: start_offset_y,
    };
    let positions = get_layout_positions(3, slide_w, slide_h, spacing, false, None, &bed);

    assert_eq!(positions.len(), 3);

    // Slide 3 X location:
    // curr_x after overflow = 18 + 25 + 5 = 48
    assert_eq!(positions[2].x, 48.0);
    assert_eq!(positions[2].y, 11.0);
}

#[test]
fn test_layout_small_bed() {
    // bed_max_x = 110 mm — zúžená podložka, standardní sklo 76×26 mm
    let bed = BedConfig {
        max_x: 110.0,
        max_y: 210.0,
        min_x: 0.0,
        offset_x: 18.0,
        offset_y: 11.0,
    };
    let positions = get_layout_positions(2, 76.0, 26.0, 5.0, true, None, &bed);

    // Prime + 2 samples
    assert_eq!(positions.len(), 3, "prime + 2 vzorky");

    // Prime: curr_x = min_x + offset_x = 0 + 18 = 18
    let prime = &positions[0];
    assert!(prime.is_prime);
    assert_eq!(prime.x, 18.0);
    assert_eq!(prime.y, 11.0);
    assert!(
        prime.x >= 0.0 && prime.x + prime.width <= 110.0,
        "prime mimo podložku"
    );

    // Sample 1: curr_y = 11+26+5 = 42, x = 18
    let s1 = &positions[1];
    assert!(!s1.is_prime);
    assert_eq!(s1.x, 18.0);
    assert_eq!(s1.y, 42.0);
    assert!(
        s1.x >= 0.0 && s1.x + s1.width <= 110.0,
        "sample1 mimo podložku v X"
    );
    assert!(
        s1.y >= 0.0 && s1.y + s1.height <= 210.0,
        "sample1 mimo podložku v Y"
    );

    // Sample 2: curr_y = 42+26+5 = 73
    let s2 = &positions[2];
    assert_eq!(s2.x, 18.0);
    assert_eq!(s2.y, 73.0);
    assert!(
        s2.x >= 0.0 && s2.x + s2.width <= 110.0,
        "sample2 mimo podložku v X"
    );
}

#[test]
fn test_extrusion_calculations() {
    // Filament diam = 9.5 mm, cross section = pi * (4.75)^2 = 70.882 mm2
    // default_cal = 1 / 70.882 = 0.014108 mm of filament per 1 mm3 of fluid volume
    let calc = ExtrusionCalculator::new(9.5, 1.0, None);

    // Verify default calibration factor
    let expected_cal = 1.0 / (std::f64::consts::PI * 4.75 * 4.75);
    assert!((calc.calibration_factor - expected_cal).abs() < 1e-6);

    // Test µl/mm conversion
    // 1.5 µl/mm * 1.0 flow_mult * cal_factor
    let e_val = calc.calculate_e_per_mm(1.5, "µl/mm");
    assert!((e_val - 1.5 * expected_cal).abs() < 1e-6);

    // Test direct steps/mm conversion (should pass rate directly)
    let e_val_steps = calc.calculate_e_per_mm(42.0, "kroky/mm");
    assert_eq!(e_val_steps, 42.0);
}

#[test]
fn test_gcode_generation_z_shift() {
    let paths = vec![SubstratePaths::new(vec![PathSegment::new(vec![
        Point2D::new(0.0, 0.0),
        Point2D::new(10.0, 10.0),
    ])])];

    let params = ProcessParams {
        sample_count: 1,
        prime_active: false,
        slide_w: 25.0,
        slide_h: 75.0,
        slide_z: 1.0,
        z_offset: 0.2,
        z_unit: "mm".to_string(),
        nozzle_height: 30.0,
        nozzle_hidden: 4.0,
        filament_diameter: 9.5,
        flow_multiplier: 1.0,
        bed_temp: 0.0,
        extrusion_rate: 1.0,
        extrusion_unit: "µl/mm".to_string(),
        nozzle_diam: 0.4,
        infill_style: "S okraji".to_string(),
        infill_val: 1.0,
        infill_type: "mm".to_string(),
        infill_angle: 0.0,
        print_speed: 1500.0,
        nozzle_type: String::new(),
    };

    let transforms = vec![Transform {
        scale: 1.0,
        rotation: 0.0,
        gui_dx: 207.0,
        gui_dy: 124.0,
        cx: 12.5,
        cy: 37.5,
    }];

    // block_height = 34.0
    // Absolute nozzle height = -34.0 + 30.0 - 4.0 + 1.0 + 0.2 = -6.8 mm (goes negative!)
    // So z_shift should be triggered! min_z = -6.8 => z_shift = 6.8 + 1.0 = 7.8 mm
    // Let's test that generate_gcode successfully builds and runs.
    let machine = MachineConfig {
        bed: BedConfig {
            max_x: 250.0,
            max_y: 210.0,
            min_x: 0.0,
            offset_x: 18.0,
            offset_y: 11.0,
        },
        start_gcode: "; START\n".into(),
        end_gcode: "; END\n".into(),
        loop_start_gcode: String::new(),
        loop_end_gcode: String::new(),
        multi_spacing: 5.0,
        block_height: 34.0,
        calibration_factor: 0.0141,
        z_hop: 2.0,
        safe_z: 20.0,
        bed_max_temp: None,
        skip_z_shift_setup: false,
        nozzle_change_gcode: String::new(),
        print_order: "substrate_major".to_string(),
        route_mode: "fastest".to_string(),
    };
    let res = generate_gcode(
        &slides_of(&paths, &transforms),
        &params,
        &std::collections::HashMap::new(),
        &machine,
    );

    assert!(res.is_ok());
    let (gcode, dist, _time) = res.unwrap();

    // Verify it contains virtual shift message and command
    assert!(gcode.contains("; --- VIRTUALNI POSUN Z (SHIFT 7.80mm) ---"));
    assert!(gcode.contains("G92 Z27.800"));
    assert!(dist > 0.0);

    // Sdílená funkce musí vracet stejný shift jako generátor
    let shift = compute_z_shift(&params, &std::collections::HashMap::new(), 34.0);
    assert!((shift - 7.8).abs() < 1e-9);

    // skip_z_shift_setup: G92 blok se negeneruje, ale tiskové výšky zůstávají posunuté
    let machine_skip = MachineConfig {
        skip_z_shift_setup: true,
        ..machine.clone()
    };
    let (gcode_skip, _, _) = generate_gcode(
        &slides_of(&paths, &transforms),
        &params,
        &std::collections::HashMap::new(),
        &machine_skip,
    )
    .unwrap();
    assert!(!gcode_skip.contains("VIRTUALNI POSUN"));
    assert!(!gcode_skip.contains("G92 Z27.800"));
    // print_z = -6.8 + 7.8 = 1.0 → sjezd k povrchu na Z1.000 zůstává
    assert!(gcode_skip.contains("Z1.000"));
}

#[test]
fn test_gcode_generation_with_overrides() {
    let paths = vec![SubstratePaths::new(vec![PathSegment::new(vec![
        Point2D::new(0.0, 0.0),
        Point2D::new(10.0, 0.0),
    ])])];

    let params = ProcessParams {
        sample_count: 1,
        prime_active: false,
        slide_w: 25.0,
        slide_h: 75.0,
        slide_z: 1.0,
        z_offset: 0.5,
        z_unit: "mm".to_string(),
        nozzle_height: 30.0,
        nozzle_hidden: 4.0,
        filament_diameter: 9.5,
        flow_multiplier: 1.0,
        bed_temp: 0.0,
        extrusion_rate: 1.0,
        extrusion_unit: "µl/mm".to_string(),
        infill_val: 1.0,
        infill_type: "mm".to_string(),
        infill_angle: 0.0,
        nozzle_type: String::new(),
        nozzle_diam: 0.4,
        infill_style: "S okraji".to_string(),
        print_speed: 1500.0,
    };

    let transforms = vec![Transform {
        scale: 1.0,
        rotation: 0.0,
        gui_dx: 207.0,
        gui_dy: 124.0,
        cx: 12.5,
        cy: 37.5,
    }];

    let mut overrides = std::collections::HashMap::new();
    overrides.insert(
        "0".to_string(),
        SlideOverride {
            name: Some("Vzorek s vetsim offsetem".to_string()),
            note: Some("Komentar".to_string()),
            z_offset: Some(0.5),
            extrusion_rate: Some(2.0),
            extrusion_unit: Some("µl/mm".to_string()),
            print_speed: Some(2000.0),
            infill_val: None,
            infill_type: None,
            nozzle_height: None,
            infill_style: None,
            slide_w: None,
            slide_h: None,
            glass_type: None,
        },
    );

    let machine = MachineConfig {
        bed: BedConfig {
            max_x: 250.0,
            max_y: 210.0,
            min_x: 0.0,
            offset_x: 18.0,
            offset_y: 11.0,
        },
        start_gcode: "; START\n".into(),
        end_gcode: "; END\n".into(),
        loop_start_gcode: String::new(),
        loop_end_gcode: String::new(),
        multi_spacing: 5.0,
        block_height: 34.0,
        calibration_factor: 0.0141,
        z_hop: 2.0,
        safe_z: 20.0,
        bed_max_temp: None,
        skip_z_shift_setup: false,
        nozzle_change_gcode: String::new(),
        print_order: "substrate_major".to_string(),
        route_mode: "fastest".to_string(),
    };
    let res = generate_gcode(&slides_of(&paths, &transforms), &params, &overrides, &machine);

    assert!(res.is_ok());
    let (gcode, _dist, _time) = res.unwrap();

    // S větším offsetem (0.5 namísto 0.2) se změní absolutní výška tisku i shift!
    // Původní min_needed_z = -34.0 + 30.0 - 4.0 + 1.0 + 0.2 = -6.8
    // S overridem min_needed_z = -34.0 + 30.0 - 4.0 + 1.0 + 0.5 = -6.5
    // Z shift = 6.5 + 1.0 = 7.5
    assert!(gcode.contains("; --- VIRTUALNI POSUN Z (SHIFT 7.50mm) ---"));

    // compute_z_shift musí zohlednit overrides stejně jako generátor
    let shift = compute_z_shift(&params, &overrides, 34.0);
    assert!((shift - 7.5).abs() < 1e-9);

    // Extruze: 2.0 (rate) * 0.014108 (cal_factor) = 0.028216 E na mm
    // Dráha je 10 mm, takže E = 10.0 * 0.028216 = 0.28216
    assert!(gcode.contains("E0.28200"));
    // Rychlost: F2000
    assert!(gcode.contains("F2000"));
}

#[test]
fn test_parse_gcode_paths() {
    let gcode = "\
G0 X10.0 Y5.0\n\
G1 X20.0 Y5.0 E0.1\n\
G1 X20.0 Y10.0 E0.1\n\
G0 X0.0 Y0.0\n\
G1 X5.0 Y0.0 E0.05\n";

    let paths = parse_gcode_paths(gcode);
    // Dva přejezdy → dvě segmenty
    assert_eq!(paths.segments.len(), 2);
    // První segment: 3 body (start + 2 G1)
    assert_eq!(paths.segments[0].points.len(), 3);
    assert!((paths.segments[0].points[0].x - 10.0).abs() < 1e-6);
    assert!((paths.segments[0].points[2].y - 10.0).abs() < 1e-6);
    // Druhý segment: 2 body
    assert_eq!(paths.segments[1].points.len(), 2);
}

#[test]
fn test_generate_prime_preview() {
    let pos = LayoutPosition {
        x: 200.0,
        y: 0.0,
        width: 76.0,
        height: 26.0,
        is_prime: true,
    };
    let params = ProcessParams {
        sample_count: 1,
        prime_active: true,
        slide_w: 76.0,
        slide_h: 26.0,
        slide_z: 1.0,
        z_offset: 0.2,
        z_unit: "mm".into(),
        nozzle_height: 30.0,
        nozzle_hidden: 4.0,
        filament_diameter: 9.5,
        flow_multiplier: 1.0,
        bed_temp: 0.0,
        extrusion_rate: 1.0,
        extrusion_unit: "nl/mm".into(),
        nozzle_diam: 0.4,
        infill_style: "Okraje + Výplň".into(),
        infill_val: 1.0,
        infill_type: "mm".into(),
        infill_angle: 0.0,
        print_speed: 1500.0,
        nozzle_type: "Červená".into(),
    };

    let preview = generate_prime_preview(&pos, &params, None);
    // Musí vrátit alespoň jeden segment s body
    assert!(!preview.segments.is_empty());
    let pts = &preview.segments[0].points;
    // Výchozí oblast 15×15 mm, spacing = nozzle_diam = 0.4 → mnoho řádků
    assert!(pts.len() >= 4);
    // Všechny body musí ležet v mezích prime oblasti (cx±7.5, cy±7.5)
    let cx = pos.width / 2.0; // 38
    let cy = pos.height / 2.0; // 13
    for pt in pts {
        assert!(
            pt.x >= cx - 7.5 - 1e-6 && pt.x <= cx + 7.5 + 1e-6,
            "x={} mimo meze",
            pt.x
        );
        assert!(
            pt.y >= cy - 7.5 - 1e-6 && pt.y <= cy + 7.5 + 1e-6,
            "y={} mimo meze",
            pt.y
        );
    }
}

#[test]
fn test_metadata_round_trip() {
    use dpi_core::{deserialize_metadata, serialize_metadata, GCodeMetadata};
    use std::collections::HashMap;

    let params = ProcessParams {
        sample_count: 2,
        prime_active: true,
        slide_w: 76.0,
        slide_h: 26.0,
        slide_z: 1.0,
        z_offset: 0.2,
        z_unit: "mm".into(),
        nozzle_height: 30.0,
        nozzle_hidden: 4.0,
        filament_diameter: 9.5,
        flow_multiplier: 1.0,
        bed_temp: 0.0,
        extrusion_rate: 1.0,
        extrusion_unit: "nl/mm".into(),
        nozzle_diam: 0.4,
        infill_style: "Okraje".into(),
        infill_val: 1.0,
        infill_type: "mm".into(),
        infill_angle: 0.0,
        print_speed: 1500.0,
        nozzle_type: "Červená".into(),
    };

    let meta = GCodeMetadata {
        params,
        overrides: HashMap::new(),
        sources: vec![],
        objects: vec![],
        slides: vec![],
        transforms: vec![],
        baked_scales: vec![1.0, 1.0],
        source_file_name: "test.svg".into(),
        source_file_ext: "svg".into(),
        source_file_content: "<svg/>".into(),
        auto_scale: true,
        fineness: 2.5,
    };

    let header = serialize_metadata(&meta);
    assert!(header.contains("; --- EDITOR METADATA ---"));
    assert!(header.contains("; --- END METADATA ---"));
    assert!(header.contains("\"dpi_version\":3"));

    // Deserializace musí vrátit shodná data
    let restored = deserialize_metadata(&header).expect("deserializace selhala");
    assert_eq!(restored.params.sample_count, 2);
    assert_eq!(restored.source_file_name, "test.svg");
    assert_eq!(restored.source_file_ext, "svg");
    assert!((restored.fineness - 2.5).abs() < 1e-9);
    assert!(restored.auto_scale);
    assert_eq!(restored.baked_scales, vec![1.0, 1.0]);
}

#[test]
fn test_metadata_overrides_round_trip() {
    use dpi_core::{
        deserialize_metadata, serialize_metadata, GCodeMetadata, SlideOverride, Transform,
    };
    use std::collections::HashMap;

    let params = ProcessParams {
        sample_count: 3,
        prime_active: false,
        slide_w: 76.0,
        slide_h: 26.0,
        slide_z: 1.0,
        z_offset: 0.2,
        z_unit: "mm".into(),
        nozzle_height: 30.0,
        nozzle_hidden: 4.0,
        filament_diameter: 9.5,
        flow_multiplier: 1.0,
        bed_temp: 0.0,
        extrusion_rate: 1.0,
        extrusion_unit: "nl/mm".into(),
        nozzle_diam: 0.4,
        infill_style: "Okraje + Výplň".into(),
        infill_val: 1.0,
        infill_type: "mm".into(),
        infill_angle: 45.0,
        print_speed: 1500.0,
        nozzle_type: "Červená".into(),
    };

    let mut overrides = HashMap::new();
    // Sklíčko 0: jiný název, z_offset, extruze
    overrides.insert(
        "0".to_string(),
        SlideOverride {
            name: Some("Vzorek A".to_string()),
            note: Some("Kontrolní vzorek".to_string()),
            z_offset: Some(0.5),
            extrusion_rate: Some(2.0),
            extrusion_unit: Some("nl/mm".to_string()),
            print_speed: Some(1200.0),
            infill_val: Some(0.8),
            infill_type: Some("mm".to_string()),
            nozzle_height: Some(31.5),
            infill_style: Some("Okraje".to_string()),
            slide_w: None,
            slide_h: None,
            glass_type: None,
        },
    );
    // Sklíčko 2: pouze rychlost
    overrides.insert(
        "2".to_string(),
        SlideOverride {
            print_speed: Some(2000.0),
            ..Default::default()
        },
    );

    let transforms = vec![
        Transform {
            scale: 1.0,
            rotation: 45.0,
            gui_dx: 18.0,
            gui_dy: 11.0,
            cx: 38.0,
            cy: 13.0,
        },
        Transform {
            scale: 1.0,
            rotation: 0.0,
            gui_dx: 18.0,
            gui_dy: 42.0,
            cx: 38.0,
            cy: 13.0,
        },
        Transform {
            scale: 1.0,
            rotation: -30.0,
            gui_dx: 18.0,
            gui_dy: 73.0,
            cx: 38.0,
            cy: 13.0,
        },
    ];

    let meta = GCodeMetadata {
        params,
        overrides,
        sources: vec![],
        objects: vec![],
        slides: vec![],
        transforms,
        baked_scales: vec![1.5, 1.0, 0.8],
        source_file_name: "vzorky.dxf".into(),
        source_file_ext: "dxf".into(),
        source_file_content: "0\nEOF\n".into(),
        auto_scale: false,
        fineness: 1.0,
    };

    let header = serialize_metadata(&meta);
    let restored = deserialize_metadata(&header).expect("deserializace selhala");

    // Params
    assert_eq!(restored.params.sample_count, 3);
    assert!((restored.params.infill_angle - 45.0).abs() < 1e-9);
    assert_eq!(restored.params.infill_style, "Okraje + Výplň");

    // Overrides — sklíčko 0
    let o0 = restored.overrides.get("0").expect("override 0 chybí");
    assert_eq!(o0.name.as_deref(), Some("Vzorek A"));
    assert_eq!(o0.note.as_deref(), Some("Kontrolní vzorek"));
    assert!((o0.z_offset.unwrap() - 0.5).abs() < 1e-9);
    assert!((o0.extrusion_rate.unwrap() - 2.0).abs() < 1e-9);
    assert!((o0.print_speed.unwrap() - 1200.0).abs() < 1e-9);
    assert!((o0.infill_val.unwrap() - 0.8).abs() < 1e-9);
    assert_eq!(o0.infill_style.as_deref(), Some("Okraje"));
    assert!((o0.nozzle_height.unwrap() - 31.5).abs() < 1e-9);

    // Overrides — sklíčko 2
    let o2 = restored.overrides.get("2").expect("override 2 chybí");
    assert!((o2.print_speed.unwrap() - 2000.0).abs() < 1e-9);
    assert!(o2.z_offset.is_none());

    // Transforms — rotace a baked_scales
    assert!((restored.transforms[0].rotation - 45.0).abs() < 1e-9);
    assert!((restored.transforms[1].rotation - 0.0).abs() < 1e-9);
    assert!((restored.transforms[2].rotation - (-30.0)).abs() < 1e-9);
    assert_eq!(restored.baked_scales, vec![1.5, 1.0, 0.8]);

    // Source file
    assert_eq!(restored.source_file_name, "vzorky.dxf");
    assert_eq!(restored.source_file_ext, "dxf");
    assert!(!restored.auto_scale);
}

/// Pomocná funkce: ověří monotónnost a boustrofedon pořadí infill segmentů.
fn assert_boustrophedon(segs: &[&PathSegment], label: &str) {
    let infill: Vec<&&PathSegment> = segs
        .iter()
        .filter(|s| {
            s.points.len() == 2
                && ((s.points[0].x - s.points[1].x).abs() > 1e-6
                    || (s.points[0].y - s.points[1].y).abs() > 1e-6)
        })
        .collect();

    assert!(!infill.is_empty(), "{}: žádné infill segmenty", label);

    let ys: Vec<f64> = infill.iter().map(|s| s.points[0].y).collect();
    let ascending = ys.windows(2).all(|w| w[1] >= w[0] - 1e-6);
    let descending = ys.windows(2).all(|w| w[1] <= w[0] + 1e-6);
    assert!(
        ascending || descending,
        "{}: Y-pořadí není monotónní (skok!): {:?}",
        label,
        ys
    );

    for (i, seg) in infill.iter().enumerate() {
        if i % 2 == 0 {
            assert!(
                seg.points[0].x <= seg.points[1].x + 1e-6,
                "{}: řada {} musí jít vlevo→vpravo (x0={:.3} x1={:.3})",
                label, i, seg.points[0].x, seg.points[1].x
            );
        } else {
            assert!(
                seg.points[0].x >= seg.points[1].x - 1e-6,
                "{}: řada {} musí jít vpravo→vlevo (x0={:.3} x1={:.3})",
                label, i, seg.points[0].x, seg.points[1].x
            );
        }
    }
}

/// Ověří že infill scanlines jsou v boustrofedon pořadí (Y monotónní, střídání směru).
/// Simuluje obdélník 20×10 mm s výplní 2 mm — očekáváme ~5 řad bez skoků.
#[test]
fn test_infill_boustrophedon_order() {
    // Jednoduchý obdélník 20×10 mm, pouze výplň (bez okraje)
    let rect = vec![
        Point2D::new(0.0, 0.0),
        Point2D::new(20.0, 0.0),
        Point2D::new(20.0, 10.0),
        Point2D::new(0.0, 10.0),
        Point2D::new(0.0, 0.0),
    ];
    let raw = SubstratePaths::new(vec![PathSegment {
        points: rect,
        is_filled: Some(true),
        has_stroke: Some(false),
    }]);

    let params = SliceParams {
        slide_w: 30.0,
        slide_h: 20.0,
        margin: 1.0,
        auto_scale: false,
        infill_style: "Okraje + Výplň".to_string(),
        infill_val: 2.0,
        infill_type: "mm".to_string(),
        infill_angle: 0.0,
        nozzle_diam: 0.4,
        user_scale: 1.0,
        frame: None,
    };

    let result = process_substrate_paths(&raw, &params);

    // Vyfiltruj pouze 2-bodové infill segmenty (ne tečky — ty mají start==end)
    let infill: Vec<&PathSegment> = result
        .segments
        .iter()
        .filter(|s| {
            s.points.len() == 2
                && ((s.points[0].x - s.points[1].x).abs() > 1e-6
                    || (s.points[0].y - s.points[1].y).abs() > 1e-6)
        })
        .collect();

    assert!(!infill.is_empty(), "Žádné infill segmenty nenalezeny");

    let infill: Vec<&PathSegment> = result
        .segments
        .iter()
        .filter(|s| {
            s.points.len() == 2
                && ((s.points[0].x - s.points[1].x).abs() > 1e-6
                    || (s.points[0].y - s.points[1].y).abs() > 1e-6)
        })
        .collect();

    assert_boustrophedon(&infill, "jednoduchý obdélník");
}

/// Ověří že celé objekty jsou renderovány zleva doprava.
/// Tři obdélníky jsou zadány v OPAČNÉM pořadí (pravý, střední, levý) —
/// výstup musí mít segmenty seřazeny: levý → střední → pravý.
#[test]
fn test_object_rendering_order_left_to_right() {
    let make_rect = |x0: f64, x1: f64, y0: f64, y1: f64| -> PathSegment {
        PathSegment {
            points: vec![
                Point2D::new(x0, y0),
                Point2D::new(x1, y0),
                Point2D::new(x1, y1),
                Point2D::new(x0, y1),
                Point2D::new(x0, y0),
            ],
            is_filled: Some(true),
            has_stroke: Some(false),
        }
    };

    // Záměrně v opačném pořadí: pravý (X=21-29), střední (X=11-19), levý (X=1-9)
    let raw = SubstratePaths::new(vec![
        make_rect(21.0, 29.0, 1.0, 8.0),
        make_rect(11.0, 19.0, 1.0, 8.0),
        make_rect(1.0,  9.0,  1.0, 8.0),
    ]);

    let params = SliceParams {
        slide_w: 50.0,
        slide_h: 20.0,
        margin: 0.5,
        auto_scale: false,
        infill_style: "Okraje + Výplň".to_string(),
        infill_val: 2.0,
        infill_type: "mm".to_string(),
        infill_angle: 0.0,
        nozzle_diam: 0.4,
        user_scale: 1.0,
        frame: None,
    };

    let result = process_substrate_paths(&raw, &params);

    // Filtruj pouze infill segmenty (2 body, horizontální)
    let segs: Vec<&PathSegment> = result
        .segments
        .iter()
        .filter(|s| {
            s.points.len() == 2
                && (s.points[0].x - s.points[1].x).abs() > 1e-6
                && (s.points[0].y - s.points[1].y).abs() < 1e-6
        })
        .collect();

    assert!(!segs.is_empty(), "Žádné infill segmenty nenalezeny");

    // Každý segment přiřaď k objektu podle X midpointu
    // Po normalizaci a centrování:
    //   bbox: x=1..29, y=1..8, span=28
    //   off_x = (50 - 28) / 2 = 11
    //   levý obj:    X = 11+0 .. 11+8  = 11..19
    //   střední obj: X = 11+10.. 11+18 = 21..29
    //   pravý obj:   X = 11+20.. 11+28 = 31..39
    let group = |x_mid: f64| -> u8 {
        if x_mid < 20.0 { 0 } // levý
        else if x_mid < 30.0 { 1 } // střední
        else { 2 } // pravý
    };

    let groups: Vec<u8> = segs
        .iter()
        .map(|s| group((s.points[0].x + s.points[1].x) / 2.0))
        .collect();

    // Skupiny nesmí nikdy klesat: 0…0, 1…1, 2…2 (žádný přechod zpět)
    for i in 1..groups.len() {
        assert!(
            groups[i] >= groups[i - 1],
            "Objekty nejsou renderovány zleva doprava: přechod z grupy {} na {} u segmentu {}",
            groups[i - 1], groups[i], i
        );
    }

    // A pořadí skupin musí být 0 → 1 → 2
    let seen_groups: Vec<u8> = {
        let mut v: Vec<u8> = groups.clone();
        v.dedup();
        v
    };
    assert_eq!(
        seen_groups,
        vec![0, 1, 2],
        "Pořadí objektů není 0(levý)→1(střední)→2(pravý): {:?}",
        seen_groups
    );
}

/// Polygon s dírou (vnější rect + vnitřní elipsa) — přesný případ z asdf.gcode (rect2).
/// NN dříve způsoboval skoky Y=17→14→13→15→16 kvůli proměnlivé šířce bbox.
#[test]
fn test_infill_boustrophedon_with_hole() {
    // Vnější obdélník 20×12 mm
    let outer = vec![
        Point2D::new(0.0, 0.0),
        Point2D::new(20.0, 0.0),
        Point2D::new(20.0, 12.0),
        Point2D::new(0.0, 12.0),
        Point2D::new(0.0, 0.0),
    ];
    // Vnitřní elipsa jako díra (8 bodů)
    let hole: Vec<Point2D> = (0..=8)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / 8.0;
            Point2D::new(10.0 + 4.0 * a.cos(), 6.0 + 2.5 * a.sin())
        })
        .collect();

    let raw = SubstratePaths::new(vec![
        PathSegment { points: outer, is_filled: Some(true), has_stroke: Some(false) },
        PathSegment { points: hole,  is_filled: Some(true), has_stroke: Some(false) },
    ]);

    let params = SliceParams {
        slide_w: 30.0,
        slide_h: 20.0,
        margin: 1.0,
        auto_scale: false,
        infill_style: "Okraje + Výplň".to_string(),
        infill_val: 1.0,
        infill_type: "mm".to_string(),
        infill_angle: 0.0,
        nozzle_diam: 0.4,
        user_scale: 1.0,
        frame: None,
    };

    let result = process_substrate_paths(&raw, &params);

    let infill: Vec<&PathSegment> = result
        .segments
        .iter()
        .filter(|s| {
            s.points.len() == 2
                && ((s.points[0].x - s.points[1].x).abs() > 1e-6
                    || (s.points[0].y - s.points[1].y).abs() > 1e-6)
        })
        .collect();

    assert_boustrophedon(&infill, "polygon s dírou");
}

// ─── Více objektů na substrátu ───────────────────────────────────────────────

fn rect(x1: f64, y1: f64, x2: f64, y2: f64) -> PathSegment {
    PathSegment {
        points: vec![
            Point2D::new(x1, y1),
            Point2D::new(x2, y1),
            Point2D::new(x2, y2),
            Point2D::new(x1, y2),
            Point2D::new(x1, y1),
        ],
        is_filled: Some(true),
        has_stroke: Some(true),
    }
}

fn multi_params(sample_count: usize) -> ProcessParams {
    ProcessParams {
        sample_count,
        prime_active: false,
        slide_w: 25.0,
        slide_h: 75.0,
        slide_z: 1.0,
        z_offset: 0.2,
        z_unit: "mm".into(),
        nozzle_height: 30.0,
        nozzle_hidden: 4.0,
        filament_diameter: 9.5,
        flow_multiplier: 1.0,
        bed_temp: 0.0,
        extrusion_rate: 1.0,
        extrusion_unit: "µl/mm".into(),
        nozzle_diam: 0.4,
        infill_style: "Okraje".into(),
        infill_val: 1.0,
        infill_type: "mm".into(),
        infill_angle: 0.0,
        print_speed: 1500.0,
        nozzle_type: String::new(),
    }
}

fn test_bed() -> BedConfig {
    BedConfig {
        max_x: 250.0,
        max_y: 210.0,
        min_x: 0.0,
        offset_x: 18.0,
        offset_y: 11.0,
    }
}

fn scene_object(id: &str, raw: SubstratePaths, frame: [f64; 4]) -> SceneObject {
    SceneObject {
        id: id.into(),
        name: id.into(),
        raw,
        frame,
        auto_scale: false,
    }
}

#[test]
fn test_split_into_objects_separates_disjoint_shapes() {
    // Dva oddělené obdélníky → dva objekty
    let raw = SubstratePaths::new(vec![rect(0.0, 0.0, 5.0, 5.0), rect(20.0, 0.0, 25.0, 5.0)]);
    let objects = split_into_objects(&raw);
    assert_eq!(objects.len(), 2);
    // Řazení podle těžiště: zleva doprava
    let cx = |o: &SubstratePaths| {
        let pts: Vec<_> = o.segments.iter().flat_map(|s| s.points.iter()).collect();
        pts.iter().map(|p| p.x).sum::<f64>() / pts.len() as f64
    };
    assert!(cx(&objects[0]) < cx(&objects[1]));
}

#[test]
fn test_split_into_objects_keeps_hole_with_its_shape() {
    // Mezikruží: díra leží uvnitř obrysu → jeden objekt, aby výplň dál
    // fungovala podle even-odd pravidla
    let raw = SubstratePaths::new(vec![rect(0.0, 0.0, 20.0, 20.0), rect(5.0, 5.0, 15.0, 15.0)]);
    let objects = split_into_objects(&raw);
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].segments.len(), 2);
}

#[test]
fn test_shared_frame_preserves_relative_layout() {
    // Dva objekty z jednoho importu se normalizují vůči společnému rámu,
    // takže si zachovají vzájemný odstup ze souboru.
    let raw = SubstratePaths::new(vec![rect(0.0, 0.0, 5.0, 5.0), rect(15.0, 0.0, 20.0, 5.0)]);
    let frame = [0.0, 20.0, 0.0, 5.0];
    let objects = split_into_objects(&raw);
    assert_eq!(objects.len(), 2);

    let slice = |frame: Option<[f64; 4]>| SliceParams {
        slide_w: 25.0,
        slide_h: 75.0,
        margin: 2.0,
        auto_scale: false,
        infill_style: "Okraje".into(),
        infill_val: 1.0,
        infill_type: "mm".into(),
        infill_angle: 0.0,
        nozzle_diam: 0.4,
        user_scale: 1.0,
        frame,
    };

    let min_x = |p: &SubstratePaths| {
        p.segments
            .iter()
            .flat_map(|s| s.points.iter())
            .map(|pt| pt.x)
            .fold(f64::INFINITY, f64::min)
    };

    let a = process_substrate_paths(&objects[0], &slice(Some(frame)));
    let b = process_substrate_paths(&objects[1], &slice(Some(frame)));
    // Odstup levých hran zůstává 15 mm jako v souboru
    assert!((min_x(&b) - min_x(&a) - 15.0).abs() < 1e-6, "odstup {}", min_x(&b) - min_x(&a));

    // Bez společného rámu se oba vycentrují na sebe a odstup zmizí
    let a2 = process_substrate_paths(&objects[0], &slice(None));
    let b2 = process_substrate_paths(&objects[1], &slice(None));
    assert!((min_x(&b2) - min_x(&a2)).abs() < 1e-6);
}

#[test]
fn test_update_layout_places_objects_on_every_slide() {
    let raw = SubstratePaths::new(vec![rect(0.0, 0.0, 5.0, 5.0)]);
    let objects = vec![
        scene_object("a", raw.clone(), [0.0, 5.0, 0.0, 5.0]),
        scene_object("b", raw.clone(), [0.0, 5.0, 0.0, 5.0]),
    ];

    let res = dpi_core::update_layout(
        &multi_params(2),
        &HashMap::new(),
        &objects,
        &[],
        &[],
        &test_bed(),
        5.0,
    );

    assert_eq!(res.slides.len(), 2);
    for row in &res.slides {
        // Každé sklíčko nese položku pro každý objekt, ve stejném pořadí
        assert_eq!(row.len(), 2);
        assert_eq!(row[0].object_id, "a");
        assert_eq!(row[1].object_id, "b");
        assert!(row.iter().all(|p| p.enabled));
        assert!(row.iter().all(|p| !p.path.segments.is_empty()));
    }
}

#[test]
fn test_update_layout_skips_hidden_object_and_inherits_to_new_slide() {
    let raw = SubstratePaths::new(vec![rect(0.0, 0.0, 5.0, 5.0)]);
    let objects = vec![scene_object("a", raw, [0.0, 5.0, 0.0, 5.0])];

    // Výchozí layout pro jedno sklíčko
    let first = dpi_core::update_layout(
        &multi_params(1),
        &HashMap::new(),
        &objects,
        &[],
        &[],
        &test_bed(),
        5.0,
    );
    let pos0 = first.positions[0];

    // Objekt posuneme o 3 mm doprava a přidáme druhý substrát
    let mut state = first.slides[0][0].state();
    state.transform.gui_dx += 3.0;
    let second = dpi_core::update_layout(
        &multi_params(2),
        &HashMap::new(),
        &objects,
        &[vec![state.clone()]],
        &first.positions,
        &test_bed(),
        5.0,
    );

    assert_eq!(second.slides.len(), 2);
    // Nové sklíčko objekt zdědí se stejným relativním posunem
    let new_pos = second.positions[1];
    let rel_old = second.slides[0][0].transform.gui_dx - pos0.x;
    let rel_new = second.slides[1][0].transform.gui_dx - new_pos.x;
    assert!((rel_old - 3.0).abs() < 1e-6, "rel_old={rel_old}");
    assert!((rel_new - 3.0).abs() < 1e-6, "rel_new={rel_new}");

    // Skrytý objekt trasu negeneruje
    let mut hidden = state.clone();
    hidden.enabled = false;
    let third = dpi_core::update_layout(
        &multi_params(1),
        &HashMap::new(),
        &objects,
        &[vec![hidden]],
        &first.positions,
        &test_bed(),
        5.0,
    );
    assert!(!third.slides[0][0].enabled);
    assert!(third.slides[0][0].path.segments.is_empty());
}

#[test]
fn test_gcode_prints_all_visible_objects() {
    let params = multi_params(1);
    let machine = MachineConfig {
        bed: test_bed(),
        start_gcode: String::new(),
        end_gcode: String::new(),
        loop_start_gcode: String::new(),
        loop_end_gcode: String::new(),
        multi_spacing: 5.0,
        block_height: 34.0,
        calibration_factor: 0.0141,
        z_hop: 2.0,
        safe_z: 20.0,
        bed_max_temp: None,
        skip_z_shift_setup: false,
        nozzle_change_gcode: String::new(),
        print_order: "substrate_major".to_string(),
        route_mode: "fastest".to_string(),
    };

    let mk = |id: &str, y: f64, nozzle: Option<&str>| Placement {
        object_id: id.into(),
        enabled: true,
        transform: Transform {
            scale: 1.0,
            rotation: 0.0,
            gui_dx: 18.0,
            gui_dy: 11.0,
            cx: 12.5,
            cy: 37.5,
        },
        baked_scale: 1.0,
        nozzle_type: nozzle.map(|s| s.to_string()),
        nozzle_diam: nozzle.map(|_| 0.3),
        path: SubstratePaths::new(vec![PathSegment::new(vec![
            Point2D::new(1.0, y),
            Point2D::new(9.0, y),
        ])]),
    };

    let slides = vec![vec![mk("a", 2.0, None), mk("b", 40.0, Some("Modrá"))]];
    let (gcode, dist, _) =
        generate_gcode(&slides, &params, &HashMap::new(), &machine).expect("generování selhalo");

    // Obě trasy se vytisknou v rámci jednoho sklíčka
    assert_eq!(gcode.matches("; --- VZOREK 1 ---").count(), 1);
    assert!(dist > 15.0, "dist={dist}");
    // Objekt s vlastní tryskou vyvolá výměnu s pauzou pro obsluhu
    assert!(gcode.contains("; --- VYMENA TRYSKY: Modrá (prumer 0.30 mm) ---"));
    assert!(gcode.contains("M0 Vymente trysku za: Modrá"));

    // Skrytý objekt se netiskne
    let mut hidden = slides.clone();
    hidden[0][1].enabled = false;
    let (_, dist_hidden, _) =
        generate_gcode(&hidden, &params, &HashMap::new(), &machine).expect("generování selhalo");
    assert!(dist_hidden < dist, "{dist_hidden} < {dist}");
}

#[test]
fn test_metadata_v2_migrates_to_objects() {
    // Starý soubor bez pole `objects` se načte jako jeden zdroj s jedním
    // objektem na každém sklíčku.
    let meta = GCodeMetadata {
        params: multi_params(2),
        overrides: HashMap::new(),
        sources: vec![],
        objects: vec![],
        slides: vec![],
        transforms: vec![
            Transform { scale: 1.0, rotation: 0.0, gui_dx: 18.0, gui_dy: 11.0, cx: 12.5, cy: 37.5 },
            Transform { scale: 1.0, rotation: 90.0, gui_dx: 18.0, gui_dy: 91.0, cx: 12.5, cy: 37.5 },
        ],
        baked_scales: vec![1.0, 1.5],
        source_file_name: "vzorek.svg".into(),
        source_file_ext: "svg".into(),
        source_file_content: "<svg/>".into(),
        auto_scale: true,
        fineness: 1.0,
    };

    let restored = deserialize_metadata(&serialize_metadata(&meta)).expect("deserializace selhala");

    assert_eq!(restored.sources.len(), 1);
    assert_eq!(restored.sources[0].content, "<svg/>");
    assert!(restored.sources[0].auto_scale);
    assert_eq!(restored.objects.len(), 1);
    assert_eq!(restored.objects[0].index, 0);
    assert_eq!(restored.slides.len(), 2);
    assert_eq!(restored.slides[1][0].object_id, restored.objects[0].id);
    assert!(restored.slides[1][0].enabled);
    assert!((restored.slides[1][0].baked_scale - 1.5).abs() < 1e-9);
    assert!((restored.slides[1][0].transform.rotation - 90.0).abs() < 1e-9);
    assert!(restored.slides[0][0].nozzle_type.is_none());
}

#[test]
fn test_metadata_v3_round_trip_with_objects() {
    let meta = GCodeMetadata {
        params: multi_params(1),
        overrides: HashMap::new(),
        sources: vec![SourceMeta {
            id: "src-1".into(),
            name: "dva.svg".into(),
            ext: "svg".into(),
            content: "<svg/>".into(),
            auto_scale: false,
            fineness: 1.0,
        }],
        objects: vec![
            ObjectMeta { id: "o1".into(), source_id: "src-1".into(), index: 0, name: "dva.svg #1".into() },
            ObjectMeta { id: "o2".into(), source_id: "src-1".into(), index: 1, name: "dva.svg #2".into() },
        ],
        slides: vec![vec![
            PlacementState {
                object_id: "o1".into(),
                enabled: true,
                transform: Transform { scale: 1.0, rotation: 0.0, gui_dx: 18.0, gui_dy: 11.0, cx: 12.5, cy: 37.5 },
                baked_scale: 1.0,
                nozzle_type: None,
                nozzle_diam: None,
            },
            PlacementState {
                object_id: "o2".into(),
                enabled: false,
                transform: Transform { scale: 1.0, rotation: 0.0, gui_dx: 20.0, gui_dy: 11.0, cx: 12.5, cy: 37.5 },
                baked_scale: 1.0,
                nozzle_type: Some("Modrá".into()),
                nozzle_diam: Some(0.41),
            },
        ]],
        transforms: vec![],
        baked_scales: vec![],
        source_file_name: String::new(),
        source_file_ext: String::new(),
        source_file_content: String::new(),
        auto_scale: false,
        fineness: 1.0,
    };

    let restored = deserialize_metadata(&serialize_metadata(&meta)).expect("deserializace selhala");
    assert_eq!(restored.objects.len(), 2);
    assert_eq!(restored.objects[1].index, 1);
    assert_eq!(restored.slides[0].len(), 2);
    // Viditelnost i tryska konkrétního objektu přežijí uložení
    assert!(!restored.slides[0][1].enabled);
    assert_eq!(restored.slides[0][1].nozzle_type.as_deref(), Some("Modrá"));
    assert!((restored.slides[0][1].nozzle_diam.unwrap() - 0.41).abs() < 1e-9);
}

#[test]
fn test_svg_import_to_gcode_end_to_end() {
    // Celá cesta jednoho importu se dvěma tvary: parse → rozdělení na objekty →
    // layout na dvou substrátech → G-kód.
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20mm" height="10mm" viewBox="0 0 20 10">
        <rect x="0" y="0" width="5" height="5" fill="black"/>
        <rect x="15" y="0" width="5" height="5" fill="black"/>
    </svg>"#;

    let parsed = parse_svg(svg, 1.0);
    let parts = split_into_objects(&parsed);
    assert_eq!(parts.len(), 2, "import má obsahovat dva objekty");

    let frame = {
        let pts: Vec<_> = parsed.segments.iter().flat_map(|s| s.points.iter()).collect();
        [
            pts.iter().map(|p| p.x).fold(f64::INFINITY, f64::min),
            pts.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max),
            pts.iter().map(|p| p.y).fold(f64::INFINITY, f64::min),
            pts.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max),
        ]
    };
    let objects: Vec<SceneObject> = parts
        .into_iter()
        .enumerate()
        .map(|(k, raw)| scene_object(&format!("o{k}"), raw, frame))
        .collect();

    // Tvary mají v SVG jen výplň (fill bez stroke) — styl "Okraje" by je podle
    // příznaků ze souboru vůbec nevykreslil, proto výchozí styl s výplní.
    let mut params = multi_params(2);
    params.infill_style = "Okraje + Výplň".into();

    let res = dpi_core::update_layout(
        &params,
        &HashMap::new(),
        &objects,
        &[],
        &[],
        &test_bed(),
        5.0,
    );
    assert_eq!(res.slides.len(), 2);
    assert!(res.slides.iter().all(|row| row.len() == 2));
    assert!(
        res.slides.iter().all(|row| row.iter().all(|p| !p.path.segments.is_empty())),
        "každý objekt má mít vlastní trasu"
    );

    let machine = MachineConfig {
        bed: test_bed(),
        start_gcode: String::new(),
        end_gcode: String::new(),
        loop_start_gcode: String::new(),
        loop_end_gcode: String::new(),
        multi_spacing: 5.0,
        block_height: 34.0,
        calibration_factor: 0.0141,
        z_hop: 2.0,
        safe_z: 20.0,
        bed_max_temp: None,
        skip_z_shift_setup: false,
        nozzle_change_gcode: String::new(),
        print_order: "substrate_major".to_string(),
        route_mode: "fastest".to_string(),
    };
    let (gcode, dist, _) =
        generate_gcode(&res.slides, &params, &HashMap::new(), &machine).expect("generování selhalo");

    assert_eq!(gcode.matches("; --- VZOREK").count(), 2);
    assert!(dist > 0.0);

    // Skrytí jednoho objektu na prvním substrátu zkrátí dráhu, druhý zůstává
    let mut hidden = res.slides.clone();
    hidden[0][0].enabled = false;
    hidden[0][0].path = SubstratePaths::new(vec![]);
    let (_, dist_hidden, _) = generate_gcode(&hidden, &params, &HashMap::new(), &machine).unwrap();
    assert!(dist_hidden < dist && dist_hidden > 0.0, "{dist_hidden} vs {dist}");
}

// ─── Výměna trysky ───────────────────────────────────────────────────────────

fn machine_with(nozzle_change_gcode: &str) -> MachineConfig {
    MachineConfig {
        bed: test_bed(),
        start_gcode: String::new(),
        end_gcode: String::new(),
        loop_start_gcode: String::new(),
        loop_end_gcode: String::new(),
        multi_spacing: 5.0,
        block_height: 34.0,
        calibration_factor: 0.0141,
        z_hop: 2.0,
        safe_z: 20.0,
        bed_max_temp: None,
        skip_z_shift_setup: false,
        nozzle_change_gcode: nozzle_change_gcode.to_string(),
        print_order: "substrate_major".to_string(),
        route_mode: "fastest".to_string(),
    }
}

fn placement_with_nozzle(id: &str, y: f64, nozzle: Option<(&str, f64)>) -> Placement {
    Placement {
        object_id: id.into(),
        enabled: true,
        transform: Transform {
            scale: 1.0,
            rotation: 0.0,
            gui_dx: 18.0,
            gui_dy: 11.0,
            cx: 12.5,
            cy: 37.5,
        },
        baked_scale: 1.0,
        nozzle_type: nozzle.map(|(n, _)| n.to_string()),
        nozzle_diam: nozzle.map(|(_, d)| d),
        path: SubstratePaths::new(vec![PathSegment::new(vec![
            Point2D::new(1.0, y),
            Point2D::new(9.0, y),
        ])]),
    }
}

#[test]
fn test_nozzle_change_emits_pause_homing_and_align_hook() {
    let mut params = multi_params(1);
    params.nozzle_type = "Červená".into();

    // Červená, Modrá, Červená v poli — `machine_with` nastavuje
    // `print_order: "substrate_major"`, který i v rámci jednoho substrátu
    // seskupuje objekty podle trysky (viz schedule::schedule_print), takže
    // "a" a "c" (obě implicitně Červená) se vytisknou před "b" (Modrá) —
    // jediná výměna trysky, ne dvě jako by vyšlo z pevného pořadí pole.
    let slides = vec![vec![
        placement_with_nozzle("a", 2.0, None),
        placement_with_nozzle("b", 20.0, Some(("Modrá", 0.41))),
        placement_with_nozzle("c", 40.0, None),
    ]];

    let (gcode, _, _) = generate_gcode(&slides, &params, &HashMap::new(), &machine_with(""))
        .expect("generování selhalo");

    assert_eq!(gcode.matches("; --- VYMENA TRYSKY").count(), 1);
    assert_eq!(gcode.matches("M0 Vymente trysku za:").count(), 1);
    // Homing je po výměně povinný a za ním následuje úchyt kontroly vyosení
    assert_eq!(gcode.matches("G28 ; Homing po vymene trysky").count(), 1);
    assert!(gcode.contains(&format!("{NOZZLE_ALIGN_MARKER}Modrá")));
    // Pořadí: pauza → homing → kontrola vyosení
    let pause = gcode.find("M0 Vymente trysku za: Modrá").unwrap();
    let homing = gcode[pause..].find("G28").unwrap();
    let align = gcode[pause..].find(NOZZLE_ALIGN_MARKER).unwrap();
    assert!(homing < align, "kontrola vyosení musí následovat až po homingu");
}

#[test]
fn test_print_order_nozzle_major_reduces_nozzle_changes_across_substrates() {
    let mut params = multi_params(2);
    params.nozzle_type = "Červená".into();

    // Prokládané trysky napříč substráty: substrát 0 = Červená,Modrá;
    // substrát 1 = Modrá,Červená. Pevné pořadí pole by dalo 2 výměny;
    // "nozzle_major" by je mělo seskupit na 1 (viz schedule::schedule_print).
    let slides = vec![
        vec![
            placement_with_nozzle("s0-a", 2.0, None),
            placement_with_nozzle("s0-b", 20.0, Some(("Modrá", 0.41))),
        ],
        vec![
            placement_with_nozzle("s1-a", 2.0, Some(("Modrá", 0.41))),
            placement_with_nozzle("s1-b", 20.0, None),
        ],
    ];

    let mut machine = machine_with("");
    machine.print_order = "substrate_major".to_string();
    let (gcode_substrate, _, _) = generate_gcode(&slides, &params, &HashMap::new(), &machine)
        .expect("generování selhalo");

    machine.print_order = "nozzle_major".to_string();
    let (gcode_nozzle, _, _) = generate_gcode(&slides, &params, &HashMap::new(), &machine)
        .expect("generování selhalo");

    let changes_substrate = gcode_substrate.matches("; --- VYMENA TRYSKY").count();
    let changes_nozzle = gcode_nozzle.matches("; --- VYMENA TRYSKY").count();

    assert_eq!(changes_nozzle, 1);
    assert!(
        changes_nozzle < changes_substrate,
        "nozzle_major ({changes_nozzle}) by mělo mít méně výměn než substrate_major ({changes_substrate})"
    );
}

#[test]
fn test_preview_and_gcode_visit_order_agree() {
    // Regresní pojistka: `compute_preview_segments` (náhled na plátně) a
    // `generate_gcode` MUSÍ sdílet stejné pořadí (schedule::schedule_print),
    // jinak se poloha náhledové trysky rozejde se skutečným výstupem.
    let mut params = multi_params(2);
    params.nozzle_type = "Červená".into();

    let slides = vec![
        vec![
            placement_with_nozzle("s0-a", 2.0, None),
            placement_with_nozzle("s0-b", 20.0, Some(("Modrá", 0.41))),
        ],
        vec![
            placement_with_nozzle("s1-a", 2.0, Some(("Modrá", 0.41))),
            placement_with_nozzle("s1-b", 20.0, None),
        ],
    ];

    let mut machine = machine_with("");
    machine.print_order = "nozzle_major".to_string();

    let positions = get_layout_positions(
        params.sample_count,
        params.slide_w,
        params.slide_h,
        machine.multi_spacing,
        params.prime_active,
        None,
        &machine.bed,
    );

    let preview = compute_preview_segments(
        &positions,
        &slides,
        None,
        &params.nozzle_type,
        &machine.print_order,
        &machine.route_mode,
    );
    let mut preview_run_order: Vec<usize> = preview.segs.iter().map(|s| s.slide_idx).collect();
    preview_run_order.dedup();

    let (gcode, _, _) = generate_gcode(&slides, &params, &HashMap::new(), &machine)
        .expect("generování selhalo");
    let gcode_run_order: Vec<usize> = gcode
        .match_indices("; --- VZOREK ")
        .map(|(idx, marker)| {
            let rest = &gcode[idx + marker.len()..];
            let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            num.parse::<usize>().unwrap() - 1
        })
        .collect();

    assert_eq!(preview_run_order, gcode_run_order);
    // Ujistíme se, že fixtura opravdu vynucuje návštěvu substrátu ve více
    // "vlnách" (jinak by test prošel triviálně i s rozjetým pořadím).
    assert!(
        gcode_run_order.len() > 2,
        "fixtura musí vynutit alespoň jedno opětovné navštívení substrátu"
    );
}

#[test]
fn test_no_nozzle_change_when_all_objects_use_global() {
    let mut params = multi_params(1);
    params.nozzle_type = "Červená".into();
    let slides = vec![vec![
        placement_with_nozzle("a", 2.0, None),
        // Explicitní volba stejné trysky jako globální výměnu nevyvolá
        placement_with_nozzle("b", 20.0, Some(("Červená", 0.3))),
    ]];

    let (gcode, _, _) = generate_gcode(&slides, &params, &HashMap::new(), &machine_with(""))
        .expect("generování selhalo");

    assert!(!gcode.contains("VYMENA TRYSKY"));
    assert!(!gcode.contains(NOZZLE_ALIGN_MARKER));
}

#[test]
fn test_nozzle_change_restores_virtual_z_shift() {
    // Nízká tryska → tiskové výšky jdou pod fyzickou nulu → virtuální posun Z.
    // Homing ho zruší, takže ho výměna musí obnovit, jinak by další objekt
    // najel o z_shift níž, tedy do skla.
    let mut params = multi_params(1);
    params.nozzle_type = "Červená".into();
    let z_shift = compute_z_shift(&params, &HashMap::new(), 34.0);
    assert!(z_shift > 0.0, "test potřebuje aktivní posun Z");

    let slides = vec![vec![
        placement_with_nozzle("a", 2.0, None),
        placement_with_nozzle("b", 20.0, Some(("Modrá", 0.41))),
    ]];
    let (gcode, _, _) = generate_gcode(&slides, &params, &HashMap::new(), &machine_with(""))
        .expect("generování selhalo");

    let change = gcode.find("; --- VYMENA TRYSKY").unwrap();
    let after = &gcode[change..];
    assert!(after.contains("Obnoveni virtualniho posunu Z po homingu"));
    assert!(after.contains(&format!("G92 Z{:.3}", 20.0 + z_shift)));
    // Obnova musí přijít až po homingu
    assert!(after.find("G28").unwrap() < after.find("G92 Z").unwrap());
}

#[test]
fn test_nozzle_change_uses_configured_block() {
    let mut params = multi_params(1);
    params.nozzle_type = "Červená".into();
    let slides = vec![vec![placement_with_nozzle("b", 20.0, Some(("Modrá", 0.41)))]];

    let machine = machine_with("G28 X Y\nG28 Z\nM117 Vymeneno");
    let (gcode, _, _) =
        generate_gcode(&slides, &params, &HashMap::new(), &machine).expect("generování selhalo");

    assert!(gcode.contains("G28 X Y\nG28 Z\nM117 Vymeneno"));
    assert!(!gcode.contains("G28 ; Homing po vymene trysky"));
}
