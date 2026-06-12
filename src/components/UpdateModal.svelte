<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { check, type Update } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { getVersion } from "@tauri-apps/api/app";
  import { Download, RefreshCw, CheckCircle, XCircle, X } from "lucide-svelte";

  interface Props {
    autoCheck?: boolean;
  }

  let { autoCheck = false }: Props = $props();

  const dispatch = createEventDispatcher();

  type Phase =
    | "checking"
    | "up-to-date"
    | "available"
    | "downloading"
    | "ready"
    | "error";

  let phase: Phase = $state("checking");
  let updateInfo: Update | null = $state(null);
  let errorMsg = $state("");
  let downloaded = $state(0);
  let total = $state(0);
  let currentVersion = $state("");
  getVersion().then((v) => (currentVersion = v));

  let progress = $derived(total > 0 ? Math.round((downloaded / total) * 100) : 0);

  async function runCheck() {
    phase = "checking";
    errorMsg = "";
    downloaded = 0;
    total = 0;
    try {
      const update = await check();
      if (update) {
        updateInfo = update;
        phase = "available";
      } else {
        phase = "up-to-date";
        if (autoCheck) {
          // Při automatické kontrole tiše zavřeme — uživatel nepotřebuje vědět
          setTimeout(() => dispatch("close"), 800);
        }
      }
    } catch (e: any) {
      if (autoCheck) {
        // Při automatické kontrole tiše zavřeme — server nemusí být dostupný
        dispatch("close");
        return;
      }
      const msg = String(e);
      if (msg.includes("fetch") || msg.includes("release JSON") || msg.includes("network")) {
        errorMsg = "Nepodařilo se spojit se serverem aktualizací.\nZkontrolujte připojení k internetu nebo zkuste znovu později.";
      } else if (msg.includes("fallback platforms") || msg.includes("were not found in the response")) {
        errorMsg = "Pro tuto platformu nejsou dostupné automatické aktualizace.\nNejnovější verzi najdete na GitHubu:\ngithub.com/Sherlock3721/DPI/releases";
      } else {
        errorMsg = msg;
      }
      phase = "error";
    }
  }

  async function startDownload() {
    if (!updateInfo) return;
    phase = "downloading";
    try {
      await updateInfo.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
        } else if (event.event === "Finished") {
          phase = "ready";
        }
      });
      phase = "ready";
    } catch (e: any) {
      errorMsg = String(e);
      phase = "error";
    }
  }

  async function doRelaunch() {
    await relaunch();
  }

  // Spustíme kontrolu hned při vytvoření komponenty
  runCheck();
</script>

<div class="fixed inset-0 bg-black/80 backdrop-blur-xs flex items-center justify-center z-200 p-4">
  <div class="glass-panel w-full max-w-md rounded-xl border border-slate-700 shadow-2xl overflow-hidden">
    <!-- HEADER -->
    <div class="flex items-center justify-between px-5 py-3 border-b border-slate-700">
      <h3 class="text-sm font-bold text-slate-200">Aktualizace aplikace</h3>
      {#if phase !== "downloading"}
        <button
          onclick={() => dispatch("close")}
          class="text-slate-400 hover:text-slate-200 transition-colors"
        >
          <X class="w-4 h-4" />
        </button>
      {/if}
    </div>

    <!-- BODY -->
    <div class="px-5 py-6 flex flex-col items-center gap-4 min-h-[160px] justify-center">
      {#if phase === "checking"}
        <RefreshCw class="w-8 h-8 text-labaccent animate-spin" />
        <p class="text-slate-300 text-sm">Kontroluji dostupné aktualizace…</p>

      {:else if phase === "up-to-date"}
        <CheckCircle class="w-8 h-8 text-labgreen" />
        <p class="text-slate-200 text-sm font-semibold">Máte nejnovější verzi</p>
        <p class="text-slate-400 text-xs">Žádná aktualizace není k dispozici.{currentVersion ? ` (verze ${currentVersion})` : ""}</p>

      {:else if phase === "available"}
        <Download class="w-8 h-8 text-labaccent" />
        <div class="text-center">
          <p class="text-slate-200 text-sm font-semibold">Dostupná aktualizace</p>
          <p class="text-slate-400 text-xs mt-1">
            Verze <span class="text-labaccent font-mono">{updateInfo?.version}</span> je připravena ke stažení.
          </p>
        </div>

      {:else if phase === "downloading"}
        <div class="w-full flex flex-col items-center gap-3">
          <p class="text-slate-300 text-sm">Stahuji aktualizaci…</p>
          <div class="w-full bg-slate-800 rounded-full h-2.5 overflow-hidden border border-slate-700">
            <div
              class="bg-labaccent h-full rounded-full transition-all duration-300"
              style="width: {total > 0 ? progress : 100}%; {total === 0 ? 'animation: pulse 1.5s infinite;' : ''}"
            ></div>
          </div>
          {#if total > 0}
            <p class="text-slate-400 text-xs">
              {progress}% ({(downloaded / 1024 / 1024).toFixed(1)} / {(total / 1024 / 1024).toFixed(1)} MB)
            </p>
          {:else}
            <p class="text-slate-400 text-xs">Probíhá stahování…</p>
          {/if}
        </div>

      {:else if phase === "ready"}
        <CheckCircle class="w-8 h-8 text-labgreen" />
        <div class="text-center">
          <p class="text-slate-200 text-sm font-semibold">Aktualizace nainstalována</p>
          <p class="text-slate-400 text-xs mt-1">Restartujte aplikaci pro aktivaci nové verze.</p>
        </div>

      {:else if phase === "error"}
        <XCircle class="w-8 h-8 text-labred" />
        <div class="text-center">
          <p class="text-slate-200 text-sm font-semibold">Chyba při aktualizaci</p>
          <p class="text-slate-400 text-xs mt-1 break-all">{errorMsg}</p>
        </div>
      {/if}
    </div>

    <!-- FOOTER -->
    <div class="flex items-center justify-end gap-2 px-5 py-3 border-t border-slate-700">
      {#if phase === "available"}
        <button
          onclick={() => dispatch("close")}
          class="px-4 py-1.5 text-xs text-slate-400 hover:text-slate-200 hover:bg-slate-800 rounded-sm transition-colors"
        >
          Přeskočit
        </button>
        <button
          onclick={startDownload}
          class="px-4 py-1.5 text-xs bg-labaccent hover:bg-blue-600 text-white font-bold rounded-sm transition-colors flex items-center gap-1.5"
        >
          <Download class="w-3.5 h-3.5" /> Stáhnout a nainstalovat
        </button>

      {:else if phase === "ready"}
        <button
          onclick={doRelaunch}
          class="px-4 py-1.5 text-xs bg-labgreen hover:bg-green-600 text-white font-bold rounded-sm transition-colors flex items-center gap-1.5"
        >
          <RefreshCw class="w-3.5 h-3.5" /> Restartovat aplikaci
        </button>

      {:else if phase === "error"}
        <button
          onclick={() => dispatch("close")}
          class="px-4 py-1.5 text-xs text-slate-400 hover:text-slate-200 hover:bg-slate-800 rounded-sm transition-colors"
        >
          Zavřít
        </button>
        <button
          onclick={runCheck}
          class="px-4 py-1.5 text-xs bg-labaccent hover:bg-blue-600 text-white font-bold rounded-sm transition-colors"
        >
          Zkusit znovu
        </button>

      {:else if phase === "up-to-date"}
        <button
          onclick={() => dispatch("close")}
          class="px-4 py-1.5 text-xs bg-labaccent hover:bg-blue-600 text-white font-bold rounded-sm transition-colors"
        >
          Zavřít
        </button>

      {:else if phase !== "downloading"}
        <button
          onclick={() => dispatch("close")}
          class="px-4 py-1.5 text-xs text-slate-400 hover:text-slate-200 hover:bg-slate-800 rounded-sm transition-colors"
        >
          Zavřít
        </button>
      {/if}
    </div>
  </div>
</div>
