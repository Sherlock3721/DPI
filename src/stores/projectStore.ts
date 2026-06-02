import { writable, get } from "svelte/store";
import {
  calculate_slide_layout,
  recalculate_layout,
  process_substrate_paths,
  parse_dxf,
  parse_svg,
  get_prime_preview,
  type ProcessParams,
  type LayoutPosition,
  type SubstratePaths,
  type Transform,
  type SlideOverride,
  type SliceParams,
  type GCodeMetadata,
} from "../lib/tauri";
import { settingsStore } from "./settingsStore";

export interface RecentFile {
  path: string;
  name: string;
  timestamp: number;
}

const RECENT_FILES_KEY = "dpi_recent_files";

function loadRecentFiles(): RecentFile[] {
  try {
    const data = localStorage.getItem(RECENT_FILES_KEY);
    if (data) {
      return JSON.parse(data);
    }
  } catch (e) {
    console.error("Chyba při načítání nedávných souborů", e);
  }
  return [];
}

export const recentFilesStore = writable<RecentFile[]>(loadRecentFiles());

export function addRecentFile(path: string, name: string) {
  if (!path.toLowerCase().endsWith(".gcode")) {
    return;
  }
  recentFilesStore.update((files) => {
    // Odstraníme existující záznam se stejnou cestou
    const filtered = files.filter((f) => f.path !== path);
    // Přidáme na začátek
    const newFiles = [{ path, name, timestamp: Date.now() }, ...filtered].slice(0, 10);
    localStorage.setItem(RECENT_FILES_KEY, JSON.stringify(newFiles));
    return newFiles;
  });
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
    z_offset: 0.2,
    z_unit: "mm",
    nozzle_height: 30.0,
    nozzle_hidden: 4.0,
    filament_diameter: 9.5,
    flow_multiplier: 1.0,
    bed_temp: 0,
    extrusion_rate: 1.0,
    extrusion_unit: "nl/mm",
    nozzle_diam: 0.4,
    infill_style: "Okraje + Výplň",
    infill_val: 1.0,
    infill_type: "mm",
    infill_angle: 0,
    print_speed: 1500.0,
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

      const bedMinX = settings.bed_min_x ?? 0.0;

      // Zjistíme absolutní maximum možných vzorků
      const testPositions = await calculate_slide_layout(
        100,
        params.slide_w,
        params.slide_h,
        settings.multi_spacing || 5.0,
        settings.bed_max_x || 250.0,
        settings.bed_max_y || 210.0,
        settings.start_offset_x || 18.0,
        settings.start_offset_y || 11.0,
        params.prime_active,
        bedMinX
      );

      const maxVal = testPositions.filter((p) => !p.is_prime).length || 1;
      const finalSampleCount = Math.min(params.sample_count, maxVal);

      if (finalSampleCount !== params.sample_count) {
        update((s) => ({ ...s, params: { ...s.params, sample_count: finalSampleCount } }));
      }

      let newPaths: SubstratePaths[] = [];
      if (rawLoadedPaths) {
        for (let i = 0; i < finalSampleCount; i++) {
          const slideOverride = overrides[i.toString()] || {};
          // Zachováme aktuální zapečené měřítko a rotaci — bez nich se geometrie resetuje
          const currentBaked = state.bakedScales[i] ?? 1.0;
          const currentRot = state.transforms[i]?.rotation ?? 0;
          const sliceParams: SliceParams = {
            slide_w: params.slide_w,
            slide_h: params.slide_h,
            margin: 2.0,
            auto_scale: autoScaleFile,
            infill_style:
              (slideOverride.infill_style ?? "") !== ""
                ? slideOverride.infill_style!
                : params.infill_style,
            infill_val: slideOverride.infill_val ?? params.infill_val ?? 1.0,
            infill_type: slideOverride.infill_type ?? params.infill_type ?? "mm",
            infill_angle: (params.infill_angle ?? 0) + currentRot,
            nozzle_diam: params.nozzle_diam ?? 0.4,
            user_scale: currentBaked,
          };
          const processed = await process_substrate_paths(rawLoadedPaths, sliceParams);
          newPaths.push(processed);
        }
      } else {
        newPaths = [];
      }

      // Zachováme bakedScales — nové sklíčka dostanou 1.0, existující si udrží hodnotu
      const newBakedScales = Array.from(
        { length: finalSampleCount },
        (_, i) => state.bakedScales[i] ?? 1.0
      );

      // Vypočítáme pozice A přizpůsobíme transformace v jediném Rust volání
      const { positions: calculatedPositions, transforms: newTransforms } =
        await recalculate_layout(
          finalSampleCount,
          params.slide_w,
          params.slide_h,
          settings.multi_spacing || 5.0,
          params.prime_active,
          settings.bed_max_x || 250.0,
          settings.bed_max_y || 210.0,
          settings.bed_min_x ?? 0.0,
          settings.start_offset_x || 18.0,
          settings.start_offset_y || 11.0,
          state.positions,
          state.transforms,
          newPaths,
          params.nozzle_diam || 0.4
        );

      const primePos = calculatedPositions.find((p) => p.is_prime);
      const primePath =
        params.prime_active && primePos
          ? await get_prime_preview(primePos, params, overrides["-1"] ?? null)
          : null;

      update((s) => ({
        ...s,
        positions: calculatedPositions,
        paths: newPaths,
        primePath,
        transforms: newTransforms,
        bakedScales: newBakedScales,
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
