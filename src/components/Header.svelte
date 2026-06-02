<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { FileText, Save, FolderOpen, Sliders } from "lucide-svelte";
  import { projectStore, recentFilesStore } from "../stores/projectStore";

  export let onResetProject: () => void;
  export let isTauri: boolean = true;
  export let onTriggerLoadFileInput: () => void;
  export let onLoadRecentFile: (path: string) => void;
  export let onSaveProject: () => void;
  export let onSaveProjectAs: () => void;
  export let onExportCSVProtocol: () => void;
  export let onQuitApp: () => void;
  export let onOpenSettings: () => void;
  export let onOpenDiagnostics: () => void;
  export let onOpenFeedback: () => void;
  export let onOpenShortcuts: () => void;
  export let onOpenAbout: () => void;

  const dispatch = createEventDispatcher();
</script>

<header
  class="glass-panel flex items-center justify-between px-4 py-2 rounded-lg border border-slate-700/30 relative z-50"
>
  <div class="flex items-center gap-6">
    <!-- DROPDOWN MENU BAR (100% PyQt6 Parity) -->
    <div class="flex items-center gap-4 text-xs font-semibold text-slate-400">
      <!-- SOUBOR -->
      {#if isTauri}
        <div class="relative group py-1">
          <button class="hover:text-slate-100 transition-colors outline-none select-none"
            >Soubor</button
          >
          <div
            class="absolute left-0 top-full hidden group-hover:flex flex-col bg-slate-950/95 backdrop-blur-xl border border-slate-800/80 rounded-lg shadow-2xl py-1 w-48 z-50 transition-all duration-200"
          >
            <button
              on:click={onResetProject}
              class="px-3 py-1.5 text-left hover:bg-labaccent/20 hover:text-labaccent text-slate-200 border-l-2 border-transparent hover:border-labaccent flex items-center justify-between transition-all duration-150"
            >
              <span>Nový projekt</span>
            </button>
            <button
              on:click={onTriggerLoadFileInput}
              class="px-3 py-1.5 text-left hover:bg-labaccent/20 hover:text-labaccent text-slate-200 border-l-2 border-transparent hover:border-labaccent flex items-center justify-between transition-all duration-150"
            >
              <span>Načíst vzorek</span>
              <span class="text-[9px] text-slate-500 font-mono">Ctrl+O</span>
            </button>
            <button
              on:click={onSaveProject}
              class="px-3 py-1.5 text-left hover:bg-labaccent/20 hover:text-labaccent text-slate-200 border-l-2 border-transparent hover:border-labaccent flex items-center justify-between transition-all duration-150"
            >
              <span>Uložit</span>
              <span class="text-[9px] text-slate-500 font-mono">Ctrl+S</span>
            </button>
            <button
              on:click={onSaveProjectAs}
              class="px-3 py-1.5 text-left hover:bg-labaccent/20 hover:text-labaccent text-slate-200 border-l-2 border-transparent hover:border-labaccent flex items-center justify-between transition-all duration-150"
            >
              <span>Uložit jako...</span>
              <span class="text-[9px] text-slate-500 font-mono">Ctrl+Shift+S</span>
            </button>
            <div class="relative group/recent">
              <button
                class="w-full px-3 py-1.5 text-left hover:bg-labaccent/20 hover:text-labaccent text-slate-200 border-l-2 border-transparent hover:border-labaccent flex items-center justify-between transition-all duration-150"
              >
                <span>Otevřít nedávné</span>
                <span class="text-[9px] text-slate-500">▸</span>
              </button>
              <div
                class="absolute left-full top-0 hidden group-hover/recent:flex flex-col bg-slate-950/95 backdrop-blur-xl border border-slate-800/80 rounded-lg shadow-2xl py-1 w-64 z-50 overflow-hidden transition-all duration-200 ml-1"
              >
                {#if $recentFilesStore.length === 0}
                  <div class="px-3 py-1.5 text-slate-500 italic text-xs">Žádné nedávné soubory</div>
                {:else}
                  {#each $recentFilesStore as file}
                    <button
                      on:click={() => onLoadRecentFile(file.path)}
                      class="px-3 py-1.5 text-left hover:bg-labaccent/20 hover:text-labaccent text-slate-200 border-l-2 border-transparent hover:border-labaccent flex flex-col transition-all duration-150 truncate"
                    >
                      <span class="truncate w-full">{file.name}</span>
                      <span class="text-[9px] text-slate-500 truncate w-full">{file.path}</span>
                    </button>
                  {/each}
                {/if}
              </div>
            </div>
            <button
              on:click={onExportCSVProtocol}
              class="px-3 py-1.5 text-left hover:bg-labaccent/20 hover:text-labaccent text-slate-200 border-l-2 border-transparent hover:border-labaccent flex items-center justify-between transition-all duration-150"
            >
              <span>Uložit protokol (CSV)</span>
            </button>
            <div class="border-t border-slate-800 my-1"></div>
            <button
              on:click={onQuitApp}
              class="px-3 py-1.5 text-left hover:bg-labred/20 hover:text-labred text-slate-200 border-l-2 border-transparent hover:border-labred flex items-center justify-between transition-all duration-150"
            >
              <span>Ukončit</span>
              <span class="text-[9px] text-slate-500 font-mono">Ctrl+Q</span>
            </button>
          </div>
        </div>
      {/if}

      <!-- NASTAVENÍ -->
      <div class="py-1">
        <button
          on:click={() => onOpenSettings()}
          class="hover:text-slate-100 transition-colors outline-none select-none">Nastavení</button
        >
      </div>

      <!-- NÁSTROJE -->
      <div class="relative group py-1">
        <button class="hover:text-slate-100 transition-colors outline-none select-none"
          >Nástroje</button
        >
        <div
          class="absolute left-0 top-full hidden group-hover:flex flex-col bg-slate-950/95 backdrop-blur-xl border border-slate-800/80 rounded-lg shadow-2xl py-1 w-48 z-50 overflow-hidden transition-all duration-200"
        >
          <button
            on:click={() => alert("Tato funkce bude definována v dalším kroku")}
            class="px-3 py-1.5 text-left hover:bg-labaccent/20 hover:text-labaccent text-slate-200 border-l-2 border-transparent hover:border-labaccent transition-all duration-150"
          >
            <span>Export držáku</span>
          </button>
          <button
            on:click={onOpenDiagnostics}
            class="px-3 py-1.5 text-left hover:bg-labaccent/20 hover:text-labaccent text-slate-200 border-l-2 border-transparent hover:border-labaccent transition-all duration-150"
          >
            <span>Diagnostika tiskárny</span>
          </button>
        </div>
      </div>

      <!-- NÁPOVĚDA -->
      <div class="relative group py-1">
        <button class="hover:text-slate-100 transition-colors outline-none select-none"
          >Nápověda</button
        >
        <div
          class="absolute left-0 top-full hidden group-hover:flex flex-col bg-slate-950/95 backdrop-blur-xl border border-slate-800/80 rounded-lg shadow-2xl py-1 w-48 z-50 overflow-hidden transition-all duration-200"
        >
          <button
            on:click={() => onOpenFeedback()}
            class="px-3 py-1.5 text-left hover:bg-labaccent/20 hover:text-labaccent text-slate-200 border-l-2 border-transparent hover:border-labaccent transition-all duration-150"
          >
            <span>Feedback</span>
          </button>
          <button
            on:click={() => onOpenShortcuts()}
            class="px-3 py-1.5 text-left hover:bg-labaccent/20 hover:text-labaccent text-slate-200 border-l-2 border-transparent hover:border-labaccent transition-all duration-150"
          >
            <span>Seznam zkratek</span>
          </button>
          <div class="border-t border-slate-800 my-1"></div>
          <button
            on:click={() => onOpenAbout()}
            class="px-3 py-1.5 text-left hover:bg-labaccent/20 hover:text-labaccent text-slate-200 border-l-2 border-transparent hover:border-labaccent transition-all duration-150"
          >
            <span>O programu</span>
          </button>
        </div>
      </div>
    </div>
  </div>
</header>
