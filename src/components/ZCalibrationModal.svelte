<script lang="ts">
  import { createEventDispatcher, onMount, onDestroy } from "svelte";
  import { send_manual_command } from "../lib/tauri";
  import { ArrowUp, ArrowDown, Check, CameraOff, Info } from "lucide-svelte";

  export let glassZTheoretical: number;
  export let blockHeight: number = 34.0;

  const dispatch = createEventDispatcher();
  let calibrationShift = 0.0;
  let isMoving = false;
  let selectedStep = 0.1;
  let localBlockHeight = blockHeight;

  const steps = [0.5, 0.1, 0.05, 0.01];

  // Camera
  let videoElement: HTMLVideoElement;
  let mediaStream: MediaStream | null = null;
  let cameraError = false;

  const rotation = parseInt(localStorage.getItem("preferredCameraRotation") || "0");
  const isMirrored = localStorage.getItem("preferredCameraMirror") === "true";

  async function moveZ(direction: 1 | -1) {
    if (isMoving) return;
    const amount = direction * selectedStep;
    isMoving = true;
    try {
      await send_manual_command(`G91\nG1 Z${amount.toFixed(3)} F300\nG90\n`);
      calibrationShift += amount;
    } catch (e) {
      console.error("Chyba při pohybu Z:", e);
    } finally {
      isMoving = false;
    }
  }

  async function startCamera() {
    try {
      const savedDeviceId = localStorage.getItem("preferredCameraId") || "";
      const constraints = { video: savedDeviceId ? { deviceId: { exact: savedDeviceId } } : true };
      mediaStream = await navigator.mediaDevices.getUserMedia(constraints);
      if (videoElement) videoElement.srcObject = mediaStream;
    } catch (e) {
      cameraError = true;
    }
  }

  function stopCamera() {
    if (mediaStream) {
      mediaStream.getTracks().forEach((t) => t.stop());
      mediaStream = null;
    }
  }

  function confirm() {
    dispatch("confirm", { shift: calibrationShift, blockHeight: localBlockHeight });
  }

  function cancel() {
    dispatch("cancel");
  }

  onMount(() => startCamera());
  onDestroy(() => stopCamera());
</script>

<div class="fixed inset-0 bg-black/80 backdrop-blur-sm flex items-center justify-center z-50 p-4">
  <div
    class="glass-panel w-full max-w-4xl rounded-xl flex flex-col border border-labaccent shadow-2xl overflow-hidden"
  >
    <!-- HEADER -->
    <div class="flex items-center justify-between px-5 py-3 border-b border-slate-700">
      <h3 class="text-base font-bold text-slate-200">Přesná kalibrace skla (Z-Shift)</h3>
      <span class="text-sm font-mono text-labaccent">
        {(glassZTheoretical + calibrationShift).toFixed(3)} mm
        <span class="text-slate-400 text-xs ml-2"
          >({calibrationShift >= 0 ? "+" : ""}{calibrationShift.toFixed(3)})</span
        >
      </span>
    </div>

    <!-- INSTRUCTION -->
    <div class="flex items-start gap-2 px-5 py-2.5 border-b border-slate-700 bg-blue-500/10">
      <Info class="w-4 h-4 text-blue-400 mt-0.5 shrink-0" />
      <p class="text-xs text-blue-300 leading-relaxed">
        Sjeďte tryskou úplně nad substrát pro nastavení nulové výšky.
        <span class="text-blue-400/70">(Pozn. u tisku bude k této výšce přičtena výška vrstvy)</span
        >
      </p>
    </div>

    <!-- CONTROLS BAR: výška kalibrátoru vlevo, krok vpravo -->
    <div
      class="flex items-center justify-between gap-4 px-5 py-2.5 border-b border-slate-700 bg-slate-900/40"
    >
      <!-- Calibrator height -->
      <div class="flex items-center gap-2 shrink-0">
        <span class="text-xs text-slate-400">Tloušťka papíru/plastu:</span>
        <div class="flex items-center gap-1">
          <input
            type="number"
            bind:value={localBlockHeight}
            min="0"
            step="0.1"
            class="w-20 bg-slate-800 border border-slate-600 text-slate-200 text-xs font-mono rounded px-2 py-1 text-right focus:outline-none focus:border-labaccent"
          />
          <span class="text-xs text-slate-400">mm</span>
        </div>
      </div>

      <!-- Step selector -->
      <div class="flex items-center gap-2">
        <span class="text-xs text-slate-400 shrink-0">Krok:</span>
        {#each steps as step}
          <button
            on:click={() => (selectedStep = step)}
            class="px-3 py-1 text-xs font-mono rounded transition-colors {selectedStep === step
              ? 'bg-labaccent text-white'
              : 'bg-slate-800 text-slate-300 hover:bg-slate-700'}"
          >
            {step} mm
          </button>
        {/each}
      </div>
    </div>

    <!-- MAIN AREA -->
    <div class="flex" style="height: 420px;">
      <!-- VIDEO (90%) -->
      <div
        class="relative bg-black overflow-hidden flex items-center justify-center"
        style="flex: 9;"
      >
        <!-- svelte-ignore a11y-media-has-caption -->
        <video
          bind:this={videoElement}
          autoplay
          playsinline
          muted
          disablePictureInPicture
          class="w-full h-full object-cover pointer-events-none {cameraError ? 'hidden' : ''}"
          style="transform: rotate({rotation}deg) scaleX({isMirrored ? -1 : 1});"
        ></video>
        {#if cameraError}
          <div class="flex flex-col items-center gap-2 text-slate-500 text-xs">
            <CameraOff class="w-8 h-8 text-slate-600" />
            <span>Kamera nedostupná</span>
          </div>
        {/if}
      </div>

      <!-- ARROWS (10%) -->
      <div
        class="flex flex-col items-center justify-center gap-4 bg-slate-900/60 border-l border-slate-700 px-3"
        style="flex: 1;"
      >
        <button
          on:click={() => moveZ(1)}
          disabled={isMoving}
          title="Nahoru +{selectedStep} mm"
          class="w-full aspect-square flex items-center justify-center rounded-xl border-2 border-slate-600 bg-slate-800 hover:bg-slate-700 hover:border-labaccent text-slate-200 hover:text-labaccent transition-all disabled:opacity-40 disabled:cursor-not-allowed shadow-lg active:scale-95"
        >
          <ArrowUp class="w-8 h-8" />
        </button>
        <button
          on:click={() => moveZ(-1)}
          disabled={isMoving}
          title="Dolů -{selectedStep} mm"
          class="w-full aspect-square flex items-center justify-center rounded-xl border-2 border-slate-600 bg-slate-800 hover:bg-slate-700 hover:border-labaccent text-slate-200 hover:text-labaccent transition-all disabled:opacity-40 disabled:cursor-not-allowed shadow-lg active:scale-95"
        >
          <ArrowDown class="w-8 h-8" />
        </button>
      </div>
    </div>

    <!-- FOOTER -->
    <div class="flex items-center justify-end gap-3 px-5 py-3 border-t border-slate-700">
      <button
        on:click={cancel}
        class="px-4 py-2 text-sm text-slate-300 hover:text-white hover:bg-slate-800 rounded transition-colors"
      >
        Zrušit
      </button>
      <button
        on:click={confirm}
        class="bg-labaccent hover:bg-opacity-80 text-white text-sm font-bold px-6 py-2 rounded-lg transition-colors flex items-center gap-2"
      >
        <Check class="w-4 h-4" /> Potvrdit a Tisknout
      </button>
    </div>
  </div>
</div>
