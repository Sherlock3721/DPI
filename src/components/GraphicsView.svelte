<script lang="ts">
  import { createEventDispatcher, onDestroy } from "svelte";
  import type { LayoutPosition, SubstratePaths, Transform, Point2D, SlideOverride } from "../lib/tauri";
  import { getTransformIdx } from "../lib/geometry";
  import { Maximize2, Ruler, Grid2x2, VectorSquare, Play, Pause, ChevronLeft, ChevronRight } from "lucide-svelte";
  import Canvas2D from "./Canvas2D.svelte";
  import { printerStore } from "../stores/printerStore";

  const dispatch = createEventDispatcher();

  export let bedMaxX = 250.0;
  export let bedMaxY = 210.0;
  export let positions: LayoutPosition[] = [];
  export let paths: SubstratePaths[] = [];
  export let primePath: SubstratePaths | null = null;
  export let transforms: Transform[] = [];
  export let overrides: Record<string, SlideOverride> = {};
  export let currentNozzle: Point2D | null = null;
  export let nozzleDiam = 0.4;
  export let externalSelectedIndex = -1;
  export let totalPreviewTime = 0; // celkový čas tisku v sekundách (0 = neznámý)

  let selectedIndex = -1;

  $: if (externalSelectedIndex >= 0 && externalSelectedIndex !== selectedIndex) {
    selectedIndex = externalSelectedIndex;
  }

  let showAxes = true;
  let isMeasuring = false;
  let measurePoints: { x: number; y: number }[] = [];
  let contextMenu = { visible: false, x: 0, y: 0, slideIndex: -1 };
  let canvasRef: any;
  let printProgress = 100;

  // ─── Play / Step logika ──────────────────────────────────────────────────
  let isPlaying = false;
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
  $: if ($printerStore.is_printing) { stopPlay(); }

  onDestroy(() => stopPlay());

  // Při tisku automaticky synchronizuj slider s reálným průběhem
  $: if ($printerStore.is_printing) printProgress = Math.round($printerStore.progress);

  // Při změně počtu sklíček/pozic resetuj pohled kamery, aby byla všechna sklíčka viditelná.
  let prevPositionCount = -1;
  $: if (positions.length !== prevPositionCount && canvasRef?.resetCamera) {
    prevPositionCount = positions.length;
    if (positions.length > 0) canvasRef.resetCamera();
  }

  function getSelectedTransform(): Transform | null {
    const tidx = getTransformIdx(selectedIndex, positions);
    return transforms[tidx] ?? null;
  }

  function handleSlideSelected(e: CustomEvent<number>) {
    selectedIndex = e.detail;
    dispatch("slideSelected", selectedIndex);
    isMeasuring = false;
  }

  function handleSlideContext(e: CustomEvent<{ index: number; x: number; y: number }>) {
    selectedIndex = e.detail.index;
    dispatch("slideSelected", selectedIndex);
    contextMenu = { visible: true, x: e.detail.x, y: e.detail.y, slideIndex: e.detail.index };
  }

  function handleMeasurePointsChange(e: CustomEvent<{ x: number; y: number }[]>) {
    measurePoints = e.detail;
  }

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

  function handleKeyDown(e: KeyboardEvent) {
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
    const t = transforms[tIdx];
    if (!t) return;

    if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      dispatch("saveState");
      runAction("delete");
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
    } else if (actionType === "delete") {
      if (paths[tIdx]) {
        paths[tIdx] = { segments: [] };
        dispatch("pathCleared", { index: tIdx });
      }
    }
  }

  function resetView() {
    if (canvasRef?.resetCamera) canvasRef.resetCamera();
  }

  $: selectedTransform = getSelectedTransform();
  $: showContextActions =
    selectedIndex >= 0 &&
    selectedIndex < positions.length &&
    !positions[selectedIndex]?.is_prime &&
    selectedTransform !== null;

  // Format values for context menu display
  $: ctxScale = selectedTransform ? selectedTransform.scale.toFixed(2) : "1.00";
  $: ctxRotation = selectedTransform ? Math.round(selectedTransform.rotation) : 0;
</script>

<svelte:window on:keydown={handleKeyDown} on:click={closeContextMenu} />

<div class="glass-panel rounded-lg flex flex-col h-full overflow-hidden relative">
  <!-- ── Toolbar ── -->
  <div class="absolute top-0 left-0 right-0 z-10 select-none pl-3 pr-2 py-2 bg-slate-950/80 backdrop-blur-sm border-b border-slate-700/40 flex flex-row gap-3 items-center">

    <div class="relative group">
      <button
        on:click={() => { showAxes = !showAxes; }}
        class="p-2 rounded-lg border shadow transition-colors {showAxes
          ? 'bg-labaccent border-blue-500/60 text-white'
          : 'bg-slate-900/70 border-slate-700 text-slate-400 hover:bg-slate-800'}"
      >
        <Grid2x2 class="w-4 h-4" />
      </button>
      <div class="absolute top-full left-1/2 -translate-x-1/2 mt-1.5 px-2 py-0.5 rounded bg-slate-800 border border-slate-700 text-slate-200 text-xs whitespace-nowrap opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none z-20">
        Mřížka
      </div>
    </div>

    <div class="relative group">
      <button
        on:click={() => { isMeasuring = !isMeasuring; if (!isMeasuring) measurePoints = []; }}
        class="p-2 rounded-lg border shadow transition-colors {isMeasuring
          ? 'bg-yellow-500 border-yellow-400 text-black'
          : measurePoints.length > 0
            ? 'bg-yellow-900/50 border-yellow-500/50 text-yellow-500'
            : 'bg-slate-900/70 border-slate-700 text-slate-400 hover:bg-slate-800'}"
      >
        <Ruler class="w-4 h-4" />
      </button>
      <div class="absolute top-full left-1/2 -translate-x-1/2 mt-1.5 px-2 py-0.5 rounded bg-slate-800 border border-slate-700 text-slate-200 text-xs whitespace-nowrap opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none z-20">
        Pravítko
      </div>
    </div>

    <div class="relative group">
      <button
        on:click={resetView}
        class="p-2 rounded-lg border border-slate-700 shadow bg-slate-900/70 text-slate-300 hover:bg-slate-800 transition-colors"
      >
        <Maximize2 class="w-4 h-4" />
      </button>
      <div class="absolute top-full left-1/2 -translate-x-1/2 mt-1.5 px-2 py-0.5 rounded bg-slate-800 border border-slate-700 text-slate-200 text-xs whitespace-nowrap opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none z-20">
        Vycentrovat
      </div>
    </div>

    <div class="relative group">
      <button
        on:click={() => canvasRef?.centerOnSlide()}
        disabled={selectedIndex < 0 || positions[selectedIndex]?.is_prime}
        class="p-2 rounded-lg border shadow transition-colors {selectedIndex >= 0 && !positions[selectedIndex]?.is_prime
          ? 'bg-slate-900/70 border-slate-700 text-slate-300 hover:bg-slate-800'
          : 'bg-slate-900/50 border-slate-800 text-slate-600 cursor-not-allowed'}"
      >
        <VectorSquare class="w-4 h-4" />
      </button>
      <div class="absolute top-full left-1/2 -translate-x-1/2 mt-1.5 px-2 py-0.5 rounded bg-slate-800 border border-slate-700 text-slate-200 text-xs whitespace-nowrap opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none z-20">
        Na sklíčko
      </div>
    </div>

  </div>

  <!-- ── Canvas ── -->
  <div role="region" aria-label="Náhled tiskové plochy" class="flex-1 w-full h-full relative">
    <Canvas2D
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
      on:slideSelected={handleSlideSelected}
      on:slideContext={handleSlideContext}
      on:measurePointsChange={handleMeasurePointsChange}
      on:transformChanged={handleTransformChanged}
      on:saveState={handleSaveState}
      on:pathRebuildNeeded={handlePathRebuildNeeded}
    />
  </div>

  <!-- ── Slider náhledu / live preview tisku ── -->
  <div class="absolute bottom-0 left-0 right-0 z-10 px-3 pb-2 pointer-events-none select-none">
    <div class="flex items-center gap-1.5 bg-slate-950/80 backdrop-blur-sm rounded-lg px-2.5 py-1.5 pointer-events-auto">
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
          on:click={stepBack}
          title="Krok zpět (←)"
          class="p-1 rounded text-slate-400 hover:text-white hover:bg-slate-700 transition-colors"
        >
          <ChevronLeft class="w-3.5 h-3.5" />
        </button>

        <button
          on:click={() => isPlaying ? stopPlay() : startPlay()}
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
          on:click={stepForward}
          title="Krok dopředu (→)"
          class="p-1 rounded text-slate-400 hover:text-white hover:bg-slate-700 transition-colors"
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
        on:mousedown={stopPlay}
        class="flex-1 h-1 cursor-pointer accent-blue-500"
      />
      <span class="text-xs font-mono text-slate-300 w-9 text-right">{Math.round(printProgress)}%</span>
      {#if printProgress < 100}
        <button
          on:click={() => { stopPlay(); printProgress = 100; }}
          title="Zobrazit celý tisk"
          class="ml-0.5 text-slate-400 hover:text-white transition-colors text-xs leading-none"
        >✕</button>
      {/if}
    </div>
  </div>

  <!-- ── Context menu (right-click) — contains all transform actions ── -->
  {#if contextMenu.visible}
    <!-- svelte-ignore a11y-click-events-have-key-events -->
    <!-- svelte-ignore a11y-no-static-element-interactions -->
    <div
      class="fixed z-50 bg-slate-950/97 border border-slate-700/80 backdrop-blur-md rounded-xl shadow-2xl py-1.5 min-w-[260px] text-slate-200 text-sm overflow-hidden select-none"
      style="left: {contextMenu.x}px; top: {contextMenu.y}px;"
      on:click|stopPropagation
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
          on:click={() => {
            dispatch("saveState");
            runAction("rot_90");
            closeContextMenu();
          }}
          class="w-full text-left px-4 py-2 hover:bg-slate-800 hover:text-white transition-colors"
        >
          Otočit trasu o 90°
        </button>
        <button
          on:click={() => {
            dispatch("saveState");
            runAction("mirror_h");
            closeContextMenu();
          }}
          class="w-full text-left px-4 py-2 hover:bg-slate-800 hover:text-white transition-colors"
        >
          Zrcadlit trasu horizontálně
        </button>
        <button
          on:click={() => {
            dispatch("saveState");
            runAction("mirror_v");
            closeContextMenu();
          }}
          class="w-full text-left px-4 py-2 hover:bg-slate-800 hover:text-white transition-colors border-b border-slate-800/60"
        >
          Zrcadlit trasu vertikálně
        </button>
        <button
          on:click={() => {
            dispatch("saveState");
            runAction("center");
            closeContextMenu();
          }}
          class="w-full text-left px-4 py-2 hover:bg-slate-800 hover:text-white transition-colors"
        >
          Vycentrovat trasu na sklo
        </button>
        <button
          on:click={() => {
            dispatch("saveState");
            runAction("apply_all");
            closeContextMenu();
          }}
          class="w-full text-left px-4 py-2 hover:bg-slate-800 hover:text-white transition-colors"
        >
          Aplikovat transformaci trasy na všechna skla
        </button>
        <button
          on:click={() => {
            dispatch("saveState");
            runAction("reset_all");
            closeContextMenu();
          }}
          class="w-full text-left px-4 py-2 hover:bg-slate-800 hover:text-white transition-colors border-b border-slate-800/60"
        >
          Resetovat transformaci trasy
        </button>
        <button
          on:click={() => {
            dispatch("saveState");
            runAction("delete");
            closeContextMenu();
          }}
          class="w-full text-left px-4 py-2 hover:bg-red-600/20 hover:text-red-400 text-red-400 transition-colors"
        >
          Smazat trasu
        </button>
      {:else}
        <button
          on:click={() => {
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
