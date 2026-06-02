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
    let positions = get_layout_positions(2, slide_w, slide_h, spacing, false, &bed);

    assert_eq!(positions.len(), 2);

    // First slide should be placed at the right-front column
    // base_right = 250 - 18 = 232
    // sample_x = 232 - 25 = 207
    assert_eq!(positions[0].x, 207.0);
    assert_eq!(positions[0].y, 11.0);
    assert_eq!(positions[0].is_prime, false);

    // Second slide should be placed in the same column, shifted in Y
    // y2 = 11.0 + 75.0 + 5.0 = 91.0
    assert_eq!(positions[1].x, 207.0);
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
    // So Slide 3 must jump to a new column on the left!
    let bed = BedConfig {
        max_x: bed_max_x,
        max_y: bed_max_y,
        min_x: 0.0,
        offset_x: start_offset_x,
        offset_y: start_offset_y,
    };
    let positions = get_layout_positions(3, slide_w, slide_h, spacing, false, &bed);

    assert_eq!(positions.len(), 3);

    // Slide 3 X location:
    // Prev column left = base_right - col_w = 232 - 25 = 207
    // New column right = 207 - 5 = 202
    // Slide 3 X = 202 - 25 = 177
    assert_eq!(positions[2].x, 177.0);
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
    let positions = get_layout_positions(2, 76.0, 26.0, 5.0, true, &bed);

    // Prime + 2 samples
    assert_eq!(positions.len(), 3, "prime + 2 vzorky");

    // Prime: base_right = 92, p_x = 92-76 = 16, šířka 76
    let prime = &positions[0];
    assert!(prime.is_prime);
    assert_eq!(prime.x, 16.0);
    assert_eq!(prime.y, 11.0);
    assert!(
        prime.x >= 0.0 && prime.x + prime.width <= 110.0,
        "prime mimo podložku"
    );

    // Sample 1: curr_y = 11+26+5 = 42, sample_x = 16
    let s1 = &positions[1];
    assert!(!s1.is_prime);
    assert_eq!(s1.x, 16.0);
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
    assert_eq!(s2.x, 16.0);
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
        retraction: 0.0,
        retract_speed: 3000.0,
        z_hop: 2.0,
        safe_z: 20.0,
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
        retraction: 0.0,
        retract_speed: 3000.0,
        z_hop: 2.0,
        safe_z: 20.0,
    };
    let res = generate_gcode(&paths, &params, &transforms, &overrides, &machine);

    assert!(res.is_ok());
    let (gcode, _dist, _time) = res.unwrap();

    // S větším offsetem (0.5 namísto 0.2) se změní absolutní výška tisku i shift!
    // Původní min_needed_z = -34.0 + 30.0 - 4.0 + 1.0 + 0.2 = -6.8
    // S overridem min_needed_z = -34.0 + 30.0 - 4.0 + 1.0 + 0.5 = -6.5
    // Z shift = 6.5 + 1.0 = 7.5
    assert!(gcode.contains("; --- VIRTUALNI POSUN Z (SHIFT 7.50mm) ---"));

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
