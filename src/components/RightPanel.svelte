<script lang="ts">
  import { run } from 'svelte/legacy';

  import { createEventDispatcher, onMount, untrack } from "svelte";
  import CameraWidget from "./CameraWidget.svelte";
  import ManualMovementWidget from "./ManualMovementWidget.svelte";
  import CollapsibleBox from "./CollapsibleBox.svelte";
  import Terminal from "./Terminal.svelte";
  import CustomSelect from "./CustomSelect.svelte";
  import NumberInput from "./NumberInput.svelte";
  import ParamIcon, { type ParamIconName } from "./ParamIcon.svelte";
  import {
    Settings2,
    RotateCcw,
    AlignJustify,
    Rows4,
    Route,
    Spline,
    Grip,
    Grid,
    Square,
    CircleDashed,
    ArrowUp,
    ArrowDown,
    ArrowDownLeft,
    ArrowDownRight,
    ArrowUpLeft,
    ArrowUpRight,
    Link2,
    Unlink,
  } from "lucide-svelte";
  import {
    subscribe_printer_status,
    type SlideOverride,
    type ObjectOverride,
    type PathObject,
    type PrinterStatus,
  } from "../lib/tauri";
  import { liquidLimits } from "../stores/liquidStore";
  import { cameraAvailable } from "../stores/cameraStore";
  import { settingsStore } from "../stores/settingsStore";
  import { projectStore, cloneGroupOf } from "../stores/projectStore";
  import { convertExtrusionRate, toCanonicalExtrusionRate, type ExtUnit } from "../lib/extrusionUnits";

  interface Props {
    sampleCount?: number;
    primeActive?: boolean;
    overrides?: Record<string, SlideOverride>;
    /** Objekty na jednotlivých sklíčkách (z drah zpracovaných backendem). */
    slideObjects?: PathObject[][];
    /** Zdroj je SVG — objekt lze přepnout na díru úpravou prvku. */
    canEditElements?: boolean;
    /** openSlideIdx + openTrigger: canvas → panel. Trigger se inkrementuje i při
     *  opakovaném výběru stejného sklíčka, aby se reactive blok vždy spustil. */
    openSlideIdx?: number;
    openTrigger?: number;
    /** Objekt vybraný v plátně → rozbalí se zde (n = trigger; slide -1 = zrušit výběr). */
    objectFocusRequest?: { slide: number; id: number; n: number };
  }

  let {
    sampleCount = 1,
    primeActive = false,
    overrides = $bindable({}),
    slideObjects = [],
    canEditElements = false,
    openSlideIdx = -1,
    openTrigger = 0,
    objectFocusRequest = { slide: -1, id: -1, n: 0 }
  }: Props = $props();

  const dispatch = createEventDispatcher();

  /** UI stav přepisu — sklíčko i objekt mají stejná pole. */
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
    /** Začátek hada (jen objekt): "" = automaticky, jinak roh "bl" | "br" | "tl" | "tr". */
    snake_start: string;

    // Zda jsou hodnoty lokálně změněné (modified)
    z_modified: boolean;
    ext_modified: boolean;
    speed_modified: boolean;
    infill_modified: boolean;
    nozzle_modified: boolean;
    infill_style_modified: boolean;
  }

  function emptyLocal(): LocalSlideData {
    return {
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
      snake_start: "",

      z_modified: false,
      ext_modified: false,
      speed_modified: false,
      infill_modified: false,
      nozzle_modified: false,
      infill_style_modified: false,
    };
  }

  let localSlides: LocalSlideData[] = $state([]);
  let openSlides: boolean[] = $state([]);
  /** Přepisy objektů: index sklíčka → id objektu → UI stav. */
  let localObjects: Record<string, Record<string, LocalSlideData>> = $state({});
  let openObjectLists: Record<number, boolean> = $state({});
  /** Rozbalený objekt („sklo:id“) — zvýrazní se v plátně. */
  let openObject: string | null = $state(null);

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

  // Výběr objektu v plátně: rozbalit substrát, seznam objektů i objekt
  let _lastFocusRequest = 0;
  $effect(() => {
    const req = objectFocusRequest;
    if (req.n === _lastFocusRequest) return;
    _lastFocusRequest = req.n;
    untrack(() => {
      if (req.slide < 0 || req.slide >= sampleCount) {
        openObject = null;
        return;
      }
      openSlides = Array.from({ length: sampleCount }, (_, i) => i === req.slide);
      openObjectLists[req.slide] = true;
      openObject = `${req.slide}:${req.id}`;
    });
  });

  function handleSlideToggle(idx: number, nowOpen: boolean) {
    if (nowOpen) {
      openSlides = openSlides.map((_, i) => i === idx);
      dispatch("slideActivated", idx);
    } else {
      openSlides = openSlides.map((v, i) => (i === idx ? false : v));
    }
  }

  function handleObjectToggle(idx: number, id: number, nowOpen: boolean) {
    const key = `${idx}:${id}`;
    if (nowOpen) {
      openObject = key;
      dispatch("objectFocus", { slide: idx, id });
    } else if (openObject === key) {
      openObject = null;
      dispatch("objectFocus", null);
    }
  }

  let primeSlide = $state({
    width: "15",
    infill_val: "1.5",
    infill_type: "mm" as "mm" | "%" | "počet",
    extrusion_rate: "",
    extrusion_unit: "nl/mm" as "µl/mm" | "nl/mm" | "kroky/mm",
    glass_type: "laboratorní" as "laboratorní" | "vzorkové",
    /** Tloušťka odplivového skla [mm]; "" = stejná jako vzorkové sklo. */
    glass_z: "1",
    modified: true,
  });

  /** Změna typu odplivového skla: vzorkové dědí tloušťku vzorkového skla,
   *  laboratorní má vlastní (výchozí 1 mm, upravitelnou). */
  function handlePrimeGlassTypeChange() {
    if (primeSlide.glass_type === "vzorkové") primeSlide.glass_z = "";
    else if (!primeSlide.glass_z) primeSlide.glass_z = "1";
    handlePrimeInput();
  }

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
        localSlides[i] = emptyLocal();
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

  // UI stav pro každý objekt založit předem (v šabloně se stav měnit nesmí)
  run(() => {
    const lists = slideObjects;
    untrack(() => {
      lists.forEach((objs, idx) => (objs ?? []).forEach((o) => objectLocal(idx, o.id)));
    });
  });

  /** UI stav přepisu objektu (založí se při prvním použití). */
  function objectLocal(idx: number, id: number): LocalSlideData {
    const sk = String(idx),
      ok = String(id);
    if (!localObjects[sk]) localObjects[sk] = {};
    if (!localObjects[sk][ok]) localObjects[sk][ok] = emptyLocal();
    return localObjects[sk][ok];
  }

  /** Označí pole jako lokálně změněné. */
  function markField(d: LocalSlideData, field: keyof LocalSlideData) {
    if (field === "z_offset") d.z_modified = true;
    else if (field === "extrusion_rate") d.ext_modified = true;
    else if (field === "print_speed") d.speed_modified = true;
    else if (field === "infill_val") d.infill_modified = true;
    else if (field === "nozzle_height") d.nozzle_modified = true;
    else if (field === "infill_style") d.infill_style_modified = true;
  }

  // Přepočet a odeslání do parenta při změně vstupu
  function handleInput(idx: number, field: keyof LocalSlideData) {
    const slide = localSlides[idx];
    if (!slide) return;
    markField(slide, field);
    updateParentOverrides();
  }

  function handleObjectInput(idx: number, id: number, field: keyof LocalSlideData) {
    markField(objectLocal(idx, id), field);
    syncClones(idx, id);
    updateParentOverrides();
  }

  /** Propojené klony převezmou nastavení objektu (kromě názvu a poznámky). */
  function syncClones(idx: number, id: number) {
    const src = objectLocal(idx, id);
    for (const m of cloneGroupOf(overrides, idx, id)) {
      if (m === id) continue;
      const d = objectLocal(idx, m);
      Object.assign(d, { ...src, name: d.name, note: d.note });
    }
  }

  // Zrušení lokálních změn a návrat ke globálnímu nastavení
  function resetSlide(idx: number) {
    if (!localSlides[idx]) return;
    localSlides[idx] = emptyLocal();
    localSlides = [...localSlides];
    updateParentOverrides();
  }

  function resetObject(idx: number, id: number) {
    const sk = String(idx);
    if (localObjects[sk]) {
      // Klony se vrací spolu s objektem (názvy si ponechají)
      for (const m of cloneGroupOf(overrides, idx, id)) {
        const name = m === id ? "" : localObjects[sk][String(m)]?.name ?? "";
        delete localObjects[sk][String(m)];
        if (name) objectLocal(idx, m).name = name;
      }
      localObjects = { ...localObjects };
    }
    updateParentOverrides();
  }

  /** Přepočet Z-offsetu při změně jednotek; jednotka se nastaví PŘED odesláním,
   *  aby se nová hodnota nepřepočítala starou jednotkou. */
  function changeZUnit(d: LocalSlideData, newUnit: "mm" | "µm", changed: () => void) {
    const val = parseFloat(d.z_offset);
    const hadValue = d.z_offset !== "" && !isNaN(val) && d.z_unit !== newUnit;
    if (hadValue) {
      d.z_offset = newUnit === "µm" ? (val * 1000.0).toFixed(1) : (val / 1000.0).toFixed(3);
    }
    d.z_unit = newUnit;
    if (hadValue) changed();
  }

  /** Přepočet extruze při změně jednotek (jednotka se nastaví před odesláním). */
  function changeExtUnit(d: LocalSlideData, newUnit: ExtUnit, changed: () => void) {
    const oldUnit = d.extrusion_unit;
    const val = parseFloat(d.extrusion_rate);
    const hadValue = d.extrusion_rate !== "" && !isNaN(val) && oldUnit !== newUnit;
    if (hadValue) {
      const newVal = convertExtrusionRate(val, oldUnit, newUnit, $settingsStore.calibration_factor);
      d.extrusion_rate = newUnit === "kroky/mm" ? newVal.toFixed(1) : newVal.toFixed(4);
    }
    d.extrusion_unit = newUnit;
    if (hadValue) changed();
  }

  /** UI stav → přepis pro backend (jen lokálně změněná pole). */
  function toOverride(d: LocalSlideData): ObjectOverride {
    const o: ObjectOverride = {};
    if (d.name) o.name = d.name;
    if (d.note) o.note = d.note;

    // Z-offset
    if (d.z_modified && d.z_offset !== "") {
      let z = parseFloat(d.z_offset);
      if (!isNaN(z)) {
        if (d.z_unit === "µm") z /= 1000.0;
        o.z_offset = z;
      }
    }

    // Extruze
    if (d.ext_modified && d.extrusion_rate !== "") {
      const ext = parseFloat(d.extrusion_rate);
      if (!isNaN(ext)) {
        o.extrusion_rate = toCanonicalExtrusionRate(ext, d.extrusion_unit, $settingsStore.calibration_factor);
        o.extrusion_unit = "µl/mm";
      }
    }

    // Speed
    if (d.speed_modified && d.print_speed !== "") {
      const spd = parseFloat(d.print_speed);
      if (!isNaN(spd)) o.print_speed = spd;
    }

    // Infill (Hustota)
    if (d.infill_modified && d.infill_val !== "") {
      const inf = parseFloat(d.infill_val);
      if (!isNaN(inf)) {
        o.infill_val = inf;
        o.infill_type = d.infill_type;
      }
    }

    // Výška trysky
    if (d.nozzle_modified && d.nozzle_height !== "") {
      const nz = parseFloat(d.nozzle_height);
      if (!isNaN(nz)) o.nozzle_height = nz;
    }

    // Styl infillu
    if (d.infill_style_modified && d.infill_style !== "") {
      o.infill_style = d.infill_style;
    }
    if (d.snake_start) o.snake_start = d.snake_start;
    return o;
  }

  /** Přepis z projektu → UI stav (načtení uloženého projektu). */
  function fromOverride(o: ObjectOverride | undefined): LocalSlideData {
    const d = emptyLocal();
    if (!o) return d;
    d.name = o.name ?? "";
    d.note = o.note ?? "";
    if (o.z_offset != null) {
      d.z_offset = (o.z_offset * 1000).toFixed(1);
      d.z_modified = true;
    }
    if (o.extrusion_rate != null) {
      const unit = (o.extrusion_unit as ExtUnit) || "µl/mm";
      const nl = convertExtrusionRate(o.extrusion_rate, unit, "nl/mm", $settingsStore.calibration_factor);
      d.extrusion_rate = String(+nl.toFixed(4));
      d.ext_modified = true;
    }
    if (o.print_speed != null) {
      d.print_speed = String(o.print_speed);
      d.speed_modified = true;
    }
    if (o.infill_val != null) {
      d.infill_val = String(o.infill_val);
      d.infill_type = (o.infill_type as LocalSlideData["infill_type"]) || "mm";
      d.infill_modified = true;
    }
    if (o.nozzle_height != null) {
      d.nozzle_height = String(o.nozzle_height);
      d.nozzle_modified = true;
    }
    if (o.infill_style) {
      d.infill_style = o.infill_style;
      d.infill_style_modified = true;
    }
    d.snake_start = o.snake_start ?? "";
    return d;
  }

  // Projekt načtený zvenku (G-kód s metadaty) → naplnit panel uloženými přepisy.
  // Vlastní změny panelu jdou přes updateParentOverrides a znovu se nenačítají.
  let lastEmitted: Record<string, SlideOverride> | null = null;
  run(() => {
    const ov = overrides;
    if (ov === lastEmitted) return;
    untrack(() => hydrate(ov ?? {}));
  });

  function hydrate(ov: Record<string, SlideOverride>) {
    localSlides = Array.from({ length: sampleCount }, (_, i) => fromOverride(ov[String(i)]));
    const objs: Record<string, Record<string, LocalSlideData>> = {};
    for (const [key, so] of Object.entries(ov)) {
      if (key === "-1" || !so.objects) continue;
      objs[key] = Object.fromEntries(
        Object.entries(so.objects).map(([id, oo]) => [id, fromOverride(oo)])
      );
    }
    localObjects = objs;
    const p = ov["-1"];
    if (p) {
      primeSlide.width = String(p.slide_w ?? 15);
      primeSlide.glass_type = p.glass_type === "vzorkové" ? "vzorkové" : "laboratorní";
      // Starší projekty tloušťku odplivu neukládaly — laboratorní sklo má 1 mm
      primeSlide.glass_z =
        p.glass_z != null ? String(p.glass_z) : primeSlide.glass_type === "vzorkové" ? "" : "1";
      primeSlide.infill_val = p.infill_val != null ? String(p.infill_val) : "";
      primeSlide.infill_type = (p.infill_type as "mm" | "%" | "počet") || "mm";
      primeSlide.extrusion_rate =
        p.extrusion_rate != null
          ? String(
              +convertExtrusionRate(
                p.extrusion_rate,
                (p.extrusion_unit as ExtUnit) || "µl/mm",
                "nl/mm",
                $settingsStore.calibration_factor
              ).toFixed(4)
            )
          : "";
      primeSlide.extrusion_unit = "nl/mm";
    }
    lastEmitted = ov;
  }

  // Sestavení finálního objektu overrides pro backend
  function updateParentOverrides() {
    const result: Record<string, SlideOverride> = {};
    for (let i = 0; i < sampleCount; i++) {
      const slide = localSlides[i];
      if (!slide) continue;

      const slide_data: SlideOverride = toOverride(slide);

      // Přepisy objektů skla (třetí úroveň nastavení)
      const objs: Record<string, ObjectOverride> = {};
      for (const [id, d] of Object.entries(localObjects[String(i)] ?? {})) {
        const o = toOverride(d);
        if (Object.keys(o).length > 0) objs[id] = o;
      }
      if (Object.keys(objs).length > 0) slide_data.objects = objs;
      // Smazané objekty, úpravy a pořadí objektů spravuje plátno (projectStore)
      // — panel je jen přenáší
      const cur = overrides[String(i)];
      if (cur?.deleted_objects?.length) slide_data.deleted_objects = cur.deleted_objects;
      if (cur?.object_transforms && Object.keys(cur.object_transforms).length)
        slide_data.object_transforms = cur.object_transforms;
      if (cur?.object_order?.length) slide_data.object_order = cur.object_order;
      if (cur?.clones?.length) slide_data.clones = cur.clones;

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

      // Prázdné pole (NumberInput vrací null) = tloušťka vzorkového skla
      const gz = parseFloat(String(primeSlide.glass_z));
      if (!isNaN(gz) && gz >= 0) p_over.glass_z = gz;

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

    lastEmitted = result;
    overrides = result;
    dispatch("overridesChanged", result);
  }

  /** Popis objektu v seznamu: rozměry a vztah k ostatním. */
  function objectInfo(o: PathObject, list: PathObject[]): string {
    const w = (o.bounds[1] - o.bounds[0]).toFixed(1);
    const h = (o.bounds[3] - o.bounds[2]).toFixed(1);
    const parts = [o.filled ? `${w} × ${h} mm` : `čára ${w} × ${h} mm`];
    if (o.merged_into != null) {
      const k = list.findIndex((x) => x.id === o.merged_into);
      parts.push(
        o.hole
          ? `díra v objektu ${k + 1}`
          : `tiskne se spolu s objektem ${k + 1} (jeho nastavením)`
      );
    }
    return parts.join(" · ");
  }

  /** Styl výplně, kterým se objekt tiskne (objekt → substrát → globální). */
  function effectiveStyle(idx: number, od: LocalSlideData | undefined): string {
    return od?.infill_style || localSlides[idx]?.infill_style || $projectStore.params.infill_style;
  }

  const snakeStarts = [
    { value: "", label: "Auto", title: "Začátek podle nejkratší trasy (a napojení na spojovací čáry)" },
    { value: "bl", icon: ArrowUpRight, title: "Začít vlevo dole" },
    { value: "br", icon: ArrowUpLeft, title: "Začít vpravo dole" },
    { value: "tl", icon: ArrowDownRight, title: "Začít vlevo nahoře" },
    { value: "tr", icon: ArrowDownLeft, title: "Začít vpravo nahoře" },
  ];

  const styleOptions = [
    { value: "Okraje + Výplň", label: "Okraje + Výplň", icon: Rows4 },
    { value: "Výplň", label: "Výplň", icon: AlignJustify },
    { value: "Okraje", label: "Okraje", icon: Square },
    { value: "Had", label: "Had", icon: Route },
    { value: "Jedna linka", label: "Jedna linka", icon: Spline },
    { value: "Mřížka", label: "Mřížka", icon: Grid },
    { value: "Tečky", label: "Tečky", icon: Grip },
  ];
</script>


<div
  class="@container glass-panel rounded-lg p-3 flex flex-col gap-3 h-full overflow-hidden text-xs select-text"
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
            <div class="grid grid-cols-1 @min-[19rem]:grid-cols-3 items-center gap-x-2 gap-y-1">
              {@render paramLabel("substrate", "Typ substrátu:", "text-orange-200/80")}
              <div class="@min-[19rem]:col-span-2 h-7">
                <CustomSelect
                  bind:value={primeSlide.glass_type}
                  on:change={handlePrimeGlassTypeChange}
                  options={[
                    { value: "laboratorní", label: "Laboratorní" },
                    { value: "vzorkové", label: "Vzorkové" },
                  ]}
                  cssStyle="height: 100%; font-size: 11px; background-color: rgba(67, 20, 7, 0.5); color: #ffedd5; border-color: rgba(249, 115, 22, 0.3);"
                />
              </div>
            </div>

            <!-- Tloušťka skla -->
            <div class="grid grid-cols-1 @min-[19rem]:grid-cols-3 items-center gap-x-2 gap-y-1">
              {@render paramLabel("glassThickness", "Tloušťka skla:", "text-orange-200/80", "Výška povrchu odplivového skla nad podložkou. Prázdné = stejná jako vzorkové sklo.")}
              <div class="@min-[19rem]:col-span-2 h-7">
                <NumberInput
                  min={0}
                  step={0.1}
                  unit="mm"
                  bind:value={primeSlide.glass_z}
                  on:input={handlePrimeInput}
                  placeholder={`= vzorkové (${$projectStore.params.slide_z})`}
                  class="w-full h-full text-[11px] bg-orange-950/50 text-orange-100 border-orange-500/30"
                />
              </div>
            </div>

            <!-- Velikost -->
            <div class="grid grid-cols-1 @min-[19rem]:grid-cols-3 items-center gap-x-2 gap-y-1">
              {@render paramLabel("primeSize", "Velikost čtverce:", "text-orange-200/80")}
              <div class="@min-[19rem]:col-span-2 h-7">
                <NumberInput
                  min={1}
                  step={1}
                  max={25}
                  unit="mm"
                  bind:value={primeSlide.width}
                  on:input={handlePrimeInput}
                  placeholder="Šířka"
                  class="w-full h-full text-[11px] bg-orange-950/50 text-orange-100 border-orange-500/30"
                />
              </div>
            </div>

            <!-- Výplň -->
            <div class="grid grid-cols-1 @min-[19rem]:grid-cols-3 items-center gap-x-2 gap-y-1">
              {@render paramLabel("infillDensity", "Hustota výplně:", "text-orange-200/80")}
              <div class="@min-[19rem]:col-span-2 flex gap-1 min-w-0">
                <div class="flex-1 min-w-0 h-7">
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
                <div class="shrink-0 h-7">
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
                    cssStyle="height: 100%; width: max-content; min-width: 4.75rem; font-size: 11px; background-color: rgba(67, 20, 7, 0.5); color: #ffedd5; border-color: rgba(249, 115, 22, 0.3);"
                  />
                </div>
              </div>
            </div>

            <!-- Extruze -->
            <div class="grid grid-cols-1 @min-[19rem]:grid-cols-3 items-center gap-x-2 gap-y-1">
              {@render paramLabel("extrusion", "Množství:", "text-orange-200/80")}
              <div class="@min-[19rem]:col-span-2 flex gap-1 min-w-0">
                <div class="flex-1 min-w-0 h-7">
                  <NumberInput
                    min={0}
                    step={0.1}
                    bind:value={primeSlide.extrusion_rate}
                    on:input={handlePrimeInput}
                    placeholder="Globální tok"
                    class="w-full h-full text-[11px] bg-orange-950/50 text-orange-100 border-orange-500/30"
                  />
                </div>
                <div class="shrink-0 h-7">
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
                    cssStyle="height: 100%; width: max-content; min-width: 4.75rem; font-size: 11px; background-color: rgba(67, 20, 7, 0.5); color: #ffedd5; border-color: rgba(249, 115, 22, 0.3);"
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

              {#if overrides[String(idx)]?.deleted_objects?.length}
                {@const nDel = overrides[String(idx)].deleted_objects!.length}
                <button
                  type="button"
                  onclick={() => projectStore.restoreDeletedObjects(idx)}
                  class="w-full bg-slate-900 border border-rose-500/30 hover:bg-rose-500/10 text-[10px] text-rose-300 font-semibold py-1 rounded-sm flex items-center justify-center gap-1 transition-colors"
                  title="Vrátí objekty smazané z tohoto substrátu"
                >
                  <RotateCcw class="w-3 h-3" /> Obnovit smazané objekty ({nDel})
                </button>
              {/if}

              {@render overrideFields(
                localSlides[idx],
                (f) => handleInput(idx, f),
                "Globální",
                `Substrát ${idx + 1}`,
                "Jméno substrátu pro lepší orientaci v protokolu",
                effectiveStyle(idx, undefined)
              )}

              <!-- OBJEKTY SKLA — třetí, nejvyšší úroveň nastavení -->
              {#if (slideObjects[idx] ?? []).length > 0}
                {@const objs = slideObjects[idx]}
                <CollapsibleBox
                  title={`Objekty (${objs.length})`}
                  isOpen={!!openObjectLists[idx]}
                  on:toggle={(e) => (openObjectLists[idx] = e.detail)}
                  headerClass="text-xs font-bold text-slate-300"
                  containerClass="border-slate-800 bg-slate-950/30"
                >
                  <div class="flex flex-col gap-2 pt-2">
                    <p class="text-[10px] text-slate-500 leading-snug">
                      Hodnoty objektu mají přednost před substrátem; prázdná pole se dědí ze substrátu.
                    </p>
                    {#each objs as obj, k (obj.id)}
                      {@const od = localObjects[String(idx)]?.[String(obj.id)]}
                      {@const key = `${idx}:${obj.id}`}
                      <CollapsibleBox
                        title={(od?.name || `Objekt ${k + 1}`) + (obj.hole ? " (díra)" : "") + (cloneGroupOf(overrides, idx, obj.id).length > 1 ? " (klon)" : "")}
                        isOpen={openObject === key}
                        on:toggle={(e) => handleObjectToggle(idx, obj.id, e.detail)}
                        headerClass="text-xs font-bold {openObject === key ? 'text-amber-300' : obj.hole ? 'text-rose-300' : 'text-slate-300'}"
                        containerClass="border {openObject === key ? 'border-amber-400/40 bg-amber-400/5' : 'border-slate-800 bg-slate-950/20'}"
                      >
                        <div class="flex flex-col gap-2.5 pt-2">
                          <div class="text-[10px] text-slate-500">{objectInfo(obj, objs)}</div>
                          {#if cloneGroupOf(overrides, idx, obj.id).length > 1}
                            {@const others = cloneGroupOf(overrides, idx, obj.id).filter((m) => m !== obj.id)}
                            <div class="flex items-center gap-1.5 text-[10px] text-sky-300" title="Měřítko, rotace a nastavení se přenáší mezi propojenými klony; poloha je vlastní">
                              <Link2 class="w-3 h-3 shrink-0" />
                              <span class="flex-1">
                                Klon propojený s {others
                                  .map((m) => {
                                    const j = objs.findIndex((o) => o.id === m);
                                    return j >= 0 ? localObjects[String(idx)]?.[String(m)]?.name || `Objekt ${j + 1}` : null;
                                  })
                                  .filter(Boolean)
                                  .join(", ") || "smazanými objekty"}
                              </span>
                              <button
                                type="button"
                                onclick={() => projectStore.unlinkClone(idx, obj.id)}
                                class="p-0.5 rounded-sm border border-slate-700 bg-slate-900 text-slate-300 hover:bg-slate-800"
                                title="Zrušit propojení — objekt se dál upravuje samostatně"
                              ><Unlink class="w-3 h-3" /></button>
                            </div>
                          {/if}
                          <!-- POŘADÍ TISKU -->
                          <div class="grid grid-cols-1 @min-[19rem]:grid-cols-3 items-center gap-x-2 gap-y-1" title="Objekty se tisknou v pořadí seznamu; objekty spojené čarou za sebou nejkratší trasou">
                            {@render paramLabel("printOrder", "Pořadí tisku:", "text-slate-400")}
                            <div class="@min-[19rem]:col-span-2 flex items-center gap-1">
                              <span class="font-mono text-slate-300 w-8">{k + 1}/{objs.length}</span>
                              <button
                                type="button"
                                disabled={k === 0}
                                onclick={() => projectStore.moveObjectInOrder(idx, obj.id, -1)}
                                class="p-1 rounded-sm border border-slate-700 bg-slate-900 text-slate-300 hover:bg-slate-800 disabled:opacity-30 disabled:cursor-not-allowed"
                                title="Tisknout dřív"
                              ><ArrowUp class="w-3 h-3" /></button>
                              <button
                                type="button"
                                disabled={k === objs.length - 1}
                                onclick={() => projectStore.moveObjectInOrder(idx, obj.id, 1)}
                                class="p-1 rounded-sm border border-slate-700 bg-slate-900 text-slate-300 hover:bg-slate-800 disabled:opacity-30 disabled:cursor-not-allowed"
                                title="Tisknout později"
                              ><ArrowDown class="w-3 h-3" /></button>
                            </div>
                          </div>
                          {#if od && !obj.hole && obj.filled && effectiveStyle(idx, od) === "Had"}
                            <!-- ZAČÁTEK HADA -->
                            <div class="grid grid-cols-1 @min-[19rem]:grid-cols-3 items-center gap-x-2 gap-y-1" title="Kde had začíná (šipky v plátně ukazují směr tisku)">
                              {@render paramLabel("snakeStart", "Začátek hada:", od.snake_start ? "text-orange-400 font-semibold" : "text-slate-400")}
                              <div class="@min-[19rem]:col-span-2 flex items-center gap-1">
                                {#each snakeStarts as opt (opt.value)}
                                  <button
                                    type="button"
                                    onclick={() => {
                                      od.snake_start = opt.value;
                                      handleObjectInput(idx, obj.id, "snake_start");
                                    }}
                                    title={opt.title}
                                    class="h-6 rounded-sm border text-[10px] font-semibold flex items-center justify-center transition-colors {opt.icon ? 'w-6' : 'px-1.5'} {od.snake_start === opt.value
                                      ? 'bg-amber-500/20 border-amber-400/60 text-amber-200'
                                      : 'bg-slate-900 border-slate-700 text-slate-400 hover:bg-slate-800'}"
                                  >
                                    {#if opt.icon}<opt.icon class="w-3 h-3" />{:else}{opt.label}{/if}
                                  </button>
                                {/each}
                              </div>
                            </div>
                          {/if}
                          {#if canEditElements && obj.filled && obj.element != null}
                            <button
                              type="button"
                              onclick={() => {
                                // Klony se přepnou spolu s objektem
                                const group = cloneGroupOf(overrides, idx, obj.id);
                                const els = objs.filter((o) => group.includes(o.id) && o.element != null).map((o) => o.element!);
                                projectStore.setElementHole(els.length ? els : obj.element!, !obj.hole);
                              }}
                              class="w-full border text-[10px] font-semibold py-1 rounded-sm flex items-center justify-center gap-1 transition-colors {obj.hole
                                ? 'bg-rose-500/15 border-rose-500/40 text-rose-300 hover:bg-rose-500/25'
                                : 'bg-slate-900 border-slate-700 text-slate-300 hover:bg-slate-800'}"
                              title="Díra se odečte z ostatních tvarů a sama se netiskne (platí pro všechna skla)"
                            >
                              <CircleDashed class="w-3 h-3" />
                              {obj.hole ? "Díra — změnit zpět na tvar" : "Změnit na díru"}
                            </button>
                          {/if}
                          {#if !obj.hole && od}
                            <button
                              type="button"
                              onclick={() => resetObject(idx, obj.id)}
                              class="w-full bg-slate-900 border border-slate-700 hover:bg-slate-800 text-[10px] text-slate-300 font-semibold py-1 rounded-sm flex items-center justify-center gap-1 transition-colors"
                            >
                              <RotateCcw class="w-3 h-3 text-slate-400" /> Zrušit změny objektu
                            </button>
                            {@render overrideFields(
                              od,
                              (f) => handleObjectInput(idx, obj.id, f),
                              "Dle substrátu",
                              `Objekt ${k + 1}`,
                              "Jméno objektu pro lepší orientaci",
                              effectiveStyle(idx, od)
                            )}
                          {/if}
                        </div>
                      </CollapsibleBox>
                    {/each}
                  </div>
                </CollapsibleBox>
              {/if}
            </div>
          </CollapsibleBox>
        {/if}
      {/each}
    </div>
  </div>
</div>

{#snippet paramLabel(icon: ParamIconName, text: string, cls: string, title?: string)}
  <span class="flex items-center gap-1.5 min-w-0 leading-tight {cls}" {title}>
    <ParamIcon name={icon} class="w-3.5 h-3.5 opacity-70" />
    <span class="min-w-0">{text}</span>
  </span>
{/snippet}

{#snippet overrideFields(
  d: LocalSlideData,
  changed: (field: keyof LocalSlideData) => void,
  inherit: string,
  namePlaceholder: string,
  nameTitle: string,
  style: string
)}
  {@const zScale = d.z_unit === "µm" ? 1000 : 1}
  {@const liqZMin = $liquidLimits?.z_offset_min != null ? $liquidLimits.z_offset_min * zScale : 0}
  {@const liqZMax = $liquidLimits?.z_offset_max != null ? $liquidLimits.z_offset_max * zScale : 2.0 * zScale}
  <!-- NÁZEV -->
  <div class="grid grid-cols-1 @min-[19rem]:grid-cols-3 items-center gap-x-2 gap-y-1" title={nameTitle}>
    {@render paramLabel("name", "Název:", "text-slate-400")}
    <input
      type="text"
      bind:value={d.name}
      oninput={() => changed("name")}
      placeholder={namePlaceholder}
      class="@min-[19rem]:col-span-2 input-premium py-0.5 text-[11px]"
    />
  </div>

  <!-- POZNÁMKA -->
  <div class="grid grid-cols-1 @min-[19rem]:grid-cols-3 items-center gap-x-2 gap-y-1" title="Volitelná textová poznámka do protokolu">
    {@render paramLabel("note", "Poznámka:", "text-slate-400")}
    <input
      type="text"
      bind:value={d.note}
      oninput={() => changed("note")}
      placeholder="Poznámka..."
      class="@min-[19rem]:col-span-2 input-premium py-0.5 text-[11px]"
    />
  </div>

  <!-- VÝŠKA TRYSKY (Z) -->
  <div class="grid grid-cols-1 @min-[19rem]:grid-cols-3 items-center gap-x-2 gap-y-1" title="Lokální override výšky hlavy (Z-offset) nad podložkou">
    {@render paramLabel("nozzleHeight", "Výška trysky:", d.z_modified ? "text-orange-400 font-semibold" : "text-slate-400")}
    <div class="@min-[19rem]:col-span-2 flex gap-1 min-w-0">
      <div class="flex-1 min-w-0 h-7">
        <NumberInput
          min={liqZMin}
          max={liqZMax}
          step={d.z_unit === "mm" ? 0.05 : 50}
          bind:value={d.z_offset}
          on:input={() => changed("z_offset")}
          placeholder={`${inherit} Z`}
          class="w-full h-full text-[11px]"
        />
      </div>
      <div class="shrink-0 h-7">
        <CustomSelect
          value={d.z_unit}
          on:change={(e) => changeZUnit(d, zUnitFromEvent(e), () => changed("z_offset"))}
          options={[
            { value: "mm", label: "mm" },
            { value: "µm", label: "µm" },
          ]}
          cssStyle="height: 100%; width: max-content; min-width: 4.75rem; font-size: 11px;"
        />
      </div>
    </div>
  </div>

  <!-- EXTRUZE -->
  <div class="grid grid-cols-1 @min-[19rem]:grid-cols-3 items-center gap-x-2 gap-y-1" title="Lokální override dávkování kapaliny">
    {@render paramLabel("extrusion", "Extruze:", d.ext_modified ? "text-orange-400 font-semibold" : "text-slate-400")}
    <div class="@min-[19rem]:col-span-2 flex gap-1 min-w-0">
      <div class="flex-1 min-w-0 h-7">
        <NumberInput
          min={liqExtMin}
          max={liqExtMax}
          step={0.1}
          bind:value={d.extrusion_rate}
          on:input={() => changed("extrusion_rate")}
          placeholder={`${inherit} tok`}
          class="w-full h-full text-[11px]"
        />
      </div>
      <div class="shrink-0 h-7">
        <CustomSelect
          value={d.extrusion_unit}
          on:change={(e) => changeExtUnit(d, extUnitFromEvent(e), () => changed("extrusion_rate"))}
          options={style === "Tečky"
            ? [
                // U teček je hodnota objem jedné kapky
                { value: "nl/mm", label: "nl/kap" },
                { value: "kroky/mm", label: "krok" },
              ]
            : [
                { value: "nl/mm", label: "nl/mm" },
                { value: "kroky/mm", label: "krok/mm" },
              ]}
          cssStyle="height: 100%; width: max-content; min-width: 4.75rem; font-size: 11px;"
        />
      </div>
    </div>
  </div>

  <!-- RYCHLOST -->
  <div class="grid grid-cols-1 @min-[19rem]:grid-cols-3 items-center gap-x-2 gap-y-1" title="Lokální override rychlosti tisku">
    {@render paramLabel("speed", "Rychlost:", d.speed_modified ? "text-orange-400 font-semibold" : "text-slate-400")}
    <div class="@min-[19rem]:col-span-2 h-7">
      <NumberInput
        min={liqSpeedMin}
        max={liqSpeedMax}
        step={100}
        unit="mm/min"
        bind:value={d.print_speed}
        on:input={() => changed("print_speed")}
        placeholder={`${inherit} rychlost`}
        class="w-full h-full text-[11px]"
      />
    </div>
  </div>

  <!-- VÝPLŇ (HUSTOTA) -->
  <div class="grid grid-cols-1 @min-[19rem]:grid-cols-3 items-center gap-x-2 gap-y-1" title="Lokální override hustoty výplně">
    {@render paramLabel("infillDensity", "Hustota výplně:", d.infill_modified ? "text-orange-400 font-semibold" : "text-slate-400")}
    <div class="@min-[19rem]:col-span-2 flex gap-1 min-w-0">
      <div class="flex-1 min-w-0 h-7">
        <NumberInput
          min={d.infill_type === "počet" ? 1 : 0.001}
          step={d.infill_type === "počet" ? 1 : 0.1}
          bind:value={d.infill_val}
          on:input={() => {
            if (d.infill_type === "počet")
              d.infill_val = String(Math.max(1, Math.round(Number(d.infill_val))));
            changed("infill_val");
          }}
          placeholder={`${inherit} výplň`}
          class="w-full h-full text-[11px]"
        />
      </div>
      <div class="shrink-0 h-7">
        <CustomSelect
          bind:value={d.infill_type}
          on:change={() => {
            if (d.infill_type === "počet")
              d.infill_val = String(Math.max(1, Math.round(Number(d.infill_val))));
            changed("infill_val");
          }}
          options={[
            { value: "mm", label: "mm" },
            { value: "%", label: "%" },
            { value: "počet", label: "počet" },
          ]}
          cssStyle="height: 100%; width: max-content; min-width: 4.75rem; font-size: 11px;"
        />
      </div>
    </div>
  </div>

  <!-- STYL VÝPLNĚ -->
  <div class="grid grid-cols-1 @min-[19rem]:grid-cols-3 items-center gap-x-2 gap-y-1" title="Lokální override stylu výplně (vzoru)">
    {@render paramLabel("infillStyle", "Styl výplně:", d.infill_style_modified ? "text-orange-400 font-semibold" : "text-slate-400")}
    <div class="@min-[19rem]:col-span-2">
      <CustomSelect
        bind:value={d.infill_style}
        on:change={() => changed("infill_style")}
        options={[{ value: "", label: `${inherit} styl` }, ...styleOptions]}
      />
    </div>
  </div>

  <!-- VÝŠKA TRYSKY -->
  <div
    class="grid grid-cols-1 @min-[19rem]:grid-cols-3 items-center gap-x-2 gap-y-1"
    title="Lokální override fyzické délky jehly/trysky (jiná zkumavka)"
  >
    {@render paramLabel("nozzleLength", "Délka trysky:", d.nozzle_modified ? "text-orange-400 font-semibold" : "text-slate-400")}
    <div class="@min-[19rem]:col-span-2 h-7">
      <NumberInput
        step={0.1}
        unit="mm"
        bind:value={d.nozzle_height}
        on:input={() => changed("nozzle_height")}
        placeholder={`${inherit} výška`}
        class="w-full h-full text-[11px]"
      />
    </div>
  </div>
{/snippet}
