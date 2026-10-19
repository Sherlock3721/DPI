import { writable, derived, get } from "svelte/store";
import {
  update_layout,
  process_substrate_paths,
  parse_dxf,
  parse_svg,
  type ProcessParams,
  type LayoutPosition,
  type SubstratePaths,
  type Transform,
  type SlideOverride,
  type SliceParams,
  type GCodeMetadata,
  type ObjectTransform,
  type ObjectOverride,
  type PathObject,
} from "../lib/tauri";
import { settingsStore } from "./settingsStore";
import { clampGuidXY, getTransformIdx, type RawBbox } from "../lib/geometry";
import {
  IDENTITY_XF,
  designToWorld,
  isIdentityXf,
  objectPivot,
  parseObjectTransforms,
  serializeObjectTransforms,
  worldToDesign,
  xfApply,
  xfInverse,
} from "../lib/objectTransform";

export type RecentFile = import("../lib/tauri").RecentFileEntry;

// Nedávné soubory žijí v settings.json (dřív localStorage) — jediný zdroj
// pravdy je settingsStore, tady jen odvozený pohled a zápisová operace.
export const recentFilesStore = derived(settingsStore, (s) => s.recent_files ?? []);

export function addRecentFile(path: string, name: string) {
  if (!path.toLowerCase().endsWith(".gcode")) {
    return;
  }
  const files = get(settingsStore).recent_files ?? [];
  // Odstraníme existující záznam se stejnou cestou a přidáme na začátek
  const filtered = files.filter((f) => f.path !== path);
  const newFiles = [{ path, name, timestamp: Date.now() }, ...filtered].slice(0, 10);
  settingsStore.persistPatch({ recent_files: newFiles });
}

/** Tvar nakreslený uživatelem v canvasu — world souřadnice v mm (Y nahoru).
 *  `hole` = tvar se odečítá z ostatních (díra). */
export type DrawnShape =
  | { kind: "rect"; x: number; y: number; w: number; h: number; hole?: boolean }
  | { kind: "ellipse"; cx: number; cy: number; rx: number; ry: number; hole?: boolean }
  | { kind: "line"; x1: number; y1: number; x2: number; y2: number; anchors?: LineAnchors };

/** Konec čáry přichycený magnetem k trase objektu (sklíčko = index transformace). */
export interface LineAnchor {
  slide: number;
  id: number;
}
export interface LineAnchors {
  start?: LineAnchor;
  end?: LineAnchor;
}

/** Dráha vkládaná ze schránky (Ctrl+V / Ctrl+D) — world souřadnice v mm. */
export interface PastedSeg {
  pts: { x: number; y: number }[];
  closed: boolean;
  /** Původní zdrojový segment (pro mapování id objektů). */
  src: number;
  /** Klíč zdrojového prvku — dráhy stejného prvku tvoří jednu even-odd skupinu. */
  group: string;
  filled: boolean;
  stroke: boolean;
  hole: boolean;
  /** Konce přichycené k objektům: id objektu ve vložené kresbě, `raw` = bod
   *  přichycení ve zdrojových souřadnicích (chybí = konec dráhy). */
  anchors?: { end: number; id: number; raw?: { x: number; y: number } }[];
}

/** Tvary vkládané na konkrétní sklíčko (index pozice). */
export interface PastedShape {
  kind: "paths";
  target: number;
  segs: PastedSeg[];
}

/** Obsah schránky plátna: objekty jednoho sklíčka ve world souřadnicích. */
export interface CanvasClip {
  /** Celá trasa, nebo vybrané objekty. */
  whole: boolean;
  /** Index pozice sklíčka, ze kterého se kopírovalo. */
  fromPos: number;
  segs: PastedSeg[];
  /** Přepisy nastavení kopírovaných objektů (klíč = původní id). */
  overrides: Record<string, ObjectOverride>;
  /** Id kopírovaných objektů. */
  ids: number[];
}

/** Naposledy nakreslený tvar — lze ho dodatečně upravit (panel jako v Blenderu),
 *  dokud se projekt mezitím nezměnil. */
export interface LastDrawn {
  shape: DrawnShape;
  /** Tvar se vytvořil jen na cílovém substrátu (na ostatních je smazaný). */
  onlyHere: boolean;
  /** Stav projektu před přidáním tvaru — výchozí bod pro opakované umístění. */
  base: ProjectState;
  resultText: string | null;
  resultTransforms: Transform[];
}

export const lastDrawnShape = writable<LastDrawn | null>(null);

/** Výhřev podložky vypnutý (0 °C je platná teplota, pokud ji nastavení dovolí). */
export const BED_OFF = -1;

/** Platí panel posledního tvaru? Jen dokud se kresba ani transformace nezměnily. */
export function isLastDrawnCurrent(ld: LastDrawn | null, state: ProjectState): boolean {
  return !!ld && state.rawFileText === ld.resultText && state.transforms === ld.resultTransforms;
}

/** Atribut, kterým poznáme vlastní generované SVG s kresbou (lze do něj přikreslovat). */
const DRAWING_SVG_MARKER = "data-dpi-drawing";
/** Atribut prvku, který je dírou (odečítá se z ostatních) — čte ho svg_parser. */
const DRAWING_HOLE_ATTR = "data-dpi-hole";
/** Atribut kořene SVG: id objektů smazaných ze sdílené kresby (ze všech skel).
 *  Prvky v SVG zůstávají, aby se id ostatních objektů neposunula. */
const DRAWING_DELETED_ATTR = "data-dpi-deleted-objects";
/** Atribut kořene SVG: úpravy objektů sdílené všemi skly — čte ho svg_parser. */
const DRAWING_XFORMS_ATTR = "data-dpi-object-transforms";
/** Atributy čáry: konec přichycený k objektu "id,x,y" (zdrojové souřadnice). */
const ANCHOR_ATTRS = { start: "data-dpi-anchor-start", end: "data-dpi-anchor-end" } as const;

const fmtMm = (v: number) => String(+v.toFixed(3));


/** Obvodové body tvaru ve world souřadnicích (obdélník 4 rohy, elipsa 64 bodů). */
function drawnShapeWorldPoints(shape: DrawnShape | PastedShape): { x: number; y: number }[] {
  switch (shape.kind) {
    case "paths":
      return shape.segs.flatMap((sg) => sg.pts);
    case "rect":
      return [
        { x: shape.x, y: shape.y },
        { x: shape.x + shape.w, y: shape.y },
        { x: shape.x + shape.w, y: shape.y + shape.h },
        { x: shape.x, y: shape.y + shape.h },
      ];
    case "ellipse": {
      const pts: { x: number; y: number }[] = [];
      for (let i = 0; i < 64; i++) {
        const a = (i / 64) * Math.PI * 2;
        pts.push({ x: shape.cx + shape.rx * Math.cos(a), y: shape.cy + shape.ry * Math.sin(a) });
      }
      return pts;
    }
    case "line":
      return [
        { x: shape.x1, y: shape.y1 },
        { x: shape.x2, y: shape.y2 },
      ];
  }
}

/** Výplň + tah: kreslené tvary mají ve výchozím stylu okraj i výplň; díra má příznak. */
function shapeFillAttrs(shape: DrawnShape): string {
  const hole = shape.kind !== "line" && shape.hole ? ` ${DRAWING_HOLE_ATTR}="1"` : "";
  return `fill="black" stroke="black"${hole}`;
}

/** SVG primitiva pro čistě posunuté zobrazení: r = (wx + tx, ty - wy) — posun + Y flip. */
function shapeToSvgPrimitive(shape: DrawnShape, tx: number, ty: number): string {
  switch (shape.kind) {
    case "rect":
      return `<rect x="${fmtMm(shape.x + tx)}" y="${fmtMm(ty - shape.y - shape.h)}" width="${fmtMm(shape.w)}" height="${fmtMm(shape.h)}" ${shapeFillAttrs(shape)}/>`;
    case "ellipse":
      return `<ellipse cx="${fmtMm(shape.cx + tx)}" cy="${fmtMm(ty - shape.cy)}" rx="${fmtMm(shape.rx)}" ry="${fmtMm(shape.ry)}" ${shapeFillAttrs(shape)}/>`;
    case "line":
      return `<line x1="${fmtMm(shape.x1 + tx)}" y1="${fmtMm(ty - shape.y1)}" x2="${fmtMm(shape.x2 + tx)}" y2="${fmtMm(ty - shape.y2)}" fill="none" stroke="black"/>`;
  }
}

/** Obecné mapování world → SVG po bodech — pro kresbu aktuálně zobrazenou
 *  s rotací/měřítkem, kdy primitiva nestačí (rotovaný obdélník apod.). */
function shapeToSvgMapped(
  shape: DrawnShape,
  map: (w: { x: number; y: number }) => { x: number; y: number }
): string {
  const pts = drawnShapeWorldPoints(shape).map(map);
  if (shape.kind === "line") {
    const [a, b] = pts;
    return `<line x1="${fmtMm(a.x)}" y1="${fmtMm(a.y)}" x2="${fmtMm(b.x)}" y2="${fmtMm(b.y)}" fill="none" stroke="black"/>`;
  }
  const ptStr = pts.map((p) => `${fmtMm(p.x)},${fmtMm(p.y)}`).join(" ");
  return `<polygon points="${ptStr}" ${shapeFillAttrs(shape)}/>`;
}

const fmtRaw = (v: number) => String(+v.toFixed(5));

/** SVG prvky vkládaných drah: dráhy stejného zdrojového prvku (a stejné výplně)
 *  jdou za sebou do jednoho <path> — zachová se even-odd (díry) i pořadí
 *  segmentů, a tím i id objektů (= index segmentu). Čára s přichycenými konci
 *  je samostatný <line> (kotvy čte parser jen z <line>). */
function pastedSegsToSvg(segs: PastedSeg[], toRaw: (w: { x: number; y: number }) => { x: number; y: number }): string {
  const out: string[] = [];
  let run: PastedSeg[] = [];
  /** Výplň prvku: určují ji uzavřené dráhy (otevřené výplň nemají). */
  let runFill: boolean | null = null;
  const flush = () => {
    if (run.length === 0) return;
    const d = run
      .map((sg) => {
        const r = sg.pts.map(toRaw);
        const pts = sg.closed ? r.slice(0, -1) : r;
        return `M ${pts.map((p) => `${fmtRaw(p.x)} ${fmtRaw(p.y)}`).join(" L ")}${sg.closed ? " Z" : ""}`;
      })
      .join(" ");
    const h = run[0];
    const hole = h.hole ? ` ${DRAWING_HOLE_ATTR}="1"` : "";
    out.push(`<path d="${d}" fill="${runFill ? "black" : "none"}" stroke="${h.stroke ? "black" : "none"}"${hole}/>`);
    run = [];
    runFill = null;
  };
  for (const sg of segs) {
    if (sg.anchors?.length && !sg.closed && sg.pts.length === 2) {
      flush();
      const [a, b] = sg.pts.map(toRaw);
      const attrs = sg.anchors
        .map((an) => {
          const p = an.raw ?? (an.end === 0 ? a : b);
          return ` ${an.end === 0 ? ANCHOR_ATTRS.start : ANCHOR_ATTRS.end}="${an.id},${fmtRaw(p.x)},${fmtRaw(p.y)}"`;
        })
        .join("");
      out.push(`<line x1="${fmtRaw(a.x)}" y1="${fmtRaw(a.y)}" x2="${fmtRaw(b.x)}" y2="${fmtRaw(b.y)}" fill="none" stroke="black"${attrs}/>`);
      continue;
    }
    const h = run[0];
    const fits =
      !!h &&
      h.group === sg.group &&
      h.stroke === sg.stroke &&
      h.hole === sg.hole &&
      (!sg.closed || runFill === null || runFill === sg.filled);
    if (!fits) flush();
    if (sg.closed && runFill === null) runFill = sg.filled;
    run.push(sg);
  }
  flush();
  return out.join("\n  ");
}

/** Mapování zdrojových souřadnic kresby → souřadnice kresby na sklíčku (D),
 *  shodné s krokem 1–3 Rust `process_substrate_paths`. */
function rawToDesignFn(state: ProjectState): ((p: { x: number; y: number }) => { x: number; y: number }) | null {
  const raw = state.rawLoadedPaths;
  const rb = raw ? rawBboxOfSegments(raw) : null;
  if (!rb) return null;
  const { slide_w, slide_h } = state.params;
  const w = rb.mxX - rb.mnX,
    h = rb.mxY - rb.mnY;
  const availW = slide_w - 4,
    availH = slide_h - 4;
  const sf = state.autoScaleFile && (w > availW || h > availH) ? Math.min(availW / w, availH / h) : 1;
  const offX = (slide_w - w * sf) / 2,
    offY = (slide_h - h * sf) / 2;
  return (p) => ({ x: (p.x - rb.mnX) * sf + offX, y: (h - (p.y - rb.mnY)) * sf + offY });
}

function rawBboxOfSegments(paths: SubstratePaths): RawBbox | null {
  let mnX = Infinity,
    mxX = -Infinity,
    mnY = Infinity,
    mxY = -Infinity;
  for (const seg of paths.segments)
    for (const pt of seg.points) {
      if (pt.x < mnX) mnX = pt.x;
      if (pt.x > mxX) mxX = pt.x;
      if (pt.y < mnY) mnY = pt.y;
      if (pt.y > mxY) mxY = pt.y;
    }
  return isFinite(mnX) ? { mnX, mxX, mnY, mxY } : null;
}

/** Obálka tvaru ve world souřadnicích. */
function drawnShapeBounds(shape: DrawnShape | PastedShape): { x0: number; x1: number; y0: number; y1: number } {
  const pts = drawnShapeWorldPoints(shape);
  return {
    x0: Math.min(...pts.map((p) => p.x)),
    x1: Math.max(...pts.map((p) => p.x)),
    y0: Math.min(...pts.map((p) => p.y)),
    y1: Math.max(...pts.map((p) => p.y)),
  };
}

/** Sklíčko (non-prime), se kterým se tvar překrývá nejvíc; -1 = tvar leží mimo
 *  všechna skla. Čára má nulovou plochu — u ní rozhoduje délka překryvu. */
export function findShapeSlide(shape: DrawnShape | PastedShape, positions: LayoutPosition[]): number {
  if (shape.kind === "paths") return shape.target;
  const b = drawnShapeBounds(shape);
  let best = -1,
    bestScore = 0;
  for (let i = 0; i < positions.length; i++) {
    const p = positions[i];
    if (p.is_prime) continue;
    const ox = Math.min(b.x1, p.x + p.width) - Math.max(b.x0, p.x);
    const oy = Math.min(b.y1, p.y + p.height) - Math.max(b.y0, p.y);
    if (ox < 0 || oy < 0) continue;
    const score = Math.max(ox, 1e-6) * Math.max(oy, 1e-6);
    if (score > bestScore) {
      bestScore = score;
      best = i;
    }
  }
  return best;
}

/** Lokální nastavení bez dat vázaných na id objektů (smazání, úpravy, pořadí) —
 *  při výměně zdrojové kresby by stará id zasáhla objekty nové kresby. */
function withoutDeletions(overrides: Record<string, SlideOverride>): Record<string, SlideOverride> {
  const has = (o: SlideOverride) =>
    o.deleted_objects?.length ||
    o.object_order?.length ||
    o.clones?.length ||
    Object.keys(o.object_transforms ?? {}).length;
  if (!Object.values(overrides).some(has)) return overrides;
  return Object.fromEntries(
    Object.entries(overrides).map(([k, o]) => {
      const { deleted_objects: _d, object_transforms: _t, object_order: _o, clones: _c, ...rest } = o;
      return [k, rest];
    })
  );
}

/** Skupina propojených klonů, do které objekt na sklíčku patří (vč. něj), jinak [id]. */
export function cloneGroupOf(overrides: Record<string, SlideOverride>, slide: number, id: number): number[] {
  return overrides[String(slide)]?.clones?.find((g) => g.includes(id)) ?? [id];
}

/** Lokální úprava objektu na sklíčku `slide` (null = zrušit). */
function withLocalXform(
  overrides: Record<string, SlideOverride>,
  slide: number,
  id: number,
  xf: ObjectTransform | null
): Record<string, SlideOverride> {
  const key = String(slide);
  const cur = overrides[key] ?? {};
  const map = { ...(cur.object_transforms ?? {}) };
  if (xf) map[String(id)] = xf;
  else delete map[String(id)];
  const { object_transforms: _, ...rest } = cur;
  return { ...overrides, [key]: Object.keys(map).length ? { ...rest, object_transforms: map } : rest };
}

const cloneJson = <T>(v: T): T => JSON.parse(JSON.stringify(v));

/** Parametry zpracování drah sklíčka `slideIdx` — shodné s Rust `update_layout`.
 *  `rotation` = rotace trasy (kompenzuje úhel výplně), `userScale(Y)` = zapečené měřítko. */
export function sliceParamsFor(
  state: ProjectState,
  slideIdx: number,
  rotation: number,
  userScale: number,
  userScaleY: number
): SliceParams {
  const { params, autoScaleFile } = state;
  const so = state.overrides[String(slideIdx)] ?? {};
  return {
    slide_w: params.slide_w,
    slide_h: params.slide_h,
    margin: 2.0,
    auto_scale: autoScaleFile,
    infill_style: so.infill_style ? so.infill_style : params.infill_style,
    infill_val: so.infill_val ?? params.infill_val ?? 1.0,
    infill_type: so.infill_type ?? params.infill_type ?? "mm",
    infill_angle: (params.infill_angle ?? 0) + rotation,
    nozzle_diam: params.nozzle_diam > 0 ? params.nozzle_diam : 0.4,
    user_scale: userScale,
    user_scale_y: userScaleY,
    object_overrides: so.objects ?? {},
    deleted_objects: so.deleted_objects ?? [],
    object_transforms: so.object_transforms ?? {},
    object_order: so.object_order ?? [],
  };
}

export interface ProjectState {
  params: ProcessParams;
  overrides: Record<string, SlideOverride>;
  rawLoadedPaths: SubstratePaths | null;
  rawFileText: string | null;
  rawFileExt: string | null;
  paths: SubstratePaths[];
  primePath: SubstratePaths | null;
  positions: LayoutPosition[];
  transforms: Transform[];
  /** Per-slide: měřítko naposledy zapečené do geometrie přes rebuildSlicePath.
   *  Ukládá se do metadat GCode a na obnovu se předá rebuildSlicePath. */
  bakedScales: number[];
  /** Per-slide: zapečené měřítko osy Y (odemčený poměr stran); chybí = bakedScales. */
  bakedScalesY: number[];
  autoScaleFile: boolean;
  generatedGCode: string;
  totalDist: number;
  totalTime: number;
  fileName: string;
  isDirty: boolean;
  projectFilePath: string | null;
}

const defaultProjectState: ProjectState = {
  params: {
    sample_count: 1,
    prime_active: true,
    slide_w: 25.0,
    slide_h: 75.0,
    slide_z: 2.0,
    z_offset: 50,
    z_unit: "µm",
    nozzle_height: 30.0,
    nozzle_hidden: 4.0,
    filament_diameter: 9.5,
    flow_multiplier: 1.0,
    bed_temp: BED_OFF,
    extrusion_rate: 200.0,
    extrusion_unit: "nl/mm",
    nozzle_diam: 0.4,
    infill_style: "Okraje + Výplň",
    infill_val: 1.0,
    infill_type: "mm",
    infill_angle: 0,
    print_speed: 600.0,
    nozzle_type: "Červená",
  },
  overrides: {},
  rawLoadedPaths: null,
  rawFileText: null,
  rawFileExt: null,
  paths: [],
  primePath: null,
  positions: [],
  transforms: [],
  bakedScales: [],
  bakedScalesY: [],
  autoScaleFile: false,
  generatedGCode: "",
  totalDist: 0,
  totalTime: 0,
  fileName: "",
  isDirty: false,
  projectFilePath: null,
};

function createProjectStore() {
  const { subscribe, set, update } = writable<ProjectState>(defaultProjectState);
  let layoutTimeout: ReturnType<typeof setTimeout>;

  /** Probíhá debounced přepočet layoutu (ještě nezačal). */
  let layoutPending = false;
  function cancelLayoutUpdate() {
    clearTimeout(layoutTimeout);
    layoutPending = false;
  }
  /** Počet běžících přepočtů geometrie (layout, zapečení měřítka, kresba). */
  let busy = 0;
  async function tracked<T>(job: () => Promise<T>): Promise<T> {
    busy++;
    try {
      return await job();
    } finally {
      busy--;
    }
  }

  // ─── Historie (Ctrl+Z / Ctrl+Y) ───────────────────────────────────────────
  // Zaznamenává se celý dokument (parametry, lokální nastavení, kresba,
  // transformace) automaticky po každé ustálené změně — operace nemusí historii
  // volat samy. Odvozená data (dráhy, pozice) se po návratu přepočítají.
  interface HistorySnap {
    params: ProcessParams;
    overrides: Record<string, SlideOverride>;
    rawLoadedPaths: SubstratePaths | null;
    rawFileText: string | null;
    rawFileExt: string | null;
    fileName: string;
    autoScaleFile: boolean;
    transforms: Transform[];
    bakedScales: number[];
    bakedScalesY: number[];
  }
  const HISTORY_LIMIT = 100;
  /** Změny v tomto okně (psaní do pole, přepočet layoutu) tvoří jeden krok. */
  const HISTORY_SETTLE_MS = 400;
  let undoStack: HistorySnap[] = [];
  let redoStack: HistorySnap[] = [];
  let committed: HistorySnap | null = null;
  let committedKey = "";
  let historyTimer: ReturnType<typeof setTimeout> | undefined;
  let restoring = false;
  let historyQueue: Promise<void> = Promise.resolve();
  // Zdrojové dráhy se neklonují (jsou neměnné) — porovnávají se identitou
  const rawIds = new WeakMap<object, number>();
  let nextRawId = 1;
  const rawId = (p: SubstratePaths | null) => {
    if (!p) return 0;
    let id = rawIds.get(p);
    if (!id) rawIds.set(p, (id = nextRawId++));
    return id;
  };

  function takeSnap(s: ProjectState): HistorySnap {
    // z_calibration nastavuje kalibrace jen dočasně při startu tisku — není to úprava dokumentu
    const { z_calibration: _, ...params } = cloneJson(s.params);
    return {
      params: params as ProcessParams,
      overrides: cloneJson(s.overrides),
      rawLoadedPaths: s.rawLoadedPaths,
      rawFileText: s.rawFileText,
      rawFileExt: s.rawFileExt,
      fileName: s.fileName,
      autoScaleFile: s.autoScaleFile,
      transforms: cloneJson(s.transforms),
      bakedScales: [...s.bakedScales],
      bakedScalesY: [...(s.bakedScalesY ?? [])],
    };
  }
  const snapKey = (h: HistorySnap) =>
    JSON.stringify([
      h.params, h.overrides, rawId(h.rawLoadedPaths), h.rawFileText, h.rawFileExt,
      h.fileName, h.autoScaleFile, h.transforms, h.bakedScales, h.bakedScalesY,
    ]);
  const isSettled = () => busy === 0 && !layoutPending;

  function scheduleCommit() {
    if (restoring) return;
    clearTimeout(historyTimer);
    historyTimer = setTimeout(commit, HISTORY_SETTLE_MS);
  }

  /** Uloží aktuální stav jako krok historie (pokud se od posledního změnil).
   *  Během přepočtu geometrie počká, aby se nezaznamenal rozpracovaný stav. */
  function commit() {
    clearTimeout(historyTimer);
    if (restoring) return;
    if (!isSettled()) {
      historyTimer = setTimeout(commit, HISTORY_SETTLE_MS);
      return;
    }
    const snap = takeSnap(get(store));
    const key = snapKey(snap);
    if (committed && key === committedKey) return;
    if (committed) {
      undoStack.push(committed);
      if (undoStack.length > HISTORY_LIMIT) undoStack.shift();
      redoStack = [];
    }
    committed = snap;
    committedKey = key;
  }

  async function whenSettled() {
    while (!isSettled()) await new Promise((r) => setTimeout(r, 30));
  }

  async function applySnap(h: HistorySnap) {
    restoring = true;
    clearTimeout(historyTimer);
    try {
      update((s) => ({
        ...s,
        params: cloneJson(h.params),
        overrides: cloneJson(h.overrides),
        rawLoadedPaths: h.rawLoadedPaths,
        rawFileText: h.rawFileText,
        rawFileExt: h.rawFileExt,
        fileName: h.fileName,
        autoScaleFile: h.autoScaleFile,
        transforms: cloneJson(h.transforms),
        bakedScales: [...h.bakedScales],
        bakedScalesY: [...h.bakedScalesY],
        isDirty: true,
      }));
      cancelLayoutUpdate();
      await store.doUpdateLayout();
      cancelLayoutUpdate();
      committed = takeSnap(get(store));
      committedKey = snapKey(committed);
    } finally {
      restoring = false;
      clearTimeout(historyTimer);
    }
  }

  /** Spustí krok historie až po dokončení předchozího (rychlé opakované Ctrl+Z). */
  function queueHistory(step: () => Promise<void>): Promise<void> {
    historyQueue = historyQueue.then(step).catch((e) => console.error("Historie:", e));
    return historyQueue;
  }

  const store = {
    subscribe,
    set,
    update,

    // Updates parameters and triggers a debounced layout update
    updateParams: (newParams: Partial<ProcessParams>) => {
      update((state) => ({
        ...state,
        params: { ...state.params, ...newParams },
        isDirty: true,
      }));
      store.triggerLayoutUpdate();
    },

    /** Uzavře rozpracované změny jako samostatný krok historie — volá se před
     *  začátkem interakce (tažení), aby šla vrátit jako celek. */
    pushState: () => commit(),

    undo: () =>
      queueHistory(async () => {
        await whenSettled();
        commit();
        const prev = undoStack.pop();
        if (!prev || !committed) return;
        redoStack.push(committed);
        await applySnap(prev);
      }),

    redo: () =>
      queueHistory(async () => {
        await whenSettled();
        commit();
        const next = redoStack.pop();
        if (!next || !committed) return;
        undoStack.push(committed);
        await applySnap(next);
      }),

    /** Nový projekt (načtení ze souboru) — historie začíná od něj. */
    resetHistory: () => {
      undoStack = [];
      redoStack = [];
      committed = null;
      committedKey = "";
      scheduleCommit();
    },

    setParams: (params: ProcessParams) => {
      update((state) => ({ ...state, params, isDirty: true }));
      store.triggerLayoutUpdate();
    },

    updateOverrides: (newOverrides: Record<string, SlideOverride>) => {
      update((state) => ({ ...state, overrides: newOverrides, isDirty: true }));
      store.triggerLayoutUpdate();
    },

    setRawPaths: (
      paths: SubstratePaths | null,
      autoScale: boolean,
      fileName: string,
      fileText?: string,
      fileExt?: string
    ) => {
      update((state) => ({
        ...state,
        rawLoadedPaths: paths,
        rawFileText: fileText ?? state.rawFileText,
        rawFileExt: fileExt ?? state.rawFileExt,
        autoScaleFile: autoScale,
        fileName,
        bakedScales: [],
        bakedScalesY: [],
        overrides: withoutDeletions(state.overrides),
        isDirty: true,
      }));
      store.triggerLayoutUpdate();
    },

    /**
     * Přidá tvar nakreslený v canvasu. Tvary se serializují do vlastního SVG
     * (1 jednotka = 1 mm), které projde standardní pipeline (parse_svg →
     * layout → infill) — tvar se tak stane trasou jako každý jiný importovaný
     * objekt včetně perzistence do G-code metadat. Pipeline kresbu centruje na
     * sklíčko, proto se po přepočtu layoutu kompenzuje gui_dx/gui_dy tak, aby
     * kresba zůstala přesně v místě nakreslení (na cílovém sklíčku; na ostatní
     * se replikuje se stejným relativním offsetem). Pokud je načten cizí soubor
     * (SVG/DXF z disku), kresba ho nahradí jako nový zdroj.
     */
    addDrawnShape: async (shape: DrawnShape, onlyHere = false): Promise<boolean> => {
      const base = get(store);
      // Tvar mimo všechna skla se nevytvoří
      if (findShapeSlide(shape, base.positions) < 0) return false;
      await tracked(() => placeDrawnShape(base, shape, onlyHere));
      const after = get(store);
      lastDrawnShape.set({
        shape,
        onlyHere,
        base,
        resultText: after.rawFileText,
        resultTransforms: after.transforms,
      });
      return true;
    },

    /** Upraví naposledy nakreslený tvar (rozměry, díra) — umístí ho znovu do
     *  stavu před jeho přidáním, takže výsledek je jako by byl nakreslen rovnou tak. */
    updateLastDrawnShape: async (shape: DrawnShape, onlyHere?: boolean) => {
      const ld = get(lastDrawnShape);
      if (!ld || !isLastDrawnCurrent(ld, get(store))) return;
      const b = ld.base;
      const here = onlyHere ?? ld.onlyHere;
      if (findShapeSlide(shape, b.positions) < 0) return;
      update((s) => ({
        ...s,
        rawLoadedPaths: b.rawLoadedPaths,
        rawFileText: b.rawFileText,
        rawFileExt: b.rawFileExt,
        fileName: b.fileName,
        autoScaleFile: b.autoScaleFile,
        overrides: b.overrides,
        transforms: b.transforms,
        bakedScales: b.bakedScales,
        bakedScalesY: b.bakedScalesY,
      }));
      await tracked(() => placeDrawnShape(b, shape, here));
      const after = get(store);
      lastDrawnShape.set({
        shape,
        onlyHere: here,
        base: b,
        resultText: after.rawFileText,
        resultTransforms: after.transforms,
      });
    },

    /** Přepne prvek zdrojového SVG na díru / zpět (objekt v seznamu objektů). */
    setElementHole: async (element: number | number[], hole: boolean) => {
      const state = get(store);
      if (state.rawFileExt !== "svg" || !state.rawFileText) return;
      const doc = new DOMParser().parseFromString(state.rawFileText, "image/svg+xml");
      const all = doc.getElementsByTagName("*");
      const els = (Array.isArray(element) ? element : [element]).map((i) => all[i]).filter(Boolean);
      if (els.length === 0) return;
      for (const el of els) {
        if (hole) el.setAttribute(DRAWING_HOLE_ATTR, "1");
        else el.removeAttribute(DRAWING_HOLE_ATTR);
      }
      const svgText = new XMLSerializer().serializeToString(doc);
      const parsed = await parse_svg(svgText, get(settingsStore).path_fineness ?? 1.0);
      update((s) => ({ ...s, rawLoadedPaths: parsed, rawFileText: svgText, isDirty: true }));
    },


    reparseRaw: async (fineness: number) => {
      const state = get(store);
      if (!state.rawFileText || !state.rawFileExt) return;
      let reparsed: SubstratePaths;
      if (state.rawFileExt === "svg") {
        reparsed = await parse_svg(state.rawFileText, fineness);
      } else if (state.rawFileExt === "dxf") {
        reparsed = await parse_dxf(state.rawFileText);
      } else {
        return;
      }
      update((s) => ({ ...s, rawLoadedPaths: reparsed, isDirty: true }));
      store.triggerLayoutUpdate();
    },

    /** Smaže objekty z jednoho sklíčka — ostatní skla je tisknou dál. */
    deleteObjects: (slideIdx: number, ids: number[]) => {
      if (ids.length === 0) return;
      update((s) => {
        const key = String(slideIdx);
        const cur = s.overrides[key] ?? {};
        const deleted = [...new Set([...(cur.deleted_objects ?? []), ...ids])].sort((a, b) => a - b);
        return {
          ...s,
          overrides: { ...s.overrides, [key]: { ...cur, deleted_objects: deleted } },
          isDirty: true,
        };
      });
    },

    /** Smaže celou trasu sklíčka (všechny jeho objekty). */
    deleteSlidePath: (slideIdx: number) => {
      const objs = get(store).paths[slideIdx]?.objects ?? [];
      store.deleteObjects(slideIdx, objs.map((o) => o.id));
    },

    /** Vrátí smazané objekty sklíčka. */
    restoreDeletedObjects: (slideIdx: number) => {
      update((s) => {
        const key = String(slideIdx);
        const cur = s.overrides[key];
        if (!cur?.deleted_objects?.length) return s;
        const { deleted_objects: _, ...rest } = cur;
        return { ...s, overrides: { ...s.overrides, [key]: rest }, isDirty: true };
      });
    },

    /** Smaže objekt ze sdílené kresby — zmizí ze všech skel. U SVG se id zapíše
     *  do kořene kresby (přežije uložení i budoucí skla), jinde do všech skel. */
    deleteObjectEverywhere: async (id: number) => {
      const state = get(store);
      if (state.rawFileExt === "svg" && state.rawFileText) {
        const doc = new DOMParser().parseFromString(state.rawFileText, "image/svg+xml");
        const root = doc.documentElement;
        const ids = (root.getAttribute(DRAWING_DELETED_ATTR) ?? "")
          .split(",")
          .filter((v) => v.trim() !== "")
          .map(Number);
        root.setAttribute(DRAWING_DELETED_ATTR, [...new Set([...ids, id])].sort((a, b) => a - b).join(","));
        const svgText = new XMLSerializer().serializeToString(doc);
        const parsed = await tracked(() => parse_svg(svgText, get(settingsStore).path_fineness ?? 1.0));
        update((s) => ({ ...s, rawLoadedPaths: parsed, rawFileText: svgText, isDirty: true }));
      } else {
        for (let i = 0; i < state.paths.length; i++) store.deleteObjects(i, [id]);
      }
    },

    /** Uloží úpravu objektu (posun, měřítko, rotace; null = zrušit). `onlyHere` =
     *  jen sklíčko `slideIdx`; jinak sdílená úprava všech skel (u SVG v kořeni
     *  kresby, přepíše i lokální úpravy objektu), u DXF na všechna skla. */
    setObjectTransform: (slideIdx: number, id: number, xf: ObjectTransform | null, onlyHere: boolean) => {
      // Propojené klony převezmou měřítko a rotaci, poloha zůstává jejich vlastní
      const s = get(store);
      const objs = s.paths[slideIdx]?.objects ?? [];
      const updates: { id: number; xf: ObjectTransform | null }[] = [{ id, xf }];
      for (const m of cloneGroupOf(s.overrides, slideIdx, id)) {
        if (m === id) continue;
        const cur = objs.find((o) => o.id === m)?.xform ?? IDENTITY_XF;
        const next: ObjectTransform = { dx: cur.dx, dy: cur.dy, scale: xf?.scale ?? 1, rotation: xf?.rotation ?? 0 };
        if (xf?.scale_y != null) next.scale_y = xf.scale_y;
        updates.push({ id: m, xf: next });
      }
      return store.setObjectTransforms(updates, onlyHere ? slideIdx : null);
    },

    /** Uloží úpravy více objektů najednou: `slideIdx` = jen toto sklíčko,
     *  null = sdílené úpravy všech skel (viz setObjectTransform). */
    setObjectTransforms: async (updates: { id: number; xf: ObjectTransform | null }[], slideIdx: number | null) => {
      const clean = updates.map(({ id, xf }) => ({ id, xf: xf && !isIdentityXf(xf) ? cloneJson(xf) : null }));
      if (slideIdx != null) {
        update((s) => {
          let overrides = s.overrides;
          for (const u of clean) overrides = withLocalXform(overrides, slideIdx, u.id, u.xf);
          return { ...s, overrides, isDirty: true };
        });
        return;
      }
      const state = get(store);
      if (state.rawFileExt === "svg" && state.rawFileText) {
        const doc = new DOMParser().parseFromString(state.rawFileText, "image/svg+xml");
        const root = doc.documentElement;
        const map = parseObjectTransforms(root.getAttribute(DRAWING_XFORMS_ATTR));
        for (const u of clean) {
          if (u.xf) map[String(u.id)] = u.xf;
          else delete map[String(u.id)];
        }
        const attr = serializeObjectTransforms(map);
        if (attr) root.setAttribute(DRAWING_XFORMS_ATTR, attr);
        else root.removeAttribute(DRAWING_XFORMS_ATTR);
        const svgText = new XMLSerializer().serializeToString(doc);
        const parsed = await tracked(() => parse_svg(svgText, get(settingsStore).path_fineness ?? 1.0));
        update((s) => {
          let overrides = s.overrides;
          for (let i = 0; i < s.paths.length; i++) {
            for (const u of clean) {
              if (overrides[String(i)]?.object_transforms?.[String(u.id)]) overrides = withLocalXform(overrides, i, u.id, null);
            }
          }
          return { ...s, rawLoadedPaths: parsed, rawFileText: svgText, overrides, isDirty: true };
        });
      } else {
        update((s) => {
          let overrides = s.overrides;
          for (let i = 0; i < s.paths.length; i++) for (const u of clean) overrides = withLocalXform(overrides, i, u.id, u.xf);
          return { ...s, overrides, isDirty: true };
        });
      }
    },

    /** Posune objekt o (dx, dy) mm ve world souřadnicích (šipky na klávesnici). */
    moveObjectBy: (slideIdx: number, id: number, dx: number, dy: number, onlyHere: boolean) => {
      const s = get(store);
      const t = s.transforms[slideIdx];
      const obj = s.paths[slideIdx]?.objects?.find((o) => o.id === id);
      if (!t || !obj) return Promise.resolve();
      const f = { t, us: s.bakedScales[slideIdx] ?? 1, usY: s.bakedScalesY?.[slideIdx] ?? s.bakedScales[slideIdx] ?? 1 };
      const a = worldToDesign(f, { x: 0, y: 0 }),
        b = worldToDesign(f, { x: dx, y: dy });
      const xf = obj.xform ?? IDENTITY_XF;
      return store.setObjectTransform(slideIdx, id, { ...xf, dx: xf.dx + b.x - a.x, dy: xf.dy + b.y - a.y }, onlyHere);
    },

    /** Posune objekt v pořadí tisku sklíčka o jedno místo (dir = -1 dřív, +1 později). */
    moveObjectInOrder: (slideIdx: number, id: number, dir: -1 | 1) => {
      update((s) => {
        const ids = (s.paths[slideIdx]?.objects ?? []).map((o) => o.id);
        const i = ids.indexOf(id),
          j = i + dir;
        if (i < 0 || j < 0 || j >= ids.length) return s;
        [ids[i], ids[j]] = [ids[j], ids[i]];
        const key = String(slideIdx);
        return { ...s, overrides: { ...s.overrides, [key]: { ...(s.overrides[key] ?? {}), object_order: ids } }, isDirty: true };
      });
    },

    /** Smaže trasu sklíčka a vrátí ho do čistého stavu: bez rotace, měřítka,
     *  posunu, úprav, pořadí a nastavení objektů (lokální nastavení skla zůstává). */
    resetSlideTrasa: (slideIdx: number) => {
      store.deleteSlidePath(slideIdx);
      update((s) => {
        const posIdx = s.positions.findIndex((_, i) => getTransformIdx(i, s.positions) === slideIdx);
        const pos = s.positions[posIdx];
        const t = s.transforms[slideIdx];
        if (!pos || !t) return s;
        const transforms = [...s.transforms];
        transforms[slideIdx] = { cx: t.cx, cy: t.cy, scale: 1, rotation: 0, gui_dx: pos.x, gui_dy: pos.y };
        const bakedScales = [...s.bakedScales];
        const bakedScalesY = [...(s.bakedScalesY ?? [])];
        bakedScales[slideIdx] = 1;
        bakedScalesY[slideIdx] = 1;
        const key = String(slideIdx);
        const { object_transforms: _t, object_order: _o, objects: _ob, clones: _c, ...rest } = s.overrides[key] ?? {};
        return { ...s, transforms, bakedScales, bakedScalesY, overrides: { ...s.overrides, [key]: rest }, isDirty: true };
      });
    },

    /** Zkopíruje objekty sklíčka `slideIdx` (`ids` = null → celou trasu) do
     *  schránky plátna — geometrie ve world souřadnicích, jak je zobrazená. */
    copyObjects: (slideIdx: number, ids: number[] | null, untransformed = false): CanvasClip | null => {
      const s = get(store);
      const raw = s.rawLoadedPaths;
      const t = s.transforms[slideIdx];
      const toD = rawToDesignFn(s);
      const objs = s.paths[slideIdx]?.objects ?? [];
      const fromPos = s.positions.findIndex((_, i) => getTransformIdx(i, s.positions) === slideIdx);
      if (!raw || !t || !toD || fromPos < 0) return null;
      const chosen = objs.filter((o) => ids === null || ids.includes(o.id));
      if (chosen.length === 0) return null;
      const us = s.bakedScales[slideIdx] ?? 1;
      const f = { t, us, usY: s.bakedScalesY?.[slideIdx] ?? us };
      const byId = new Map(objs.map((o) => [o.id, o]));
      /** Bod objektu `o` po jeho úpravě (prostor kresby D). */
      const moved = (o: PathObject | undefined, d: { x: number; y: number }) => {
        // Klon: geometrie bez úpravy objektu (úpravu převezme jako vlastní)
        if (untransformed) return d;
        const pivot = o ? objectPivot(o) : null;
        return o?.xform && pivot ? xfApply(o.xform, d, pivot) : d;
      };
      const copiedIds = new Set(chosen.map((o) => o.id));
      const srcs = chosen
        .flatMap((o) => (o.sources?.length ? o.sources : [o.id]).map((src) => ({ src, o })))
        .sort((a, b) => a.src - b.src);
      const segs: PastedSeg[] = [];
      for (const { src, o } of srcs) {
        const seg = raw.segments[src];
        if (!seg || seg.points.length < 2) continue;
        const pts = seg.points.map((p) => moved(o, toD(p)));
        // Konce čáry přichycené k objektům sledují jejich úpravu (jako v Rustu)
        for (const a of seg.anchors ?? []) {
          const target = byId.get(a.object);
          if (!target) continue;
          pts[a.end === 0 ? 0 : pts.length - 1] = moved(target, toD({ x: a.x, y: a.y }));
        }
        const first = seg.points[0],
          last = seg.points[seg.points.length - 1];
        segs.push({
          pts: pts.map((p) => designToWorld(f, p)),
          closed: seg.points.length >= 3 && Math.hypot(first.x - last.x, first.y - last.y) < 0.1,
          src,
          group: String(seg.group ?? "-"),
          filled: seg.is_filled ?? true,
          stroke: seg.has_stroke ?? true,
          hole: !!seg.hole,
          // Kotvy jen ke kopírovaným objektům (id se při vložení přemapují)
          anchors: (seg.anchors ?? [])
            .filter((a) => copiedIds.has(a.object))
            .map((a) => ({ end: a.end, id: a.object })),
        });
      }
      const so = s.overrides[String(slideIdx)]?.objects ?? {};
      const overrides: Record<string, ObjectOverride> = {};
      for (const o of chosen) if (so[String(o.id)]) overrides[String(o.id)] = cloneJson(so[String(o.id)]);
      return { whole: ids === null, fromPos, segs, overrides, ids: chosen.map((o) => o.id) };
    },

    /** Vloží schránku plátna na sklíčko `targetPos` (index pozice) na stejné
     *  místo vůči sklu jako u zdroje, posunuté o `offset` (world mm). Vložené
     *  objekty existují jen na cílovém skle. Vrací nová id objektů. */
    pasteClip: (clip: CanvasClip, targetPos: number, offset = { x: 0, y: 0 }): Promise<Map<number, number>> =>
      tracked(async () => {
        await ensureDrawingSvg();
        const base = get(store);
        const from = base.positions[clip.fromPos],
          to = base.positions[targetPos];
        if (!from || !to || to.is_prime || !base.rawLoadedPaths) return new Map();
        const oldCount = base.rawLoadedPaths.segments.length;
        const dx = to.x - from.x + offset.x,
          dy = to.y - from.y + offset.y;
        // Nové id = index segmentu ve vložené kresbě (pořadí segmentů se zachová)
        const newSrc = new Map(clip.segs.map((sg, k) => [sg.src, oldCount + k]));
        const segs: PastedSeg[] = clip.segs.map((sg) => ({
          ...sg,
          pts: sg.pts.map((p) => ({ x: p.x + dx, y: p.y + dy })),
          anchors: sg.anchors
            ?.filter((a) => newSrc.has(a.id))
            .map((a) => ({ end: a.end, id: newSrc.get(a.id)! })),
        }));
        await placeDrawnShape(base, { kind: "paths", target: targetPos, segs }, true);
        lastDrawnShape.set(null);
        const ti = getTransformIdx(targetPos, base.positions);
        const present = new Set((get(store).paths[ti]?.objects ?? []).map((o) => o.id));
        // Nastavení kopírovaných objektů (styl, výplň, začátek hada…) přejde na kopie
        const copied: Record<string, ObjectOverride> = {};
        for (const [oldId, ov] of Object.entries(clip.overrides)) {
          const nid = newSrc.get(Number(oldId));
          if (nid != null) copied[String(nid)] = ov;
        }
        if (Object.keys(copied).length) {
          update((s) => {
            const key = String(ti);
            const cur = s.overrides[key] ?? {};
            return {
              ...s,
              overrides: { ...s.overrides, [key]: { ...cur, objects: { ...(cur.objects ?? {}), ...copied } } },
            };
          });
        }
        // Původní id objektu → id kopie (id objektu = index jeho prvního segmentu)
        const out = new Map<number, number>();
        for (const id of clip.ids) {
          const nid = newSrc.get(id);
          if (nid != null && present.has(nid)) out.set(id, nid);
        }
        return out;
      }),

    /** Klonuje objekty sklíčka (`ids` = null → celou trasu): kopie posunutá
     *  o `offset` (world mm), propojená s originálem — měřítko, rotace
     *  a nastavení objektu se pak přenáší mezi všemi klony. Vrací id klonů. */
    cloneObjects: (slideIdx: number, ids: number[] | null, offset: { x: number; y: number }): Promise<number[]> =>
      tracked(async () => {
        const clip = store.copyObjects(slideIdx, ids, true);
        if (!clip) return [];
        const before = get(store);
        const xfs = new Map((before.paths[slideIdx]?.objects ?? []).map((o) => [o.id, o.xform] as const));
        const map = await store.pasteClip(clip, clip.fromPos, offset);
        // Klon převezme úpravu originálu (geometrie se kopírovala bez ní)
        const updates = [...map]
          .filter(([oldId]) => xfs.get(oldId))
          .map(([oldId, nid]) => ({ id: nid, xf: xfs.get(oldId)! }));
        if (updates.length) await store.setObjectTransforms(updates, null);
        update((s) => {
          const key = String(slideIdx);
          const cur = s.overrides[key] ?? {};
          const groups = (cur.clones ?? []).map((g) => [...g]);
          for (const [oldId, nid] of map) {
            const g = groups.find((x) => x.includes(oldId));
            if (g) g.push(nid);
            else groups.push([oldId, nid]);
          }
          return { ...s, overrides: { ...s.overrides, [key]: { ...cur, clones: groups } }, isDirty: true };
        });
        return [...map.values()];
      }),

    /** Zruší propojení objektu s jeho klony (dál se upravuje samostatně). */
    unlinkClone: (slideIdx: number, id: number) => {
      update((s) => {
        const key = String(slideIdx);
        const cur = s.overrides[key];
        if (!cur?.clones?.length) return s;
        const clones = cur.clones.map((g) => g.filter((x) => x !== id)).filter((g) => g.length > 1);
        const { clones: _, ...rest } = cur;
        return { ...s, overrides: { ...s.overrides, [key]: clones.length ? { ...rest, clones } : rest }, isDirty: true };
      });
    },

    /** Smaže celou kresbu — trasy ze všech skel. */
    clearDesign: () => {
      update((s) => ({
        ...s,
        rawLoadedPaths: null,
        rawFileText: null,
        rawFileExt: null,
        fileName: "",
        autoScaleFile: false,
        overrides: withoutDeletions(s.overrides),
        transforms: [],
        bakedScales: [],
        bakedScalesY: [],
        isDirty: true,
      }));
      lastDrawnShape.set(null);
    },

    setTransforms: (transforms: Transform[]) => {
      update((state) => ({ ...state, transforms }));
    },

    updateTransform: (index: number, transform: Transform) => {
      update((state) => {
        const newTransforms = [...state.transforms];
        newTransforms[index] = transform;
        return { ...state, transforms: newTransforms, isDirty: true };
      });
    },

    setProjectSaved: (filePath: string | null) => {
      update((state) => ({
        ...state,
        isDirty: false,
        projectFilePath: filePath !== null ? filePath : state.projectFilePath,
      }));
    },

    setGCodeResult: (gcode: string, dist: number, time: number) => {
      update((state) => ({ ...state, generatedGCode: gcode, totalDist: dist, totalTime: time }));
    },

    /** Zapeče měřítko do geometrie sklíčka. `scale`/`scaleY` jsou relativní vůči
     *  aktuálně zapečené geometrii (scaleY chybí = rovnoměrné škálování). */
    rebuildSlicePath: (slideIdx: number, scale: number, rotation: number, scaleY?: number) =>
      tracked(async () => {
      const state = get(store);
      const { rawLoadedPaths, bakedScales } = state;
      if (!rawLoadedPaths) return;

      // scale z dragu je relativní vůči aktuálně upečené geometrii → kumulativní total od raw
      const prevBaked = bakedScales[slideIdx] ?? 1.0;
      const totalScale = prevBaked * scale;
      const prevBakedY = state.bakedScalesY?.[slideIdx] ?? prevBaked;
      const totalScaleY = prevBakedY * (scaleY ?? scale);

      // Scale se zapéká do geometrie; rotace kompenzuje úhel výplně
      const sliceParams = sliceParamsFor(state, slideIdx, rotation, totalScale, totalScaleY);

      const processed = await process_substrate_paths(rawLoadedPaths, sliceParams);

      update((s) => {
        const newPaths = [...s.paths];
        newPaths[slideIdx] = processed;
        const newTransforms = [...s.transforms];
        if (newTransforms[slideIdx]) {
          const { scale_y: _, ...t } = newTransforms[slideIdx];
          newTransforms[slideIdx] = { ...t, scale: 1.0 };
        }
        const newBakedScales = [...s.bakedScales];
        while (newBakedScales.length <= slideIdx) newBakedScales.push(1.0);
        newBakedScales[slideIdx] = totalScale;
        const newBakedScalesY = [...(s.bakedScalesY ?? [])];
        while (newBakedScalesY.length <= slideIdx) newBakedScalesY.push(newBakedScales[newBakedScalesY.length] ?? 1.0);
        newBakedScalesY[slideIdx] = totalScaleY;
        return {
          ...s,
          paths: newPaths,
          transforms: newTransforms,
          bakedScales: newBakedScales,
          bakedScalesY: newBakedScalesY,
          isDirty: true,
        };
      });
    }),

    /**
     * Obnoví celý projekt z GCode metadat (Rust GCodeMetadata).
     * Sekvence: nastav stav → doUpdateLayout → přepiš transforms uloženými.
     */
    restoreFromGCode: async (meta: GCodeMetadata, fineness: number) => {
      let parsed: SubstratePaths | null = null;

      if (
        meta.source_file_content &&
        (meta.source_file_ext === "svg" || meta.source_file_ext === "dxf")
      ) {
        const f = meta.fineness || fineness;
        parsed =
          meta.source_file_ext === "svg"
            ? await parse_svg(meta.source_file_content, f)
            : await parse_dxf(meta.source_file_content);
      }

      update((s) => ({
        ...s,
        params: meta.params,
        overrides: meta.overrides,
        rawLoadedPaths: parsed ?? s.rawLoadedPaths,
        rawFileText: meta.source_file_content || s.rawFileText,
        rawFileExt: meta.source_file_ext || s.rawFileExt,
        fileName: meta.source_file_name || s.fileName,
        autoScaleFile: meta.auto_scale,
        // Uložená měřítka se zapečou níže přes rebuildSlicePath (relativně k 1.0)
        bakedScales: [],
        bakedScalesY: [],
        isDirty: false,
      }));

      await store.doUpdateLayout();

      if (meta.transforms && meta.transforms.length > 0) {
        update((s) => {
          const saved = meta.transforms;
          const count = s.transforms.length;
          const restored = saved.slice(0, count);
          while (restored.length < count) restored.push(s.transforms[restored.length]);
          return { ...s, transforms: restored };
        });
      }

      const savedBaked = meta.baked_scales ?? [];
      const savedBakedY = meta.baked_scales_y ?? [];
      const savedTf = meta.transforms ?? [];
      const slideCount = get(store).transforms.length;
      const rebuildJobs: Promise<void>[] = [];
      for (let i = 0; i < slideCount; i++) {
        const bs = savedBaked[i] ?? 1.0;
        const bsY = savedBakedY[i] ?? bs;
        const rot = savedTf[i]?.rotation ?? 0;
        if (Math.abs(bs - 1.0) > 1e-4 || Math.abs(bsY - 1.0) > 1e-4 || Math.abs(rot) > 0.01) {
          rebuildJobs.push(store.rebuildSlicePath(i, bs, rot, bsY));
        }
      }
      await Promise.all(rebuildJobs);

      // Zruší debounced doUpdateLayout spuštěný subscriberem při nastavení params —
      // ten by přepsal právě obnovené cesty a bakedScales zpět na scale=1.
      cancelLayoutUpdate();
      store.resetHistory();
    },

    triggerLayoutUpdate: () => {
      cancelLayoutUpdate();
      layoutPending = true;
      layoutTimeout = setTimeout(() => {
        layoutPending = false;
        store.doUpdateLayout();
      }, 150);
    },

    doUpdateLayout: () => tracked(async () => {
      const state = get(store);
      const settings = get(settingsStore);
      const { params, overrides, rawLoadedPaths, autoScaleFile } = state;

      // Kompletní přepočet (kapacita, dráhy, pozice, transformace, prime náhled)
      // proběhne v jediném Rust volání.
      const res = await update_layout(
        params,
        overrides,
        rawLoadedPaths,
        autoScaleFile,
        state.bakedScales,
        state.bakedScalesY ?? [],
        state.positions,
        state.transforms,
        {
          max_x: settings.bed_max_x || 250.0,
          max_y: settings.bed_max_y || 210.0,
          min_x: settings.bed_min_x ?? 0.0,
          offset_x: settings.start_offset_x || 16.0,
          offset_y: settings.start_offset_y || 12.0,
        },
        settings.multi_spacing || 5.0
      );

      if (res.final_sample_count !== params.sample_count) {
        update((s) => ({
          ...s,
          params: { ...s.params, sample_count: res.final_sample_count },
        }));
      }

      update((s) => ({
        ...s,
        positions: res.positions,
        paths: res.paths,
        primePath: res.prime_path,
        transforms: res.transforms,
        bakedScales: res.baked_scales,
        bakedScalesY: res.baked_scales_y ?? res.baked_scales,
      }));
    }),
  };

  // bind:params={$projectStore.params} v App.svelte obchází explicitní metody store
  // a volá set() přímo — subscriber zachytí tyto změny a spustí layout update.
  let lastParams = defaultProjectState.params;
  let lastOverrides = defaultProjectState.overrides;
  let lastRawPaths = defaultProjectState.rawLoadedPaths;
  subscribe((state) => {
    scheduleCommit();
    if (
      state.params !== lastParams ||
      state.overrides !== lastOverrides ||
      state.rawLoadedPaths !== lastRawPaths
    ) {
      lastParams = state.params;
      lastOverrides = state.overrides;
      lastRawPaths = state.rawLoadedPaths;
      store.triggerLayoutUpdate();
    }
  });

  /** Zajistí, že zdroj je vlastní kresba (SVG s DRAWING_SVG_MARKER), do které
   *  lze vkládat. Jiný zdroj (DXF, cizí SVG) se převede beze změny geometrie:
   *  stejné body, pořadí segmentů (= id objektů) i skupiny prvků; zapečené
   *  automatické měřítko se přepočte do bodů. */
  async function ensureDrawingSvg() {
    const s = get(store);
    const raw = s.rawLoadedPaths;
    if (!raw || raw.segments.length === 0) return;
    if (s.rawFileExt === "svg" && s.rawFileText?.includes(DRAWING_SVG_MARKER)) return;
    const rb = rawBboxOfSegments(raw)!;
    const w = rb.mxX - rb.mnX,
      h = rb.mxY - rb.mnY;
    const availW = s.params.slide_w - 4,
      availH = s.params.slide_h - 4;
    const sf = s.autoScaleFile && (w > availW || h > availH) ? Math.min(availW / w, availH / h) : 1;
    const sc = (p: { x: number; y: number }) => ({ x: p.x * sf, y: p.y * sf });
    const segs: PastedSeg[] = raw.segments.map((seg, src) => {
      const first = seg.points[0],
        last = seg.points[seg.points.length - 1];
      return {
        pts: seg.points,
        closed: seg.points.length >= 3 && Math.hypot(first.x - last.x, first.y - last.y) < 0.1,
        src,
        group: String(seg.group ?? "-"),
        filled: seg.is_filled ?? true,
        stroke: seg.has_stroke ?? true,
        hole: !!seg.hole,
        anchors: seg.anchors?.map((a) => ({ end: a.end, id: a.object, raw: sc({ x: a.x, y: a.y }) })),
      };
    });
    const vbW = Math.max(1, Math.ceil(rb.mxX * sf + 1)),
      vbH = Math.max(1, Math.ceil(rb.mxY * sf + 1));
    const rootAttrs: string[] = [];
    if (raw.deleted_objects?.length) rootAttrs.push(`${DRAWING_DELETED_ATTR}="${raw.deleted_objects.join(",")}"`);
    const xfs = serializeObjectTransforms(raw.object_transforms ?? {});
    if (xfs) rootAttrs.push(`${DRAWING_XFORMS_ATTR}="${xfs}"`);
    const svgText = [
      `<svg xmlns="http://www.w3.org/2000/svg" width="${vbW}mm" height="${vbH}mm" viewBox="0 0 ${vbW} ${vbH}" ${DRAWING_SVG_MARKER}="1"${rootAttrs.map((a) => " " + a).join("")}>`,
      `  ${pastedSegsToSvg(segs, sc)}`,
      `</svg>`,
    ].join("\n");
    const parsed = await parse_svg(svgText, get(settingsStore).path_fineness ?? 1.0);
    if (parsed.segments.length !== raw.segments.length) {
      throw new Error(`Převod zdroje na kresbu změnil počet segmentů (${raw.segments.length} → ${parsed.segments.length})`);
    }
    update((st) => ({ ...st, rawLoadedPaths: parsed, rawFileText: svgText, rawFileExt: "svg", autoScaleFile: false, isDirty: true }));
    cancelLayoutUpdate();
    await store.doUpdateLayout();
    cancelLayoutUpdate();
  }

  /** Umístí tvar do kresby vycházející ze stavu `state` (viz addDrawnShape).
   *  Tvary se serializují do vlastního SVG (1 jednotka = 1 mm), které projde
   *  standardní pipeline (parse_svg → layout → infill). Pipeline kresbu centruje
   *  na sklíčko, proto se po přepočtu layoutu kompenzuje gui_dx/gui_dy tak, aby
   *  kresba zůstala přesně v místě nakreslení. `onlyHere` = tvar se na ostatních
   *  sklech rovnou smaže (vytvoří se jen na cílovém substrátu). */
  async function placeDrawnShape(state: ProjectState, shape: DrawnShape | PastedShape, onlyHere: boolean) {
      const settings = get(settingsStore);
      const { params } = state;
      const slideW = params.slide_w;
      const slideH = params.slide_h;
      const nozzle = params.nozzle_diam ?? 0.4;
      const cx = slideW / 2;
      const cy = slideH / 2;

      const isAppend =
        state.rawFileExt === "svg" &&
        !!state.rawFileText?.includes(DRAWING_SVG_MARKER) &&
        !!state.rawLoadedPaths &&
        state.rawLoadedPaths.segments.length > 0;

      let svgText: string;
      // Mapování raw→sklíčko před přidáním tvaru (pro kompenzaci po layoutu)
      let oldRb: RawBbox | null = null;
      let oldOffX = 0,
        oldOffY = 0;

      if (!isAppend) {
        // Vkládání drah ze schránky předpokládá kresbu (viz ensureDrawingSvg)
        if (shape.kind === "paths") throw new Error("Vkládání vyžaduje vlastní kresbu");
        // Nová kresba: SVG souřadnice = world s Y flipem přes výšku podložky
        const bedW = settings.bed_max_x || 250.0;
        const bedH = settings.bed_max_y || 210.0;
        svgText = [
          `<svg xmlns="http://www.w3.org/2000/svg" width="${bedW}mm" height="${bedH}mm" viewBox="0 0 ${bedW} ${bedH}" ${DRAWING_SVG_MARKER}="1">`,
          `  ${shapeToSvgPrimitive(shape, 0, bedH)}`,
          `</svg>`,
        ].join("\n");
      } else {
        // Přikreslení: nový tvar se promítne do souřadnic kresby inverzí jejího
        // aktuálního zobrazení na cílovém sklíčku, aby zůstal kde byl nakreslen.
        oldRb = rawBboxOfSegments(state.rawLoadedPaths!)!;
        oldOffX = (slideW - (oldRb.mxX - oldRb.mnX)) / 2;
        oldOffY = (slideH - (oldRb.mxY - oldRb.mnY)) / 2;

        const targetIdx = findShapeSlide(shape, state.positions);
        const tIdx = targetIdx >= 0 ? getTransformIdx(targetIdx, state.positions) : -1;
        const t = tIdx >= 0 ? state.transforms[tIdx] : undefined;
        const pos = targetIdx >= 0 ? state.positions[targetIdx] : undefined;
        const guiX = t?.gui_dx ?? pos?.x ?? 0;
        const guiY = t?.gui_dy ?? pos?.y ?? 0;
        const scale = t?.scale ?? 1.0;
        const rot = t?.rotation ?? 0.0;
        const us = (tIdx >= 0 ? state.bakedScales[tIdx] : 1.0) ?? 1.0;
        const usY = (tIdx >= 0 ? state.bakedScalesY?.[tIdx] : undefined) ?? us;

        const rotNorm = ((rot % 360) + 360) % 360;
        const isTranslationOnly =
          Math.abs(scale - 1.0) < 1e-9 &&
          Math.abs(us - 1.0) < 1e-9 &&
          Math.abs(usY - 1.0) < 1e-9 &&
          rotNorm < 1e-9;

        // Plná inverze tpt (rotace/scale kolem středu sklíčka + zapečené měřítko)
        const rad = (rot * Math.PI) / 180;
        const cr = Math.cos(rad),
          sr = Math.sin(rad);
        const rbM = oldRb;
        const toRaw = (w: { x: number; y: number }) => {
          const vx = w.x - guiX - cx;
          const vy = w.y - guiY - cy;
          const px = cx + (vx * cr - vy * sr) / scale;
          const py = cy + (vx * sr + vy * cr) / scale;
          const qx = cx + (px - cx) / us;
          const qy = cy + (py - cy) / usY;
          return { x: qx - oldOffX + rbM.mnX, y: rbM.mxY - (qy - oldOffY) };
        };
        let el: string;
        if (shape.kind === "paths") {
          el = pastedSegsToSvg(shape.segs, toRaw);
        } else if (isTranslationOnly) {
          // Zobrazení je jen posunuté → primitiva zůstanou primitivy
          el = shapeToSvgPrimitive(shape, oldRb.mnX - oldOffX - guiX, oldRb.mxY + oldOffY + guiY);
        } else {
          el = shapeToSvgMapped(shape, toRaw);
        }
        // Konce čáry přichycené magnetem: bod přichycení v souřadnicích
        // neupraveného objektu, aby se ho čára držela i po úpravě objektu
        if (shape.kind === "line" && shape.anchors && t) {
          const rb = oldRb;
          const ends = { start: { x: shape.x1, y: shape.y1 }, end: { x: shape.x2, y: shape.y2 } };
          let attrs = "";
          for (const which of ["start", "end"] as const) {
            const a = shape.anchors[which];
            const obj = a?.slide === tIdx ? state.paths[tIdx]?.objects?.find((o) => o.id === a.id) : undefined;
            if (!a || !obj) continue;
            const f = { t, us, usY };
            let d = worldToDesign(f, ends[which]);
            const pivot = objectPivot(obj);
            if (obj.xform && pivot) d = xfInverse(obj.xform, d, pivot);
            const raw = { x: d.x - oldOffX + rb.mnX, y: rb.mxY - (d.y - oldOffY) };
            attrs += ` ${ANCHOR_ATTRS[which]}="${a.id},${+raw.x.toFixed(6)},${+raw.y.toFixed(6)}"`;
          }
          if (attrs) el = el.replace(/\/>$/, `${attrs}/>`);
        }
        svgText = state.rawFileText!.replace("</svg>", `  ${el}\n</svg>`);
      }

      const parsed = await parse_svg(svgText, settings.path_fineness ?? 1.0);
      if (parsed.segments.length === 0) return;

      if (!isAppend) {
        // Nový zdroj — čisté transformy a zapečená měřítka (jako load souboru)
        update((s) => ({
          ...s,
          rawLoadedPaths: parsed,
          rawFileText: svgText,
          rawFileExt: "svg",
          fileName: "Vlastní kresba",
          autoScaleFile: false,
          overrides: withoutDeletions(s.overrides),
          transforms: [],
          bakedScales: [],
          bakedScalesY: [],
          isDirty: true,
        }));
      } else {
        // Zapečená měřítka a transformy se zachovají — jsou součástí mapování
        update((s) => ({ ...s, rawLoadedPaths: parsed, rawFileText: svgText, isDirty: true }));
      }
      cancelLayoutUpdate(); // subscriber spustil debounce — layout uděláme hned
      await store.doUpdateLayout();
      cancelLayoutUpdate();

      // ── Kompenzace centrování: kresba má zůstat v místě nakreslení ─────────
      const st2 = get(store);
      if (st2.positions.length === 0) return;
      const newRb = rawBboxOfSegments(parsed);
      if (!newRb) return;
      const newOffX = (slideW - (newRb.mxX - newRb.mnX)) / 2;
      const newOffY = (slideH - (newRb.mxY - newRb.mnY)) / 2;

      const newTransforms = [...st2.transforms];
      const clampTo = (t: Transform, posIdx: number, tIdx: number) => {
        const bb = st2.paths[tIdx] ? rawBboxOfSegments(st2.paths[tIdx]) : null;
        if (bb) clampGuidXY(t, st2.positions[posIdx], bb, nozzle);
      };

      if (!isAppend) {
        // Cílové sklíčko: gui tak, aby kresba ležela přesně kde byla nakreslena;
        // world bbox kresby: x = SVG x, y = bedH - SVG y
        const bedH = settings.bed_max_y || 210.0;
        const guiTX = newRb.mnX - newOffX;
        const guiTY = bedH - newRb.mxY - newOffY;
        let targetIdx = findShapeSlide(shape, st2.positions);
        if (targetIdx < 0) targetIdx = st2.positions.findIndex((p) => !p.is_prime);
        if (targetIdx < 0) return;
        const tgtPos = st2.positions[targetIdx];

        st2.positions.forEach((p, i) => {
          if (p.is_prime) return;
          const ti = getTransformIdx(i, st2.positions);
          const nt: Transform = {
            cx,
            cy,
            scale: 1.0,
            rotation: 0.0,
            gui_dx: p.x + (guiTX - tgtPos.x),
            gui_dy: p.y + (guiTY - tgtPos.y),
          };
          clampTo(nt, i, ti);
          newTransforms[ti] = nt;
        });
      } else {
        // Posun lokálních souřadnic způsobený změnou bboxu/centrování — stejný
        // pro všechna sklíčka (v pre-user_scale prostoru)
        const dqx = (newRb.mnX - oldRb!.mnX) + (oldOffX - newOffX);
        const dqy = (oldRb!.mxY - newRb.mxY) + (oldOffY - newOffY);

        st2.positions.forEach((p, i) => {
          if (p.is_prime) return;
          const ti = getTransformIdx(i, st2.positions);
          // Základ = transform PŘED layoutem (fit mohl gui resetovat na střed)
          const base = state.transforms[ti] ?? newTransforms[ti];
          if (!base) return;
          const usI = state.bakedScales[ti] ?? 1.0;
          const usIY = state.bakedScalesY?.[ti] ?? usI;
          const dx = usI * dqx;
          const dy = usIY * dqy;
          const rad = (-base.rotation * Math.PI) / 180; // shodné s tpt
          const cr = Math.cos(rad),
            sr = Math.sin(rad);
          const nt: Transform = {
            ...base,
            gui_dx: base.gui_dx + base.scale * (dx * cr - dy * sr),
            gui_dy: base.gui_dy + base.scale * (dx * sr + dy * cr),
          };
          clampTo(nt, i, ti);
          newTransforms[ti] = nt;
        });
      }

      update((s) => ({ ...s, transforms: newTransforms }));

      // „Vytvořit jen na tento substrát“: nové objekty (id ≥ počet dosavadních
      // zdrojových segmentů) se na ostatních sklech smažou
      if (onlyHere) {
        const targetPos = findShapeSlide(shape, st2.positions);
        const targetT = getTransformIdx(targetPos, st2.positions);
        const oldCount = isAppend ? state.rawLoadedPaths!.segments.length : 0;
        const newIds = (st2.paths[targetT]?.objects ?? []).map((o) => o.id).filter((id) => id >= oldCount);
        st2.paths.forEach((_, ti) => {
          if (ti !== targetT) store.deleteObjects(ti, newIds);
        });
      }
  }

  return store;
}

export const projectStore = createProjectStore();
