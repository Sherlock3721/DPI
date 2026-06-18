<script lang="ts">
  import { run } from 'svelte/legacy';

  import { onMount, createEventDispatcher } from "svelte";
  import {
    get_available_ports,
    connect_to_printer,
    auto_connect_printer,
    disconnect_from_printer,
    subscribe_printer_status,
    start_print,
    pause_print,
    resume_print,
    stop_print,
    get_app_settings,
    save_app_settings,
    send_manual_command,
    send_manual_blocking,
    split_gcode_pauses,
    compute_z_calibration,
    type PrinterStatus,
    type ProcessParams,
    type AppSettings,
    type LayoutPosition,
  } from "../lib/tauri";
  import { projectStore } from "../stores/projectStore";
  import { listen } from "@tauri-apps/api/event";
  import CustomSelect from "./CustomSelect.svelte";
  import NumberInput from "./NumberInput.svelte";
  import ZCalibrationModal from "./ZCalibrationModal.svelte";
  import type PrintPauseModal from "./PrintPauseModal.svelte";
  import { liquidLimits, selectedLiquidName } from "../stores/liquidStore";
  import { convertExtrusionRate, type ExtUnit } from "../lib/extrusionUnits";
  import { settingsStore } from "../stores/settingsStore";
  import {
    Play,
    Pause,
    Square,
    Link,
    Link2Off,
    RefreshCw,
    Cpu,
    Thermometer,
    FolderOpen,
    Save,
    FileSpreadsheet,
    AlignJustify,
    Route,
    Grip,
    Grid,
    Rows4,
  } from "lucide-svelte";

  const dispatch = createEventDispatcher();

  // --- PARAMETRY PŘIPOJENÍ ---
  let ports: string[] = $state([]);
  let selectedPort = $state("");
  let baudrate = $state(115200);
  let status: PrinterStatus = $state({
    is_connected: false,
    is_printing: false,
    is_paused: false,
    current_x: 0.0,
    current_y: 0.0,
    current_z: 0.0,
    temp_extruder: 0.0,
    temp_bed: 0.0,
    progress: 0,
    total_dist: 0.0,
    time_remaining: 0.0,
  });

  // Presety načítané z nastavení
  let sklo_dims: Record<string, number[]> = {
    "Laboratorní Sklo (76 x 26 x 1 mm)": [76.0, 26.0, 1.0],
    "FTO (76 x 50 x 1 mm)": [50.0, 76.0, 1.0],
  };
  let nozzle_defs: Record<string, [number, number, number, string]> = {
    Červená: [31.1, 0.3, 4.0, "#ef4444"],
    Modrá: [31.0, 0.41, 4.0, "#3b82f6"],
  };
  let glassPresets: string[] = $state([
    "Laboratorní Sklo (76 x 26 x 1 mm)",
    "FTO (76 x 50 x 1 mm)",
    "Vlastní",
  ]);
  let nozzlePresets: string[] = $state(["Červená", "Modrá", "Vlastní"]);

  interface Props {
    params?: ProcessParams;
    totalDist?: number;
    totalTime?: number;
    generatedGCode?: string;
    generateGCodeSilently?: 
    | ((overrideStartGcode?: string, skipZShiftSetup?: boolean) => Promise<{ gcode: string; dist: number; time: number }>)
    | undefined;
    positions?: LayoutPosition[];
    selectedGlass?: any;
    pauseModal: PrintPauseModal;
  }

  let {
    params = $bindable({
    sample_count: 1,
    prime_active: true,
    slide_w: 25.0,
    slide_h: 75.0,
    slide_z: 2.0,
    z_offset: 50,
    z_unit: "µm", // "mm" nebo "µm"
    nozzle_height: 30.0,
    nozzle_hidden: 4.0,
    filament_diameter: 9.5,
    flow_multiplier: 1.0,
    bed_temp: 0.0,
    extrusion_rate: 200.0,
    extrusion_unit: "nl/mm", // "µl/mm", "nl/mm", "kroky/mm"
    nozzle_diam: 0.4,
    infill_style: "Okraje + Výplň",
    infill_val: 1.0,
    infill_type: "mm", // "mm" nebo "%"
    infill_angle: 0,
    print_speed: 600.0,
    bed_leveling: true,
    nozzle_type: "Modrá",
  }),
    totalDist = 0,
    totalTime = 0,
    generatedGCode = "",
    generateGCodeSilently = undefined,
    positions = [],
    selectedGlass = $bindable(glassPresets[0]),
    pauseModal,
  }: Props = $props();
  // svelte-ignore state_referenced_locally -- záměrně jen výchozí volba trysky
  let selectedNozzle = $state(nozzlePresets[1]);

  let bed_max_x = $state(200.0);
  let bed_max_y = $state(200.0);
  let bed_max_temp = $state(100.0);
  let bed_min_temp = 30; // Min. teplota při zapnutí (konfigurovatelno v nastavení)

  // ─── Limity aktivní kapaliny (null = bez limitu, použij globální) ─────────
  let liqZMin = $derived($liquidLimits?.z_offset_min != null
    ? (params.z_unit === "µm" ? $liquidLimits.z_offset_min * 1000 : $liquidLimits.z_offset_min)
    : 0);
  let liqZMax = $derived($liquidLimits?.z_offset_max != null
    ? (params.z_unit === "µm" ? $liquidLimits.z_offset_max * 1000 : $liquidLimits.z_offset_max)
    : (params.z_unit === "µm" ? 2000 : 2.0));
  let liqExtMin = $derived($liquidLimits?.extrusion_min ?? 0);
  let liqExtMax = $derived($liquidLimits?.extrusion_max ?? 1000);
  let liqSpeedMin = $derived($liquidLimits?.print_speed_min ?? 50);
  let liqSpeedMax = $derived($liquidLimits?.print_speed_max ?? 1500);
  let liqBedTempMax = $derived($liquidLimits?.bed_temp_max ?? (bed_max_temp > 0 ? bed_max_temp : undefined));
  let filteredNozzlePresets = $derived($liquidLimits?.forbidden_nozzles?.length
    ? nozzlePresets.filter((n) => n === "Vlastní" || !$liquidLimits!.forbidden_nozzles.includes(n))
    : nozzlePresets);
  let firstPrintCompleted = $state(false);
  let calibrationDone = $state(false);
  let calibrationShift = $state(0.0);

  // Kalibrace
  let showZCalibrationModal = $state(false);
  let glassZTheoretical = $state(0.0);
  let machineBlockHeight = 34.0;
  let settingsCache: AppSettings | null = null;

  // Ochrana proti double-click / re-kliku během blokující fáze 1
  let isStarting = $state(false);

  // Pauza (M1 / M0 / M601 příkazy v start_gcode) — UI a logika v PrintPauseModal

  // Výhřev podložky: sleduje poslední platnou hodnotu pro správný skok přes šedou zónu
  let lastBedTemp = 0;
  function handleBedTempChange() {
    // Šedá zóna: při přechodu z 0 skočíme na bed_min_temp (viz nastavení), jinak na 0
    if (params.bed_temp > 0 && params.bed_temp < bed_min_temp) {
      params.bed_temp = lastBedTemp === 0 ? bed_min_temp : 0;
    }
    // Omezení na maximální teplotu z nastavení (0 = neomezeno)
    if (bed_max_temp > 0 && params.bed_temp > bed_max_temp) {
      params.bed_temp = bed_max_temp;
    }
    lastBedTemp = params.bed_temp;
    dispatch("paramsChanged", params);
  }

  function getNozzleColor(name: string): string {
    if (name === "Vlastní") return "transparent";
    const def = nozzle_defs[name];
    // nozzle_defs ukládá [výška, průměr, skrytá, barva]
    if (def && def.length >= 4 && def[3]) {
      return String(def[3]);
    }
    return "#3b82f6"; // výchozí modrá
  }

  let maxSamples =
    $derived(Math.floor(bed_max_x / (params.slide_w + 5.0)) *
      Math.floor(bed_max_y / (params.slide_h + 5.0)) || 1);
  run(() => {
    if (!firstPrintCompleted) params.bed_leveling = true;
  });

  // --- UNIT CONVERSIONS LOGIC (Strictly mirrored from Python LeftPanel) ---
  let lastZUnit = "µm";
  function handleZUnitChange() {
    if (params.z_unit === "µm" && lastZUnit === "mm") {
      params.z_offset = params.z_offset * 1000.0;
    } else if (params.z_unit === "mm" && lastZUnit === "µm") {
      params.z_offset = params.z_offset / 1000.0;
    }
    lastZUnit = params.z_unit;
    dispatch("paramsChanged", params);
  }

  let lastExtUnit: ExtUnit = "nl/mm";

  function handleExtUnitChange() {
    const calFactor = settingsCache?.calibration_factor ?? 0.014108;
    params.extrusion_rate = convertExtrusionRate(
      params.extrusion_rate,
      lastExtUnit,
      params.extrusion_unit as ExtUnit,
      calFactor
    );
    lastExtUnit = params.extrusion_unit as ExtUnit;
    dispatch("paramsChanged", params);
  }

  // --- PRESETS SYNC ---
  function handleGlassChange() {
    if (selectedGlass === "Vlastní") return;
    const dims = sklo_dims[selectedGlass];
    if (dims && dims.length >= 3) {
      params.slide_w = dims[0];
      params.slide_h = dims[1];
      params.slide_z = dims[2];
      dispatch("paramsChanged", params);
    }
  }

  function handleNozzleChange() {
    params.nozzle_type = selectedNozzle;
    if (selectedNozzle === "Vlastní") return;
    const def = nozzle_defs[selectedNozzle];
    if (def && def.length >= 3) {
      params.nozzle_height = def[0];
      params.nozzle_diam = def[1];
      params.nozzle_hidden = def[2];
      dispatch("paramsChanged", params);
    }
  }

  async function handlePrintSpeedChange() {
    dispatch("paramsChanged", params);
    if (settingsCache) {
      settingsCache.print_speed = params.print_speed;
      await save_app_settings(settingsCache);
    }
  }

  // Načtení portů a nastavení
  async function refreshPorts() {
    ports = await get_available_ports();
    if (!selectedPort) {
      selectedPort = "Automaticky";
    }
  }

  export async function loadSettings() {
    try {
      const settings = await get_app_settings();
      settingsCache = settings;
      if (settings.sklo_dims) sklo_dims = settings.sklo_dims;
      if (settings.nozzle_defs) nozzle_defs = settings.nozzle_defs;

      bed_max_x = settings.bed_max_x || 200.0;
      bed_max_y = settings.bed_max_y || 200.0;
      bed_max_temp = settings.bed_max_temp ?? 0; // 0 = neomezeno
      bed_min_temp = settings.bed_min_temp ?? 30; // z nastavení
      lastBedTemp = params.bed_temp;
      if (settings.print_speed) params.print_speed = settings.print_speed;
    } catch (e) {
      console.warn("Failed to load settings (Web Mode), using defaults.");
    }

    // Provede se i při chybě (s defaulty)
    glassPresets = Array.from(new Set([...Object.keys(sklo_dims), "Vlastní"]));
    nozzlePresets = Array.from(new Set([...Object.keys(nozzle_defs), "Vlastní"]));

    if (!glassPresets.includes(selectedGlass)) selectedGlass = glassPresets[0] || "Vlastní";
    if (!nozzlePresets.includes(selectedNozzle)) selectedNozzle = nozzlePresets[0] || "Vlastní";

    handleGlassChange();
    handleNozzleChange();
  }

  async function toggleConnection() {
    if (status.is_connected) {
      await disconnect_from_printer();
      return;
    }
    try {
      const res =
        selectedPort === "Automaticky"
          ? await auto_connect_printer(baudrate)
          : await connect_to_printer(selectedPort, baudrate);
      status = res;
      if (res.is_connected) {
        firstPrintCompleted = false;
        calibrationDone = false;
        calibrationShift = 0.0;
        params.bed_leveling = true;
        dispatch("paramsChanged", params);
      }
    } catch (e) {
      alert(`Připojení selhalo: ${e}`);
    }
  }

  async function handleStart() {
    if (!status.is_connected || isStarting) return;
    isStarting = true;

    const setts = await get_app_settings();
    const blockH = setts.block_height || 34.0;
    machineBlockHeight = blockH;
    const safeZ: number = setts.safe_z ?? 20.0;
    // Teoretická Z povrchu sklíčka — stejný výpočet jako generátor (dpi-core)
    const z_offset_mm = params.z_unit === "µm" ? params.z_offset / 1000.0 : params.z_offset;
    const calibInfo = await compute_z_calibration(
      { ...params, z_offset: z_offset_mm },
      $projectStore.overrides,
      blockH
    );
    glassZTheoretical = calibInfo.glass_z_theoretical;

    const firstPos = positions[0];
    const targetX = firstPos
      ? (firstPos.x + firstPos.width / 2).toFixed(3)
      : (bed_max_x / 2).toFixed(3);
    const targetY = firstPos
      ? (firstPos.y + firstPos.height / 2).toFixed(3)
      : (bed_max_y / 2).toFixed(3);

    // Pokud je kalibračníZ záporné, Marlin odmítne pohyb — posuneme souřadnicový systém
    // stejnou logikou jako z_shift v generátoru G-kódu (dpi-core/gcode.rs).
    // Buffer 20 mm (místo 1 mm) dává uživateli prostor sjet dolu i bez M211 S0.
    const CALIB_BUFFER = 20.0;
    const zShift = glassZTheoretical < 0 ? Math.abs(glassZTheoretical) + CALIB_BUFFER : 0.0;
    const calibVirtualZ = glassZTheoretical + zShift;    // vždy >= CALIB_BUFFER nebo glassZTheoretical
    const approachVirtualZ = calibVirtualZ + 2.0;

    let initGcode = setts.start_gcode ?? "";
    if (!params.bed_leveling) {
      initGcode = initGcode.replace(/\bG28\b(?!\s*[XYZW])/g, "G28 W");
    }
    const initSegments = await split_gcode_pauses(initGcode);

    try {
      // 1. Nouzový stop + start_gcode po segmentech (pauzy M1/M0 zobrazí modal)
      for (let i = 0; i < initSegments.length; i++) {
        const gcode = initSegments[i].code.trim();
        const prefix = i === 0 ? "M410\n" : "";
        if (gcode || i === 0) {
          await send_manual_blocking(prefix + gcode);
        }
        if (initSegments[i].msg !== null) {
          await pauseModal.waitFor(initSegments[i].msg!);
        }
      }

      // 2. Bezpečná výška + případný G92 posun + přejezd nad první pozici
      // M211 S0 zakáže soft endstopy před příjezdem do záporné machine-Z oblasti
      const shiftCmd = zShift > 0
        ? `M211 S0\nG0 Z${safeZ.toFixed(3)} F1000\nG92 Z${(safeZ + zShift).toFixed(3)}\n`
        : `G0 Z${safeZ.toFixed(3)} F1000\n`;

      if (calibrationDone) {
        // Kalibrace již proběhla — přejed na XY v bezpečné výšce a rovnou tiskni.
        // Ruční sjezd na printZ vynecháme: G-kód začne od safeZ, sjede sám.
        const origOffset = params.z_offset;
        const calibShiftInUnit = params.z_unit === "µm" ? calibrationShift * 1000.0 : calibrationShift;
        params.z_offset = origOffset + calibShiftInUnit;
        await send_manual_blocking(shiftCmd + `G0 X${targetX} Y${targetY} F3000\nM400`);
        // Virtuální posun Z nastavíme zde přes G92 (tryska už je na safeZ po shiftCmd);
        // generátor pak blok výjezdu na safe_z + G92 vynechá (skip_z_shift_setup).
        // z_shift se počítá v Rustu stejně jako v generátoru — včetně per-slide overrides.
        const z_offset_mm_calib = params.z_unit === "µm" ? params.z_offset / 1000.0 : params.z_offset;
        const { z_shift: z_shift_gen_calib } = await compute_z_calibration(
          { ...params, z_offset: z_offset_mm_calib },
          $projectStore.overrides,
          machineBlockHeight
        );
        const startOverride = `G92 Z${(safeZ + z_shift_gen_calib).toFixed(3)}\nG92 E0.0`;
        try {
          const res = generateGCodeSilently
            ? await generateGCodeSilently(startOverride, true)
            : { gcode: generatedGCode, dist: totalDist, time: totalTime };
          await start_print(res.gcode, res.dist, res.time);
          dispatch("paramsChanged", params);
        } finally {
          params.z_offset = origOffset;
          isStarting = false;
        }
      } else {
        await send_manual_blocking(
          shiftCmd +
          `G0 X${targetX} Y${targetY} F3000\nM400\n` +
          `G0 Z${approachVirtualZ.toFixed(3)} F1000\n` +
          `G1 Z${calibVirtualZ.toFixed(3)} F300\nM400`
        );
        showZCalibrationModal = true;
      }
    } catch (e) {
      alert(`Chyba při přípravě tisku: ${e}`);
      isStarting = false;
    }
  }

  async function startPrintAfterCalibration(e: CustomEvent) {
    showZCalibrationModal = false;
    calibrationShift = e.detail.shift;
    const calibShiftInUnit = params.z_unit === "µm" ? calibrationShift * 1000.0 : calibrationShift;
    const pendingZOffset = params.z_offset + calibShiftInUnit;
    const pendingOriginalZOffset = params.z_offset;
    try {
      await send_manual_blocking("M211 S1\nG91\nG0 Z5 F1000\nG90");
      params.z_offset = pendingZOffset;
      // z_shift z Rustu (stejný algoritmus jako generátor, včetně overrides);
      // G92 nastaví virtuální systém zde, generátor svůj blok vynechá (skip flag).
      const pendingZOffset_mm = params.z_unit === "µm" ? pendingZOffset / 1000.0 : pendingZOffset;
      const { z_shift: z_shift_gen } = await compute_z_calibration(
        { ...params, z_offset: pendingZOffset_mm },
        $projectStore.overrides,
        machineBlockHeight
      );
      const currentMachineZ = glassZTheoretical + calibrationShift + 5.0;
      const startOverride = `G92 Z${(currentMachineZ + z_shift_gen).toFixed(3)}\nG92 E0.0`;
      const res = generateGCodeSilently
        ? await generateGCodeSilently(startOverride, true)
        : { gcode: generatedGCode, dist: totalDist, time: totalTime };
      await start_print(res.gcode, res.dist, res.time);
      firstPrintCompleted = true;
      calibrationDone = true;
      params.bed_leveling = false;
      dispatch("paramsChanged", params);
    } catch (err) {
      alert(`Nepodařilo se zahájit tisk: ${err}`);
    } finally {
      params.z_offset = pendingOriginalZOffset;
      isStarting = false;
    }
  }

  async function handlePause() {
    status.is_paused ? await resume_print() : await pause_print();
  }

  async function handleStop() {
    if (confirm("Opravdu chcete tisk okamžitě zrušit?")) {
      await stop_print();
    }
  }

  // Spouštění externího načtení z nadřazeného App.svelte
  function triggerLoadFile() {
    dispatch("loadFile");
  }

  function triggerSaveFile() {
    dispatch("saveFile");
  }

  function triggerExportCSV() {
    dispatch("exportCSV");
  }

  // POZOR: Záměrně ŽÁDNÝ catch-all reaktivní blok pro dispatch.
  // Dřívější `$: { if (params) dispatch(...) }` způsoboval nekonečnou
  // reaktivní smyčku při editaci teploty podložky → zamrzání GUI.
  // Dispatch je voláno explicitně v každém handleru kde je potřeba.

  onMount(() => {
    refreshPorts();
    loadSettings();

    const unsubscribe = subscribe_printer_status((newStatus) => {
      const wasConnected = status.is_connected;
      status = newStatus;
      // Po odpojení → při dalším připojení musí být bed leveling povinný
      if (wasConnected && !newStatus.is_connected) {
        firstPrintCompleted = false;
        calibrationDone = false;
        calibrationShift = 0.0;
        params.bed_leveling = true;
        dispatch("paramsChanged", params);
      }
    });

    const unlistenPause = listen<string>("app-pause-requested", (event) => {
      console.log("[PAUSE-DEBUG] app-pause-requested přijato:", event.payload, "isStarting =", isStarting);
      // Ignoruj APP_PAUSE eventy během blokující fáze 1 — přepis interní Promise
      // by natrvalo zablokoval pauseModal.waitFor().
      if (isStarting) return;
      setTimeout(() => {
        console.log("[PAUSE-DEBUG] volám showFromPrintQueue");
        pauseModal.showFromPrintQueue(event.payload || "");
      }, 50);
    });

    return () => {
      unsubscribe.then((unsub) => unsub());
      unlistenPause.then((unsub) => unsub());
    };
  });
</script>

<div
  class="glass-panel rounded-lg p-2 flex flex-col gap-2 overflow-hidden h-full text-xs select-text"
>
  <!-- NADPIS PANELU -->
  <div class="flex items-center justify-between gap-1.5 text-sm font-extrabold uppercase tracking-wider text-slate-200 border-b border-slate-700/50 pb-2 shrink-0">
    <span>Globální nastavení</span>
    <span class="flex items-center gap-1 text-[10px] font-normal normal-case tracking-normal {$selectedLiquidName ? 'text-labaccent' : 'text-slate-500'}">
      {#if $selectedLiquidName}
        <span class="w-1.5 h-1.5 rounded-full shrink-0" style="background-color: {$liquidLimits?.color ?? '#3b82f6'}"></span>
      {/if}
      {$selectedLiquidName ?? 'Žádná kapalina'}
    </span>
  </div>

  <!-- HLAVNÍ FORMULÁŘ -->
  <div class="flex flex-col gap-1.5 flex-1 overflow-y-auto min-h-0">
    <!-- SEKCE: PODLOŽKA -->
    <div
      class="bg-slate-900/40 rounded-xl p-2 border border-slate-800 shadow-xl space-y-1 relative group shrink"
    >
      <h3
        class="text-xs font-extrabold uppercase tracking-wider text-slate-200 border-b border-slate-700/50 pb-1 mb-0.5"
      >
        Podložka
      </h3>

      <div
        class="grid grid-cols-3 items-center gap-2"
        title="Typ substrátu nebo vlastní rozměr"
      >
        <span class="col-span-1 text-slate-400">Substrát:</span>
        <div class="col-span-2">
          <CustomSelect
            bind:value={selectedGlass}
            on:change={handleGlassChange}
            options={glassPresets.map((p) => ({
              value: p,
              label: p,
              cssStyle: "background: rgba(255, 255, 255, 0.15); backdrop-filter: blur(4px);",
            }))}
          />
        </div>
      </div>

      <!-- VLASTNÍ ROZMĚRY SKLA (Visible if custom selected) -->
      {#if selectedGlass === "Vlastní"}
        <div
          class="grid grid-cols-4 items-center gap-2 bg-slate-950/40 p-2 rounded-sm border border-slate-850"
        >
          <div class="flex flex-col gap-0.5">
            <span class="text-[9px] text-slate-500">Šířka (mm)</span>
            <NumberInput
              step={0.5}
              bind:value={params.slide_w}
              on:input={() => dispatch("paramsChanged", params)}
              class="col-span-2 py-0.5"
            />
          </div>
          <div class="flex flex-col gap-1 items-center">
            <span class="text-[10px] text-slate-400">Y [mm]</span>
            <NumberInput
              step={0.5}
              bind:value={params.slide_h}
              on:input={() => dispatch("paramsChanged", params)}
              class="py-0.5"
            />
          </div>
          <div class="flex flex-col gap-1 items-center">
            <span class="text-[10px] text-slate-400">Z [mm]</span>
            <NumberInput
              step={0.5}
              bind:value={params.slide_z}
              on:input={() => dispatch("paramsChanged", params)}
              class="py-0.5"
            />
          </div>
          <span class="text-[10px] text-slate-400 pt-3">mm</span>
        </div>
      {/if}

      <div
        class="grid grid-cols-3 items-center gap-2"
        title="Počet substrátů, které budou vysázeny vedle sebe"
      >
        <span class="col-span-1 text-slate-400">Počet vzorků:</span>
        <div class="col-span-2 h-8">
          <NumberInput
            min={0}
            max={maxSamples}
            step={1}
            bind:value={params.sample_count}
            on:input={() => dispatch("paramsChanged", params)}
            class="w-full h-full"
          />
        </div>
      </div>

      <div
        class="grid grid-cols-3 items-center gap-2"
        title="Teplota vyhřívané podložky ve stupních Celsia"
      >
        <span class="col-span-1 text-slate-400">Výhřev podložky:</span>
        <div class="col-span-2 flex gap-2 items-center">
          <div class="flex-1 h-8">
            <NumberInput
              min={0}
              max={liqBedTempMax}
              step={5}
              bind:value={params.bed_temp}
              on:input={handleBedTempChange}
              class="w-full h-full"
            />
          </div>
          <span class="text-slate-400 text-xs w-16">{params.bed_temp > 0 ? "°C" : "Vypnuto"}</span>
        </div>
      </div>

      <div
        class="grid grid-cols-3 items-center gap-2"
        title="Vytvoří mapu nerovností podložky před prvním tiskem"
      >
        <span class="col-span-1 text-slate-400">Příprava podložky:</span>
        <button
          disabled={!firstPrintCompleted}
          onclick={() => {
            if (firstPrintCompleted) {
              params.bed_leveling = !params.bed_leveling;
              dispatch("paramsChanged", params);
            }
          }}
          class="col-span-2 text-center py-1 rounded font-bold border transition-colors {params.bed_leveling
            ? 'bg-labaccent/20 border-labaccent/50 text-labaccent'
            : 'bg-slate-900 border-slate-800 text-slate-400'} {!firstPrintCompleted
            ? 'opacity-50 cursor-not-allowed'
            : ''}"
        >
          Bed Leveling {params.bed_leveling ? "AKTIVNÍ" : "VYPNUTÝ"}
          {!firstPrintCompleted ? "(Nutné)" : ""}
        </button>
      </div>

      <div
        class="grid grid-cols-3 items-center gap-2"
        title="Prvotní nanesení kapaliny na extra substrát pro vyčištění trysky"
      >
        <span class="col-span-1 text-slate-400">Příprava trysky:</span>
        <button
          onclick={() => {
            params.prime_active = !params.prime_active;
            dispatch("paramsChanged", params);
          }}
          class="col-span-2 text-center py-1 rounded font-bold border transition-colors {params.prime_active
            ? 'bg-orange-500/20 border-orange-500/50 text-orange-500'
            : 'bg-slate-900 border-slate-800 text-slate-400'}"
        >
          Odpliv {params.prime_active ? "AKTIVNÍ" : "VYPNUTÝ"}
        </button>
      </div>

      {#if firstPrintCompleted}
        <div
          class="grid grid-cols-3 items-center gap-2"
          title="Zapíná/vypíná kalibrační dialog při příštím startu tisku"
        >
          <span class="col-span-1 text-slate-400">Kalibrace Z:</span>
          <button
            onclick={() => {
              calibrationDone = calibrationDone ? false : true;
              if (!calibrationDone) calibrationShift = 0.0;
            }}
            class="col-span-2 text-center py-1 rounded font-bold border transition-colors {!calibrationDone
              ? 'bg-labaccent/20 border-labaccent/50 text-labaccent'
              : 'bg-slate-900 border-slate-800 text-slate-400'}"
          >
            Kalibrace {!calibrationDone ? "AKTIVNÍ" : "VYPNUTÁ"}
          </button>
        </div>
      {/if}
    </div>

    <!-- SEKCE: TISKOVÉ PARAMETRY -->
    <div
      class="bg-slate-900/40 rounded-xl p-2 border border-slate-800 shadow-xl space-y-1 relative group shrink"
    >
      <h3
        class="text-xs font-extrabold uppercase tracking-wider text-slate-200 border-b border-slate-700/50 pb-1 mb-0.5"
      >
        Tiskové parametry
      </h3>

      <div
        class="grid grid-cols-3 items-center gap-2"
        title="Výška hlavy nad podložkou při tisku (Z-offset)"
      >
        <span class="col-span-1 text-slate-400">Výška trysky:</span>
        <div class="col-span-2 grid grid-cols-3 gap-1">
          <div class="col-span-2 h-8">
            <NumberInput
              min={liqZMin}
              max={liqZMax}
              step={params.z_unit === "µm" ? 50 : 0.05}
              bind:value={params.z_offset}
              on:input={() => dispatch("paramsChanged", params)}
              class="w-full h-full"
            />
          </div>
          <div class="col-span-1 h-8">
            <CustomSelect
              bind:value={params.z_unit}
              on:change={handleZUnitChange}
              options={[
                { value: "mm", label: "mm" },
                { value: "µm", label: "µm" },
              ]}
              cssStyle="height: 100%; font-size: 11px;"
            />
          </div>
        </div>
      </div>

      <div
        class="grid grid-cols-3 items-center gap-2"
        title="Rychlost vytlačování kapaliny (dávkování na milimetr trasy)"
      >
        <span class="col-span-1 text-slate-400">Extruze:</span>
        <div class="col-span-2 grid grid-cols-3 gap-1">
          <div class="col-span-2 h-8">
            <NumberInput
              min={liqExtMin}
              max={liqExtMax}
              step={0.1}
              bind:value={params.extrusion_rate}
              on:input={() => dispatch("paramsChanged", params)}
              class="w-full h-full"
            />
          </div>
          <div class="col-span-1 h-8">
            <CustomSelect
              bind:value={params.extrusion_unit}
              on:change={handleExtUnitChange}
              options={[
                { value: "nl/mm", label: "nl/mm" },
                { value: "kroky/mm", label: "krok/mm" },
              ]}
              cssStyle="height: 100%; font-size: 10px; padding-left: 2px; padding-right: 2px;"
            />
          </div>
        </div>
      </div>

      <div
        class="grid grid-cols-3 items-center gap-2"
        title="Fyzický tvar a průměr nasazené trysky"
      >
        <span class="col-span-1 text-slate-400">Typ trysky:</span>
        <div class="col-span-2">
          <CustomSelect
            bind:value={selectedNozzle}
            on:change={handleNozzleChange}
            options={filteredNozzlePresets.map((p) => ({
              value: p,
              label: p,
              color: p !== "Vlastní" ? getNozzleColor(p) : undefined,
            }))}
          />
        </div>
      </div>

      <!-- VLASTNÍ ROZMĚRY TRYSKY -->
      {#if selectedNozzle === "Vlastní"}
        <div
          class="grid grid-cols-4 items-center gap-2 bg-slate-950/40 p-2 rounded-sm border border-slate-850"
        >
          <div class="flex flex-col gap-0.5">
            <span class="text-[9px] text-slate-500">Výška (mm)</span>
            <NumberInput
              step={1}
              bind:value={params.nozzle_height}
              on:input={() => dispatch("paramsChanged", params)}
              class="w-full h-full"
            />
          </div>
          <div class="flex flex-col gap-0.5">
            <span class="text-[9px] text-slate-500">Průměr (mm)</span>
            <div class="flex-1 h-8">
              <NumberInput
                step={0.05}
                min={0.01}
                bind:value={params.nozzle_diam}
                on:input={() => dispatch("paramsChanged", params)}
                class="w-full h-full"
              />
            </div>
          </div>
          <div class="flex flex-col gap-0.5">
            <span class="text-[9px] text-slate-500">Schovaná (mm)</span>
            <NumberInput
              step={0.5}
              bind:value={params.nozzle_hidden}
              on:input={() => dispatch("paramsChanged", params)}
              class="w-full h-full"
            />
          </div>
          <span class="text-[10px] text-slate-400 pt-3">mm</span>
        </div>
      {/if}

      <div
        class="grid grid-cols-3 items-center gap-2"
        title="Globální rychlost pohybu tiskové hlavy (mm/min)"
      >
        <span class="col-span-1 text-slate-400">Rychlost tisku:</span>
        <div class="col-span-2 grid grid-cols-3 gap-1">
          <div class="col-span-2 h-8">
            <NumberInput
              min={liqSpeedMin}
              max={liqSpeedMax}
              step={100}
              bind:value={params.print_speed}
              on:input={handlePrintSpeedChange}
              class="w-full h-full"
            />
          </div>
          <div class="col-span-1 flex items-center pl-1">
            <span class="text-xs text-slate-400">mm/min</span>
          </div>
        </div>
      </div>
    </div>

    <!-- SEKCE: VEKTOROVÉ VÝPLNĚ -->
    <div
      class="bg-slate-900/40 rounded-xl p-2 border border-slate-800 shadow-xl space-y-1 relative group shrink"
    >
      <h3
        class="text-xs font-extrabold uppercase tracking-wider text-slate-200 border-b border-slate-700/50 pb-1 mb-0.5"
      >
        Vektorové výplně
      </h3>

      <div
        class="grid grid-cols-3 items-center gap-2"
        title="Vzor, jakým bude obrazec uvnitř vyplněn"
      >
        <span class="col-span-1 text-slate-400">Styl výplně:</span>
        <div class="col-span-2">
          <CustomSelect
            bind:value={params.infill_style}
            on:change={() => dispatch("paramsChanged", params)}
            options={[
              { value: "Okraje + Výplň", label: "Okraje + Výplň", icon: Rows4 },
              { value: "Výplň", label: "Výplň", icon: AlignJustify },
              { value: "Okraje", label: "Okraje", icon: Square },
              { value: "Had", label: "Had", icon: Route },
              { value: "Mřížka", label: "Mřížka", icon: Grid },
              { value: "Tečky", label: "Tečky", icon: Grip },
            ]}
          />
        </div>
      </div>

      <div
        class="grid grid-cols-3 items-center gap-2"
        title="Rozestup mezi tiskovými čarami ve výplni"
      >
        <span class="col-span-1 text-slate-400">Hustota výplně:</span>
        <div class="col-span-2 grid grid-cols-3 gap-1">
          <div class="col-span-2 h-8">
            <NumberInput
              min={params.infill_type === "počet" ? 1 : 0.001}
              step={params.infill_type === "počet" ? 1 : 0.1}
              bind:value={params.infill_val}
              on:input={() => {
                if (params.infill_type === "počet")
                  params.infill_val = Math.max(1, Math.round(params.infill_val));
                dispatch("paramsChanged", params);
              }}
              class="w-full h-full"
            />
          </div>
          <div class="col-span-1 h-8">
            <CustomSelect
              bind:value={params.infill_type}
              on:change={() => {
                if (params.infill_type === "počet")
                  params.infill_val = Math.max(1, Math.round(params.infill_val));
                dispatch("paramsChanged", params);
              }}
              options={[
                { value: "mm", label: "mm" },
                { value: "%", label: "%" },
                { value: "počet", label: "počet" },
              ]}
              cssStyle="height: 100%; font-size: 11px;"
            />
          </div>
        </div>
      </div>

      <div class="grid grid-cols-3 items-center gap-2" title="Natočení čar výplně ve stupních">
        <span class="col-span-1 text-slate-400">Úhel výplně [°]:</span>
        <div class="col-span-2 h-8">
          <NumberInput
            min={0}
            max={90}
            step={5}
            bind:value={params.infill_angle}
            on:input={() => dispatch("paramsChanged", params)}
            class="w-full h-full"
          />
        </div>
      </div>
    </div>

    <!-- SEKCE: OVLÁDÁNÍ TISKÁRNY -->
    <div
      class="bg-slate-900/40 rounded-xl p-2 border border-slate-800 shadow-xl space-y-1 relative group shrink"
    >
      <h3
        class="text-xs font-extrabold uppercase tracking-wider text-slate-200 border-b border-slate-700/50 pb-1 mb-0.5"
      >
        Ovládání tiskárny
      </h3>

      <div class="text-slate-300 font-semibold mb-0.5">
        Stav: <span
          class={status.is_printing
            ? "text-labgreen"
            : status.is_connected
              ? "text-blue-400"
              : "text-slate-400"}
        >
          {status.is_printing
            ? status.is_paused
              ? "Pozastaveno"
              : "Tiskne..."
            : status.is_connected
              ? "Připojeno"
              : "Odpojeno"}
        </span>
      </div>

      <div class="grid grid-cols-3 items-center gap-2" title="Komunikační sériový port tiskárny">
        <span class="col-span-1 text-slate-400">Port:</span>
        <div class="col-span-2 flex gap-1 items-center">
          <div class="flex-1">
            <CustomSelect
              bind:value={selectedPort}
              options={[
                { value: "Automaticky", label: "Automaticky" },
                ...ports.map((p) => ({ value: p, label: p.replace("/dev/tty", "") })),
              ]}
              cssStyle="font-size: 11px;"
            />
          </div>
          <button
            onclick={refreshPorts}
            class="bg-slate-900 border border-slate-700 hover:bg-slate-800 text-slate-400 p-1.5 rounded-sm"
          >
            <RefreshCw class="w-3.5 h-3.5" />
          </button>
        </div>
      </div>

      <div class="grid grid-cols-3 items-center gap-2" title="Rychlost komunikace po sériové lince">
        <span class="col-span-1 text-slate-400">Rychlost (Baud):</span>
        <div class="col-span-2">
          <CustomSelect
            bind:value={baudrate}
            options={[
              { value: 115200, label: "115200" },
              { value: 250000, label: "250000" },
            ]}
          />
        </div>
      </div>

      <!-- Tlačítko Připojit / Odpojit -->
      <button
        onclick={toggleConnection}
        class="w-full font-bold py-1 rounded text-white transition-colors {status.is_connected
          ? 'bg-labred hover:bg-red-600'
          : 'bg-labaccent hover:bg-blue-600'}"
      >
        {status.is_connected ? "Odpojit tiskárnu" : "Připojit tiskárnu"}
      </button>

      {#if status.is_connected}
        <!-- Tlačítko Start Tisku -->
        {#if !status.is_printing}
          <button
            onclick={handleStart}
            disabled={isStarting}
            class="w-full bg-labgreen hover:bg-green-600 disabled:opacity-40 disabled:cursor-not-allowed text-white font-bold py-1 rounded-sm transition-colors flex items-center justify-center gap-1.5"
          >
            {#if isStarting}
              <svg class="w-4 h-4 animate-spin" viewBox="0 0 24 24" fill="none">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"/>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v8H4z"/>
              </svg>
              Inicializace…
            {:else}
              <Play class="w-4 h-4" /> Start tisku
            {/if}
          </button>
        {:else}
          <!-- Ovládání za běhu (Pauza, Stop) -->
          <div class="grid grid-cols-2 gap-2">
            <button
              onclick={handlePause}
              class="bg-yellow-500 hover:bg-yellow-600 text-black font-bold py-1 rounded-sm transition-colors flex items-center justify-center gap-1"
            >
              <Pause class="w-3.5 h-3.5" />
              {status.is_paused ? "Pokračovat" : "Pozastavit"}
            </button>
            <button
              onclick={handleStop}
              class="bg-labred hover:bg-red-600 text-white font-bold py-1 rounded-sm transition-colors flex items-center justify-center gap-1"
            >
              <Square class="w-3.5 h-3.5" /> Zastavit
            </button>
          </div>
        {/if}

        <!-- Progress bar a statistiky -->
        <div class="flex flex-col gap-1.5 mt-1 border-t border-slate-800 pt-2">
          <div class="w-full bg-slate-850 rounded-full h-2 overflow-hidden border border-slate-800">
            <div
              class="bg-labaccent h-full rounded-full transition-all duration-300"
              style="width: {status.progress}%"
            ></div>
          </div>
          {#if status.is_printing}
            <div class="text-center text-[10px] text-slate-400">
              Dokončeno: {status.progress}% | Zbývá: {Math.ceil(status.time_remaining / 60)} min
            </div>
          {/if}
        </div>
      {/if}
    </div>
  </div>

  <div class="border-b border-slate-800 my-0.5"></div>

  <!-- TLAČÍTKO FEEDBACK (NAHLÁSIT CHYBU) -->
  <button
    onclick={() => window.dispatchEvent(new CustomEvent("open-feedback-form"))}
    class="w-full bg-yellow-500 hover:bg-yellow-600 text-black font-bold py-1 rounded-sm transition-colors text-xs"
  >
    Nahlásit chybu / Nápad
  </button>
</div>


{#if showZCalibrationModal}
  <ZCalibrationModal
    {glassZTheoretical}
    on:confirm={startPrintAfterCalibration}
    on:cancel={async () => {
      showZCalibrationModal = false;
      isStarting = false;
      try { await send_manual_blocking("M211 S1"); } catch (_) {}
    }}
  />
{/if}
