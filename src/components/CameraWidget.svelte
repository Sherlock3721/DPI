<script lang="ts">
  import { run } from 'svelte/legacy';

  import { onMount, onDestroy } from "svelte";
  import {
    RotateCw,
    FlipHorizontal,
    Camera,
    CameraOff,
    Video,
    CircleDot,
    Square,
    Maximize,
    X,
  } from "lucide-svelte";
  import { save } from "@tauri-apps/plugin-dialog";
  import { writeFile } from "@tauri-apps/plugin-fs";
  import CustomSelect from "./CustomSelect.svelte";
  import { cameraStream, cameraAvailable } from "../stores/cameraStore";
  import { settingsStore } from "../stores/settingsStore";

  let videoElementInline: HTMLVideoElement = $state()!;
  let videoElementPopup: HTMLVideoElement = $state()!;
  let mediaStream: MediaStream | null = $state(null);
  let devices: MediaDeviceInfo[] = $state([]);
  let isActive = $state(false);
  // Preference kamery žijí v settings.json — store je jediný zdroj pravdy,
  // změny jdou výhradně přes settingsStore.persistPatch (viz handlery níže).
  let rotation = $derived((($settingsStore.camera_rotation % 360) + 360) % 360); // 0, 90, 180, 270
  let isMirrored = $derived($settingsStore.camera_mirror);
  let selectedDeviceId = $state("");
  let deviceIdInitDone = $state(false);
  run(() => {
    if (!deviceIdInitDone && $settingsStore.camera_device_id) {
      selectedDeviceId = $settingsStore.camera_device_id;
      deviceIdInitDone = true;
    }
  });
  let errorMessage = $state("");
  let sessionSaveDir = "";

  // Nahrávání videa
  let isRecording = $state(false);
  let mediaRecorder: MediaRecorder | null = null;
  let recordedChunks: Blob[] = [];
  let recordingCanvas: HTMLCanvasElement;
  let recordingCtx: CanvasRenderingContext2D | null;
  let recordingInterval: number;

  // Maximalizace okna
  let isMaximized = $state(false);
  let popupX = $state(100);
  let popupY = $state(100);
  let popupW = $state(640);
  let popupH = $state(480);
  let isDragging = false;
  let isResizing = false;
  let startX = 0;
  let startY = 0;
  let initialX = 0;
  let initialY = 0;
  let initialW = 0;
  let initialH = 0;

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return {
      destroy() {
        if (node.parentNode) {
          node.parentNode.removeChild(node);
        }
      },
    };
  }

  run(() => {
    if (mediaStream) {
      if (videoElementInline && videoElementInline.srcObject !== mediaStream) {
        videoElementInline.srcObject = mediaStream;
      }
      if (videoElementPopup && videoElementPopup.srcObject !== mediaStream) {
        videoElementPopup.srcObject = mediaStream;
      }
    }
  });

  async function getDevices() {
    try {
      // Try to get permission first with basic constraints
      const tempStream = await navigator.mediaDevices.getUserMedia({ video: true });
      tempStream.getTracks().forEach((track) => track.stop());
    } catch (e) {
      console.warn("Dočasný stream pro získání názvů kamer nelze otevřít:", e);
    }

    try {
      const allDevices = await navigator.mediaDevices.enumerateDevices();
      const videoDevices = allDevices.filter((device) => device.kind === "videoinput");

      // Deduplikace (na Linuxu v4l2 často vrací stejnou kameru dvakrát s různým deviceId)
      const unique = [];
      const seenLabels = new Set();
      for (const d of videoDevices) {
        const label = d.label || d.deviceId;
        // Občas má metadata stream index, ale většinou chceme jen ten první hlavní
        if (!seenLabels.has(label)) {
          seenLabels.add(label);
          unique.push(d);
        }
      }
      devices = unique;
      cameraAvailable.set(devices.length > 0);

      if (devices.length > 0) {
        // Pokud nemáme uložené ID, zkusíme najít integrovanou (obsahuje "integrated" nebo "built-in"), jinak první
        if (!selectedDeviceId || !devices.find((d) => d.deviceId === selectedDeviceId)) {
          const internal = devices.find(
            (d) =>
              d.label.toLowerCase().includes("integrated") ||
              d.label.toLowerCase().includes("built-in")
          );
          selectedDeviceId = internal ? internal.deviceId : devices[0].deviceId;
          deviceIdInitDone = true;
          settingsStore.persistPatch({ camera_device_id: selectedDeviceId });
        }
      }
    } catch (e) {
      console.error("Chyba při získávání video zařízení:", e);
      cameraAvailable.set(false);
    }
  }

  async function startCamera() {
    errorMessage = "";
    if (mediaStream) {
      stopCamera();
    }

    try {
      // Basic constraints to avoid GStreamer crashes on Linux
      const constraints = {
        video: selectedDeviceId ? { deviceId: { exact: selectedDeviceId } } : true,
      };
      mediaStream = await navigator.mediaDevices.getUserMedia(constraints);
      if (videoElementInline) videoElementInline.srcObject = mediaStream;
      if (videoElementPopup) videoElementPopup.srcObject = mediaStream;
      cameraStream.set(mediaStream);
      isActive = true;
    } catch (e: any) {
      console.error("Nelze spustit kameru s HD rozlišením, zkouším fallback:", e);
      try {
        // Fallback pro kamery nepodporující 1280x720
        const fallbackConstraints = {
          video: selectedDeviceId ? { deviceId: { exact: selectedDeviceId } } : true,
        };
        mediaStream = await navigator.mediaDevices.getUserMedia(fallbackConstraints);
        if (videoElementInline) videoElementInline.srcObject = mediaStream;
        if (videoElementPopup) videoElementPopup.srcObject = mediaStream;
        cameraStream.set(mediaStream);
        isActive = true;
      } catch (err2) {
        console.error("Nelze spustit kameru ani s fallbackem:", err2);
        errorMessage = "Kameru nelze spustit. Zkontrolujte připojení.";
        isActive = false;
      }
    }
  }

  function stopCamera() {
    if (isRecording) {
      toggleRecording(); // Uloží video pokud natáčíme
    }
    if (mediaStream) {
      mediaStream.getTracks().forEach((track) => track.stop());
      mediaStream = null;
      cameraStream.set(null);
    }
    if (videoElementInline) videoElementInline.srcObject = null;
    if (videoElementPopup) videoElementPopup.srcObject = null;
    isActive = false;
  }

  function toggleCamera() {
    if (isActive) {
      stopCamera();
    } else {
      startCamera();
    }
  }

  function rotateCamera() {
    settingsStore.persistPatch({ camera_rotation: (rotation + 90) % 360 });
  }

  function toggleMirror() {
    settingsStore.persistPatch({ camera_mirror: !isMirrored });
  }

  async function takeScreenshot() {
    if ((!videoElementInline && !videoElementPopup) || !isActive) return;

    try {
      const targetVideo = isMaximized ? videoElementPopup : videoElementInline;
      if (!targetVideo) return;

      const isRotated90 = rotation === 90 || rotation === 270;
      const canvas = document.createElement("canvas");
      canvas.width = isRotated90 ? targetVideo.videoHeight : targetVideo.videoWidth;
      canvas.height = isRotated90 ? targetVideo.videoWidth : targetVideo.videoHeight;

      const ctx = canvas.getContext("2d");
      if (ctx) {
        ctx.translate(canvas.width / 2, canvas.height / 2);
        ctx.rotate((rotation * Math.PI) / 180);
        if (isMirrored) ctx.scale(-1, 1);

        ctx.drawImage(
          targetVideo,
          -targetVideo.videoWidth / 2,
          -targetVideo.videoHeight / 2,
          targetVideo.videoWidth,
          targetVideo.videoHeight
        );

        const dataUrl = canvas.toDataURL("image/png");
        const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
        const fileName = `screenshot_${timestamp}.png`;
        let filePath: string | null = "";

        if (!sessionSaveDir) {
          filePath = await save({
            defaultPath: fileName,
            filters: [{ name: "Image", extensions: ["png"] }],
          });
          if (filePath) {
            const sep = filePath.includes("\\") ? "\\" : "/";
            sessionSaveDir = filePath.substring(0, filePath.lastIndexOf(sep));
          }
        } else {
          const sep = sessionSaveDir.includes("\\") ? "\\" : "/";
          filePath = `${sessionSaveDir}${sep}${fileName}`;
        }

        if (filePath) {
          const base64Data = dataUrl.split(",")[1];
          const binaryString = window.atob(base64Data);
          const bytes = new Uint8Array(binaryString.length);
          for (let i = 0; i < binaryString.length; i++) {
            bytes[i] = binaryString.charCodeAt(i);
          }
          await writeFile(filePath, bytes);
        }
      }
    } catch (e) {
      console.error("Chyba při pořizování snímku:", e);
    }
  }

  function toggleRecording() {
    if (!isActive || !mediaStream) return;
    if (isRecording) {
      mediaRecorder?.stop();
      isRecording = false;
      cancelAnimationFrame(recordingInterval);
    } else {
      recordedChunks = [];
      
      const targetVideo = isMaximized ? videoElementPopup : videoElementInline;
      if (!targetVideo) return;

      if (!recordingCanvas) {
        recordingCanvas = document.createElement("canvas");
        recordingCtx = recordingCanvas.getContext("2d");
      }

      function drawFrame() {
        if (!isRecording) return;
        const currentVideo = isMaximized ? videoElementPopup : videoElementInline;
        if (!currentVideo || !recordingCtx || !recordingCanvas) {
          recordingInterval = requestAnimationFrame(drawFrame);
          return;
        }

        const isRotated90 = rotation === 90 || rotation === 270;
        const expectedWidth = isRotated90 ? currentVideo.videoHeight : currentVideo.videoWidth;
        const expectedHeight = isRotated90 ? currentVideo.videoWidth : currentVideo.videoHeight;
        
        if (recordingCanvas.width !== expectedWidth || recordingCanvas.height !== expectedHeight) {
          recordingCanvas.width = expectedWidth || 640;
          recordingCanvas.height = expectedHeight || 480;
        }

        if (recordingCanvas.width > 0 && recordingCanvas.height > 0) {
          recordingCtx.save();
          recordingCtx.clearRect(0, 0, recordingCanvas.width, recordingCanvas.height);
          recordingCtx.translate(recordingCanvas.width / 2, recordingCanvas.height / 2);
          recordingCtx.rotate((rotation * Math.PI) / 180);
          if (isMirrored) recordingCtx.scale(-1, 1);
          
          recordingCtx.drawImage(
            currentVideo,
            -currentVideo.videoWidth / 2,
            -currentVideo.videoHeight / 2,
            currentVideo.videoWidth,
            currentVideo.videoHeight
          );
          recordingCtx.restore();
        }
        
        recordingInterval = requestAnimationFrame(drawFrame);
      }

      // Record from canvas instead of raw camera stream
      const canvasStream = recordingCanvas.captureStream(30);

      const options = { mimeType: "video/webm" };
      try {
        mediaRecorder = new MediaRecorder(canvasStream, options);
      } catch (e) {
        console.warn("Mime type not supported, using default", e);
        mediaRecorder = new MediaRecorder(canvasStream);
      }
      mediaRecorder.ondataavailable = (e) => {
        if (e.data.size > 0) recordedChunks.push(e.data);
      };
      mediaRecorder.onstop = async () => {
        try {
          const blob = new Blob(recordedChunks, { type: "video/webm" });
          const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
          const fileName = `zaznam_${timestamp}.webm`;
          let filePath: string | null = "";

          if (!sessionSaveDir) {
            filePath = await save({
              defaultPath: fileName,
              filters: [{ name: "Video", extensions: ["webm"] }],
            });
            if (filePath) {
              const sep = filePath.includes("\\") ? "\\" : "/";
              sessionSaveDir = filePath.substring(0, filePath.lastIndexOf(sep));
            }
          } else {
            const sep = sessionSaveDir.includes("\\") ? "\\" : "/";
            filePath = `${sessionSaveDir}${sep}${fileName}`;
          }

          if (filePath) {
            const arrayBuffer = await blob.arrayBuffer();
            const bytes = new Uint8Array(arrayBuffer);
            await writeFile(filePath, bytes);
          }
        } catch (err) {
          console.error("Chyba při ukládání videa:", err);
        }
      };
      
      isRecording = true;
      drawFrame();
      mediaRecorder.start();
    }
  }

  function toggleMaximize() {
    isMaximized = !isMaximized;
  }

  // Popup Window drag & resize logic
  function onMouseDownDrag(e: MouseEvent) {
    isDragging = true;
    startX = e.clientX;
    startY = e.clientY;
    initialX = popupX;
    initialY = popupY;
  }
  function onMouseDownResize(e: MouseEvent) {
    isResizing = true;
    startX = e.clientX;
    startY = e.clientY;
    initialW = popupW;
    initialH = popupH;
    e.stopPropagation();
  }
  function onMouseMoveWindow(e: MouseEvent) {
    if (isDragging) {
      popupX = initialX + (e.clientX - startX);
      popupY = initialY + (e.clientY - startY);
    } else if (isResizing) {
      popupW = Math.max(300, initialW + (e.clientX - startX));
      popupH = Math.max(200, initialH + (e.clientY - startY));
    }
  }
  function onMouseUpWindow() {
    isDragging = false;
    isResizing = false;
  }

  function handleSourceChange() {
    if (selectedDeviceId) {
      deviceIdInitDone = true;
      settingsStore.persistPatch({ camera_device_id: selectedDeviceId });
    }
    if (isActive) {
      startCamera();
    }
  }

  onMount(() => {
    getDevices().then(() => {
      startCamera();
    });
  });

  onDestroy(() => {
    stopCamera();
  });
</script>

<svelte:window onmousemove={onMouseMoveWindow} onmouseup={onMouseUpWindow} />

<div class="flex flex-col gap-2 bg-slate-900/40 p-2.5 rounded-lg border border-slate-800/80">
  <!-- HEADLINE AND QUICK CONTROLS -->
  <div class="flex flex-col gap-1 pb-1 border-b border-slate-800/50">
    <div class="flex items-center justify-between gap-1.5">
      <span
        class="font-bold text-[10px] text-slate-400 uppercase tracking-wider flex items-center gap-1 shrink-0"
      >
        <Video class="w-3.5 h-3.5 text-slate-500" /> Kamera
      </span>

      <div class="flex items-center gap-1 min-w-0">
        <!-- RECORD BUTTON -->
        <button
          onclick={toggleRecording}
          disabled={!isActive}
          title={isRecording ? "Zastavit nahrávání a uložit" : "Začít nahrávat video"}
          class="p-1 rounded border transition-colors shrink-0 {isRecording
            ? 'bg-red-500/20 border-red-500/50 text-red-500 animate-pulse'
            : 'bg-slate-850 border-slate-800 text-slate-300 hover:bg-slate-800 disabled:opacity-40'}"
        >
          {#if isRecording}
            <Square class="w-3.5 h-3.5 fill-current" />
          {:else}
            <CircleDot class="w-3.5 h-3.5" />
          {/if}
        </button>

        <!-- ROTATE BUTTON -->
        <button
          onclick={rotateCamera}
          disabled={!isActive}
          title="Otočit obraz o 90°"
          class="p-1 rounded-sm bg-slate-850 border border-slate-800 hover:bg-slate-800 text-slate-300 disabled:opacity-40 transition-colors shrink-0"
        >
          <RotateCw class="w-3.5 h-3.5" />
        </button>

        <!-- MIRROR BUTTON -->
        <button
          onclick={toggleMirror}
          disabled={!isActive}
          title="Zrcadlově obrátit obraz"
          class="p-1 rounded border transition-colors shrink-0 {isMirrored && isActive
            ? 'bg-labaccent/20 border-labaccent/50 text-labaccent'
            : 'bg-slate-850 border-slate-800 text-slate-300 hover:bg-slate-800 disabled:opacity-40'}"
        >
          <FlipHorizontal class="w-3.5 h-3.5" />
        </button>

        <!-- SCREENSHOT BUTTON -->
        <button
          onclick={takeScreenshot}
          disabled={!isActive}
          title="Uložit snímek"
          class="p-1 rounded-sm bg-slate-850 border border-slate-800 hover:bg-slate-800 text-slate-300 disabled:opacity-40 transition-colors shrink-0"
        >
          <Camera class="w-3.5 h-3.5" />
        </button>

        <!-- MAXIMIZE BUTTON -->
        <button
          onclick={toggleMaximize}
          disabled={!isActive}
          title="Maximalizovat okno s videem"
          class="p-1 rounded-sm bg-slate-850 border border-slate-800 hover:bg-slate-800 text-slate-300 disabled:opacity-40 transition-colors shrink-0"
        >
          <Maximize class="w-3.5 h-3.5" />
        </button>

        <!-- TOGGLE SWITCH BUTTON -->
        <button
          onclick={toggleCamera}
          title={isActive ? "Vypnout kameru" : "Zapnout kameru"}
          class="text-[10px] font-bold px-2 py-1 rounded text-white flex items-center gap-1 transition-colors shrink-0 {isActive
            ? 'bg-labred hover:bg-red-600'
            : 'bg-labgreen hover:bg-green-600'}"
        >
          {#if isActive}
            <CameraOff class="w-3 h-3" />
          {:else}
            <Camera class="w-3 h-3" />
          {/if}
        </button>
      </div>
    </div>

    <!-- SOURCE SELECT — jen při více zařízeních -->
    {#if devices.length > 1}
      <CustomSelect
        bind:value={selectedDeviceId}
        on:change={handleSourceChange}
        options={devices.map((dev, i) => ({
          value: dev.deviceId,
          label: dev.label || `Kamera ${i + 1}`,
        }))}
        placeholder="Vyberte kameru"
      />
    {/if}
  </div>

  <!-- VIEWFINDER AREA -->
  {#if !isMaximized && (isActive || errorMessage)}
    <div
      class="relative w-full aspect-video bg-black overflow-hidden flex items-center justify-center"
    >
      <!-- svelte-ignore a11y_media_has_caption -->
      <video
        bind:this={videoElementInline}
        autoplay
        playsinline
        muted
        disablePictureInPicture
        class="w-full h-full object-cover transition-transform duration-200 pointer-events-none {!isActive ||
        errorMessage
          ? 'hidden'
          : ''}"
        style="transform: rotate({rotation}deg) scaleX({isMirrored ? -1 : 1});"
      ></video>

      {#if !isActive || errorMessage}
        <div
          class="absolute inset-0 flex flex-col items-center justify-center gap-1 text-slate-500 text-[11px] p-4 text-center"
        >
          <CameraOff class="w-8 h-8 text-slate-650 mb-1" />
          {#if errorMessage}
            <span class="text-labred font-semibold">{errorMessage}</span>
          {:else}
            <span>Kamera vypnuta</span>
          {/if}
        </div>
      {/if}

      {#if isRecording}
        <div
          class="absolute top-2 left-2 flex items-center gap-1 bg-black/60 px-2 py-1 rounded-sm text-red-500 text-[10px] font-bold animate-pulse"
        >
          <CircleDot class="w-3 h-3" /> REC
        </div>
      {/if}
    </div>
  {/if}
</div>

<!-- FLOATING POPUP PRO MAXIMALIZOVANÉ VIDEO -->
{#if isMaximized}
  <div
    use:portal
    class="fixed bg-black border border-slate-700 shadow-2xl rounded-lg overflow-hidden flex flex-col z-999999"
    style="left: {popupX}px; top: {popupY}px; width: {popupW}px; height: {popupH}px;"
  >
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="h-8 bg-slate-800 flex items-center justify-between px-2 cursor-move shrink-0 select-none border-b border-slate-700"
      onmousedown={onMouseDownDrag}
    >
      <span class="text-xs font-bold text-slate-300 flex items-center gap-1"
        ><Video class="w-3.5 h-3.5" /> Kamera (Náhled)</span
      >
      <button
        onclick={() => (isMaximized = false)}
        class="p-1 hover:bg-slate-700 rounded-sm text-slate-300 transition-colors"
        ><X class="w-4 h-4" /></button
      >
    </div>

    <div class="relative flex-1 bg-black overflow-hidden flex items-center justify-center">
      <!-- svelte-ignore a11y_media_has_caption -->
      <video
        bind:this={videoElementPopup}
        autoplay
        playsinline
        muted
        disablePictureInPicture
        class="w-full h-full object-cover transition-transform duration-200 pointer-events-none {!isActive ||
        errorMessage
          ? 'hidden'
          : ''}"
        style="transform: rotate({rotation}deg) scaleX({isMirrored ? -1 : 1});"
      ></video>

      {#if !isActive || errorMessage}
        <div
          class="absolute inset-0 flex flex-col items-center justify-center gap-1 text-slate-500 text-[11px] p-4 text-center"
        >
          <CameraOff class="w-8 h-8 text-slate-650 mb-1" />
          {#if errorMessage}
            <span class="text-labred font-semibold">{errorMessage}</span>
          {:else}
            <span>Kamera vypnuta</span>
          {/if}
        </div>
      {/if}

      {#if isRecording}
        <div
          class="absolute top-2 left-2 flex items-center gap-1 bg-black/60 px-2 py-1 rounded-sm text-red-500 text-[10px] font-bold animate-pulse"
        >
          <CircleDot class="w-3 h-3" /> REC
        </div>
      {/if}
    </div>

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="absolute bottom-0 right-0 w-4 h-4 cursor-se-resize z-10"
      onmousedown={onMouseDownResize}
    >
      <svg
        class="w-full h-full text-slate-500"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <polyline points="15 3 21 3 21 9"></polyline>
        <polyline points="9 21 3 21 3 15"></polyline>
        <line x1="21" y1="3" x2="14" y2="10"></line>
        <line x1="3" y1="21" x2="10" y2="14"></line>
      </svg>
    </div>
  </div>
{/if}
