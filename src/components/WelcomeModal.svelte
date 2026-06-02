<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import { FolderOpen, FileText, Github, Clock, ChevronRight } from "lucide-svelte";
  import { recentFilesStore, type RecentFile } from "../stores/projectStore";
  import { open } from "@tauri-apps/plugin-shell";
  import { invoke } from "@tauri-apps/api/core";
  import QRCode from "qrcode";
  import iconImg from "../assets/icon.png";

  export let show: boolean = false;

  const dispatch = createEventDispatcher();

  let networkUrl = "";
  let qrCodeDataUrl = "";

  function handleNewProject() {
    dispatch("newProject");
  }

  function handleOpenRecent(file: RecentFile) {
    dispatch("openRecent", file.path);
  }

  function openGithub() {
    open("https://github.com/Sherlock3721/DPI");
  }

  function formatDate(ts: number) {
    return new Date(ts).toLocaleString("cs-CZ");
  }

  onMount(async () => {
    try {
      const ip: string = await invoke("get_local_ip");
      networkUrl = `http://${ip}:5173`;
      qrCodeDataUrl = await QRCode.toDataURL(networkUrl, {
        color: {
          dark: "#000000",
          light: "#ffffff",
        },
        margin: 1,
        width: 180,
      });
    } catch (e) {
      console.error("Failed to get local IP or generate QR", e);
    }
  });
</script>

{#if show}
  <div
    class="fixed inset-0 bg-black/80 backdrop-blur-md z-[100] flex items-center justify-center p-4 select-none"
  >
    <div
      class="bg-slate-900 border border-slate-700 shadow-2xl rounded-xl w-full max-w-2xl overflow-hidden flex flex-col relative animate-fade-in-up"
    >
      <!-- Hlavička s logem -->
      <div
        class="p-8 flex flex-col items-center justify-center border-b border-slate-800 bg-gradient-to-b from-slate-800/50 to-slate-900"
      >
        <div class="bg-labaccent/20 p-2 rounded-2xl mb-4 shadow-lg shadow-labaccent/10">
          <img src={iconImg} alt="DPI Icon" class="w-16 h-16 object-contain" />
        </div>
        <h1 class="text-2xl font-bold text-white tracking-wide">Droplet Printing Interface</h1>
        <div
          class="text-slate-400 text-sm mt-1 font-mono bg-slate-950 px-3 py-1 rounded-full border border-slate-800"
        >
          Verze 1.5.0
        </div>
      </div>

      <!-- Tělo - dva sloupce -->
      <div class="flex flex-1 min-h-[300px] max-h-[500px]">
        <!-- Levý sloupec - Nový projekt -->
        <div
          class="w-5/12 bg-slate-900/50 p-6 flex flex-col items-center justify-center border-r border-slate-800 gap-6"
        >
          <button
            on:click={handleNewProject}
            class="w-full aspect-square max-w-[180px] bg-slate-800 hover:bg-labaccent hover:text-white text-labaccent border border-slate-700 hover:border-labaccent rounded-2xl flex flex-col items-center justify-center gap-4 transition-all duration-300 shadow-lg group"
          >
            <FolderOpen class="w-12 h-12 group-hover:scale-110 transition-transform duration-300" />
            <span class="font-bold text-sm tracking-wide">Otevřít projekt</span>
          </button>

          {#if qrCodeDataUrl}
            <div class="w-full max-w-[180px] flex flex-col items-center gap-2 mt-2">
              <img
                src={qrCodeDataUrl}
                alt="QR Code"
                class="ounded-lg shadow-md border border-slate-700"
              />
              <a
                href={networkUrl}
                target="_blank"
                rel="noopener noreferrer"
                class="text-xs text-labaccent hover:underline font-mono"
              >
                {networkUrl}
              </a>
            </div>
          {/if}
        </div>

        <!-- Pravý sloupec - Nedávné projekty -->
        <div class="w-7/12 bg-slate-950 p-6 flex flex-col">
          <div
            class="flex items-center gap-2 text-slate-400 font-semibold mb-4 text-xs uppercase tracking-wider shrink-0"
          >
            <Clock class="w-4 h-4" /> Nedávné projekty
          </div>

          <div class="flex-1 overflow-y-auto custom-scrollbar pr-2 space-y-2">
            {#if $recentFilesStore.length === 0}
              <div class="text-slate-600 text-sm flex items-center justify-center h-full italic">
                Zatím nebyly otevřeny žádné soubory.
              </div>
            {:else}
              {#each $recentFilesStore as file}
                <button
                  on:click={() => handleOpenRecent(file)}
                  class="w-full text-left bg-slate-900 border border-slate-800 hover:border-labaccent/50 rounded-lg p-3 group transition-all"
                >
                  <div class="flex items-center justify-between">
                    <div class="flex items-center gap-3 overflow-hidden">
                      <FileText
                        class="w-5 h-5 text-slate-500 group-hover:text-labaccent transition-colors shrink-0"
                      />
                      <div class="overflow-hidden">
                        <div class="text-slate-200 font-medium truncate text-sm">{file.name}</div>
                        <div class="text-slate-500 text-[10px] truncate">{file.path}</div>
                      </div>
                    </div>
                    <ChevronRight
                      class="w-4 h-4 text-slate-600 group-hover:text-labaccent opacity-0 group-hover:opacity-100 transition-all shrink-0"
                    />
                  </div>
                  <div class="text-slate-600 text-[9px] mt-2 text-right">
                    {formatDate(file.timestamp)}
                  </div>
                </button>
              {/each}
            {/if}
          </div>
        </div>
      </div>

      <!-- Patička -->
      <div
        class="p-3 bg-slate-950 border-t border-slate-800 flex items-center justify-between text-xs text-slate-500"
      >
        <div class="flex items-center gap-1">
          Autor: <span class="text-slate-300 font-medium">Cyril Veverka</span>
        </div>
        <button
          on:click={openGithub}
          class="flex items-center gap-1.5 hover:text-white transition-colors"
        >
          <Github class="w-3.5 h-3.5" />
          <span>github.com/Sherlock3721/DPI</span>
        </button>
      </div>
    </div>
  </div>
{/if}
