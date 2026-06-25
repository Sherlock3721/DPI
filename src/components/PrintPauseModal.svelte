<script lang="ts">
  import { resume_app_pause } from "../lib/tauri";

  /** Modální dialog pauzy tisku (M0/M1/M601 + LIVE_ADJUST tok). */

  let pauseMessage: string | null = $state(null);
  let pauseResolve: (() => void) | null = null;
  let pauseReject: (() => void) | null = null;
  let pauseIsFromPrintQueue = false;
  let showCancelButton = $state(false);
  let isReady = $state(false);

  const FALLBACK_MSG = "Tisk pozastaven";

  // Klik stisknutý v prvních ARM_DELAY_MS po zobrazení dialog neukončí —
  // jde o zbytkový vstup z akce, která dialog otevřela/potvrdila předchozí.
  const ARM_DELAY_MS = 400;

  function show(message: string) {
    console.log("[PAUSE-DEBUG] show():", message);
    pauseMessage = message || FALLBACK_MSG;
    isReady = false;
    setTimeout(() => {
      isReady = true;
    }, ARM_DELAY_MS);
  }

  /** Pauza z tiskové fronty (APP_PAUSE event) — potvrzení volá resume_app_pause. */
  export function showFromPrintQueue(message: string) {
    console.log("[PAUSE-DEBUG] showFromPrintQueue():", message);
    show(message || FALLBACK_MSG);
    pauseIsFromPrintQueue = true;
    showCancelButton = false;
    pauseResolve = null;
    pauseReject = null;
  }

  /** Programová pauza — Promise se resolvne po potvrzení uživatelem. */
  export function waitFor(message: string): Promise<void> {
    console.log("[PAUSE-DEBUG] waitFor():", message);
    return new Promise((resolve) => {
      setTimeout(() => {
        show(message);
        pauseIsFromPrintQueue = false;
        showCancelButton = false;
        pauseResolve = resolve;
        pauseReject = null;
      }, 50);
    });
  }

  /** Potvrzení s možností zrušení — resolvne true (potvrzeno) nebo false (zrušeno). */
  export function confirmOrCancel(message: string): Promise<boolean> {
    console.log("[PAUSE-DEBUG] confirmOrCancel():", message);
    return new Promise((resolve) => {
      setTimeout(() => {
        show(message);
        pauseIsFromPrintQueue = false;
        showCancelButton = true;
        pauseResolve = () => resolve(true);
        pauseReject = () => resolve(false);
      }, 50);
    });
  }

  async function dismiss() {
    if (!isReady) return;
    console.log("[PAUSE-DEBUG] dismiss(), pauseIsFromPrintQueue =", pauseIsFromPrintQueue);
    pauseMessage = null;
    showCancelButton = false;
    if (pauseIsFromPrintQueue) {
      pauseIsFromPrintQueue = false;
      await resume_app_pause();
      console.log("[PAUSE-DEBUG] resume_app_pause() hotovo");
    } else if (pauseResolve) {
      pauseResolve();
      pauseResolve = null;
      pauseReject = null;
    }
  }

  function cancel() {
    if (!isReady) return;
    console.log("[PAUSE-DEBUG] cancel()");
    pauseMessage = null;
    showCancelButton = false;
    if (pauseReject) {
      pauseReject();
      pauseReject = null;
      pauseResolve = null;
    }
  }
</script>

{#if pauseMessage !== null}
  <div
    class="fixed inset-0 bg-black/80 backdrop-blur-xs flex items-center justify-center z-100"
    role="presentation"
  >
    <div
      class="glass-panel rounded-xl p-6 max-w-sm w-full mx-4 text-center shadow-2xl border border-slate-600 flex flex-col items-center gap-4"
      role="dialog"
      aria-modal="true"
    >
      <p class="text-slate-100 font-semibold text-sm">{pauseMessage}</p>
      <div class="flex gap-2 w-full">
        {#if showCancelButton}
          <button
            onclick={cancel}
            onkeydown={(e: KeyboardEvent) => e.preventDefault()}
            disabled={!isReady}
            class="flex-1 px-4 py-2 rounded-lg border border-slate-600 bg-slate-800/60 text-slate-300 hover:border-slate-400 font-medium transition-colors disabled:opacity-50"
          >
            Zrušit
          </button>
        {/if}
        <button
          onclick={dismiss}
          onkeydown={(e: KeyboardEvent) => e.preventDefault()}
          disabled={!isReady}
          class="flex-1 px-4 py-2 rounded-lg bg-labaccent hover:bg-blue-600 text-white font-medium transition-colors disabled:opacity-50"
        >
          Pokračovat
        </button>
      </div>
    </div>
  </div>
{/if}
