<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { X, Download } from "lucide-svelte";
  import NumberInput from "./NumberInput.svelte";
  import { projectStore } from "../stores/projectStore";
  import type { LayoutPosition } from "../lib/tauri";
  import { save } from "@tauri-apps/plugin-dialog";
  import { writeTextFile } from "@tauri-apps/plugin-fs";

  export let isOpen = false;
  const dispatch = createEventDispatcher();
  function close() { dispatch("close"); }

  // === PARAMETRY ===
  let topFrameThick   = 10.0;  // výška horního rámu (pružinová zóna)
  let rightFrameThick = 10.0;  // šířka pravého rámu (pružinová zóna)
  let leftBorderW     = 3.0;   // levý pevný okraj
  let bottomBorderH   = 3.0;   // spodní pevný okraj
  let springWidth     = 10.0;  // šířka jednoho pružinového elementu (mm)
  let springCountX    = 3;     // počet pružin v ose X (horní rám)
  let springCountY    = 1;     // počet pružin v ose Y (pravý rám)
  let springBends     = 4;     // počet U-ohybů jedné pružiny
  let cornerDiam      = 3.0;   // průměr výseče v rozích (mm)
  let offsetTop       = 2.0;
  let offsetBottom    = 2.0;
  let offsetLeft      = 2.0;
  let offsetRight     = 2.0;
  let bracketThick    = 3.0;   // tloušťka pro 3D export (mm)

  // === DATA ZE STORE ===
  $: positions = $projectStore.positions.filter((p: LayoutPosition) => !p.is_prime);
  $: glassW    = $projectStore.params?.slide_w ?? 25.0;
  $: glassH    = $projectStore.params?.slide_h ?? 75.0;

  $: glassBBox = (() => {
    if (positions.length === 0) return { minX: 0, minY: 0, maxX: glassW, maxY: glassH };
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const p of positions) {
      const pw = p.width  || glassW;
      const ph = p.height || glassH;
      minX = Math.min(minX, p.x);
      minY = Math.min(minY, p.y);
      maxX = Math.max(maxX, p.x + pw);
      maxY = Math.max(maxY, p.y + ph);
    }
    return { minX, minY, maxX, maxY };
  })();

  $: ggW = Math.max(1, glassBBox.maxX - glassBBox.minX);
  $: ggH = Math.max(1, glassBBox.maxY - glassBBox.minY);

  $: bW = Math.max(10, leftBorderW + offsetLeft + ggW + offsetRight + rightFrameThick);
  $: bH = Math.max(10, topFrameThick + offsetTop + ggH + offsetBottom + bottomBorderH);

  $: gox = leftBorderW  + offsetLeft;
  $: goy = topFrameThick + offsetTop;

  $: glassRects = (() => {
    if (positions.length === 0) return [{ x: gox, y: goy, w: glassW, h: glassH }];
    return positions.map((p: LayoutPosition) => ({
      x: gox + (p.x - glassBBox.minX),
      y: goy + (p.y - glassBBox.minY),
      w: p.width  || glassW,
      h: p.height || glassH,
    }));
  })();

  // Pružiny X — v horním rámu, rovnoměrně rozloženy přes šířku skupiny skel
  $: xSprings = (() => {
    if (springCountX <= 0 || ggW <= 0) return [];
    const sw  = Math.min(springWidth, ggW / (springCountX + 0.5));
    const gap = (ggW - springCountX * sw) / (springCountX + 1);
    return Array.from({ length: springCountX }, (_, i) => ({
      x: gox + gap * (i + 1) + sw * i,
      y: 0,
      w: sw,
      h: topFrameThick,
    }));
  })();

  // Pružiny Y — v pravém rámu, rozloženy přes výšku skupiny skel
  $: ySprings = (() => {
    if (springCountY <= 0 || ggH <= 0) return [];
    const sh  = Math.min(springWidth, ggH / (springCountY + 0.5));
    const gap = (ggH - springCountY * sh) / (springCountY + 1);
    const sx  = gox + ggW + offsetRight;
    return Array.from({ length: springCountY }, (_, i) => ({
      x: sx,
      y: goy + gap * (i + 1) + sh * i,
      w: rightFrameThick,
      h: sh,
    }));
  })();

  $: cornerR = cornerDiam / 2;

  // === GENERÁTORY CEST ===

  // Horizontální pružina: řezy jdou vodorovně střídavě zleva/zprava
  function hSpringPath(x: number, y: number, w: number, h: number, bends: number): string {
    if (bends <= 0 || w < 1 || h < 1) return "";
    const cutT = Math.max(0.3, Math.min(1.2, h / (bends * 5)));
    const tabW = Math.max(1.0, w * 0.2);
    const legH = (h - bends * cutT) / (bends + 1);
    if (legH < 0.3) return "";
    let d = "";
    for (let i = 0; i < bends; i++) {
      const cy = y + legH * (i + 1) + cutT * i;
      d += i % 2 === 0
        ? `M ${x} ${cy} H ${x + w - tabW} V ${cy + cutT} H ${x} Z `
        : `M ${x + tabW} ${cy} H ${x + w} V ${cy + cutT} H ${x + tabW} Z `;
    }
    return d;
  }

  // Vertikální pružina: řezy jdou svisle střídavě shora/zdola
  function vSpringPath(x: number, y: number, w: number, h: number, bends: number): string {
    if (bends <= 0 || w < 1 || h < 1) return "";
    const cutT = Math.max(0.3, Math.min(1.2, w / (bends * 5)));
    const tabH = Math.max(1.0, h * 0.2);
    const legW = (w - bends * cutT) / (bends + 1);
    if (legW < 0.3) return "";
    let d = "";
    for (let i = 0; i < bends; i++) {
      const cx = x + legW * (i + 1) + cutT * i;
      d += i % 2 === 0
        ? `M ${cx} ${y} V ${y + h - tabH} H ${cx + cutT} V ${y} Z `
        : `M ${cx} ${y + tabH} V ${y + h} H ${cx + cutT} V ${y + tabH} Z `;
    }
    return d;
  }

  // === EXPORT SVG ===
  function generateExportSVG(): string {
    const glassHoles = glassRects.map(gr =>
      `  <rect x="${gr.x.toFixed(3)}" y="${gr.y.toFixed(3)}" width="${gr.w.toFixed(3)}" height="${gr.h.toFixed(3)}" fill="white"/>`
    ).join("\n");

    const xZones = xSprings.map(s => [
      `  <rect x="${s.x.toFixed(3)}" y="${s.y.toFixed(3)}" width="${s.w.toFixed(3)}" height="${s.h.toFixed(3)}" fill="#1e40af"/>`,
      `  <path d="${hSpringPath(s.x, s.y, s.w, s.h, springBends)}" fill="white"/>`,
    ].join("\n")).join("\n");

    const yZones = ySprings.map(s => [
      `  <rect x="${s.x.toFixed(3)}" y="${s.y.toFixed(3)}" width="${s.w.toFixed(3)}" height="${s.h.toFixed(3)}" fill="#1e40af"/>`,
      `  <path d="${vSpringPath(s.x, s.y, s.w, s.h, springBends)}" fill="white"/>`,
    ].join("\n")).join("\n");

    const corners = cornerR > 0 ? glassRects.flatMap(gr => [
      `  <circle cx="${gr.x.toFixed(3)}" cy="${gr.y.toFixed(3)}" r="${cornerR.toFixed(3)}" fill="white"/>`,
      `  <circle cx="${(gr.x + gr.w).toFixed(3)}" cy="${gr.y.toFixed(3)}" r="${cornerR.toFixed(3)}" fill="white"/>`,
      `  <circle cx="${gr.x.toFixed(3)}" cy="${(gr.y + gr.h).toFixed(3)}" r="${cornerR.toFixed(3)}" fill="white"/>`,
      `  <circle cx="${(gr.x + gr.w).toFixed(3)}" cy="${(gr.y + gr.h).toFixed(3)}" r="${cornerR.toFixed(3)}" fill="white"/>`,
    ]).join("\n") : "";

    return [
      `<?xml version="1.0" encoding="UTF-8"?>`,
      `<!-- DPI Bracket Mask: ${bW.toFixed(1)} x ${bH.toFixed(1)} mm, tloušťka: ${bracketThick} mm -->`,
      `<svg width="${bW.toFixed(3)}mm" height="${bH.toFixed(3)}mm"`,
      `  viewBox="0 0 ${bW.toFixed(3)} ${bH.toFixed(3)}"`,
      `  xmlns="http://www.w3.org/2000/svg">`,
      `  <rect x="0" y="0" width="${bW.toFixed(3)}" height="${bH.toFixed(3)}" fill="#3b82f6"/>`,
      xZones,
      yZones,
      glassHoles,
      corners,
      `</svg>`,
    ].filter(Boolean).join("\n");
  }

  async function handleExportSVG() {
    const content = generateExportSVG();
    try {
      const filePath = await save({
        filters: [{ name: "SVG soubor", extensions: ["svg"] }],
        defaultPath: "drzak.svg",
      });
      if (filePath) {
        await writeTextFile(filePath, content);
      }
    } catch (e) {
      alert(`Chyba při exportu SVG: ${e}`);
    }
  }
</script>

{#if isOpen}
<div
  class="fixed inset-0 bg-black/75 backdrop-blur-sm flex items-center justify-center z-50 p-4"
>
  <div
    class="glass-panel w-[94vw] h-[90vh] rounded-xl flex flex-col border border-slate-800 shadow-2xl overflow-hidden"
  >
    <!-- Hlavička -->
    <div class="flex items-center justify-between px-5 py-3 border-b border-slate-800 shrink-0">
      <div>
        <h3 class="text-sm font-bold text-slate-100 uppercase tracking-wider">
          Nastavení / export držáku
        </h3>
        <p class="text-[10px] text-slate-500 mt-0.5">
          Rozměr: {bW.toFixed(1)} × {bH.toFixed(1)} mm
          · {positions.length > 0 ? positions.length : 1} sklo/skel
          {#if positions.length === 0}
            <span class="text-yellow-500"> — žádné sklo nenačteno, zobrazen náhled</span>
          {/if}
        </p>
      </div>
      <button
        on:click={close}
        class="p-1.5 hover:bg-slate-800 rounded-md transition-colors text-slate-400 hover:text-slate-100"
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    <!-- Hlavní obsah -->
    <div class="flex-1 flex overflow-hidden min-h-0">

      <!-- LEVÁ ČÁST: SVG canvas (~75%) -->
      <div class="flex-1 overflow-hidden bg-slate-950 flex items-center justify-center p-6 min-w-0">
        <svg
          viewBox="0 0 {bW} {bH}"
          preserveAspectRatio="xMidYMid meet"
          style="max-width: 100%; max-height: 100%; display: block;"
        >
          <defs>
            <pattern id="bgrid" width="10" height="10" patternUnits="userSpaceOnUse">
              <path d="M 10 0 L 0 0 0 10" fill="none" stroke="#1e293b" stroke-width="0.15"/>
            </pattern>
          </defs>

          <!-- Mřížkové pozadí -->
          <rect width={bW} height={bH} fill="url(#bgrid)"/>

          <!-- Tělo držáku (modrá = tisknutelná plocha) -->
          <rect x="0" y="0" width={bW} height={bH} fill="#3b82f6"/>

          <!-- Pružinové zóny X (horní rám, tmavší modrá) -->
          {#each xSprings as s}
            <rect x={s.x} y={s.y} width={s.w} height={s.h} fill="#1e40af"/>
            <path d={hSpringPath(s.x, s.y, s.w, s.h, springBends)} fill="#0b0f19"/>
          {/each}

          <!-- Pružinové zóny Y (pravý rám, tmavší modrá) -->
          {#each ySprings as s}
            <rect x={s.x} y={s.y} width={s.w} height={s.h} fill="#1e40af"/>
            <path d={vSpringPath(s.x, s.y, s.w, s.h, springBends)} fill="#0b0f19"/>
          {/each}

          <!-- Díry pro skla -->
          {#each glassRects as gr}
            <rect x={gr.x} y={gr.y} width={gr.w} height={gr.h} fill="#0b0f19"/>
          {/each}

          <!-- Výseče v rozích skel -->
          {#if cornerR > 0}
            {#each glassRects as gr}
              <circle cx={gr.x}        cy={gr.y}        r={cornerR} fill="#0b0f19"/>
              <circle cx={gr.x + gr.w} cy={gr.y}        r={cornerR} fill="#0b0f19"/>
              <circle cx={gr.x}        cy={gr.y + gr.h} r={cornerR} fill="#0b0f19"/>
              <circle cx={gr.x + gr.w} cy={gr.y + gr.h} r={cornerR} fill="#0b0f19"/>
            {/each}
          {/if}

          <!-- Kontury skel (pro přehlednost) -->
          {#each glassRects as gr}
            <rect
              x={gr.x} y={gr.y} width={gr.w} height={gr.h}
              fill="none" stroke="rgba(255,255,255,0.15)" stroke-width="0.3"
            />
          {/each}

          <!-- Rozměrové popisky -->
          <text
            x={bW / 2} y={bH + 4}
            text-anchor="middle" font-size="3" fill="#94a3b8"
          >{bW.toFixed(1)} mm</text>
          <text
            x={bW + 2} y={bH / 2}
            text-anchor="start" font-size="3" fill="#94a3b8"
            transform="rotate(90, {bW + 2}, {bH / 2})"
          >{bH.toFixed(1)} mm</text>
        </svg>
      </div>

      <!-- PRAVÁ ČÁST: Ovládací panel (~25%) -->
      <div class="w-72 shrink-0 flex flex-col border-l border-slate-800 overflow-y-auto">
        <div class="flex flex-col gap-4 p-4">

          <!-- Rám -->
          <section>
            <div class="text-[9px] font-bold text-slate-400 uppercase tracking-widest border-b border-slate-800 pb-1 mb-2">
              Rám
            </div>
            <div class="flex flex-col gap-1.5">
              {#each [
                { label: "Horní rám (mm)",   bind: "topFrameThick",  val: topFrameThick,   min: 3, max: 50 },
                { label: "Pravý rám (mm)",   bind: "rightFrameThick", val: rightFrameThick, min: 3, max: 50 },
                { label: "Levý okraj (mm)",  bind: "leftBorderW",    val: leftBorderW,     min: 1, max: 20 },
                { label: "Dolní okraj (mm)", bind: "bottomBorderH",  val: bottomBorderH,   min: 1, max: 20 },
              ] as row}
                <div class="flex items-center justify-between gap-2">
                  <span class="text-[11px] text-slate-300 flex-1">{row.label}</span>
                  <div class="w-28">
                    {#if row.bind === "topFrameThick"}
                      <NumberInput bind:value={topFrameThick}   step={0.5} min={row.min} max={row.max}/>
                    {:else if row.bind === "rightFrameThick"}
                      <NumberInput bind:value={rightFrameThick} step={0.5} min={row.min} max={row.max}/>
                    {:else if row.bind === "leftBorderW"}
                      <NumberInput bind:value={leftBorderW}     step={0.5} min={row.min} max={row.max}/>
                    {:else}
                      <NumberInput bind:value={bottomBorderH}   step={0.5} min={row.min} max={row.max}/>
                    {/if}
                  </div>
                </div>
              {/each}
            </div>
          </section>

          <!-- Pružiny -->
          <section>
            <div class="text-[9px] font-bold text-slate-400 uppercase tracking-widest border-b border-slate-800 pb-1 mb-2">
              Pružiny
            </div>
            <div class="flex flex-col gap-1.5">
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Šířka pružiny (mm)</span>
                <div class="w-28">
                  <NumberInput bind:value={springWidth} step={0.5} min={3} max={60}/>
                </div>
              </div>
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Počet pružin X</span>
                <div class="w-28">
                  <NumberInput bind:value={springCountX} step={1} min={0} max={15}/>
                </div>
              </div>
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Počet pružin Y</span>
                <div class="w-28">
                  <NumberInput bind:value={springCountY} step={1} min={0} max={15}/>
                </div>
              </div>
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Počet zakřivení</span>
                <div class="w-28">
                  <NumberInput bind:value={springBends} step={1} min={1} max={30}/>
                </div>
              </div>
            </div>
          </section>

          <!-- Rohy -->
          <section>
            <div class="text-[9px] font-bold text-slate-400 uppercase tracking-widest border-b border-slate-800 pb-1 mb-2">
              Rohy
            </div>
            <div class="flex items-center justify-between gap-2">
              <span class="text-[11px] text-slate-300 flex-1">Průměr výseče (mm)</span>
              <div class="w-28">
                <NumberInput bind:value={cornerDiam} step={0.1} min={0} max={10}/>
              </div>
            </div>
          </section>

          <!-- Odsazení -->
          <section>
            <div class="text-[9px] font-bold text-slate-400 uppercase tracking-widest border-b border-slate-800 pb-1 mb-2">
              Odsazení od skel
            </div>
            <div class="flex flex-col gap-1.5">
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Nahoře (mm)</span>
                <div class="w-28"><NumberInput bind:value={offsetTop}    step={0.5} min={0} max={30}/></div>
              </div>
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Dole (mm)</span>
                <div class="w-28"><NumberInput bind:value={offsetBottom} step={0.5} min={0} max={30}/></div>
              </div>
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Vlevo (mm)</span>
                <div class="w-28"><NumberInput bind:value={offsetLeft}   step={0.5} min={0} max={30}/></div>
              </div>
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Vpravo (mm)</span>
                <div class="w-28"><NumberInput bind:value={offsetRight}  step={0.5} min={0} max={30}/></div>
              </div>
            </div>
          </section>

          <!-- 3D tisk -->
          <section>
            <div class="text-[9px] font-bold text-slate-400 uppercase tracking-widest border-b border-slate-800 pb-1 mb-2">
              3D tisk
            </div>
            <div class="flex items-center justify-between gap-2">
              <span class="text-[11px] text-slate-300 flex-1">Tloušťka (mm)</span>
              <div class="w-28">
                <NumberInput bind:value={bracketThick} step={0.5} min={0.5} max={30}/>
              </div>
            </div>
            <p class="text-[10px] text-slate-600 mt-1.5">STL export bude přidán v další verzi.</p>
          </section>

          <!-- Legenda -->
          <section class="mt-auto">
            <div class="text-[9px] font-bold text-slate-400 uppercase tracking-widest border-b border-slate-800 pb-1 mb-2">
              Legenda
            </div>
            <div class="flex flex-col gap-1">
              <div class="flex items-center gap-2">
                <div class="w-3 h-3 rounded-sm bg-[#3b82f6] shrink-0"></div>
                <span class="text-[10px] text-slate-400">Pevný materiál (tisk)</span>
              </div>
              <div class="flex items-center gap-2">
                <div class="w-3 h-3 rounded-sm bg-[#1e40af] shrink-0"></div>
                <span class="text-[10px] text-slate-400">Pružinová zóna</span>
              </div>
              <div class="flex items-center gap-2">
                <div class="w-3 h-3 rounded-sm bg-[#0b0f19] border border-slate-700 shrink-0"></div>
                <span class="text-[10px] text-slate-400">Díra (nevytisknuto)</span>
              </div>
            </div>
          </section>

        </div>
      </div>
    </div>

    <!-- Patička -->
    <div class="shrink-0 flex items-center justify-between px-5 py-3 border-t border-slate-800 bg-slate-900/30">
      <div class="text-[10px] text-slate-500">
        Celkový rozměr: <span class="text-slate-300 font-mono">{bW.toFixed(1)} × {bH.toFixed(1)} mm</span>
      </div>
      <div class="flex items-center gap-2">
        <button
          on:click={handleExportSVG}
          class="flex items-center gap-1.5 bg-labaccent hover:bg-blue-600 text-white text-xs font-medium px-4 py-1.5 rounded-md transition-colors"
        >
          <Download class="w-3.5 h-3.5" />
          Export SVG
        </button>
        <button
          on:click={close}
          class="bg-slate-800 hover:bg-slate-700 border border-slate-700 text-slate-200 text-xs px-4 py-1.5 rounded-md transition-colors"
        >
          Zavřít
        </button>
      </div>
    </div>
  </div>
</div>
{/if}
