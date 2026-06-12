<script lang="ts">
  import { onMount, onDestroy } from "svelte";

  let canvas: HTMLCanvasElement = $state()!;
  let animFrameId: number;
  let resizeObserver: ResizeObserver;

  interface Flake {
    x: number;
    y: number;
    r: number;       // radius
    speed: number;   // px/frame (svislá rychlost)
    drift: number;   // amplituda bočního pohybu
    phase: number;   // fáze sinusoidy (0–2π)
    freq: number;    // frekvence bočního pohybu
    opacity: number;
  }

  const FLAKE_COUNT = 90;
  let flakes: Flake[] = [];
  let w = 0;
  let h = 0;

  function randomFlake(startAbove = false): Flake {
    return {
      x:       Math.random() * w,
      y:       startAbove ? -Math.random() * h : Math.random() * h,
      r:       0.8 + Math.random() * 2.8,
      speed:   0.25 + Math.random() * 0.7,
      drift:   0.4 + Math.random() * 1.2,
      phase:   Math.random() * Math.PI * 2,
      freq:    0.004 + Math.random() * 0.008,
      opacity: 0.35 + Math.random() * 0.55,
    };
  }

  function resize() {
    if (!canvas) return;
    w = canvas.offsetWidth;
    h = canvas.offsetHeight;
    canvas.width  = w;
    canvas.height = h;
  }

  function init() {
    resize();
    flakes = Array.from({ length: FLAKE_COUNT }, () => randomFlake(false));
  }

  let tick = 0;

  function draw() {
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, w, h);

    tick++;

    for (const f of flakes) {
      f.y += f.speed;
      f.x += Math.sin(f.phase + tick * f.freq) * f.drift * 0.15;

      // Pokud vylezl ze spodku nebo boků, znovu ho zaplácni nahoře
      if (f.y > h + f.r * 2) {
        f.y = -f.r * 2;
        f.x = Math.random() * w;
        f.phase = Math.random() * Math.PI * 2;
      }
      if (f.x < -f.r * 4)  f.x = w + f.r;
      if (f.x > w + f.r * 4) f.x = -f.r;

      ctx.save();
      ctx.globalAlpha = f.opacity;
      ctx.beginPath();
      ctx.arc(f.x, f.y, f.r, 0, Math.PI * 2);
      ctx.fillStyle = "#ffffff";
      ctx.shadowColor = "rgba(200, 225, 255, 0.6)";
      ctx.shadowBlur = f.r * 2.5;
      ctx.fill();
      ctx.restore();
    }

    animFrameId = requestAnimationFrame(draw);
  }

  onMount(() => {
    init();

    resizeObserver = new ResizeObserver(() => {
      resize();
    });
    resizeObserver.observe(canvas.parentElement ?? document.body);

    animFrameId = requestAnimationFrame(draw);
  });

  onDestroy(() => {
    cancelAnimationFrame(animFrameId);
    resizeObserver?.disconnect();
  });
</script>

<canvas bind:this={canvas} class="snow-canvas" aria-hidden="true"></canvas>

<style>
  .snow-canvas {
    position: fixed;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
    z-index: 9999;
  }
</style>
