<script lang="ts">
  import { run } from 'svelte/legacy';

  import { createEventDispatcher, onMount } from "svelte";
  import CameraWidget from "./CameraWidget.svelte";
  import ManualMovementWidget from "./ManualMovementWidget.svelte";
  import CollapsibleBox from "./CollapsibleBox.svelte";
  import Terminal from "./Terminal.svelte";
  import CustomSelect from "./CustomSelect.svelte";
  import NumberInput from "./NumberInput.svelte";
  import {
    Settings2,
    RotateCcw,
    AlignJustify,
    Rows4,
    Route,
    Grip,
    Grid,
    Square,
  } from "lucide-svelte";
  import { subscribe_printer_status, type SlideOverride, type PrinterStatus } from "../lib/tauri";
  import { liquidLimits } from "../stores/liquidStore";
  import { cameraAvailable } from "../stores/cameraStore";
  import { settingsStore } from "../stores/settingsStore";
  import { convertExtrusionRate, toCanonicalExtrusionRate, type ExtUnit } from "../lib/extrusionUnits";

  interface Props {
    sampleCount?: number;
    primeActive?: boolean;
    overrides?: Record<string, SlideOverride>;
    /** openSlideIdx + openTrigger: canvas → panel. Trigger se inkrementuje i při
     *  opakovaném výběru stejného sklíčka, aby se reactive blok vždy spustil. */
    openSlideIdx?: number;
    openTrigger?: number;
  }

  let {
    sampleCount = 1,
    primeActive = false,
    overrides = $bindable({}),
    openSlideIdx = -1,
    openTrigger = 0
  }: Props = $props();

  const dispatch = createEventDispatcher();

  interface LocalSlideData {
    name: string;
    note: string;
    z_offset: string;
    z_unit: "mm" | "µm";
    extrusion_rate: string;
    extrusion_unit: "µl/mm" | "nl/mm" | "kroky/mm";
    print_speed: string;
    infill_val: string;
    infill_type: "mm" | "%" | "počet";
    nozzle_height: string;
    infill_style: string;

    // Zda jsou hodnoty lokálně změněné (modified)
    z_modified: boolean;
    ext_modified: boolean;
    speed_modified: boolean;
    infill_modified: boolean;
    nozzle_modified: boolean;
    infill_style_modified: boolean;
  }

  let localSlides: LocalSlideData[] = $state([]);
  let openSlides: boolean[] = $state([]);

  // ─── Limity aktivní kapaliny pro override vstupy ──────────────────────────
  let liqExtMin = $derived($liquidLimits?.extrusion_min ?? 0);
  let liqExtMax = $derived($liquidLimits?.extrusion_max ?? 1000);
  let liqSpeedMin = $derived($liquidLimits?.print_speed_min ?? 50);
  let liqSpeedMax = $derived($liquidLimits?.print_speed_max ?? 1500);

  // Udržuje openSlides v souladu s počtem sklíček
  run(() => {
    if (openSlides.length !== sampleCount) {
      openSlides = Array(Math.max(sampleCount, 0)).fill(false);
    }
  });

  // Otevření accordionu z plátna — _lastHandledTrigger brání reaktivní smyčce
  // (dřív afterUpdate; reaktivní blok se stejnou strážní podmínkou je ekvivalentní)
  let _lastHandledTrigger = $state(-1);
  run(() => {
    if (openTrigger !== _lastHandledTrigger && openTrigger > 0) {
      _lastHandledTrigger = openTrigger;
      if (openSlideIdx >= 0 && openSlideIdx < sampleCount) {
        openSlides = Array.from({ length: sampleCount }, (_, i) => i === openSlideIdx);
      }
    }
  });

  function handleSlideToggle(idx: number, nowOpen: boolean) {
    if (nowOpen) {
      openSlides = openSlides.map((_, i) => i === idx);
      dispatch("slideActivated", idx);
    } else {
      openSlides = openSlides.map((v, i) => (i === idx ? false : v));
    }
  }

  let primeSlide = $state({
    width: "15",
    infill_val: "1.5",
    infill_type: "mm" as "mm" | "%" | "počet",
    extrusion_rate: "",
    extrusion_unit: "nl/mm" as "µl/mm" | "nl/mm" | "kroky/mm",
    glass_type: "laboratorní" as "laboratorní" | "vzorkové",
    modified: true,
  });

  function handlePrimeInput() {
    primeSlide.modified = true;
    updateParentOverrides();
  }

  let isManualMovementOpen = $state(false);
  let lastCanControl = false;

  onMount(() => {
    updateParentOverrides();

    const unsubscribe = subscribe_printer_status((status) => {
      const canControl = status.is_connected && !status.is_printing;
      if (canControl !== lastCanControl) {
        lastCanControl = canControl;
        if (canControl) {
          isManualMovementOpen = true;
        } else {
          isManualMovementOpen = false;
        }
      }
    });

    return () => {
      unsubscribe.then(unsub => unsub());
    };
  });

  function extUnitFromEvent(e: CustomEvent): "µl/mm" | "nl/mm" | "kroky/mm" {
    return e.detail.value;
  }
  function zUnitFromEvent(e: CustomEvent): "mm" | "µm" {
    return e.detail.value;
  }

  function handlePrimeExtUnitChange(
    newUnit: ExtUnit,
    oldUnit: ExtUnit
  ) {
    if (!primeSlide.extrusion_rate || oldUnit === newUnit) return;
    const val = parseFloat(primeSlide.extrusion_rate);
    if (!isNaN(val)) {
      const calFactor = $settingsStore.calibration_factor;
      primeSlide.extrusion_rate = convertExtrusionRate(val, oldUnit, newUnit, calFactor).toFixed(4);
      handlePrimeInput();
    }
  }

  // Inicializujeme místní strukturu pro všechna sklíčka
  run(() => {
    let changed = false;
    for (let i = 0; i < sampleCount; i++) {
      if (!localSlides[i]) {
        localSlides[i] = {
          name: "",
          note: "",
          z_offset: "",
          z_unit: "µm",
          extrusion_rate: "",
          extrusion_unit: "nl/mm",
          print_speed: "",
          infill_val: "",
          infill_type: "mm",
          nozzle_height: "",
          infill_style: "",

          z_modified: false,
          ext_modified: false,
          speed_modified: false,
          infill_modified: false,
          nozzle_modified: false,
          infill_style_modified: false,
        };
        changed = true;
      }
    }
    // Oříznout přebytečná sklíčka, pokud se sampleCount zmenšil
    if (localSlides.length > sampleCount) {
      localSlides = localSlides.slice(0, sampleCount);
      changed = true;
    }

    if (changed) {
      localSlides = [...localSlides];
    }
  });

  // Přepočet a odeslání do parenta při změně vstupu
  function handleInput(idx: number, field: keyof LocalSlideData) {
    const slide = localSlides[idx];
    if (!slide) return;

    // Označíme pole jako upravené
    if (field === "z_offset") slide.z_modified = true;
    else if (field === "extrusion_rate") slide.ext_modified = true;
    else if (field === "print_speed") slide.speed_modified = true;
    else if (field === "infill_val") slide.infill_modified = true;
    else if (field === "nozzle_height") slide.nozzle_modified = true;
    else if (field === "infill_style") slide.infill_style_modified = true;

    updateParentOverrides();
  }

  // Zrušení lokálních změn a návrat ke globálnímu nastavení
  function resetSlide(idx: number) {
    const slide = localSlides[idx];
    if (!slide) return;

    slide.name = "";
    slide.note = "";
    slide.z_offset = "";
    slide.z_unit = "µm";
    slide.extrusion_rate = "";
    slide.extrusion_unit = "nl/mm";
    slide.print_speed = "";
    slide.infill_val = "";
    slide.infill_type = "mm";
    slide.nozzle_height = "";
    slide.infill_style = "";

    slide.z_modified = false;
    slide.ext_modified = false;
    slide.speed_modified = false;
    slide.infill_modified = false;
    slide.nozzle_modified = false;
    slide.infill_style_modified = false;

    localSlides = [...localSlides];
    updateParentOverrides();
  }

  // Přepočet tloušťky vrstvy při změně jednotek
  function handleZUnitChange(idx: number, newUnit: "mm" | "µm") {
    const slide = localSlides[idx];
    if (!slide || !slide.z_offset) return;

    let val = parseFloat(slide.z_offset);
    if (!isNaN(val)) {
      if (newUnit === "µm") {
        // mm -> µm
        slide.z_offset = (val * 1000.0).toFixed(1);
      } else {
        // µm -> mm
        slide.z_offset = (val / 1000.0).toFixed(3);
      }
      localSlides = [...localSlides];
      handleInput(idx, "z_offset");
    }
  }

  // Přepočet extruze při změně jednotek
  function handleExtUnitChange(
    idx: number,
    newUnit: ExtUnit,
    oldUnit: ExtUnit
  ) {
    const slide = localSlides[idx];
    if (!slide || !slide.extrusion_rate || oldUnit === newUnit) return;

    const val = parseFloat(slide.extrusion_rate);
    if (!isNaN(val)) {
      const calFactor = $settingsStore.calibration_factor;
      const newVal = convertExtrusionRate(val, oldUnit, newUnit, calFactor);
      slide.extrusion_rate = newUnit === "kroky/mm" ? newVal.toFixed(1) : newVal.toFixed(4);
      localSlides = [...localSlides];
      handleInput(idx, "extrusion_rate");
    }
  }

  // Sestavení finálního objektu overrides pro backend
  function updateParentOverrides() {
    const result: Record<string, SlideOverride> = {};
    for (let i = 0; i < sampleCount; i++) {
      const slide = localSlides[i];
      if (!slide) continue;

      const slide_data: SlideOverride = {};

      if (slide.name) slide_data.name = slide.name;
      if (slide.note) slide_data.note = slide.note;

      // Z-offset
      if (slide.z_modified && slide.z_offset !== "") {
        let z = parseFloat(slide.z_offset);
        if (!isNaN(z)) {
          if (slide.z_unit === "µm") z /= 1000.0;
          slide_data.z_offset = z;
        }
      }

      // Extruze
      if (slide.ext_modified && slide.extrusion_rate !== "") {
        const ext = parseFloat(slide.extrusion_rate);
        if (!isNaN(ext)) {
          slide_data.extrusion_rate = toCanonicalExtrusionRate(
            ext, slide.extrusion_unit, $settingsStore.calibration_factor
          );
          slide_data.extrusion_unit = "µl/mm";
        }
      }

      // Speed
      if (slide.speed_modified && slide.print_speed !== "") {
        let spd = parseFloat(slide.print_speed);
        if (!isNaN(spd)) slide_data.print_speed = spd;
      }

      // Infill (Hustota)
      if (slide.infill_modified && slide.infill_val !== "") {
        let inf = parseFloat(slide.infill_val);
        if (!isNaN(inf)) {
          slide_data.infill_val = inf;
          slide_data.infill_type = slide.infill_type;
        }
      }

      // Výška trysky
      if (slide.nozzle_modified && slide.nozzle_height !== "") {
        let nz = parseFloat(slide.nozzle_height);
        if (!isNaN(nz)) slide_data.nozzle_height = nz;
      }

      // Styl infillu
      if (slide.infill_style_modified && slide.infill_style !== "") {
        slide_data.infill_style = slide.infill_style;
      }

      if (Object.keys(slide_data).length > 0) {
        result[i.toString()] = slide_data;
      }
    }

    // Odpliv overrides (key "-1")
    if (primeSlide.modified) {
      let pw = parseFloat(primeSlide.width);
      let ph = pw; // v2: vždy čtverec

      const p_over: SlideOverride = {};
      if (!isNaN(pw)) {
        p_over.slide_w = pw;
        p_over.slide_h = ph;
      }
      p_over.glass_type = primeSlide.glass_type;

      if (primeSlide.infill_val !== "") {
        let inf = parseFloat(primeSlide.infill_val);
        if (!isNaN(inf)) {
          p_over.infill_val = inf;
          p_over.infill_type = primeSlide.infill_type;
        }
      }

      if (primeSlide.extrusion_rate !== "") {
        const ext = parseFloat(primeSlide.extrusion_rate);
        if (!isNaN(ext)) {
          p_over.extrusion_rate = toCanonicalExtrusionRate(
            ext, primeSlide.extrusion_unit, $settingsStore.calibration_factor
          );
          p_over.extrusion_unit = "µl/mm";
        }
      }

      result["-1"] = p_over;
    }

    overrides = result;
    dispatch("overridesChanged", result);
  }
</script>

<div
  class="glass-panel rounded-lg p-3 flex flex-col gap-3 h-full overflow-hidden text-xs select-text"
>
  <!-- CAMERA PREVIEW (At the top) -->
  {#if $cameraAvailable}
    <CameraWidget />
  {/if}

  <!-- MANUAL MOVEMENT GRID (In a collapsible box) -->
  <CollapsibleBox
    title="RUČNÍ OVLÁDÁNÍ POSUVU"
    bind:isOpen={isManualMovementOpen}
    headerClass="text-sm font-extrabold uppercase text-slate-200 tracking-wide"
  >
    <ManualMovementWidget />
  </CollapsibleBox>

  <!-- SCROLLABLE LIST OF ALL SLIDES -->
  <div
    class="flex-1 flex flex-col gap-2 overflow-y-auto pr-0.5 min-h-0 border-t border-slate-800/40 pt-2"
  >
    <div
      class="flex items-center gap-1.5 text-sm font-extrabold uppercase tracking-wider text-slate-200 border-b border-slate-700/50 pb-2 mb-1"
    >
      <Settings2 class="w-4 h-4 text-labaccent" />
      <span>Lokální nastavení</span>
    </div>

    <div class="flex flex-col gap-2">
      <!-- ODPLIV KONFIGURACE -->
      {#if primeActive}
      <div class="mb-4">
        <CollapsibleBox
          title="Odpliv (Prime)"
          isOpen={false}
          headerClass="text-sm font-extrabold text-orange-400 tracking-wide bg-orange-500/10"
          containerClass="border-orange-500/50 bg-orange-500/5"
        >
          <div class="flex flex-col gap-3 p-3">
            <!-- Typ skla -->
            <div class="grid grid-cols-3 items-center gap-2">
              <span class="text-orange-200/80">Typ substrátu:</span>
              <div class="col-span-2 h-7">
                <CustomSelect
                  bind:value={primeSlide.glass_type}
                  on:change={handlePrimeInput}
                  options={[
                    { value: "laboratorní", label: "Laboratorní" },
                    { value: "vzorkové", label: "Vzorkové" },
                  ]}
                  cssStyle="height: 100%; font-size: 11px; background-color: rgba(67, 20, 7, 0.5); color: #ffedd5; border-color: rgba(249, 115, 22, 0.3);"
                />
              </div>
            </div>

            <!-- Velikost -->
            <div class="grid grid-cols-3 items-center gap-2">
              <span class="text-orange-200/80">Velikost čtverce [mm]:</span>
              <div class="col-span-2 h-7">
                <NumberInput
                  min={1}
                  step={1}
                  max={25}
                  bind:value={primeSlide.width}
                  on:input={handlePrimeInput}
                  placeholder="Šířka"
                  class="w-full h-full text-[11px] bg-orange-950/50 text-orange-100 border-orange-500/30"
                />
              </div>
            </div>

            <!-- Výplň -->
            <div class="grid grid-cols-3 items-center gap-2">
              <span class="text-orange-200/80">Hustota výplně:</span>
              <div class="col-span-2 grid grid-cols-3 gap-1">
                <div class="col-span-2 h-7">
                  <NumberInput
                    min={primeSlide.infill_type === "počet" ? 1 : 0.001}
                    step={primeSlide.infill_type === "počet" ? 1 : 0.1}
                    bind:value={primeSlide.infill_val}
                    on:input={() => {
                      if (primeSlide.infill_type === "počet")
                        primeSlide.infill_val = Math.max(1, Math.round(Number(primeSlide.infill_val))).toString();
                      handlePrimeInput();
                    }}
                    placeholder="Dle trysky"
                    class="w-full h-full text-[11px] bg-orange-950/50 text-orange-100 border-orange-500/30"
                  />
                </div>
                <div class="col-span-1 h-7">
                  <CustomSelect
                    bind:value={primeSlide.infill_type}
                    on:change={() => {
                      if (primeSlide.infill_type === "počet")
                        primeSlide.infill_val = Math.max(1, Math.round(Number(primeSlide.infill_val))).toString();
                      handlePrimeInput();
                    }}
                    options={[
                      { value: "mm", label: "mm" },
                      { value: "%", label: "%" },
                      { value: "počet", label: "počet" },
                    ]}
                    cssStyle="height: 100%; font-size: 10px; background-color: rgba(67, 20, 7, 0.5); color: #ffedd5; border-color: rgba(249, 115, 22, 0.3);"
                  />
                </div>
              </div>
            </div>

            <!-- Extruze -->
            <div class="grid grid-cols-3 items-center gap-2">
              <span class="text-orange-200/80">Množství:</span>
              <div class="col-span-2 grid grid-cols-3 gap-1">
                <div class="col-span-2 h-7">
                  <NumberInput
                    min={0}
                    step={0.1}
                    bind:value={primeSlide.extrusion_rate}
                    on:input={handlePrimeInput}
                    placeholder="Globální tok"
                    class="w-full h-full text-[11px] bg-orange-950/50 text-orange-100 border-orange-500/30"
                  />
                </div>
                <div class="col-span-1 h-7">
                  <CustomSelect
                    value={primeSlide.extrusion_unit}
                    on:change={(e) => {
                      const newUnit = extUnitFromEvent(e);
                      const oldUnit = primeSlide.extrusion_unit;
                      handlePrimeExtUnitChange(newUnit, oldUnit);
                      primeSlide.extrusion_unit = newUnit;
                    }}
                    options={[
                      { value: "nl/mm", label: "nl/mm" },
                      { value: "kroky/mm", label: "krok" },
                    ]}
                    cssStyle="height: 100%; font-size: 9px; padding-left: 1px; padding-right: 1px; background-color: rgba(67, 20, 7, 0.5); color: #ffedd5; border-color: rgba(249, 115, 22, 0.3);"
                  />
                </div>
              </div>
            </div>
          </div>
        </CollapsibleBox>
      </div>
      {/if}

      <!-- SKLÍČKA SMYČKA -->
      {#each Array(sampleCount) as _, idx}
        {@const _zScale = localSlides[idx]?.z_unit === "µm" ? 1000 : 1}
        {@const _liqZMin = $liquidLimits?.z_offset_min != null ? $liquidLimits.z_offset_min * _zScale : 0}
        {@const _liqZMax = $liquidLimits?.z_offset_max != null ? $liquidLimits.z_offset_max * _zScale : 2.0 * _zScale}
        {#if localSlides[idx]}
          <CollapsibleBox
            title={localSlides[idx].name ? localSlides[idx].name : `Substrát ${idx + 1}`}
            bind:isOpen={openSlides[idx]}
            on:toggle={(e) => handleSlideToggle(idx, e.detail)}
            headerClass="text-sm font-extrabold tracking-wide transition-colors {openSlides[idx]
              ? 'text-labaccent'
              : 'text-slate-200'}"
            containerClass="border bg-slate-950/20 transition-colors {openSlides[idx]
              ? 'border-labaccent/40 bg-labaccent/5'
              : 'border-slate-800'}"
          >
            <!-- FORM LAYOUT INSIDE ACCORDION CARD -->
            <div class="flex flex-col gap-2.5 pt-2">
              <!-- RESET BUTTON -->
              <button
                type="button"
                onclick={() => resetSlide(idx)}
                class="w-full bg-slate-900 border border-slate-700 hover:bg-slate-800 text-[10px] text-slate-300 font-semibold py-1 rounded-sm flex items-center justify-center gap-1 transition-colors"
              >
                <RotateCcw class="w-3 h-3 text-slate-400" /> Zrušit lokální změny
              </button>

              <!-- NÁZEV -->
              <div
                class="grid grid-cols-3 items-center gap-2"
                title="Jméno substrátu pro lepší orientaci v protokolu"
              >
                <span class="text-slate-400">Název:</span>
                <input
                  type="text"
                  bind:value={localSlides[idx].name}
                  oninput={() => handleInput(idx, "name")}
                  placeholder={`Substrát ${idx + 1}`}
                  class="col-span-2 input-premium py-0.5 text-[11px]"
                />
              </div>

              <!-- POZNÁMKA -->
              <div
                class="grid grid-cols-3 items-center gap-2"
                title="Volitelná textová poznámka do protokolu"
              >
                <span class="text-slate-400">Poznámka:</span>
                <input
                  type="text"
                  bind:value={localSlides[idx].note}
                  oninput={() => handleInput(idx, "note")}
                  placeholder="Poznámka..."
                  class="col-span-2 input-premium py-0.5 text-[11px]"
                />
              </div>

              <!-- VÝŠKA TRYSKY (Z) -->
              <div
                class="grid grid-cols-3 items-center gap-2"
                title="Lokální override výšky hlavy (Z-offset) nad podložkou pro tento konkrétní substrát"
              >
                <span
                  class="text-slate-400 {localSlides[idx].z_modified
                    ? 'text-orange-400 font-semibold'
                    : ''}">Výška trysky:</span
                >
                <div class="col-span-2 grid grid-cols-3 gap-1">
                  <div class="col-span-2 h-7">
                    <NumberInput
                      min={_liqZMin}
                      max={_liqZMax}
                      step={localSlides[idx].z_unit === "mm" ? 0.05 : 50}
                      bind:value={localSlides[idx].z_offset}
                      on:input={() => handleInput(idx, "z_offset")}
                      placeholder="Globální Z"
                      class="w-full h-full text-[11px]"
                    />
                  </div>
                  <div class="col-span-1 h-7">
                    <CustomSelect
                      value={localSlides[idx].z_unit}
                      on:change={(e) => {
                        const newUnit = zUnitFromEvent(e);
                        handleZUnitChange(idx, newUnit);
                        localSlides[idx].z_unit = newUnit;
                        localSlides = localSlides;
                      }}
                      options={[
                        { value: "mm", label: "mm" },
                        { value: "µm", label: "µm" },
                      ]}
                      cssStyle="height: 100%; font-size: 10px;"
                    />
                  </div>
                </div>
              </div>

              <!-- EXTRUZE -->
              <div
                class="grid grid-cols-3 items-center gap-2"
                title="Lokální override dávkování kapaliny pro tento substrát"
              >
                <span
                  class="text-slate-400 {localSlides[idx].ext_modified
                    ? 'text-orange-400 font-semibold'
                    : ''}">Extruze:</span
                >
                <div class="col-span-2 grid grid-cols-3 gap-1">
                  <div class="col-span-2 h-7">
                    <NumberInput
                      min={liqExtMin}
                      max={liqExtMax}
                      step={0.1}
                      bind:value={localSlides[idx].extrusion_rate}
                      on:input={() => handleInput(idx, "extrusion_rate")}
                      placeholder="Globální tok"
                      class="w-full h-full text-[11px]"
                    />
                  </div>
                  <div class="col-span-1 h-7">
                    <CustomSelect
                      value={localSlides[idx].extrusion_unit}
                      on:change={(e) => {
                        const newUnit = extUnitFromEvent(e);
                        const oldUnit = localSlides[idx].extrusion_unit;
                        handleExtUnitChange(idx, newUnit, oldUnit);
                        localSlides[idx].extrusion_unit = newUnit;
                        localSlides = localSlides;
                      }}
                      options={[
                        { value: "nl/mm", label: "nl/mm" },
                        { value: "kroky/mm", label: "krok/mm" },
                      ]}
                      cssStyle="height: 100%; font-size: 9px; padding-left: 1px; padding-right: 1px;"
                    />
                  </div>
                </div>
              </div>

              <!-- RYCHLOST -->
              <div
                class="grid grid-cols-3 items-center gap-2"
                title="Lokální override rychlosti tisku"
              >
                <span
                  class="text-slate-400 {localSlides[idx].speed_modified
                    ? 'text-orange-400 font-semibold'
                    : ''}">Rychlost [mm/min]:</span
                >
                <div class="col-span-2 h-7">
                  <NumberInput
                    min={liqSpeedMin}
                    max={liqSpeedMax}
                    step={100}
                    bind:value={localSlides[idx].print_speed}
                    on:input={() => handleInput(idx, "print_speed")}
                    placeholder="Globální rychlost"
                    class="w-full h-full text-[11px]"
                  />
                </div>
              </div>

              <!-- VÝPLŇ (HUSTOTA) -->
              <div
                class="grid grid-cols-3 items-center gap-2"
                title="Lokální override hustoty výplně"
              >
                <span
                  class="text-slate-400 {localSlides[idx].infill_modified
                    ? 'text-orange-400 font-semibold'
                    : ''}">Výplň:</span
                >
                <div class="col-span-2 grid grid-cols-3 gap-1">
                  <div class="col-span-2 h-7">
                    <NumberInput
                      min={localSlides[idx].infill_type === "počet" ? 1 : 0.001}
                      step={localSlides[idx].infill_type === "počet" ? 1 : 0.1}
                      bind:value={localSlides[idx].infill_val}
                      on:input={() => {
                        if (localSlides[idx].infill_type === "počet")
                          localSlides[idx].infill_val = String(
                            Math.max(1, Math.round(Number(localSlides[idx].infill_val)))
                          );
                        handleInput(idx, "infill_val");
                      }}
                      placeholder="Globální výplň"
                      class="w-full h-full text-[11px]"
                    />
                  </div>
                  <div class="col-span-1 h-7">
                    <CustomSelect
                      bind:value={localSlides[idx].infill_type}
                      on:change={() => {
                        if (localSlides[idx].infill_type === "počet")
                          localSlides[idx].infill_val = String(
                            Math.max(1, Math.round(Number(localSlides[idx].infill_val)))
                          );
                        handleInput(idx, "infill_val");
                      }}
                      options={[
                        { value: "mm", label: "mm" },
                        { value: "%", label: "%" },
                        { value: "počet", label: "počet" },
                      ]}
                      cssStyle="height: 100%; font-size: 10px;"
                    />
                  </div>
                </div>
              </div>

              <!-- STYL VÝPLNĚ -->
              <div
                class="grid grid-cols-3 items-center gap-2"
                title="Lokální override stylu výplně (vzoru)"
              >
                <span
                  class="text-slate-400 {localSlides[idx].infill_style_modified
                    ? 'text-orange-400 font-semibold'
                    : ''}">Styl výplně:</span
                >
                <div class="col-span-2">
                  <CustomSelect
                    bind:value={localSlides[idx].infill_style}
                    on:change={() => handleInput(idx, "infill_style")}
                    options={[
                      { value: "", label: "Globální styl" },
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

              <!-- VÝŠKA TRYSKY -->
              <div
                class="grid grid-cols-3 items-center gap-2"
                title="Lokální override fyzické délky jehly/trysky (pokud má tento substrát jinou zkumavku)"
              >
                <span
                  class="text-slate-400 {localSlides[idx].nozzle_modified
                    ? 'text-orange-400 font-semibold'
                    : ''}">Výška trysky [mm]:</span
                >
                <div class="col-span-2 h-7">
                  <NumberInput
                    step={0.1}
                    bind:value={localSlides[idx].nozzle_height}
                    on:input={() => handleInput(idx, "nozzle_height")}
                    placeholder="Globální výška"
                    class="w-full h-full text-[11px]"
                  />
                </div>
              </div>
            </div>
          </CollapsibleBox>
        {/if}
      {/each}
    </div>
  </div>
</div>
