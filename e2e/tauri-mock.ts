/**
 * Mock Tauri IPC pro Playwright testy — injektován před načtením stránky.
 *
 * Protože testy běží v holém Vite dev serveru (ne v Tauri webview),
 * musíme simulovat window.__TAURI_INTERNALS__ a invoke() tak, aby se
 * aplikace normálně inicializovala a UI bylo plně renderováno.
 */

export const TAURI_MOCK_SCRIPT = `
(function () {
  const defaults = {
    bed_max_x: 250, bed_max_y: 210, bed_min_x: 0,
    start_offset_x: 18, start_offset_y: 11,
    multi_spacing: 5, block_height: 34,
    hidden_nozzle_part: 4, print_speed: 1500,
    retraction: 0, bed_min_temp: 30,
    default_z_offset: 0.2, default_z_hop: 2.0, safe_z: 20.0,
    default_speed: 1500, default_infill: 1.0, default_density: 0.05,
    filament_diameter: 9.5, flow_multiplier: 1.0,
    calibration_factor: 0.323877, calibration_object_height: 0.1,
    camera_rotation: 180, z_step: 0.0025,
    camera_mirror: false, show_slide_grid: true, show_bed_axes: true,
    liquid_density: 1.0, leveling_circle_diameter: 5,
    path_fineness: 1.0,
    start_gcode: "; mock\\nG28\\n", end_gcode: "G0 Z30\\n",
    loop_start_gcode: "", loop_end_gcode: "",
    sklo_dims: { "Laboratorní Sklo (76 x 26 x 1 mm)": [76, 26, 1] },
    nozzle_defs: { "Červená": [31.1, 0.3, 4.0, "#ef4444"] },
  };

  const printerStatus = {
    is_connected: false, is_printing: false, is_paused: false,
    current_x: 0, current_y: 0, current_z: 0,
    temp_extruder: 0, temp_bed: 0, progress: 0,
    total_dist: 0, time_remaining: 0,
  };

  const handlers = {
    get_settings:        () => defaults,
    save_settings:       () => null,
    get_ports:           () => [],
    connect_printer:     () => printerStatus,
    auto_connect_printer: () => printerStatus,
    disconnect_printer:  () => null,
    start_print_job:     () => null,
    pause_print_job:     () => null,
    resume_print_job:    () => null,
    stop_print_job:      () => null,
    send_manual_gcode:   () => null,
    get_startup_time:    () => 0,
    get_local_ip:        () => "127.0.0.1",
    calculate_layout:    (args) => {
      const { count = 1, slideW = 76, slideH = 26 } = args || {};
      return Array.from({ length: count }, (_, i) => ({
        x: 18 + i * (slideW + 5), y: 11,
        width: slideW, height: slideH, is_prime: false,
      }));
    },
    generate_gcode_job:  () => ({ gcode: "; test gcode\\n", total_dist: 10, total_time: 5 }),
    process_paths:       (args) => args?.rawPaths ?? { segments: [] },
    parse_dxf_file:      () => ({ segments: [] }),
    parse_svg_file:      () => ({ segments: [] }),
    build_gcode_metadata_header: () => "; meta\\n",
    parse_gcode_metadata: () => null,
    parse_gcode_file_paths: () => ({ segments: [] }),
    get_prime_preview:   () => ({ segments: [] }),
    generate_csv_protocol: () => "",
    save_feedback:       () => null,
  };

  // Simulace Tauri event systému
  const listeners = {};
  window.__TAURI_INTERNALS__ = {
    invoke: async (cmd, args) => {
      const handler = handlers[cmd];
      if (handler) return Promise.resolve(handler(args));
      console.warn("[tauri-mock] neznámý příkaz:", cmd, args);
      return Promise.resolve(null);
    },
    transformCallback: (cb, once) => {
      const id = Math.random();
      return id;
    },
    convertFileSrc: (src) => src,
    metadata: { currentWindow: { label: "main" } },
  };

  // Mock pro @tauri-apps/api/event listen()
  window.__TAURI_MOCK_LISTEN__ = (event, cb) => {
    if (!listeners[event]) listeners[event] = [];
    listeners[event].push(cb);
    return Promise.resolve(() => {
      listeners[event] = listeners[event].filter(fn => fn !== cb);
    });
  };
})();
`;
