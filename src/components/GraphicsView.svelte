<script lang="ts">
  import { run, createBubbler, stopPropagation } from 'svelte/legacy';

  const bubble = createBubbler();
  import { createEventDispatcher, onDestroy } from "svelte";
  import type { LayoutPosition, SubstratePaths, Transform, Point2D, SlideOverride, Placement } from "../lib/tauri";
  import { getTransformIdx } from "../lib/geometry";
  import { Maximize2, Ruler, Grid2x2, VectorSquare, Play, Pause, ChevronLeft, ChevronRight, Square, Circle, Slash, Merge } from "lucide-svelte";
  import Tooltip from "./Tooltip.svelte";
  import Canvas2D from "./Canvas2D.svelte";
  import MergeNodesPopup from "./MergeNodesPopup.svelte";
  import { samePathGeometry, type MergePreview, type MergeResult } from "../lib/nodeMerge";
  import { printerStore } from "../stores/printerStore";
  import { projectStore, type DrawnShape } from "../stores/projectStore";

  const dispatch = createEventDispatcher();

  interface Props {
    bedMaxX?: number;
    bedMaxY?: number;
    positions?: LayoutPosition[];
    /** Umístění objektů po sklíčkách — `slides[i][k]` patří k objektu `k`. */
    slides?: Placement[][];
    /** Názvy objektů pro kontextovou nabídku a popup sloučení uzlů. */
    objectNames?: string[];
    primePath?: SubstratePaths | null;
    overrides?: Record<string, SlideOverride>;
    currentNozzle?: Point2D | null;
    nozzleDiam?: number;
    externalSelectedIndex?: any;
    externalSelectedObjIdx?: number;
    totalPreviewTime?: number; // celkový čas tisku v sekundách (0 = neznámý)
    extrusionRateUl?: number; // µl/mm pro zobrazení objemu v overlay
    suspendAutoCenter?: boolean; // dokud true (např. welcome screen), centrování se odloží
  }

  let {
    bedMaxX = 250.0,
    bedMaxY = 210.0,
    positions = [],
    slides = [],
    objectNames = [],
    primePath = null,
    overrides = {},
    currentNozzle = null,
    nozzleDiam = 0.4,
    externalSelectedIndex = -1,
    externalSelectedObjIdx = -1,
    totalPreviewTime = 0,
    extrusionRateUl = 0,
    suspendAutoCenter = false
  }: Props = $props();

  let selectedIndex = $state(-1);
  /** Vybraný objekt na vybraném sklíčku; -1 = vybráno jen sklíčko. */
  let selectedObjIdx = $state(-1);

  run(() => {
    if (externalSelectedIndex >= 0 && externalSelectedIndex !== selectedIndex) {
      selectedIndex = externalSelectedIndex;
    }
  });
  run(() => {
    if (externalSelectedObjIdx !== selectedObjIdx && externalSelectedObjIdx >= 0) {
      selectedObjIdx = externalSelectedObjIdx;
    }
  });

  /** Umístění na sklíčku dané POZICÍ v layoutu (přeskakuje odpliv). */
  function placementAt(posIdx: number, objIdx: number): Placement | null {
    if (posIdx < 0 || posIdx >= positions.length || positions[posIdx].is_prime) return null;
    const tIdx = getTransformIdx(posIdx, positions);
    return slides[tIdx]?.[objIdx] ?? null;
  }

  let showAxes = $state(true);
  let isMeasuring = $state(false);
  // Aktivní kreslicí nástroj — kreslení a měření se vzájemně vylučují
  let drawTool: "rect" | "ellipse" | "line" | null = $state(null);
  let measurePoints: { x: number; y: number }[] = $state([]);
  let contextMenu = $state({ visible: false, x: 0, y: 0, slideIndex: -1 });
  let showMergePopup = $state(false);
  let mergePreview: MergePreview | null = $state(null);
  let canvasRef: any = $state();
  let printProgress = $state(100);

  // ─── Play / Step logika ──────────────────────────────────────────────────
  let isPlaying = $state(false);
  let rafId: number | null = null;
  let playStartMs = 0;
  let playStartProgress = 0;
  // Rychlost přehrávání: 3x (60s tisk = 20s animace). Pokud není znám čas, používáme 60s default.
  const PLAY_SPEED = 3;

  function getAnimDuration(): number {
    // ms na celou animaci od 0 do 100 %
    const totalSec = totalPreviewTime > 0 ? totalPreviewTime : 60;
    return (totalSec / PLAY_SPEED) * 1000;
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
    if ($printerStore.is_printing) printProgress = Math.round($printerStore.progress);
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
        else canvasRef.resetCamera();
      }
    }
  });
  run(() => {
    if (!suspendAutoCenter && pendingAutoCenter && canvasRef?.resetCamera) {
      pendingAutoCenter = false;
      canvasRef.resetCamera();
    }
  });

  function getSelectedTransform(): Transform | null {
    return placementAt(selectedIndex, selectedObjIdx)?.transform ?? null;
  }

  function handleSlideSelected(e: CustomEvent<number>) {
    selectedIndex = e.detail;
    dispatch("slideSelected", selectedIndex);
    isMeasuring = false;
  }

  function handleObjectSelected(e: CustomEvent<number>) {
    selectedObjIdx = e.detail;
    dispatch("objectSelected", selectedObjIdx);
  }

  function handleSlideContext(e: CustomEvent<{ index: number; x: number; y: number }>) {
    selectedIndex = e.detail.index;
    dispatch("slideSelected", selectedIndex);
    contextMenu = { visible: true, x: e.detail.x, y: e.detail.y, slideIndex: e.detail.index };
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

  function handleShapeDrawn(e: CustomEvent<DrawnShape>) {
    projectStore.addDrawnShape(e.detail).catch((err) => {
      console.error("Nepodařilo se přidat nakreslený tvar:", err);
    });
  }

  function handleTransformChanged(
    e: CustomEvent<{ slideIdx: number; objIdx: number; transform: Transform }>
  ) {
    dispatch("transformChanged", e.detail);
  }

  function handleSaveState() {
    dispatch("saveState");
  }

  function handlePathRebuildNeeded(
    e: CustomEvent<{ slideIdx: number; objIdx: number; scale: number; rotation: number }>
  ) {
    dispatch("pathRebuildNeeded", e.detail);
  }

  function closeContextMenu() {
    contextMenu.visible = false;
  }

  function handleKeyDown(e: KeyboardEvent) {
    // Escape zavře merge popup i z jeho vstupních polí
    if (e.key === "Escape" && showMergePopup) {
      e.preventDefault();
      closeMergePopup();
      return;
    }
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;

    // Šipky pro krokování náhledu (jen když netiskne a není vybráno sklíčko)
    if (!$printerStore.is_printing && (selectedIndex < 0 || selectedIndex >= positions.length)) {
      if (e.key === "ArrowLeft") { e.preventDefault(); stepBack(); return; }
      if (e.key === "ArrowRight") { e.preventDefault(); stepForward(); return; }
    }

    if (selectedIndex < 0 || selectedIndex >= positions.length) return;
    const pos = positions[selectedIndex];
    if (pos.is_prime) return;
    const tIdx = getTransformIdx(selectedIndex, positions);
    const t = placementAt(selectedIndex, selectedObjIdx)?.transform;
    if (!t) return;

    if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      dispatch("saveState");
      runAction("hide");
      return;
    }

    const step = e.shiftKey ? 0.1 : 1.0;
    let moved = false;
    if (e.key === "ArrowUp") {
      t.gui_dy += step;
      moved = true;
    } else if (e.key === "ArrowDown") {
      t.gui_dy -= step;
      moved = true;
    } else if (e.key === "ArrowLeft") {
      t.gui_dx -= step;
      moved = true;
    } else if (e.key === "ArrowRight") {
      t.gui_dx += step;
      moved = true;
    }
    if (moved) {
      e.preventDefault();
      dispatch("transformChanged", { slideIdx: tIdx, objIdx: selectedObjIdx, transform: t });
    }
  }

  function runAction(actionType: string) {
    const idx = contextMenu.slideIndex !== -1 ? contextMenu.slideIndex : selectedIndex;
    if (idx < 0 || idx >= positions.length) return;
    const pos = positions[idx];
    const tIdx = getTransformIdx(idx, positions);
    const pl = placementAt(idx, selectedObjIdx);
    const t = pl?.transform;
    if (!t || !pl) return;
    const emit = (transform: Transform, slideIdx = tIdx, objIdx = selectedObjIdx) =>
      dispatch("transformChanged", { slideIdx, objIdx, transform });

    if (actionType === "center") {
      t.gui_dx = pos.x;
      t.gui_dy = pos.y;
      emit(t);
    } else if (actionType === "reset_all") {
      t.scale = 1.0;
      t.rotation = 0.0;
      t.gui_dx = pos.x;
      t.gui_dy = pos.y;
      emit(t);
    } else if (actionType === "rot_90") {
      t.rotation = (t.rotation + 90) % 360;
      emit(t);
    } else if (actionType === "apply_all") {
      // Tentýž objekt na všech sklech — párujeme přes object_id
      const rel_dx = t.gui_dx - pos.x,
        rel_dy = t.gui_dy - pos.y;
      positions.forEach((p, i) => {
        if (p.is_prime) return;
        const oi = getTransformIdx(i, positions);
        const ok = (slides[oi] ?? []).findIndex((q) => q.object_id === pl.object_id);
        const ot = ok >= 0 ? slides[oi]?.[ok]?.transform : null;
        if (ot) {
          ot.scale = t.scale;
          ot.rotation = t.rotation;
          ot.gui_dx = p.x + rel_dx;
          ot.gui_dy = p.y + rel_dy;
          emit(ot, oi, ok);
        }
      });
    } else if (actionType === "mirror_h") {
      t.rotation = (180 - t.rotation + 360) % 360;
      emit(t);
    } else if (actionType === "mirror_v") {
      t.rotation = (-t.rotation + 360) % 360;
      emit(t);
    } else if (actionType === "hide") {
      // Vratné odebrání objektu z tohoto skla — zpět přes výčet v pravém panelu
      dispatch("objectHidden", { slideIdx: tIdx, objIdx: selectedObjIdx });
    }
  }

  // ─── Sloučení uzlů (merge by distance) ───────────────────────────────────
  // Merge pracuje nad trasou jednoho objektu: přednost má vybraný, jinak první
  // objekt s neprázdnou trasou — tlačítko tak funguje i bez předchozího výběru.
  function hasPath(posIdx: number, objIdx: number): boolean {
    const pl = placementAt(posIdx, objIdx);
    return !!pl && pl.enabled && pl.path.segments.length > 0;
  }

  /** První objekt s trasou na daném skle. */
  function firstObjWithPath(posIdx: number): number {
    if (posIdx < 0 || posIdx >= positions.length || positions[posIdx].is_prime) return -1;
    const row = slides[getTransformIdx(posIdx, positions)] ?? [];
    for (let k = 0; k < row.length; k++) if (hasPath(posIdx, k)) return k;
    return -1;
  }

  let mergeTarget: { posIdx: number; objIdx: number } = $derived.by(() => {
    // 1. vybraný objekt, 2. první objekt vybraného skla, 3. první objekt vůbec
    if (hasPath(selectedIndex, selectedObjIdx))
      return { posIdx: selectedIndex, objIdx: selectedObjIdx };
    const onSelected = firstObjWithPath(selectedIndex);
    if (onSelected >= 0) return { posIdx: selectedIndex, objIdx: onSelected };
    for (let i = 0; i < positions.length; i++) {
      const k = firstObjWithPath(i);
      if (k >= 0) return { posIdx: i, objIdx: k };
    }
    return { posIdx: -1, objIdx: -1 };
  });
  let mergeTIdx = $derived(
    mergeTarget.posIdx >= 0 ? getTransformIdx(mergeTarget.posIdx, positions) : -1
  );
  let mergePlacement = $derived(placementAt(mergeTarget.posIdx, mergeTarget.objIdx));
  let mergePathData = $derived(mergePlacement?.path ?? null);
  let mergeTransform = $derived(mergePlacement?.transform ?? null);
  /** Kolik dalších sklíček má tentýž objekt se shodnou trasou — merge se na ně
   *  dá promítnout naráz. */
  let mergeTwinCount = $derived.by(() => {
    if (!mergePathData || !mergePlacement) return 0;
    let n = 0;
    for (let i = 0; i < slides.length; i++) {
      if (i === mergeTIdx) continue;
      const twin = (slides[i] ?? []).find((p) => p.object_id === mergePlacement!.object_id);
      if (twin && samePathGeometry(twin.path, mergePathData)) n++;
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
    if (mergeTIdx < 0 || mergeTarget.objIdx < 0) return;
    dispatch("saveState");
    projectStore.applyMergedPath(
      mergeTIdx,
      mergeTarget.objIdx,
      e.detail.paths,
      e.detail.applyToAll
    );
  }

  function resetView() {
    if (canvasRef?.resetCamera) canvasRef.resetCamera();
  }

  let selectedTransform = $derived(getSelectedTransform());
  let showContextActions =
    $derived(selectedIndex >= 0 &&
    selectedObjIdx >= 0 &&
    selectedIndex < positions.length &&
    !positions[selectedIndex]?.is_prime &&
    selectedTransform !== null);
  /** Název vybraného objektu pro kontextovou nabídku. */
  let selectedObjName = $derived.by(() => {
    const pl = placementAt(selectedIndex, selectedObjIdx);
    if (!pl) return "";
    return objectNames[selectedObjIdx] || `Objekt ${selectedObjIdx + 1}`;
  });

  // Format values for context menu display
  let ctxScale = $derived(selectedTransform ? selectedTransform.scale.toFixed(2) : "1.00");
  let ctxRotation = $derived(selectedTransform ? Math.round(selectedTransform.rotation) : 0);
</script>

<svelte:window onkeydown={handleKeyDown} onclick={closeContextMenu} />

<div class="glass-panel rounded-lg flex flex-col h-full overflow-hidden relative">
  <!-- ── Toolbar ── -->
  <div class="absolute top-0 left-0 right-0 z-10 select-none pl-3 pr-2 py-2 bg-slate-950/80 backdrop-blur-xs border-b border-slate-700/40 flex flex-row gap-3 items-center">

    <div class="relative group">
      <button
        onclick={() => { showAxes = !showAxes; }}
        class="p-2 rounded-lg border shadow transition-colors {showAxes
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
        class="p-2 rounded-lg border shadow transition-colors {isMeasuring
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
        class="p-2 rounded-lg border border-slate-700 shadow-sm bg-slate-900/70 text-slate-300 hover:bg-slate-800 transition-colors"
      >
        <Maximize2 class="w-4 h-4" />
      </button>
      <Tooltip text="Vycentrovat" />
    </div>

    <div class="relative group">
      <button
        onclick={() => canvasRef?.centerOnSlide()}
        disabled={selectedIndex < 0 || positions[selectedIndex]?.is_prime}
        class="p-2 rounded-lg border shadow transition-colors {selectedIndex >= 0 && !positions[selectedIndex]?.is_prime
          ? 'bg-slate-900/70 border-slate-700 text-slate-300 hover:bg-slate-800'
          : 'bg-slate-900/50 border-slate-800 text-slate-600 cursor-not-allowed'}"
      >
        <VectorSquare class="w-4 h-4" />
      </button>
      <Tooltip text="Na sklíčko" />
    </div>

    <!-- ── Kreslicí nástroje — vpravo, odlišené od navigačních vlevo ── -->
    <div class="ml-auto flex flex-row gap-3 items-center">
      {#if drawTool}
        <span class="text-[11px] text-slate-400 whitespace-nowrap hidden sm:block">
          Ctrl = mřížka · Alt = od středu · Shift = 1:1
        </span>
      {/if}

      <div class="relative group">
        <button
          onclick={toggleMergePopup}
          disabled={mergeTarget.posIdx < 0}
          class="p-2 rounded-lg border shadow transition-colors {mergeTarget.posIdx < 0
            ? 'bg-slate-900/50 border-slate-800 text-slate-600 cursor-not-allowed'
            : showMergePopup
              ? 'bg-labaccent border-blue-500/60 text-white'
              : 'bg-slate-900/70 border-slate-700 text-slate-400 hover:bg-slate-800'}"
        >
          <Merge class="w-4 h-4" />
        </button>
        <Tooltip text="Sloučit uzly (merge by distance)" />
      </div>

      <div class="w-px h-6 bg-slate-700/60"></div>

      <div class="relative group">
        <button
          onclick={() => toggleDrawTool("rect")}
          class="p-2 rounded-lg border shadow transition-colors {drawTool === 'rect'
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
          class="p-2 rounded-lg border shadow transition-colors {drawTool === 'ellipse'
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
          class="p-2 rounded-lg border shadow transition-colors {drawTool === 'line'
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
      slideLabel={mergeTIdx >= 0
        ? `Substrát ${mergeTIdx + 1} · ${objectNames[mergeTarget.objIdx] || `Objekt ${mergeTarget.objIdx + 1}`}`
        : ""}
      twinCount={mergeTwinCount}
      bind:preview={mergePreview}
      on:close={closeMergePopup}
      on:apply={handleMergeApply}
    />

  </div>

  <!-- ── Canvas ── -->
  <div role="region" aria-label="Náhled tiskové plochy" class="flex-1 w-full h-full relative">
    <Canvas2D
      bind:this={canvasRef}
      {bedMaxX}
      {bedMaxY}
      {positions}
      {slides}
      {primePath}
      {overrides}
      {nozzleDiam}
      {selectedIndex}
      {selectedObjIdx}
      {showAxes}
      {isMeasuring}
      {measurePoints}
      {currentNozzle}
      {printProgress}
      {extrusionRateUl}
      {drawTool}
      {mergePreview}
      on:slideSelected={handleSlideSelected}
      on:objectSelected={handleObjectSelected}
      on:slideContext={handleSlideContext}
      on:measurePointsChange={handleMeasurePointsChange}
      on:transformChanged={handleTransformChanged}
      on:saveState={handleSaveState}
      on:pathRebuildNeeded={handlePathRebuildNeeded}
      on:shapeDrawn={handleShapeDrawn}
      on:drawToolExit={() => (drawTool = null)}
    />
  </div>

  <!-- ── Slider náhledu / live preview tisku ── -->
  <div class="absolute bottom-0 left-0 right-0 z-10 px-3 pb-2 pointer-events-none select-none">
    <div class="flex items-center gap-1.5 bg-slate-950/80 backdrop-blur-xs rounded-lg px-2.5 py-1.5 pointer-events-auto">
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
      class="fixed z-50 bg-slate-950/97 border border-slate-700/80 backdrop-blur-md rounded-xl shadow-2xl py-1.5 min-w-[260px] text-slate-200 text-sm overflow-hidden select-none"
      style="left: {contextMenu.x}px; top: {contextMenu.y}px;"
      onclick={stopPropagation(bubble('click'))}
    >
      {#if showContextActions}
        <!-- Transform info header -->
        <div class="px-4 pt-2 pb-1 text-xs text-slate-300 truncate" title={selectedObjName}>
          {selectedObjName}
        </div>
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
            dispatch("saveState");
            runAction("hide");
            closeContextMenu();
          }}
          class="w-full text-left px-4 py-2 hover:bg-red-600/20 hover:text-red-400 text-red-400 transition-colors"
        >
          Odebrat objekt z tohoto skla
        </button>
      {:else}
        <button
          onclick={() => {
            closeContextMenu();
          }}
          class="w-full text-left px-4 py-2 text-slate-500 cursor-default"
        >
          Vyberte objekt
        </button>
      {/if}
    </div>
  {/if}
</div>
