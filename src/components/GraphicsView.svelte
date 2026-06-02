<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import type { LayoutPosition, SubstratePaths, Transform, Point2D, SlideOverride } from "../lib/tauri";
  import { getTransformIdx } from "../lib/geometry";
  import { Maximize2, Ruler, Grid2x2 } from "lucide-svelte";
  import Canvas2D from "./Canvas2D.svelte";

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

  let selectedIndex = -1;

  $: if (externalSelectedIndex >= 0 && externalSelectedIndex !== selectedIndex) {
    selectedIndex = externalSelectedIndex;
  }

  let showAxes = true;
  let isMeasuring = false;
  let measurePoints: { x: number; y: number }[] = [];
  let contextMenu = { visible: false, x: 0, y: 0, slideIndex: -1 };
  let canvasRef: any;

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
    if (selectedIndex < 0 || selectedIndex >= positions.length) return;
    const pos = positions[selectedIndex];
    if (pos.is_prime) return;
    const tIdx = getTransformIdx(selectedIndex, positions);
    const t = transforms[tIdx];
    if (!t) return;
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;

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
  <div class="absolute top-3 left-3 flex flex-col gap-1.5 z-10 select-none">
    <button
      on:click={() => {
        showAxes = !showAxes;
      }}
      title={showAxes ? "Skrýt mřížku" : "Zobrazit mřížku"}
      class="p-2 rounded-lg border shadow-lg transition-colors {showAxes
        ? 'bg-labaccent border-blue-500/60 text-white'
        : 'bg-slate-900/90 border-slate-700 text-slate-400 hover:bg-slate-800'}"
    >
      <Grid2x2 class="w-4 h-4" />
    </button>

    <button
      on:click={() => {
        isMeasuring = !isMeasuring;
        if (!isMeasuring) {
          measurePoints = [];
        }
      }}
      title="Měřit vzdálenost (pravý klik = smaž bod, Escape = vymaž)"
      class="p-2 rounded-lg border shadow-lg transition-colors {isMeasuring
        ? 'bg-yellow-500 border-yellow-400 text-black'
        : measurePoints.length > 0
          ? 'bg-yellow-900/50 border-yellow-500/50 text-yellow-500'
          : 'bg-slate-900/90 border-slate-700 text-slate-400 hover:bg-slate-800'}"
    >
      <Ruler class="w-4 h-4" />
    </button>

    <button
      on:click={resetView}
      title="Vycentrovat pohled"
      class="p-2 rounded-lg border border-slate-700 shadow-lg bg-slate-900/90 text-slate-300 hover:bg-slate-800 transition-colors"
    >
      <Maximize2 class="w-4 h-4" />
    </button>
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
      on:slideSelected={handleSlideSelected}
      on:slideContext={handleSlideContext}
      on:measurePointsChange={handleMeasurePointsChange}
      on:transformChanged={handleTransformChanged}
      on:saveState={handleSaveState}
      on:pathRebuildNeeded={handlePathRebuildNeeded}
    />
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
