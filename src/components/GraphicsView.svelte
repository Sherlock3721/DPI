<script lang="ts">
  import { run, createBubbler, stopPropagation } from 'svelte/legacy';

  const bubble = createBubbler();
  import { createEventDispatcher, onDestroy, tick } from "svelte";
  import type { LayoutPosition, SubstratePaths, Transform, Point2D, SlideOverride, ObjectTransform } from "../lib/tauri";
  import { getTransformIdx } from "../lib/geometry";
  import { Maximize2, Ruler, Grid2x2, VectorSquare, Play, Pause, ChevronLeft, ChevronRight, Square, Circle, Slash, CircleDashed, X, Magnet, Merge, Box, Route, Palette, Pyramid, ScanEye } from "lucide-svelte";
  import Tooltip from "./Tooltip.svelte";
  import { portal } from "../lib/portal";
  import Viewport from "./Viewport.svelte";
  import { settingsStore } from "../stores/settingsStore";
  import type { ViewColorMode } from "../lib/tauri";
  import MergeNodesPopup from "./MergeNodesPopup.svelte";
  import { samePathGeometry, type MergePreview, type MergeResult } from "../lib/nodeMerge";
  import NumberInput from "./NumberInput.svelte";
  import { printerStore } from "../stores/printerStore";
  import {
    projectStore,
    lastDrawnShape,
    isLastDrawnCurrent,
    type DrawnShape,
    type CanvasClip,
  } from "../stores/projectStore";

  const dispatch = createEventDispatcher();

  interface Props {
    bedMaxX?: number;
    bedMaxY?: number;
    positions?: LayoutPosition[];
    paths?: SubstratePaths[];
    primePath?: SubstratePaths | null;
    transforms?: Transform[];
    overrides?: Record<string, SlideOverride>;
    currentNozzle?: Point2D | null;
    nozzleDiam?: number;
    externalSelectedIndex?: any;
    totalPreviewTime?: number; // celkový čas tisku v sekundách (0 = neznámý)
    extrusionRateUl?: number; // µl/mm pro zobrazení objemu v overlay
    suspendAutoCenter?: boolean; // dokud true (např. welcome screen), centrování se odloží
    /** Objekt rozbalený v lokálním panelu — zvýrazní se v canvasu. */
    focusedObject?: { slide: number; id: number } | null;
  }

  let {
    bedMaxX = 250.0,
    bedMaxY = 210.0,
    positions = [],
    paths = $bindable([]),
    primePath = null,
    transforms = [],
    overrides = {},
    currentNozzle = null,
    nozzleDiam = 0.4,
    externalSelectedIndex = -1,
    totalPreviewTime = 0,
    extrusionRateUl = 0,
    suspendAutoCenter = false,
    focusedObject = null
  }: Props = $props();

  let selectedIndex = $state(-1);

  run(() => {
    if (externalSelectedIndex >= 0 && externalSelectedIndex !== selectedIndex) {
      selectedIndex = externalSelectedIndex;
    }
  });

  let showAxes = $state(true);
  let isMeasuring = $state(false);
  // Aktivní kreslicí nástroj — kreslení a měření se vzájemně vylučují
  let drawTool: "rect" | "ellipse" | "line" | null = $state(null);
  let measurePoints: { x: number; y: number }[] = $state([]);
  // Kontextové menu: souřadnice v rámci panelu (panel má backdrop-filter, takže
  // position: fixed by se počítalo od něj, ne od okna — proto absolute + ořez do panelu)
  let contextMenu = $state({ visible: false, x: 0, y: 0, slideIndex: -1 });
  let showMergePopup = $state(false);
  let mergePreview: MergePreview | null = $state(null);
  let rootEl: HTMLDivElement | undefined = $state();
  let menuEl: HTMLDivElement | undefined = $state();
  let canvasRef: any = $state();
  let printProgress = $state(100);
  /** Délka časové osy náhledu (s) — počítá Viewport z rychlostí objektů/skel. */
  let previewTotalTime = $state(0);

  // ─── Play / Step logika ──────────────────────────────────────────────────
  let isPlaying = $state(false);
  let rafId: number | null = null;
  let playStartMs = 0;
  let playStartProgress = 0;
  // Rychlost přehrávání: 1× = reálný čas tisku podle nastavených rychlostí
  const PLAY_SPEEDS = [1, 2, 5, 10, 50];
  let playSpeed = $state(1);

  function getAnimDuration(): number {
    // ms na celou animaci od 0 do 100 %
    const totalSec = previewTotalTime > 0 ? previewTotalTime : totalPreviewTime > 0 ? totalPreviewTime : 60;
    return (totalSec / playSpeed) * 1000;
  }

  function cyclePlaySpeed() {
    const next = PLAY_SPEEDS[(PLAY_SPEEDS.indexOf(playSpeed) + 1) % PLAY_SPEEDS.length];
    if (isPlaying) {
      // Pokračovat plynule z aktuální pozice novou rychlostí
      playStartMs = performance.now();
      playStartProgress = printProgress;
    }
    playSpeed = next;
  }

  // Spodní panel náhledu se vysune při najetí myší ke spodnímu okraji plátna
  // (i pod ním, mimo plátno); při přehrávání (časová osa < 100 %) zůstává
  // zobrazený a nejde schovat.
  const TIMELINE_REVEAL_PX = 56;
  let timelineHover = $state(false);
  const timelineVisible = $derived(isPlaying || printProgress < 100 || timelineHover);

  function updateTimelineHover(e: PointerEvent) {
    if (!rootEl) return;
    const r = rootEl.getBoundingClientRect();
    timelineHover = e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.bottom - TIMELINE_REVEAL_PX;
  }

  function formatPreviewTime(progress: number): string {
    const total = previewTotalTime > 0 ? previewTotalTime : 0;
    const sec = Math.round((total * progress) / 100);
    return `${Math.floor(sec / 60)}:${String(sec % 60).padStart(2, "0")}`;
  }

  function startPlay() {
    if (isPlaying) return;
    if (printProgress >= 100) printProgress = 0;
    isPlaying = true;
    playStartMs = performance.now();
    playStartProgress = printProgress;
    scheduleFrame();
  }

  function stopPlay() {
    isPlaying = false;
    if (rafId !== null) { cancelAnimationFrame(rafId); rafId = null; }
  }

  function scheduleFrame() {
    rafId = requestAnimationFrame((now) => {
      if (!isPlaying) return;
      const elapsed = now - playStartMs;
      const animDuration = getAnimDuration();
      const newProgress = playStartProgress + (elapsed / animDuration) * 100;
      if (newProgress >= 100) {
        printProgress = 100;
        stopPlay();
      } else {
        printProgress = newProgress;
        scheduleFrame();
      }
    });
  }

  function stepForward() {
    stopPlay();
    printProgress = Math.min(100, printProgress + 1);
  }

  function stepBack() {
    stopPlay();
    printProgress = Math.max(0, printProgress - 1);
  }

  // Zastavit přehrávání při zahájení reálného tisku
  run(() => {
    if ($printerStore.is_printing) { stopPlay(); }
  });

  onDestroy(() => stopPlay());

  // Při tisku automaticky synchronizuj slider s reálným průběhem
  run(() => {
    if ($printerStore.is_printing) printProgress = $printerStore.progress;
  });

  // Při změně počtu sklíček/pozic resetuj pohled kamery, aby byla všechna sklíčka viditelná.
  // Pokud je centrování odloženo (např. běží welcome screen), provedeme ho až po jeho zavření.
  let prevPositionCount = $state(-1);
  let pendingAutoCenter = $state(false);
  run(() => {
    if (positions.length !== prevPositionCount && canvasRef?.resetCamera) {
      prevPositionCount = positions.length;
      if (positions.length > 0) {
        if (suspendAutoCenter) pendingAutoCenter = true;
        else canvasRef.fitAll();
      }
    }
  });
  run(() => {
    if (!suspendAutoCenter && pendingAutoCenter && canvasRef?.resetCamera) {
      pendingAutoCenter = false;
      canvasRef.fitAll();
    }
  });

  function getSelectedTransform(): Transform | null {
    const tidx = getTransformIdx(selectedIndex, positions);
    return transforms[tidx] ?? null;
  }

  function handleSlideSelected(e: CustomEvent<number>) {
    selectedIndex = e.detail;
    dispatch("slideSelected", selectedIndex);
    isMeasuring = false;
  }

  async function handleSlideContext(e: CustomEvent<{ index: number; x: number; y: number }>) {
    selectedIndex = e.detail.index;
    dispatch("slideSelected", selectedIndex);
    if (!rootEl) return;
    const r = rootEl.getBoundingClientRect();
    contextMenu = { visible: true, x: e.detail.x - r.left, y: e.detail.y - r.top, slideIndex: e.detail.index };
    // Menu u kurzoru, ale celé uvnitř panelu (u pravého/dolního okraje se otočí)
    await tick();
    if (!menuEl) return;
    const m = menuEl.getBoundingClientRect();
    const PAD = 4;
    let { x, y } = contextMenu;
    if (x + m.width > r.width - PAD) x = Math.max(PAD, x - m.width);
    if (y + m.height > r.height - PAD) y = Math.max(PAD, r.height - m.height - PAD);
    contextMenu.x = x;
    contextMenu.y = y;
  }

  function handleMeasurePointsChange(e: CustomEvent<{ x: number; y: number }[]>) {
    measurePoints = e.detail;
  }

  function toggleDrawTool(tool: "rect" | "ellipse" | "line") {
    drawTool = drawTool === tool ? null : tool;
    if (drawTool) {
      isMeasuring = false;
      measurePoints = [];
      closeMergePopup();
    }
  }

  // ─── Sloučení uzlů (merge by distance) ───────────────────────────────────
  function hasPath(tIdx: number): boolean {
    const p = paths[tIdx];
    return !!p && p.segments && p.segments.length > 0;
  }

  let mergeTIdx = $derived.by(() => {
    if (selectedIndex >= 0 && !positions[selectedIndex]?.is_prime) {
      const t = getTransformIdx(selectedIndex, positions);
      if (t >= 0 && hasPath(t)) return t;
    }
    for (let i = 0; i < paths.length; i++) {
      if (hasPath(i)) return i;
    }
    return -1;
  });

  let mergePathData = $derived(mergeTIdx >= 0 ? paths[mergeTIdx] ?? null : null);
  let mergeTransform = $derived(mergeTIdx >= 0 ? transforms[mergeTIdx] ?? null : null);

  let mergeTwinCount = $derived.by(() => {
    if (!mergePathData || mergeTIdx < 0) return 0;
    let n = 0;
    for (let i = 0; i < paths.length; i++) {
      if (i === mergeTIdx) continue;
      if (paths[i] && samePathGeometry(paths[i], mergePathData)) n++;
    }
    return n;
  });

  function toggleMergePopup() {
    showMergePopup = !showMergePopup;
    if (showMergePopup) {
      isMeasuring = false;
      drawTool = null;
    } else {
      mergePreview = null;
    }
  }

  function closeMergePopup() {
    showMergePopup = false;
    mergePreview = null;
  }

  function handleMergeApply(e: CustomEvent<MergeResult & { applyToAll: boolean }>) {
    if (mergeTIdx < 0) return;
    dispatch("saveState");
    projectStore.applyMergedPath(
      mergeTIdx,
      e.detail.paths,
      e.detail.applyToAll
    );
  }

  // Kreslení díry: obdélník/elipsa se odečte z ostatních tvarů
  let drawHole = $state(false);
  // Magnet (klávesa M): body tvarů se přichytávají k trasám
  let snapToPaths = $state(false);
  // Nový tvar jen na substrát, kde se kreslí (jinak na všechna skla)
  let drawOnlyHere = $state(false);
  // Úprava objektu (posun, měřítko, rotace) jen na substrátu, kde je vybraný
  let editObjOnlyHere = $state(false);

  /** Uloží úpravu objektu z plátna (sdílenou, nebo jen pro jeho substrát). */
  function handleObjectTransform(e: CustomEvent<{ slide: number; id: number; xform: ObjectTransform }>) {
    const { slide, id, xform } = e.detail;
    projectStore.setObjectTransform(slide, id, xform, editObjOnlyHere).catch((err) => console.error(err));
  }

  /** Vybraný objekt má úpravu (posun, měřítko, rotaci), kterou lze vrátit. */
  function focusedHasXform(): boolean {
    if (!focusedObject) return false;
    const o = paths[focusedObject.slide]?.objects?.find((x) => x.id === focusedObject!.id);
    return !!o?.xform;
  }

  // Krátké upozornění v plátně (např. tvar nakreslený mimo sklo)
  let notice = $state("");
  let _noticeTimer: ReturnType<typeof setTimeout> | undefined;
  function showNotice(text: string) {
    notice = text;
    clearTimeout(_noticeTimer);
    _noticeTimer = setTimeout(() => (notice = ""), 2500);
  }

  function handleShapeDrawn(e: CustomEvent<DrawnShape>) {
    const shape = e.detail.kind === "line" ? e.detail : { ...e.detail, hole: drawHole };
    projectStore
      .addDrawnShape(shape, drawOnlyHere)
      .then((created) => {
        if (!created) showNotice("Tvar leží mimo všechna skla — nevytvořil se.");
      })
      .catch((err) => {
        console.error("Nepodařilo se přidat nakreslený tvar:", err);
      });
  }

  // ─── Panel posledního tvaru (jako „Adjust Last Operation“ v Blenderu) ─────
  let lastShapeActive = $derived(isLastDrawnCurrent($lastDrawnShape, $projectStore));
  let editW = $state(0);
  let editH = $state(0);
  let editHole = $state(false);
  let editOnlyHere = $state(false);
  let _editSource: unknown = null;
  run(() => {
    const ld = $lastDrawnShape;
    if (!ld || ld === _editSource) return;
    _editSource = ld;
    editOnlyHere = ld.onlyHere;
    const sh = ld.shape;
    if (sh.kind === "rect") {
      editW = +sh.w.toFixed(3);
      editH = +sh.h.toFixed(3);
      editHole = !!sh.hole;
    } else if (sh.kind === "ellipse") {
      editW = +(sh.rx * 2).toFixed(3);
      editH = +(sh.ry * 2).toFixed(3);
      editHole = !!sh.hole;
    } else {
      editW = +Math.hypot(sh.x2 - sh.x1, sh.y2 - sh.y1).toFixed(3);
    }
  });

  /** Tvar s novými rozměry — střed zůstává na místě. */
  function editedShape(sh: DrawnShape): DrawnShape | null {
    const w = Number(editW),
      h = Number(editH);
    if (!(w > 0) || (sh.kind !== "line" && !(h > 0))) return null;
    if (sh.kind === "rect") {
      const cx = sh.x + sh.w / 2,
        cy = sh.y + sh.h / 2;
      return { kind: "rect", x: cx - w / 2, y: cy - h / 2, w, h, hole: editHole };
    }
    if (sh.kind === "ellipse") {
      return { kind: "ellipse", cx: sh.cx, cy: sh.cy, rx: w / 2, ry: h / 2, hole: editHole };
    }
    const mx = (sh.x1 + sh.x2) / 2,
      my = (sh.y1 + sh.y2) / 2;
    const a = Math.atan2(sh.y2 - sh.y1, sh.x2 - sh.x1);
    const dx = (Math.cos(a) * w) / 2,
      dy = (Math.sin(a) * w) / 2;
    return { kind: "line", x1: mx - dx, y1: my - dy, x2: mx + dx, y2: my + dy };
  }

  let _editTimer: ReturnType<typeof setTimeout> | undefined;
  function applyShapeEdit() {
    clearTimeout(_editTimer);
    _editTimer = setTimeout(() => {
      const ld = $lastDrawnShape;
      const next = ld ? editedShape(ld.shape) : null;
      if (next) projectStore.updateLastDrawnShape(next, editOnlyHere).catch((err) => console.error(err));
    }, 300);
  }

  const shapeTitle = (k: DrawnShape["kind"]) =>
    k === "rect" ? "Obdélník" : k === "ellipse" ? "Elipsa" : "Čára";

  function handleTransformChanged(e: CustomEvent<{ index: number; transform: Transform }>) {
    dispatch("transformChanged", e.detail);
  }

  function handleSaveState() {
    dispatch("saveState");
  }

  function handlePathRebuildNeeded(
    e: CustomEvent<{ slideIdx: number; scale: number; rotation: number }>
  ) {
    dispatch("pathRebuildNeeded", e.detail);
  }

  function closeContextMenu() {
    contextMenu.visible = false;
  }

  // ─── Schránka plátna (Ctrl+C / Ctrl+V / Ctrl+D) ───────────────────────────
  let clip: CanvasClip | null = $state(null);
  /** Počet vložení schránky na jednotlivá skla — každé další se posune. */
  let pasteCounts = new Map<number, number>();
  /** Posun kopie na stejném místě, aby se nesloučila s originálem (mm). */
  const PASTE_STEP = { x: 2, y: -2 };

  /** Vybraný objekt (nebo celá trasa) vybraného skla. */
  function selectionToClip(): CanvasClip | null {
    if (selectedIndex < 0 || selectedIndex >= positions.length || positions[selectedIndex].is_prime) return null;
    const tIdx = getTransformIdx(selectedIndex, positions);
    const ids = focusedObject?.slide === tIdx ? [focusedObject.id] : null;
    return projectStore.copyObjects(tIdx, ids);
  }

  async function pasteTo(c: CanvasClip, posIdx: number, offset: { x: number; y: number }) {
    dispatch("saveState");
    try {
      const ids = [...(await projectStore.pasteClip(c, posIdx, offset)).values()];
      selectPasted(posIdx, ids);
    } catch (err) {
      console.error("Vložení selhalo:", err);
      showNotice("Vložení se nepodařilo.");
    }
  }

  /** Klonuje vybraný objekt (bez objektu celou trasu) — propojená kopie. */
  async function cloneSelection() {
    if (selectedIndex < 0 || selectedIndex >= positions.length || positions[selectedIndex].is_prime) return;
    const posIdx = selectedIndex;
    const tIdx = getTransformIdx(posIdx, positions);
    const ids = focusedObject?.slide === tIdx ? [focusedObject.id] : null;
    dispatch("saveState");
    try {
      selectPasted(posIdx, await projectStore.cloneObjects(tIdx, ids, PASTE_STEP));
    } catch (err) {
      console.error("Klonování selhalo:", err);
      showNotice("Klonování se nepodařilo.");
    }
  }

  function selectPasted(posIdx: number, ids: number[]) {
    {
      const tIdx = getTransformIdx(posIdx, positions);
      selectedIndex = posIdx;
      dispatch("slideSelected", posIdx);
      dispatch("objectSelected", ids.length === 1 ? { slide: tIdx, id: ids[0] } : null);
      if (ids.length === 0) showNotice("Nic se nevložilo.");
    }
  }

  function handleClipboardKey(e: KeyboardEvent): boolean {
    if (!e.ctrlKey || e.metaKey) return false;
    // Ctrl+Alt+D = klon (Alt může změnit e.key, proto i podle fyzické klávesy)
    if (e.altKey) {
      if (e.code !== "KeyD" && e.key.toLowerCase() !== "d") return false;
      if ($printerStore.is_printing) return false;
      e.preventDefault();
      cloneSelection();
      return true;
    }
    const k = e.key.toLowerCase();
    if (k !== "c" && k !== "v" && k !== "d") return false;
    // Označený text v panelech se kopíruje normálně
    if (k === "c" && (window.getSelection()?.toString() ?? "") !== "") return false;
    if ($printerStore.is_printing) return false;
    e.preventDefault();
    if (k === "c") {
      const c = selectionToClip();
      if (c) {
        clip = c;
        pasteCounts = new Map();
        showNotice(c.whole ? "Trasa zkopírována." : "Objekt zkopírován.");
      }
    } else if (k === "v") {
      if (!clip) return true;
      const target = selectedIndex >= 0 && !positions[selectedIndex]?.is_prime ? selectedIndex : clip.fromPos;
      // Na stejné místo (stejné sklo, opakované vložení) se kopie posune
      const n = (pasteCounts.get(target) ?? 0) + (target === clip.fromPos ? 1 : 0);
      pasteCounts.set(target, (pasteCounts.get(target) ?? 0) + 1);
      pasteTo(clip, target, { x: PASTE_STEP.x * n, y: PASTE_STEP.y * n });
    } else {
      const c = selectionToClip();
      if (c) pasteTo(c, c.fromPos, PASTE_STEP);
    }
    return true;
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape" && showMergePopup) {
      e.preventDefault();
      closeMergePopup();
      return;
    }
    // Psaní do polí má přednost; zaškrtávací pole ani tlačítka zkratky neblokují
    const el = e.target;
    if (el instanceof HTMLTextAreaElement) return;
    if (el instanceof HTMLInputElement && !["checkbox", "radio", "button", "range"].includes(el.type)) return;
    if (handleClipboardKey(e)) return;

    if (e.key.toLowerCase() === "m" && !e.ctrlKey && !e.altKey && !e.metaKey) {
      snapToPaths = !snapToPaths;
      return;
    }

    // Delete: vybraný objekt (v plátně nebo v lokálním panelu) → smazat objekt,
    // jinak celou trasu vybraného skla; Shift = ze všech substrátů
    if ((e.key === "Delete" || e.key === "Backspace") && e.code !== "NumpadDecimal" && !$printerStore.is_printing) {
      if (focusedObject) {
        e.preventDefault();
        deleteObjectById(focusedObject.slide, focusedObject.id, e.shiftKey);
        return;
      }
      if (selectedIndex >= 0 && selectedIndex < positions.length && !positions[selectedIndex].is_prime) {
        e.preventDefault();
        deletePath(selectedIndex, e.shiftKey);
        return;
      }
    }

    // Šipky pro krokování náhledu (jen když netiskne a není vybráno sklíčko)
    if (!$printerStore.is_printing && (selectedIndex < 0 || selectedIndex >= positions.length)) {
      if (e.key === "ArrowLeft") { e.preventDefault(); stepBack(); return; }
      if (e.key === "ArrowRight") { e.preventDefault(); stepForward(); return; }
    }

    if (selectedIndex < 0 || selectedIndex >= positions.length) return;
    const pos = positions[selectedIndex];
    if (pos.is_prime) return;
    const tIdx = getTransformIdx(selectedIndex, positions);
    const t = transforms[tIdx];
    if (!t) return;

    const step = e.shiftKey ? 0.1 : 1.0;
    // Šipka posouvá v ose světa nejbližší jejímu směru na obrazovce (natočený pohled)
    const ax: { up: [number, number]; right: [number, number] } = canvasRef?.arrowAxes?.() ?? { up: [0, 1], right: [1, 0] };
    const d: Record<string, [number, number]> = {
      ArrowUp: [ax.up[0] * step, ax.up[1] * step],
      ArrowDown: [-ax.up[0] * step, -ax.up[1] * step],
      ArrowLeft: [-ax.right[0] * step, -ax.right[1] * step],
      ArrowRight: [ax.right[0] * step, ax.right[1] * step],
    };
    // Vybraný objekt na této trase: šipky posouvají objekt, ne celou trasu
    if (focusedObject?.slide === tIdx && e.key.startsWith("Arrow")) {
      const v = d[e.key];
      if (v) {
        e.preventDefault();
        projectStore.moveObjectBy(tIdx, focusedObject.id, v[0], v[1], editObjOnlyHere).catch((err) => console.error(err));
      }
      return;
    }
    const v = d[e.key];
    if (v) {
      t.gui_dx += v[0];
      t.gui_dy += v[1];
      e.preventDefault();
      dispatch("transformChanged", { index: tIdx, transform: t });
    }
  }

  function runAction(actionType: string) {
    const idx = contextMenu.slideIndex !== -1 ? contextMenu.slideIndex : selectedIndex;
    if (idx < 0 || idx >= positions.length) return;
    const pos = positions[idx];
    const tIdx = getTransformIdx(idx, positions);
    const t = transforms[tIdx];
    if (!t) return;

    if (actionType === "center") {
      t.gui_dx = pos.x;
      t.gui_dy = pos.y;
      dispatch("transformChanged", { index: tIdx, transform: t });
    } else if (actionType === "reset_all") {
      t.scale = 1.0;
      t.rotation = 0.0;
      t.gui_dx = pos.x;
      t.gui_dy = pos.y;
      dispatch("transformChanged", { index: tIdx, transform: t });
    } else if (actionType === "rot_90") {
      t.rotation = (t.rotation + 90) % 360;
      dispatch("transformChanged", { index: tIdx, transform: t });
    } else if (actionType === "apply_all") {
      const rel_dx = t.gui_dx - pos.x,
        rel_dy = t.gui_dy - pos.y;
      positions.forEach((p, i) => {
        if (p.is_prime) return;
        const ot = transforms[getTransformIdx(i, positions)];
        if (ot) {
          ot.scale = t.scale;
          ot.rotation = t.rotation;
          ot.gui_dx = p.x + rel_dx;
          ot.gui_dy = p.y + rel_dy;
          dispatch("transformChanged", { index: getTransformIdx(i, positions), transform: ot });
        }
      });
    } else if (actionType === "mirror_h") {
      t.rotation = (180 - t.rotation + 360) % 360;
      dispatch("transformChanged", { index: tIdx, transform: t });
    } else if (actionType === "mirror_v") {
      t.rotation = (-t.rotation + 360) % 360;
      dispatch("transformChanged", { index: tIdx, transform: t });
    }
  }

  /** Id objektu vybraného na daném skle (klikem v plátně nebo v lokálním panelu). */
  function selectedObjectId(posIdx: number): number | null {
    const tIdx = getTransformIdx(posIdx, positions);
    return tIdx >= 0 && focusedObject?.slide === tIdx ? focusedObject.id : null;
  }

  /** Smaže vybraný objekt skla menu — z tohoto substrátu, nebo (everywhere) ze všech. */
  function deleteObject(posIdx: number, everywhere: boolean) {
    const id = selectedObjectId(posIdx);
    if (id != null) deleteObjectById(getTransformIdx(posIdx, positions), id, everywhere);
  }

  function deleteObjectById(tIdx: number, id: number, everywhere: boolean) {
    if (tIdx < 0) return;
    dispatch("saveState");
    if (everywhere) projectStore.deleteObjectEverywhere(id).catch((err) => console.error(err));
    else projectStore.deleteObjects(tIdx, [id]);
    dispatch("objectSelected", null);
  }

  /** Smaže trasu (všechny objekty) — z tohoto substrátu, nebo (everywhere) celou kresbu. */
  function deletePath(posIdx: number, everywhere: boolean) {
    const tIdx = getTransformIdx(posIdx, positions);
    if (tIdx < 0) return;
    dispatch("saveState");
    // Trasa jednoho skla: smazat a vrátit sklo do čistého stavu (bez rotace a měřítka)
    if (everywhere) projectStore.clearDesign();
    else projectStore.resetSlideTrasa(tIdx);
    dispatch("objectSelected", null);
  }

  function resetView() {
    if (canvasRef?.resetCamera) canvasRef.resetCamera(true);
  }

  // ─── 3D pohled ──────────────────────────────────────────────────────────
  /** Projekce a pohled shora — pro stav tlačítek (posílá Viewport). */
  let viewInfo = $state({ projection: "ortho" as "ortho" | "persp", top: true, axis: true });
  const COLOR_MODES: { mode: ViewColorMode; label: string }[] = [
    { mode: "substrate", label: "Barvy: podle substrátu" },
    { mode: "object", label: "Barvy: podle objektu" },
    { mode: "order", label: "Barvy: pořadí tisku" },
  ];
  let colorMode = $derived(($settingsStore.view_color_mode ?? "substrate") as ViewColorMode);
  function cycleColorMode() {
    const i = COLOR_MODES.findIndex((m) => m.mode === colorMode);
    settingsStore.persistPatch({ view_color_mode: COLOR_MODES[(i + 1) % COLOR_MODES.length].mode });
  }
  function toggleTravels() {
    settingsStore.persistPatch({ show_travels: !($settingsStore.show_travels ?? false) });
  }

  let selectedTransform = $derived(getSelectedTransform());
  let showContextActions =
    $derived(selectedIndex >= 0 &&
    selectedIndex < positions.length &&
    !positions[selectedIndex]?.is_prime &&
    selectedTransform !== null);

  // Format values for context menu display
  let ctxScale = $derived(selectedTransform ? selectedTransform.scale.toFixed(2) : "1.00");
  let ctxRotation = $derived(selectedTransform ? Math.round(selectedTransform.rotation) : 0);
  /** Objekt, kterého se týká kontextové menu (vybraný na skle menu). */
  let menuObjectId = $derived(contextMenu.visible ? selectedObjectId(contextMenu.slideIndex) : null);
</script>

<svelte:window
  onkeydown={handleKeyDown}
  onclick={closeContextMenu}
  onpointermove={updateTimelineHover}
  onmouseout={(e) => !e.relatedTarget && (timelineHover = false)}
/>

<div bind:this={rootEl} class="glass-panel rounded-lg flex flex-col h-full overflow-hidden relative">
  <!-- ── Toolbar — přenesený do horní lišty aplikace nad plátno (Header, #canvas-toolbar-slot) ── -->
  <div use:portal={"canvas-toolbar-slot"} class="relative select-none flex flex-row gap-1.5 items-center min-w-0">

    <div class="relative group">
      <button
        onclick={() => { showAxes = !showAxes; }}
        class="p-1.5 rounded-lg border shadow transition-colors {showAxes
          ? 'bg-labaccent border-blue-500/60 text-white'
          : 'bg-slate-900/70 border-slate-700 text-slate-400 hover:bg-slate-800'}"
      >
        <Grid2x2 class="w-4 h-4" />
      </button>
      <Tooltip text="Mřížka" />
    </div>

    <div class="relative group">
      <button
        onclick={() => { isMeasuring = !isMeasuring; if (!isMeasuring) measurePoints = []; else { drawTool = null; closeMergePopup(); } }}
        class="p-1.5 rounded-lg border shadow transition-colors {isMeasuring
          ? 'bg-yellow-500 border-yellow-400 text-black'
          : measurePoints.length > 0
            ? 'bg-yellow-900/50 border-yellow-500/50 text-yellow-500'
            : 'bg-slate-900/70 border-slate-700 text-slate-400 hover:bg-slate-800'}"
      >
        <Ruler class="w-4 h-4" />
      </button>
      <Tooltip text="Pravítko" />
    </div>

    <div class="relative group">
      <button
        onclick={resetView}
        class="p-1.5 rounded-lg border border-slate-700 shadow-sm bg-slate-900/70 text-slate-300 hover:bg-slate-800 transition-colors"
      >
        <Maximize2 class="w-4 h-4" />
      </button>
      <Tooltip text="Vycentrovat" />
    </div>

    <div class="relative group">
      <button
        onclick={() => canvasRef?.frameSelection()}
        disabled={selectedIndex < 0 || positions[selectedIndex]?.is_prime}
        class="p-1.5 rounded-lg border shadow transition-colors {selectedIndex >= 0 && !positions[selectedIndex]?.is_prime
          ? 'bg-slate-900/70 border-slate-700 text-slate-300 hover:bg-slate-800'
          : 'bg-slate-900/50 border-slate-800 text-slate-600 cursor-not-allowed'}"
      >
        <VectorSquare class="w-4 h-4" />
      </button>
      <Tooltip text="Na výběr (Numpad .)" />
    </div>

    <!-- ── 3D pohled ── -->
    <div class="relative group">
      <button
        onclick={() => canvasRef?.topView()}
        class="p-1.5 rounded-lg border shadow transition-colors {viewInfo.top
          ? 'bg-labaccent border-blue-500/60 text-white'
          : 'bg-slate-900/70 border-slate-700 text-slate-400 hover:bg-slate-800'}"
      >
        <ScanEye class="w-4 h-4" />
      </button>
      <Tooltip text="Pohled shora (Numpad 7)" />
    </div>

    <div class="relative group">
      <button
        onclick={() => canvasRef?.obliqueView()}
        class="p-1.5 rounded-lg border border-slate-700 shadow-sm bg-slate-900/70 text-slate-300 hover:bg-slate-800 transition-colors"
      >
        <Box class="w-4 h-4" />
      </button>
      <Tooltip text="3D pohled (prostřední tlačítko = otáčení)" />
    </div>

    <div class="relative group">
      <button
        onclick={() => canvasRef?.toggleProjection()}
        class="p-1.5 rounded-lg border shadow transition-colors {viewInfo.axis
          ? 'bg-slate-900/50 border-slate-800 text-slate-600 cursor-not-allowed'
          : viewInfo.projection === 'persp'
            ? 'bg-labaccent border-blue-500/60 text-white'
            : 'bg-slate-900/70 border-slate-700 text-slate-400 hover:bg-slate-800'}"
      >
        <Pyramid class="w-4 h-4" />
      </button>
      <Tooltip
        text={viewInfo.axis
          ? "Pohled po ose X / Y / Z je vždy ortho"
          : viewInfo.projection === "persp"
            ? "Perspektiva — přepnout na ortho (Numpad 5)"
            : "Ortho — přepnout na perspektivu (Numpad 5)"}
      />
    </div>

    <div class="relative group">
      <button
        onclick={toggleTravels}
        class="p-1.5 rounded-lg border shadow transition-colors {$settingsStore.show_travels
          ? 'bg-labaccent border-blue-500/60 text-white'
          : 'bg-slate-900/70 border-slate-700 text-slate-400 hover:bg-slate-800'}"
      >
        <Route class="w-4 h-4" />
      </button>
      <Tooltip text="Přejezdy trysky (Z-hop)" />
    </div>

    <div class="relative group">
      <button
        onclick={cycleColorMode}
        class="p-1.5 rounded-lg border shadow transition-colors {colorMode !== 'substrate'
          ? 'bg-labaccent border-blue-500/60 text-white'
          : 'bg-slate-900/70 border-slate-700 text-slate-400 hover:bg-slate-800'}"
      >
        <Palette class="w-4 h-4" />
      </button>
      <Tooltip text={COLOR_MODES.find((m) => m.mode === colorMode)?.label ?? ""} />
    </div>

    <div class="relative group">
      <button
        onclick={toggleMergePopup}
        disabled={mergeTIdx < 0}
        class="p-1.5 rounded-lg border shadow transition-colors {mergeTIdx < 0
          ? 'bg-slate-900/50 border-slate-800 text-slate-600 cursor-not-allowed'
          : showMergePopup
            ? 'bg-labaccent border-blue-500/60 text-white'
            : 'bg-slate-900/70 border-slate-700 text-slate-400 hover:bg-slate-800'}"
      >
        <Merge class="w-4 h-4" />
      </button>
      <Tooltip text="Sloučit uzly (merge by distance)" />
    </div>

    <!-- ── Kreslicí nástroje — vpravo, odlišené od navigačních vlevo ── -->
    <div class="ml-auto flex flex-row gap-1.5 items-center">
      <div class="relative group">
        <button
          onclick={() => (snapToPaths = !snapToPaths)}
          class="p-1.5 rounded-lg border shadow transition-colors {snapToPaths
            ? 'bg-fuchsia-600/80 border-fuchsia-400/60 text-white'
            : 'bg-slate-900/70 border-slate-700 text-slate-400 hover:bg-slate-800'}"
        >
          <Magnet class="w-4 h-4" />
        </button>
        <Tooltip text={snapToPaths ? "Přichytávání k trase zapnuto (M)" : "Přichytávat k trase (M)"} />
      </div>

      <div class="relative group">
        <button
          onclick={() => (drawHole = !drawHole)}
          class="p-1.5 rounded-lg border shadow transition-colors {drawHole
            ? 'bg-rose-600/80 border-rose-400/60 text-white'
            : 'bg-slate-900/70 border-slate-700 text-slate-400 hover:bg-slate-800'}"
        >
          <CircleDashed class="w-4 h-4" />
        </button>
        <Tooltip text={drawHole ? "Kreslí se díra (odečte se z tvarů)" : "Kreslit díru"} />
      </div>

      <div class="relative group">
        <button
          onclick={() => toggleDrawTool("rect")}
          class="p-1.5 rounded-lg border shadow transition-colors {drawTool === 'rect'
            ? 'bg-labaccent border-blue-500/60 text-white'
            : 'bg-slate-900/70 border-slate-700 text-slate-400 hover:bg-slate-800'}"
        >
          <Square class="w-4 h-4" />
        </button>
        <Tooltip text="Kreslit obdélník" />
      </div>

      <div class="relative group">
        <button
          onclick={() => toggleDrawTool("ellipse")}
          class="p-1.5 rounded-lg border shadow transition-colors {drawTool === 'ellipse'
            ? 'bg-labaccent border-blue-500/60 text-white'
            : 'bg-slate-900/70 border-slate-700 text-slate-400 hover:bg-slate-800'}"
        >
          <Circle class="w-4 h-4" />
        </button>
        <Tooltip text="Kreslit elipsu" />
      </div>

      <div class="relative group">
        <button
          onclick={() => toggleDrawTool("line")}
          class="p-1.5 rounded-lg border shadow transition-colors {drawTool === 'line'
            ? 'bg-labaccent border-blue-500/60 text-white'
            : 'bg-slate-900/70 border-slate-700 text-slate-400 hover:bg-slate-800'}"
        >
          <Slash class="w-4 h-4" />
        </button>
        <Tooltip text="Kreslit čáru" />
      </div>
    </div>

    <MergeNodesPopup
      open={showMergePopup}
      pathData={mergePathData}
      transform={mergeTransform}
      slideLabel={mergeTIdx >= 0 ? `Substrát ${mergeTIdx + 1}` : ""}
      twinCount={mergeTwinCount}
      bind:preview={mergePreview}
      on:close={closeMergePopup}
      on:apply={handleMergeApply}
    />
  </div>

  <!-- ── Volby kreslení (pod lištou, aby lišta nepřetékala) ── -->
  {#if !drawTool && focusedObject && getTransformIdx(selectedIndex, positions) === focusedObject.slide}
    <div
      class="absolute top-3 left-3 z-10 flex items-center gap-3 px-2.5 py-1.5 rounded-lg bg-slate-950/85 backdrop-blur-xs border border-amber-500/40 text-[11px] select-none"
      title="Tažení = posun · rohy = měřítko (Alt = volný poměr stran, Shift = od středu) · dvojklik = rotace · šipky = posun"
    >
      <label
        class="flex items-center gap-1.5 text-slate-300 whitespace-nowrap cursor-pointer"
        title="Úprava objektu platí jen pro tento substrát (jinak pro všechna skla)"
      >
        <input type="checkbox" bind:checked={editObjOnlyHere} class="accent-amber-500" />
        Upravit objekt jen na tomto substrátu
      </label>
      <span class="text-slate-500 whitespace-nowrap hidden 2xl:block">
        Alt = poměr stran · Shift = od středu · dvojklik = rotace
      </span>
    </div>
  {/if}

  {#if drawTool}
    <div
      class="absolute top-3 left-3 z-10 flex items-center gap-3 px-2.5 py-1.5 rounded-lg bg-slate-950/85 backdrop-blur-xs border border-slate-700/60 text-[11px] select-none"
      title="Ctrl = mřížka · Alt = od středu · Shift = 1:1 · M = magnet"
    >
      <label
        class="flex items-center gap-1.5 text-slate-300 whitespace-nowrap cursor-pointer"
        title="Tvar se vytvoří jen na substrátu, na který se kreslí (jinak na všech sklech)"
      >
        <input type="checkbox" bind:checked={drawOnlyHere} class="accent-blue-500" />
        Vytvořit jen na tento substrát
      </label>
      <span class="text-slate-500 whitespace-nowrap hidden 2xl:block">
        Ctrl = mřížka · Alt = od středu · Shift = 1:1 · M = magnet
      </span>
    </div>
  {/if}

  <!-- ── Canvas ── -->
  <div role="region" aria-label="Náhled tiskové plochy" class="flex-1 w-full h-full relative">
    <Viewport
      bind:this={canvasRef}
      {bedMaxX}
      {bedMaxY}
      {positions}
      {paths}
      {primePath}
      {transforms}
      {overrides}
      {nozzleDiam}
      {selectedIndex}
      {showAxes}
      {isMeasuring}
      {measurePoints}
      {currentNozzle}
      {printProgress}
      printCursor={$printerStore.is_printing ? ($printerStore.print_cursor ?? null) : null}
      bind:previewTotalTime
      {focusedObject}
      {extrusionRateUl}
      {drawTool}
      {snapToPaths}
      {mergePreview}
      objEditOnlyHere={editObjOnlyHere}
      on:slideSelected={handleSlideSelected}
      on:slideContext={handleSlideContext}
      on:measurePointsChange={handleMeasurePointsChange}
      on:transformChanged={handleTransformChanged}
      on:saveState={handleSaveState}
      on:pathRebuildNeeded={handlePathRebuildNeeded}
      on:shapeDrawn={handleShapeDrawn}
      on:drawToolExit={() => (drawTool = null)}
      on:objectSelected={(e) => dispatch("objectSelected", e.detail)}
      on:objectTransformChanged={handleObjectTransform}
      on:notice={(e) => showNotice(e.detail)}
      on:viewChanged={(e) => (viewInfo = e.detail)}
    />
    {#if notice}
      <div
        class="absolute top-3 left-1/2 -translate-x-1/2 z-20 px-3 py-1.5 rounded-lg bg-slate-950/90 border border-amber-500/40 text-amber-300 text-xs shadow-lg pointer-events-none"
      >
        {notice}
      </div>
    {/if}
  </div>

  <!-- ── Poslední nakreslený tvar: přesné rozměry (jako v Blenderu) ── -->
  {#if lastShapeActive && $lastDrawnShape}
    {@const sh = $lastDrawnShape.shape}
    <div
      class="absolute left-3 {timelineVisible ? 'bottom-10' : 'bottom-3'} transition-[bottom] duration-200 z-10 w-56 bg-slate-950/90 backdrop-blur-xs border border-slate-700/60 rounded-lg shadow-xl p-2.5 text-xs select-none"
    >
      <div class="flex items-center justify-between mb-2">
        <span class="font-bold text-slate-200">{shapeTitle(sh.kind)}</span>
        <button
          onclick={() => lastDrawnShape.set(null)}
          title="Zavřít"
          class="text-slate-500 hover:text-white transition-colors"
        >
          <X class="w-3.5 h-3.5" />
        </button>
      </div>
      <div class="grid grid-cols-5 items-center gap-1.5">
        <span class="col-span-2 text-slate-400">{sh.kind === "line" ? "Délka [mm]" : "Šířka [mm]"}</span>
        <div class="col-span-3 h-7">
          <NumberInput min={0.1} step={0.5} bind:value={editW} on:input={applyShapeEdit} class="w-full h-full" />
        </div>
        {#if sh.kind !== "line"}
          <span class="col-span-2 text-slate-400">Výška [mm]</span>
          <div class="col-span-3 h-7">
            <NumberInput min={0.1} step={0.5} bind:value={editH} on:input={applyShapeEdit} class="w-full h-full" />
          </div>
          <label class="col-span-5 flex items-center gap-2 mt-1 text-slate-300 cursor-pointer">
            <input type="checkbox" bind:checked={editHole} onchange={applyShapeEdit} class="accent-rose-500" />
            Díra (odečíst z ostatních tvarů)
          </label>
        {/if}
        <label class="col-span-5 flex items-center gap-2 text-slate-300 cursor-pointer">
          <input type="checkbox" bind:checked={editOnlyHere} onchange={applyShapeEdit} class="accent-blue-500" />
          Jen na tento substrát
        </label>
      </div>
    </div>
  {/if}

  <!-- ── Slider náhledu / live preview tisku ── -->
  <div
    class="absolute bottom-0 left-0 right-0 z-10 select-none transition-[opacity,translate] duration-200 ease-out {timelineVisible
      ? 'opacity-100 translate-y-0'
      : 'opacity-0 translate-y-full pointer-events-none'}"
    inert={!timelineVisible}
  >
    <div class="flex items-center gap-1.5 bg-slate-950/85 backdrop-blur-xs border-t border-slate-700/60 px-2.5 py-1">
      <!-- Stav tisku / label -->
      <span class="text-xs whitespace-nowrap {$printerStore.is_printing ? 'text-emerald-400' : 'text-slate-400'} mr-0.5">
        {#if $printerStore.is_printing}
          ● Tisk
        {:else}
          Náhled
        {/if}
      </span>

      <!-- Ovládací tlačítka (jen když netiskne) -->
      {#if !$printerStore.is_printing}
        <button
          onclick={stepBack}
          title="Krok zpět (←)"
          class="p-1 rounded-sm text-slate-400 hover:text-white hover:bg-slate-700 transition-colors"
        >
          <ChevronLeft class="w-3.5 h-3.5" />
        </button>

        <button
          onclick={() => isPlaying ? stopPlay() : startPlay()}
          title={isPlaying ? "Pauza" : "Přehrát od aktuální pozice"}
          class="p-1 rounded transition-colors {isPlaying
            ? 'text-cyan-400 hover:text-white hover:bg-slate-700'
            : 'text-slate-300 hover:text-white hover:bg-slate-700'}"
        >
          {#if isPlaying}
            <Pause class="w-3.5 h-3.5" />
          {:else}
            <Play class="w-3.5 h-3.5" />
          {/if}
        </button>

        <button
          onclick={stepForward}
          title="Krok dopředu (→)"
          class="p-1 rounded-sm text-slate-400 hover:text-white hover:bg-slate-700 transition-colors"
        >
          <ChevronRight class="w-3.5 h-3.5" />
        </button>
      {/if}

      <!-- Slider -->
      <input
        type="range"
        min="0"
        max="100"
        step="0.1"
        bind:value={printProgress}
        onmousedown={stopPlay}
        class="flex-1 h-1 cursor-pointer accent-blue-500"
      />
      <span class="text-xs font-mono text-slate-300 w-9 text-right">{Math.round(printProgress)}%</span>
      {#if previewTotalTime > 0}
        <span class="text-xs font-mono text-slate-400 whitespace-nowrap" title="Čas tisku v náhledu / celkem">
          {formatPreviewTime(printProgress)} / {formatPreviewTime(100)}
        </span>
      {/if}
      {#if !$printerStore.is_printing}
        <button
          onclick={cyclePlaySpeed}
          title="Rychlost přehrávání (1× = reálný čas tisku)"
          class="ml-0.5 px-1 rounded-sm text-xs font-mono transition-colors {playSpeed === 1
            ? 'text-emerald-400 hover:bg-slate-700'
            : 'text-amber-400 hover:bg-slate-700'}"
        >{playSpeed}×</button>
      {/if}
      {#if printProgress < 100}
        <button
          onclick={() => { stopPlay(); printProgress = 100; }}
          title="Zobrazit celý tisk"
          class="ml-0.5 text-slate-400 hover:text-white transition-colors text-xs leading-none"
        >✕</button>
      {/if}
    </div>
  </div>

  <!-- ── Context menu (right-click) — contains all transform actions ── -->
  {#if contextMenu.visible}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      bind:this={menuEl}
      class="absolute z-50 w-[330px] bg-slate-950/97 border border-slate-700/80 backdrop-blur-md rounded-xl shadow-2xl py-1.5 text-slate-200 text-sm overflow-y-auto select-none"
      style="left: {contextMenu.x}px; top: {contextMenu.y}px; max-width: calc(100% - 8px); max-height: calc(100% - 8px);"
      onclick={stopPropagation(bubble('click'))}
    >
      {#if showContextActions}
        <!-- Transform info header -->
        <div
          class="px-4 py-2 border-b border-slate-800 flex items-center justify-between gap-4 text-xs text-slate-400"
        >
          <span>Měřítko: <span class="text-slate-200 font-mono">{ctxScale}×</span></span>
          <span>Rotace: <span class="text-slate-200 font-mono">{ctxRotation}°</span></span>
        </div>
        <!-- Quick rotation / scale -->
        <button
          onclick={() => {
            dispatch("saveState");
            runAction("rot_90");
            closeContextMenu();
          }}
          class="w-full text-left px-4 py-2 hover:bg-slate-800 hover:text-white transition-colors"
        >
          Otočit trasu o 90°
        </button>
        <button
          onclick={() => {
            dispatch("saveState");
            runAction("mirror_h");
            closeContextMenu();
          }}
          class="w-full text-left px-4 py-2 hover:bg-slate-800 hover:text-white transition-colors"
        >
          Zrcadlit trasu horizontálně
        </button>
        <button
          onclick={() => {
            dispatch("saveState");
            runAction("mirror_v");
            closeContextMenu();
          }}
          class="w-full text-left px-4 py-2 hover:bg-slate-800 hover:text-white transition-colors border-b border-slate-800/60"
        >
          Zrcadlit trasu vertikálně
        </button>
        <button
          onclick={() => {
            dispatch("saveState");
            runAction("center");
            closeContextMenu();
          }}
          class="w-full text-left px-4 py-2 hover:bg-slate-800 hover:text-white transition-colors"
        >
          Vycentrovat trasu na sklo
        </button>
        <button
          onclick={() => {
            dispatch("saveState");
            runAction("apply_all");
            closeContextMenu();
          }}
          class="w-full text-left px-4 py-2 hover:bg-slate-800 hover:text-white transition-colors"
        >
          Aplikovat transformaci trasy na všechna skla
        </button>
        <button
          onclick={() => {
            dispatch("saveState");
            runAction("reset_all");
            closeContextMenu();
          }}
          class="w-full text-left px-4 py-2 hover:bg-slate-800 hover:text-white transition-colors border-b border-slate-800/60"
        >
          Resetovat transformaci trasy
        </button>
        <button
          onclick={() => {
            const c = selectionToClip();
            if (c) {
              clip = c;
              pasteCounts = new Map();
            }
            closeContextMenu();
          }}
          class="w-full text-left px-4 py-2 hover:bg-slate-800 hover:text-white transition-colors flex justify-between gap-4"
        >
          {menuObjectId != null ? "Kopírovat objekt" : "Kopírovat trasu"} <span class="text-xs text-slate-500">Ctrl+C</span>
        </button>
        <button
          disabled={!clip}
          onclick={() => {
            if (clip) {
              const target = contextMenu.slideIndex;
              const n = (pasteCounts.get(target) ?? 0) + (target === clip.fromPos ? 1 : 0);
              pasteCounts.set(target, (pasteCounts.get(target) ?? 0) + 1);
              pasteTo(clip, target, { x: PASTE_STEP.x * n, y: PASTE_STEP.y * n });
            }
            closeContextMenu();
          }}
          class="w-full text-left px-4 py-2 hover:bg-slate-800 hover:text-white transition-colors flex justify-between gap-4 disabled:text-slate-600 disabled:hover:bg-transparent"
        >
          {clip ? (clip.whole ? "Vložit trasu" : "Vložit objekt") : "Vložit"} <span class="text-xs text-slate-500">Ctrl+V</span>
        </button>
        <button
          onclick={() => {
            const c = selectionToClip();
            if (c) pasteTo(c, c.fromPos, PASTE_STEP);
            closeContextMenu();
          }}
          class="w-full text-left px-4 py-2 hover:bg-slate-800 hover:text-white transition-colors flex justify-between gap-4"
        >
          {menuObjectId != null ? "Duplikovat objekt" : "Duplikovat trasu"} <span class="text-xs text-slate-500">Ctrl+D</span>
        </button>
        <button
          onclick={() => {
            cloneSelection();
            closeContextMenu();
          }}
          title="Propojená kopie: měřítko, rotace a nastavení se přenáší mezi klony, poloha je vlastní"
          class="w-full text-left px-4 py-2 hover:bg-slate-800 hover:text-white transition-colors flex justify-between gap-4 border-b border-slate-800/60"
        >
          {menuObjectId != null ? "Klonovat objekt" : "Klonovat trasu"} <span class="text-xs text-slate-500">Ctrl+Alt+D</span>
        </button>
        {#if menuObjectId != null && focusedHasXform()}
          <button
            onclick={() => {
              dispatch("saveState");
              const id = menuObjectId!;
              projectStore
                .setObjectTransform(getTransformIdx(contextMenu.slideIndex, positions), id, null, editObjOnlyHere)
                .catch((err) => console.error(err));
              closeContextMenu();
            }}
            class="w-full text-left px-4 py-2 hover:bg-slate-800 hover:text-white transition-colors border-b border-slate-800/60"
            title={editObjOnlyHere ? "Jen na tomto substrátu (vrátí se sdílená úprava)" : "Na všech substrátech"}
          >
            Vrátit úpravu objektu (poloha, měřítko, rotace)
          </button>
        {/if}
        {#if menuObjectId != null}
          <button
            onclick={() => {
              deleteObject(contextMenu.slideIndex, false);
              closeContextMenu();
            }}
            class="w-full text-left px-4 py-2 hover:bg-red-600/20 hover:text-red-400 text-red-400 transition-colors flex justify-between gap-4"
          >
            Odstranit objekt <span class="text-xs text-slate-500">Del</span>
          </button>
          <button
            onclick={() => {
              deleteObject(contextMenu.slideIndex, true);
              closeContextMenu();
            }}
            class="w-full text-left px-4 py-2 hover:bg-red-600/20 hover:text-red-400 text-red-400 transition-colors flex justify-between gap-4 border-b border-slate-800/60"
          >
            Odstranit objekt ze všech substrátů <span class="text-xs text-slate-500">Shift+Del</span>
          </button>
        {/if}
        <button
          onclick={() => {
            deletePath(contextMenu.slideIndex, false);
            closeContextMenu();
          }}
          class="w-full text-left px-4 py-2 hover:bg-red-600/20 hover:text-red-400 text-red-400 transition-colors flex justify-between gap-4"
        >
          Odstranit trasu (celý substrát)
          {#if menuObjectId == null}<span class="text-xs text-slate-500">Del</span>{/if}
        </button>
        <button
          onclick={() => {
            deletePath(contextMenu.slideIndex, true);
            closeContextMenu();
          }}
          class="w-full text-left px-4 py-2 hover:bg-red-600/20 hover:text-red-400 text-red-400 transition-colors flex justify-between gap-4"
        >
          Odstranit trasu ze všech substrátů
          {#if menuObjectId == null}<span class="text-xs text-slate-500">Shift+Del</span>{/if}
        </button>
      {:else}
        <button
          onclick={() => {
            closeContextMenu();
          }}
          class="w-full text-left px-4 py-2 text-slate-500 cursor-default"
        >
          Vyberte trasu
        </button>
      {/if}
    </div>
  {/if}
</div>
