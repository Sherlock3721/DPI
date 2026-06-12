<script lang="ts">
  import { resume_app_pause } from "../lib/tauri";

  /** Modální dialog pauzy tisku (M0/M1/M601 + LIVE_ADJUST tok).
   *  Dva režimy:
   *  - `waitFor(msg)` — programová pauza; vrácená Promise se resolvne potvrzením,
   *  - `showFromPrintQueue(msg)` — pauza z tiskové fronty; potvrzení pošle
   *    resume_app_pause() backendu.
   *  Potvrzení: pouze klávesa Enter — myš/touch (klik na tlačítko, backdrop)
   *  i Mezerník byly na Windows nespolehlivé (zbytkový vstup z akce, která
   *  dialog otevřela, dialog okamžitě zavřel). */

  let pauseMessage: string | null = $state(null);
  let pauseResolve: (() => void) | null = null;
  let pauseIsFromPrintQueue = false;
  // pauseShownAt: kdy dialog vznikl — filtruje zbytkový Enter ze spouštěcí akce
  let pauseShownAt = 0;

  const FALLBACK_MSG = "Stiskněte Enter pro pokračování";

  // Enter stisknutý v prvních ARM_DELAY_MS po zobrazení dialog neukončí —
  // jde o zbytkový vstup z akce, která dialog otevřela/potvrdila předchozí.
  const ARM_DELAY_MS = 400;

  function show(message: string) {
    console.log("[PAUSE-DEBUG] show():", message);
    pauseMessage = message || FALLBACK_MSG;
    pauseShownAt = Date.now();
  }

  /** Pauza z tiskové fronty (APP_PAUSE event) — potvrzení volá resume_app_pause. */
  export function showFromPrintQueue(message: string) {
    console.log("[PAUSE-DEBUG] showFromPrintQueue():", message);
    show(message || FALLBACK_MSG);
    pauseIsFromPrintQueue = true;
    pauseResolve = null;
  }

  /** Programová pauza — Promise se resolvne po potvrzení uživatelem. */
  export function waitFor(message: string): Promise<void> {
    console.log("[PAUSE-DEBUG] waitFor():", message);
    return new Promise((resolve) => {
      setTimeout(() => {
        show(message);
        pauseIsFromPrintQueue = false;
        pauseResolve = resolve;
      }, 50);
    });
  }

  async function dismiss() {
    console.log("[PAUSE-DEBUG] dismiss(), pauseIsFromPrintQueue =", pauseIsFromPrintQueue);
    pauseMessage = null;
    if (pauseIsFromPrintQueue) {
      pauseIsFromPrintQueue = false;
      await resume_app_pause();
      console.log("[PAUSE-DEBUG] resume_app_pause() hotovo");
    } else if (pauseResolve) {
      pauseResolve();
      pauseResolve = null;
    }
  }

  async function handlePauseKeydown(event: KeyboardEvent) {
    if (pauseMessage === null) return;
    console.log(
      "[PAUSE-DEBUG] keydown:",
      event.key,
      "repeat =",
      event.repeat,
      "od zobrazení =",
      Date.now() - pauseShownAt,
      "ms",
    );
    if (event.key !== "Enter") return;
    // Auto-repeat držené klávesy nesmí potvrdit dialog — klávesa musí být
    // stisknuta znovu až po jeho zobrazení.
    if (event.repeat) return;
    if (Date.now() - pauseShownAt < ARM_DELAY_MS) return;
    event.preventDefault();
    await dismiss();
  }
</script>

<svelte:window onkeydown={handlePauseKeydown} />

{#if pauseMessage !== null}
  <div
    class="fixed inset-0 bg-black/80 backdrop-blur-xs flex items-center justify-center z-100"
    role="presentation"
  >
    <div
      class="glass-panel rounded-xl p-6 max-w-sm w-full mx-4 text-center shadow-2xl border border-slate-600"
      role="dialog"
      aria-modal="true"
    >
      <p class="text-slate-100 font-semibold text-sm mb-3">{pauseMessage}</p>
      <p class="text-slate-400 text-xs">{FALLBACK_MSG}</p>
    </div>
  </div>
{/if}
