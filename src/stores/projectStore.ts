import { writable, derived, get } from "svelte/store";
import {
  update_layout,
  process_substrate_paths,
  parse_dxf,
  parse_svg,
  split_objects,
  find_free_offset,
  type ProcessParams,
  type LayoutPosition,
  type SubstratePaths,
  type Transform,
  type SlideOverride,
  type SliceParams,
  type GCodeMetadata,
  type SceneObject,
  type Placement,
  type PlacementState,
} from "../lib/tauri";
import { settingsStore } from "./settingsStore";
import { clampGuidXY, computeWorldAABB, type RawBbox } from "../lib/geometry";
import { samePathGeometry } from "../lib/nodeMerge";

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

/** Zdrojový soubor importu. Geometrii z něj nese `objects`; text se drží kvůli
 *  přeparsování při změně jemnosti křivek a kvůli uložení do G-kód metadat. */
export interface ImportSource {
  id: string;
  name: string;
  ext: string;
  text: string;
  fineness: number;
  autoScale: boolean;
}

/** Objekt scény doplněný o vazbu na zdroj — `sourceId` + `index` drží pořadí
 *  ve výsledku `split_objects`, takže přeparsování i obnova z metadat trefí
 *  ten samý objekt. Rust pole navíc ignoruje. */
export interface ProjectObject extends SceneObject {
  sourceId: string;
  index: number;
}

const fmtMm = (v: number) => String(+v.toFixed(3));

let _idCounter = 0;
function nextId(prefix: string): string {
  _idCounter += 1;
  return `${prefix}-${Date.now().toString(36)}-${_idCounter}`;
}

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

/** World bbox nakresleného tvaru — cílová poloha objektu na sklíčku. */
function drawnShapeBbox(shape: DrawnShape): RawBbox {
  switch (shape.kind) {
    case "rect":
      return { mnX: shape.x, mxX: shape.x + shape.w, mnY: shape.y, mxY: shape.y + shape.h };
    case "ellipse":
      return {
        mnX: shape.cx - shape.rx,
        mxX: shape.cx + shape.rx,
        mnY: shape.cy - shape.ry,
        mxY: shape.cy + shape.ry,
      };
    case "line":
      return {
        mnX: Math.min(shape.x1, shape.x2),
        mxX: Math.max(shape.x1, shape.x2),
        mnY: Math.min(shape.y1, shape.y2),
        mxY: Math.max(shape.y1, shape.y2),
      };
  }
}

/** SVG primitivum tvaru: r = (wx + tx, ty - wy) — posun + Y flip. */
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

export function rawBboxOfSegments(paths: SubstratePaths | null | undefined): RawBbox | null {
  if (!paths) return null;
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

/** Rám importu pro `SliceParams.frame` — bbox všech jeho objektů dohromady. */
function frameOf(paths: SubstratePaths): [number, number, number, number] {
  const b = rawBboxOfSegments(paths);
  return b ? [b.mnX, b.mxX, b.mnY, b.mxY] : [0, 0, 0, 0];
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
  /** Importované soubory (a vlastní kresby) — zdroj objektů. */
  sources: ImportSource[];
  /** Objekty scény. `slides[i][k]` odpovídá `objects[k]` na každém sklíčku. */
  objects: ProjectObject[];
  /** Umístění objektů po sklíčkách — geometrie, transformace, viditelnost. */
  slides: Placement[][];
  primePath: SubstratePaths | null;
  positions: LayoutPosition[];
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
  sources: [],
  objects: [],
  slides: [],
  primePath: null,
  positions: [],
  generatedGCode: "",
  totalDist: 0,
  totalTime: 0,
  fileName: "",
  isDirty: false,
  projectFilePath: null,
};

function clonePaths(p: SubstratePaths): SubstratePaths {
  return {
    segments: p.segments.map((seg) => ({
      ...seg,
      points: seg.points.map((pt) => ({ x: pt.x, y: pt.y })),
    })),
  };
}

/** Stav umístění bez geometrie — vstup do `update_layout`. */
function toPlacementStates(slides: Placement[][]): PlacementState[][] {
  return slides.map((row) =>
    row.map((p) => ({
      object_id: p.object_id,
      enabled: p.enabled,
      transform: p.transform,
      baked_scale: p.baked_scale,
      nozzle_type: p.nozzle_type ?? null,
      nozzle_diam: p.nozzle_diam ?? null,
    }))
  );
}

/** Efektivní průměr trysky umístění: override objektu, jinak globální hodnota. */
export function placementNozzleDiam(p: Placement | undefined, globalDiam: number): number {
  const d = p?.nozzle_diam;
  return d != null && d > 0 ? d : globalDiam;
}

function createProjectStore() {
  const { subscribe, set, update } = writable<ProjectState>(defaultProjectState);
  let layoutTimeout: ReturnType<typeof setTimeout>;

  /** Snímek pro Ctrl+Z. Trasy se sdílejí referencí — nikdy se nemutují na
   *  místě, vždy se nahrazují novými objekty, takže mělká kopie umístění stačí
   *  a hustá výplň se nekopíruje bod po bodu. Transform se kopíruje, protože
   *  ten plátno při tažení mění na místě. */
  interface UndoEntry {
    sources: ImportSource[];
    objects: ProjectObject[];
    slides: Placement[][];
  }
  let undoStack: UndoEntry[] = [];
  let redoStack: UndoEntry[] = [];

  function snapshot(state: ProjectState): UndoEntry {
    // Zdroje i objekty se drží REFERENCÍ, ne kopií: operace je vždy nahrazují
    // novým polem, takže starý snímek zůstane nedotčený — a hlavně se obnovou
    // nezmění identita pole, která by subscriberu signalizovala změnu scény
    // a spustila přepočet layoutu, jenž by obnovenou geometrii hned přepsal.
    return {
      sources: state.sources,
      objects: state.objects,
      slides: state.slides.map((row) =>
        row.map((p) => ({ ...p, transform: { ...p.transform } }))
      ),
    };
  }

  function restore(entry: UndoEntry) {
    update((s) => ({
      ...s,
      sources: entry.sources,
      objects: entry.objects,
      slides: entry.slides,
      isDirty: true,
    }));
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

    /** Uloží krok pro Ctrl+Z. */
    pushState: () => {
      update((state) => {
        undoStack.push(snapshot(state));
        if (undoStack.length > 50) undoStack.shift();
        redoStack = [];
        return state;
      });
    },

    undo: () => {
      if (undoStack.length === 0) return;
      const prev = undoStack.pop()!;
      redoStack.push(snapshot(get(store)));
      restore(prev);
    },

    redo: () => {
      if (redoStack.length === 0) return;
      const next = redoStack.pop()!;
      undoStack.push(snapshot(get(store)));
      restore(next);
    },

    setParams: (params: ProcessParams) => {
      update((state) => ({ ...state, params, isDirty: true }));
      store.triggerLayoutUpdate();
    },

    updateOverrides: (newOverrides: Record<string, SlideOverride>) => {
      update((state) => ({ ...state, overrides: newOverrides, isDirty: true }));
      store.triggerLayoutUpdate();
    },

    // ─── Objekty ──────────────────────────────────────────────────────────

    /**
     * Přidá objekty z naparsovaného importu. Import se rozdělí na samostatné
     * objekty podle oddělených tvarů; všechny sdílejí rám celého souboru, takže
     * drží vzájemné rozložení. Objekty se založí na všech sklíčkách naráz a
     * skupina se umístí do volného místa, aby nepřekryla ty stávající.
     *
     * `replace: true` (načtení vzorku) nahradí celou scénu, `false` (přidat
     * objekt) ji rozšíří.
     */
    addImport: async (
      parsed: SubstratePaths,
      opts: {
        fileName: string;
        ext: string;
        text: string;
        fineness?: number;
        autoScale?: boolean;
        replace: boolean;
      }
    ): Promise<string[]> => {
      if (parsed.segments.length === 0) return [];
      const settings = get(settingsStore);
      const fineness = opts.fineness ?? settings.path_fineness ?? 1.0;
      const autoScale = opts.autoScale ?? false;

      const parts = await split_objects(parsed);
      const groups = parts.length > 0 ? parts : [parsed];
      const frame = frameOf(parsed);

      const source: ImportSource = {
        id: nextId("src"),
        name: opts.fileName,
        ext: opts.ext,
        text: opts.text,
        fineness,
        autoScale,
      };
      const newObjects: ProjectObject[] = groups.map((raw, k) => ({
        id: nextId("obj"),
        name: groups.length > 1 ? `${opts.fileName} #${k + 1}` : opts.fileName,
        raw,
        frame,
        auto_scale: autoScale,
        sourceId: source.id,
        index: k,
      }));

      update((s) => ({
        ...s,
        sources: opts.replace ? [source] : [...s.sources, source],
        objects: opts.replace ? newObjects : [...s.objects, ...newObjects],
        slides: opts.replace ? [] : s.slides,
        fileName: opts.replace ? opts.fileName : s.fileName || opts.fileName,
        isDirty: true,
      }));

      clearTimeout(layoutTimeout); // subscriber spustil debounce — layout uděláme hned
      await store.doUpdateLayout();
      clearTimeout(layoutTimeout);

      const ids = newObjects.map((o) => o.id);
      if (!opts.replace) await store.packObjects(ids);
      return ids;
    },

    /** Načte SVG/DXF text, naparsuje ho a přidá jako objekty. */
    addImportFromText: async (
      text: string,
      ext: string,
      fileName: string,
      replace: boolean
    ): Promise<string[]> => {
      const fineness = get(settingsStore).path_fineness ?? 1.0;
      let parsed: SubstratePaths;
      if (ext === "svg") parsed = await parse_svg(text, fineness);
      else if (ext === "dxf") parsed = await parse_dxf(text);
      else return [];
      return await store.addImport(parsed, { fileName, ext, text, fineness, replace });
    },

    /**
     * Posune zadané objekty na každém sklíčku do volného místa, aby nepřekryly
     * ty stávající. Skupina se posouvá jako celek — objekty z jednoho importu
     * si tak zachovají vzájemné rozložení ze souboru.
     */
    packObjects: async (objectIds: string[]) => {
      const st = get(store);
      if (objectIds.length === 0 || st.slides.length === 0) return;
      const nonPrime = st.positions.filter((p) => !p.is_prime);
      const globalDiam = st.params.nozzle_diam ?? 0.4;
      const ids = new Set(objectIds);

      const aabbOf = (p: Placement): [number, number, number, number] | null => {
        const raw = rawBboxOfSegments(p.path);
        if (!raw) return null;
        const t = p.transform;
        const a = computeWorldAABB(t.gui_dx, t.gui_dy, t.scale, t.rotation, t.cx, t.cy, raw);
        return [a.minX, a.maxX, a.minY, a.maxY];
      };

      const offsets: ([number, number] | null)[] = [];
      for (let i = 0; i < st.slides.length; i++) {
        const pos = nonPrime[i];
        const row = st.slides[i];
        if (!pos || !row) {
          offsets.push(null);
          continue;
        }
        const occupied: [number, number, number, number][] = [];
        let group: [number, number, number, number] | null = null;
        for (const p of row) {
          const box = aabbOf(p);
          if (!box) continue;
          if (ids.has(p.object_id)) {
            group = group
              ? [
                  Math.min(group[0], box[0]),
                  Math.max(group[1], box[1]),
                  Math.min(group[2], box[2]),
                  Math.max(group[3], box[3]),
                ]
              : box;
          } else if (p.enabled) {
            occupied.push(box);
          }
        }
        if (!group || occupied.length === 0) {
          offsets.push(null);
          continue;
        }
        offsets.push(await find_free_offset(pos, group, occupied, globalDiam));
      }

      update((s) => ({
        ...s,
        slides: s.slides.map((row, i) => {
          const off = offsets[i];
          if (!off) return row;
          return row.map((p) =>
            ids.has(p.object_id)
              ? {
                  ...p,
                  transform: {
                    ...p.transform,
                    gui_dx: p.transform.gui_dx + off[0],
                    gui_dy: p.transform.gui_dy + off[1],
                  },
                }
              : p
          );
        }),
      }));
    },

    /** Zapne/vypne automatické zmenšení pro zdroj i objekty, které z něj vznikly. */
    setSourceAutoScale: (sourceId: string, autoScale: boolean) => {
      update((state) => {
        const src = state.sources.find((s) => s.id === sourceId);
        if (!src || src.autoScale === autoScale) return state;
        return {
          ...state,
          sources: state.sources.map((s) => (s.id === sourceId ? { ...s, autoScale } : s)),
          objects: state.objects.map((o) =>
            o.sourceId === sourceId ? { ...o, auto_scale: autoScale } : o
          ),
          isDirty: true,
        };
      });
    },

    /** Smaže objekt ze všech sklíček (a jeho zdroj, pokud zůstal prázdný). */
    removeObject: (objectId: string) => {
      update((state) => {
        if (!state.objects.some((o) => o.id === objectId)) return state;
        const objects = state.objects.filter((o) => o.id !== objectId);
        const usedSources = new Set(objects.map((o) => o.sourceId));
        return {
          ...state,
          objects,
          sources: state.sources.filter((s) => usedSources.has(s.id)),
          slides: state.slides.map((row) => row.filter((p) => p.object_id !== objectId)),
          isDirty: true,
        };
      });
      store.triggerLayoutUpdate();
    },

    /** Zapne/vypne objekt na jednom skle — vratná náhrada mazání trasy. */
    setObjectEnabled: (slideIdx: number, objIdx: number, enabled: boolean) => {
      update((state) => {
        const row = state.slides[slideIdx];
        if (!row || !row[objIdx]) return state;
        const slides = [...state.slides];
        slides[slideIdx] = [...row];
        slides[slideIdx][objIdx] = { ...row[objIdx], enabled };
        return { ...state, slides, isDirty: true };
      });
      store.triggerLayoutUpdate();
    },

    /** Přenastaví trysku pro jeden objekt na jednom skle (null = globální). */
    setObjectNozzle: (
      slideIdx: number,
      objIdx: number,
      nozzle: { type: string; diam: number } | null
    ) => {
      update((state) => {
        const row = state.slides[slideIdx];
        if (!row || !row[objIdx]) return state;
        const slides = [...state.slides];
        slides[slideIdx] = [...row];
        slides[slideIdx][objIdx] = {
          ...row[objIdx],
          nozzle_type: nozzle ? nozzle.type : null,
          nozzle_diam: nozzle ? nozzle.diam : null,
        };
        return { ...state, slides, isDirty: true };
      });
      store.triggerLayoutUpdate();
    },

    /** Vloží tvar nakreslený v plátně jako samostatný objekt.
     *
     *  Tvar se serializuje do vlastního SVG (1 jednotka = 1 mm), které projde
     *  standardní pipeline (parse_svg → layout → výplň), takže se z něj stane
     *  plnohodnotný objekt včetně perzistence do G-code metadat. Pipeline ho
     *  centruje na sklíčko, proto se výsledná transformace nastaví tak, aby
     *  ležel přesně v místě nakreslení; na ostatní sklíčka se replikuje se
     *  stejným relativním offsetem. */
    addDrawnShape: async (shape: DrawnShape) => {
      const settings = get(settingsStore);
      const state = get(store);
      const { slide_w: slideW, slide_h: slideH } = state.params;
      const nozzle = state.params.nozzle_diam ?? 0.4;
      const bedW = settings.bed_max_x || 250.0;
      const bedH = settings.bed_max_y || 210.0;

      const label =
        shape.kind === "rect" ? "Obdélník" : shape.kind === "ellipse" ? "Elipsa" : "Čára";
      const labelRe = new RegExp(`^${label}( \\d+)?$`);
      const drawnCount = state.sources.filter((s) => labelRe.test(s.name)).length;
      const name = drawnCount > 0 ? `${label} ${drawnCount + 1}` : label;

      const svgText = [
        `<svg xmlns="http://www.w3.org/2000/svg" width="${bedW}mm" height="${bedH}mm" viewBox="0 0 ${bedW} ${bedH}">`,
        `  ${shapeToSvgPrimitive(shape, 0, bedH)}`,
        `</svg>`,
      ].join("\n");

      const parsed = await parse_svg(svgText, settings.path_fineness ?? 1.0);
      if (parsed.segments.length === 0) return;

      const ids = await store.addImport(parsed, {
        fileName: name,
        ext: "svg",
        text: svgText,
        replace: false,
      });
      if (ids.length === 0) return;

      // Kresba má zůstat v místě nakreslení: pipeline ji vycentrovala na
      // sklíčko, takže gui = world bbox tvaru − offset centrování.
      const st2 = get(store);
      const objIdx = st2.objects.findIndex((o) => o.id === ids[0]);
      if (objIdx < 0) return;
      let targetIdx = findTargetPosIdx(drawnShapeCenter(shape), st2.positions);
      if (targetIdx < 0) targetIdx = st2.positions.findIndex((p) => !p.is_prime);
      if (targetIdx < 0) return;
      const tgtPos = st2.positions[targetIdx];

      const wb = drawnShapeBbox(shape);
      const guiTX = wb.mnX - (slideW - (wb.mxX - wb.mnX)) / 2;
      const guiTY = wb.mnY - (slideH - (wb.mxY - wb.mnY)) / 2;

      update((s) => ({
        ...s,
        slides: s.slides.map((row, i) => {
          const pos = s.positions.filter((p) => !p.is_prime)[i];
          const p = row[objIdx];
          if (!pos || !p) return row;
          const t: Transform = {
            ...p.transform,
            gui_dx: pos.x + (guiTX - tgtPos.x),
            gui_dy: pos.y + (guiTY - tgtPos.y),
          };
          const bb = rawBboxOfSegments(p.path);
          if (bb) clampGuidXY(t, pos, bb, placementNozzleDiam(p, nozzle));
          const next = [...row];
          next[objIdx] = { ...p, transform: t };
          return next;
        }),
      }));
    },

    /** Přeparsuje všechny zdroje při změně jemnosti křivek. Id objektů se
     *  zachovají (párují se přes zdroj a pořadí), takže umístění, viditelnost
     *  i tryska přeparsování přežijí. */
    reparseRaw: async (fineness: number) => {
      const state = get(store);
      if (state.sources.length === 0) return;

      const objects: ProjectObject[] = [];
      for (const src of state.sources) {
        const own = state.objects.filter((o) => o.sourceId === src.id);
        let parsed: SubstratePaths | null = null;
        if (src.ext === "svg") parsed = await parse_svg(src.text, fineness);
        else if (src.ext === "dxf") parsed = await parse_dxf(src.text);
        if (!parsed) {
          // Zdroj bez parseru (např. externí G-kód) — objekty necháme být
          objects.push(...own);
          continue;
        }
        const parts = await split_objects(parsed);
        const groups = parts.length > 0 ? parts : [parsed];
        const frame = frameOf(parsed);
        groups.forEach((raw, k) => {
          const prev = own.find((o) => o.index === k);
          objects.push({
            id: prev?.id ?? nextId("obj"),
            name: prev?.name ?? (groups.length > 1 ? `${src.name} #${k + 1}` : src.name),
            raw,
            frame,
            auto_scale: src.autoScale,
            sourceId: src.id,
            index: k,
          });
        });
      }

      update((s) => ({ ...s, objects, isDirty: true }));
      store.triggerLayoutUpdate();
    },

    updateTransform: (slideIdx: number, objIdx: number, transform: Transform) => {
      update((state) => {
        const row = state.slides[slideIdx];
        if (!row || !row[objIdx]) return state;
        const slides = [...state.slides];
        slides[slideIdx] = [...row];
        slides[slideIdx][objIdx] = { ...row[objIdx], transform: { ...transform } };
        return { ...state, slides, isDirty: true };
      });
    },

    /**
     * Zapíše trasu po sloučení uzlů (merge by distance). Volitelně ji promítne
     * i na ostatní sklíčka — jen tam, kde je geometrie téhož objektu shodná
     * s původní trasou, takže individuálně přeškálovaná či pootočená sklíčka
     * zůstanou beze změny.
     */
    applyMergedPath: (
      slideIdx: number,
      objIdx: number,
      merged: SubstratePaths,
      applyToAll: boolean
    ) => {
      update((state) => {
        const source = state.slides[slideIdx]?.[objIdx];
        if (!source) return state;
        const objectId = source.object_id;
        const slides = state.slides.map((row, i) => {
          if (i === slideIdx) {
            const next = [...row];
            next[objIdx] = { ...source, path: merged };
            return next;
          }
          if (!applyToAll) return row;
          const k = row.findIndex((p) => p.object_id === objectId);
          if (k < 0 || !samePathGeometry(row[k].path, source.path)) return row;
          const next = [...row];
          next[k] = { ...row[k], path: clonePaths(merged) };
          return next;
        });
        return { ...state, slides, isDirty: true };
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

    /** Přepeče geometrii jednoho objektu po změně měřítka/rotace v plátně. */
    rebuildSlicePath: async (
      slideIdx: number,
      objIdx: number,
      scale: number,
      rotation: number
    ) => {
      const state = get(store);
      const placement = state.slides[slideIdx]?.[objIdx];
      const obj = placement ? state.objects.find((o) => o.id === placement.object_id) : null;
      if (!placement || !obj) return;
      const { params, overrides } = state;

      // scale z dragu je relativní vůči aktuálně upečené geometrii → kumulativní total od raw
      const totalScale = (placement.baked_scale ?? 1.0) * scale;

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
        auto_scale: obj.auto_scale,
        infill_style: infStyle,
        infill_val: infVal,
        infill_type: infType,
        // Scale se zapéká do geometrie; rotace kompenzuje úhel výplně
        infill_angle: (params.infill_angle ?? 0) + rotation,
        nozzle_diam: placementNozzleDiam(placement, params.nozzle_diam ?? 0.4),
        user_scale: totalScale,
        frame: obj.frame,
      };

      const processed = await process_substrate_paths(obj.raw, sliceParams);

      update((s) => {
        const row = s.slides[slideIdx];
        if (!row || !row[objIdx]) return s;
        const slides = [...s.slides];
        slides[slideIdx] = [...row];
        slides[slideIdx][objIdx] = {
          ...row[objIdx],
          path: processed,
          baked_scale: totalScale,
          transform: { ...row[objIdx].transform, scale: 1.0 },
        };
        return { ...s, slides, isDirty: true };
      });
    },

    /**
     * Obnoví celý projekt z GCode metadat (Rust GCodeMetadata).
     * Zdroje se znovu naparsují a rozdělí — `split_objects` je deterministické,
     * takže objekty vyjdou ve stejném pořadí, v jakém byly uloženy.
     */
    restoreFromGCode: async (meta: GCodeMetadata, fineness: number) => {
      const sources: ImportSource[] = [];
      const objects: ProjectObject[] = [];
      const knownObjectIds = new Set<string>();

      for (const src of meta.sources ?? []) {
        const f = src.fineness || fineness;
        let parsed: SubstratePaths | null = null;
        if (src.ext === "svg") parsed = await parse_svg(src.content, f);
        else if (src.ext === "dxf") parsed = await parse_dxf(src.content);
        if (!parsed) continue;

        sources.push({
          id: src.id,
          name: src.name,
          ext: src.ext,
          text: src.content,
          fineness: f,
          autoScale: src.auto_scale,
        });

        const parts = await split_objects(parsed);
        const groups = parts.length > 0 ? parts : [parsed];
        const frame = frameOf(parsed);
        for (const om of (meta.objects ?? []).filter((o) => o.source_id === src.id)) {
          const raw = groups[om.index];
          if (!raw) continue;
          objects.push({
            id: om.id,
            name: om.name || src.name,
            raw,
            frame,
            auto_scale: src.auto_scale,
            sourceId: src.id,
            index: om.index,
          });
          knownObjectIds.add(om.id);
        }
      }

      const slides: Placement[][] = (meta.slides ?? []).map((row) =>
        row
          .filter((p) => knownObjectIds.has(p.object_id))
          .map((p) => ({
            object_id: p.object_id,
            enabled: p.enabled !== false,
            transform: p.transform,
            baked_scale: p.baked_scale ?? 1.0,
            nozzle_type: p.nozzle_type ?? null,
            nozzle_diam: p.nozzle_diam ?? null,
            path: { segments: [] },
          }))
      );

      update((s) => ({
        ...s,
        params: meta.params,
        overrides: meta.overrides,
        sources,
        objects,
        slides,
        fileName: sources[0]?.name || meta.source_file_name || s.fileName,
        isDirty: false,
      }));

      await store.doUpdateLayout();
      // Zruší debounced doUpdateLayout spuštěný subscriberem při nastavení params
      clearTimeout(layoutTimeout);
    },

    triggerLayoutUpdate: () => {
      clearTimeout(layoutTimeout);
      layoutTimeout = setTimeout(store.doUpdateLayout, 150);
    },

    doUpdateLayout: async () => {
      const state = get(store);
      const settings = get(settingsStore);
      const { params, overrides, objects, slides } = state;

      // Kompletní přepočet (kapacita, dráhy všech objektů, pozice, transformace,
      // prime náhled) proběhne v jediném Rust volání.
      const res = await update_layout(
        params,
        overrides,
        objects,
        toPlacementStates(slides),
        state.positions,
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
        slides: res.slides,
        primePath: res.prime_path,
      }));
    },
  };

  // bind:params={$projectStore.params} v App.svelte obchází explicitní metody store
  // a volá set() přímo — subscriber zachytí tyto změny a spustí layout update.
  let lastParams = defaultProjectState.params;
  let lastOverrides = defaultProjectState.overrides;
  let lastObjects = defaultProjectState.objects;
  subscribe((state) => {
    if (
      state.params !== lastParams ||
      state.overrides !== lastOverrides ||
      state.objects !== lastObjects
    ) {
      lastParams = state.params;
      lastOverrides = state.overrides;
      lastObjects = state.objects;
      store.triggerLayoutUpdate();
    }
  });

  return store;
}

export const projectStore = createProjectStore();
