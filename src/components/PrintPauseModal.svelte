<script lang="ts">
  import { resume_app_pause } from "../lib/tauri";

  /** Modální dialog pauzy tisku (M0/M1/M601 + LIVE_ADJUST tok).
   *  Dva režimy:
   *  - `waitFor(msg)` — programová pauza; vrácená Promise se resolvne potvrzením,
   *  - `showFromPrintQueue(msg)` — pauza z tiskové fronty; potvrzení pošle
   *    resume_app_pause() backendu.
   *  Potvrzení: klik na tlačítko/backdrop, Enter nebo mezerník. */

  let pauseMessage: string | null = $state(null);
  let pauseResolve: (() => void) | null = null;
  let pauseIsFromPrintQueue = false;
  // pauseShownAt: kdy dialog vznikl (filtruje zbytkové keydown eventy ze spouštěcí akce)
  // backdropPointerDownAt: kdy padl pointerdown přímo na backdrop — musí být >= pauseShownAt,
  //   jinak jde o click-through z tlačítka, které dialog otevřelo
  let pauseShownAt = 0;
  let backdropPointerDownAt = 0;

  const FALLBACK_MSG = "Stiskněte Enter, Mezerník nebo klikněte pro pokračování";

  // Doba "odjištění" dialogu: VŠECHNY cesty potvrzení (tlačítko, backdrop, klávesy)
  // se ignorují prvních ARM_DELAY_MS po zobrazení. Chrání proti zbytkovému vstupu
  // z akce, která dialog otevřela/potvrdila předchozí: druhý klik z double-clicku
  // (odskakující spínač bezdrátové myši) a key auto-repeat — Windows má výchozí
  // repeat delay ~250 ms, takže kratší guard tam nestačil.
  const ARM_DELAY_MS = 400;

  function isArmed(): boolean {
    return Date.now() - pauseShownAt >= ARM_DELAY_MS;
  }

  function show(message: string) {
    pauseMessage = message || FALLBACK_MSG;
    pauseShownAt = Date.now();
    backdropPointerDownAt = 0;
  }

  /** Pauza z tiskové fronty (APP_PAUSE event) — potvrzení volá resume_app_pause. */
  export function showFromPrintQueue(message: string) {
    show(message || FALLBACK_MSG);
    pauseIsFromPrintQueue = true;
    pauseResolve = null;
  }

  /** Programová pauza — Promise se resolvne po potvrzení uživatelem. */
  export function waitFor(message: string): Promise<void> {
    return new Promise((resolve) => {
      setTimeout(() => {
        show(message);
        pauseIsFromPrintQueue = false;
        pauseResolve = resolve;
      }, 50);
    });
  }

  async function dismiss() {
    if (pauseMessage === null || !isArmed()) return;
    pauseMessage = null;
    if (pauseIsFromPrintQueue) {
      pauseIsFromPrintQueue = false;
      await resume_app_pause();
    } else if (pauseResolve) {
      pauseResolve();
      pauseResolve = null;
    }
  }

  function handleBackdropPointerDown(event: PointerEvent) {
    if (event.target !== event.currentTarget) return;
    backdropPointerDownAt = Date.now();
  }

  async function handleBackdropClick(event: MouseEvent) {
    if (event.target !== event.currentTarget) return;
    // Akceptovat pouze klik, jehož pointerdown nastal AŽ PO zobrazení dialogu.
    // Tím se eliminuje "click-through" — mouseup z tlačítka, které dialog spustilo.
    if (backdropPointerDownAt >= pauseShownAt) {
      await dismiss();
    }
  }

  async function handlePauseKeydown(event: KeyboardEvent) {
    if (pauseMessage === null) return;
    if (event.key !== "Enter" && event.key !== " ") return;
    // Auto-repeat držené klávesy nesmí potvrdit dialog — klávesa musí být
    // stisknuta znovu až po jeho zobrazení.
    if (event.repeat) return;
    if (!isArmed()) return;
    event.preventDefault();
    await dismiss();
  }
</script>

<svelte:window onkeydown={handlePauseKeydown} />

{#if pauseMessage !== null}
  <div
    class="fixed inset-0 bg-black/80 backdrop-blur-xs flex items-center justify-center z-100"
    onpointerdown={handleBackdropPointerDown}
    onclick={handleBackdropClick}
    role="presentation"
  >
    <div
      class="glass-panel rounded-xl p-6 max-w-sm w-full mx-4 text-center shadow-2xl border border-slate-600"
      role="dialog"
      aria-modal="true"
    >
      <p class="text-slate-100 font-semibold text-sm mb-3">{pauseMessage}</p>
      <p class="text-slate-400 text-xs mb-4">{FALLBACK_MSG}</p>
      <button
        onclick={dismiss}
        class="px-5 py-2 bg-labaccent hover:bg-blue-600 text-white rounded-lg font-bold text-sm transition-colors"
      >
        Pokračovat →
      </button>
    </div>
  </div>
{/if}
