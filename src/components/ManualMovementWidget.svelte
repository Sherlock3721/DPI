<script lang="ts">
  import { onMount } from "svelte";
  import { send_manual_command, subscribe_printer_status, type PrinterStatus } from "../lib/tauri";
  import {
    ChevronUp,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    Home,
    ArrowUp,
    ArrowDown,
    Send,
  } from "lucide-svelte";

  let currentStep = 10.0;
  let customGCode = "";

  let status: PrinterStatus = {
    is_connected: false,
    is_printing: false,
    is_paused: false,
    current_x: 0.0,
    current_y: 0.0,
    current_z: 0.0,
    temp_extruder: 0.0,
    temp_bed: 0.0,
    progress: 0,
    total_dist: 0.0,
    time_remaining: 0.0,
  };

  // Ovládací prvky lze aktivovat pouze tehdy, když je tiskárna připojena a zrovna netiskne
  $: canControl = status.is_connected && !status.is_printing;

  function setStep(step: number) {
    if (!canControl) return;
    currentStep = step;
  }

  async function move(axis: "X" | "Y" | "Z", direction: number) {
    if (!canControl) return;
    const value = currentStep * direction;
    // Marlin relativní pohyb: G91, G0, G90
    const gcode = `G91\nG0 ${axis}${value.toFixed(2)} F3000\nG90`;
    await sendGCode(gcode);
  }

  async function homeXY() {
    if (!canControl) return;
    await sendGCode("G28 X Y");
  }

  async function homeZ() {
    if (!canControl) return;
    await sendGCode("G28 Z");
  }

  async function runCalibration() {
    if (!canControl) return;
    await sendGCode("G80");
  }

  async function handleSendCustom() {
    const cmd = customGCode.trim();
    if (cmd && canControl) {
      await sendGCode(cmd);
      customGCode = "";
    }
  }

  async function sendGCode(gcode: string) {
    try {
      for (const line of gcode.split("\n")) {
        const cleanLine = line.trim();
        if (cleanLine) {
          await send_manual_command(cleanLine);
        }
      }
    } catch (e) {
      console.error("Selhalo odeslání G-kódu:", e);
    }
  }

  onMount(() => {
    const unsubscribe = subscribe_printer_status((newStatus) => {
      status = newStatus;
    });

    return () => {
      unsubscribe.then((unsub) => unsub());
    };
  });
</script>

<div class="flex flex-col gap-3 p-3 rounded-lg bg-slate-900/60 border border-slate-800 text-xs">
  <!-- VOLBA KROKU -->
  <div class="flex flex-col gap-1">
    <span class="text-[9px] text-slate-500 font-bold uppercase tracking-wider"
      >Krok posunu [mm]</span
    >
    <div class="grid grid-cols-4 gap-1">
      {#each [0.1, 1, 10, 50] as step}
        <button
          type="button"
          on:click={() => setStep(step)}
          disabled={!canControl}
          class="py-1 rounded font-bold transition-all border {currentStep === step && canControl
            ? 'bg-orange-500 border-orange-500 text-black shadow-lg shadow-orange-500/10'
            : 'bg-slate-850 border-slate-800 text-slate-300 hover:bg-slate-800 disabled:opacity-40'}"
        >
          {step}
        </button>
      {/each}
    </div>
  </div>

  <!-- JOG SMĚRY -->
  <div class="grid grid-cols-5 gap-3 items-center">
    <!-- POHYB X/Y (3/5) -->
    <div class="col-span-3 flex flex-col gap-1.5 items-center">
      <span class="text-[9px] text-slate-500 font-bold uppercase tracking-wider w-full text-center"
        >Pohyb X / Y</span
      >

      <div class="grid grid-cols-3 gap-1">
        <!-- R1: Empty, Y+, Empty -->
        <div></div>
        <button
          type="button"
          on:click={() => move("Y", 1)}
          disabled={!canControl}
          title="Y+"
          class="w-10 h-8 flex items-center justify-center rounded bg-slate-850 hover:bg-slate-800 border border-slate-800 text-slate-200 disabled:opacity-40 transition-colors"
        >
          <ChevronUp class="w-4 h-4" />
        </button>
        <div></div>

        <!-- R2: X-, Home XY, X+ -->
        <button
          type="button"
          on:click={() => move("X", -1)}
          disabled={!canControl}
          title="X-"
          class="w-10 h-8 flex items-center justify-center rounded bg-slate-850 hover:bg-slate-800 border border-slate-800 text-slate-200 disabled:opacity-40 transition-colors"
        >
          <ChevronLeft class="w-4 h-4" />
        </button>
        <button
          type="button"
          on:click={homeXY}
          disabled={!canControl}
          title="Home X Y"
          class="w-10 h-8 flex items-center justify-center rounded bg-orange-500/20 hover:bg-orange-500/30 border border-orange-500/50 text-orange-500 disabled:opacity-40 transition-colors"
        >
          <Home class="w-4 h-4" />
        </button>
        <button
          type="button"
          on:click={() => move("X", 1)}
          disabled={!canControl}
          title="X+"
          class="w-10 h-8 flex items-center justify-center rounded bg-slate-850 hover:bg-slate-800 border border-slate-800 text-slate-200 disabled:opacity-40 transition-colors"
        >
          <ChevronRight class="w-4 h-4" />
        </button>

        <!-- R3: Empty, Y-, Empty -->
        <div></div>
        <button
          type="button"
          on:click={() => move("Y", -1)}
          disabled={!canControl}
          title="Y-"
          class="w-10 h-8 flex items-center justify-center rounded bg-slate-850 hover:bg-slate-800 border border-slate-800 text-slate-200 disabled:opacity-40 transition-colors"
        >
          <ChevronDown class="w-4 h-4" />
        </button>
        <div></div>
      </div>
    </div>

    <!-- POHYB Z (2/5) -->
    <div class="col-span-2 flex flex-col gap-1.5 items-center border-l border-slate-850 pl-2">
      <span class="text-[9px] text-slate-500 font-bold uppercase tracking-wider w-full text-center"
        >Osa Z</span
      >

      <div class="flex flex-col gap-1">
        <button
          type="button"
          on:click={() => move("Z", 1)}
          disabled={!canControl}
          title="Z+"
          class="w-12 h-8 flex items-center justify-center rounded bg-slate-850 hover:bg-slate-800 border border-slate-800 text-slate-200 disabled:opacity-40 transition-colors"
        >
          <ArrowUp class="w-4 h-4 text-blue-400" />
        </button>
        <button
          type="button"
          on:click={homeZ}
          disabled={!canControl}
          title="Home Z"
          class="w-12 h-8 flex items-center justify-center rounded bg-blue-500/20 hover:bg-blue-500/30 border border-blue-500/50 text-blue-400 disabled:opacity-40 transition-colors"
        >
          <Home class="w-4 h-4 text-blue-400" />
        </button>
        <button
          type="button"
          on:click={() => move("Z", -1)}
          disabled={!canControl}
          title="Z-"
          class="w-12 h-8 flex items-center justify-center rounded bg-slate-850 hover:bg-slate-800 border border-slate-800 text-slate-200 disabled:opacity-40 transition-colors"
        >
          <ArrowDown class="w-4 h-4 text-blue-400" />
        </button>
      </div>
    </div>
  </div>

  <!-- VLASTNÍ G-CODE -->
  <form on:submit|preventDefault={handleSendCustom} class="flex gap-1">
    <input
      type="text"
      bind:value={customGCode}
      disabled={!canControl}
      placeholder="Vlastní G-code..."
      class="flex-1 input-premium py-1 text-[11px]"
    />
    <button
      type="submit"
      disabled={!canControl || !customGCode.trim()}
      class="bg-slate-800 border border-slate-700 hover:bg-slate-700 text-slate-200 font-bold px-2 py-1 rounded flex items-center justify-center disabled:opacity-40 transition-colors"
    >
      <Send class="w-3.5 h-3.5" />
    </button>
  </form>

  <!-- SERVISNÍ TLAČÍTKA -->
  <div class="grid grid-cols-2 gap-2 mt-0.5">
    <button
      type="button"
      on:click={runCalibration}
      disabled={!canControl}
      class="py-1.5 rounded font-bold text-center border bg-yellow-500/10 border-yellow-550/40 text-yellow-500 hover:bg-yellow-500/20 disabled:opacity-40 transition-colors"
    >
      KALIBRACE (G80)
    </button>
    <div class="grid grid-cols-2 gap-1">
      <button
        type="button"
        on:click={() => { sendGCode("M17"); }}
        disabled={!canControl}
        title="Zapnout motory (M17)"
        class="py-1.5 rounded font-bold text-center border transition-all bg-slate-850 border-slate-800 text-slate-300 hover:bg-slate-700 disabled:opacity-40"
      >
        M17 ZAP
      </button>
      <button
        type="button"
        on:click={() => { sendGCode("M84"); }}
        disabled={!canControl}
        title="Vypnout motory (M84)"
        class="py-1.5 rounded font-bold text-center border transition-all bg-slate-850 border-slate-800 text-slate-300 hover:bg-slate-700 disabled:opacity-40"
      >
        M84 VYP
      </button>
    </div>
  </div>
</div>
