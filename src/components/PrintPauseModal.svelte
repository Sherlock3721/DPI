<script lang="ts">
  import { onMount } from "svelte";
  import { resume_app_pause, subscribe_printer_status } from "../lib/tauri";

  /** Modální dialog pauzy tisku (M0/M1/M601) a potvrzovacích dotazů.
   *  Požadavky se řadí do FIFO fronty — souběžné dialogy se vzájemně
   *  nepřepisují (přepsání slotu dřív ztrácelo resume_app_pause/resolve). */

  interface DialogRequest {
    message: string;
    showCancel: boolean;
    /** Pauza z tiskové fronty (APP_PAUSE) — potvrzení volá resume_app_pause. */
    fromPrintQueue: boolean;
    resolve: ((confirmed: boolean) => void) | null;
  }

  let queue: DialogRequest[] = [];
  let current: DialogRequest | null = $state(null);
  let isReady = $state(false);

  const FALLBACK_MSG = "Tisk pozastaven";

  // Klik stisknutý v prvních ARM_DELAY_MS po zobrazení dialog neukončí —
  // jde o zbytkový vstup z akce, která dialog otevřela/potvrdila předchozí.
  const ARM_DELAY_MS = 400;

  function showNext() {
    current = queue.shift() ?? null;
    if (current) {
      isReady = false;
      setTimeout(() => {
        isReady = true;
      }, ARM_DELAY_MS);
    }
  }

  function enqueue(req: DialogRequest) {
    queue.push(req);
    if (!current) showNext();
  }

  /** Pauza z tiskové fronty (APP_PAUSE event) — potvrzení volá resume_app_pause. */
  export function showFromPrintQueue(message: string) {
    enqueue({
      message: message || FALLBACK_MSG,
      showCancel: false,
      fromPrintQueue: true,
      resolve: null,
    });
  }

  /** Potvrzení s možností zrušení — resolvne true (potvrzeno) nebo false (zrušeno). */
  export function confirmOrCancel(message: string): Promise<boolean> {
    return new Promise((resolve) => {
      enqueue({
        message: message || FALLBACK_MSG,
        showCancel: true,
        fromPrintQueue: false,
        resolve,
      });
    });
  }

  async function finish(confirmed: boolean) {
    if (!isReady || !current) return;
    const done = current;
    showNext();
    if (done.fromPrintQueue) {
      await resume_app_pause();
    } else {
      done.resolve?.(confirmed);
    }
  }

  // Pauza ukončená mimo dialog (tlačítko Pokračovat v panelu, stop tisku,
  // odpojení) — dialog z tiskové fronty zavřeme, resume_app_pause() už není
  // namístě. Potvrzovací dotazy (confirmOrCancel) se stavu tiskárny netýkají.
  onMount(() => {
    const unsub = subscribe_printer_status((status) => {
      if (status.is_paused) return;
      queue = queue.filter((r) => !r.fromPrintQueue);
      if (current?.fromPrintQueue) showNext();
    }).catch(() => null); // web režim bez Tauri eventů
    return () => {
      unsub.then((fn) => fn?.());
    };
  });
</script>

{#if current !== null}
  <div
    class="fixed inset-0 bg-black/80 backdrop-blur-xs flex items-center justify-center z-100"
    role="presentation"
  >
    <div
      class="glass-panel rounded-xl p-6 max-w-sm w-full mx-4 text-center shadow-2xl border border-slate-600 flex flex-col items-center gap-4"
      role="dialog"
      aria-modal="true"
    >
      <p class="text-slate-100 font-semibold text-sm">{current.message}</p>
      <div class="flex gap-2 w-full">
        {#if current.showCancel}
          <button
            onclick={() => finish(false)}
            onkeydown={(e: KeyboardEvent) => e.preventDefault()}
            disabled={!isReady}
            class="flex-1 px-4 py-2 rounded-lg border border-slate-600 bg-slate-800/60 text-slate-300 hover:border-slate-400 font-medium transition-colors disabled:opacity-50"
          >
            Zrušit
          </button>
        {/if}
        <button
          onclick={() => finish(true)}
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
