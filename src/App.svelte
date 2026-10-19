<script lang="ts">
  import { run } from 'svelte/legacy';

  import { onDestroy, onMount } from "svelte";
  import LeftPanel from "./components/LeftPanel.svelte";
  import RightPanel from "./components/RightPanel.svelte";
  import GraphicsView from "./components/GraphicsView.svelte";
  import Header from "./components/Header.svelte";
  import FeedbackModal from "./components/FeedbackModal.svelte";
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
    generate_gcode,
    send_manual_command,
    submit_feedback,
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
  import { projectStore, addRecentFile, type ProjectState } from "./stores/projectStore";
  import { settingsStore } from "./stores/settingsStore";
  import { toCanonicalExtrusionRate, type ExtUnit } from "./lib/extrusionUnits";
  import { selectedLiquidName } from "./stores/liquidStore";
  import { save, open } from "@tauri-apps/plugin-dialog";
  import { getVersion } from "@tauri-apps/api/app";
  import { writeTextFile, readTextFile } from "@tauri-apps/plugin-fs";
  import { check } from "@tauri-apps/plugin-updater";
  import WelcomeModal from "./components/WelcomeModal.svelte";
  import BracketExportModal from "./components/BracketExportModal.svelte";
  import PrintPauseModal from "./components/PrintPauseModal.svelte";

  // --- STAV APLIKACE ---
  const isTauri = typeof window !== "undefined" && window.__TAURI_INTERNALS__ !== undefined;
  let showWelcomeModal = $state(isTauri);

  // Sněžení aktivní mezi 15. 11. a 30. 1.
  function isSnowSeason(): boolean {
    const now = new Date();
    const m = now.getMonth() + 1; // 1–12
    const d = now.getDate();
    return m === 12 || m === 1 || (m === 11 && d >= 15) || (m === 1 && d <= 30);
  }
  let showSnow = $derived(isSnowSeason() && !$settingsStore.disable_snow);

  let ws: WebSocket | null = null;
  let wsCleanup: (() => void) | null = null;

  // --- OPRÁVNĚNÝ A DIAGNOSTICKÝ FORMULÁŘ ZPĚTNÉ VAZBY ---
  let showFeedbackModal = $state(false);

  let selectedGlass = $state("");

  // Konfigurace stroje — jediným zdrojem pravdy je settingsStore
  let globalBedX = $derived($settingsStore.bed_max_x || 250.0);
  let globalBedY = $derived($settingsStore.bed_max_y || 210.0);
  let globalStartOffsetX = $derived($settingsStore.start_offset_x || 18.0);
  let globalStartOffsetY = $derived($settingsStore.start_offset_y || 11.0);
  let globalMultiSpacing = $derived($settingsStore.multi_spacing || 5.0);
  let globalBlockHeight = $derived($settingsStore.block_height || 34.0);

  let generatedGCode = "";
  let totalDist = 0;
  let totalTime = 0;
  let showGCodeModal = false;
  let gcodeError = "";

  // Tisková pozice trysky z telemetry smyčky
  let currentNozzle: Point2D | null = null;

  // Modály
  let showSettingsModal = $state(false);
  let settingsModalRef: any = $state();
  let showDiagnosticsModal = $state(false);
  let showAboutModal = $state(false);
  let showShortcutsModal = $state(false);
  let showUpdateModal = $state(false);
  let updateModalAutoCheck = $state(false);
  let showBracketExportModal = $state(false);

  let leftPanelRef: any = $state();
  let pauseModalRef: PrintPauseModal = $state() as any;

  // Canvas ↔ pravý panel
  let canvasExternalSelected = $state(-1); // posIdx → zvýrazní sklíčko v plátně
  let canvasExternalObjIdx = $state(-1); // index objektu → zvýrazní trasu v plátně
  let rightPanelOpenIdx = $state(-1); // sampleIdx → otevře accordion
  let rightPanelTrigger = $state(0); // inkrementuje se při každém canvas kliku

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

  let extrusionRateUl = $derived($projectStore.params
    ? toCanonicalExtrusionRate(
        $projectStore.params.extrusion_rate,
        $projectStore.params.extrusion_unit as ExtUnit,
        $settingsStore?.calibration_factor ?? 0.323877
      )
    : 0);

  // Reaktivní re-parse SVG/DXF při změně jemnosti křivek
  let _prevFineness = $state(1.0);
  run(() => {
    const f = $settingsStore.path_fineness ?? 1.0;
    if (f !== _prevFineness && $projectStore.sources.length > 0) {
      _prevFineness = f;
      projectStore.reparseRaw(f);
    }
  });

  /** Zjistí, zda se import vejde na substrát; při přesahu zapne auto-scale
   *  na jeho zdroji (a tím i na objektech, které z něj vznikly). */
  function checkBoundsAgainstNozzle(
    paths: SubstratePaths,
    sourceId: string | null,
    silent = false
  ): boolean {
    const bbox = getBoundingBoxOfPaths(paths.segments);
    if (!bbox.hasPoints) return false;
    const width = bbox.maxX - bbox.minX;
    const height = bbox.maxY - bbox.minY;
    const nozzleDiam = $projectStore.params.nozzle_diam ?? 0.4;
    const usableW = $projectStore.params.slide_w - nozzleDiam;
    const usableH = $projectStore.params.slide_h - nozzleDiam;
    const overflows = width > usableW || height > usableH;
    if (sourceId) projectStore.setSourceAutoScale(sourceId, overflows);
    if (overflows && !silent) {
      alert(
        `Upozornění: Objekt (${width.toFixed(1)} × ${height.toFixed(1)} mm) přesahuje tisknutelnou plochu substrátu s ohledem na průměr trysky ${nozzleDiam} mm.\n` +
          `Dostupná plocha: ${usableW.toFixed(1)} × ${usableH.toFixed(1)} mm.\n\n` +
          `Objekt byl automaticky zmenšen.`
      );
    }
    return overflows;
  }

  // Zkontroluje, zda by aktuálně umístěný objekt přesáhl okraj sklíčka
  // s insetem nového průměru trysky. Ptá se uživatele před zmenšením.
  async function handleNozzleDiamGrew(newDiam: number) {
    const state = $projectStore;
    if (state.objects.length === 0 || state.objects.every((o) => o.auto_scale)) return;

    const nonPrimePositions = state.positions.filter((p) => !p.is_prime);
    const anyOverflow = await check_paths_overflow(state.slides, nonPrimePositions, newDiam);

    if (!anyOverflow) return;

    const confirmed = await pauseModalRef.confirmOrCancel(
      `Se zvolenou tryskou (∅ ${newDiam} mm) by tisknutá trasa přesáhla okraj substrátu ` +
        `a tryska by se dotkla jeho stěny. Chcete objekty automaticky zmenšit?`
    );

    if (confirmed) {
      for (const src of state.sources) projectStore.setSourceAutoScale(src.id, true);
    }
  }

  // Reaktivní kontrola při změně trysky — spustí se jen při zvětšení průměru
  let _prevNozzleDiam = $state($projectStore.params.nozzle_diam);
  run(() => {
    const nd = $projectStore.params.nozzle_diam;
    if (nd !== _prevNozzleDiam) {
      const grew = nd > _prevNozzleDiam;
      _prevNozzleDiam = nd;
      if (grew) handleNozzleDiamGrew(nd);
    }
  });

  // Spustí generování G-kódu na Rust backendu.
  // overrideStartGcode: pokud předáno, použije se místo start_gcode ze settings
  // (předej "" pokud byl start_gcode již odeslán v pre-kalibrační fázi)
  // skipZShiftSetup: true = generátor neemituje blok virtuálního posunu Z
  // (kalibrační tok ho už nastavil sám přes G92)
  async function triggerGCodeGeneration(overrideStartGcode?: string, silent = false, skipZShiftSetup = false) {
    gcodeError = "";
    try {
      // Settings čteme ze store (jediný zdroj pravdy v paměti) — žádné čtení
      // z disku při každém debounced přepočtu statistik.
      const setts = $settingsStore;
      let startGcode = overrideStartGcode !== undefined ? overrideStartGcode : (setts.start_gcode ?? "");
      const endGcode = setts.end_gcode ?? "";
      const loopStartGcode = setts.loop_start_gcode ?? "";
      const loopEndGcode = setts.loop_end_gcode ?? "";
      const calFactor = setts.calibration_factor ?? 0.0141;
      const zHop = setts.default_z_hop ?? 2.0;
      const safeZ = setts.safe_z ?? 20.0;

      const currentParams = $projectStore.params;
      if (!currentParams.bed_leveling) {
        // Bez bed levelingu: G28 → G28 W (obnoví mesh z paměti, neprobuje znovu)
        startGcode = startGcode.replace(/\bG28\b(?!\s*[XYZW])/g, "G28 W");
      }
      const currentSlides = $projectStore.slides;
      const currentOverrides = $projectStore.overrides;

      // Rust expects z_offset in mm — convert if the UI unit is µm
      const paramsForRust = currentParams.z_unit === "µm"
        ? { ...currentParams, z_offset: currentParams.z_offset / 1000.0 }
        : currentParams;

      const res = await generate_gcode(currentSlides, paramsForRust, currentOverrides, {
        bed: {
          max_x: globalBedX,
          max_y: globalBedY,
          min_x: setts.bed_min_x ?? 0.0,
          offset_x: globalStartOffsetX,
          offset_y: globalStartOffsetY,
        },
        start_gcode: startGcode,
        end_gcode: endGcode,
        loop_start_gcode: loopStartGcode,
        loop_end_gcode: loopEndGcode,
        nozzle_change_gcode: setts.nozzle_change_gcode ?? "",
        print_order: setts.print_order ?? "nozzle_major",
        route_mode: setts.route_mode ?? "no_cross",
        multi_spacing: globalMultiSpacing,
        block_height: globalBlockHeight,
        calibration_factor: calFactor,
        z_hop: zHop,
        safe_z: safeZ,
        bed_max_temp: setts.bed_max_temp ?? null,
        skip_z_shift_setup: skipZShiftSetup,
      });

      generatedGCode = res.gcode;
      totalDist = res.total_dist;
      totalTime = res.total_time;
      // Uložíme výsledek do projectStore, aby ho LeftPanel měl aktuální
      projectStore.setGCodeResult(res.gcode, res.total_dist, res.total_time);
      return { gcode: generatedGCode, dist: totalDist, time: totalTime };
    } catch (e) {
      gcodeError = `Generování selhalo: ${e}`;
      if (!silent) alert(gcodeError);
      throw e;
    }
  }

  // Tiché přepočítání statistik G-kódu (dráha, čas, objem) — odloženo, spouští se
  // při jakékoliv změně parametrů/transformací/cest, aby statistiky v Canvasu byly vždy aktuální.
  let statsRefreshTimer: ReturnType<typeof setTimeout>;
  function scheduleStatsRefresh() {
    clearTimeout(statsRefreshTimer);
    statsRefreshTimer = setTimeout(() => {
      triggerGCodeGeneration(undefined, true).catch(() => {});
    }, 400);
  }

  run(() => {
    $projectStore.params;
    $projectStore.overrides;
    $projectStore.slides;
    $projectStore.positions;
    if ($projectStore.slides.length > 0) scheduleStatsRefresh();
  });

  export async function generateGCodeSilently(overrideStartGcode?: string, skipZShiftSetup = false) {
    return await triggerGCodeGeneration(overrideStartGcode, false, skipZShiftSetup);
  }

  // Otevření dialogu souboru přes Tauri API
  async function checkUnsavedChanges(): Promise<boolean> {
    if ($projectStore.isDirty) {
      return await pauseModalRef.confirmOrCancel("Máte neuložené změny. Opravdu chcete pokračovat a změny zahodit?");
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

  /** Přidá další objekt ke stávající scéně (na rozdíl od „Načíst vzorek“,
   *  které scénu nahradí). Objekt se založí na všech substrátech naráz. */
  async function triggerAddObject() {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "Vektorová grafika", extensions: ["svg", "dxf"] }],
      });
      if (!selected || typeof selected !== "string") return;
      const text = await readTextFile(selected);
      const fileName = selected.split(/[/\\]/).pop() || "";
      const ext = fileName.split(".").pop()?.toLowerCase() ?? "";
      if (ext !== "svg" && ext !== "dxf") {
        alert(`Jako objekt lze přidat jen SVG nebo DXF (zvoleno: .${ext}).`);
        return;
      }
      showWelcomeModal = false;
      await projectStore.addImportFromText(text, ext, fileName, false);
      const src = $projectStore.sources[$projectStore.sources.length - 1];
      const obj = $projectStore.objects.find((o) => o.sourceId === src?.id);
      if (obj) checkBoundsAgainstNozzle({ segments: obj.raw.segments }, src.id);
    } catch (e) {
      console.error("Chyba při přidávání objektu:", e);
      alert("Nepodařilo se přidat objekt: " + e);
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
        case "svg":
        case "dxf": {
          const parsed = ext === "svg" ? await parse_svg(text, fineness) : await parse_dxf(text);
          await projectStore.addImport(parsed, {
            fileName,
            ext,
            text,
            fineness,
            replace: true,
          });
          checkBoundsAgainstNozzle(parsed, $projectStore.sources[0]?.id ?? null);
          projectStore.setProjectSaved(null);
          break;
        }
        case "gcode": {
          projectStore.setProjectSaved(absolutePath);
          const meta = await parse_gcode_metadata(text);
          if (meta) {
            await projectStore.restoreFromGCode(meta, fineness);
          } else {
            // Externí GCode bez DPI metadat — načti jako vizualizaci dráhy
            const parsed = await parse_gcode_file_paths(text);
            await projectStore.addImport(parsed, {
              fileName,
              ext: "gcode",
              text,
              fineness,
              replace: true,
            });
            checkBoundsAgainstNozzle(parsed, $projectStore.sources[0]?.id ?? null);
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
    // Projekt s jediným objektem zapisujeme i ve formátu v2 (source_file_* +
    // transforms), aby ho otevřela i starší verze aplikace.
    const single = state.objects.length === 1 ? state.objects[0] : null;
    const legacyRows = single ? state.slides.filter((row) => row.length === 1) : [];
    const singleSrc =
      single && legacyRows.length === state.slides.length
        ? state.sources.find((s) => s.id === single.sourceId)
        : null;
    const meta: GCodeMetadata = {
      params: state.params,
      overrides: state.overrides,
      sources: state.sources.map((s) => ({
        id: s.id,
        name: s.name,
        ext: s.ext,
        content: s.text,
        auto_scale: s.autoScale,
        fineness: s.fineness,
      })),
      objects: state.objects.map((o) => ({
        id: o.id,
        source_id: o.sourceId,
        index: o.index,
        name: o.name,
      })),
      slides: state.slides.map((row) =>
        row.map((p) => ({
          object_id: p.object_id,
          enabled: p.enabled,
          transform: p.transform,
          baked_scale: p.baked_scale,
          nozzle_type: p.nozzle_type ?? null,
          nozzle_diam: p.nozzle_diam ?? null,
        }))
      ),
      transforms: singleSrc ? legacyRows.map((row) => row[0].transform) : [],
      baked_scales: singleSrc ? legacyRows.map((row) => row[0].baked_scale) : [],
      source_file_name: singleSrc ? singleSrc.name : "",
      source_file_ext: singleSrc ? singleSrc.ext : "",
      source_file_content: singleSrc ? singleSrc.text : "",
      auto_scale: singleSrc ? singleSrc.autoScale : false,
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
    const appVersion = await getVersion().catch(() => "");
    const csvContent = await generate_csv_protocol(
      $projectStore.params,
      $projectStore.overrides,
      $projectStore.totalDist,
      $projectStore.totalTime,
      selectedGlass || "",
      `DPI ${appVersion}`,
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
    if (!(await checkUnsavedChanges())) return;
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
    await settingsStore.load();
    projectStore.triggerLayoutUpdate();
  }

  // Globální klávesové zkratky
  function handleKeyDown(event: KeyboardEvent) {
    if (event.ctrlKey && event.key.toLowerCase() === "o") {
      event.preventDefault();
      if (event.shiftKey) triggerAddObject();
      else triggerLoadFileInput();
    } else if (event.ctrlKey && event.key.toLowerCase() === "s") {
      event.preventDefault();
      if (event.shiftKey) {
        saveProjectAs();
      } else {
        saveProject();
      }
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

  // Reaktivní sledování tažení objektů na Canvasu
  function handleTransformChanged(
    event: CustomEvent<{ slideIdx: number; objIdx: number; transform: Transform }>
  ) {
    const { slideIdx, objIdx, transform } = event.detail;
    projectStore.updateTransform(slideIdx, objIdx, transform);
  }

  /** Odebrání objektu z jednoho skla (Delete / kontextová nabídka) — vratné. */
  function handleObjectHidden(event: CustomEvent<{ slideIdx: number; objIdx: number }>) {
    const { slideIdx, objIdx } = event.detail;
    projectStore.setObjectEnabled(slideIdx, objIdx, false);
  }

  onMount(async () => {
    // Odstranění loading screenu
    const loader = document.querySelector(".loading-screen");
    if (loader) loader.remove();

    // WebSocket synchronizace stavu — jednosměrná:
    // Tauri host stav pouze publikuje (je zdrojem pravdy), webový klient
    // na LAN pouze přijímá (server zprávy z LAN zahazuje, viz sync.rs).
    const host = isTauri ? "127.0.0.1" : window.location.hostname;

    // Reconnect s exponenciálním backoffem — server může startovat později
    // (obsazený port) nebo spojení může spadnout, náhled se pak sám obnoví.
    const WS_RECONNECT_BASE_MS = 2000;
    const WS_RECONNECT_MAX_MS = 30000;
    let wsRetryDelay = WS_RECONNECT_BASE_MS;
    let wsReconnectTimer: ReturnType<typeof setTimeout> | undefined;
    let wsStopped = false;

    // Throttle publikace: stav se serializuje (JSON celých drah) nejvýš
    // jednou za interval — tažení sklíčka jinak publikuje každý pohyb myši.
    const WS_PUBLISH_INTERVAL_MS = 200;
    let publishTimer: ReturnType<typeof setTimeout> | undefined;
    let pendingState: ProjectState | null = null;

    function publishState(state: ProjectState) {
      if (!ws || ws.readyState !== WebSocket.OPEN) return;
      // Těžká pole (zdrojový soubor, vygenerovaný G-kód) vzdálený náhled
      // nepotřebuje — neposíláme je při každé změně po síti.
      // Těžká pole (zdrojové soubory, surová geometrie objektů, G-kód) vzdálený
      // náhled nepotřebuje — kreslí z `slides[].path`.
      const lean = {
        ...state,
        sources: state.sources.map((s) => ({ ...s, text: "" })),
        objects: state.objects.map((o) => ({ ...o, raw: { segments: [] } })),
        generatedGCode: "",
      };
      ws.send(JSON.stringify(lean));
    }

    function connectWs() {
      if (wsStopped) return;
      ws = new WebSocket(`ws://${host}:5174`);
      ws.onopen = () => {
        wsRetryDelay = WS_RECONNECT_BASE_MS;
        // Po (re)připojení hned publikuj aktuální stav, ať náhled nečeká na změnu
        if (isTauri) publishState(pendingState ?? $projectStore);
      };
      ws.onclose = () => {
        if (wsStopped) return;
        clearTimeout(wsReconnectTimer);
        wsReconnectTimer = setTimeout(connectWs, wsRetryDelay);
        wsRetryDelay = Math.min(wsRetryDelay * 2, WS_RECONNECT_MAX_MS);
      };
      if (!isTauri) {
        ws.onmessage = (event) => {
          try {
            projectStore.set(JSON.parse(event.data));
          } catch (e) {
            console.error("Failed to parse state from WS", e);
          }
        };
      }
    }
    connectWs();

    let unsubWs: (() => void) | null = null;
    if (isTauri) {
      unsubWs = projectStore.subscribe((state) => {
        pendingState = state;
        if (publishTimer) return;
        publishTimer = setTimeout(() => {
          publishTimer = undefined;
          if (pendingState) publishState(pendingState);
        }, WS_PUBLISH_INTERVAL_MS);
      });
    }
    wsCleanup = () => {
      wsStopped = true;
      clearTimeout(wsReconnectTimer);
      clearTimeout(publishTimer);
      unsubWs?.();
      ws?.close();
    };

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
          const confirmed = await pauseModalRef.confirmOrCancel("Máte neuložené změny. Opravdu chcete aplikaci zavřít?");
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
      await settingsStore.load();
    } catch (e) {
      console.warn("Nepodařilo se načíst nastavení z Tauri (Web Mode):", e);
    }

    projectStore.triggerLayoutUpdate();

    const openFeedback = () => {
      showFeedbackModal = true;
    };
    window.addEventListener("open-feedback-form", openFeedback);
    const prevCleanup = wsCleanup;
    wsCleanup = () => {
      prevCleanup?.();
      window.removeEventListener("open-feedback-form", openFeedback);
    };
  });

  // Cleanup nelze vracet z async onMount callbacku (Svelte ho ignoruje) —
  // proto explicitní onDestroy.
  onDestroy(() => {
    wsCleanup?.();
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

<svelte:window onkeydown={handleKeyDown} />

<main
  class="w-screen h-screen flex flex-col bg-labdark text-slate-100 overflow-hidden font-sans p-3 gap-3"
>
  <!-- APPLIKAČNÍ HLAVIČKA -->
  <Header
    {isTauri}
    onResetProject={resetProject}
    onTriggerLoadFileInput={triggerLoadFileInput}
    onAddObject={triggerAddObject}
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
        on:generateGCode={() => triggerGCodeGeneration()}
        {generateGCodeSilently}
        pauseModal={pauseModalRef}
      />
    </div>

    <!-- STŘEDNÍ ČÁST -->
    <div class="col-span-6 overflow-hidden h-full flex flex-col">
      <GraphicsView
        bedMaxX={globalBedX}
        bedMaxY={globalBedY}
        positions={$projectStore.positions}
        slides={$projectStore.slides}
        objectNames={$projectStore.objects.map((o) => o.name)}
        primePath={$projectStore.primePath}
        {currentNozzle}
        nozzleDiam={$projectStore.params ? $projectStore.params.nozzle_diam : 0.4}
        overrides={$projectStore.overrides}
        externalSelectedIndex={canvasExternalSelected}
        externalSelectedObjIdx={canvasExternalObjIdx}
        totalPreviewTime={$projectStore.totalTime ?? 0}
        {extrusionRateUl}
        suspendAutoCenter={showWelcomeModal}
        on:transformChanged={handleTransformChanged}
        on:objectHidden={handleObjectHidden}
        on:saveState={() => projectStore.pushState()}
        on:pathRebuildNeeded={(e) =>
          projectStore.rebuildSlicePath(
            e.detail.slideIdx,
            e.detail.objIdx,
            e.detail.scale,
            e.detail.rotation
          )}
        on:objectSelected={(e) => (canvasExternalObjIdx = e.detail)}
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
        slides={$projectStore.slides}
        objects={$projectStore.objects}
        selectedObjIdx={canvasExternalObjIdx}
        globalNozzleDiam={$projectStore.params ? $projectStore.params.nozzle_diam : 0.4}
        pauseModal={pauseModalRef}
        on:slideActivated={(e) => {
          canvasExternalSelected = sampleToPositionIdx(e.detail);
        }}
        on:objectSelected={(e) => {
          canvasExternalSelected = sampleToPositionIdx(e.detail.slideIdx);
          canvasExternalObjIdx = e.detail.objIdx;
        }}
      />
    </div>
  </div>

  <!-- ADVANCED SETTINGS MODAL -->
  <SettingsModal bind:this={settingsModalRef} bind:isOpen={showSettingsModal} on:save={handleSettingsSave} />

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
    on:close={() => (showWelcomeModal = false)}
    on:openRecent={async (e) => {
      showWelcomeModal = false;
      await loadFileFromPath(e.detail);
    }}
  />

  {#if showSnow}
    <SnowEffect />
  {/if}

  <PrintPauseModal bind:this={pauseModalRef} />
</main>
