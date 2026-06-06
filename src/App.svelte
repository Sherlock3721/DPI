<script lang="ts">
  import { onMount } from "svelte";
  import LeftPanel from "./components/LeftPanel.svelte";
  import RightPanel from "./components/RightPanel.svelte";
  import GraphicsView from "./components/GraphicsView.svelte";
  import Header from "./components/Header.svelte";
  import FeedbackModal from "./components/FeedbackModal.svelte";
  import Terminal from "./components/Terminal.svelte";
  import SettingsModal from "./components/SettingsModal.svelte";
  import DiagnosticsModal from "./components/DiagnosticsModal.svelte";
  import AboutModal from "./components/AboutModal.svelte";
  import ShortcutsModal from "./components/ShortcutsModal.svelte";
  import UpdateModal from "./components/UpdateModal.svelte";
  import SnowEffect from "./components/SnowEffect.svelte";
  import { getBoundingBoxOfPaths } from "./lib/path_processor";
  import { getTransformIdx } from "./lib/geometry";
  import {
    parse_dxf,
    parse_svg,
    calculate_slide_layout,
    generate_gcode,
    send_manual_command,
    submit_feedback,
    get_app_settings,
    build_gcode_metadata_header,
    parse_gcode_metadata,
    parse_gcode_file_paths,
    generate_csv_protocol,
    check_paths_overflow,
    type ProcessParams,
    type LayoutPosition,
    type SubstratePaths,
    type Transform,
    type Point2D,
    type SlideOverride,
    type GCodeMetadata,
  } from "./lib/tauri";
  import { projectStore, addRecentFile } from "./stores/projectStore";
  import { settingsStore } from "./stores/settingsStore";
  import { selectedLiquidName } from "./stores/liquidStore";
  import { printerStore } from "./stores/printerStore";
  import { Cpu, FileText, Keyboard, Save } from "lucide-svelte";
  import { save, ask, open } from "@tauri-apps/plugin-dialog";
  import { writeTextFile, readTextFile } from "@tauri-apps/plugin-fs";
  import { check } from "@tauri-apps/plugin-updater";
  import WelcomeModal from "./components/WelcomeModal.svelte";
  import BracketExportModal from "./components/BracketExportModal.svelte";

  // --- STAV APLIKACE ---
  const isTauri = typeof window !== "undefined" && window.__TAURI_INTERNALS__ !== undefined;
  let showWelcomeModal = isTauri;

  // Sněžení aktivní mezi 15. 11. a 30. 1.
  function isSnowSeason(): boolean {
    const now = new Date();
    const m = now.getMonth() + 1; // 1–12
    const d = now.getDate();
    return m === 12 || m === 1 || (m === 11 && d >= 15) || (m === 1 && d <= 30);
  }
  let showSnow = isSnowSeason() && localStorage.getItem("disable-snow") !== "1";

  let ws: WebSocket | null = null;
  let wsUpdateInProgress = false;

  // --- OPRÁVNĚNÝ A DIAGNOSTICKÝ FORMULÁŘ ZPĚTNÉ VAZBY ---
  let showFeedbackModal = false;

  let selectedGlass = "";

  let globalBedX = 250.0;
  let globalBedY = 210.0;
  let globalStartOffsetX = 18.0;
  let globalStartOffsetY = 11.0;
  let globalMultiSpacing = 5.0;
  let globalBlockHeight = 34.0;

  let generatedGCode = "";
  let totalDist = 0;
  let totalTime = 0;
  let showGCodeModal = false;
  let gcodeError = "";

  // Tisková pozice trysky z telemetry smyčky
  let currentNozzle: Point2D | null = null;

  // Modály
  let showSettingsModal = false;
  let settingsModalRef: any;
  let showDiagnosticsModal = false;
  let showAboutModal = false;
  let showShortcutsModal = false;
  let showUpdateModal = false;
  let updateModalAutoCheck = false;
  let showBracketExportModal = false;

  let leftPanelRef: any;

  // Canvas ↔ pravý panel
  let canvasExternalSelected = -1; // posIdx → zvýrazní sklíčko v plátně
  let rightPanelOpenIdx = -1; // sampleIdx → otevře accordion
  let rightPanelTrigger = 0; // inkrementuje se při každém canvas kliku

  function sampleToPositionIdx(sampleIdx: number): number {
    const positions = $projectStore.positions;
    let cnt = 0;
    for (let i = 0; i < positions.length; i++) {
      if (!positions[i].is_prime) {
        if (cnt === sampleIdx) return i;
        cnt++;
      }
    }
    return -1;
  }

  // Reaktivní re-parse SVG/DXF při změně jemnosti křivek
  let _prevFineness = 1.0;
  $: {
    const f = $settingsStore.path_fineness ?? 1.0;
    if (f !== _prevFineness && $projectStore.rawFileText !== null) {
      _prevFineness = f;
      projectStore.reparseRaw(f);
    }
  }

  // Kontrola mezí s ohledem na průměr trysky
  function checkBoundsAgainstNozzle(paths: SubstratePaths, silent = false): boolean {
    const bbox = getBoundingBoxOfPaths(paths.segments);
    if (!bbox.hasPoints) return false;
    const width = bbox.maxX - bbox.minX;
    const height = bbox.maxY - bbox.minY;
    const nozzleDiam = $projectStore.params.nozzle_diam ?? 0.4;
    const usableW = $projectStore.params.slide_w - nozzleDiam;
    const usableH = $projectStore.params.slide_h - nozzleDiam;
    if (width > usableW || height > usableH) {
      $projectStore.autoScaleFile = true;
      if (!silent) {
        alert(
          `Upozornění: Objekt (${width.toFixed(1)} × ${height.toFixed(1)} mm) přesahuje tisknutelnou plochu substrátu s ohledem na průměr trysky ${nozzleDiam} mm.\n` +
            `Dostupná plocha: ${usableW.toFixed(1)} × ${usableH.toFixed(1)} mm.\n\n` +
            `Objekt byl automaticky zmenšen.`
        );
      }
      return true;
    } else {
      $projectStore.autoScaleFile = false;
      return false;
    }
  }

  // Zkontroluje, zda by aktuálně umístěný objekt přesáhl okraj sklíčka
  // s insetem nového průměru trysky. Ptá se uživatele před zmenšením.
  async function handleNozzleDiamGrew(newDiam: number) {
    if (!$projectStore.rawLoadedPaths || $projectStore.autoScaleFile) return;

    const state = $projectStore;
    const nonPrimePositions = state.positions.filter((p) => !p.is_prime);

    const anyOverflow = await check_paths_overflow(
      state.paths,
      state.transforms,
      nonPrimePositions,
      newDiam
    );

    if (!anyOverflow) return;

    const confirmed = await ask(
      `Se zvolenou tryskou (∅ ${newDiam} mm) by tisknutá trasa přesáhla okraj substrátu\n` +
        `a tryska by se dotkla jeho stěny.\n\n` +
        `Chcete objekt automaticky zmenšit?`,
      { title: "Varování — průměr trysky", type: "warning" }
    );

    if (confirmed) {
      projectStore.update((s) => ({ ...s, autoScaleFile: true }));
    }
  }

  // Reaktivní kontrola při změně trysky — spustí se jen při zvětšení průměru
  let _prevNozzleDiam = $projectStore.params.nozzle_diam;
  $: {
    const nd = $projectStore.params.nozzle_diam;
    if (nd !== _prevNozzleDiam) {
      const grew = nd > _prevNozzleDiam;
      _prevNozzleDiam = nd;
      if (grew) handleNozzleDiamGrew(nd);
    }
  }

  // Generuje automatický náhled drah na základě tiskového nastavení
  function generatePreviewPaths(p: ProcessParams): SubstratePaths[] {
    const pathsList: SubstratePaths[] = [];
    const margin = 2.0;

    for (let i = 0; i < p.sample_count; i++) {
      const segments = [];
      const w = p.slide_w;
      const h = p.slide_h;

      // 1. Okraje (Perimeter)
      if (p.infill_style !== "Výplň" && p.infill_style !== "Tečky") {
        segments.push({
          points: [
            { x: margin, y: margin },
            { x: w - margin, y: margin },
            { x: w - margin, y: h - margin },
            { x: margin, y: h - margin },
            { x: margin, y: margin },
          ],
        });
      }

      // 2. Výplň (Infill)
      if (
        p.infill_style === "Okraje + Výplň" ||
        p.infill_style === "Výplň" ||
        p.infill_style === "Had"
      ) {
        const infillSpacing = 3.0;
        const points = [];
        let direction = 1;
        for (let y = margin + infillSpacing; y <= h - margin - infillSpacing; y += infillSpacing) {
          if (direction > 0) {
            points.push({ x: margin + infillSpacing, y });
            points.push({ x: w - margin - infillSpacing, y });
          } else {
            points.push({ x: w - margin - infillSpacing, y });
            points.push({ x: margin + infillSpacing, y });
          }
          direction *= -1;
        }
        if (points.length > 0) {
          segments.push({ points });
        }
      } else if (p.infill_style === "Mřížka") {
        const gridSpacing = 3.0;
        for (let y = margin + gridSpacing; y <= h - margin - gridSpacing; y += gridSpacing) {
          segments.push({
            points: [
              { x: margin + gridSpacing, y },
              { x: w - margin - gridSpacing, y },
            ],
          });
        }
        for (let x = margin + gridSpacing; x <= w - margin - gridSpacing; x += gridSpacing) {
          segments.push({
            points: [
              { x, y: margin + gridSpacing },
              { x, y: h - margin - gridSpacing },
            ],
          });
        }
      } else if (p.infill_style === "Tečky") {
        const dotSpacing = 6.0;
        for (let y = margin + dotSpacing; y <= h - margin - dotSpacing; y += dotSpacing) {
          for (let x = margin + dotSpacing; x <= w - margin - dotSpacing; x += dotSpacing) {
            segments.push({
              points: [
                { x, y },
                { x, y },
              ],
            });
          }
        }
      }

      pathsList.push({ segments });
    }
    return pathsList;
  }

  // Přepočítá rozložení sklíček a synchronizuje transformace
  let layoutTimeout: ReturnType<typeof setTimeout>;

  // Spustí generování G-kódu na Rust backendu.
  // overrideStartGcode: pokud předáno, použije se místo start_gcode ze settings
  // (předej "" pokud byl start_gcode již odeslán v pre-kalibrační fázi)
  async function triggerGCodeGeneration(overrideStartGcode?: string) {
    gcodeError = "";
    try {
      const setts = await get_app_settings();
      let startGcode = overrideStartGcode !== undefined ? overrideStartGcode : (setts.start_gcode ?? "");
      const endGcode = setts.end_gcode ?? "";
      const loopStartGcode = setts.loop_start_gcode ?? "";
      const loopEndGcode = setts.loop_end_gcode ?? "";
      const calFactor = setts.calibration_factor ?? 0.0141;
      const zHop = setts.default_z_hop ?? 2.0;
      const safeZ = (setts as any).safe_z ?? 20.0;

      const currentParams = $projectStore.params;
      if (!currentParams.bed_leveling) {
        // Bez bed levelingu: G28 → G28 W (obnoví mesh z paměti, neprobuje znovu)
        startGcode = startGcode.replace(/\bG28\b(?!\s*[XYZW])/g, "G28 W");
      }
      const currentPaths = $projectStore.paths;
      const currentTransforms = $projectStore.transforms;
      const currentOverrides = $projectStore.overrides;

      // Rust expects z_offset in mm — convert if the UI unit is µm
      const paramsForRust = currentParams.z_unit === "µm"
        ? { ...currentParams, z_offset: currentParams.z_offset / 1000.0 }
        : currentParams;

      const res = await generate_gcode(
        currentPaths,
        paramsForRust,
        currentTransforms,
        currentOverrides,
        startGcode,
        endGcode,
        loopStartGcode,
        loopEndGcode,
        globalBedX,
        globalBedY,
        globalStartOffsetX,
        globalStartOffsetY,
        globalMultiSpacing,
        globalBlockHeight,
        calFactor,
        setts.bed_min_x ?? 0.0,
        zHop,
        safeZ
      );

      generatedGCode = res.gcode;
      totalDist = res.total_dist;
      totalTime = res.total_time;
      // Uložíme výsledek do projectStore, aby ho LeftPanel měl aktuální
      projectStore.setGCodeResult(res.gcode, res.total_dist, res.total_time);
      return { gcode: generatedGCode, dist: totalDist, time: totalTime };
    } catch (e) {
      gcodeError = `Generování selhalo: ${e}`;
      alert(gcodeError);
      throw e;
    }
  }

  export async function generateGCodeSilently(overrideStartGcode?: string) {
    return await triggerGCodeGeneration(overrideStartGcode);
  }

  // Otevření dialogu souboru přes Tauri API
  async function checkUnsavedChanges(): Promise<boolean> {
    if ($projectStore.isDirty) {
      return await ask("Máte neuložené změny. Opravdu chcete pokračovat a změny zahodit?", {
        title: "DPI",
        type: "warning",
      });
    }
    return true;
  }

  async function triggerLoadFileInput() {
    if (!(await checkUnsavedChanges())) return;
    try {
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: "Supported Files",
            extensions: ["gcode", "svg", "dxf"],
          },
        ],
      });
      if (selected && typeof selected === "string") {
        await loadFileFromPath(selected);
      }
    } catch (e) {
      console.error("Chyba při otevírání souboru:", e);
      alert("Nepodařilo se otevřít dialog pro výběr souboru.");
    }
  }

  async function loadRecentFile(absolutePath: string) {
    if (!(await checkUnsavedChanges())) return;
    await loadFileFromPath(absolutePath);
  }

  // Načtení souboru podle cesty (voláno i z Welcome Screenu)
  export async function loadFileFromPath(absolutePath: string) {
    try {
      const text = await readTextFile(absolutePath);
      const fileName = absolutePath.split(/[/\\]/).pop() || "";
      const ext = fileName.split(".").pop()?.toLowerCase() ?? "";
      const fineness = $settingsStore.path_fineness ?? 1.0;

      showWelcomeModal = false;
      addRecentFile(absolutePath, fileName);

      switch (ext) {
        case "svg": {
          const parsed = await parse_svg(text, fineness);
          projectStore.setRawPaths(parsed, false, fileName, text, ext);
          checkBoundsAgainstNozzle(parsed);
          projectStore.setProjectSaved(null);
          break;
        }
        case "dxf": {
          const parsed = await parse_dxf(text);
          projectStore.setRawPaths(parsed, false, fileName, text, ext);
          checkBoundsAgainstNozzle(parsed);
          projectStore.setProjectSaved(null);
          break;
        }
        case "gcode": {
          projectStore.setProjectSaved(absolutePath);
          const meta = await parse_gcode_metadata(text);
          if (meta) {
            await projectStore.restoreFromGCode(meta, fineness);
            if ($projectStore.rawLoadedPaths) {
              checkBoundsAgainstNozzle($projectStore.rawLoadedPaths, true);
            }
          } else {
            // Externí GCode bez DPI metadat — načti jako vizualizaci dráhy
            const parsed = await parse_gcode_file_paths(text);
            projectStore.setRawPaths(parsed, false, fileName, text, "gcode");
            checkBoundsAgainstNozzle(parsed);
          }
          break;
        }
        default:
          alert(`Nepodporovaný formát souboru: .${ext}`);
      }
    } catch (e) {
      console.error("Chyba při načítání souboru:", e);
      alert("Chyba při čtení souboru: " + e);
    }
  }

  // Uložení vygenerovaného G-kódu na klienta
  async function _internalSaveGCode(saveAs: boolean) {
    const data = await triggerGCodeGeneration();
    const state = $projectStore;
    const meta: GCodeMetadata = {
      params: state.params,
      overrides: state.overrides,
      transforms: state.transforms,
      baked_scales: state.bakedScales ?? [],
      source_file_name: state.fileName ?? "",
      source_file_ext: state.rawFileExt ?? "",
      source_file_content: state.rawFileText ?? "",
      auto_scale: state.autoScaleFile,
      fineness: $settingsStore.path_fineness ?? 1.0,
    };
    const header = await build_gcode_metadata_header(meta);
    const finalGCode = header + data.gcode;

    try {
      let filePath = $projectStore.projectFilePath;

      if (saveAs || !filePath) {
        const selected = await save({
          filters: [
            {
              name: "G-Code",
              extensions: ["gcode"],
            },
          ],
          defaultPath: filePath || `vzorek_${new Date().toISOString().substring(0, 10)}.gcode`,
        });
        if (!selected) return;
        filePath = selected;
      }

      await writeTextFile(filePath, finalGCode);
      // Přidání do nedávných projektů po uložení
      const fileName = filePath.split(/[/\\]/).pop() || filePath;
      addRecentFile(filePath, fileName);
      projectStore.setProjectSaved(filePath);
    } catch (e) {
      alert(`Chyba při ukládání G-kódu: ${e}`);
    }
  }

  async function saveProject() {
    await _internalSaveGCode(false);
  }

  async function saveProjectAs() {
    await _internalSaveGCode(true);
  }

  // Uložení protokolu tisku jako CSV pro chemiky a další zpracování
  async function exportCSVProtocol() {
    const csvContent = await generate_csv_protocol(
      $projectStore.params,
      $projectStore.overrides,
      $projectStore.totalDist,
      $projectStore.totalTime,
      selectedGlass || "",
      "DPI 1.5.1",
      new Date().toLocaleString()
    );
    try {
      const filePath = await save({
        filters: [{ name: "CSV Protokol", extensions: ["csv"] }],
        defaultPath: `protokol_${new Date().toISOString().substring(0, 10)}.csv`,
      });
      if (filePath) {
        await writeTextFile(filePath, "\uFEFF" + csvContent);
      }
    } catch (e) {
      alert(`Chyba při ukládání CSV protokolu: ${e}`);
    }
  }

  // Ukončení aplikace
  async function quitApp() {
    try {
      const { exit } = await import("@tauri-apps/plugin-process");
      await exit(0);
    } catch (e) {
      window.close();
    }
  }

  // Otevření ručního posuvu (vyvolá window event)
  function openManualControl() {
    window.dispatchEvent(new CustomEvent("open-manual-movement"));
  }

  // Obnovení stavu pro nový projekt
  async function resetProject() {
    if (!(await checkUnsavedChanges())) return;

    // Kompletní vyresetování Svelte aplikace, jako při novém startu
    window.location.reload();
  }


  // Načtení nových nastavení v panelu po uložení
  async function handleSettingsSave() {
    if (leftPanelRef && leftPanelRef.loadSettings) {
      leftPanelRef.loadSettings();
    }
    const setts = await get_app_settings();
    globalBedX = setts.bed_max_x || 250.0;
    globalBedY = setts.bed_max_y || 210.0;
    globalStartOffsetX = setts.start_offset_x || 18.0;
    globalStartOffsetY = setts.start_offset_y || 11.0;
    globalMultiSpacing = setts.multi_spacing || 5.0;
    globalBlockHeight = setts.block_height || 34.0;
    await settingsStore.load();
    projectStore.triggerLayoutUpdate();
    showSnow = isSnowSeason() && localStorage.getItem("disable-snow") !== "1";
  }

  // Globální klávesové zkratky
  function handleKeyDown(event: KeyboardEvent) {
    if (event.ctrlKey && event.key.toLowerCase() === "o") {
      event.preventDefault();
      triggerLoadFileInput();
    } else if (event.ctrlKey && event.key.toLowerCase() === "s") {
      event.preventDefault();
      triggerGCodeGeneration();
    } else if (event.ctrlKey && event.key.toLowerCase() === "q") {
      event.preventDefault();
      quitApp();
    } else if (event.ctrlKey && event.key.toLowerCase() === "z") {
      event.preventDefault();
      if (event.shiftKey) {
        projectStore.redo();
      } else {
        projectStore.undo();
      }
    }
  }

  // Reaktivní sledování parametrů z LeftPanel
  function handleParamsChanged(event: CustomEvent<ProcessParams>) {
    $projectStore.params = event.detail;
    projectStore.triggerLayoutUpdate();
  }

  // Reaktivní sledování tažení sklíček na Canvasu
  function handleTransformChanged(event: CustomEvent<{ index: number; transform: Transform }>) {
    const { index, transform } = event.detail;
    projectStore.updateTransform(index, transform);
  }

  function handlePathCleared(event: CustomEvent<{ index: number }>) {
    const { index } = event.detail;
    projectStore.clearPath(index);
  }

  onMount(async () => {
    // Odstranění loading screenu
    const loader = document.querySelector(".loading-screen");
    if (loader) loader.remove();

    // WebSocket synchronizace stavu
    const host = isTauri ? "127.0.0.1" : window.location.hostname;
    ws = new WebSocket(`ws://${host}:5174`);

    ws.onmessage = (event) => {
      try {
        const state = JSON.parse(event.data);
        wsUpdateInProgress = true;
        projectStore.set(state);
        setTimeout(() => {
          wsUpdateInProgress = false;
        }, 50);
      } catch (e) {
        console.error("Failed to parse state from WS", e);
      }
    };

    const unsubWs = projectStore.subscribe((state) => {
      if (ws && ws.readyState === WebSocket.OPEN && !wsUpdateInProgress) {
        ws.send(JSON.stringify(state));
      }
    });

    // Zobrazení okna (řeší bílé probliknutí na začátku)
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const appWindow = getCurrentWindow();
      await appWindow.show();
      const startupTime = await import("@tauri-apps/api/core").then((m) =>
        m.invoke("get_startup_time")
      );
      console.log(`Doba startu aplikace: ${startupTime} ms`);

      let closeConfirmed = false;
      appWindow.onCloseRequested(async (event) => {
        if (closeConfirmed) return;
        if ($projectStore.isDirty) {
          event.preventDefault();
          const confirmed = await ask("Máte neuložené změny. Opravdu chcete aplikaci zavřít?", {
            title: "DPI",
            type: "warning",
          });
          if (confirmed) {
            closeConfirmed = true;
            appWindow.close();
          }
        }
      });
    } catch (e) {
      console.warn("Nepodařilo se zobrazit okno:", e);
    }

    // Fetch initial settings
    try {
      const setts = await get_app_settings();
      globalBedX = setts.bed_max_x || 250.0;
      globalBedY = setts.bed_max_y || 210.0;
      globalStartOffsetX = setts.start_offset_x || 18.0;
      globalStartOffsetY = setts.start_offset_y || 11.0;
      globalMultiSpacing = setts.multi_spacing || 5.0;
      globalBlockHeight = setts.block_height || 34.0;
      await settingsStore.load();
    } catch (e) {
      console.warn("Nepodařilo se načíst nastavení z Tauri (Web Mode):", e);
    }

    projectStore.triggerLayoutUpdate();

    const openFeedback = () => {
      showFeedbackModal = true;
    };
    window.addEventListener("open-feedback-form", openFeedback);

    return () => {
      window.removeEventListener("open-feedback-form", openFeedback);
      unsubWs();
      if (ws) ws.close();
    };
  });

  // Auto-updater: tiše zkontroluje na pozadí, ukáže modal jen pokud je update
  onMount(() => {
    if (!isTauri) return;
    setTimeout(async () => {
      try {
        const update = await check();
        if (update) {
          updateModalAutoCheck = true;
          showUpdateModal = true;
        }
      } catch (err) {
        console.error("Chyba při kontrole aktualizací:", err);
      }
    }, 2500);
  });
</script>

<svelte:window on:keydown={handleKeyDown} />

<main
  class="w-screen h-screen flex flex-col bg-labdark text-slate-100 overflow-hidden font-sans p-3 gap-3"
>
  <!-- APPLIKAČNÍ HLAVIČKA -->
  <Header
    {isTauri}
    onResetProject={resetProject}
    onTriggerLoadFileInput={triggerLoadFileInput}
    onLoadRecentFile={loadRecentFile}
    onSaveProject={saveProject}
    onSaveProjectAs={saveProjectAs}
    onExportCSVProtocol={exportCSVProtocol}
    onQuitApp={quitApp}
    liquidNames={Object.keys($settingsStore.liquid_defs ?? {})}
    activeLiquid={$selectedLiquidName}
    activeLiquidColor={$settingsStore.liquid_defs?.[$selectedLiquidName ?? ""]?.color ?? null}
    onSelectLiquid={(name) => selectedLiquidName.set(name)}
    onOpenSettings={() => (showSettingsModal = true)}
    onOpenLiquidDefinition={() => { settingsModalRef?.openOnTab("liquids"); showSettingsModal = true; }}
    onOpenDiagnostics={() => (showDiagnosticsModal = true)}
    onOpenFeedback={() => (showFeedbackModal = true)}
    onOpenShortcuts={() => (showShortcutsModal = true)}
    onOpenAbout={() => (showAboutModal = true)}
    onCheckForUpdates={() => { updateModalAutoCheck = false; showUpdateModal = true; }}
    onOpenBracketExport={() => (showBracketExportModal = true)}
  />

  <!-- HLAVNÍ PROSTOR - TŘÍSLOUPOVÝ LAYOUT -->
  <div class="flex-1 grid grid-cols-12 gap-3 overflow-hidden">
    <!-- LEVÝ PANEL -->
    <div class="col-span-3 overflow-hidden h-full">
      <LeftPanel
        bind:this={leftPanelRef}
        {isTauri}
        bind:params={$projectStore.params}
        bind:selectedGlass
        totalDist={$projectStore.totalDist}
        totalTime={$projectStore.totalTime}
        generatedGCode={$projectStore.generatedGCode}
        positions={$projectStore.positions}
        on:paramsChanged={handleParamsChanged}
        on:loadFile={triggerLoadFileInput}
        on:saveFile={saveProject}
        on:exportCSV={exportCSVProtocol}
        on:generateGCode={triggerGCodeGeneration}
        {generateGCodeSilently}
      />
    </div>

    <!-- STŘEDNÍ ČÁST -->
    <div class="col-span-6 overflow-hidden h-full flex flex-col">
      <GraphicsView
        bedMaxX={globalBedX}
        bedMaxY={globalBedY}
        positions={$projectStore.positions}
        paths={$projectStore.paths}
        primePath={$projectStore.primePath}
        transforms={$projectStore.transforms}
        {currentNozzle}
        nozzleDiam={$projectStore.params ? $projectStore.params.nozzle_diam : 0.4}
        overrides={$projectStore.overrides}
        externalSelectedIndex={canvasExternalSelected}
        totalPreviewTime={$projectStore.totalTime ?? 0}
        on:transformChanged={handleTransformChanged}
        on:pathCleared={handlePathCleared}
        on:saveState={() => projectStore.pushState()}
        on:pathRebuildNeeded={(e) =>
          projectStore.rebuildSlicePath(e.detail.slideIdx, e.detail.scale, e.detail.rotation)}
        on:slideSelected={(e) => {
          const posIdx = e.detail;
          canvasExternalSelected = posIdx;
          leftPanelRef?.selectSlide?.(posIdx);
          const sampleIdx = getTransformIdx(posIdx, $projectStore.positions);
          if (sampleIdx >= 0) {
            rightPanelOpenIdx = sampleIdx;
            rightPanelTrigger += 1;
          }
        }}
      />
    </div>

    <!-- PRAVÝ PANEL -->
    <div class="col-span-3 overflow-hidden h-full">
      <RightPanel
        sampleCount={$projectStore.params ? $projectStore.params.sample_count : 1}
        primeActive={$projectStore.params ? $projectStore.params.prime_active : false}
        bind:overrides={$projectStore.overrides}
        openSlideIdx={rightPanelOpenIdx}
        openTrigger={rightPanelTrigger}
        on:slideActivated={(e) => {
          canvasExternalSelected = sampleToPositionIdx(e.detail);
        }}
      />
    </div>
  </div>

  <!-- ADVANCED SETTINGS MODAL -->
  <SettingsModal bind:this={settingsModalRef} bind:isOpen={showSettingsModal} on:save={handleSettingsSave} />

  <!-- SHORTCUTS DIALOG -->
  {#if showShortcutsModal}
    <div
      class="fixed inset-0 bg-black/75 backdrop-blur-sm flex items-center justify-center z-50 p-4"
    >
      <div
        class="glass-panel max-w-md w-full rounded-xl p-5 flex flex-col gap-4 border border-slate-800 shadow-2xl"
      >
        <div
          class="flex items-center gap-2.5 pb-2 border-b border-slate-800 text-labaccent font-bold"
        >
          <Keyboard class="w-5 h-5" />
          <h3 class="text-sm font-bold uppercase tracking-wider">Klávesové zkratky</h3>
        </div>

        <div class="flex flex-col gap-2 max-h-[60vh] overflow-y-auto text-xs text-slate-300">
          <table class="w-full text-left divide-y divide-slate-850">
            <tbody class="divide-y divide-slate-850">
              <tr
                ><td class="py-1.5 font-bold text-slate-200">Ctrl + O</td><td class="py-1.5"
                  >Načíst vzorek (G-code / SVG / DXF)</td
                ></tr
              >
              <tr
                ><td class="py-1.5 font-bold text-slate-200">Ctrl + S</td><td class="py-1.5"
                  >Vygenerovat G-kód</td
                ></tr
              >
              <tr
                ><td class="py-1.5 font-bold text-slate-200">Ctrl + Q</td><td class="py-1.5"
                  >Ukončit aplikaci</td
                ></tr
              >
              <tr class="bg-slate-900/50"
                ><td colspan="2" class="py-1.5 font-bold text-slate-400 pl-1">Práce s podložkou:</td
                ></tr
              >
              <tr
                ><td class="py-1.5 font-bold text-slate-200">Ctrl + Tažení</td><td class="py-1.5"
                  >Přichytit k mřížce (Snap to Grid)</td
                ></tr
              >
              <tr
                ><td class="py-1.5 font-bold text-slate-200">Shift + Tažení</td><td class="py-1.5"
                  >Synchronizovaný pohyb všech substrátů</td
                ></tr
              >
            </tbody>
          </table>
        </div>

        <div class="flex justify-end border-t border-slate-800 pt-3">
          <button
            on:click={() => (showShortcutsModal = false)}
            class="bg-slate-900 hover:bg-slate-800 border border-slate-700 text-slate-200 text-xs px-4 py-1.5 rounded-md transition-colors"
          >
            Zavřít
          </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- ABOUT DIALOG -->
  {#if showAboutModal}
    <div
      class="fixed inset-0 bg-black/75 backdrop-blur-sm flex items-center justify-center z-50 p-4"
    >
      <div
        class="glass-panel max-w-sm w-full rounded-xl p-5 flex flex-col gap-4 border border-slate-800 shadow-2xl text-center items-center"
      >
        <div class="bg-labaccent/10 border border-labaccent/30 p-3 rounded-2xl mb-1">
          <Cpu class="w-10 h-10 text-labaccent" />
        </div>

        <div>
          <h3 class="text-md font-bold text-slate-200 tracking-wide uppercase">
            Droplet Printing Interface
          </h3>
          <p class="text-xs text-slate-500 font-mono mt-0.5">Verze 1.5.0</p>
        </div>

        <p class="text-[11px] text-slate-400 leading-relaxed max-w-xs mt-1">
          Specializovaná aplikace pro laboratorní výzkum a vývoj 2D tisku kapalin na podložní
          sklíčka. Portováno do nativního systému Rust + Tauri.
        </p>

        <div class="text-[9px] text-slate-500 mt-2">© 2026 Sherlock3721 / VUT Brno</div>

        <button
          on:click={() => (showAboutModal = false)}
          class="mt-2 bg-slate-900 hover:bg-slate-800 border border-slate-700 text-slate-200 text-xs px-6 py-1.5 rounded-md transition-colors"
        >
          Zavřít
        </button>
      </div>
    </div>
  {/if}

  <!-- FEEDBACK FORM MODAL -->
  <FeedbackModal bind:show={showFeedbackModal} />

  <!-- DIAGNOSTICS MODAL -->
  <DiagnosticsModal isOpen={showDiagnosticsModal} on:close={() => (showDiagnosticsModal = false)} />

  <AboutModal show={showAboutModal} on:close={() => (showAboutModal = false)} />

  <ShortcutsModal show={showShortcutsModal} on:close={() => (showShortcutsModal = false)} />

  {#if showUpdateModal}
    <UpdateModal
      autoCheck={updateModalAutoCheck}
      on:close={() => (showUpdateModal = false)}
    />
  {/if}

  <BracketExportModal
    isOpen={showBracketExportModal}
    on:close={() => (showBracketExportModal = false)}
  />

  <WelcomeModal
    show={showWelcomeModal}
    on:newProject={triggerLoadFileInput}
    on:openRecent={async (e) => {
      showWelcomeModal = false;
      await loadFileFromPath(e.detail);
    }}
  />

  {#if showSnow}
    <SnowEffect />
  {/if}
</main>
