<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { X, Keyboard } from "lucide-svelte";

  export let show: boolean = false;
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
    { keys: ["Ctrl", "Shift", "Z"], desc: "Znovu (Redo)" },
    { keys: ["Ctrl", "Q"],       desc: "Ukončit aplikaci" },

    { group: "Pohled a výběr" },
    { keys: ["Kolečko myši"],    desc: "Přiblížit / Oddálit" },
    { keys: ["LMB tažení"],      desc: "Panoramování pohledu (na prázdné ploše)" },
    { keys: ["LMB"],             desc: "Vybrat substrát" },
    { keys: ["LMB tažení"],      desc: "Přesunout trasu (na vybraném sklíčku)" },
    { keys: ["Dvojklik"],        desc: "Přepnout Scale ↔ Rotate mód" },
    { keys: ["RMB"],             desc: "Kontextové menu sklíčka" },

    { group: "Modifikátory při tažení" },
    { keys: ["Ctrl"],            desc: "Přichytit k mřížce (Snap to Grid)" },
    { keys: ["Alt"],             desc: "Synchronizovaný pohyb všech substrátů" },
    { keys: ["Shift"],           desc: "Anchor na středu při škálování" },
    { keys: ["Ctrl"],            desc: "Zaokrouhlit měřítko na 0.1 / rotaci na 15°" },

    { group: "Vybraný substrát" },
    { keys: ["Delete"],          desc: "Smazat dráhu sklíčka" },
    { keys: ["↑ ↓ ← →"],        desc: "Posunout trasu o 1 mm" },
    { keys: ["Shift", "↑ ↓ ← →"], desc: "Posunout trasu o 0.1 mm" },

    { group: "Měřidlo" },
    { keys: ["LMB"],             desc: "Přidat bod měření" },
    { keys: ["RMB"],             desc: "Smazat poslední bod měření" },
    { keys: ["Ctrl / Alt"],      desc: "Přichytit k mřížce nebo rohům sklíček" },
    { keys: ["Escape"],          desc: "Zrušit měření / Reset transformačního módu" },
  ];
</script>

{#if show}
  <!-- svelte-ignore a11y-click-events-have-key-events -->
  <!-- svelte-ignore a11y-no-static-element-interactions -->
  <div
    class="fixed inset-0 bg-black/80 backdrop-blur-sm z-[200] flex items-center justify-center p-4 animate-fade-in"
    on:click={close}
  >
    <div
      class="bg-slate-900 border border-slate-700 shadow-2xl rounded-xl w-full max-w-lg overflow-hidden flex flex-col relative animate-fade-in-up"
      on:click|stopPropagation
    >
      <!-- Hlavička -->
      <div class="flex justify-between items-center p-4 border-b border-slate-800 bg-slate-950/50">
        <h2 class="text-lg font-bold text-slate-200 flex items-center gap-2">
          <Keyboard class="w-5 h-5 text-labaccent" />
          Klávesové zkratky
        </h2>
        <button on:click={close} class="text-slate-400 hover:text-white transition-colors">
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
                      class="px-2 py-0.5 bg-slate-800 border border-slate-600 rounded text-xs text-slate-200 font-mono shadow-sm whitespace-nowrap"
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
          on:click={close}
          class="px-6 py-2 bg-labaccent hover:bg-blue-600 text-white rounded transition-colors text-sm font-medium"
        >
          Zavřít
        </button>
      </div>
    </div>
  </div>
{/if}
