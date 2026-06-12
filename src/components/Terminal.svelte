<script lang="ts">
  import { send_manual_command, subscribe_serial_rx } from "../lib/tauri";
  import { Terminal as TerminalIcon, Send } from "lucide-svelte";
  import { onMount, onDestroy } from "svelte";

  interface Props {
    compact?: boolean;
  }

  let { compact = false }: Props = $props();

  let manualCommand = $state("");
  let logs: string[] = $state(["Terminál připraven. Zadejte G-kód (např. G28, G0 X10 Y10)..."]);
  let consoleContainer: HTMLDivElement = $state()!;
  let unsub: () => void;

  // Historie příkazů pro šipky nahoru/dolů
  let commandHistory: string[] = [];
  let historyIndex = -1;

  // Dávkování příchozích serial-rx zpráv (zabraňuje zmrznutí UI při přílivu zpráv)
  let pendingLogs: string[] = [];
  let flushScheduled = false;

  function scheduleFlush() {
    if (flushScheduled) return;
    flushScheduled = true;
    requestAnimationFrame(() => {
      if (pendingLogs.length > 0) {
        const combined = [...logs, ...pendingLogs];
        logs = combined.length > 300 ? combined.slice(combined.length - 300) : combined;
        pendingLogs = [];
      }
      flushScheduled = false;
      if (consoleContainer) consoleContainer.scrollTop = consoleContainer.scrollHeight;
    });
  }

  onMount(async () => {
    unsub = await subscribe_serial_rx((line) => {
      const trimmed = line.trim();
      if (
        !trimmed ||
        trimmed === "ok" ||
        trimmed.startsWith("ok T:") ||
        trimmed.startsWith("echo: ") ||
        trimmed.startsWith("T:")
      ) {
        return;
      }
      pendingLogs.push(`< ${trimmed}`);
      scheduleFlush();
    });
  });

  onDestroy(() => {
    if (unsub) unsub();
  });

  async function handleSend() {
    if (!manualCommand.trim()) return;
    const cmd = manualCommand.trim();

    // Uložit do historie (bez duplicit po sobě)
    if (commandHistory.length === 0 || commandHistory[commandHistory.length - 1] !== cmd) {
      commandHistory = [...commandHistory, cmd];
      if (commandHistory.length > 50) commandHistory = commandHistory.slice(-50);
    }
    historyIndex = -1;

    logs = [...logs, `> ${cmd}`];
    manualCommand = "";
    requestAnimationFrame(() => {
      if (consoleContainer) consoleContainer.scrollTop = consoleContainer.scrollHeight;
    });

    try {
      await send_manual_command(cmd);
    } catch (e) {
      logs = [...logs, `< Chyba: ${e}`];
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      handleSend();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (commandHistory.length === 0) return;
      historyIndex = Math.min(historyIndex + 1, commandHistory.length - 1);
      manualCommand = commandHistory[commandHistory.length - 1 - historyIndex];
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (historyIndex <= 0) {
        historyIndex = -1;
        manualCommand = "";
      } else {
        historyIndex--;
        manualCommand = commandHistory[commandHistory.length - 1 - historyIndex];
      }
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
    class="flex-1 min-h-0 bg-slate-950/80 rounded border border-slate-800 p-2 font-mono text-[10px] overflow-y-auto mb-1.5 select-text {compact
      ? 'h-[110px]'
      : ''}"
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
      onkeydown={handleKeyDown}
      placeholder="G-code..."
      class="input-premium py-0.5 text-xs flex-1"
    />
    <button
      onclick={handleSend}
      class="bg-labaccent hover:bg-blue-600 text-white p-1 rounded-sm flex items-center justify-center transition-colors duration-200"
    >
      <Send class="w-3.5 h-3.5" />
    </button>
  </div>
</div>
