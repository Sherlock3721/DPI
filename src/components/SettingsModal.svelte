<script lang="ts">
  import { onMount } from "svelte";
  import { get_app_settings, save_app_settings, send_manual_command, auto_connect_printer } from "../lib/tauri";
  import { printerStore } from "../stores/printerStore";
  import { X, Plus, Trash2, Save, GripVertical, ShieldAlert, Cog } from "lucide-svelte";
  import SettingsGCode from "./settings/SettingsGCode.svelte";
  import SettingsProgram from "./settings/SettingsProgram.svelte";
  import SettingsLimits from "./settings/SettingsLimits.svelte";
  import { createEventDispatcher } from "svelte";

  export let isOpen = false;

  const dispatch = createEventDispatcher();
  let activeTab: "nozzles" | "glass" | "liquids" | "limits" | "leveling" | "gcode" | "program" = "nozzles";

  let settings: any = null;
  let nozzleList: { name: string; h: number; d: number; s: number; c: string }[] = [];
  let glassList: { name: string; w: number; h: number; z: number }[] = [];
  let liquidList: {
    name: string; color: string; category: string;
    z_offset: number; z_offset_min: number | null; z_offset_max: number | null;
    extrusion: number; extrusion_min: number | null; extrusion_max: number | null;
    forbidden_nozzles: string[];
    print_speed: number; print_speed_min: number | null; print_speed_max: number | null;
    bed_temp: number; bed_temp_min: number | null; bed_temp_max: number | null;
  }[] = [];
  let expandedLiquidOrigIdx: number | null = null;
  let liquidSortBy: "id" | "name" = "id";
  $: displayLiquidIndices = liquidSortBy === "name"
    ? [...Array(liquidList.length).keys()].sort((a, b) =>
        liquidList[a].name.localeCompare(liquidList[b].name, "cs"))
    : [...Array(liquidList.length).keys()];
  $: liquidCategories = [...new Set(
    liquidList.map(l => l.category?.trim() || "").filter(c => c !== "")
  )].sort((a, b) => a.localeCompare(b, "cs"));
  $: liquidsHaveCategories = liquidList.some(l => (l.category?.trim() || "") !== "");
  $: liquidGroups = (() => {
    const catMap = new Map<string, number[]>();
    const uncat: number[] = [];
    for (const origIdx of displayLiquidIndices) {
      const cat = liquidList[origIdx]?.category?.trim() || "";
      if (cat === "") { uncat.push(origIdx); }
      else { if (!catMap.has(cat)) catMap.set(cat, []); catMap.get(cat)!.push(origIdx); }
    }
    const groups: { category: string; items: { origIdx: number; displayIdx: number }[] }[] = [];
    let flatIdx = 0;
    const catNames = [...catMap.keys()].sort((a, b) => a.localeCompare(b, "cs"));
    for (const cat of catNames) {
      groups.push({ category: cat, items: (catMap.get(cat) || []).map(origIdx => ({ origIdx, displayIdx: flatIdx++ })) });
    }
    if (uncat.length > 0) groups.push({ category: "", items: uncat.map(origIdx => ({ origIdx, displayIdx: flatIdx++ })) });
    return groups;
  })();
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

  // ─── Expert mode (session-only, never persisted) ──────────────────────────
  let expertModeActive = false;
  let showExpertWarning = false;

  function requestExpertMode() {
    if (!expertModeActive) showExpertWarning = true;
  }

  function confirmExpertMode() {
    expertModeActive = true;
    showExpertWarning = false;
  }

  function disableExpertMode() {
    expertModeActive = false;
    if (activeTab === "gcode") activeTab = "program";
  }

  // ─── Theme ────────────────────────────────────────────────────────────────
  type Theme = "dark" | "light";
  let currentTheme: Theme = "dark";

  function applyTheme(t: Theme) {
    currentTheme = t;
    document.documentElement.setAttribute("data-theme", t);
    localStorage.setItem("app-theme", t);
  }

  // ─── Sněžení ──────────────────────────────────────────────────────────────
  function isSnowSeason(): boolean {
    const now = new Date();
    const m = now.getMonth() + 1;
    const d = now.getDate();
    return m === 12 || (m === 11 && d >= 15) || (m === 1 && d <= 30);
  }
  const snowSeason = isSnowSeason();
  let snowDisabled = false;

  onMount(() => {
    const stored = localStorage.getItem("app-theme") as Theme | null;
    applyTheme(stored ?? "dark");
    snowDisabled = localStorage.getItem("disable-snow") === "1";
  });

  // ─── Drag & Drop helper ───────────────────────────────────────────────────
  let dragSrcIndex: number | null = null;
  let dragOverIndex: number | null = null;
  let dragList: "nozzles" | "glass" | "liquids" | null = null;

  function onDragStart(e: DragEvent, list: "nozzles" | "glass" | "liquids", i: number) {
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

  function onDrop(list: "nozzles" | "glass" | "liquids", i: number) {
    if (dragSrcIndex === null || dragList !== list || dragSrcIndex === i) {
      dragSrcIndex = dragOverIndex = null;
      return;
    }
    if (list === "nozzles") {
      const arr = [...nozzleList];
      const [item] = arr.splice(dragSrcIndex, 1);
      arr.splice(i, 0, item);
      nozzleList = arr;
    } else if (list === "glass") {
      const arr = [...glassList];
      const [item] = arr.splice(dragSrcIndex, 1);
      arr.splice(i, 0, item);
      glassList = arr;
    } else if (list === "liquids") {
      const arr = [...liquidList];
      const [item] = arr.splice(dragSrcIndex, 1);
      arr.splice(i, 0, item);
      liquidList = arr;
      expandedLiquidOrigIdx = null;
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

      // Liquids
      liquidList = Object.entries(settings.liquid_defs || {}).map(([name, val]: [string, any]) => ({
        name,
        color: val.color ?? "#3b82f6",
        category: val.category ?? "",
        z_offset: val.z_offset ?? 0.2, z_offset_min: val.z_offset_min ?? null, z_offset_max: val.z_offset_max ?? null,
        extrusion: val.extrusion ?? 5.0, extrusion_min: val.extrusion_min ?? null, extrusion_max: val.extrusion_max ?? null,
        forbidden_nozzles: Array.isArray(val.forbidden_nozzles) ? val.forbidden_nozzles : [],
        print_speed: val.print_speed ?? 1500, print_speed_min: val.print_speed_min ?? null, print_speed_max: val.print_speed_max ?? null,
        bed_temp: val.bed_temp ?? 0, bed_temp_min: val.bed_temp_min ?? null, bed_temp_max: val.bed_temp_max ?? null,
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

  function addLiquid() {
    liquidList = [...liquidList, {
      name: "Nová kapalina", color: "#3b82f6", category: "",
      z_offset: 0.2, z_offset_min: null, z_offset_max: null,
      extrusion: 5.0, extrusion_min: null, extrusion_max: null,
      forbidden_nozzles: [],
      print_speed: 1500, print_speed_min: null, print_speed_max: null,
      bed_temp: 0, bed_temp_min: null, bed_temp_max: null,
    }];
  }
  function deleteLiquid(origIdx: number) {
    if (expandedLiquidOrigIdx === origIdx) expandedLiquidOrigIdx = null;
    else if (expandedLiquidOrigIdx !== null && expandedLiquidOrigIdx > origIdx) expandedLiquidOrigIdx--;
    liquidList = liquidList.filter((_, idx) => idx !== origIdx);
  }
  function toggleLiquidExpand(origIdx: number) {
    expandedLiquidOrigIdx = expandedLiquidOrigIdx === origIdx ? null : origIdx;
  }

  // ─── Nozzle drag-and-drop mezi povolen./zakázanými ────────────────────────
  let nozzleDragSrcList: "allowed" | "forbidden" | null = null;
  let nozzleDragSrcName = "";

  function onNozzleDragStart(e: DragEvent, srcList: "allowed" | "forbidden", name: string) {
    e.stopPropagation();
    if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
    nozzleDragSrcList = srcList;
    nozzleDragSrcName = name;
  }

  function onNozzleDragOver(e: DragEvent) {
    e.preventDefault();
  }

  function onNozzleDrop(e: DragEvent, origIdx: number, targetList: "allowed" | "forbidden") {
    e.preventDefault();
    e.stopPropagation();
    if (!nozzleDragSrcList || nozzleDragSrcList === targetList) {
      nozzleDragSrcList = null;
      return;
    }
    moveLiquidNozzle(origIdx, nozzleDragSrcName, nozzleDragSrcList);
    nozzleDragSrcList = null;
  }

  function onNozzleDragEnd() {
    nozzleDragSrcList = null;
    nozzleDragSrcName = "";
  }

  function moveLiquidNozzle(origIdx: number, nozzleName: string, fromList: "allowed" | "forbidden") {
    if (fromList === "allowed") {
      if (!liquidList[origIdx].forbidden_nozzles.includes(nozzleName)) {
        liquidList[origIdx].forbidden_nozzles = [...liquidList[origIdx].forbidden_nozzles, nozzleName];
        liquidList = liquidList;
      }
    } else {
      liquidList[origIdx].forbidden_nozzles = liquidList[origIdx].forbidden_nozzles.filter((n) => n !== nozzleName);
      liquidList = liquidList;
    }
  }

  function allowedNozzlesFor(origIdx: number) {
    const forbidden = liquidList[origIdx]?.forbidden_nozzles ?? [];
    return nozzleList.filter((n) => !forbidden.includes(n.name));
  }

  function forbiddenNozzlesFor(origIdx: number) {
    const forbidden = liquidList[origIdx]?.forbidden_nozzles ?? [];
    return nozzleList.filter((n) => forbidden.includes(n.name));
  }

  export function openOnTab(tab: typeof activeTab) {
    activeTab = tab;
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

      const liquid_defs: Record<string, any> = {};
      liquidList.forEach((l) => {
        if (l.name.trim()) liquid_defs[l.name.trim()] = {
          color: l.color,
          category: l.category?.trim() || "",
          z_offset: l.z_offset, z_offset_min: l.z_offset_min, z_offset_max: l.z_offset_max,
          extrusion: l.extrusion, extrusion_min: l.extrusion_min, extrusion_max: l.extrusion_max,
          forbidden_nozzles: l.forbidden_nozzles,
          print_speed: l.print_speed, print_speed_min: l.print_speed_min, print_speed_max: l.print_speed_max,
          bed_temp: l.bed_temp, bed_temp_min: l.bed_temp_min, bed_temp_max: l.bed_temp_max,
        };
      });

      settings.nozzle_defs = nozzle_defs;
      settings.sklo_dims = sklo_dims;
      settings.liquid_defs = liquid_defs;
      settings.leveling_points = levelingPoints.map((p) => ({ name: p.name, x: p.x, y: p.y }));

      await save_app_settings(settings);
      localStorage.setItem("disable-snow", snowDisabled ? "1" : "0");
      dispatch("save");
      isOpen = false;
    } catch (e) {
      alert(`Chyba při ukládání nastavení: ${e}`);
    }
  }

  function close() {
    isOpen = false;
  }

  // minBedMaxX/Y jsou nyní počítány uvnitř SettingsLimits.svelte

  // ─── Bed leveling SVG ────────────────────────────────────────────────────
  const LVL_VW = 420;
  const LVL_VH = 330;
  const LVL_ML = 38;
  const LVL_MT = 14;
  const LVL_MR = 10;
  const LVL_MB = 22;

  let lvlScale = 0, lvlBedW = 0, lvlBedH = 0, lvlCircleR = 0;
  let lvlXTicks: number[] = [], lvlYTicks: number[] = [];
  $: {
    lvlScale = Math.min(
      (LVL_VW - LVL_ML - LVL_MR) / (settings?.bed_max_x ?? 250),
      (LVL_VH - LVL_MT - LVL_MB) / (settings?.bed_max_y ?? 210)
    );
    lvlBedW = (settings?.bed_max_x ?? 250) * lvlScale;
    lvlBedH = (settings?.bed_max_y ?? 210) * lvlScale;
    lvlCircleR = ((settings?.leveling_circle_diameter ?? 8) / 2) * lvlScale;
    lvlXTicks = Array.from({ length: Math.floor((settings?.bed_max_x ?? 250) / 50) + 1 }, (_, i) => i * 50);
    lvlYTicks = Array.from({ length: Math.floor((settings?.bed_max_y ?? 210) / 50) + 1 }, (_, i) => i * 50);
  }

  async function restoreDefaults() {
    const confirmed = await import("@tauri-apps/plugin-dialog").then((m) =>
      m.ask(
        "Opravdu chcete obnovit výchozí nastavení?\n\nPřepíšou se limitace tiskárny a inicializační G-kódy. Definice trysek a substrátů zůstanou zachovány.",
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
    settings.leveling_circle_diameter = 8.0;
    settings.leveling_points = DEFAULT_LEVELING_POINTS;
    levelingPoints = DEFAULT_LEVELING_POINTS.map((p) => ({ ...p }));
  }
</script>

{#if showExpertWarning}
  <div class="fixed inset-0 bg-black/80 backdrop-blur-sm flex items-center justify-center z-[60] p-4">
    <div class="glass-panel w-full max-w-sm rounded-xl border border-labred/40 shadow-2xl shadow-labred/10 p-6 flex flex-col gap-5">
      <div class="flex items-center gap-3">
        <div class="w-10 h-10 rounded-full bg-labred/20 border border-labred/40 flex items-center justify-center shrink-0">
          <ShieldAlert class="w-5 h-5 text-labred" />
        </div>
        <div>
          <h3 class="text-sm font-bold text-slate-200">Aktivovat expertní režim?</h3>
          <p class="text-[10px] text-slate-500 mt-0.5">Tato akce zpřístupní nebezpečná nastavení</p>
        </div>
      </div>
      <div class="flex flex-col gap-2 bg-labred/8 border border-labred/25 rounded-lg p-3">
        <p class="text-[11px] text-labred/90 font-semibold">Upozornění — čtěte pozorně:</p>
        <ul class="text-[11px] text-slate-400 flex flex-col gap-1 list-disc list-inside">
          <li>Úprava G-kódů může způsobit kolizi nebo poškození tiskárny</li>
          <li>Chybné příkazy mohou způsobit nekontrolovaný pohyb os</li>
          <li>Změny provádějte pouze pokud víte, co děláte</li>
          <li>Expertní režim se automaticky deaktivuje po zavření aplikace</li>
        </ul>
      </div>
      <div class="flex gap-2 justify-end">
        <button
          on:click={() => (showExpertWarning = false)}
          class="px-4 py-2 text-xs font-bold rounded-lg border border-slate-600 bg-slate-800 text-slate-300 hover:bg-slate-700 transition-colors"
        >
          Zrušit
        </button>
        <button
          on:click={confirmExpertMode}
          class="px-4 py-2 text-xs font-bold rounded-lg bg-labred/80 hover:bg-labred text-white transition-colors flex items-center gap-1.5"
        >
          <ShieldAlert class="w-3.5 h-3.5" /> Rozumím, aktivovat
        </button>
      </div>
    </div>
  </div>
{/if}

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
            class="w-36 bg-slate-950/20 border-r border-slate-800 flex flex-col p-2 gap-1 relative"
          >
            <div class="flex-1 flex flex-col gap-1">
              <button
                on:click={() => (activeTab = "nozzles")}
                class="px-3 py-2 rounded text-left text-xs font-bold transition-colors {activeTab === 'nozzles' ? 'bg-labaccent text-white' : 'text-slate-400 hover:bg-slate-900/40 hover:text-slate-200'}"
              >Tryska</button>
              <button
                on:click={() => (activeTab = "glass")}
                class="px-3 py-2 rounded text-left text-xs font-bold transition-colors {activeTab === 'glass' ? 'bg-labaccent text-white' : 'text-slate-400 hover:bg-slate-900/40 hover:text-slate-200'}"
              >Skla</button>
              <button
                on:click={() => (activeTab = "liquids")}
                class="px-3 py-2 rounded text-left text-xs font-bold transition-colors {activeTab === 'liquids' ? 'bg-labaccent text-white' : 'text-slate-400 hover:bg-slate-900/40 hover:text-slate-200'}"
              >Kapaliny</button>
              <button
                on:click={() => (activeTab = "limits")}
                class="px-3 py-2 rounded text-left text-xs font-bold transition-colors {activeTab === 'limits' || activeTab === 'leveling' ? 'bg-labaccent text-white' : 'text-slate-400 hover:bg-slate-900/40 hover:text-slate-200'}"
              >Tiskárna</button>
              {#if expertModeActive}
                <button
                  on:click={() => (activeTab = "gcode")}
                  class="px-3 py-2 rounded text-left text-xs font-bold transition-colors flex items-center gap-1.5 {activeTab === 'gcode' ? 'bg-labred/80 text-white' : 'text-labred/70 hover:bg-labred/20 hover:text-labred'}"
                >
                  <ShieldAlert class="w-3 h-3 shrink-0" />G-kódy
                </button>
              {/if}
              <button
                on:click={() => (activeTab = "program")}
                class="px-3 py-2 rounded text-left text-xs font-bold transition-colors {activeTab === 'program' ? 'bg-labaccent text-white' : 'text-slate-400 hover:bg-slate-900/40 hover:text-slate-200'}"
              >Program</button>
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
                    <span class="col-span-4 text-left pl-1">Název substrátu</span>
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

            <!-- ═══ 3. KAPALINY ═══ -->
            {#if activeTab === "liquids"}
              <div class="flex flex-col gap-3">
                <!-- Header: název + sort + přidat -->
                <div class="flex justify-between items-center pb-2 border-b border-slate-800">
                  <div class="flex items-center gap-3">
                    <span class="font-bold text-xs text-slate-300">Presety kapalin</span>
                    <div class="flex items-center gap-0.5 bg-slate-900/60 border border-slate-800 rounded p-0.5">
                      <button
                        on:click={() => (liquidSortBy = "id")}
                        class="px-2 py-0.5 text-[9px] font-bold rounded transition-colors {liquidSortBy === 'id' ? 'bg-labaccent text-white' : 'text-slate-400 hover:text-slate-200'}"
                      >ID</button>
                      <button
                        on:click={() => (liquidSortBy = "name")}
                        class="px-2 py-0.5 text-[9px] font-bold rounded transition-colors {liquidSortBy === 'name' ? 'bg-labaccent text-white' : 'text-slate-400 hover:text-slate-200'}"
                      >A–Z</button>
                    </div>
                  </div>
                  <button
                    on:click={addLiquid}
                    class="bg-labaccent hover:bg-blue-600 text-white text-[10px] font-bold px-2 py-1 rounded flex items-center gap-1 transition-colors"
                  >
                    <Plus class="w-3 h-3" /> Přidat kapalinu
                  </button>
                </div>

                <!-- Tabulka kapalin -->
                <div class="flex flex-col border border-slate-800 rounded-lg overflow-hidden bg-slate-900/20">
                  <!-- hlavička tabulky -->
                  <div class="grid grid-cols-12 bg-slate-950/50 p-2 font-bold text-[10px] text-slate-400 text-center border-b border-slate-800">
                    <span class="col-span-1"></span>
                    <span class="col-span-1">Barva</span>
                    <span class="col-span-5 text-left pl-1">Název kapaliny</span>
                    <span class="col-span-4">Parametry</span>
                    <span class="col-span-1">Akce</span>
                  </div>

                  <!-- datalist pro autocomplete kategorií -->
                  <datalist id="liquid-categories-list">
                    {#each liquidCategories as cat}
                      <option value={cat} />
                    {/each}
                  </datalist>

                  <div class="flex flex-col">
                    {#if liquidGroups.length === 0}
                      <div class="p-6 text-center text-slate-500 text-xs">
                        Žádné kapaliny nejsou definovány.
                      </div>
                    {:else}
                      {#each liquidGroups as group}
                        <!-- kategorie hlavička -->
                        {#if liquidsHaveCategories}
                          <div class="flex items-center gap-2 px-3 py-1.5 bg-slate-900/70 border-b border-slate-700/80 border-t border-t-slate-700/40">
                            <span class="w-1.5 h-1.5 rounded-full bg-labaccent flex-shrink-0"></span>
                            <span class="text-[9px] font-bold uppercase tracking-widest text-slate-400">
                              {group.category || "Bez kategorie"}
                            </span>
                            <span class="ml-auto text-[9px] text-slate-600">{group.items.length}</span>
                          </div>
                        {/if}
                        <div class="flex flex-col divide-y divide-slate-800">
                        {#each group.items as { origIdx, displayIdx }}
                          {@const liquid = liquidList[origIdx]}
                          {@const isExpanded = expandedLiquidOrigIdx === origIdx}
                          <!-- svelte-ignore a11y-no-static-element-interactions -->
                          <div class="flex flex-col">
                            <!-- hlavní řádek -->
                            <div
                              class="grid grid-cols-12 p-2 items-center text-center gap-1 text-xs transition-colors
                                     {dragOverIndex === displayIdx && dragList === 'liquids' && liquidSortBy === 'id' && !liquidsHaveCategories
                                       ? 'bg-labaccent/10 border-t-2 border-labaccent'
                                       : 'hover:bg-slate-900/30'}"
                              draggable={liquidSortBy === "id" && !liquidsHaveCategories}
                              on:dragstart={(e) => liquidSortBy === "id" && !liquidsHaveCategories && onDragStart(e, "liquids", displayIdx)}
                              on:dragover={(e) => onDragOver(e, displayIdx)}
                              on:drop={() => onDrop("liquids", displayIdx)}
                              on:dragend={onDragEnd}
                            >
                              <!-- drag handle -->
                              <div class="col-span-1 flex justify-center {liquidSortBy === 'id' && !liquidsHaveCategories ? 'text-slate-600 hover:text-slate-400 cursor-grab active:cursor-grabbing' : 'text-slate-800 cursor-default'}">
                                <GripVertical class="w-3.5 h-3.5" />
                              </div>
                              <!-- barevný kroužek -->
                              <div class="col-span-1 flex justify-center items-center">
                                <label class="relative w-5 h-5 cursor-pointer block">
                                  <span
                                    class="block w-5 h-5 rounded-full border-2 border-slate-600 shadow-inner"
                                    style="background-color: {liquid.color}"
                                  ></span>
                                  <input
                                    type="color"
                                    bind:value={liquidList[origIdx].color}
                                    class="absolute inset-0 opacity-0 w-full h-full cursor-pointer"
                                  />
                                </label>
                              </div>
                              <!-- název -->
                              <input
                                type="text"
                                bind:value={liquidList[origIdx].name}
                                class="col-span-5 input-premium py-0.5 text-left text-[11px]"
                              />
                              <!-- parametry tlačítko (ozubené kolečko) -->
                              <button
                                on:click={() => toggleLiquidExpand(origIdx)}
                                title="Zobrazit / skrýt parametry kapaliny"
                                class="col-span-4 flex items-center justify-center gap-1 py-0.5 rounded transition-colors
                                       {isExpanded
                                         ? 'text-labaccent bg-labaccent/10 border border-labaccent/30'
                                         : 'text-slate-500 hover:text-slate-300 border border-transparent hover:border-slate-700'}"
                              >
                                <Cog class="w-3.5 h-3.5" />
                              </button>
                              <!-- smazat -->
                              <button
                                on:click={() => deleteLiquid(origIdx)}
                                class="col-span-1 p-1 text-slate-500 hover:text-labred hover:bg-labred/10 rounded flex items-center justify-center transition-colors"
                              >
                                <Trash2 class="w-3.5 h-3.5" />
                              </button>
                            </div>

                            <!-- rozbalené parametry kapaliny -->
                            {#if isExpanded}
                              <div class="bg-slate-950/50 border-t border-labaccent/20 px-4 py-2.5 text-[11px]">
                                <!-- hlavička tabulky parametrů -->
                                <div class="grid grid-cols-[1fr_5rem_5rem_5rem] gap-x-2 text-[9px] font-bold text-slate-500 uppercase tracking-wide pb-1.5 mb-1 border-b border-slate-800">
                                  <span>Parametr</span>
                                  <span class="text-center">Hodnota</span>
                                  <span class="text-center">Min.</span>
                                  <span class="text-center">Max.</span>
                                </div>

                                <!-- Kategorie -->
                                <div class="grid grid-cols-[1fr_auto] gap-x-2 items-center py-1 border-b border-slate-900/70">
                                  <span class="text-slate-400">Kategorie</span>
                                  <input
                                    type="text"
                                    list="liquid-categories-list"
                                    placeholder="Bez kategorie"
                                    value={liquid.category ?? ""}
                                    on:change={(e) => { liquidList[origIdx].category = e.currentTarget.value; liquidList = liquidList; }}
                                    class="input-premium py-0.5 text-left w-40 placeholder-slate-700"
                                  />
                                </div>

                            <!-- Výška trysky -->
                            <div class="grid grid-cols-[1fr_5rem_5rem_5rem] gap-x-2 items-center py-1 border-b border-slate-900/70">
                              <span class="text-slate-400">Výška trysky <span class="text-slate-600 text-[10px]">mm</span></span>
                              <input type="number" step="0.05"
                                value={liquid.z_offset}
                                on:change={(e) => { liquidList[origIdx].z_offset = +e.currentTarget.value; liquidList = liquidList; }}
                                class="input-premium py-0.5 text-center" />
                              <input type="number" step="0.05" placeholder="—"
                                value={liquid.z_offset_min ?? ""}
                                on:change={(e) => { const v = e.currentTarget.value; liquidList[origIdx].z_offset_min = v === "" ? null : +v; liquidList = liquidList; }}
                                class="input-premium py-0.5 text-center placeholder-slate-700" />
                              <input type="number" step="0.05" placeholder="—"
                                value={liquid.z_offset_max ?? ""}
                                on:change={(e) => { const v = e.currentTarget.value; liquidList[origIdx].z_offset_max = v === "" ? null : +v; liquidList = liquidList; }}
                                class="input-premium py-0.5 text-center placeholder-slate-700" />
                            </div>

                            <!-- Extruze -->
                            <div class="grid grid-cols-[1fr_5rem_5rem_5rem] gap-x-2 items-center py-1 border-b border-slate-900/70">
                              <span class="text-slate-400">Extruze <span class="text-slate-600 text-[10px]">nl/mm</span></span>
                              <input type="number" step="0.1" min="0"
                                value={liquid.extrusion}
                                on:change={(e) => { liquidList[origIdx].extrusion = +e.currentTarget.value; liquidList = liquidList; }}
                                class="input-premium py-0.5 text-center" />
                              <input type="number" step="0.1" placeholder="—"
                                value={liquid.extrusion_min ?? ""}
                                on:change={(e) => { const v = e.currentTarget.value; liquidList[origIdx].extrusion_min = v === "" ? null : +v; liquidList = liquidList; }}
                                class="input-premium py-0.5 text-center placeholder-slate-700" />
                              <input type="number" step="0.1" placeholder="—"
                                value={liquid.extrusion_max ?? ""}
                                on:change={(e) => { const v = e.currentTarget.value; liquidList[origIdx].extrusion_max = v === "" ? null : +v; liquidList = liquidList; }}
                                class="input-premium py-0.5 text-center placeholder-slate-700" />
                            </div>

                            <!-- Povolené trysky — dual DnD list -->
                            <div class="py-2 border-b border-slate-900/70">
                              <span class="text-slate-400 text-[10px] font-semibold block mb-1.5">Povolené trysky</span>
                              <div class="flex gap-2">

                                <!-- ── POVOLENÉ ── -->
                                <!-- svelte-ignore a11y-no-static-element-interactions -->
                                <div
                                  class="flex-1 min-h-[40px] rounded border p-1 flex flex-col gap-0.5 transition-colors
                                         {nozzleDragSrcList === 'forbidden' ? 'border-labaccent/60 bg-labaccent/5' : 'border-slate-700/50 bg-slate-900/30'}"
                                  on:dragover={onNozzleDragOver}
                                  on:drop={(e) => onNozzleDrop(e, origIdx, "allowed")}
                                >
                                  <div class="text-[8px] font-bold text-slate-500 uppercase tracking-wide px-0.5 pb-0.5 border-b border-slate-800 mb-0.5 shrink-0">
                                    Povolené
                                  </div>
                                  {#each allowedNozzlesFor(origIdx) as n (n.name)}
                                    <!-- svelte-ignore a11y-no-static-element-interactions -->
                                    <div
                                      draggable="true"
                                      title="Přetáhněte nebo dvakrát klikněte pro přesun"
                                      on:dragstart={(e) => onNozzleDragStart(e, "allowed", n.name)}
                                      on:dragend={onNozzleDragEnd}
                                      on:dragover={(e) => e.preventDefault()}
                                      on:drop={(e) => { e.stopPropagation(); onNozzleDrop(e, origIdx, "allowed"); }}
                                      on:dblclick={() => moveLiquidNozzle(origIdx, n.name, "allowed")}
                                      class="flex items-center gap-1.5 px-1.5 py-0.5 rounded text-[11px] text-slate-300 cursor-grab select-none
                                             hover:bg-slate-800/60 active:opacity-60 transition-colors
                                             {nozzleDragSrcList === 'allowed' && nozzleDragSrcName === n.name ? 'opacity-30' : ''}"
                                    >
                                      <span class="w-2.5 h-2.5 rounded-full shrink-0 border border-slate-600" style="background-color: {n.c}"></span>
                                      <span class="truncate flex-1">{n.name}</span>
                                    </div>
                                  {:else}
                                    <div class="text-[10px] text-slate-700 px-1 py-0.5 italic">Žádné</div>
                                  {/each}
                                </div>

                                <!-- separator -->
                                <div class="flex items-center text-slate-600 text-sm select-none shrink-0">⇄</div>

                                <!-- ── ZAKÁZANÉ ── -->
                                <!-- svelte-ignore a11y-no-static-element-interactions -->
                                <div
                                  class="flex-1 min-h-[40px] rounded border p-1 flex flex-col gap-0.5 transition-colors
                                         {nozzleDragSrcList === 'allowed' ? 'border-labred/40 bg-labred/5' : 'border-slate-700/50 bg-slate-900/30'}"
                                  on:dragover={onNozzleDragOver}
                                  on:drop={(e) => onNozzleDrop(e, origIdx, "forbidden")}
                                >
                                  <div class="text-[8px] font-bold text-slate-500 uppercase tracking-wide px-0.5 pb-0.5 border-b border-slate-800 mb-0.5 shrink-0">
                                    Zakázané
                                  </div>
                                  {#each forbiddenNozzlesFor(origIdx) as n (n.name)}
                                    <!-- svelte-ignore a11y-no-static-element-interactions -->
                                    <div
                                      draggable="true"
                                      title="Přetáhněte nebo dvakrát klikněte pro přesun"
                                      on:dragstart={(e) => onNozzleDragStart(e, "forbidden", n.name)}
                                      on:dragend={onNozzleDragEnd}
                                      on:dragover={(e) => e.preventDefault()}
                                      on:drop={(e) => { e.stopPropagation(); onNozzleDrop(e, origIdx, "forbidden"); }}
                                      on:dblclick={() => moveLiquidNozzle(origIdx, n.name, "forbidden")}
                                      class="flex items-center gap-1.5 px-1.5 py-0.5 rounded text-[11px] text-slate-400 cursor-grab select-none
                                             hover:bg-slate-800/60 active:opacity-60 transition-colors
                                             {nozzleDragSrcList === 'forbidden' && nozzleDragSrcName === n.name ? 'opacity-30' : ''}"
                                    >
                                      <span class="w-2.5 h-2.5 rounded-full shrink-0 border border-slate-600 opacity-50" style="background-color: {n.c}"></span>
                                      <span class="truncate flex-1 line-through opacity-60">{n.name}</span>
                                    </div>
                                  {:else}
                                    <div class="text-[10px] text-slate-700 px-1 py-0.5 italic">Žádné</div>
                                  {/each}
                                </div>

                              </div>
                              <p class="text-[9px] text-slate-600 mt-1.5">Přetáhněte trysku nebo <strong class="text-slate-500">dvakrát klikněte</strong> pro přesun. Zakázané trysky se nezobrazí v nabídce.</p>
                            </div>

                            <!-- Rychlost tisku -->
                            <div class="grid grid-cols-[1fr_5rem_5rem_5rem] gap-x-2 items-center py-1 border-b border-slate-900/70">
                              <span class="text-slate-400">Rychlost tisku <span class="text-slate-600 text-[10px]">mm/min</span></span>
                              <input type="number" step="50" min="0"
                                value={liquid.print_speed}
                                on:change={(e) => { liquidList[origIdx].print_speed = +e.currentTarget.value; liquidList = liquidList; }}
                                class="input-premium py-0.5 text-center" />
                              <input type="number" step="50" placeholder="—"
                                value={liquid.print_speed_min ?? ""}
                                on:change={(e) => { const v = e.currentTarget.value; liquidList[origIdx].print_speed_min = v === "" ? null : +v; liquidList = liquidList; }}
                                class="input-premium py-0.5 text-center placeholder-slate-700" />
                              <input type="number" step="50" placeholder="—"
                                value={liquid.print_speed_max ?? ""}
                                on:change={(e) => { const v = e.currentTarget.value; liquidList[origIdx].print_speed_max = v === "" ? null : +v; liquidList = liquidList; }}
                                class="input-premium py-0.5 text-center placeholder-slate-700" />
                            </div>

                            <!-- Výhřev podložky -->
                            <div class="grid grid-cols-[1fr_5rem_5rem_5rem] gap-x-2 items-center py-1">
                              <span class="text-slate-400">Výhřev podložky <span class="text-slate-600 text-[10px]">°C</span></span>
                              <input type="number" step="5" min="0"
                                value={liquid.bed_temp}
                                on:change={(e) => { liquidList[origIdx].bed_temp = +e.currentTarget.value; liquidList = liquidList; }}
                                class="input-premium py-0.5 text-center" />
                              <input type="number" step="5" placeholder="—"
                                value={liquid.bed_temp_min ?? ""}
                                on:change={(e) => { const v = e.currentTarget.value; liquidList[origIdx].bed_temp_min = v === "" ? null : +v; liquidList = liquidList; }}
                                class="input-premium py-0.5 text-center placeholder-slate-700" />
                              <input type="number" step="5" placeholder="—"
                                value={liquid.bed_temp_max ?? ""}
                                on:change={(e) => { const v = e.currentTarget.value; liquidList[origIdx].bed_temp_max = v === "" ? null : +v; liquidList = liquidList; }}
                                class="input-premium py-0.5 text-center placeholder-slate-700" />
                            </div>
                          </div>
                        {/if}
                          </div>
                        {/each}
                        </div>
                      {/each}
                    {/if}
                  </div>
                </div>

                <p class="text-[10px] text-slate-500 flex items-center gap-1">
                  <GripVertical class="w-3 h-3" /> {liquidsHaveCategories ? "Kapaliny jsou seskupeny dle kategorie." : "Přetáhněte řádky pro změnu pořadí (ID)."} Kliknutím na <Cog class="w-3 h-3 inline" /> zobrazíte parametry kapaliny — Min./Max. hodnoty jsou nadřazené globálním limitům.
                </p>
              </div>
            {/if}

            <!-- ═══ 4. TISKÁRNA ═══ -->
            {#if activeTab === "limits"}
              <SettingsLimits
                bind:settings
                {glassList}
                onGoToLeveling={() => (activeTab = "leveling")}
              />
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
              <SettingsGCode bind:settings />
            {/if}

            <!-- ═══ 7. PROGRAM ═══ -->
            {#if activeTab === "program"}
              <SettingsProgram
                {expertModeActive}
                {snowSeason}
                bind:snowDisabled
                {currentTheme}
                onRequestExpertMode={requestExpertMode}
                onDisableExpertMode={disableExpertMode}
                onApplyTheme={applyTheme}
              />
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
