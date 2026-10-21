use dpi_core::*;

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
        z_calibration: 0.0,
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
        gui_dx: 18.0,
        gui_dy: 11.0,
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
        nozzle_change_gcode: String::new(),
        multi_spacing: 5.0,
        block_height: 34.0,
        calibration_factor: 0.0141,
        z_hop: 2.0,
        safe_z: 20.0,
        bed_max_temp: None,
        skip_z_shift_setup: false,
        retract_nl: 0.0,
        retract_speed: 300.0,
        dot_dwell_ms: 0.0,
    };
    let res = generate_gcode(
        &paths,
        &params,
        &transforms,
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
        &paths,
        &params,
        &transforms,
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
        z_calibration: 0.0,
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
        gui_dx: 18.0,
        gui_dy: 11.0,
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
            glass_z: None,
            ..Default::default()
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
        nozzle_change_gcode: String::new(),
        multi_spacing: 5.0,
        block_height: 34.0,
        calibration_factor: 0.0141,
        z_hop: 2.0,
        safe_z: 20.0,
        bed_max_temp: None,
        skip_z_shift_setup: false,
        retract_nl: 0.0,
        retract_speed: 300.0,
        dot_dwell_ms: 0.0,
    };
    let res = generate_gcode(&paths, &params, &transforms, &overrides, &machine);

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
        z_calibration: 0.0,
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
        z_calibration: 0.0,
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
        transforms: vec![],
        baked_scales: vec![1.0, 1.0],
        baked_scales_y: vec![],
        source_file_name: "test.svg".into(),
        source_file_ext: "svg".into(),
        source_file_content: "<svg/>".into(),
        auto_scale: true,
        fineness: 2.5,
    };

    let header = serialize_metadata(&meta);
    assert!(header.contains("; --- EDITOR METADATA ---"));
    assert!(header.contains("; --- END METADATA ---"));
    assert!(header.contains("\"dpi_version\":2"));

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
        z_calibration: 0.0,
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
            glass_z: None,
            ..Default::default()
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
        transforms,
        baked_scales: vec![1.5, 1.0, 0.8],
        baked_scales_y: vec![1.5, 2.0, 0.8],
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
    assert_eq!(restored.baked_scales_y, vec![1.5, 2.0, 0.8]);

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
        ..Default::default()
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
        object_overrides: Default::default(),
        user_scale_y: None,
        deleted_objects: Vec::new(),
        object_transforms: Default::default(),
        object_order: Vec::new(),
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

/// Objekty se tisknou celé a v pořadí seznamu objektů: bez lokálního pořadí
/// podle id (pořadí ve zdroji), s lokálním pořadím sklíčka podle něj.
/// Tři obdélníky jsou zadány v pořadí pravý, střední, levý.
#[test]
fn test_object_print_order() {
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
            ..Default::default()
        }
    };

    // Záměrně v opačném pořadí: pravý (X=21-29), střední (X=11-19), levý (X=1-9)
    let raw = SubstratePaths::new(vec![
        make_rect(21.0, 29.0, 1.0, 8.0),
        make_rect(11.0, 19.0, 1.0, 8.0),
        make_rect(1.0,  9.0,  1.0, 8.0),
    ]);

    let mut params = SliceParams {
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
        object_overrides: Default::default(),
        user_scale_y: None,
        deleted_objects: Vec::new(),
        object_transforms: Default::default(),
        object_order: Vec::new(),
    };

    // Každý infill segment (2 body, horizontální) přiřaď k objektu podle X midpointu.
    // Po normalizaci a centrování (bbox x=1..29 → off_x = 11):
    //   levý obj X = 11..19, střední 21..29, pravý 31..39
    let order_of = |params: &SliceParams| -> Vec<u8> {
        let result = process_substrate_paths(&raw, params);
        let mut groups: Vec<u8> = result
            .segments
            .iter()
            .filter(|s| {
                s.points.len() == 2
                    && (s.points[0].x - s.points[1].x).abs() > 1e-6
                    && (s.points[0].y - s.points[1].y).abs() < 1e-6
            })
            .map(|s| {
                let x_mid = (s.points[0].x + s.points[1].x) / 2.0;
                if x_mid < 20.0 { 0 } else if x_mid < 30.0 { 1 } else { 2 }
            })
            .collect();
        assert!(!groups.is_empty(), "Žádné infill segmenty nenalezeny");
        // Objekt se tiskne celý najednou — žádný návrat k předchozímu
        groups.dedup();
        groups
    };

    // Výchozí pořadí = id (zdroj: pravý 0, střední 1, levý 2)
    assert_eq!(order_of(&params), vec![2, 1, 0], "výchozí pořadí podle id");
    // Lokální pořadí sklíčka: levý, střední, pravý
    params.object_order = vec![2, 1, 0];
    assert_eq!(order_of(&params), vec![0, 1, 2], "lokální pořadí");
    // Neuvedené objekty následují podle id
    params.object_order = vec![1];
    assert_eq!(order_of(&params), vec![1, 2, 0], "částečné pořadí");
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
        PathSegment { points: outer, is_filled: Some(true), has_stroke: Some(false), ..Default::default() },
        PathSegment { points: hole,  is_filled: Some(true), has_stroke: Some(false), ..Default::default() },
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
        object_overrides: Default::default(),
        user_scale_y: None,
        deleted_objects: Vec::new(),
        object_transforms: Default::default(),
        object_order: Vec::new(),
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

    // Řady se procházejí po oblastech: v řetězci jde každá další řada hned nad
    // (pod) předchozí a směr se střídá; přes díru vede nejvýš jeden přeskok
    // (dřív se střídal směr podle pořadí úseku a přes díru se jezdilo 2× na řadu)
    let mut jumps = 0;
    let mut travel = 0.0;
    for w in infill.windows(2) {
        let (end, next) = (*w[0].points.last().unwrap(), w[1].points[0]);
        let d = end.distance(&next);
        travel += d;
        // Přeskok = další úsek neleží v sousední řadě (přechod podél díry
        // sousední řadou může být delší než rozestup)
        if ((next.y - end.y).abs() - params.infill_val).abs() > 1e-6 {
            jumps += 1;
            continue;
        }
        assert!(d < 3.0, "řetězec musí pokračovat sousední řadou: {end:?} → {next:?}");
        let dir = |s: &PathSegment| (s.points[1].x - s.points[0].x).signum();
        assert_ne!(dir(w[0]), dir(w[1]), "směr se v řetězci musí střídat");
    }
    assert!(jumps <= 1, "polygon s dírou: {jumps} přeskoků");
    // Přechody podél okraje (≤ 1 mm, u dna a vršku díry ~2.6 mm) + jeden
    // přeskok k druhé straně díry; dřív 8 přejezdů přes celou díru
    assert!(travel < 30.0, "přejezdy výplně {travel:.1} mm");
    // Každá řada je pokrytá: 6 plných řad + 6 řad protnutých dírou po dvou
    // úsecích (díra je o kompenzaci šířky stopy větší, protíná i krajní řady)
    assert_eq!(infill.len(), 18);
}

/// Přepisy objektu (třetí úroveň nastavení): rychlost, extruze a výška platí jen
/// pro dráhy toho objektu, zbytek skla tiskne s nastavením skla. G-kód nese
/// značky drah pro živý náhled a virtuální posun Z pokryje i výšku objektu.
#[test]
fn test_gcode_object_overrides_and_markers() {
    let seg = |x0: f64, object: u32| PathSegment {
        object: Some(object),
        ..PathSegment::new(vec![Point2D::new(x0, 0.0), Point2D::new(x0 + 10.0, 0.0)])
    };
    let paths = vec![SubstratePaths::new(vec![seg(0.0, 0), seg(0.0, 5)])];
    let params = ProcessParams {
        sample_count: 1,
        prime_active: false,
        slide_w: 25.0,
        slide_h: 75.0,
        slide_z: 1.0,
        z_offset: 0.5,
        z_unit: "mm".to_string(),
        z_calibration: 0.0,
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
        infill_style: "Okraje".to_string(),
        print_speed: 1500.0,
    };
    let transforms = vec![Transform {
        scale: 1.0,
        rotation: 0.0,
        gui_dx: 18.0,
        gui_dy: 11.0,
        cx: 12.5,
        cy: 37.5,
    }];
    let mut slide = SlideOverride::default();
    slide.objects.insert(
        "5".to_string(),
        ObjectOverride {
            print_speed: Some(321.0),
            z_offset: Some(0.2),
            ..Default::default()
        },
    );
    let mut overrides = std::collections::HashMap::new();
    overrides.insert("0".to_string(), slide);
    let machine = MachineConfig {
        bed: BedConfig { max_x: 250.0, max_y: 210.0, min_x: 0.0, offset_x: 18.0, offset_y: 11.0 },
        start_gcode: String::new(),
        end_gcode: String::new(),
        loop_start_gcode: String::new(),
        loop_end_gcode: String::new(),
        nozzle_change_gcode: String::new(),
        multi_spacing: 5.0,
        block_height: 34.0,
        calibration_factor: 0.0141,
        z_hop: 2.0,
        safe_z: 20.0,
        bed_max_temp: None,
        skip_z_shift_setup: false,
        retract_nl: 0.0,
        retract_speed: 300.0,
        dot_dwell_ms: 0.0,
    };
    let (gcode, _, time) =
        generate_gcode(&paths, &params, &transforms, &overrides, &machine).unwrap();

    // Objekt 5 má nižší z_offset → virtuální posun počítá s ním: 6.8 + 1.0
    assert!(gcode.contains("SHIFT 7.80mm"), "{gcode}");
    assert!((compute_z_shift(&params, &overrides, 34.0) - 7.8).abs() < 1e-9);

    let lines: Vec<&str> = gcode.lines().collect();
    let m0 = lines.iter().position(|l| *l == "; DPI_SEG 0 0 0").expect("značka dráhy 0");
    let m1 = lines.iter().position(|l| *l == "; DPI_SEG 0 1 0").expect("značka dráhy 1");
    let prints = |from: usize, to: usize| -> Vec<&str> {
        lines[from..to].iter().copied().filter(|l| l.contains(" E")).collect()
    };
    // Dráha objektu 0: nastavení skla; dráha objektu 5: vlastní rychlost
    assert!(prints(m0, m1).iter().all(|l| l.ends_with("F1500")));
    assert!(prints(m1, lines.len()).iter().all(|l| l.ends_with("F321")));
    // Jiná výška objektu → přejezd se zdvihem a sjezd na výšku objektu (z_offset 0.2)
    // print_z = -34 + 30 - 4 + 1 + 0.2 + 7.8 = 1.0
    assert!(lines[m1..].iter().any(|l| l.starts_with("G1 Z1.000 F1000")), "{gcode}");
    // Čas tisku zohledňuje pomalejší objekt: 10 mm při 321 mm/min ≈ 1.87 s
    assert!(time > 10.0 / 321.0 * 60.0);
}

/// Odplivové sklo s jinou tloušťkou než vzorková skla: každé sklo se tiskne
/// nad svým povrchem a korekce z kalibrace platí pro všechna (i s vlastním z_offsetem).
#[test]
fn test_prime_glass_thickness_and_calibration() {
    let paths = vec![SubstratePaths::new(vec![PathSegment::new(vec![
        Point2D::new(0.0, 0.0),
        Point2D::new(10.0, 0.0),
    ])])];
    let mut params = ProcessParams {
        sample_count: 1,
        prime_active: true,
        slide_w: 25.0,
        slide_h: 75.0,
        slide_z: 2.2,
        z_offset: 0.2,
        z_unit: "mm".to_string(),
        z_calibration: 0.05,
        nozzle_height: 40.0,
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
        infill_style: "Okraje".to_string(),
        print_speed: 1500.0,
    };
    let transforms = vec![Transform {
        scale: 1.0,
        rotation: 0.0,
        gui_dx: 16.0,
        gui_dy: 43.0,
        cx: 12.5,
        cy: 37.5,
    }];
    let mut overrides = std::collections::HashMap::new();
    overrides.insert(
        "-1".to_string(),
        SlideOverride {
            glass_type: Some("laboratorní".into()),
            glass_z: Some(1.0),
            ..Default::default()
        },
    );
    overrides.insert(
        "0".to_string(),
        SlideOverride { z_offset: Some(0.3), ..Default::default() },
    );
    let machine = MachineConfig {
        bed: BedConfig { max_x: 250.0, max_y: 210.0, min_x: 0.0, offset_x: 16.0, offset_y: 12.0 },
        start_gcode: String::new(),
        end_gcode: String::new(),
        loop_start_gcode: String::new(),
        loop_end_gcode: String::new(),
        nozzle_change_gcode: String::new(),
        multi_spacing: 5.0,
        block_height: 34.0,
        calibration_factor: 0.0141,
        z_hop: 2.0,
        safe_z: 20.0,
        bed_max_temp: None,
        skip_z_shift_setup: false,
        retract_nl: 0.0,
        retract_speed: 300.0,
        dot_dwell_ms: 0.0,
    };
    let (gcode, _, _) = generate_gcode(&paths, &params, &transforms, &overrides, &machine).unwrap();
    assert!(!gcode.contains("SHIFT"), "{gcode}");

    let (prime, sample) = gcode.split_once("; --- VZOREK 1 ---").unwrap();
    // Odpliv: -34 + 40 - 4 + 1.0 (odplivové sklo) + 0.2 + 0.05 = 3.25
    assert!(prime.lines().any(|l| l.starts_with("G1 Z3.250 F1000")), "{prime}");
    // Vzorek: -34 + 40 - 4 + 2.2 (vzorkové sklo) + 0.3 (lokální) + 0.05 = 4.55
    assert!(sample.lines().any(|l| l.starts_with("G1 Z4.550")), "{sample}");

    // Kalibrace najíždí na odplivové sklo → teoretický povrch s jeho tloušťkou
    assert!((calibration_glass_z(&params, &overrides, 34.0) - 3.0).abs() < 1e-9);
    params.prime_active = false;
    assert!((calibration_glass_z(&params, &overrides, 34.0) - 4.2).abs() < 1e-9);

    // Virtuální posun Z musí pokrýt i (nižší) odplivové sklo:
    // odpliv -34 + 30 - 4 + 1.0 + 0.2 + 0.05 = -6.75 → shift 7.75
    params.prime_active = true;
    params.nozzle_height = 30.0;
    assert!((compute_z_shift(&params, &overrides, 34.0) - 7.75).abs() < 1e-9);
}

fn rect_seg(x0: f64, y0: f64, x1: f64, y1: f64, group: u32) -> PathSegment {
    PathSegment {
        points: vec![
            Point2D::new(x0, y0),
            Point2D::new(x1, y0),
            Point2D::new(x1, y1),
            Point2D::new(x0, y1),
            Point2D::new(x0, y0),
        ],
        is_filled: Some(true),
        has_stroke: Some(true),
        group: Some(group),
        ..Default::default()
    }
}

fn slice_params(slide_w: f64, slide_h: f64) -> SliceParams {
    SliceParams {
        slide_w,
        slide_h,
        margin: 1.0,
        auto_scale: false,
        infill_style: "Okraje".to_string(),
        infill_val: 1.0,
        infill_type: "mm".to_string(),
        infill_angle: 0.0,
        nozzle_diam: 0.4,
        user_scale: 1.0,
        user_scale_y: None,
        object_overrides: Default::default(),
        deleted_objects: Vec::new(),
        object_transforms: Default::default(),
        object_order: Vec::new(),
    }
}

/// Smazaný objekt (ze sklíčka i ze sdílené kresby) se netiskne, zbylé objekty
/// si ponechají id i polohu.
#[test]
fn test_deleted_objects_removed_with_stable_ids() {
    let raw = SubstratePaths::new(vec![rect_seg(0.0, 0.0, 5.0, 5.0, 0), rect_seg(10.0, 0.0, 15.0, 5.0, 1)]);
    let full = process_substrate_paths(&raw, &slice_params(30.0, 20.0));
    let ids: Vec<u32> = full.objects.iter().map(|o| o.id).collect();
    assert_eq!(ids, vec![0, 1]);

    let mut p = slice_params(30.0, 20.0);
    p.deleted_objects = vec![1];
    let part = process_substrate_paths(&raw, &p);
    assert_eq!(part.objects.len(), 1);
    assert_eq!(part.objects[0].id, 0);
    assert_eq!(part.objects[0].bounds, full.objects[0].bounds, "zbytek se nesmí posunout");
    assert!(part.segments.iter().all(|s| s.object == Some(0)));

    // Smazání ze sdílené kresby (SVG kořenový atribut) funguje stejně
    let raw_global = SubstratePaths { deleted_objects: vec![0], ..raw.clone() };
    let g = process_substrate_paths(&raw_global, &slice_params(30.0, 20.0));
    assert_eq!(g.objects.iter().map(|o| o.id).collect::<Vec<_>>(), vec![1]);

    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10mm" height="10mm" data-dpi-deleted-objects="3, 7"><rect x="1" y="1" width="2" height="2"/></svg>"#;
    assert_eq!(parse_svg(svg, 1.0).deleted_objects, vec![3, 7]);
}

/// Nerovnoměrné zapečené měřítko (odemčený poměr stran) škáluje osy zvlášť.
#[test]
fn test_user_scale_y() {
    let raw = SubstratePaths::new(vec![rect_seg(0.0, 0.0, 4.0, 4.0, 0)]);
    let mut p = slice_params(30.0, 20.0);
    p.user_scale_y = Some(2.0);
    let out = process_substrate_paths(&raw, &p);
    let b = out.objects[0].bounds;
    assert!(((b[1] - b[0]) - 4.0).abs() < 1e-6, "šířka {:?}", b);
    assert!(((b[3] - b[2]) - 8.0).abs() < 1e-6, "výška {:?}", b);
}

/// Část trasy mimo sklo se ořízne — v G-kódu ani v náhledu není.
#[test]
fn test_clip_paths_to_slide() {
    let pos = LayoutPosition { x: 0.0, y: 0.0, width: 10.0, height: 10.0, is_prime: false };
    let t = Transform { scale: 1.0, rotation: 0.0, gui_dx: 0.0, gui_dy: 0.0, cx: 5.0, cy: 5.0 };

    let inside = SubstratePaths::new(vec![PathSegment::new(vec![Point2D::new(1.0, 1.0), Point2D::new(9.0, 9.0)])]);
    assert!(matches!(clip_paths_to_slide(&inside, &t, &pos), std::borrow::Cow::Borrowed(_)));

    let paths = SubstratePaths::new(vec![
        // Čára přes celé sklo → jen vnitřní část
        PathSegment::new(vec![Point2D::new(-5.0, 5.0), Point2D::new(15.0, 5.0)]),
        // Celá mimo → zmizí
        PathSegment::new(vec![Point2D::new(20.0, 20.0), Point2D::new(30.0, 20.0)]),
        // Uzavřený čtverec začínající uvnitř, pravý okraj mimo → jedna spojitá dráha
        rect_seg(2.0, 2.0, 14.0, 8.0, 0),
    ]);
    let c = clip_paths_to_slide(&paths, &t, &pos);
    assert_eq!(c.segments.len(), 2, "{:?}", c.segments);
    let line = &c.segments[0].points;
    assert_eq!((line[0], line[1]), (Point2D::new(0.0, 5.0), Point2D::new(10.0, 5.0)));
    let sq = &c.segments[1];
    assert_eq!(sq.group, Some(0));
    assert_eq!(sq.points.first(), Some(&Point2D::new(10.0, 8.0)));
    assert_eq!(sq.points.last(), Some(&Point2D::new(10.0, 2.0)));
    for s in &c.segments {
        assert!(s.points.iter().all(|p| p.x >= 0.0 && p.x <= 10.0 && p.y >= 0.0 && p.y <= 10.0));
    }

    // Posunutá a otočená trasa: všechny body výstupu leží po transformaci na skle
    let t2 = Transform { scale: 1.5, rotation: 30.0, gui_dx: 4.0, gui_dy: -2.0, cx: 5.0, cy: 5.0 };
    let c2 = clip_paths_to_slide(&paths, &t2, &pos);
    let (s, r) = (t2.scale, (-t2.rotation).to_radians());
    for seg in &c2.segments {
        for p in &seg.points {
            let (dx, dy) = ((p.x - t2.cx) * s, (p.y - t2.cy) * s);
            let wx = t2.gui_dx + t2.cx + dx * r.cos() - dy * r.sin();
            let wy = t2.gui_dy + t2.cy + dx * r.sin() + dy * r.cos();
            assert!(wx > -1e-6 && wx < 10.0 + 1e-6 && wy > -1e-6 && wy < 10.0 + 1e-6, "({wx}, {wy})");
        }
    }
}

/// G-kód netiskne mimo sklo, i když je trasa částečně vysunutá.
#[test]
fn test_gcode_clips_outside_slide() {
    let paths = vec![SubstratePaths::new(vec![PathSegment::new(vec![
        Point2D::new(-10.0, 30.0),
        Point2D::new(40.0, 30.0),
    ])])];
    let params = ProcessParams {
        sample_count: 1,
        prime_active: false,
        slide_w: 25.0,
        slide_h: 75.0,
        slide_z: 1.0,
        z_offset: 0.2,
        z_unit: "mm".to_string(),
        z_calibration: 0.0,
        nozzle_height: 40.0,
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
        infill_style: "Okraje".to_string(),
        print_speed: 1500.0,
    };
    let machine = MachineConfig {
        bed: BedConfig { max_x: 250.0, max_y: 210.0, min_x: 0.0, offset_x: 16.0, offset_y: 12.0 },
        start_gcode: String::new(),
        end_gcode: String::new(),
        loop_start_gcode: String::new(),
        loop_end_gcode: String::new(),
        nozzle_change_gcode: String::new(),
        multi_spacing: 5.0,
        block_height: 34.0,
        calibration_factor: 0.0141,
        z_hop: 2.0,
        safe_z: 20.0,
        bed_max_temp: None,
        skip_z_shift_setup: false,
        retract_nl: 0.0,
        retract_speed: 300.0,
        dot_dwell_ms: 0.0,
    };
    // Sklo začíná na (16, 12), trasa bez posunu → X tisku 6..56, sklo 16..41
    let transforms = vec![Transform { scale: 1.0, rotation: 0.0, gui_dx: 16.0, gui_dy: 12.0, cx: 12.5, cy: 37.5 }];
    let (gcode, dist, _) = generate_gcode(&paths, &params, &transforms, &Default::default(), &machine).unwrap();
    let xs: Vec<f64> = gcode
        .lines()
        .filter(|l| l.starts_with("G1 X") || l.starts_with("G0 X"))
        .filter_map(|l| parse_axis_value(l, 'X'))
        .collect();
    assert!(!xs.is_empty());
    assert!(xs.iter().all(|&x| (16.0 - 1e-6..=41.0 + 1e-6).contains(&x)), "{xs:?}");
    assert!((dist - 25.0).abs() < 1e-6, "dist={dist}");
}

fn line_seg(x0: f64, y0: f64, x1: f64, y1: f64, anchors: Vec<Anchor>) -> PathSegment {
    PathSegment {
        points: vec![Point2D::new(x0, y0), Point2D::new(x1, y1)],
        is_filled: Some(false),
        has_stroke: Some(true),
        anchors,
        ..Default::default()
    }
}

fn xform(dx: f64, dy: f64, scale: f64, rotation: f64) -> ObjectTransform {
    ObjectTransform { dx, dy, scale, scale_y: None, rotation }
}

/// Úprava objektu posune jen jeho, ostatní objekty i id zůstanou; posun je
/// v souřadnicích kresby, zapečené měřítko trasy ho škáluje s ostatními.
#[test]
fn test_object_transform_moves_only_object() {
    let raw = SubstratePaths::new(vec![rect_seg(0.0, 0.0, 4.0, 4.0, 0), rect_seg(10.0, 0.0, 14.0, 4.0, 1)]);
    let base = process_substrate_paths(&raw, &slice_params(40.0, 20.0));

    let mut p = slice_params(40.0, 20.0);
    p.object_transforms.insert("0".into(), xform(3.0, 2.0, 1.0, 0.0));
    let out = process_substrate_paths(&raw, &p);
    let (a0, a1) = (base.objects[0].bounds, out.objects.iter().find(|o| o.id == 0).unwrap().bounds);
    assert!((a1[0] - a0[0] - 3.0).abs() < 1e-6 && (a1[2] - a0[2] - 2.0).abs() < 1e-6, "{a0:?} → {a1:?}");
    let b1 = out.objects.iter().find(|o| o.id == 1).unwrap();
    assert_eq!(b1.bounds, base.objects[1].bounds, "druhý objekt se nesmí pohnout");
    assert_eq!(out.objects.iter().find(|o| o.id == 0).unwrap().base_bounds, Some(a0));

    // Sdílená úprava z kresby platí, vlastní úprava sklíčka má přednost
    let shared = SubstratePaths {
        object_transforms: [("0".to_string(), xform(-5.0, 0.0, 1.0, 0.0))].into(),
        ..raw.clone()
    };
    let s = process_substrate_paths(&shared, &slice_params(40.0, 20.0));
    assert!((s.objects[0].bounds[0] - a0[0] + 5.0).abs() < 1e-6);
    let o = process_substrate_paths(&shared, &p);
    assert!((o.objects.iter().find(|o| o.id == 0).unwrap().bounds[0] - a1[0]).abs() < 1e-6);

    // Zapečené měřítko trasy 2× → posun objektu 2×
    p.user_scale = 2.0;
    let scaled_base = process_substrate_paths(&raw, &SliceParams { user_scale: 2.0, ..slice_params(40.0, 20.0) });
    let scaled = process_substrate_paths(&raw, &p);
    let d = scaled.objects.iter().find(|o| o.id == 0).unwrap().bounds[0] - scaled_base.objects[0].bounds[0];
    assert!((d - 6.0).abs() < 1e-6, "posun {d}");

    // Měřítko a rotace kolem středu objektu
    let mut p = slice_params(40.0, 20.0);
    p.object_transforms.insert("1".into(), xform(0.0, 0.0, 0.5, 90.0));
    let r = process_substrate_paths(&raw, &p);
    let (b0, b1) = (base.objects[1].bounds, r.objects.iter().find(|o| o.id == 1).unwrap().bounds);
    let c = |b: [f64; 4]| ((b[0] + b[1]) / 2.0, (b[2] + b[3]) / 2.0);
    assert!((c(b0).0 - c(b1).0).abs() < 1e-6 && (c(b0).1 - c(b1).1).abs() < 1e-6);
    assert!(((b1[1] - b1[0]) - 2.0).abs() < 1e-6, "{b1:?}");
}

/// Konec čáry přichycený magnetem k objektu se drží objektu i po jeho úpravě.
#[test]
fn test_anchored_line_follows_object() {
    // Zdroj má Y dolů; konec čáry leží na pravé hraně čtverce
    let anchor = Anchor { end: 0, object: 0, x: 4.0, y: 2.0 };
    let raw = SubstratePaths::new(vec![
        rect_seg(0.0, 0.0, 4.0, 4.0, 0),
        line_seg(4.0, 2.0, 12.0, 2.0, vec![anchor]),
    ]);
    let base = process_substrate_paths(&raw, &slice_params(40.0, 20.0));
    let line = |out: &SubstratePaths| out.segments.iter().find(|s| s.object == Some(1)).unwrap().points.clone();
    let l0 = line(&base);

    let mut p = slice_params(40.0, 20.0);
    p.object_transforms.insert("0".into(), xform(1.0, 3.0, 1.0, 0.0));
    let l1 = line(&process_substrate_paths(&raw, &p));
    let (s0, s1) = (l0.iter().find(|q| (q.x - l0[0].x).abs() + (q.y - l0[0].y).abs() < 20.0).unwrap(), &l1);
    // Přichycený konec se posunul s objektem, volný konec zůstal
    let near = |pts: &[Point2D], q: Point2D| pts.iter().any(|r| r.distance(&q) < 1e-6);
    let start0 = *[l0[0], l0[1]].iter().min_by(|a, b| a.x.partial_cmp(&b.x).unwrap()).unwrap();
    let free0 = *[l0[0], l0[1]].iter().max_by(|a, b| a.x.partial_cmp(&b.x).unwrap()).unwrap();
    assert!(near(s1, Point2D::new(start0.x + 1.0, start0.y + 3.0)), "{s0:?} → {s1:?}");
    assert!(near(s1, free0));

    // Kotvy se čtou ze SVG
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 10" width="20mm" height="10mm" data-dpi-object-transforms="0:1,2,1.5,,30;4:0,0,1,2,0"><rect x="1" y="1" width="2" height="2"/><line x1="3" y1="2" x2="9" y2="2" data-dpi-anchor-start="0,3,2"/></svg>"#;
    let parsed = parse_svg(svg, 1.0);
    let ln = parsed.segments.iter().find(|s| !s.anchors.is_empty()).unwrap();
    assert_eq!(ln.anchors, vec![Anchor { end: 0, object: 0, x: 3.0, y: 2.0 }]);
    assert_eq!(parsed.object_transforms["0"], ObjectTransform { dx: 1.0, dy: 2.0, scale: 1.5, scale_y: None, rotation: 30.0 });
    assert_eq!(parsed.object_transforms["4"].scale_y, Some(2.0));
}

/// Objekty spojené čarou se tisknou za sebou a čára je spojí bez přejezdu:
/// okraj prvního končí v bodě napojení, druhý v něm začíná.
#[test]
fn test_connected_objects_print_continuously() {
    let raw = SubstratePaths::new(vec![
        rect_seg(0.0, 0.0, 4.0, 4.0, 0),
        rect_seg(20.0, 0.0, 24.0, 4.0, 1),
        // Třetí objekt mezi nimi — bez spojení by se tiskl v pořadí id
        rect_seg(10.0, 10.0, 14.0, 14.0, 2),
        line_seg(4.0, 2.0, 20.0, 2.0, vec![
            Anchor { end: 0, object: 0, x: 4.0, y: 2.0 },
            Anchor { end: 1, object: 1, x: 20.0, y: 2.0 },
        ]),
    ]);
    let out = process_substrate_paths(&raw, &slice_params(40.0, 30.0));
    let objs: Vec<Option<u32>> = out.segments.iter().map(|s| s.object).collect();
    // Komponenta {0, čára, 1} v pořadí prvního objektu, pak objekt 2
    assert_eq!(objs.len(), 4, "{objs:?}");
    assert_eq!(objs[3], Some(2), "{objs:?}");
    assert_eq!(objs[1], Some(3), "čára uprostřed: {objs:?}");
    for w in out.segments[..3].windows(2) {
        let (a, b) = (*w[0].points.last().unwrap(), w[1].points[0]);
        assert!(a.distance(&b) < 1e-6, "přejezd {a:?} → {b:?}");
    }
    // Uzavřené okraje v bodě napojení — generátor je nesmí otáčet
    assert!(out.segments[0].fixed_start && out.segments[2].fixed_start);
}

/// Začátek hada lze zvolit v rohu obálky objektu; dráhy hada jsou označené.
#[test]
fn test_snake_start_corner() {
    let raw = SubstratePaths::new(vec![rect_seg(0.0, 0.0, 10.0, 6.0, 0)]);
    let mut p = slice_params(30.0, 20.0);
    p.infill_style = "Had".into();
    p.infill_val = 1.0;
    let b = process_substrate_paths(&raw, &p).objects[0].bounds;
    for (corner, at) in [
        ("bl", (b[0], b[2])),
        ("br", (b[1], b[2])),
        ("tl", (b[0], b[3])),
        ("tr", (b[1], b[3])),
    ] {
        p.object_overrides.insert(
            "0".into(),
            ObjectOverride { snake_start: Some(corner.into()), ..Default::default() },
        );
        let out = process_substrate_paths(&raw, &p);
        assert!(out.segments.iter().all(|s| s.snake));
        let s = out.segments[0].points[0];
        // Řady začínají půl rozestupu od okraje
        assert!(s.distance(&Point2D::new(at.0, at.1)) < 1.0, "{corner}: start {s:?}, roh {at:?}");
    }
}

/// Dva hadi spojení čarou přichycenou na jejich konce se tisknou jedním tahem:
/// had → čára → had (jako drátový odpor s přívodem).
#[test]
fn test_connected_snakes_print_continuously() {
    let rects = vec![rect_seg(0.0, 0.0, 6.0, 4.0, 0), rect_seg(14.0, 0.0, 20.0, 4.0, 1)];
    let mut p = slice_params(40.0, 20.0);
    p.infill_style = "Had".into();
    p.infill_val = 1.0;
    let first = process_substrate_paths(&SubstratePaths::new(rects.clone()), &p);
    let ends = |id: u32| -> Vec<Point2D> {
        first
            .segments
            .iter()
            .filter(|s| s.object == Some(id))
            .flat_map(|s| [s.points[0], *s.points.last().unwrap()])
            .collect()
    };
    // Konec hada A nejblíž B a konec hada B nejblíž A (lokální → zdrojové souřadnice)
    let pick = |pts: Vec<Point2D>, right: bool| {
        pts.into_iter()
            .max_by(|a, b| if right { a.x.partial_cmp(&b.x) } else { b.x.partial_cmp(&a.x) }.unwrap())
            .unwrap()
    };
    let (a, b) = (pick(ends(0), true), pick(ends(1), false));
    let to_raw = |q: Point2D| Point2D::new(q.x - 10.0, 4.0 - (q.y - 8.0));
    let (ra, rb) = (to_raw(a), to_raw(b));
    let mut segs = rects;
    segs.push(line_seg(ra.x, ra.y, rb.x, rb.y, vec![
        Anchor { end: 0, object: 0, x: ra.x, y: ra.y },
        Anchor { end: 1, object: 1, x: rb.x, y: rb.y },
    ]));
    let out = process_substrate_paths(&SubstratePaths::new(segs), &p);
    let printed: Vec<&PathSegment> = out.segments.iter().collect();
    assert_eq!(printed.len(), 3, "jeden had na objekt + čára");
    assert_eq!(printed[1].object, Some(2));
    for w in printed.windows(2) {
        let (x, y) = (*w[0].points.last().unwrap(), w[1].points[0]);
        assert!(x.distance(&y) < 1e-6, "přejezd {x:?} → {y:?}");
    }
}

/// Výplň v procentech zachová rozměry tvaru: krajní řady leží na okrajích
/// (zúžených o polovinu trysky — kompenzace šířky stopy), čtverec dá čtvercovou
/// trasu (rozestup se rovnoměrně zmenší). Výplň v mm začíná dál půl rozestupu
/// od okraje.
#[test]
fn test_percent_infill_keeps_shape_extent() {
    let raw = SubstratePaths::new(vec![rect_seg(0.0, 0.0, 10.0, 10.0, 0)]);
    let extent = |style: &str, typ: &str, val: f64| -> [f64; 4] {
        let mut p = slice_params(30.0, 20.0);
        p.infill_style = style.into();
        p.infill_type = typ.into();
        p.infill_val = val;
        let out = process_substrate_paths(&raw, &p);
        let pts: Vec<Point2D> = out.segments.iter().flat_map(|s| s.points.clone()).collect();
        let f = |g: fn(&Point2D) -> f64, max: bool| {
            pts.iter().map(g).fold(if max { f64::MIN } else { f64::MAX }, |a, b| if max { a.max(b) } else { a.min(b) })
        };
        [f(|p| p.x, false), f(|p| p.x, true), f(|p| p.y, false), f(|p| p.y, true)]
    };
    for style in ["Výplň", "Had", "Mřížka", "Tečky"] {
        // 0.4 mm tryska, 50 % → rozestup 0.8 mm; 10 mm se nedělí beze zbytku
        let e = extent(style, "%", 50.0);
        let (w, h) = (e[1] - e[0], e[3] - e[2]);
        assert!((w - 9.6).abs() < 1e-3 && (h - 9.6).abs() < 1e-3, "{style}: {w} × {h}");
    }
    let e = extent("Výplň", "mm", 0.8);
    assert!(((e[3] - e[2]) - 9.6).abs() > 0.1, "mm výplň se nemění: {e:?}");
}

// ─── Tisk kapalin: rozměry výplně, přejezdy, odpliv, kapky ───────────────────

/// Typ "počet" zachová rozměry tvaru: N řad, krajní na okrajích zúžených
/// o polovinu trysky (dřív začínaly půl rozestupu od okraje a výplň měla jen
/// h·(N−1)/N).
#[test]
fn test_count_infill_keeps_shape_extent() {
    let raw = SubstratePaths::new(vec![rect_seg(0.0, 0.0, 10.0, 6.0, 0)]);
    for style in ["Výplň", "Had", "Tečky"] {
        for n in [2.0, 3.0, 5.0, 10.0] {
            let mut p = slice_params(30.0, 20.0);
            p.infill_style = style.into();
            p.infill_type = "počet".into();
            p.infill_val = n;
            let out = process_substrate_paths(&raw, &p);
            let pts: Vec<Point2D> = out.segments.iter().flat_map(|s| s.points.clone()).collect();
            let (min_y, max_y) = pts.iter().fold((f64::MAX, f64::MIN), |(a, b), q| (a.min(q.y), b.max(q.y)));
            let (min_x, max_x) = pts.iter().fold((f64::MAX, f64::MIN), |(a, b), q| (a.min(q.x), b.max(q.x)));
            assert!(((max_y - min_y) - 5.6).abs() < 1e-3, "{style} {n}: výška {}", max_y - min_y);
            assert!(((max_x - min_x) - 9.6).abs() < 1e-3, "{style} {n}: šířka {}", max_x - min_x);
            if style != "Had" {
                let mut ys: Vec<f64> = pts.iter().map(|q| q.y).collect();
                ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
                ys.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
                assert_eq!(ys.len(), n as usize, "{style}: počet řad");
            }
        }
    }
    // Tečky u kruhu: krajní tečky na okrajích úseků řad, ne na mřížce obálky
    // (ta by u kruhu padla mimo tvar a nevznikla by žádná tečka)
    let circle: Vec<Point2D> = (0..=64)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / 64.0;
            Point2D::new(4.0 * a.cos(), 4.0 * a.sin())
        })
        .collect();
    let raw = SubstratePaths::new(vec![PathSegment {
        points: circle,
        is_filled: Some(true),
        has_stroke: Some(false),
        ..Default::default()
    }]);
    let mut p = slice_params(30.0, 20.0);
    p.infill_style = "Tečky".into();
    p.infill_type = "počet".into();
    for n in [2.0, 5.0, 10.0] {
        p.infill_val = n;
        let out = process_substrate_paths(&raw, &p);
        let (min_x, max_x) =
            out.segments.iter().map(|s| s.points[0].x).fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(x), b.max(x)));
        assert!(!out.segments.is_empty(), "počet {n}: žádné tečky");
        if n > 2.0 {
            // Průměr 8 mm zúžený kompenzací šířky stopy na 7.6 mm
            assert!(max_x - min_x > 7.5, "počet {n}: šířka teček {}", max_x - min_x);
        }
    }
}

fn liquid_params(style: &str) -> ProcessParams {
    ProcessParams {
        sample_count: 1,
        prime_active: false,
        slide_w: 25.0,
        slide_h: 75.0,
        slide_z: 1.0,
        z_offset: 0.2,
        z_unit: "mm".into(),
        z_calibration: 0.0,
        nozzle_height: 30.0,
        nozzle_hidden: 4.0,
        filament_diameter: 9.5,
        flow_multiplier: 1.0,
        bed_temp: 0.0,
        extrusion_rate: 200.0,
        extrusion_unit: "nl/mm".into(),
        nozzle_diam: 0.4,
        infill_style: style.into(),
        infill_val: 1.0,
        infill_type: "mm".into(),
        infill_angle: 0.0,
        print_speed: 600.0,
        nozzle_type: String::new(),
    }
}

fn liquid_machine() -> MachineConfig {
    MachineConfig {
        bed: BedConfig { max_x: 250.0, max_y: 210.0, min_x: 0.0, offset_x: 18.0, offset_y: 11.0 },
        start_gcode: "G28\n".into(),
        end_gcode: String::new(),
        loop_start_gcode: String::new(),
        loop_end_gcode: String::new(),
        nozzle_change_gcode: String::new(),
        multi_spacing: 5.0,
        block_height: 34.0,
        calibration_factor: 0.323877,
        z_hop: 2.0,
        safe_z: 20.0,
        bed_max_temp: None,
        skip_z_shift_setup: false,
        retract_nl: 0.0,
        retract_speed: 300.0,
        dot_dwell_ms: 0.0,
    }
}

fn liquid_gcode(
    raw: &SubstratePaths,
    params: &ProcessParams,
    overrides: &std::collections::HashMap<String, SlideOverride>,
    machine: &MachineConfig,
) -> (String, f64) {
    let lay = update_layout(params, overrides, Some(raw), false, &[], &[], &[], &[], &machine.bed, 5.0);
    let (gcode, _, time) = generate_gcode(&lay.paths, params, &lay.transforms, overrides, machine).unwrap();
    (gcode, time)
}

/// Výška Z po řádcích (None = neznámá) — jednoduchá simulace absolutního G-kódu.
fn z_trace(gcode: &str) -> Vec<(Option<f64>, &str)> {
    let mut z = None;
    gcode
        .lines()
        .map(|l| {
            let c = l.split(';').next().unwrap().trim();
            if c.starts_with("G28") {
                z = None;
            } else if c.starts_with("G92") || is_linear_move(c) {
                if let Some(v) = parse_axis_value(c, 'Z') {
                    z = Some(v);
                }
            }
            (z, l)
        })
        .collect()
}

/// Před přejezdem se nikdy nesjíždí: tryska přejíždí aspoň z_hop nad poslední
/// tiskovou výškou, i když je další sklo tenčí (dřív sjela pod povrch
/// tlustšího skla ještě nad ním) — a nad tenčím sklem až pak sjede.
#[test]
fn test_travel_never_descends_below_previous_glass() {
    let raw = SubstratePaths::new(vec![rect_seg(0.0, 0.0, 10.0, 6.0, 0)]);
    let mut params = liquid_params("Okraje");
    params.sample_count = 2;
    params.prime_active = true;
    let mut overrides = std::collections::HashMap::new();
    // Tlusté odplivové sklo (4.5 mm), vzorky 1 mm
    overrides.insert("-1".to_string(), SlideOverride { glass_z: Some(4.5), ..Default::default() });
    let machine = liquid_machine();
    let (gcode, _) = liquid_gcode(&raw, &params, &overrides, &machine);

    let mut last_print_z: Option<f64> = None;
    let mut travels = 0;
    for (z, l) in z_trace(&gcode) {
        let c = l.split(';').next().unwrap().trim();
        if is_extrusion_move(c) && parse_axis_value(c, 'X').is_some() {
            last_print_z = z;
        }
        if c.starts_with("G0 X") && !l.contains("mokre") {
            travels += 1;
            if let (Some(z), Some(pz)) = (z, last_print_z) {
                assert!(z >= pz + machine.z_hop - 1e-6, "přejezd ve výšce {z} po tisku ve {pz}: {l}");
            }
        }
    }
    assert!(travels >= 3, "{gcode}");
    // Žádný nulový pohyb Z (zdvih na výšku, ve které tryska už je)
    let trace = z_trace(&gcode);
    for w in trace.windows(2) {
        let c = w[1].1.split(';').next().unwrap().trim();
        if c.starts_with("G1 Z") {
            assert_ne!(w[0].0, w[1].0, "nulový pohyb Z: {}", w[1].1);
        }
    }
}

/// Spojitá výplň se uvnitř tvaru nezvedá: přechody mezi řadami vedou po mokré
/// ploše (dřív proporcionální mikro-zdvihy 0.001–0.2 mm, které kapalinu
/// neodtrhnou). Přes díru se přejíždí se zdvihem.
#[test]
fn test_fill_rows_travel_over_wet_area() {
    let circle: Vec<Point2D> = (0..=64)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / 64.0;
            Point2D::new(5.0 * a.cos(), 5.0 * a.sin())
        })
        .collect();
    let raw = SubstratePaths::new(vec![PathSegment {
        points: circle,
        is_filled: Some(true),
        has_stroke: Some(false),
        ..Default::default()
    }]);
    let (gcode, _) = liquid_gcode(&raw, &liquid_params("Výplň"), &Default::default(), &liquid_machine());
    let sample = gcode.split_once("; --- VZOREK 1 ---").unwrap().1;
    let lines: Vec<&str> = sample.lines().collect();
    let first = lines.iter().position(|l| l.contains(" E")).unwrap();
    let last = lines.iter().rposition(|l| l.contains(" E")).unwrap();
    assert!(
        !lines[first..last].iter().any(|l| l.starts_with("G1 Z")),
        "výplň kruhu bez zdvihů: {sample}"
    );
    assert!(sample.contains("Prejezd po mokre plose"), "{sample}");

    // Obdélník s dírou: přes díru se jede se zdvihem
    let mut hole = rect_seg(3.0, 2.0, 7.0, 4.0, 0);
    hole.points.reverse();
    let raw = SubstratePaths::new(vec![rect_seg(0.0, 0.0, 10.0, 6.0, 0), hole]);
    let (gcode, _) = liquid_gcode(&raw, &liquid_params("Výplň"), &Default::default(), &liquid_machine());
    let sample = gcode.split_once("; --- VZOREK 1 ---").unwrap().1;
    for (z, l) in z_trace(sample) {
        if l.contains("mokre") {
            // Mokrý přejezd zůstává v tiskové výšce (1.0)
            assert!((z.unwrap() - 1.0).abs() < 1e-6, "{l}");
        }
    }
    assert!(sample.matches("Z-hop pro prejezd").count() >= 2, "přes díru se zdvihem: {sample}");
}

/// Jedna linka: obdélník s dírou (jinak dvě větve hada a přejezd přes díru)
/// se tiskne jedinou dráhou — v G-kódu mezi první a poslední dávkou žádný
/// přejezd, zdvih ani retrakce.
#[test]
fn test_oneline_prints_without_travel() {
    let mut hole = rect_seg(3.0, 2.0, 7.0, 4.0, 0);
    hole.points.reverse();
    let raw = SubstratePaths::new(vec![rect_seg(0.0, 0.0, 10.0, 6.0, 0), hole]);
    let mut machine = liquid_machine();
    machine.retract_nl = 20.0;
    let mut params = liquid_params("Jedna linka");
    params.infill_val = 0.5;
    let (gcode, _) = liquid_gcode(&raw, &params, &Default::default(), &machine);
    let sample = gcode.split_once("; --- VZOREK 1 ---").unwrap().1;
    let lines: Vec<&str> = sample.lines().collect();
    let first = lines.iter().position(|l| l.starts_with("G1 X") && l.contains(" E")).unwrap();
    let last = lines.iter().rposition(|l| l.starts_with("G1 X") && l.contains(" E")).unwrap();
    let printing = |l: &&str| l.starts_with("G1 X") && l.contains(" E");
    assert!(lines[first..=last].iter().all(printing), "uvnitř linky jen tisk: {sample}");
    // Had stejný tvar tiskne na víc drah s přejezdem mezi nimi
    let mut snake_params = params.clone();
    snake_params.infill_style = "Had".into();
    let (snake, _) = liquid_gcode(&raw, &snake_params, &Default::default(), &machine);
    let sample = snake.split_once("; --- VZOREK 1 ---").unwrap().1;
    let lines: Vec<&str> = sample.lines().collect();
    let first = lines.iter().position(printing).unwrap();
    let last = lines.iter().rposition(printing).unwrap();
    assert!(!lines[first..=last].iter().all(printing), "had přejíždí: {sample}");
}

/// Odpliv: výchozí rozestup je nastavení vzorku (hodnota a typ v páru — dřív se
/// průměr trysky vykládal jako procenta a vznikla jediná linie), spojky
/// extrudují podle skutečné délky a poslední řada nechybí.
#[test]
fn test_prime_pattern_rows_and_connectors() {
    let raw = SubstratePaths::new(vec![rect_seg(0.0, 0.0, 10.0, 6.0, 0)]);
    let machine = liquid_machine();
    let mut params = liquid_params("Okraje");
    params.prime_active = true;
    let rows = |gcode: &str| -> Vec<(f64, f64, f64)> {
        let prime = gcode.split_once("ODPLIV").unwrap().1.split_once("VZOREK 1").unwrap().0;
        let mut prev: Option<(f64, f64)> = None;
        let mut out = Vec::new();
        for l in prime.lines() {
            let c = l.split(';').next().unwrap().trim();
            let (Some(x), Some(y)) = (parse_axis_value(c, 'X'), parse_axis_value(c, 'Y')) else { continue };
            if let (Some((px, py)), Some(e)) = (prev, parse_axis_value(c, 'E')) {
                out.push((y, (x - px).hypot(y - py), e));
            }
            prev = Some((x, y));
        }
        out
    };

    // Rozestup 1 mm (vlastní nastavení odplivu): 16 řad 0…15 mm včetně poslední
    let mut ov = std::collections::HashMap::new();
    ov.insert("-1".to_string(), SlideOverride { infill_val: Some(1.0), infill_type: Some("mm".into()), ..Default::default() });
    let (gcode, _) = liquid_gcode(&raw, &params, &ov, &machine);
    let moves = rows(&gcode);
    let ys: Vec<f64> = moves.iter().map(|m| m.0).collect();
    let (y0, y1) = (ys.iter().cloned().fold(f64::MAX, f64::min), ys.iter().cloned().fold(f64::MIN, f64::max));
    assert!(((y1 - y0) - 15.0).abs() < 1e-6, "výška odplivu {}", y1 - y0);
    assert_eq!(moves.len(), 16 + 15, "16 linií + 15 spojek");
    let e_per_mm = moves[0].2 / moves[0].1;
    for (_, d, e) in &moves {
        assert!((e / d - e_per_mm).abs() < 1e-3 * e_per_mm, "E/mm spojky {} vs {e_per_mm}", e / d);
    }

    // Globální rozestup v % bez nastavení odplivu → plný vzor, ne jedna linie
    params.infill_type = "%".into();
    params.infill_val = 100.0;
    let (gcode, _) = liquid_gcode(&raw, &params, &Default::default(), &machine);
    assert!(rows(&gcode).len() > 50, "odpliv v %: {} pohybů", rows(&gcode).len());
}

/// Kapky: po dávce se čeká, kapalina se stáhne a tryska zvedne; na další kapku
/// se přejíždí ve zdvižené výšce (dřív sjela nad čerstvou kapku na 0.22 mm),
/// sjede a kapalinu vrátí. Odhad času odpovídá čekání.
#[test]
fn test_dot_dispensing_dwell_retract_and_lift() {
    let raw = SubstratePaths::new(vec![rect_seg(0.0, 0.0, 4.0, 3.0, 0)]);
    let params = liquid_params("Tečky");
    let mut machine = liquid_machine();
    let (_, time0) = liquid_gcode(&raw, &params, &Default::default(), &machine);
    machine.retract_nl = 20.0;
    machine.dot_dwell_ms = 300.0;
    let (gcode, time) = liquid_gcode(&raw, &params, &Default::default(), &machine);
    let lines: Vec<&str> = gcode.lines().collect();
    let dots: Vec<usize> = lines.iter().enumerate().filter(|(_, l)| l.contains("Davkovani kapky")).map(|(i, _)| i).collect();
    assert!(dots.len() >= 6, "{gcode}");
    for &i in &dots {
        assert!(lines[i + 1].starts_with("G4 P300"), "{}", lines[i + 1]);
        assert!(lines[i + 2].contains("Zpetne nasati"), "{}", lines[i + 2]);
        assert!(lines[i + 3].contains("Z-hop po davkovani"), "{}", lines[i + 3]);
        assert!(lines[i - 1].contains("Navrat kapaliny"), "{}", lines[i - 1]);
        assert!(lines[i - 2].contains("Sjezd k povrchu"), "{}", lines[i - 2]);
    }
    // Mezi kapkami jen přejezd a sjezd — žádný zdvih ani sjezd před přejezdem
    for w in dots.windows(2) {
        let between = &lines[w[0] + 4..w[1]];
        assert_eq!(between.iter().filter(|l| l.starts_with("G1 Z")).count(), 1, "{between:?}");
    }
    // Retrakce a návrat jsou v rovnováze (končí se se staženou kapalinou)
    let net: f64 = lines
        .iter()
        .filter(|l| l.contains("nasati") || l.contains("Navrat kapaliny"))
        .map(|l| parse_axis_value(l.split(';').next().unwrap(), 'E').unwrap())
        .sum();
    let retract_e = 20.0 / 1000.0 * machine.calibration_factor;
    assert!((net + retract_e).abs() < 1e-4, "bilance retrakce {net}");
    assert!(time - time0 > dots.len() as f64 * 0.3, "čekání v odhadu času: {time} vs {time0}");
}

/// Multiplikátor průtoku platí i pro jednotku kroky (frontend převádí
/// nl ↔ kroky bez něj — jinak by se přepnutím jednotky změnil objem).
#[test]
fn test_flow_multiplier_applies_to_steps() {
    let calc = ExtrusionCalculator::new(9.5, 1.5, Some(0.3));
    let nl = calc.calculate_e_per_mm(200.0, "nl/mm");
    let steps = calc.calculate_e_per_mm(200.0 / 1000.0 * 0.3, "kroky/mm");
    assert!((nl - steps).abs() < 1e-12, "{nl} vs {steps}");
    assert!((calc.calculate_dot_extrusion(2.0, "kroky") - 3.0).abs() < 1e-12);
}
