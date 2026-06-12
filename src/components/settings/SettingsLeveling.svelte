<script lang="ts">
  import { run } from 'svelte/legacy';

  import { Plus, Trash2, Target } from "lucide-svelte";
  import { send_manual_command, auto_connect_printer } from "../../lib/tauri";
  import { printerStore } from "../../stores/printerStore";

  interface Props {
    settings: any;
    levelingPoints: { name: string; x: number; y: number }[];
  }

  let { settings = $bindable(), levelingPoints = $bindable() }: Props = $props();

  function addLevelingPoint() {
    levelingPoints = [...levelingPoints, { name: "", x: 125, y: 105 }];
  }
  function deleteLevelingPoint(i: number) {
    levelingPoints = levelingPoints.filter((_, idx) => idx !== i);
  }

  // ─── Test bodů ───────────────────────────────────────────────────────────
  let testRunning = $state(false);
  let testIndex = $state(0);
  let testOrder: number[] = $state([]);
  let testEditX = $state(0);
  let testEditY = $state(0);
  let testMoving = $state(false);
  let testError = $state("");
  let showPindaWarning = $state(false);

  function computeTestOrder(): number[] {
    if (levelingPoints.length === 0) return [];
    const groups: { y: number; indices: number[] }[] = [];
    levelingPoints.forEach((pt, i) => {
      const g = groups.find((g) => Math.abs(g.y - pt.y) < 5);
      if (g) g.indices.push(i);
      else groups.push({ y: pt.y, indices: [i] });
    });
    groups.sort((a, b) => a.y - b.y);
    const order: number[] = [];
    groups.forEach((g, rowIdx) => {
      const sorted = [...g.indices].sort((a, b) => levelingPoints[a].x - levelingPoints[b].x);
      if (rowIdx % 2 === 1) sorted.reverse();
      order.push(...sorted);
    });
    return order;
  }

  async function ensureConnected(): Promise<boolean> {
    if ($printerStore.is_connected) return true;
    try {
      await auto_connect_printer(115200);
      return $printerStore.is_connected;
    } catch {
      return false;
    }
  }

  function startTest() {
    if (levelingPoints.length === 0) return;
    showPindaWarning = true;
  }

  async function confirmStartTest() {
    showPindaWarning = false;
    const connected = await ensureConnected();
    if (!connected) {
      testError = "Tiskárna nemohla být připojena.";
      return;
    }
    testError = "";
    testOrder = computeTestOrder();
    testIndex = 0;
    testRunning = true;
    testMoving = true;
    const firstPt = levelingPoints[testOrder[0]];
    testEditX = firstPt.x;
    testEditY = firstPt.y;
    try {
      await send_manual_command(
        `G28 W\nG0 X${testEditX} Y${testEditY} F3000\n`
      );
    } catch (e: any) {
      testError = e?.message || String(e);
    } finally {
      testMoving = false;
    }
  }

  async function _testMoveTo(ptIndex: number) {
    testMoving = true;
    testEditX = levelingPoints[ptIndex].x;
    testEditY = levelingPoints[ptIndex].y;
    try {
      await send_manual_command(
        `G90\nG0 X${testEditX} Y${testEditY} F3000\n`
      );
    } catch (e: any) {
      testError = e?.message || String(e);
    } finally {
      testMoving = false;
    }
  }

  async function testMoveToEdited() {
    if (testMoving) return;
    testMoving = true;
    testError = "";
    try {
      const x = Number(testEditX);
      const y = Number(testEditY);
      await send_manual_command(`G0 X${x.toFixed(1)} Y${y.toFixed(1)} F3000\n`);
      levelingPoints[testOrder[testIndex]].x = x;
      levelingPoints[testOrder[testIndex]].y = y;
      levelingPoints = levelingPoints;
    } catch (e: any) {
      testError = e?.message || String(e);
    } finally {
      testMoving = false;
    }
  }

  async function testNext() {
    if (testMoving) return;
    levelingPoints[testOrder[testIndex]].x = Number(testEditX);
    levelingPoints[testOrder[testIndex]].y = Number(testEditY);
    levelingPoints = levelingPoints;
    if (testIndex < levelingPoints.length - 1) {
      testIndex++;
      await _testMoveTo(testOrder[testIndex]);
    } else {
      testRunning = false;
    }
  }

  function stopTest() {
    testRunning = false;
    testMoving = false;
  }

  // ─── Bed leveling SVG ────────────────────────────────────────────────────
  const LVL_VW = 420;
  const LVL_VH = 330;
  const LVL_ML = 38;
  const LVL_MT = 14;
  const LVL_MR = 10;
  const LVL_MB = 22;

  let lvlScale = $state(0), lvlBedW = $state(0), lvlBedH = $state(0), lvlCircleR = $state(0);
  let lvlXTicks: number[] = $state([]), lvlYTicks: number[] = $state([]);
  run(() => {
    lvlScale = Math.min(
      (LVL_VW - LVL_ML - LVL_MR) / (settings?.bed_max_x ?? 250),
      (LVL_VH - LVL_MT - LVL_MB) / (settings?.bed_max_y ?? 210)
    );
    lvlBedW = (settings?.bed_max_x ?? 250) * lvlScale;
    lvlBedH = (settings?.bed_max_y ?? 210) * lvlScale;
    lvlCircleR = ((settings?.leveling_circle_diameter ?? 8) / 2) * lvlScale;
    lvlXTicks = Array.from({ length: Math.floor((settings?.bed_max_x ?? 250) / 50) + 1 }, (_, i) => i * 50);
    lvlYTicks = Array.from({ length: Math.floor((settings?.bed_max_y ?? 210) / 50) + 1 }, (_, i) => i * 50);
  });
</script>

<div class="flex flex-col gap-3 text-xs">
  <!-- ── PINDA VAROVÁNÍ ────────────────────────────────────── -->
  {#if showPindaWarning}
    <div
      class="border border-yellow-500/60 bg-yellow-500/10 rounded-lg p-3 flex flex-col gap-3"
    >
      <div class="flex items-start gap-2">
        <span class="text-yellow-400 text-base leading-none mt-0.5">⚠</span>
        <div class="flex flex-col gap-1">
          <span class="font-bold text-[11px] text-yellow-300">Před zahájením kalibrace</span>
          <span class="text-[11px] text-yellow-200/80">
            Připevněte PINDA sondu k podložce (tiskové hlavě), než bude provedeno automatické najetí na home.
          </span>
        </div>
      </div>
      <div class="flex gap-2 justify-end">
        <button
          onclick={() => (showPindaWarning = false)}
          class="px-3 py-1 text-[11px] rounded-sm border border-slate-600 bg-slate-800/60 text-slate-300 hover:border-slate-400 transition-colors"
        >
          Zrušit
        </button>
        <button
          onclick={confirmStartTest}
          class="px-3 py-1 text-[11px] font-bold rounded-sm bg-yellow-500/80 hover:bg-yellow-500 text-slate-900 transition-colors"
        >
          Sonda připevněna — zahájit kalibraci
        </button>
      </div>
    </div>
  {/if}

  <!-- ── TEST PANEL (aktivní při testu) ─────────────────────── -->
  {#if testRunning}
    <div
      class="border border-labaccent/50 bg-labaccent/5 rounded-lg p-3 flex flex-col gap-2.5"
    >
      <!-- záhlaví: číslo bodu + stop -->
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-2">
          <div
            class="w-2 h-2 rounded-full bg-labaccent {testMoving
              ? 'animate-pulse'
              : ''}"
          ></div>
          <span class="font-bold text-[11px] text-slate-200">
            Bod {testIndex + 1} / {levelingPoints.length}
            {#if levelingPoints[testOrder[testIndex]]?.name}
              <span class="text-slate-400 font-normal"
                >— {levelingPoints[testOrder[testIndex]].name}</span
              >
            {/if}
          </span>
        </div>
        <button
          onclick={stopTest}
          class="text-[10px] text-slate-500 hover:text-labred transition-colors"
        >
          Ukončit test
        </button>
      </div>

      <!-- editace souřadnic + přejeď -->
      <div class="flex items-center gap-2 flex-wrap">
        <span class="text-slate-400 shrink-0">Upravit polohu:</span>
        <div class="flex items-center gap-1">
          <span class="text-slate-500 text-[10px]">X</span>
          <input
            type="number"
            step="1"
            bind:value={testEditX}
            class="w-16 input-premium py-0.5 text-center text-xs"
          />
        </div>
        <div class="flex items-center gap-1">
          <span class="text-slate-500 text-[10px]">Y</span>
          <input
            type="number"
            step="1"
            bind:value={testEditY}
            class="w-16 input-premium py-0.5 text-center text-xs"
          />
        </div>
        <button
          onclick={testMoveToEdited}
          disabled={testMoving}
          class="px-3 py-0.5 text-[11px] font-bold rounded border transition-colors
                 {testMoving
            ? 'opacity-40 cursor-not-allowed border-slate-700 text-slate-500'
            : 'border-slate-600 bg-slate-800/60 hover:border-labaccent text-slate-300 hover:text-labaccent'}"
        >
          Přejeď
        </button>
      </div>

      <!-- stavový řádek + OK / Dokončit -->
      <div
        class="flex items-center justify-between pt-1.5 border-t border-slate-800/60"
      >
        <span class="text-[10px] text-slate-500">
          {#if testMoving}
            Probíhá pohyb…
          {:else if testError}
            <span class="text-labred">{testError}</span>
          {:else}
            Čeká na potvrzení
          {/if}
        </span>
        <button
          onclick={testNext}
          disabled={testMoving}
          class="px-4 py-1 text-[11px] font-bold rounded transition-colors flex items-center gap-1.5
                 {testMoving
            ? 'opacity-40 cursor-not-allowed bg-slate-800 text-slate-500'
            : 'bg-labaccent hover:bg-blue-600 text-white shadow-xs shadow-blue-500/20'}"
        >
          {testIndex < levelingPoints.length - 1 ? "Další bod →" : "✓ Dokončit"}
        </button>
      </div>
    </div>
  {/if}

  <!-- ── HLAVNÍ OBSAH: levý sloupec + SVG ───────────────────── -->
  <div class="flex gap-4">
    <!-- LEVÝ SLOUPEC -->
    <div class="flex flex-col gap-3 w-44 shrink-0">
      <!-- Průměr kružnice -->
      <div
        class="flex items-center justify-between gap-2 pb-2 border-b border-slate-800"
      >
        <span class="text-slate-300 font-medium text-[11px]">Průměr kružnice</span>
        <div class="flex items-center gap-1">
          <input
            type="number"
            step="0.5"
            min="0.5"
            bind:value={settings.leveling_circle_diameter}
            class="w-14 input-premium py-0.5 text-center text-xs"
          />
          <span class="text-slate-500 text-[10px]">mm</span>
        </div>
      </div>

      <!-- Seznam bodů -->
      <div class="flex flex-col gap-2">
        <div class="flex justify-between items-center">
          <span class="font-bold text-[11px] text-slate-300">Kalibrační body</span>
          <button
            onclick={addLevelingPoint}
            disabled={testRunning}
            class="bg-labaccent hover:bg-blue-600 text-white text-[10px] font-bold px-2 py-0.5 rounded-sm flex items-center gap-1 transition-colors disabled:opacity-40"
          >
            <Plus class="w-3 h-3" /> Přidat
          </button>
        </div>

        <div
          class="flex flex-col border border-slate-800 rounded-lg overflow-hidden bg-slate-900/20"
        >
          <div
            class="grid grid-cols-5 bg-slate-950/50 px-2 py-1.5 font-bold text-[10px] text-slate-400 border-b border-slate-800"
          >
            <span class="col-span-2 text-center">X</span>
            <span class="col-span-2 text-center">Y</span>
            <span class="col-span-1"></span>
          </div>
          <div
            class="flex flex-col divide-y divide-slate-800 max-h-80 overflow-y-auto"
          >
            {#each levelingPoints as point, i}
              <div
                class="grid grid-cols-5 px-1 py-1 items-center gap-1
                          {testRunning && i === testOrder[testIndex] ? 'bg-labaccent/10' : ''}"
              >
                <input
                  type="number"
                  step="1"
                  bind:value={point.x}
                  class="col-span-2 input-premium py-0.5 text-center text-[11px]"
                />
                <input
                  type="number"
                  step="1"
                  bind:value={point.y}
                  class="col-span-2 input-premium py-0.5 text-center text-[11px]"
                />
                <button
                  onclick={() => deleteLevelingPoint(i)}
                  disabled={testRunning}
                  class="col-span-1 p-1 text-slate-500 hover:text-labred hover:bg-labred/10 rounded-sm flex items-center justify-center transition-colors disabled:opacity-30"
                >
                  <Trash2 class="w-3 h-3" />
                </button>
              </div>
            {:else}
              <div class="p-4 text-center text-slate-500 text-[10px]">
                Žádné body.
              </div>
            {/each}
          </div>
        </div>
      </div>

      <!-- Tlačítko Test bodů -->
      <button
        onclick={testRunning ? stopTest : startTest}
        disabled={!testRunning && levelingPoints.length === 0}
        class="w-full py-1.5 text-[11px] font-bold rounded border transition-colors flex items-center justify-center gap-1.5
               {testRunning
          ? 'border-labred/60 bg-labred/10 text-labred hover:bg-labred/20'
          : 'border-labaccent/50 bg-labaccent/10 text-labaccent hover:bg-labaccent/20 disabled:opacity-40 disabled:cursor-not-allowed'}"
      >
        <Target class="w-3.5 h-3.5" />
        {testRunning ? "Zastavit test" : "Test bodů"}
      </button>
    </div>
    <!-- end levý sloupec -->

    <!-- PRAVÝ SLOUPEC: SVG -->
    <div class="flex-1 flex flex-col gap-2 min-w-0">
      <span class="font-bold text-xs text-slate-300 pb-1 border-b border-slate-800"
        >Náhled rozmístění na podložce</span
      >
      <div class="bg-slate-900/20 rounded-lg border border-slate-800 p-2">
        <svg
          viewBox="0 0 {LVL_VW} {LVL_VH}"
          width="100%"
          preserveAspectRatio="xMidYMid meet"
        >
          <!-- Podložka -->
          <rect
            x={LVL_ML}
            y={LVL_MT}
            width={lvlBedW}
            height={lvlBedH}
            fill="rgba(15,23,42,0.9)"
            stroke="rgba(100,116,139,0.5)"
            stroke-width="1"
            rx="2"
          />

          <!-- Mřížka X -->
          {#each lvlXTicks as tick}
            {#if tick > 0 && tick < (settings?.bed_max_x ?? 250)}
              <line
                x1={LVL_ML + tick * lvlScale}
                y1={LVL_MT}
                x2={LVL_ML + tick * lvlScale}
                y2={LVL_MT + lvlBedH}
                stroke="rgba(100,116,139,0.18)"
                stroke-width="0.5"
                stroke-dasharray="3,3"
              />
            {/if}
          {/each}

          <!-- Mřížka Y -->
          {#each lvlYTicks as tick}
            {#if tick > 0 && tick < (settings?.bed_max_y ?? 210)}
              <line
                x1={LVL_ML}
                y1={LVL_MT + lvlBedH - tick * lvlScale}
                x2={LVL_ML + lvlBedW}
                y2={LVL_MT + lvlBedH - tick * lvlScale}
                stroke="rgba(100,116,139,0.18)"
                stroke-width="0.5"
                stroke-dasharray="3,3"
              />
            {/if}
          {/each}

          <!-- Číselné popisky X -->
          {#each lvlXTicks as tick}
            <line
              x1={LVL_ML + tick * lvlScale}
              y1={LVL_MT + lvlBedH}
              x2={LVL_ML + tick * lvlScale}
              y2={LVL_MT + lvlBedH + 4}
              stroke="rgba(100,116,139,0.5)"
              stroke-width="1"
            />
            <text
              x={LVL_ML + tick * lvlScale}
              y={LVL_MT + lvlBedH + 13}
              text-anchor="middle"
              font-size="8"
              fill="rgba(148,163,184,0.7)">{tick}</text
            >
          {/each}

          <!-- Číselné popisky Y -->
          {#each lvlYTicks as tick}
            <line
              x1={LVL_ML - 4}
              y1={LVL_MT + lvlBedH - tick * lvlScale}
              x2={LVL_ML}
              y2={LVL_MT + lvlBedH - tick * lvlScale}
              stroke="rgba(100,116,139,0.5)"
              stroke-width="1"
            />
            <text
              x={LVL_ML - 7}
              y={LVL_MT + lvlBedH - tick * lvlScale + 3}
              text-anchor="end"
              font-size="8"
              fill="rgba(148,163,184,0.7)">{tick}</text
            >
          {/each}

          <!-- Kalibrační body -->
          {#each levelingPoints as point, i}
            {@const px = LVL_ML + point.x * lvlScale}
            {@const py = LVL_MT + lvlBedH - point.y * lvlScale}
            {@const active = testRunning && i === testOrder[testIndex]}
            <circle
              cx={px}
              cy={py}
              r={lvlCircleR}
              fill={active ? "rgba(59,130,246,0.30)" : "rgba(59,130,246,0.12)"}
              stroke={active ? "rgba(120,170,255,1)" : "rgba(99,153,255,0.65)"}
              stroke-width={active ? "2.5" : "1.5"}
            />
            <circle
              cx={px}
              cy={py}
              r={Math.max(2, lvlCircleR * 0.14)}
              fill={active ? "rgba(255,255,255,0.95)" : "rgba(99,153,255,0.9)"}
            />
            {#if point.name}
              <text
                x={px}
                y={py - lvlCircleR - 4}
                text-anchor="middle"
                font-size="7.5"
                font-weight={active ? "bold" : "normal"}
                fill={active ? "rgba(220,235,255,1)" : "rgba(186,207,255,0.85)"}
                >{point.name}</text
              >
            {/if}
          {/each}

          <!-- Editovaná poloha při testu (žlutý křížek) -->
          {#if testRunning}
            {@const epx = LVL_ML + Number(testEditX) * lvlScale}
            {@const epy = LVL_MT + lvlBedH - Number(testEditY) * lvlScale}
            <line
              x1={epx - 6}
              y1={epy}
              x2={epx + 6}
              y2={epy}
              stroke="rgba(250,204,21,0.85)"
              stroke-width="1.5"
            />
            <line
              x1={epx}
              y1={epy - 6}
              x2={epx}
              y2={epy + 6}
              stroke="rgba(250,204,21,0.85)"
              stroke-width="1.5"
            />
          {/if}
        </svg>
      </div>
    </div>
    <!-- end pravý sloupec -->
  </div>
</div>
