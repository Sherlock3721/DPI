<script lang="ts">
  import { createBubbler, stopPropagation } from 'svelte/legacy';

  const bubble = createBubbler();
  import { createEventDispatcher } from "svelte";
  import { X, Keyboard } from "lucide-svelte";

  interface Props {
    show?: boolean;
  }

  let { show = false }: Props = $props();
  const dispatch = createEventDispatcher();

  function close() {
    dispatch("close");
  }

  type ShortcutGroup = { group: string };
  type ShortcutItem = { keys: string[]; desc: string };
  type ShortcutEntry = ShortcutGroup | ShortcutItem;

  function isGroup(e: ShortcutEntry): e is ShortcutGroup {
    return "group" in e;
  }

  const shortcuts: ShortcutEntry[] = [
    { group: "Globální" },
    { keys: ["Ctrl", "O"],       desc: "Načíst soubor (G-Code / SVG / DXF)" },
    { keys: ["Ctrl", "S"],       desc: "Vygenerovat G-kód" },
    { keys: ["Ctrl", "Z"],       desc: "Zpět (Undo)" },
    { keys: ["Ctrl", "Y"],       desc: "Znovu (Redo), také Ctrl+Shift+Z" },
    { keys: ["Ctrl", "Q"],       desc: "Ukončit aplikaci" },

    { group: "Pohled a výběr" },
    { keys: ["Kolečko myši"],    desc: "Přiblížit / Oddálit" },
    { keys: ["LMB tažení"],      desc: "Panoramování pohledu (na prázdné ploše)" },
    { keys: ["LMB"],             desc: "Vybrat substrát (trasu)" },
    { keys: ["LMB znovu"],       desc: "Vybrat objekt trasy (druhý klik bez pohybu)" },
    { keys: ["LMB tažení"],      desc: "Přesunout trasu / vybraný objekt" },
    { keys: ["Dvojklik"],        desc: "Přepnout Scale ↔ Rotate mód (trasa i objekt)" },
    { keys: ["RMB"],             desc: "Kontextové menu sklíčka" },

    { group: "Modifikátory při tažení" },
    { keys: ["Ctrl"],            desc: "Přichytit k mřížce (Snap to Grid)" },
    { keys: ["Alt"],             desc: "Přesun / rotace všech substrátů najednou" },
    { keys: ["Alt"],             desc: "Při škálování: odemčený poměr stran" },
    { keys: ["Shift"],           desc: "Anchor na středu při škálování" },
    { keys: ["Ctrl"],            desc: "Zaokrouhlit měřítko na 0.1 / rotaci na 15°" },

    { group: "Vybraný substrát / objekt" },
    { keys: ["Delete"],          desc: "Smazat objekt; bez vybraného objektu celou trasu (sklo se vrátí do čistého stavu)" },
    { keys: ["Shift", "Delete"], desc: "Totéž ze všech substrátů" },
    { keys: ["Ctrl", "C"],       desc: "Kopírovat vybraný objekt, bez objektu celou trasu" },
    { keys: ["Ctrl", "V"],       desc: "Vložit na vybraný substrát (i jiný než zdroj)" },
    { keys: ["Ctrl", "D"],       desc: "Duplikovat objekt / trasu na stejném substrátu" },
    { keys: ["Ctrl", "Alt", "D"], desc: "Klonovat: propojená kopie (měřítko, rotace a nastavení se přenáší)" },
    { keys: ["↑ ↓ ← →"],        desc: "Posunout trasu (vybraný objekt) o 1 mm" },
    { keys: ["Shift", "↑ ↓ ← →"], desc: "Posunout o 0.1 mm" },

    { group: "Kreslení" },
    { keys: ["M"],               desc: "Magnet: přichytit k trase (čára se pak objektu drží)" },
    { keys: ["Shift"],           desc: "Čtverec / kruh, čára po 45°" },
    { keys: ["Alt"],             desc: "Kreslit od středu" },
    { keys: ["Escape"],          desc: "Zrušit tvar / ukončit nástroj" },

    { group: "Měřidlo" },
    { keys: ["LMB"],             desc: "Přidat bod měření" },
    { keys: ["RMB"],             desc: "Smazat poslední bod měření" },
    { keys: ["Ctrl / Alt"],      desc: "Přichytit k mřížce nebo rohům sklíček" },
    { keys: ["Escape"],          desc: "Zrušit měření / Reset transformačního módu" },
  ];
</script>

{#if show}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 bg-black/80 backdrop-blur-xs z-200 flex items-center justify-center p-4 animate-fade-in"
    onclick={close}
  >
    <div
      class="bg-slate-900 border border-slate-700 shadow-2xl rounded-xl w-full max-w-lg overflow-hidden flex flex-col relative animate-fade-in-up"
      onclick={stopPropagation(bubble('click'))}
    >
      <!-- Hlavička -->
      <div class="flex justify-between items-center p-4 border-b border-slate-800 bg-slate-950/50">
        <h2 class="text-lg font-bold text-slate-200 flex items-center gap-2">
          <Keyboard class="w-5 h-5 text-labaccent" />
          Klávesové zkratky
        </h2>
        <button onclick={close} class="text-slate-400 hover:text-white transition-colors">
          <X class="w-5 h-5" />
        </button>
      </div>

      <!-- Obsah -->
      <div class="px-4 py-4 overflow-y-auto max-h-[65vh] custom-scrollbar">
        <div class="space-y-0.5">
          {#each shortcuts as entry}
            {#if isGroup(entry)}
              <div class="pt-3 pb-1 first:pt-0">
                <span class="text-[10px] font-bold uppercase tracking-widest text-labaccent/80">
                  {entry.group}
                </span>
              </div>
            {:else}
              <div
                class="flex items-center justify-between px-2 py-1.5 hover:bg-slate-800/50 rounded-lg transition-colors"
              >
                <span class="text-slate-300 text-sm">{entry.desc}</span>
                <div class="flex items-center gap-1 shrink-0 ml-3">
                  {#each entry.keys as k, i}
                    <kbd
                      class="px-2 py-0.5 bg-slate-800 border border-slate-600 rounded-sm text-xs text-slate-200 font-mono shadow-xs whitespace-nowrap"
                    >
                      {k}
                    </kbd>
                    {#if i < entry.keys.length - 1}
                      <span class="text-slate-500 text-xs">+</span>
                    {/if}
                  {/each}
                </div>
              </div>
            {/if}
          {/each}
        </div>
      </div>

      <!-- Patička -->
      <div class="p-4 border-t border-slate-800 bg-slate-950/50 flex justify-end">
        <button
          onclick={close}
          class="px-6 py-2 bg-labaccent hover:bg-blue-600 text-white rounded-sm transition-colors text-sm font-medium"
        >
          Zavřít
        </button>
      </div>
    </div>
  </div>
{/if}
