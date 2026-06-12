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
} from "../lib/tauri";
import { settingsStore } from "./settingsStore";
import { clampGuidXY, getTransformIdx, type RawBbox } from "../lib/geometry";

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

/** Tvar nakreslený uživatelem v canvasu — world souřadnice v mm (Y nahoru). */
export type DrawnShape =
  | { kind: "rect"; x: number; y: number; w: number; h: number }
  | { kind: "ellipse"; cx: number; cy: number; rx: number; ry: number }
  | { kind: "line"; x1: number; y1: number; x2: number; y2: number };

/** Atribut, kterým poznáme vlastní generované SVG s kresbou (lze do něj přikreslovat). */
const DRAWING_SVG_MARKER = "data-dpi-drawing";

const fmtMm = (v: number) => String(+v.toFixed(3));

function drawnShapeCenter(shape: DrawnShape): { x: number; y: number } {
  switch (shape.kind) {
    case "rect":
      return { x: shape.x + shape.w / 2, y: shape.y + shape.h / 2 };
    case "ellipse":
      return { x: shape.cx, y: shape.cy };
    case "line":
      return { x: (shape.x1 + shape.x2) / 2, y: (shape.y1 + shape.y2) / 2 };
  }
}

/** Obvodové body tvaru ve world souřadnicích (obdélník 4 rohy, elipsa 64 bodů). */
function drawnShapeWorldPoints(shape: DrawnShape): { x: number; y: number }[] {
  switch (shape.kind) {
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

/** SVG primitiva pro čistě posunuté zobrazení: r = (wx + tx, ty - wy) — posun + Y flip. */
function shapeToSvgPrimitive(shape: DrawnShape, tx: number, ty: number): string {
  switch (shape.kind) {
    case "rect":
      return `<rect x="${fmtMm(shape.x + tx)}" y="${fmtMm(ty - shape.y - shape.h)}" width="${fmtMm(shape.w)}" height="${fmtMm(shape.h)}" fill="black"/>`;
    case "ellipse":
      return `<ellipse cx="${fmtMm(shape.cx + tx)}" cy="${fmtMm(ty - shape.cy)}" rx="${fmtMm(shape.rx)}" ry="${fmtMm(shape.ry)}" fill="black"/>`;
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
  return `<polygon points="${ptStr}" fill="black"/>`;
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

/** Sklíčko, na které tvar patří: obsahuje jeho střed, jinak nejbližší (non-prime). */
function findTargetPosIdx(center: { x: number; y: number }, positions: LayoutPosition[]): number {
  let best = -1,
    bestD = Infinity;
  for (let i = 0; i < positions.length; i++) {
    const p = positions[i];
    if (p.is_prime) continue;
    if (
      center.x >= p.x &&
      center.x <= p.x + p.width &&
      center.y >= p.y &&
      center.y <= p.y + p.height
    ) {
      return i;
    }
    const d = Math.hypot(p.x + p.width / 2 - center.x, p.y + p.height / 2 - center.y);
    if (d < bestD) {
      bestD = d;
      best = i;
    }
  }
  return best;
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
    bed_temp: 0,
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

  interface UndoEntry {
    transforms: string;
    bakedScales: number[];
  }
  let undoStack: UndoEntry[] = [];
  let redoStack: UndoEntry[] = [];

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

    pushState: () => {
      update((state) => {
        undoStack.push({
          transforms: JSON.stringify(state.transforms),
          bakedScales: [...state.bakedScales],
        });
        if (undoStack.length > 50) undoStack.shift();
        redoStack = [];
        return state;
      });
    },

    undo: async () => {
      if (undoStack.length === 0) return;
      const state = get(store);
      redoStack.push({
        transforms: JSON.stringify(state.transforms),
        bakedScales: [...state.bakedScales],
      });
      const prev = undoStack.pop()!;
      const prevTransforms: Transform[] = JSON.parse(prev.transforms);
      const prevBakedScales = prev.bakedScales;

      // Obnov transforms okamžitě (pozice, rotace)
      update((s) => ({ ...s, transforms: prevTransforms }));

      // Pro sklíčka kde se změnilo měřítko nebo rotace přestaví geometrii
      const jobs: Promise<void>[] = [];
      for (let i = 0; i < prevTransforms.length; i++) {
        const curBaked = state.bakedScales[i] ?? 1.0;
        const tgtBaked = prevBakedScales[i] ?? 1.0;
        const prevRot = prevTransforms[i]?.rotation ?? 0;
        const curRot = state.transforms[i]?.rotation ?? 0;
        if (Math.abs(curBaked - tgtBaked) > 1e-6 || Math.abs(prevRot - curRot) > 0.01) {
          // relScale: relativní vůči aktuálně zapečenému měřítku
          const relScale = curBaked > 0 ? tgtBaked / curBaked : tgtBaked;
          jobs.push(store.rebuildSlicePath(i, relScale, prevRot));
        }
      }
      await Promise.all(jobs);
    },

    redo: async () => {
      if (redoStack.length === 0) return;
      const state = get(store);
      undoStack.push({
        transforms: JSON.stringify(state.transforms),
        bakedScales: [...state.bakedScales],
      });
      const next = redoStack.pop()!;
      const nextTransforms: Transform[] = JSON.parse(next.transforms);
      const nextBakedScales = next.bakedScales;

      update((s) => ({ ...s, transforms: nextTransforms }));

      const jobs: Promise<void>[] = [];
      for (let i = 0; i < nextTransforms.length; i++) {
        const curBaked = state.bakedScales[i] ?? 1.0;
        const tgtBaked = nextBakedScales[i] ?? 1.0;
        const nextRot = nextTransforms[i]?.rotation ?? 0;
        const curRot = state.transforms[i]?.rotation ?? 0;
        if (Math.abs(curBaked - tgtBaked) > 1e-6 || Math.abs(nextRot - curRot) > 0.01) {
          const relScale = curBaked > 0 ? tgtBaked / curBaked : tgtBaked;
          jobs.push(store.rebuildSlicePath(i, relScale, nextRot));
        }
      }
      await Promise.all(jobs);
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
    addDrawnShape: async (shape: DrawnShape) => {
      const settings = get(settingsStore);
      const state = get(store);
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

        const targetIdx = findTargetPosIdx(drawnShapeCenter(shape), state.positions);
        const tIdx = targetIdx >= 0 ? getTransformIdx(targetIdx, state.positions) : -1;
        const t = tIdx >= 0 ? state.transforms[tIdx] : undefined;
        const pos = targetIdx >= 0 ? state.positions[targetIdx] : undefined;
        const guiX = t?.gui_dx ?? pos?.x ?? 0;
        const guiY = t?.gui_dy ?? pos?.y ?? 0;
        const scale = t?.scale ?? 1.0;
        const rot = t?.rotation ?? 0.0;
        const us = (tIdx >= 0 ? state.bakedScales[tIdx] : 1.0) ?? 1.0;

        const rotNorm = ((rot % 360) + 360) % 360;
        const isTranslationOnly =
          Math.abs(scale - 1.0) < 1e-9 && Math.abs(us - 1.0) < 1e-9 && rotNorm < 1e-9;

        let el: string;
        if (isTranslationOnly) {
          // Zobrazení je jen posunuté → primitiva zůstanou primitivy
          el = shapeToSvgPrimitive(shape, oldRb.mnX - oldOffX - guiX, oldRb.mxY + oldOffY + guiY);
        } else {
          // Plná inverze tpt (rotace/scale kolem středu sklíčka + zapečené měřítko)
          const rad = (rot * Math.PI) / 180;
          const cr = Math.cos(rad),
            sr = Math.sin(rad);
          const rb = oldRb;
          el = shapeToSvgMapped(shape, (w) => {
            const vx = w.x - guiX - cx;
            const vy = w.y - guiY - cy;
            const px = cx + (vx * cr - vy * sr) / scale;
            const py = cy + (vx * sr + vy * cr) / scale;
            const qx = cx + (px - cx) / us;
            const qy = cy + (py - cy) / us;
            return { x: qx - oldOffX + rb.mnX, y: rb.mxY - (qy - oldOffY) };
          });
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
          transforms: [],
          bakedScales: [],
          isDirty: true,
        }));
      } else {
        // Zapečená měřítka a transformy se zachovají — jsou součástí mapování
        update((s) => ({ ...s, rawLoadedPaths: parsed, rawFileText: svgText, isDirty: true }));
      }
      clearTimeout(layoutTimeout); // subscriber spustil debounce — layout uděláme hned
      await store.doUpdateLayout();
      clearTimeout(layoutTimeout);

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
        let targetIdx = findTargetPosIdx(drawnShapeCenter(shape), st2.positions);
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
          const dx = usI * dqx;
          const dy = usI * dqy;
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

    clearPath: (index: number) => {
      update((state) => {
        const newPaths = [...state.paths];
        if (newPaths[index]) {
          newPaths[index] = { segments: [] };
        }
        return { ...state, paths: newPaths, isDirty: true };
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

    rebuildSlicePath: async (slideIdx: number, scale: number, rotation: number) => {
      const state = get(store);
      const { params, overrides, rawLoadedPaths, autoScaleFile, bakedScales } = state;
      if (!rawLoadedPaths) return;

      // scale z dragu je relativní vůči aktuálně upečené geometrii → kumulativní total od raw
      const prevBaked = bakedScales[slideIdx] ?? 1.0;
      const totalScale = prevBaked * scale;

      const slideOverride = overrides[slideIdx.toString()] || {};
      const infStyle =
        (slideOverride.infill_style ?? "") !== ""
          ? slideOverride.infill_style!
          : params.infill_style;
      const infVal = slideOverride.infill_val ?? params.infill_val ?? 1.0;
      const infType = slideOverride.infill_type ?? params.infill_type ?? "mm";

      const sliceParams: SliceParams = {
        slide_w: params.slide_w,
        slide_h: params.slide_h,
        margin: 2.0,
        auto_scale: autoScaleFile,
        infill_style: infStyle,
        infill_val: infVal,
        infill_type: infType,
        // Scale se zapéká do geometrie; rotace kompenzuje úhel výplně
        infill_angle: (params.infill_angle ?? 0) + rotation,
        nozzle_diam: params.nozzle_diam ?? 0.4,
        user_scale: totalScale,
      };

      const processed = await process_substrate_paths(rawLoadedPaths, sliceParams);

      update((s) => {
        const newPaths = [...s.paths];
        newPaths[slideIdx] = processed;
        const newTransforms = [...s.transforms];
        if (newTransforms[slideIdx]) {
          newTransforms[slideIdx] = { ...newTransforms[slideIdx], scale: 1.0 };
        }
        const newBakedScales = [...s.bakedScales];
        while (newBakedScales.length <= slideIdx) newBakedScales.push(1.0);
        newBakedScales[slideIdx] = totalScale;
        return {
          ...s,
          paths: newPaths,
          transforms: newTransforms,
          bakedScales: newBakedScales,
          isDirty: true,
        };
      });
    },

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
      const savedTf = meta.transforms ?? [];
      const slideCount = get(store).transforms.length;
      const rebuildJobs: Promise<void>[] = [];
      for (let i = 0; i < slideCount; i++) {
        const bs = savedBaked[i] ?? 1.0;
        const rot = savedTf[i]?.rotation ?? 0;
        if (Math.abs(bs - 1.0) > 1e-4 || Math.abs(rot) > 0.01) {
          rebuildJobs.push(store.rebuildSlicePath(i, bs, rot));
        }
      }
      await Promise.all(rebuildJobs);

      // Zruší debounced doUpdateLayout spuštěný subscriberem při nastavení params —
      // ten by přepsal právě obnovené cesty a bakedScales zpět na scale=1.
      clearTimeout(layoutTimeout);
    },

    triggerLayoutUpdate: () => {
      clearTimeout(layoutTimeout);
      layoutTimeout = setTimeout(store.doUpdateLayout, 150);
    },

    doUpdateLayout: async () => {
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
        state.positions,
        state.transforms,
        {
          max_x: settings.bed_max_x || 250.0,
          max_y: settings.bed_max_y || 210.0,
          min_x: settings.bed_min_x ?? 0.0,
          offset_x: settings.start_offset_x || 18.0,
          offset_y: settings.start_offset_y || 11.0,
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
      }));
    },
  };

  // bind:params={$projectStore.params} v App.svelte obchází explicitní metody store
  // a volá set() přímo — subscriber zachytí tyto změny a spustí layout update.
  let lastParams = defaultProjectState.params;
  let lastOverrides = defaultProjectState.overrides;
  let lastRawPaths = defaultProjectState.rawLoadedPaths;
  subscribe((state) => {
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

  return store;
}

export const projectStore = createProjectStore();
