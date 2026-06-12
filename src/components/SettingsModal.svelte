<script lang="ts">
  import { run } from 'svelte/legacy';

  import { get_app_settings, save_app_settings } from "../lib/tauri";
  import { settingsStore } from "../stores/settingsStore";
  import { X, Plus, Trash2, Save, GripVertical, ShieldAlert } from "lucide-svelte";
  import SettingsGCode from "./settings/SettingsGCode.svelte";
  import SettingsProgram from "./settings/SettingsProgram.svelte";
  import SettingsLimits from "./settings/SettingsLimits.svelte";
  import SettingsLiquids from "./settings/SettingsLiquids.svelte";
  import SettingsLeveling from "./settings/SettingsLeveling.svelte";
  import { createEventDispatcher } from "svelte";

  interface Props {
    isOpen?: boolean;
  }

  let { isOpen = $bindable(false) }: Props = $props();

  const dispatch = createEventDispatcher();
  let activeTab: "nozzles" | "glass" | "liquids" | "limits" | "leveling" | "gcode" | "program" = $state("nozzles");

  let settings: any = $state(null);
  let nozzleList: { name: string; h: number; d: number; s: number; c: string }[] = $state([]);
  let glassList: { name: string; w: number; h: number; z: number }[] = $state([]);
  let liquidList: {
    name: string; color: string; category: string;
    z_offset: number; z_offset_min: number | null; z_offset_max: number | null;
    extrusion: number; extrusion_min: number | null; extrusion_max: number | null;
    forbidden_nozzles: string[];
    print_speed: number; print_speed_min: number | null; print_speed_max: number | null;
    bed_temp: number; bed_temp_min: number | null; bed_temp_max: number | null;
  }[] = $state([]);
  let levelingPoints: { name: string; x: number; y: number }[] = $state([]);

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
  let loading = $state(false);
  let errorMsg = $state("");

  // ─── Expert mode (session-only, never persisted) ──────────────────────────
  let expertModeActive = $state(false);
  let showExpertWarning = $state(false);

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
  let currentTheme: Theme = $state("dark");

  // Theme žije v settings.json — DOM atribut se aplikuje reaktivně podle store
  // (pokrývá i počáteční načtení settings po startu aplikace).
  run(() => {
    const t: Theme = $settingsStore.theme === "light" ? "light" : "dark";
    currentTheme = t;
    document.documentElement.setAttribute("data-theme", t);
  });

  function applyTheme(t: Theme) {
    // Okamžité uložení — přepnutí themu platí i bez tlačítka Uložit
    settingsStore.persistPatch({ theme: t });
  }

  // ─── Sněžení ──────────────────────────────────────────────────────────────
  function isSnowSeason(): boolean {
    const now = new Date();
    const m = now.getMonth() + 1;
    const d = now.getDate();
    return m === 12 || (m === 11 && d >= 15) || (m === 1 && d <= 30);
  }
  const snowSeason = isSnowSeason();
  let snowDisabled = $state(false);

  // ─── Drag & Drop helper ───────────────────────────────────────────────────
  let dragSrcIndex: number | null = null;
  let dragOverIndex: number | null = $state(null);
  let dragList: "nozzles" | "glass" | null = $state(null);

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
    } else if (list === "glass") {
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

      snowDisabled = settings.disable_snow ?? false;
    } catch (e: any) {
      console.error("Failed to load settings in modal:", e);
      errorMsg = e?.message || String(e) || "Neznámá chyba při načítání nastavení";
      settings = null;
    } finally {
      loading = false;
    }
  }

  run(() => {
    if (isOpen) loadSettings();
  });

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

  export function openOnTab(tab: typeof activeTab) {
    activeTab = tab;
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

      settings.disable_snow = snowDisabled;
      // Theme se mohl změnit přepínačem (persistPatch) až po načtení kopie
      // settings při otevření modálu — nesmí ho přepsat stará hodnota.
      settings.theme = currentTheme;
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

  // minBedMaxX/Y jsou nyní počítány uvnitř SettingsLimits.svelte

  async function restoreDefaults() {
    const confirmed = await import("@tauri-apps/plugin-dialog").then((m) =>
      m.ask(
        "Opravdu chcete obnovit výchozí nastavení?\n\nPřepíšou se limitace tiskárny a inicializační G-kódy. Definice trysek a substrátů zůstanou zachovány.",
        { title: "DPI", kind: "warning" }
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
  <div class="fixed inset-0 bg-black/80 backdrop-blur-xs flex items-center justify-center z-60 p-4">
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
          onclick={() => (showExpertWarning = false)}
          class="px-4 py-2 text-xs font-bold rounded-lg border border-slate-600 bg-slate-800 text-slate-300 hover:bg-slate-700 transition-colors"
        >
          Zrušit
        </button>
        <button
          onclick={confirmExpertMode}
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
    class="fixed inset-0 bg-black/75 backdrop-blur-xs flex items-center justify-center z-50 p-4 select-text"
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
          onclick={close}
          class="p-1 rounded-sm bg-slate-900 border border-slate-800 hover:bg-slate-800 text-slate-400 hover:text-slate-200 transition-colors"
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
              onclick={loadSettings}
              class="bg-labaccent hover:bg-blue-600 text-white font-bold text-xs px-4 py-2 rounded-lg transition-colors"
              >Zkusit znovu</button
            >
            <button
              onclick={close}
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
                onclick={() => (activeTab = "nozzles")}
                class="px-3 py-2 rounded-sm text-left text-xs font-bold transition-colors {activeTab === 'nozzles' ? 'bg-labaccent text-white' : 'text-slate-400 hover:bg-slate-900/40 hover:text-slate-200'}"
              >Tryska</button>
              <button
                onclick={() => (activeTab = "glass")}
                class="px-3 py-2 rounded-sm text-left text-xs font-bold transition-colors {activeTab === 'glass' ? 'bg-labaccent text-white' : 'text-slate-400 hover:bg-slate-900/40 hover:text-slate-200'}"
              >Skla</button>
              <button
                onclick={() => (activeTab = "liquids")}
                class="px-3 py-2 rounded-sm text-left text-xs font-bold transition-colors {activeTab === 'liquids' ? 'bg-labaccent text-white' : 'text-slate-400 hover:bg-slate-900/40 hover:text-slate-200'}"
              >Kapaliny</button>
              <button
                onclick={() => (activeTab = "limits")}
                class="px-3 py-2 rounded-sm text-left text-xs font-bold transition-colors {activeTab === 'limits' || activeTab === 'leveling' ? 'bg-labaccent text-white' : 'text-slate-400 hover:bg-slate-900/40 hover:text-slate-200'}"
              >Tiskárna</button>
              {#if expertModeActive}
                <button
                  onclick={() => (activeTab = "gcode")}
                  class="px-3 py-2 rounded-sm text-left text-xs font-bold transition-colors flex items-center gap-1.5 {activeTab === 'gcode' ? 'bg-labred/80 text-white' : 'text-labred/70 hover:bg-labred/20 hover:text-labred'}"
                >
                  <ShieldAlert class="w-3 h-3 shrink-0" />G-kódy
                </button>
              {/if}
              <button
                onclick={() => (activeTab = "program")}
                class="px-3 py-2 rounded-sm text-left text-xs font-bold transition-colors {activeTab === 'program' ? 'bg-labaccent text-white' : 'text-slate-400 hover:bg-slate-900/40 hover:text-slate-200'}"
              >Program</button>
            </div>
            <button
              onclick={restoreDefaults}
              class="mt-auto px-3 py-2 rounded-sm text-left text-xs font-bold transition-colors text-slate-400 hover:bg-labred/20 hover:text-labred"
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
                    onclick={addNozzle}
                    class="bg-labaccent hover:bg-blue-600 text-white text-[10px] font-bold px-2 py-1 rounded-sm flex items-center gap-1 transition-colors"
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
                      <!-- svelte-ignore a11y_no_static_element_interactions -->
                      <div
                        class="grid grid-cols-12 p-2 items-center text-center gap-1 text-xs transition-colors
                               {dragOverIndex === i && dragList === 'nozzles'
                          ? 'bg-labaccent/10 border-t-2 border-labaccent'
                          : 'hover:bg-slate-900/30'}"
                        draggable="true"
                        ondragstart={(e) => onDragStart(e, "nozzles", i)}
                        ondragover={(e) => onDragOver(e, i)}
                        ondrop={() => onDrop("nozzles", i)}
                        ondragend={onDragEnd}
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
                            class="w-5 h-5 rounded-sm cursor-pointer border-none bg-transparent"
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
                          onclick={() => deleteNozzle(i)}
                          class="col-span-1 p-1 text-slate-500 hover:text-labred hover:bg-labred/10 rounded-sm flex items-center justify-center transition-colors"
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
                    onclick={addGlass}
                    class="bg-labaccent hover:bg-blue-600 text-white text-[10px] font-bold px-2 py-1 rounded-sm flex items-center gap-1 transition-colors"
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
                      <!-- svelte-ignore a11y_no_static_element_interactions -->
                      <div
                        class="grid grid-cols-12 p-2 items-center text-center gap-1 text-xs transition-colors
                               {dragOverIndex === i && dragList === 'glass'
                          ? 'bg-labaccent/10 border-t-2 border-labaccent'
                          : 'hover:bg-slate-900/30'}"
                        draggable="true"
                        ondragstart={(e) => onDragStart(e, "glass", i)}
                        ondragover={(e) => onDragOver(e, i)}
                        ondrop={() => onDrop("glass", i)}
                        ondragend={onDragEnd}
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
                          onclick={() => deleteGlass(i)}
                          class="col-span-1 p-1 text-slate-500 hover:text-labred hover:bg-labred/10 rounded-sm flex items-center justify-center transition-colors"
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
              <SettingsLiquids bind:liquidList {nozzleList} />
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
              <SettingsLeveling bind:settings bind:levelingPoints />
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
            onclick={close}
            class="bg-slate-900 border border-slate-700 hover:bg-slate-800 text-slate-300 font-bold text-xs px-4 py-2 rounded-lg transition-colors"
          >
            Zrušit
          </button>
          <button
            onclick={save}
            class="bg-labaccent hover:bg-blue-600 text-white font-bold text-xs px-4 py-2 rounded-lg flex items-center gap-1.5 shadow-lg shadow-blue-500/10 transition-colors"
          >
            <Save class="w-4 h-4" /> Uložit a zavřít
          </button>
        </div>
      {/if}
    </div>
  </div>
{/if}
