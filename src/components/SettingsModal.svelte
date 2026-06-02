<script lang="ts">
  import { onMount } from "svelte";
  import { get_app_settings, save_app_settings, send_manual_command, auto_connect_printer } from "../lib/tauri";
  import { printerStore } from "../stores/printerStore";
  import { X, Plus, Trash2, Save, GripVertical, Sun, Moon, Monitor, Target } from "lucide-svelte";
  import { createEventDispatcher } from "svelte";

  export let isOpen = false;

  const dispatch = createEventDispatcher();
  let activeTab: "nozzles" | "glass" | "limits" | "leveling" | "gcode" | "program" = "nozzles";

  let settings: any = null;
  let nozzleList: { name: string; h: number; d: number; s: number; c: string }[] = [];
  let glassList: { name: string; w: number; h: number; z: number }[] = [];
  let levelingPoints: { name: string; x: number; y: number }[] = [];

  const DEFAULT_LEVELING_POINTS = [
    { name: "Levý přední", x: 37, y: 7 },
    { name: "Střed přední", x: 125, y: 7 },
    { name: "Pravý přední", x: 213, y: 7 },
    { name: "Levý střed", x: 37, y: 105 },
    { name: "Střed", x: 125, y: 105 },
    { name: "Pravý střed", x: 213, y: 105 },
    { name: "Levý zadní", x: 37, y: 203 },
    { name: "Střed zadní", x: 125, y: 203 },
    { name: "Pravý zadní", x: 213, y: 203 },
  ];
  let loading = false;
  let errorMsg = "";

  // ─── Theme ────────────────────────────────────────────────────────────────
  type Theme = "dark" | "light";
  let currentTheme: Theme = "dark";

  function applyTheme(t: Theme) {
    currentTheme = t;
    document.documentElement.setAttribute("data-theme", t);
    localStorage.setItem("app-theme", t);
  }

  onMount(() => {
    const stored = localStorage.getItem("app-theme") as Theme | null;
    applyTheme(stored ?? "dark");
  });

  // ─── Drag & Drop helper ───────────────────────────────────────────────────
  let dragSrcIndex: number | null = null;
  let dragOverIndex: number | null = null;
  let dragList: "nozzles" | "glass" | null = null;

  function onDragStart(e: DragEvent, list: "nozzles" | "glass", i: number) {
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", i.toString()); // Povinné pro Firefox
    }
    dragSrcIndex = i;
    dragList = list;
  }

  function onDragOver(e: DragEvent, i: number) {
    e.preventDefault();
    dragOverIndex = i;
  }

  function onDrop(list: "nozzles" | "glass", i: number) {
    if (dragSrcIndex === null || dragList !== list || dragSrcIndex === i) {
      dragSrcIndex = dragOverIndex = null;
      return;
    }
    if (list === "nozzles") {
      const arr = [...nozzleList];
      const [item] = arr.splice(dragSrcIndex, 1);
      arr.splice(i, 0, item);
      nozzleList = arr;
    } else {
      const arr = [...glassList];
      const [item] = arr.splice(dragSrcIndex, 1);
      arr.splice(i, 0, item);
      glassList = arr;
    }
    dragSrcIndex = dragOverIndex = null;
  }

  function onDragEnd() {
    dragSrcIndex = dragOverIndex = null;
  }

  // ─── Settings load ────────────────────────────────────────────────────────
  async function loadSettings() {
    loading = true;
    errorMsg = "";
    try {
      settings = await get_app_settings();

      // Nozzles — zachováme pořadí z pole nozzle_defs (Object.entries zachovává vložení)
      nozzleList = Object.entries(settings.nozzle_defs || {}).map(([name, val]: [string, any]) => ({
        name,
        h: val[0] ?? 30.0,
        d: val[1] ?? 0.4,
        s: val[2] ?? 4.0,
        c: val[3] ?? "#3b82f6",
      }));

      // Glass
      glassList = Object.entries(settings.sklo_dims || {}).map(([name, val]: [string, any]) => ({
        name,
        w: val[0] ?? 25.0,
        h: val[1] ?? 75.0,
        z: val[2] ?? 1.0,
      }));

      // Leveling points
      const rawPoints = settings.leveling_points ?? [];
      levelingPoints = (rawPoints.length > 0 ? rawPoints : DEFAULT_LEVELING_POINTS).map(
        (p: any) => ({ name: p.name || "Bod", x: Number(p.x) || 0, y: Number(p.y) || 0 })
      );
      settings.leveling_circle_diameter ??= 8.0;

      // Výchozí hodnoty nových políček pokud nejsou v settings.json
      settings.bed_min_temp ??= 30;
      settings.bed_min_x ??= 0.0;
    } catch (e: any) {
      console.error("Failed to load settings in modal:", e);
      errorMsg = e?.message || String(e) || "Neznámá chyba při načítání nastavení";
      settings = null;
    } finally {
      loading = false;
    }
  }

  $: if (isOpen) loadSettings();

  // ─── CRUD ─────────────────────────────────────────────────────────────────
  function addNozzle() {
    nozzleList = [...nozzleList, { name: "Nová tryska", h: 30.0, d: 0.4, s: 4.0, c: "#3b82f6" }];
  }
  function deleteNozzle(i: number) {
    nozzleList = nozzleList.filter((_, idx) => idx !== i);
  }

  function addGlass() {
    glassList = [...glassList, { name: "Nové sklo", w: 25.0, h: 75.0, z: 1.0 }];
  }
  function deleteGlass(i: number) {
    glassList = glassList.filter((_, idx) => idx !== i);
  }

  function addLevelingPoint() {
    levelingPoints = [...levelingPoints, { name: "", x: 125, y: 105 }];
  }
  function deleteLevelingPoint(i: number) {
    levelingPoints = levelingPoints.filter((_, idx) => idx !== i);
  }

  // ─── Test bodů ───────────────────────────────────────────────────────────
  let testRunning = false;
  let testIndex = 0;
  let testOrder: number[] = [];
  let testEditX = 0;
  let testEditY = 0;
  let testMoving = false;
  let testError = "";
  let showPindaWarning = false;

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

  // ─── Save ────────────────────────────────────────────────────────────────
  async function save() {
    try {
      // Nozzles — pořadí zachováno z nozzleList
      const nozzle_defs: Record<string, any[]> = {};
      nozzleList.forEach((n) => {
        if (n.name.trim()) nozzle_defs[n.name.trim()] = [n.h, n.d, n.s, n.c];
      });

      // Glass — pořadí zachováno z glassList
      const sklo_dims: Record<string, number[]> = {};
      glassList.forEach((g) => {
        if (g.name.trim()) sklo_dims[g.name.trim()] = [g.w, g.h, g.z];
      });

      settings.nozzle_defs = nozzle_defs;
      settings.sklo_dims = sklo_dims;
      settings.leveling_points = levelingPoints.map((p) => ({ name: p.name, x: p.x, y: p.y }));

      await save_app_settings(settings);
      dispatch("save");
      isOpen = false;
    } catch (e) {
      alert(`Chyba při ukládání nastavení: ${e}`);
    }
  }

  function close() {
    isOpen = false;
  }

  // ─── Reaktivní minimy rozměrů tiskárny ───────────────────────────────────
  // Pravá strana je fixovaná (bed_max_x). Plocha roste doleva od ní.
  // Minimum bed_max_x: start_offset_x + max(šířka odplivu=76, největší sklíčko šířka)
  // Minimum bed_max_y: start_offset_y + výška odplivu(26) + mezera + největší sklíčko výška
  $: maxSlideW = glassList.length > 0 ? Math.max(...glassList.map((g) => g.w)) : 0;
  $: maxSlideH = glassList.length > 0 ? Math.max(...glassList.map((g) => g.h)) : 0;
  $: minBedMaxX = Math.ceil((settings?.start_offset_x ?? 18) + Math.max(76, maxSlideW));
  $: minBedMaxY = Math.ceil(
    (settings?.start_offset_y ?? 11) + 26 + (settings?.multi_spacing ?? 5) + maxSlideH
  );

  // ─── Bed leveling SVG ────────────────────────────────────────────────────
  const LVL_VW = 420;
  const LVL_VH = 330;
  const LVL_ML = 38; // margin left (Y tick numbers)
  const LVL_MT = 14; // margin top
  const LVL_MR = 10; // margin right
  const LVL_MB = 22; // margin bottom (X tick numbers)

  $: lvlScale = Math.min(
    (LVL_VW - LVL_ML - LVL_MR) / (settings?.bed_max_x ?? 250),
    (LVL_VH - LVL_MT - LVL_MB) / (settings?.bed_max_y ?? 210)
  );
  $: lvlBedW = (settings?.bed_max_x ?? 250) * lvlScale;
  $: lvlBedH = (settings?.bed_max_y ?? 210) * lvlScale;
  $: lvlCircleR = ((settings?.leveling_circle_diameter ?? 8) / 2) * lvlScale;
  $: lvlXTicks = Array.from(
    { length: Math.floor((settings?.bed_max_x ?? 250) / 50) + 1 },
    (_, i) => i * 50
  );
  $: lvlYTicks = Array.from(
    { length: Math.floor((settings?.bed_max_y ?? 210) / 50) + 1 },
    (_, i) => i * 50
  );

  // ─── Interaktivní SVG (bed visualizer) ───────────────────────────────────
  // SVG viewport: 400×280, s marginy 30px vlevo/dole pro osy a 20px nahoře/vpravo
  const SVG_MARGIN_L = 35;
  const SVG_MARGIN_T = 30;
  const SVG_INNER_W = 320;
  const SVG_INNER_H = 210;

  // Přepočet bed_max_x/y → SVG souřadnice (scaled do max 320×210)
  $: maxBedDim = Math.max(settings?.bed_max_x ?? 200, settings?.bed_max_y ?? 200, 1);
  $: svgBedW = ((settings?.bed_max_x ?? 200) / maxBedDim) * SVG_INNER_W;
  $: svgBedH = ((settings?.bed_max_y ?? 200) / maxBedDim) * SVG_INNER_H;
  $: svgBedX = SVG_MARGIN_L;
  $: svgBedY = SVG_MARGIN_T + (SVG_INNER_H - svgBedH); // zarovnat doleva-dolů

  // Přepočet offset → SVG souřadnice (relativně k tiskové ploše)
  $: svgOffX = svgBedX + ((settings?.start_offset_x ?? 0) / (settings?.bed_max_x ?? 200)) * svgBedW;
  $: svgOffY =
    svgBedY + svgBedH - ((settings?.start_offset_y ?? 0) / (settings?.bed_max_y ?? 200)) * svgBedH;

  // Drag stav
  let svgDragTarget: "bed-br" | "offset" | null = null;
  let svgDragStart = { x: 0, y: 0 };
  let svgValStart = { bx: 200, by: 200, ox: 0, oy: 0 };

  function svgStartDrag(e: MouseEvent, target: "bed-br" | "offset") {
    e.preventDefault();
    svgDragTarget = target;
    svgDragStart = { x: e.clientX, y: e.clientY };
    svgValStart = {
      bx: settings?.bed_max_x ?? 200,
      by: settings?.bed_max_y ?? 200,
      ox: settings?.start_offset_x ?? 0,
      oy: settings?.start_offset_y ?? 0,
    };
  }

  function svgMouseMove(e: MouseEvent) {
    if (!svgDragTarget || !settings) return;
    const dx = e.clientX - svgDragStart.x;
    const dy = e.clientY - svgDragStart.y;

    // Kolik mm odpovídá jednomu pixelu SVG
    const pxPerMm = SVG_INNER_W / (svgValStart.bx || 200);

    if (svgDragTarget === "bed-br") {
      // Táhnutí pravého dolního rohu → mění bed_max_x a bed_max_y
      const newBx = Math.max(
        10,
        Math.round(svgValStart.bx + dx / (SVG_INNER_W / (svgValStart.bx || 200)) / 5) * 5
      );
      const newBy = Math.max(
        10,
        Math.round(svgValStart.by + dy / (SVG_INNER_H / (svgValStart.by || 200)) / 5) * 5
      );
      settings.bed_max_x = newBx;
      settings.bed_max_y = newBy;
      settings = settings; // trigger reaktivity
    } else if (svgDragTarget === "offset") {
      // Táhnutí startu → mění start_offset_x a start_offset_y
      const dxMm = dx / (svgBedW / (settings.bed_max_x || 200));
      const dyMm = -dy / (svgBedH / (settings.bed_max_y || 200));
      settings.start_offset_x = Math.max(0, Math.round((svgValStart.ox + dxMm) * 10) / 10);
      settings.start_offset_y = Math.max(0, Math.round((svgValStart.oy + dyMm) * 10) / 10);
      settings = settings;
    }
  }

  function svgMouseUp() {
    svgDragTarget = null;
  }

  function tabClass(tab: string) {
    return activeTab === tab
      ? "px-3 py-2 rounded text-left text-xs font-bold transition-colors bg-labaccent text-white shadow-md shadow-labaccent/20"
      : "px-3 py-2 rounded text-left text-xs font-bold transition-colors text-slate-400 hover:bg-slate-900/40 hover:text-slate-200";
  }

  async function restoreDefaults() {
    const confirmed = await import("@tauri-apps/plugin-dialog").then((m) =>
      m.ask(
        "Opravdu chcete obnovit výchozí nastavení?\n\nPřepíšou se limitace tiskárny a inicializační G-kódy. Definice trysek a sklíček zůstanou zachovány.",
        { title: "DPI", type: "warning" }
      )
    );
    if (!confirmed) return;

    settings.start_gcode =
      ";FLAVOR:Marlin\n; --- INICIALIZACE TISKÁRNY PRO KAPALINY ---\nM201 X1000 Y1000 Z200 E5000\nM203 X200 Y200 Z12 E120\nM204 S1250 T1250\nM205 X8.00 Y8.00 Z0.40 E4.50\nM205 S0 T0\n\nG90 ; use absolute coordinates\nM83 ; extruder RELATIVE mode\nM302 P1 ; disable cold extrusion checking\nM302 S0 ; always allow extrusion\nM900 K0 ; disable Linear Advance for liquids\n\nG28\nG92 E0.0\n";
    settings.end_gcode =
      "G0 Z30 F1000 ; Zvednuti tiskove hlavy\nG0 X0 Y200 F3000 ; Vysunuti podlozky vpred\nM84 ; Vypnuti motoru\n";
    settings.loop_start_gcode = "";
    settings.loop_end_gcode = "";

    settings.bed_max_x = 250.0;
    settings.bed_max_y = 210.0;
    settings.bed_min_x = 0.0;
    settings.start_offset_x = 18.0;
    settings.start_offset_y = 11.0;
    settings.multi_spacing = 5.0;
    settings.block_height = 34.0;
    settings.print_speed = 1500;
    settings.bed_min_temp = 30;
    settings.calibration_factor = 0.323877;
    settings.calibration_object_height = 0.1;
    settings.leveling_circle_diameter = 8.0;
    settings.leveling_points = DEFAULT_LEVELING_POINTS;
    levelingPoints = DEFAULT_LEVELING_POINTS.map((p) => ({ ...p }));
  }
</script>

{#if isOpen}
  <div
    class="fixed inset-0 bg-black/75 backdrop-blur-sm flex items-center justify-center z-50 p-4 select-text"
  >
    <div
      class="glass-panel w-full max-w-4xl h-[85vh] rounded-xl flex flex-col overflow-hidden border border-slate-700/50 shadow-2xl"
    >
      <!-- HEADER -->
      <div
        class="flex items-center justify-between px-5 py-3.5 border-b border-slate-800/80 bg-slate-950/20"
      >
        <div>
          <h2 class="text-sm font-bold text-slate-200 uppercase tracking-wider">
            Pokročilé nastavení aplikace
          </h2>
          <p class="text-[10px] text-slate-500">
            Konfigurace tiskových parametrů, presetů a limitů tiskárny
          </p>
        </div>
        <button
          on:click={close}
          class="p-1 rounded bg-slate-900 border border-slate-800 hover:bg-slate-800 text-slate-400 hover:text-slate-200 transition-colors"
        >
          <X class="w-4 h-4" />
        </button>
      </div>

      {#if loading}
        <div class="flex-1 flex flex-col items-center justify-center p-8 gap-4 text-center">
          <div
            class="w-12 h-12 border-4 border-slate-800 border-t-labaccent rounded-full animate-spin"
          ></div>
          <p class="text-slate-300 text-sm font-medium">Načítám nastavení...</p>
        </div>
      {:else if errorMsg}
        <div
          class="flex-1 flex flex-col items-center justify-center p-8 gap-4 text-center max-w-md mx-auto"
        >
          <div
            class="w-12 h-12 rounded-full bg-labred/20 border border-labred/40 flex items-center justify-center text-labred"
          >
            <X class="w-6 h-6" />
          </div>
          <h3 class="text-slate-200 font-bold text-sm">Nastavení se nepodařilo načíst</h3>
          <p
            class="text-slate-400 text-xs font-mono bg-slate-950/40 p-3 rounded-lg border border-slate-800/60 max-w-full overflow-x-auto whitespace-pre-wrap"
          >
            {errorMsg}
          </p>
          <div class="flex gap-3 mt-2">
            <button
              on:click={loadSettings}
              class="bg-labaccent hover:bg-blue-600 text-white font-bold text-xs px-4 py-2 rounded-lg transition-colors"
              >Zkusit znovu</button
            >
            <button
              on:click={close}
              class="bg-slate-900 border border-slate-700 hover:bg-slate-800 text-slate-300 font-bold text-xs px-4 py-2 rounded-lg transition-colors"
              >Zavřít</button
            >
          </div>
        </div>
      {:else if settings}
        <div class="flex-1 flex overflow-hidden">
          <!-- SIDEBAR -->
          <div
            class="w-48 bg-slate-950/20 border-r border-slate-800 flex flex-col p-2 gap-1 relative"
          >
            <div class="flex-1 flex flex-col gap-1">
              <button on:click={() => (activeTab = "nozzles")} class={tabClass("nozzles")}
                >Definice trysek</button
              >
              <button on:click={() => (activeTab = "glass")} class={tabClass("glass")}
                >Rozměry sklíček</button
              >
              <button on:click={() => (activeTab = "limits")} class={tabClass("limits")}
                >Limitace tiskárny</button
              >
              <button on:click={() => (activeTab = "gcode")} class={tabClass("gcode")}
                >Inicializační G-kódy</button
              >
              <button on:click={() => (activeTab = "program")} class={tabClass("program")}
                >Program</button
              >
            </div>
            <button
              on:click={restoreDefaults}
              class="mt-auto px-3 py-2 rounded text-left text-xs font-bold transition-colors text-slate-400 hover:bg-labred/20 hover:text-labred"
            >
              Obnovit výchozí
            </button>
          </div>

          <!-- TAB CONTENT -->
          <div class="flex-1 p-5 overflow-y-auto min-h-0 bg-slate-950/5">
            <!-- ═══ 1. TRYSKY ═══ -->
            {#if activeTab === "nozzles"}
              <div class="flex flex-col gap-3">
                <div class="flex justify-between items-center pb-2 border-b border-slate-800">
                  <span class="font-bold text-xs text-slate-300">Presety laboratorních trysek</span>
                  <button
                    on:click={addNozzle}
                    class="bg-labaccent hover:bg-blue-600 text-white text-[10px] font-bold px-2 py-1 rounded flex items-center gap-1 transition-colors"
                  >
                    <Plus class="w-3 h-3" /> Přidat trysku
                  </button>
                </div>

                <div
                  class="flex flex-col border border-slate-800 rounded-lg overflow-hidden bg-slate-900/20"
                >
                  <!-- header -->
                  <div
                    class="grid grid-cols-12 bg-slate-950/50 p-2 font-bold text-[10px] text-slate-400 text-center border-b border-slate-800"
                  >
                    <span class="col-span-1"></span>
                    <span class="col-span-3 text-left pl-1">Název trysky</span>
                    <span class="col-span-1">Barva</span>
                    <span class="col-span-2">Výška [mm]</span>
                    <span class="col-span-2">Průměr [mm]</span>
                    <span class="col-span-2">Skrytá [mm]</span>
                    <span class="col-span-1">Akce</span>
                  </div>

                  <div class="flex flex-col divide-y divide-slate-800">
                    {#each nozzleList as nozzle, i}
                      <!-- svelte-ignore a11y-no-static-element-interactions -->
                      <div
                        class="grid grid-cols-12 p-2 items-center text-center gap-1 text-xs transition-colors
                               {dragOverIndex === i && dragList === 'nozzles'
                          ? 'bg-labaccent/10 border-t-2 border-labaccent'
                          : 'hover:bg-slate-900/30'}"
                        draggable="true"
                        on:dragstart={(e) => onDragStart(e, "nozzles", i)}
                        on:dragover={(e) => onDragOver(e, i)}
                        on:drop={() => onDrop("nozzles", i)}
                        on:dragend={onDragEnd}
                      >
                        <!-- drag handle -->
                        <div
                          class="col-span-1 flex justify-center text-slate-600 hover:text-slate-400 cursor-grab active:cursor-grabbing"
                        >
                          <GripVertical class="w-3.5 h-3.5" />
                        </div>
                        <input
                          type="text"
                          bind:value={nozzle.name}
                          class="col-span-3 input-premium py-0.5 text-left text-[11px]"
                        />
                        <div class="col-span-1 flex justify-center items-center">
                          <input
                            type="color"
                            bind:value={nozzle.c}
                            class="w-5 h-5 rounded cursor-pointer border-none bg-transparent"
                          />
                        </div>
                        <input
                          type="number"
                          step="0.1"
                          bind:value={nozzle.h}
                          class="col-span-2 input-premium py-0.5 text-center text-[11px]"
                        />
                        <input
                          type="number"
                          step="0.01"
                          bind:value={nozzle.d}
                          class="col-span-2 input-premium py-0.5 text-center text-[11px]"
                        />
                        <input
                          type="number"
                          step="0.1"
                          bind:value={nozzle.s}
                          class="col-span-2 input-premium py-0.5 text-center text-[11px]"
                        />
                        <button
                          on:click={() => deleteNozzle(i)}
                          class="col-span-1 p-1 text-slate-500 hover:text-labred hover:bg-labred/10 rounded flex items-center justify-center transition-colors"
                        >
                          <Trash2 class="w-3.5 h-3.5" />
                        </button>
                      </div>
                    {:else}
                      <div class="p-6 text-center text-slate-500 text-xs">
                        Žádné trysky nejsou definovány.
                      </div>
                    {/each}
                  </div>
                </div>
                <p class="text-[10px] text-slate-500 flex items-center gap-1">
                  <GripVertical class="w-3 h-3" /> Přetáhněte řádky pro změnu pořadí. Pořadí v panelu
                  odpovídá tomuto seznamu.
                </p>
              </div>
            {/if}

            <!-- ═══ 2. SKLÍČKA ═══ -->
            {#if activeTab === "glass"}
              <div class="flex flex-col gap-3">
                <div class="flex justify-between items-center pb-2 border-b border-slate-800">
                  <span class="font-bold text-xs text-slate-300">Presety podložek (skel)</span>
                  <button
                    on:click={addGlass}
                    class="bg-labaccent hover:bg-blue-600 text-white text-[10px] font-bold px-2 py-1 rounded flex items-center gap-1 transition-colors"
                  >
                    <Plus class="w-3 h-3" /> Přidat podložku
                  </button>
                </div>

                <div
                  class="flex flex-col border border-slate-800 rounded-lg overflow-hidden bg-slate-900/20"
                >
                  <div
                    class="grid grid-cols-12 bg-slate-950/50 p-2 font-bold text-[10px] text-slate-400 text-center border-b border-slate-800"
                  >
                    <span class="col-span-1"></span>
                    <span class="col-span-4 text-left pl-1">Název sklíčka</span>
                    <span class="col-span-2">Šířka X [mm]</span>
                    <span class="col-span-2">Výška Y [mm]</span>
                    <span class="col-span-2">Tloušťka Z [mm]</span>
                    <span class="col-span-1">Akce</span>
                  </div>

                  <div class="flex flex-col divide-y divide-slate-800">
                    {#each glassList as glass, i}
                      <!-- svelte-ignore a11y-no-static-element-interactions -->
                      <div
                        class="grid grid-cols-12 p-2 items-center text-center gap-1 text-xs transition-colors
                               {dragOverIndex === i && dragList === 'glass'
                          ? 'bg-labaccent/10 border-t-2 border-labaccent'
                          : 'hover:bg-slate-900/30'}"
                        draggable="true"
                        on:dragstart={(e) => onDragStart(e, "glass", i)}
                        on:dragover={(e) => onDragOver(e, i)}
                        on:drop={() => onDrop("glass", i)}
                        on:dragend={onDragEnd}
                      >
                        <div
                          class="col-span-1 flex justify-center text-slate-600 hover:text-slate-400 cursor-grab active:cursor-grabbing"
                        >
                          <GripVertical class="w-3.5 h-3.5" />
                        </div>
                        <input
                          type="text"
                          bind:value={glass.name}
                          class="col-span-4 input-premium py-0.5 text-left text-[11px]"
                        />
                        <input
                          type="number"
                          step="1"
                          bind:value={glass.w}
                          class="col-span-2 input-premium py-0.5 text-center text-[11px]"
                        />
                        <input
                          type="number"
                          step="1"
                          bind:value={glass.h}
                          class="col-span-2 input-premium py-0.5 text-center text-[11px]"
                        />
                        <input
                          type="number"
                          step="0.1"
                          bind:value={glass.z}
                          class="col-span-2 input-premium py-0.5 text-center text-[11px]"
                        />
                        <button
                          on:click={() => deleteGlass(i)}
                          class="col-span-1 p-1 text-slate-500 hover:text-labred hover:bg-labred/10 rounded flex items-center justify-center transition-colors"
                        >
                          <Trash2 class="w-3.5 h-3.5" />
                        </button>
                      </div>
                    {:else}
                      <div class="p-6 text-center text-slate-500 text-xs">
                        Žádné podložky nejsou definovány.
                      </div>
                    {/each}
                  </div>
                </div>
                <p class="text-[10px] text-slate-500 flex items-center gap-1">
                  <GripVertical class="w-3 h-3" /> Přetáhněte řádky pro změnu pořadí. První v seznamu
                  bude výchozí volba.
                </p>
              </div>
            {/if}

            <!-- ═══ 3. LIMITY ═══ -->
            {#if activeTab === "limits"}
              <div class="flex flex-col gap-5">
                <span class="font-bold text-xs text-slate-300 pb-1 border-b border-slate-800"
                  >Limitace a parametry tiskárny</span
                >

                <!-- INTERAKTIVNÍ SVG PODLOŽKY ODSTRANĚN DLE ZADÁNÍ -->

                <!-- SEKCE: ROZMĚRY PODLOŽKY -->
                <div class="flex flex-col gap-3">
                  <div class="flex items-center gap-2">
                    <span class="w-2 h-2 rounded-full bg-blue-500"></span>
                    <span class="text-xs font-bold text-slate-300 uppercase tracking-wider"
                      >Rozměry tiskové plochy</span
                    >
                  </div>
                  <p class="text-[10px] text-slate-500 pl-4">
                    Pravá strana tisku je fixovaná. Oblast roste doleva — sklíčka se přidávají ve
                    sloupcích od pravé strany.
                  </p>
                  <div class="grid grid-cols-1 gap-2.5 text-xs pl-4">
                    <div class="grid grid-cols-5 items-center gap-3">
                      <div class="col-span-3">
                        <div class="text-slate-300 font-medium">Pravá hranice osy X</div>
                        <div class="text-[10px] text-slate-500 mt-0.5">
                          Pravý okraj tiskové plochy. Min: <span class="font-mono text-slate-400"
                            >{minBedMaxX} mm</span
                          >
                        </div>
                      </div>
                      <div class="col-span-2 flex items-center gap-1.5">
                        <input
                          type="number"
                          step="5"
                          min={minBedMaxX}
                          bind:value={settings.bed_max_x}
                          class="flex-1 input-premium py-1 text-center text-xs"
                        />
                        <span class="text-slate-500 text-[10px] w-6">mm</span>
                      </div>
                    </div>
                    <div class="grid grid-cols-5 items-center gap-3">
                      <div class="col-span-3">
                        <div class="text-slate-300 font-medium">Maximální délka osy Y</div>
                        <div class="text-[10px] text-slate-500 mt-0.5">
                          Fyzická délka podložky. Min: <span class="font-mono text-slate-400"
                            >{minBedMaxY} mm</span
                          >
                        </div>
                      </div>
                      <div class="col-span-2 flex items-center gap-1.5">
                        <input
                          type="number"
                          step="5"
                          min={minBedMaxY}
                          bind:value={settings.bed_max_y}
                          class="flex-1 input-premium py-1 text-center text-xs"
                        />
                        <span class="text-slate-500 text-[10px] w-6">mm</span>
                      </div>
                    </div>
                  </div>
                </div>

                <div class="border-b border-slate-800/60"></div>

                <!-- SEKCE: TEPLOTA -->
                <div class="flex flex-col gap-3">
                  <div class="flex items-center gap-2">
                    <span class="w-2 h-2 rounded-full bg-orange-500"></span>
                    <span class="text-xs font-bold text-slate-300 uppercase tracking-wider"
                      >Teplota podložky</span
                    >
                  </div>
                  <div class="grid grid-cols-1 gap-2.5 text-xs pl-4">
                    <div class="grid grid-cols-5 items-center gap-3">
                      <div class="col-span-3">
                        <div class="text-slate-300 font-medium">Maximální teplota</div>
                        <div class="text-[10px] text-slate-500 mt-0.5">
                          Horní mez výhřevu. Hodnota 0 = bez limitu
                        </div>
                      </div>
                      <div class="col-span-2 flex items-center gap-1.5">
                        <input
                          type="number"
                          step="5"
                          min="0"
                          bind:value={settings.bed_max_temp}
                          class="flex-1 input-premium py-1 text-center text-xs"
                        />
                        <span class="text-slate-500 text-[10px] w-6">°C</span>
                      </div>
                    </div>
                    <div class="grid grid-cols-5 items-center gap-3">
                      <div class="col-span-3">
                        <div class="text-slate-300 font-medium">Minimální teplota při zapnutí</div>
                        <div class="text-[10px] text-slate-500 mt-0.5">
                          Teplota výhřevu při aktivaci — přeskočí šedou zónu 1–29 °C
                        </div>
                      </div>
                      <div class="col-span-2 flex items-center gap-1.5">
                        <input
                          type="number"
                          step="1"
                          min="1"
                          max="100"
                          bind:value={settings.bed_min_temp}
                          class="flex-1 input-premium py-1 text-center text-xs"
                        />
                        <span class="text-slate-500 text-[10px] w-6">°C</span>
                      </div>
                    </div>
                  </div>
                </div>

                <div class="border-b border-slate-800/60"></div>

                <!-- SEKCE: STARTOVNÍ POZICE -->
                <div class="flex flex-col gap-3">
                  <div class="flex items-center gap-2">
                    <span class="w-2 h-2 rounded-full bg-emerald-500"></span>
                    <span class="text-xs font-bold text-slate-300 uppercase tracking-wider"
                      >Startovní pozice tisku</span
                    >
                  </div>
                  <div class="grid grid-cols-1 gap-2.5 text-xs pl-4">
                    <div class="grid grid-cols-5 items-center gap-3">
                      <div class="col-span-3">
                        <div class="text-slate-300 font-medium">Offset X</div>
                        <div class="text-[10px] text-slate-500 mt-0.5">
                          Posunutí výchozí pozice od nuly osy X
                        </div>
                      </div>
                      <div class="col-span-2 flex items-center gap-1.5">
                        <input
                          type="number"
                          step="1"
                          bind:value={settings.start_offset_x}
                          class="flex-1 input-premium py-1 text-center text-xs"
                        />
                        <span class="text-slate-500 text-[10px] w-6">mm</span>
                      </div>
                    </div>
                    <div class="grid grid-cols-5 items-center gap-3">
                      <div class="col-span-3">
                        <div class="text-slate-300 font-medium">Offset Y</div>
                        <div class="text-[10px] text-slate-500 mt-0.5">
                          Posunutí výchozí pozice od nuly osy Y
                        </div>
                      </div>
                      <div class="col-span-2 flex items-center gap-1.5">
                        <input
                          type="number"
                          step="1"
                          bind:value={settings.start_offset_y}
                          class="flex-1 input-premium py-1 text-center text-xs"
                        />
                        <span class="text-slate-500 text-[10px] w-6">mm</span>
                      </div>
                    </div>
                    <div class="grid grid-cols-5 items-center gap-3">
                      <div class="col-span-3">
                        <div class="text-slate-300 font-medium">Výška přesunu (cestovní Z)</div>
                        <div class="text-[10px] text-slate-500 mt-0.5">
                          Výška zdvihu trysky při přesunu mezi tisky
                        </div>
                      </div>
                      <div class="col-span-2 flex items-center gap-1.5">
                        <input
                          type="number"
                          step="0.5"
                          bind:value={settings.block_height}
                          class="flex-1 input-premium py-1 text-center text-xs"
                        />
                        <span class="text-slate-500 text-[10px] w-6">mm</span>
                      </div>
                    </div>
                    <div class="grid grid-cols-5 items-center gap-3">
                      <div class="col-span-3">
                        <div class="text-slate-300 font-medium">Mezera mezi sklíčky</div>
                        <div class="text-[10px] text-slate-500 mt-0.5">
                          Vzdálenost mezi sklíčky při multiplexním tisku
                        </div>
                      </div>
                      <div class="col-span-2 flex items-center gap-1.5">
                        <input
                          type="number"
                          step="0.5"
                          bind:value={settings.multi_spacing}
                          class="flex-1 input-premium py-1 text-center text-xs"
                        />
                        <span class="text-slate-500 text-[10px] w-6">mm</span>
                      </div>
                    </div>
                  </div>
                  <button
                    on:click={() => (activeTab = "leveling")}
                    class="flex items-center gap-1.5 text-[10px] text-labaccent hover:text-blue-300 transition-colors mt-0.5 w-fit"
                  >
                    <Target class="w-3 h-3" /> Upravit kalibrační body bed levelingu →
                  </button>
                </div>

                <div class="border-b border-slate-800/60"></div>

                <!-- SEKCE: KALIBRACE -->
                <div class="flex flex-col gap-3">
                  <div class="flex items-center gap-2">
                    <span class="w-2 h-2 rounded-full bg-purple-500"></span>
                    <span class="text-xs font-bold text-slate-300 uppercase tracking-wider"
                      >Kalibrace extruze</span
                    >
                  </div>
                  <div class="grid grid-cols-1 gap-2.5 text-xs pl-4">
                    <div class="grid grid-cols-5 items-center gap-3">
                      <div class="col-span-3">
                        <div class="text-slate-300 font-medium">Kalibrační faktor extruze</div>
                        <div class="text-[10px] text-slate-500 mt-0.5">
                          Konstanta přepočtu kroků motoru na objemový průtok (µl/krok)
                        </div>
                      </div>
                      <div class="col-span-2 flex items-center gap-1.5">
                        <input
                          type="number"
                          step="0.0001"
                          bind:value={settings.calibration_factor}
                          class="flex-1 input-premium py-1 text-center text-xs"
                        />
                        <span class="text-slate-500 text-[10px] w-12">krok/µl</span>
                      </div>
                    </div>
                    <div class="grid grid-cols-5 items-center gap-3">
                      <div class="col-span-3">
                        <div class="text-slate-300 font-medium">Výška kalibračního objektu</div>
                        <div class="text-[10px] text-slate-500 mt-0.5">
                          Výška měrky / papíru použitého při kalibraci Z. Předvyplní se v
                          kalibračním dialogu před tiskem
                        </div>
                      </div>
                      <div class="col-span-2 flex items-center gap-1.5">
                        <input
                          type="number"
                          step="0.01"
                          bind:value={settings.calibration_object_height}
                          class="flex-1 input-premium py-1 text-center text-xs"
                        />
                        <span class="text-slate-500 text-[10px] w-6">mm</span>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            {/if}

            <!-- ═══ 4. BED LEVELING ═══ -->
            {#if activeTab === "leveling"}
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
                        on:click={() => (showPindaWarning = false)}
                        class="px-3 py-1 text-[11px] rounded border border-slate-600 bg-slate-800/60 text-slate-300 hover:border-slate-400 transition-colors"
                      >
                        Zrušit
                      </button>
                      <button
                        on:click={confirmStartTest}
                        class="px-3 py-1 text-[11px] font-bold rounded bg-yellow-500/80 hover:bg-yellow-500 text-slate-900 transition-colors"
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
                        on:click={stopTest}
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
                        on:click={testMoveToEdited}
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
                        on:click={testNext}
                        disabled={testMoving}
                        class="px-4 py-1 text-[11px] font-bold rounded transition-colors flex items-center gap-1.5
                               {testMoving
                          ? 'opacity-40 cursor-not-allowed bg-slate-800 text-slate-500'
                          : 'bg-labaccent hover:bg-blue-600 text-white shadow-sm shadow-blue-500/20'}"
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
                          on:click={addLevelingPoint}
                          disabled={testRunning}
                          class="bg-labaccent hover:bg-blue-600 text-white text-[10px] font-bold px-2 py-0.5 rounded flex items-center gap-1 transition-colors disabled:opacity-40"
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
                                on:click={() => deleteLevelingPoint(i)}
                                disabled={testRunning}
                                class="col-span-1 p-1 text-slate-500 hover:text-labred hover:bg-labred/10 rounded flex items-center justify-center transition-colors disabled:opacity-30"
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
                      on:click={testRunning ? stopTest : startTest}
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
            {/if}

            <!-- ═══ 6. G-KÓDY ═══ -->
            {#if activeTab === "gcode"}
              <div class="flex flex-col gap-4 text-xs h-full">
                <span class="font-bold text-xs text-slate-300 pb-1 border-b border-slate-800"
                  >Inicializační a ukončovací sekvence G-kódu</span
                >
                <div class="flex flex-col gap-1">
                  <span class="text-slate-400 font-bold uppercase text-[9px]">Start G-code</span>
                  <textarea
                    bind:value={settings.start_gcode}
                    rows="4"
                    class="input-premium font-mono text-[10px] w-full resize-y min-h-[80px]"
                  ></textarea>
                </div>
                <div class="flex flex-col gap-1">
                  <span class="text-slate-400 font-bold uppercase text-[9px]">End G-code</span>
                  <textarea
                    bind:value={settings.end_gcode}
                    rows="4"
                    class="input-premium font-mono text-[10px] w-full resize-y min-h-[80px]"
                  ></textarea>
                </div>
                <div class="flex flex-col gap-1">
                  <span class="text-slate-400 font-bold uppercase text-[9px]"
                    >Loop Start G-code</span
                  >
                  <textarea
                    bind:value={settings.loop_start_gcode}
                    rows="2"
                    class="input-premium font-mono text-[10px] w-full resize-y min-h-[50px]"
                  ></textarea>
                </div>
                <div class="flex flex-col gap-1">
                  <span class="text-slate-400 font-bold uppercase text-[9px]">Loop End G-code</span>
                  <textarea
                    bind:value={settings.loop_end_gcode}
                    rows="2"
                    class="input-premium font-mono text-[10px] w-full resize-y min-h-[50px]"
                  ></textarea>
                </div>
              </div>
            {/if}

            <!-- ═══ 7. PROGRAM ═══ -->
            {#if activeTab === "program"}
              <div class="flex flex-col gap-6">
                <span class="font-bold text-xs text-slate-300 pb-1 border-b border-slate-800"
                  >Nastavení aplikace</span
                >

                <!-- THEME -->
                <div class="flex flex-col gap-3">
                  <span class="text-xs font-bold text-slate-400 uppercase tracking-wider"
                    >Barevný motiv</span
                  >
                  <div class="grid grid-cols-2 gap-3">
                    <!-- DARK -->
                    <button
                      on:click={() => applyTheme("dark")}
                      class="relative flex flex-col gap-3 p-4 rounded-xl border-2 transition-all cursor-pointer
                             {currentTheme === 'dark'
                        ? 'border-labaccent bg-labaccent/10 shadow-lg shadow-labaccent/10'
                        : 'border-slate-700 bg-slate-900/40 hover:border-slate-600'}"
                    >
                      {#if currentTheme === "dark"}
                        <span class="absolute top-2 right-2 w-2 h-2 rounded-full bg-labaccent"
                        ></span>
                      {/if}
                      <!-- preview -->
                      <div
                        class="w-full h-16 rounded-lg overflow-hidden border border-slate-700 flex flex-col"
                      >
                        <div class="h-4 bg-slate-950 flex items-center gap-1 px-2">
                          <span class="w-1.5 h-1.5 rounded-full bg-red-500"></span>
                          <span class="w-1.5 h-1.5 rounded-full bg-yellow-500"></span>
                          <span class="w-1.5 h-1.5 rounded-full bg-green-500"></span>
                        </div>
                        <div class="flex-1 bg-slate-900 flex gap-1 p-1">
                          <div class="w-8 bg-slate-800 rounded"></div>
                          <div class="flex-1 bg-slate-950/50 rounded"></div>
                          <div class="w-8 bg-slate-800 rounded"></div>
                        </div>
                      </div>
                      <div class="flex items-center gap-2">
                        <Moon class="w-4 h-4 text-slate-300" />
                        <div>
                          <p class="text-xs font-bold text-slate-200">Tmavý</p>
                          <p class="text-[10px] text-slate-500">Výchozí laboratorní motiv</p>
                        </div>
                      </div>
                    </button>

                    <!-- LIGHT -->
                    <button
                      on:click={() => applyTheme("light")}
                      class="relative flex flex-col gap-3 p-4 rounded-xl border-2 transition-all cursor-pointer
                             {currentTheme === 'light'
                        ? 'border-labaccent bg-labaccent/10 shadow-lg shadow-labaccent/10'
                        : 'border-slate-700 bg-slate-900/40 hover:border-slate-600'}"
                    >
                      {#if currentTheme === "light"}
                        <span class="absolute top-2 right-2 w-2 h-2 rounded-full bg-labaccent"
                        ></span>
                      {/if}
                      <!-- preview -->
                      <div
                        class="w-full h-16 rounded-lg overflow-hidden border border-slate-300 flex flex-col"
                      >
                        <div class="h-4 bg-gray-100 flex items-center gap-1 px-2">
                          <span class="w-1.5 h-1.5 rounded-full bg-red-500"></span>
                          <span class="w-1.5 h-1.5 rounded-full bg-yellow-500"></span>
                          <span class="w-1.5 h-1.5 rounded-full bg-green-500"></span>
                        </div>
                        <div class="flex-1 bg-white flex gap-1 p-1">
                          <div class="w-8 bg-gray-100 rounded"></div>
                          <div class="flex-1 bg-gray-50 rounded"></div>
                          <div class="w-8 bg-gray-100 rounded"></div>
                        </div>
                      </div>
                      <div class="flex items-center gap-2">
                        <Sun class="w-4 h-4 text-yellow-400" />
                        <div>
                          <p class="text-xs font-bold text-slate-200">Světlý</p>
                          <p class="text-[10px] text-slate-500">Čistý bílý motiv</p>
                        </div>
                      </div>
                    </button>
                  </div>
                  <p class="text-[10px] text-slate-500">
                    Motiv se projeví okamžitě a zapamatuje si pro příští spuštění.
                  </p>
                </div>
              </div>
            {/if}
          </div>
          <!-- end TAB CONTENT -->
        </div>
        <!-- end flex tabs -->

        <!-- FOOTER -->
        <div class="flex justify-end gap-2 px-5 py-3 border-t border-slate-800 bg-slate-950/20">
          <button
            on:click={close}
            class="bg-slate-900 border border-slate-700 hover:bg-slate-800 text-slate-300 font-bold text-xs px-4 py-2 rounded-lg transition-colors"
          >
            Zrušit
          </button>
          <button
            on:click={save}
            class="bg-labaccent hover:bg-blue-600 text-white font-bold text-xs px-4 py-2 rounded-lg flex items-center gap-1.5 shadow-lg shadow-blue-500/10 transition-colors"
          >
            <Save class="w-4 h-4" /> Uložit a zavřít
          </button>
        </div>
      {/if}
    </div>
  </div>
{/if}
