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

/** Značka kontroly vyosení trysky, kterou generátor vkládá za homing po každé
 *  výměně trysky — zrcadlí `NOZZLE_ALIGN_MARKER` v `dpi-core/src/gcode.rs`.
 *
 *  Dnes je to jen komentář: tiskový front komentáře zahazuje, takže tisk
 *  pokračuje bez přerušení. Až kontrola vyosení vznikne, bude ji `comms.rs`
 *  rozpoznávat stejně jako `; APP_PAUSE:` a posílat frontendu událost. */
export const NOZZLE_ALIGN_MARKER = "; DPI_NOZZLE_ALIGN:";

export interface Transform {
  scale: number;
  rotation: number;
  gui_dx: number;
  gui_dy: number;
  cx: number;
  cy: number;
}

/** Objekt scény — samostatně umístitelný tvar. Jeden import se na objekty
 *  rozpadá podle oddělených tvarů (`split_objects` v Rustu). */
export interface SceneObject {
  id: string;
  name: string;
  /** Surová geometrie jen tohoto objektu (souřadnice zdrojového importu). */
  raw: SubstratePaths;
  /** Bbox celého importu `[min_x, max_x, min_y, max_y]` — společný rám pro
   *  normalizaci, díky němuž objekty z jednoho souboru drží vzájemné rozložení. */
  frame: [number, number, number, number];
  auto_scale: boolean;
}

/** Stav umístění objektu na jednom skle (bez geometrie) — vstup do update_layout. */
export interface PlacementState {
  object_id: string;
  enabled: boolean;
  transform: Transform;
  baked_scale: number;
  /** Tryska přenastavená jen pro tento objekt; null = globální tryska. */
  nozzle_type?: string | null;
  nozzle_diam?: number | null;
}

/** Umístění objektu včetně spočtené trasy — výstup update_layout. */
export interface Placement extends PlacementState {
  path: SubstratePaths;
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

/** Zdrojový soubor importu v metadatech — geometrie se neukládá, při obnově
 *  se soubor znovu naparsuje a rozdělí na objekty. */
export interface SourceMeta {
  id: string;
  name: string;
  ext: string;
  content: string;
  auto_scale: boolean;
  fineness: number;
}

export interface ObjectMeta {
  id: string;
  source_id: string;
  /** Pořadí objektu ve výsledku `split_objects` nad zdrojem. */
  index: number;
  name: string;
}

export interface GCodeMetadata {
  params: ProcessParams;
  overrides: Record<string, SlideOverride>;
  sources: SourceMeta[];
  objects: ObjectMeta[];
  slides: PlacementState[][];
  // Pole formátu v2 — čtou se ze starších souborů, zapisují se jen
  // u projektů s jediným objektem (aby je otevřela i starší verze).
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

/** Kompletní nastavení aplikace — odráží strukturu AppSettings v src-tauri/src/settings.rs. */
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
  /** Blok po výměně trysky — homing a příprava na kontrolu vyosení. */
  nozzle_change_gcode?: string;
  /** Pořadí tisku napříč substráty a tryskami — "substrate_major" | "nozzle_major". */
  print_order?: string;
  /** Strategie přejezdů — "fastest" | "cross_allowed" (alias) | "no_cross". */
  route_mode?: string;
  // Výchozí procesní hodnoty
  default_z_offset: number;
  default_z_hop: number;
  /** Bezpečná výška Z pro přejezdy a virtuální posun souřadnic (mm). */
  safe_z: number;
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
  camera_device_id: string;
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
  // UI stav (dřív roztroušený v localStorage)
  theme: string;
  disable_snow: boolean;
  recent_files: RecentFileEntry[];
  // Neznámá rozšíření z JSON souboru
  [key: string]: unknown;
}

/** Položka seznamu nedávno otevřených souborů. */
export interface RecentFileEntry {
  path: string;
  name: string;
  timestamp: number;
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

/** Výsledek update_layout — kompletní data pro překreslení plátna. */
export interface LayoutUpdateResult {
  positions: LayoutPosition[];
  /** `slides[i][k]` patří k `objects[k]` — každé sklíčko nese položku pro každý
   *  objekt, skryté mají `enabled === false`. */
  slides: Placement[][];
  prime_path: SubstratePaths | null;
  final_sample_count: number;
}

export interface BedConfig {
  max_x: number;
  max_y: number;
  min_x: number;
  offset_x: number;
  offset_y: number;
}

/** Kompletní přepočet layoutu v jediném Rust volání (kapacita, dráhy, pozice,
 *  transformace, prime náhled) — nahrazuje sekvenci 3+N samostatných invoke. */
export async function update_layout(
  params: ProcessParams,
  overrides: Record<string, SlideOverride>,
  objects: SceneObject[],
  slides: PlacementState[][],
  oldPositions: LayoutPosition[],
  bed: BedConfig,
  multiSpacing: number
): Promise<LayoutUpdateResult> {
  return await invoke<LayoutUpdateResult>("update_layout", {
    params,
    overrides,
    objects,
    slides,
    oldPositions,
    bed,
    multiSpacing,
  });
}

/** Rozdělí surový import na samostatné objekty podle oddělených tvarů. */
export async function split_objects(rawPaths: SubstratePaths): Promise<SubstratePaths[]> {
  return await invoke<SubstratePaths[]>("split_objects", { rawPaths });
}

/** Najde volné místo na sklíčku pro nově přidanou skupinu objektů.
 *  Vrací `[dx, dy]`, nebo null když se skupina nikam nevejde. */
export async function find_free_offset(
  pos: LayoutPosition,
  newAabb: [number, number, number, number],
  occupied: [number, number, number, number][],
  nozzleDiam: number
): Promise<[number, number] | null> {
  return await invoke<[number, number] | null>("find_free_offset", {
    pos,
    newAabb,
    occupied,
    nozzleDiam,
  });
}

export interface PreviewSegData {
  slide_idx: number;
  /** Index objektu (placementu) na sklíčku — u odplivu vždy 0. */
  obj_idx: number;
  seg_idx: number;
  path_start_dist: number;
  point_dists: number[];
  seg_dist: number;
}

export interface PreviewDistResult {
  segs: PreviewSegData[];
  total_dist: number;
}

/** Předpočítá kumulativní vzdálenosti segmentů pro náhled průběhu tisku. */
export async function compute_preview_segments(
  positions: LayoutPosition[],
  slides: Placement[][],
  primePath: SubstratePaths | null,
  globalNozzle: string,
  printOrder: string,
  routeMode: string
): Promise<PreviewDistResult> {
  return await invoke<PreviewDistResult>("compute_preview_segments", {
    positions,
    slides,
    primePath,
    globalNozzle,
    printOrder,
    routeMode,
  });
}

/** Vrátí `true` pokud některá z transformovaných tras přesahuje okraj sklíčka s insetem trysky. */
export async function check_paths_overflow(
  slides: Placement[][],
  nonPrimePositions: LayoutPosition[],
  nozzleDiam: number
): Promise<boolean> {
  return await invoke<boolean>("check_paths_overflow", {
    slides,
    nonPrimePositions,
    nozzleDiam,
  });
}

/** Konfigurace stroje pro generování G-kódu — odpovídá dpi_core::MachineConfig. */
export interface MachineConfig {
  bed: {
    max_x: number;
    max_y: number;
    min_x: number;
    offset_x: number;
    offset_y: number;
  };
  start_gcode: string;
  end_gcode: string;
  loop_start_gcode: string;
  loop_end_gcode: string;
  /** G-kód po výměně trysky; prázdný = vestavěný default (G28). */
  nozzle_change_gcode?: string;
  /** Pořadí tisku napříč substráty a tryskami — "substrate_major" | "nozzle_major". */
  print_order?: string;
  /** Strategie přejezdů — "fastest" | "cross_allowed" (alias) | "no_cross". */
  route_mode?: string;
  multi_spacing: number;
  block_height: number;
  calibration_factor: number;
  z_hop: number;
  safe_z: number;
  /** Bezpečnostní strop teploty podložky (°C); null = konzervativní default v Rustu. */
  bed_max_temp: number | null;
  /** Pokud true, negeneruje se blok virtuálního posunu Z (G1 safe_z + G92) —
   * volající už virtuální souřadný systém nastavil sám (kalibrační tok). */
  skip_z_shift_setup?: boolean;
}

export async function generate_gcode(
  slides: Placement[][],
  params: ProcessParams,
  slideOverrides: Record<string, SlideOverride>,
  machine: MachineConfig
): Promise<GCodeResponse> {
  return await invoke<GCodeResponse>("generate_gcode_job", {
    slides,
    params,
    slideOverrides,
    machine,
  });
}

/** Výsledek `compute_z_calibration` — odpovídá ZCalibrationInfo v src-tauri. */
export interface ZCalibrationInfo {
  glass_z_theoretical: number;
  z_shift: number;
}

/** Spočítá teoretickou Z povrchu sklíčka a virtuální posun Z stejným
 * algoritmem jako generátor G-kódu (včetně per-slide overrides). */
export async function compute_z_calibration(
  params: ProcessParams,
  slideOverrides: Record<string, SlideOverride>,
  blockHeight: number
): Promise<ZCalibrationInfo> {
  return await invoke<ZCalibrationInfo>("compute_z_calibration", {
    params,
    slideOverrides,
    blockHeight,
  });
}

/** Segment G-kódu mezi pauzami — odpovídá dpi_core::GcodePauseSegment. */
export interface GcodePauseSegment {
  code: string;
  /** Zpráva z pauzovacího příkazu; null u posledního segmentu (bez pauzy za ním). */
  msg: string | null;
}

/** Rozdělí G-kód podle pauzovacích příkazů M0/M1/M601. */
export async function split_gcode_pauses(gcode: string): Promise<GcodePauseSegment[]> {
  return await invoke<GcodePauseSegment[]>("split_gcode_pauses", { gcode });
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
  /** Společný rám importu `[min_x, max_x, min_y, max_y]`; null = bbox vlastní geometrie. */
  frame?: [number, number, number, number] | null;
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

/** Chyby komunikace s tiskárnou (timeout potvrzení, chyba zápisu, ztráta spojení). */
export function subscribe_print_error(callback: (message: string) => void) {
  return listen<string>("print-error", (event: Event<string>) => {
    callback(event.payload);
  });
}
