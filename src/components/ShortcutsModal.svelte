<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { X, Keyboard } from "lucide-svelte";

  export let show: boolean = false;
  const dispatch = createEventDispatcher();

  function close() {
    dispatch("close");
  }

  const shortcuts = [
    { keys: ["Ctrl", "O"], desc: "Otevřít projekt / Načíst soubor" },
    { keys: ["Ctrl", "S"], desc: "Uložit projekt / Generovat G-Code" },
    { keys: ["Ctrl", "Z"], desc: "Zpět (Undo)" },
    { keys: ["Ctrl", "Shift", "Z"], desc: "Znovu (Redo)" },
    { keys: ["Ctrl", "Q"], desc: "Ukončit aplikaci" },
    { keys: ["Z"], desc: "Přepnout nástroj Zoom" },
    { keys: ["H"], desc: "Přepnout nástroj Ručička (Pan)" },
    { keys: ["Mezerník"], desc: "Držením dočasně aktivovat Ručičku (Pan)" },
    { keys: ["Kolečko myši"], desc: "Přiblížit / Oddálit (Zoom)" },
    { keys: ["Delete"], desc: "Smazat vybraný vzorek na ploše" },
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
        <h2 class="text-lg font-bold text-white flex items-center gap-2">
          <Keyboard class="w-5 h-5 text-labaccent" />
          Klávesové zkratky
        </h2>
        <button on:click={close} class="text-slate-400 hover:text-white transition-colors">
          <X class="w-5 h-5" />
        </button>
      </div>

      <!-- Obsah -->
      <div class="p-6 overflow-y-auto max-h-[60vh] custom-scrollbar">
        <div class="space-y-3">
          {#each shortcuts as sc}
            <div
              class="flex items-center justify-between p-2 hover:bg-slate-800/50 rounded-lg transition-colors border border-transparent hover:border-slate-700"
            >
              <span class="text-slate-300 text-sm">{sc.desc}</span>
              <div class="flex items-center gap-1.5">
                {#each sc.keys as k}
                  <kbd
                    class="px-2 py-1 bg-slate-800 border border-slate-600 rounded text-xs text-slate-200 font-mono shadow-sm"
                  >
                    {k}
                  </kbd>
                  {#if k !== sc.keys[sc.keys.length - 1]}
                    <span class="text-slate-500 text-xs">+</span>
                  {/if}
                {/each}
              </div>
            </div>
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
