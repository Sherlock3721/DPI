import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";

export interface Point2D {
  x: number;
  y: number;
}

export interface PathSegment {
  points: Point2D[];
  is_filled?: boolean;
}

export interface SubstratePaths {
  segments: PathSegment[];
}

export interface Transform {
  scale: number;
  rotation: number;
  gui_dx: number;
  gui_dy: number;
  cx: number;
  cy: number;
}

export interface ProcessParams {
  sample_count: number;
  prime_active: boolean;
  slide_w: number;
  slide_h: number;
  slide_z: number;
  z_offset: number;
  z_unit: string;
  nozzle_height: number;
  nozzle_hidden: number;
  filament_diameter: number;
  flow_multiplier: number;
  bed_temp: number;
  extrusion_rate: number;
  extrusion_unit: string;
  nozzle_diam: number;
  infill_style: string;
  infill_val: number;
  infill_type: string;
  infill_angle: number;
  print_speed: number;
  nozzle_type: string;
  /** Frontend-only příznak: zda provést full bed leveling (G28) před tiskem. Rust toto pole ignoruje. */
  bed_leveling?: boolean;
}
export interface SlideOverride {
  name?: string;
  note?: string;
  z_offset?: number | null;
  extrusion_rate?: number | null;
  extrusion_unit?: string | null;
  print_speed?: number | null;
  infill_val?: number | null;
  infill_type?: string | null;
  nozzle_height?: number | null;
  infill_style?: string | null;
  slide_w?: number | null;
  slide_h?: number | null;
  glass_type?: string | null;
}

export interface LayoutPosition {
  x: number;
  y: number;
  width: number;
  height: number;
  is_prime: boolean;
}

export interface PrinterStatus {
  is_connected: boolean;
  is_printing: boolean;
  is_paused: boolean;
  current_x: number;
  current_y: number;
  current_z: number;
  temp_extruder: number;
  temp_bed: number;
  progress: number;
  total_dist: number;
  time_remaining: number;
}

export interface GCodeResponse {
  gcode: string;
  total_dist: number;
  total_time: number;
}

export interface GCodeMetadata {
  params: ProcessParams;
  overrides: Record<string, SlideOverride>;
  transforms: Transform[];
  baked_scales: number[];
  source_file_name: string;
  source_file_ext: string;
  source_file_content: string;
  auto_scale: boolean;
  fineness: number;
}

export interface LevelingPoint {
  name: string;
  x: number;
  y: number;
}

/** Kompletní nastavení aplikace — odráží strukturu DEFAULT_SETTINGS_JSON v src-tauri/src/lib.rs. */
export interface AppSettings {
  // Tisková plocha
  bed_max_x: number;
  bed_max_y: number;
  bed_min_x: number;
  bed_max_temp?: number;
  bed_min_temp: number;
  // Polohovací offsety a rozestupy
  start_offset_x: number;
  start_offset_y: number;
  multi_spacing: number;
  // Mechanika tiskárny
  block_height: number;
  hidden_nozzle_part: number;
  print_speed: number;
  // G-kód makra
  start_gcode: string;
  end_gcode: string;
  loop_start_gcode: string;
  loop_end_gcode: string;
  // Výchozí procesní hodnoty
  default_z_offset: number;
  default_z_hop: number;
  default_speed: number;
  default_infill: number;
  default_density: number;
  // Extruze a kalibrace
  filament_diameter: number;
  flow_multiplier: number;
  calibration_factor: number;
  // Definice sklíček a trysek
  sklo_dims: Record<string, [number, number, number]>;
  nozzle_defs: Record<string, [number, number, number, string]>;
  // Nivelace
  leveling_points?: LevelingPoint[];
  leveling_circle_diameter: number;
  // Kamera
  camera_rotation: number;
  camera_mirror: boolean;
  // UI
  show_slide_grid: boolean;
  show_bed_axes: boolean;
  path_fineness: number;
  z_step: number;
  liquid_density: number;
  liquid_defs?: Record<string, {
    color?: string;
    category?: string;
    z_offset?: number; z_offset_min?: number | null; z_offset_max?: number | null;
    extrusion?: number; extrusion_min?: number | null; extrusion_max?: number | null;
    forbidden_nozzles?: string[];
    print_speed?: number; print_speed_min?: number | null; print_speed_max?: number | null;
    bed_temp?: number; bed_temp_min?: number | null; bed_temp_max?: number | null;
  }>;
  // Neznámá rozšíření z JSON souboru
  [key: string]: unknown;
}

// --- TAURI COMMAND WRAPPERS ---

export async function get_available_ports(): Promise<string[]> {
  try {
    return await invoke<string[]>("get_ports");
  } catch (e) {
    console.error("Failed to get ports:", e);
    return [];
  }
}

export async function connect_to_printer(
  portName: string,
  baudrate: number
): Promise<PrinterStatus> {
  return await invoke<PrinterStatus>("connect_printer", { portName, baudrate });
}

/// Automaticky projde dostupné porty a připojí první nalezenou tiskárnu.
/// Odpovídá Python `connect_printer(manual_port=None)`.
export async function auto_connect_printer(baudrate: number): Promise<PrinterStatus> {
  return await invoke<PrinterStatus>("auto_connect_printer", { baudrate });
}

export async function disconnect_from_printer(): Promise<void> {
  await invoke("disconnect_printer");
}

export async function calculate_slide_layout(
  count: number,
  slideW: number,
  slideH: number,
  spacing: number,
  bedMaxX: number,
  bedMaxY: number,
  startOffsetX: number,
  startOffsetY: number,
  primeActive: boolean,
  bedMinX: number = 0.0,
  primeGlassType: string | null = null
): Promise<LayoutPosition[]> {
  return await invoke<LayoutPosition[]>("calculate_layout", {
    count,
    slideW,
    slideH,
    spacing,
    bedMaxX,
    bedMaxY,
    startOffsetX,
    startOffsetY,
    primeActive,
    bedMinX,
    primeGlassType,
  });
}

export interface LayoutWithTransforms {
  positions: LayoutPosition[];
  transforms: Transform[];
}

/** Vypočítá nové pozice sklíček a přizpůsobí transformace v jediném volání Rustu.
 *  Nahrazuje `calculate_slide_layout` + TS smyčku pro přizpůsobení transformací. */
export async function recalculate_layout(
  sampleCount: number,
  slideW: number,
  slideH: number,
  multiSpacing: number,
  primeActive: boolean,
  bedMaxX: number,
  bedMaxY: number,
  bedMinX: number,
  startOffsetX: number,
  startOffsetY: number,
  oldPositions: LayoutPosition[],
  currentTransforms: Transform[],
  currentPaths: SubstratePaths[],
  nozzleDiam: number,
  primeGlassType: string | null = null
): Promise<LayoutWithTransforms> {
  return await invoke<LayoutWithTransforms>("recalculate_layout", {
    sampleCount,
    slideW,
    slideH,
    multiSpacing,
    primeActive,
    bedMaxX,
    bedMaxY,
    bedMinX,
    startOffsetX,
    startOffsetY,
    oldPositions,
    currentTransforms,
    currentPaths,
    nozzleDiam,
    primeGlassType,
  });
}

export async function generate_gcode(
  slidePaths: SubstratePaths[],
  params: ProcessParams,
  transforms: Transform[],
  slideOverrides: Record<string, SlideOverride>,
  startGcode: string,
  endGcode: string,
  loopStartGcode: string,
  loopEndGcode: string,
  bedMaxX: number,
  bedMaxY: number,
  startOffsetX: number,
  startOffsetY: number,
  multiSpacing: number,
  blockHeight: number,
  calibrationFactor: number,
  bedMinX: number = 0.0,
  zHop: number = 2.0,
  safeZ: number = 20.0
): Promise<GCodeResponse> {
  return await invoke<GCodeResponse>("generate_gcode_job", {
    slidePaths,
    params,
    transforms,
    slideOverrides,
    startGcode,
    endGcode,
    loopStartGcode,
    loopEndGcode,
    bedMaxX,
    bedMaxY,
    startOffsetX,
    startOffsetY,
    multiSpacing,
    blockHeight,
    calibrationFactor,
    bedMinX,
    zHop,
    safeZ,
  });
}

export async function start_print(
  gcode: string,
  totalDist: number,
  totalTime: number
): Promise<void> {
  await invoke("start_print_job", { gcode, totalDist, totalTime });
}

export async function pause_print(): Promise<void> {
  await invoke("pause_print_job");
}

export async function resume_print(): Promise<void> {
  await invoke("resume_print_job");
}

export async function resume_app_pause(): Promise<void> {
  await invoke("resume_app_pause_job");
}

export async function stop_print(): Promise<void> {
  await invoke("stop_print_job");
}

export async function send_manual_command(gcode: string): Promise<void> {
  await invoke("send_manual_gcode", { gcode });
}

export async function send_manual_blocking(gcode: string): Promise<void> {
  await invoke("send_manual_gcode_blocking", { gcode });
}

export async function get_app_settings(): Promise<AppSettings> {
  return await invoke<AppSettings>("get_settings");
}

export async function save_app_settings(settings: AppSettings): Promise<void> {
  await invoke("save_settings", { settings });
}

// --- TELEMETRY AND ALIGNMENT EVENT SUBSCRIPTIONS ---

export function subscribe_printer_status(callback: (status: PrinterStatus) => void) {
  return listen<PrinterStatus>("printer-status-changed", (event: Event<PrinterStatus>) => {
    callback(event.payload);
  });
}

export interface SliceParams {
  slide_w: number;
  slide_h: number;
  margin: number;
  auto_scale: boolean;
  infill_style: string;
  infill_val: number;
  infill_type: string;
  infill_angle: number;
  nozzle_diam: number;
  user_scale: number;
}

/** Zpracuje raw vektorové dráhy (normalizace, škálování, infill) — Rust dpi-core. */
export async function process_substrate_paths(
  rawPaths: SubstratePaths,
  sliceParams: SliceParams
): Promise<SubstratePaths> {
  return await invoke<SubstratePaths>("process_paths", { rawPaths, sliceParams });
}

/** Parsuje ASCII DXF soubor na SubstratePaths — Rust dpi-core. */
export async function parse_dxf(dxfText: string): Promise<SubstratePaths> {
  return await invoke<SubstratePaths>("parse_dxf_file", { dxfText });
}

/** Parsuje SVG soubor na SubstratePaths — Rust dpi-core. */
export async function parse_svg(svgText: string, fineness: number = 1.0): Promise<SubstratePaths> {
  return await invoke<SubstratePaths>("parse_svg_file", { svgText, fineness });
}

/** Sestaví G-kód metadata hlavičku (vkládá se před vygenerovaný G-kód). */
export async function build_gcode_metadata_header(meta: GCodeMetadata): Promise<string> {
  return await invoke<string>("build_gcode_metadata_header", { meta });
}

/** Extrahuje DPI metadata ze záhlaví G-kód souboru. Vrátí null pokud soubor metadata neobsahuje. */
export async function parse_gcode_metadata(gcodeText: string): Promise<GCodeMetadata | null> {
  return await invoke<GCodeMetadata | null>("parse_gcode_metadata", { gcodeText });
}

/** Parsuje G-kód (G0/G1) na SubstratePaths pro vizualizaci. */
export async function parse_gcode_file_paths(gcodeText: string): Promise<SubstratePaths> {
  return await invoke<SubstratePaths>("parse_gcode_file_paths", { gcodeText });
}

/** Vrátí náhledové dráhy odplivové (prime) pozice pro canvas. */
export async function get_prime_preview(
  pos: LayoutPosition,
  params: ProcessParams,
  primeOverride: SlideOverride | null
): Promise<SubstratePaths> {
  return await invoke<SubstratePaths>("get_prime_preview", { pos, params, primeOverride });
}

/** Sestaví CSV protokol tisku. `dateStr` předávej z frontendu kvůli lokalizaci data. */
export async function generate_csv_protocol(
  params: ProcessParams,
  overrides: Record<string, SlideOverride>,
  totalDist: number,
  totalTime: number,
  selectedGlass: string,
  appVersion: string,
  dateStr: string
): Promise<string> {
  return await invoke<string>("generate_csv_protocol", {
    params,
    overrides,
    totalDist,
    totalTime,
    selectedGlass,
    appVersion,
    dateStr,
  });
}

export async function submit_feedback(data: any): Promise<void> {
  await invoke("save_feedback", { data });
}

export function subscribe_serial_rx(callback: (line: string) => void) {
  return listen<string>("serial-rx", (event: Event<string>) => {
    callback(event.payload);
  });
}
