<script lang="ts">
  import { Plus, Trash2, GripVertical, Cog } from "lucide-svelte";

  export interface LiquidRow {
    name: string; color: string; category: string;
    z_offset: number; z_offset_min: number | null; z_offset_max: number | null;
    extrusion: number; extrusion_min: number | null; extrusion_max: number | null;
    forbidden_nozzles: string[];
    print_speed: number; print_speed_min: number | null; print_speed_max: number | null;
    bed_temp: number; bed_temp_min: number | null; bed_temp_max: number | null;
  }

  interface Props {
    liquidList: LiquidRow[];
    nozzleList: { name: string; h: number; d: number; s: number; c: string }[];
  }

  let { liquidList = $bindable(), nozzleList }: Props = $props();

  let expandedLiquidOrigIdx: number | null = $state(null);
  let liquidSortBy: "id" | "name" = $state("id");
  // $derived.by: inline výraz by TS zúžil liquidSortBy na literál "id" z inicializace
  let displayLiquidIndices = $derived.by(() => liquidSortBy === "name"
    ? [...Array(liquidList.length).keys()].sort((a, b) =>
        liquidList[a].name.localeCompare(liquidList[b].name, "cs"))
    : [...Array(liquidList.length).keys()]);
  let liquidCategories = $derived([...new Set(
    liquidList.map(l => l.category?.trim() || "").filter(c => c !== "")
  )].sort((a, b) => a.localeCompare(b, "cs")));
  let liquidsHaveCategories = $derived(liquidList.some(l => (l.category?.trim() || "") !== ""));
  let liquidGroups = $derived((() => {
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
  })());

  // ─── CRUD ─────────────────────────────────────────────────────────────────
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

  // ─── Přeřazení řádků drag & drop (jen řazení dle ID bez kategorií) ─────────
  let dragSrcIndex: number | null = null;
  let dragOverIndex: number | null = $state(null);

  function onRowDragStart(e: DragEvent, i: number) {
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", i.toString()); // Povinné pro Firefox
    }
    dragSrcIndex = i;
  }
  function onRowDragOver(e: DragEvent, i: number) {
    e.preventDefault();
    dragOverIndex = i;
  }
  function onRowDrop(i: number) {
    if (dragSrcIndex === null || dragSrcIndex === i) {
      dragSrcIndex = dragOverIndex = null;
      return;
    }
    const arr = [...liquidList];
    const [item] = arr.splice(dragSrcIndex, 1);
    arr.splice(i, 0, item);
    liquidList = arr;
    expandedLiquidOrigIdx = null;
    dragSrcIndex = dragOverIndex = null;
  }
  function onRowDragEnd() {
    dragSrcIndex = dragOverIndex = null;
  }

  // ─── Nozzle drag-and-drop mezi povolen./zakázanými ────────────────────────
  let nozzleDragSrcList: "allowed" | "forbidden" | null = $state(null);
  let nozzleDragSrcName = $state("");

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
</script>

<div class="flex flex-col gap-3">
  <!-- Header: název + sort + přidat -->
  <div class="flex justify-between items-center pb-2 border-b border-slate-800">
    <div class="flex items-center gap-3">
      <span class="font-bold text-xs text-slate-300">Presety kapalin</span>
      <div class="flex items-center gap-0.5 bg-slate-900/60 border border-slate-800 rounded-sm p-0.5">
        <button
          onclick={() => (liquidSortBy = "id")}
          class="px-2 py-0.5 text-[9px] font-bold rounded-sm transition-colors {liquidSortBy === 'id' ? 'bg-labaccent text-white' : 'text-slate-400 hover:text-slate-200'}"
        >ID</button>
        <button
          onclick={() => (liquidSortBy = "name")}
          class="px-2 py-0.5 text-[9px] font-bold rounded-sm transition-colors {liquidSortBy === 'name' ? 'bg-labaccent text-white' : 'text-slate-400 hover:text-slate-200'}"
        >A–Z</button>
      </div>
    </div>
    <button
      onclick={addLiquid}
      class="bg-labaccent hover:bg-blue-600 text-white text-[10px] font-bold px-2 py-1 rounded-sm flex items-center gap-1 transition-colors"
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
        <option value={cat}></option>
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
              <span class="w-1.5 h-1.5 rounded-full bg-labaccent shrink-0"></span>
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
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="flex flex-col">
              <!-- hlavní řádek -->
              <div
                class="grid grid-cols-12 p-2 items-center text-center gap-1 text-xs transition-colors
                       {dragOverIndex === displayIdx && liquidSortBy === 'id' && !liquidsHaveCategories
                         ? 'bg-labaccent/10 border-t-2 border-labaccent'
                         : 'hover:bg-slate-900/30'}"
                draggable={liquidSortBy === "id" && !liquidsHaveCategories}
                ondragstart={(e) => liquidSortBy === "id" && !liquidsHaveCategories && onRowDragStart(e, displayIdx)}
                ondragover={(e) => onRowDragOver(e, displayIdx)}
                ondrop={() => onRowDrop(displayIdx)}
                ondragend={onRowDragEnd}
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
                  onclick={() => toggleLiquidExpand(origIdx)}
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
                  onclick={() => deleteLiquid(origIdx)}
                  class="col-span-1 p-1 text-slate-500 hover:text-labred hover:bg-labred/10 rounded-sm flex items-center justify-center transition-colors"
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
                      onchange={(e) => { liquidList[origIdx].category = e.currentTarget.value; liquidList = liquidList; }}
                      class="input-premium py-0.5 text-left w-40 placeholder-slate-700"
                    />
                  </div>

              <!-- Výška trysky -->
              <div class="grid grid-cols-[1fr_5rem_5rem_5rem] gap-x-2 items-center py-1 border-b border-slate-900/70">
                <span class="text-slate-400">Výška trysky <span class="text-slate-600 text-[10px]">mm</span></span>
                <input type="number" step="0.05"
                  value={liquid.z_offset}
                  onchange={(e) => { liquidList[origIdx].z_offset = +e.currentTarget.value; liquidList = liquidList; }}
                  class="input-premium py-0.5 text-center" />
                <input type="number" step="0.05" placeholder="—"
                  value={liquid.z_offset_min ?? ""}
                  onchange={(e) => { const v = e.currentTarget.value; liquidList[origIdx].z_offset_min = v === "" ? null : +v; liquidList = liquidList; }}
                  class="input-premium py-0.5 text-center placeholder-slate-700" />
                <input type="number" step="0.05" placeholder="—"
                  value={liquid.z_offset_max ?? ""}
                  onchange={(e) => { const v = e.currentTarget.value; liquidList[origIdx].z_offset_max = v === "" ? null : +v; liquidList = liquidList; }}
                  class="input-premium py-0.5 text-center placeholder-slate-700" />
              </div>

              <!-- Extruze -->
              <div class="grid grid-cols-[1fr_5rem_5rem_5rem] gap-x-2 items-center py-1 border-b border-slate-900/70">
                <span class="text-slate-400">Extruze <span class="text-slate-600 text-[10px]">nl/mm</span></span>
                <input type="number" step="0.1" min="0"
                  value={liquid.extrusion}
                  onchange={(e) => { liquidList[origIdx].extrusion = +e.currentTarget.value; liquidList = liquidList; }}
                  class="input-premium py-0.5 text-center" />
                <input type="number" step="0.1" placeholder="—"
                  value={liquid.extrusion_min ?? ""}
                  onchange={(e) => { const v = e.currentTarget.value; liquidList[origIdx].extrusion_min = v === "" ? null : +v; liquidList = liquidList; }}
                  class="input-premium py-0.5 text-center placeholder-slate-700" />
                <input type="number" step="0.1" placeholder="—"
                  value={liquid.extrusion_max ?? ""}
                  onchange={(e) => { const v = e.currentTarget.value; liquidList[origIdx].extrusion_max = v === "" ? null : +v; liquidList = liquidList; }}
                  class="input-premium py-0.5 text-center placeholder-slate-700" />
              </div>

              <!-- Povolené trysky — dual DnD list -->
              <div class="py-2 border-b border-slate-900/70">
                <span class="text-slate-400 text-[10px] font-semibold block mb-1.5">Povolené trysky</span>
                <div class="flex gap-2">

                  <!-- ── POVOLENÉ ── -->
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <div
                    class="flex-1 min-h-[40px] rounded border p-1 flex flex-col gap-0.5 transition-colors
                           {nozzleDragSrcList === 'forbidden' ? 'border-labaccent/60 bg-labaccent/5' : 'border-slate-700/50 bg-slate-900/30'}"
                    ondragover={onNozzleDragOver}
                    ondrop={(e) => onNozzleDrop(e, origIdx, "allowed")}
                  >
                    <div class="text-[8px] font-bold text-slate-500 uppercase tracking-wide px-0.5 pb-0.5 border-b border-slate-800 mb-0.5 shrink-0">
                      Povolené
                    </div>
                    {#each allowedNozzlesFor(origIdx) as n (n.name)}
                      <!-- svelte-ignore a11y_no_static_element_interactions -->
                      <div
                        draggable="true"
                        title="Přetáhněte nebo dvakrát klikněte pro přesun"
                        ondragstart={(e) => onNozzleDragStart(e, "allowed", n.name)}
                        ondragend={onNozzleDragEnd}
                        ondragover={(e) => e.preventDefault()}
                        ondrop={(e) => { e.stopPropagation(); onNozzleDrop(e, origIdx, "allowed"); }}
                        ondblclick={() => moveLiquidNozzle(origIdx, n.name, "allowed")}
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
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <div
                    class="flex-1 min-h-[40px] rounded border p-1 flex flex-col gap-0.5 transition-colors
                           {nozzleDragSrcList === 'allowed' ? 'border-labred/40 bg-labred/5' : 'border-slate-700/50 bg-slate-900/30'}"
                    ondragover={onNozzleDragOver}
                    ondrop={(e) => onNozzleDrop(e, origIdx, "forbidden")}
                  >
                    <div class="text-[8px] font-bold text-slate-500 uppercase tracking-wide px-0.5 pb-0.5 border-b border-slate-800 mb-0.5 shrink-0">
                      Zakázané
                    </div>
                    {#each forbiddenNozzlesFor(origIdx) as n (n.name)}
                      <!-- svelte-ignore a11y_no_static_element_interactions -->
                      <div
                        draggable="true"
                        title="Přetáhněte nebo dvakrát klikněte pro přesun"
                        ondragstart={(e) => onNozzleDragStart(e, "forbidden", n.name)}
                        ondragend={onNozzleDragEnd}
                        ondragover={(e) => e.preventDefault()}
                        ondrop={(e) => { e.stopPropagation(); onNozzleDrop(e, origIdx, "forbidden"); }}
                        ondblclick={() => moveLiquidNozzle(origIdx, n.name, "forbidden")}
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
                  onchange={(e) => { liquidList[origIdx].print_speed = +e.currentTarget.value; liquidList = liquidList; }}
                  class="input-premium py-0.5 text-center" />
                <input type="number" step="50" placeholder="—"
                  value={liquid.print_speed_min ?? ""}
                  onchange={(e) => { const v = e.currentTarget.value; liquidList[origIdx].print_speed_min = v === "" ? null : +v; liquidList = liquidList; }}
                  class="input-premium py-0.5 text-center placeholder-slate-700" />
                <input type="number" step="50" placeholder="—"
                  value={liquid.print_speed_max ?? ""}
                  onchange={(e) => { const v = e.currentTarget.value; liquidList[origIdx].print_speed_max = v === "" ? null : +v; liquidList = liquidList; }}
                  class="input-premium py-0.5 text-center placeholder-slate-700" />
              </div>

              <!-- Výhřev podložky -->
              <div class="grid grid-cols-[1fr_5rem_5rem_5rem] gap-x-2 items-center py-1">
                <span class="text-slate-400">Výhřev podložky <span class="text-slate-600 text-[10px]">°C</span></span>
                <input type="number" step="5" min="0"
                  value={liquid.bed_temp}
                  onchange={(e) => { liquidList[origIdx].bed_temp = +e.currentTarget.value; liquidList = liquidList; }}
                  class="input-premium py-0.5 text-center" />
                <input type="number" step="5" placeholder="—"
                  value={liquid.bed_temp_min ?? ""}
                  onchange={(e) => { const v = e.currentTarget.value; liquidList[origIdx].bed_temp_min = v === "" ? null : +v; liquidList = liquidList; }}
                  class="input-premium py-0.5 text-center placeholder-slate-700" />
                <input type="number" step="5" placeholder="—"
                  value={liquid.bed_temp_max ?? ""}
                  onchange={(e) => { const v = e.currentTarget.value; liquidList[origIdx].bed_temp_max = v === "" ? null : +v; liquidList = liquidList; }}
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
