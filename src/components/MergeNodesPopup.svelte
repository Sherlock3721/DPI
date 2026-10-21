<script lang="ts">
  import { run } from "svelte/legacy";
  import { createEventDispatcher } from "svelte";
  import { X, Merge, RotateCcw } from "lucide-svelte";
  import NumberInput from "./NumberInput.svelte";
  import type { SubstratePaths, Transform } from "../lib/tauri";
  import {
    applyMerge,
    buildMergeGroups,
    buildPreview,
    collectNodes,
    groupTarget,
    includedNodes,
    type MergeOrigin,
    type MergePreview,
    type NodeFlagMap,
  } from "../lib/nodeMerge";

  /** Automerge uzlů podle vzdálenosti — parametry, výčet skupin a živý náhled.
   *  Skupiny jsou nezávislé: kotva i vyjmutí uzlu platí vždy jen v rámci
   *  jedné skupiny. Vlastní geometrii mění až potvrzení (událost `apply`). */

  const dispatch = createEventDispatcher();

  interface Props {
    open?: boolean;
    /** Trasa vybraného sklíčka (lokální souřadnice). */
    pathData?: SubstratePaths | null;
    /** Transformace téhož sklíčka — převod uzlů do world space. */
    transform?: Transform | null;
    /** Popisek cílového sklíčka v hlavičce. */
    slideLabel?: string;
    /** Počet dalších sklíček se shodnou trasou (nabídka „použít na všechna“). */
    twinCount?: number;
    /** Data pro vykreslení náhledu v plátně (HUD 3D náhledu). */
    preview?: MergePreview | null;
  }

  let {
    open = false,
    pathData = null,
    transform = null,
    slideLabel = "",
    twinCount = 0,
    preview = $bindable(null),
  }: Props = $props();

  /** Delší výčet by zahltil DOM — na plátně je vidět náhled všech skupin. */
  const LIST_LIMIT = 200;

  let maxDist = $state(0.1);
  // Typový parametr místo anotace: jinak TS zúží `origin` na literál "centroid"
  // a porovnání v odvozených hodnotách nahlásí jako nemožné.
  let origin = $state<MergeOrigin>("centroid");
  let flags: NodeFlagMap = $state({});
  let hovered = $state(0);
  let applyToAll = $state(true);

  /** Vyprázdněné pole vrací null — bez sanitace by spadl clustering i popisky. */
  let dist = $derived(Number.isFinite(maxDist) && maxDist > 0 ? maxDist : 0);
  let nodes = $derived(open ? collectNodes(pathData, transform) : []);
  let groups = $derived(buildMergeGroups(nodes, dist));
  let listed = $derived(groups.slice(0, LIST_LIMIT));

  // V automatickém režimu výčet uzlů není vidět, takže by kotvy ani vyjmutí
  // tiše měnily výsledek. Zůstávají uložené pro návrat do manuálního režimu.
  let activeFlags: NodeFlagMap = $derived(origin === "manual" ? flags : {});

  /** Kolik uzlů skutečně ubyde — skupiny s méně než dvěma zbylými uzly se přeskočí. */
  let stats = $derived.by(() => {
    let active = 0,
      removed = 0;
    for (const g of groups) {
      const inc = includedNodes(g, activeFlags).length;
      if (inc < 2) continue;
      active++;
      removed += inc - 1;
    }
    return { active, removed };
  });

  // Těžká část náhledu (uzly mimo merge + úseky výsledné trasy) nezávisí na
  // zvýraznění — hover pak jen dosadí index a nepřepočítává celou trasu.
  let previewBase = $derived(
    open
      ? buildPreview({ paths: pathData, transform, nodes, groups, flags: activeFlags, origin, maxDist: dist })
      : null
  );

  run(() => {
    preview = previewBase ? { ...previewBase, hovered } : null;
  });

  function flagOf(key: string) {
    return flags[key] ?? {};
  }

  /** Klik na číslo uzlu — kotva cílové pozice. Druhý klik kotvu ruší. */
  function toggleAnchor(key: string) {
    const f = flagOf(key);
    if (f.excluded) return;
    if (origin !== "manual") origin = "manual";
    flags = { ...flags, [key]: { ...f, anchor: !f.anchor } };
  }

  /** Klik na ✕ — uzel se merge neúčastní. Kotvu přitom ztrácí. */
  function toggleExcluded(key: string) {
    const f = flagOf(key);
    const excluded = !f.excluded;
    flags = { ...flags, [key]: { anchor: excluded ? false : f.anchor, excluded } };
  }

  function resetFlags() {
    flags = {};
  }

  function close() {
    hovered = 0;
    dispatch("close");
  }

  function confirmMerge() {
    if (!pathData || stats.active === 0) return;
    const result = applyMerge(pathData, groups, activeFlags, origin);
    dispatch("apply", { ...result, applyToAll: applyToAll && twinCount > 0 });
    close();
  }

  /** Popis cílové pozice skupiny pro řádek výčtu. */
  function targetLabel(gi: number): string {
    const g = groups[gi];
    const inc = includedNodes(g, activeFlags);
    if (inc.length < 2) return "neaktivní";
    if (origin === "centroid") return "těžiště";
    const anchors = inc.filter((n) => flagOf(n.key).anchor);
    if (anchors.length === 0) return "těžiště (bez kotvy)";
    if (anchors.length === 1) return `na uzlu ${g.nodes.indexOf(anchors[0]) + 1}`;
    return `střed ${anchors.length} kotev`;
  }
</script>

{#if open}
  <div
    class="absolute top-full right-2 mt-2 z-40 w-[23rem] max-h-[70vh] flex flex-col
      rounded-xl border border-slate-700 bg-slate-950/97 backdrop-blur-md shadow-2xl
      text-slate-200 select-none overflow-hidden"
    role="dialog"
    aria-modal="false"
    aria-label="Sloučení uzlů podle vzdálenosti"
  >
    <!-- ── Hlavička ── -->
    <div class="flex items-center gap-2 px-3 py-2 border-b border-slate-800 bg-slate-900/60">
      <Merge class="w-4 h-4 text-labaccent" />
      <div class="flex-1 min-w-0">
        <div class="text-xs font-semibold leading-tight">Sloučení uzlů</div>
        <div class="text-[10px] text-slate-500 truncate">
          {slideLabel || "trasa"} · {nodes.length} uzlů
        </div>
      </div>
      <button
        onclick={close}
        title="Zavřít"
        class="p-1 rounded-sm text-slate-400 hover:text-white hover:bg-slate-800 transition-colors"
      >
        <X class="w-3.5 h-3.5" />
      </button>
    </div>

    <!-- ── Parametry ── -->
    <div class="px-3 py-2 border-b border-slate-800 flex flex-col gap-2">
      <div class="flex items-center gap-2">
        <span class="text-[11px] text-slate-300 flex-1">Max. vzdálenost uzlů (mm)</span>
        <div class="w-24"><NumberInput bind:value={maxDist} step={0.05} min={0.001} max={100} /></div>
      </div>
      <input
        type="range"
        min="0.01"
        max={Math.max(5, maxDist)}
        step="0.01"
        bind:value={maxDist}
        class="w-full h-1 cursor-pointer accent-blue-500"
      />

      <div class="flex items-center gap-2 pt-1">
        <span class="text-[11px] text-slate-300 flex-1">Nový uzel vznikne</span>
        <div class="flex rounded-md overflow-hidden border border-slate-700">
          <button
            onclick={() => {
              origin = "centroid";
              hovered = 0;
            }}
            class="px-2 py-1 text-[10px] transition-colors {origin === 'centroid'
              ? 'bg-labaccent text-white'
              : 'bg-slate-900 text-slate-400 hover:bg-slate-800'}"
          >
            Uprostřed
          </button>
          <button
            onclick={() => (origin = "manual")}
            class="px-2 py-1 text-[10px] border-l border-slate-700 transition-colors {origin === 'manual'
              ? 'bg-labaccent text-white'
              : 'bg-slate-900 text-slate-400 hover:bg-slate-800'}"
          >
            Manuálně
          </button>
        </div>
      </div>
      <p class="text-[10px] text-slate-500 leading-snug">
        {#if origin === "centroid"}
          Nový uzel leží v těžišti slučovaných uzlů skupiny.
        {:else}
          Klik na číslo uzlu = kotva cílové pozice (více kotev = jejich střed),
          druhý klik kotvu zruší. Bez kotvy se použije těžiště.
        {/if}
      </p>
    </div>

    <!-- ── Výčet skupin ── -->
    {#if origin === "manual"}
      <div class="flex items-center gap-2 px-3 py-1.5 border-b border-slate-800 bg-slate-900/40">
        <span class="text-[10px] text-slate-400 flex-1">
          {groups.length} skupin · ✕ vyjme uzel z merge
        </span>
        <button
          onclick={resetFlags}
          title="Zrušit všechny kotvy a vyjmutí"
          class="flex items-center gap-1 px-1.5 py-0.5 rounded-sm text-[10px] text-slate-400
            hover:text-white hover:bg-slate-800 transition-colors"
        >
          <RotateCcw class="w-3 h-3" /> Obnovit
        </button>
      </div>

      <div class="flex-1 overflow-y-auto px-2 py-2 flex flex-col gap-1.5 min-h-[3rem]">
        {#if groups.length === 0}
          <p class="text-[11px] text-slate-500 text-center py-3">
            Do vzdálenosti {dist} mm není žádná dvojice uzlů — zvyš mez.
          </p>
        {/if}

        {#each listed as group, gi (gi)}
          {@const inactive = includedNodes(group, flags).length < 2}
          <div
            role="group"
            aria-label="Skupina {gi + 1}"
            onmouseenter={() => (hovered = gi + 1)}
            onmouseleave={() => (hovered = 0)}
            class="rounded-lg border px-2 py-1.5 transition-colors {hovered === gi + 1
              ? 'border-cyan-500/60 bg-cyan-500/5'
              : 'border-slate-800 bg-slate-900/40'} {inactive ? 'opacity-50' : ''}"
          >
            <div class="flex items-center gap-2 mb-1">
              <span class="text-[10px] font-semibold text-slate-300">Skupina {gi + 1}</span>
              <span class="text-[10px] text-slate-500 flex-1">
                {group.nodes.length} uzlů · rozptyl {group.spread.toFixed(3)} mm
              </span>
              <span class="text-[10px] text-cyan-400">{targetLabel(gi)}</span>
            </div>

            <div class="flex flex-wrap gap-1">
              {#each group.nodes as node, ni (node.key)}
                {@const f = flagOf(node.key)}
                <div
                  class="flex items-stretch rounded-sm border overflow-hidden text-[10px] {f.excluded
                    ? 'border-slate-800 bg-slate-900/60 text-slate-600'
                    : f.anchor && origin === 'manual'
                      ? 'border-amber-500/70 bg-amber-500/15 text-amber-300'
                      : 'border-slate-700 bg-slate-900 text-slate-300'}"
                >
                  <button
                    onclick={() => toggleAnchor(node.key)}
                    disabled={f.excluded}
                    title={f.excluded
                      ? "Uzel je vyjmut z merge"
                      : f.anchor
                        ? "Zrušit kotvu"
                        : "Sloučit sem (kotva cílové pozice)"}
                    class="px-1.5 py-0.5 tabular-nums font-semibold transition-colors
                      {f.excluded ? 'cursor-not-allowed' : 'hover:bg-slate-700/70'}"
                  >
                    {ni + 1}.
                  </button>
                  <button
                    onclick={() => toggleExcluded(node.key)}
                    title={f.excluded ? "Vrátit uzel do merge" : "Vyjmout uzel z merge"}
                    class="px-1 border-l flex items-center transition-colors {f.excluded
                      ? 'border-slate-800 text-slate-600 hover:text-slate-300 hover:bg-slate-800'
                      : 'border-slate-700 text-slate-500 hover:text-red-300 hover:bg-slate-700/70'}"
                  >
                    <X class="w-2.5 h-2.5" />
                  </button>
                </div>
              {/each}
            </div>
          </div>
        {/each}

        {#if groups.length > LIST_LIMIT}
          <p class="text-[10px] text-slate-500 text-center py-1">
            Zobrazeno prvních {LIST_LIMIT} z {groups.length} skupin — sloučí se všechny.
          </p>
        {/if}
      </div>
    {:else}
      <!-- Automatický režim: cíl určuje těžiště, vybírat z uzlů není co. -->
      <div
        class="px-3 py-3 border-b border-slate-800 bg-slate-900/40 text-[11px]
          text-slate-400 text-center leading-snug"
      >
        {#if groups.length === 0}
          Do vzdálenosti {dist} mm není žádná dvojice uzlů — zvyš mez.
        {:else}
          {groups.length} skupin ke sloučení — výsledek je vidět v náhledu na plátně.
        {/if}
      </div>
    {/if}

    <!-- ── Potvrzení ── -->
    <div class="px-3 py-2 border-t border-slate-800 bg-slate-900/60 flex flex-col gap-2">
      {#if twinCount > 0}
        <label class="flex items-center gap-1.5 text-[11px] text-slate-300 cursor-pointer">
          <input
            type="checkbox"
            bind:checked={applyToAll}
            class="w-3.5 h-3.5 rounded-sm accent-labaccent cursor-pointer"
          />
          Použít i na ostatní sklíčka se shodnou trasou ({twinCount})
        </label>
      {/if}
      <div class="flex items-center gap-2">
        <span class="text-[10px] text-slate-500 flex-1">
          {stats.active} skupin · ubyde {stats.removed} uzlů
        </span>
        <button
          onclick={close}
          class="px-3 py-1.5 rounded-lg border border-slate-600 bg-slate-800/60 text-slate-300
            hover:border-slate-400 text-[11px] font-medium transition-colors"
        >
          Zrušit
        </button>
        <button
          onclick={confirmMerge}
          disabled={stats.active === 0}
          class="px-3 py-1.5 rounded-lg bg-labaccent hover:bg-blue-600 text-white text-[11px]
            font-medium transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
        >
          Sloučit
        </button>
      </div>
    </div>
  </div>
{/if}
