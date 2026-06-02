<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { X, FlaskConical, Github } from "lucide-svelte";
  import { open } from "@tauri-apps/plugin-shell";

  export let show: boolean = false;
  const dispatch = createEventDispatcher();

  function close() {
    dispatch("close");
  }

  function openGithub() {
    open("https://github.com/Sherlock3721/DPI");
  }
</script>

{#if show}
  <!-- svelte-ignore a11y-click-events-have-key-events -->
  <!-- svelte-ignore a11y-no-static-element-interactions -->
  <div
    class="fixed inset-0 bg-black/80 backdrop-blur-sm z-[200] flex items-center justify-center p-4 animate-fade-in"
    on:click={close}
  >
    <div
      class="bg-slate-900 border border-slate-700 shadow-2xl rounded-xl w-full max-w-md overflow-hidden flex flex-col relative animate-fade-in-up"
      on:click|stopPropagation
    >
      <!-- Hlavička -->
      <div class="flex justify-between items-center p-4 border-b border-slate-800 bg-slate-950/50">
        <h2 class="text-lg font-bold text-white flex items-center gap-2">
          <FlaskConical class="w-5 h-5 text-labaccent" />
          O programu
        </h2>
        <button on:click={close} class="text-slate-400 hover:text-white transition-colors">
          <X class="w-5 h-5" />
        </button>
      </div>

      <!-- Obsah -->
      <div class="p-6 flex flex-col items-center justify-center text-center space-y-4">
        <div class="bg-labaccent/20 text-labaccent p-4 rounded-2xl shadow-lg shadow-labaccent/10">
          <FlaskConical class="w-16 h-16" />
        </div>

        <div>
          <h1 class="text-2xl font-bold text-white tracking-wide">Droplet Printing Interface</h1>
          <p class="text-slate-400 text-sm mt-1">
            Pokročilý nástroj pro generování a správu G-code
          </p>
        </div>

        <div class="bg-slate-950 px-4 py-2 rounded-lg border border-slate-800 w-full">
          <div class="grid grid-cols-2 gap-2 text-sm text-left">
            <span class="text-slate-500">Verze:</span>
            <span class="text-slate-200 font-mono text-right">1.5.0</span>
            <span class="text-slate-500">Autor:</span>
            <span class="text-slate-200 text-right">Cyril Veverka</span>
            <span class="text-slate-500">Sestavení:</span>
            <span class="text-slate-200 font-mono text-right">2026-05</span>
          </div>
        </div>

        <p class="text-xs text-slate-500 text-left mt-2">
          Tento software je určen pro návrh, vizualizaci a řízení procesu tisku kapek kapalin na
          sklíčka a další podklady. Využívá moderních technologií jako Svelte, Tauri a Rust pro
          zajištění maximálního výkonu a stability.
        </p>
      </div>

      <!-- Patička -->
      <div class="p-4 border-t border-slate-800 bg-slate-950/50 flex justify-between items-center">
        <button
          on:click={openGithub}
          class="flex items-center gap-2 text-sm text-slate-400 hover:text-white transition-colors"
        >
          <Github class="w-4 h-4" />
          Zdrojové kódy
        </button>
        <button
          on:click={close}
          class="px-4 py-1.5 bg-labaccent hover:bg-blue-600 text-white rounded transition-colors text-sm font-medium"
        >
          Zavřít
        </button>
      </div>
    </div>
  </div>
{/if}
