<script lang="ts">
  import { send_manual_command, subscribe_serial_rx } from "../lib/tauri";
  import { Terminal as TerminalIcon, Send } from "lucide-svelte";
  import { onMount, onDestroy } from "svelte";

  export let compact = false;

  let manualCommand = "";
  let logs: string[] = ["Terminál připraven. Zadejte G-kód (např. G28, G0 X10 Y10)..."];
  let consoleContainer: HTMLDivElement;
  let unsub: () => void;

  onMount(async () => {
    unsub = await subscribe_serial_rx((line) => {
      const trimmed = line.trim();
      // Ignorovat běžnou telemetrii na kterou se uživatel neptal
      if (
        !trimmed ||
        trimmed === "ok" ||
        trimmed.startsWith("ok T:") ||
        trimmed.startsWith("echo: ") ||
        trimmed.startsWith("T:")
      ) {
        return;
      }

      logs = [...logs, `< ${trimmed}`];
      // Keep only last 500 lines to prevent memory issues
      if (logs.length > 500) {
        logs = logs.slice(logs.length - 500);
      }
      setTimeout(() => {
        if (consoleContainer) {
          consoleContainer.scrollTop = consoleContainer.scrollHeight;
        }
      }, 10);
    });
  });

  onDestroy(() => {
    if (unsub) unsub();
  });

  async function handleSend() {
    if (!manualCommand.trim()) return;
    const cmd = manualCommand.trim();
    logs = [...logs, `> ${cmd}`];
    manualCommand = "";

    // Automatické posunutí scrollbaru dolů
    setTimeout(() => {
      if (consoleContainer) {
        consoleContainer.scrollTop = consoleContainer.scrollHeight;
      }
    }, 20);

    try {
      await send_manual_command(cmd);
    } catch (e) {
      logs = [...logs, `< Chyba: ${e}`];
    }
  }

  function handleKeyPress(e: KeyboardEvent) {
    if (e.key === "Enter") {
      handleSend();
    }
  }
</script>

<div
  class={compact
    ? "flex flex-col h-full gap-1.5"
    : "glass-panel rounded-lg p-4 flex flex-col h-full"}
>
  {#if !compact}
    <div class="flex items-center gap-2 border-b border-slate-700/50 pb-2 mb-3">
      <TerminalIcon class="w-5 h-5 text-labaccent" />
      <span class="font-semibold text-slate-200">Sériová Konzole</span>
    </div>
  {/if}

  <div
    bind:this={consoleContainer}
    class="flex-1 bg-slate-950/80 rounded border border-slate-800 p-2 font-mono text-[10px] overflow-y-auto mb-1.5 select-text {compact
      ? 'h-[110px]'
      : 'h-48'}"
  >
    {#each logs as log}
      <div
        class={log.startsWith(">")
          ? "text-labaccent"
          : log.startsWith("< Chyba")
            ? "text-labred"
            : "text-slate-400"}
      >
        {log}
      </div>
    {/each}
  </div>

  <div class="flex gap-1.5 shrink-0">
    <input
      type="text"
      bind:value={manualCommand}
      on:keypress={handleKeyPress}
      placeholder="G-code..."
      class="input-premium py-0.5 text-xs flex-1"
    />
    <button
      on:click={handleSend}
      class="bg-labaccent hover:bg-blue-600 text-white p-1 rounded flex items-center justify-center transition-colors duration-200"
    >
      <Send class="w-3.5 h-3.5" />
    </button>
  </div>
</div>
