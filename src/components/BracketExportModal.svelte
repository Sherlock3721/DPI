<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { X, Download } from "lucide-svelte";
  import NumberInput from "./NumberInput.svelte";
  import CustomSelect from "./CustomSelect.svelte";
  import { settingsStore } from "../stores/settingsStore";
  import { save } from "@tauri-apps/plugin-dialog";
  import { writeTextFile, writeFile } from "@tauri-apps/plugin-fs";
  import { wasm_bracket_geometry, wasm_bracket_svg, wasm_bracket_stl } from "../lib/dpiWasm";
  import type { LayoutPosition } from "../lib/tauri";

  export let isOpen = false;
  const dispatch = createEventDispatcher();
  function close() { dispatch("close"); }

  // === PARAMETRY ===
  let leftBorderW   = 18.0;  // tloušťka levé pevné stěny (jen u prvního sloupce sestavy)
  let bottomBorderH = 24.0;  // tloušťka spodní pevné stěny (jen u posledního řádku sestavy)
  let extendWalls   = false; // rozšířit pevné stěny o extendAmount doleva a dolů
  let extendAmount  = 2.0;   // velikost rozšíření stěn (mm)
  let fixedThickX   = 5.0;   // tloušťka pravého ramene pevného L (směrem doleva)
  let fixedThickY   = 5.0;   // tloušťka horního ramene pevného L (směrem dolů)
  let flexThick     = 2.0;   // tloušťka ramen flexibilního L
  let flexGap       = 1.0;   // mezera od levé a spodní stěny skla
  let springCountX  = 2;     // počet pružin v ose X (horní zóna)
  let springCountY  = 1;     // počet pružin v ose Y (pravá zóna)
  let springWidth   = 10.0;  // šířka/výška jednoho pružinového prvku (mm)
  let springBends   = 6;     // počet ohybů jedné pružiny
  let springGapMod  = 0.2;   // modifikátor mezery řezu (mm) – kompenzace tolerancí tiskárny
  let cornerR       = 1.5;   // poloměr rohových výsečí (mm)
  let magnetSize    = 3.0;   // průměr (kružnice) / strana (čtverec) díry pro magnet (mm)
  let magnetShape: "circle" | "square" = "circle"; // tvar díry pro magnet

  // === 3D TISK (STL) ===
  // Tloušťka držáku (výška v ose Z) — nikdy nesmí přesáhnout tloušťku skla
  // (jinak by deska vyčnívala nad/pod sklo a bránila jeho usazení), proto je
  // vždy ořízlá na glassThickness — viz reaktivní clamp níže.
  let bracketThickness = 1.0;
  // Dodatečná výška ROZŠÍŘENÉ ČÁSTI pevných stěn (viz extendWalls/extendAmount)
  // nad rámec bracketThickness — tato část tak vyčnívá nad desku a tvoří
  // "zarážku" držící sklo na místě. Má smysl jen když je rozšíření aktivní.
  let wallExtraHeight  = 5.0;

  // === SUBSTRÁTY ===
  $: substrateOptions = Object.entries($settingsStore.sklo_dims || {}).map(([name, dims]: [string, any]) => ({
    value: name,
    label: `${name}  (${Number(dims[0]).toFixed(1)} × ${Number(dims[1]).toFixed(1)} mm)`,
    w: Number(dims[0]),
    h: Number(dims[1]),
    thickness: Number(dims[2]),
  }));
  let selectedSubstrateId: string = "";
  $: if (!selectedSubstrateId && substrateOptions.length > 0) selectedSubstrateId = substrateOptions[0].value;
  $: selectedSubstrate = substrateOptions.find(s => s.value === selectedSubstrateId) ?? null;
  $: glassW = selectedSubstrate?.w ?? 25.0;
  $: glassH = selectedSubstrate?.h ?? 75.0;
  $: glassThickness = selectedSubstrate?.thickness ?? 1.0;

  // Tloušťka držáku nesmí nikdy přesáhnout tloušťku skla — jinak by deska
  // vyčnívala nad jeho povrch a bránila jeho usazení do sestavy.
  $: bracketThickness = Math.min(Math.max(0.2, bracketThickness), glassThickness);

  // === MULTIPLIKACE ===
  // Skutečné rozložení kopií na podložce počítá Rust (compute_bracket_geometry,
  // viz dpi-core/src/bracket.rs::grid_layout_positions). Max. kapacita mřížky
  // se počítá zde přímo ze vstupů (bedConfig + rozměry skla) — NESMÍ záviset
  // na `geometry`, protože by to vytvořilo reaktivní cyklus:
  //   bracketParams → geometry → maxMultiply → multiplyCount → bracketParams.
  // Algoritmus zrcadlí `grid_max_capacity` v bracket.rs (závisí jen na glass_w/h,
  // spacing a bed — nikoli na multiply_count).
  let multiplyCount = 1;
  let maxMultiply   = 1;
  $: spacing = $settingsStore.multi_spacing || 5.0;
  $: bedConfig = {
    min_x:    $settingsStore.bed_min_x ?? 0.0,
    max_x:    $settingsStore.bed_max_x || 250.0,
    max_y:    $settingsStore.bed_max_y || 250.0,
    offset_x: $settingsStore.start_offset_x || 18.0,
    offset_y: $settingsStore.start_offset_y || 18.0,
  };
  $: {
    const colTop     = bedConfig.offset_y;
    const rowsPerCol = Math.max(0, Math.floor((bedConfig.max_y - colTop + spacing) / (glassH + spacing)));
    let currX = bedConfig.min_x + bedConfig.offset_x;
    let total = 0;
    while (currX + glassW <= bedConfig.max_x) { total += rowsPerCol; currX += glassW + spacing; }
    maxMultiply = Math.max(1, total);
  }
  $: multiplyCount = Math.min(Math.max(1, multiplyCount), maxMultiply);

  // === GEOMETRIE (jediný zdroj pravdy — Rust, viz dpi-core/src/bracket.rs) ===
  // Veškerá geometrie sestavy (cesty, obdélníky, středy děr, layout kopií…) se
  // počítá v Rustu jedním voláním — náhled, SVG export i rasterizace pro STL
  // export tak vždy vychází ze stejných dat. Frontend pouze vykresluje/ukládá.
  interface BracketPoint { x: number; y: number; }
  interface BracketRect  { x: number; y: number; w: number; h: number; }
  interface SpringGeom extends BracketRect { path: string; }
  interface CopyOffset   { tx: number; ty: number; }
  interface BracketGeometry {
    b_w: number; b_h: number; hole_x: number; hole_y: number; hole_x2: number; hole_y2: number;
    flex_l_path: string; fixed_l_path: string; corner_square_size: number;
    magnet_center: BracketPoint; effective_magnet_size: number;
    x_springs: SpringGeom[]; y_springs: SpringGeom[];
    multiply_positions: LayoutPosition[]; copy_offsets: CopyOffset[];
    corner_hole_centers: BracketPoint[];
    left_wall_rect: BracketRect; bottom_wall_rect: BracketRect; wall_extend: number;
    wall_magnet_centers: BracketPoint[];
    assembly_min_x: number; assembly_min_y: number; assembly_max_x: number; assembly_max_y: number;
    assembly_w: number; assembly_h: number;
    /** Skutečný max. počet kopií na podložku — vypočítán v Rustu. */
    max_multiply: number;
  }

  $: bracketParams = {
    glass_w: glassW, glass_h: glassH, glass_label: selectedSubstrate?.label ?? "—",
    left_border_w: leftBorderW, bottom_border_h: bottomBorderH,
    extend_walls: extendWalls, extend_amount: extendAmount,
    fixed_thick_x: fixedThickX, fixed_thick_y: fixedThickY,
    flex_thick: flexThick, flex_gap: flexGap,
    spring_count_x: springCountX, spring_count_y: springCountY,
    spring_width: springWidth, spring_bends: springBends, spring_gap_mod: springGapMod,
    corner_r: cornerR, magnet_size: magnetSize, magnet_shape: magnetShape,
    multiply_count: multiplyCount, spacing,
    bed: bedConfig,
  };

  let geometry: BracketGeometry | null = null;
  let geometryError: string | null = null;

  // Výpočet geometrie proběhne synchronně — WASM je rychlý (<5 ms) a geometrie
  // musí být vždy aktuální: slouží jako validační základ pro max_multiply/clampy
  // i pro export. Debounce slouží jen pro vykreslení SVG náhledu (viz renderedPreview).
  $: {
    try {
      geometry      = JSON.parse(wasm_bracket_geometry(JSON.stringify(bracketParams))) as BracketGeometry;
      geometryError = null;
    } catch (e) {
      geometry      = null;
      geometryError = String(e);
    }
  }

  // === EXPORT SVG ===
  async function handleExportSVG() {
    let content: string;
    try {
      content = wasm_bracket_svg(JSON.stringify(bracketParams));
    } catch (e) { alert(`Chyba při generování SVG: ${e}`); return; }
    try {
      const filePath = await save({ filters: [{ name: "SVG soubor", extensions: ["svg"] }], defaultPath: "drzak.svg" });
      if (filePath) await writeTextFile(filePath, content);
    } catch (e) { alert(`Chyba při exportu SVG: ${e}`); }
  }

  // === EXPORT STL (3D model) ===
  // STL se generuje jako 2.5D vytlačení (extruze) 2D průřezu — stejná geometrie
  // jako u SVG náhledu/exportu, jen "naskládaná" do výšky. Protože průřez
  // obsahuje díry a oblé řezy (pružiny, magnety, výseče), vyrastruje se nejprve
  // do bitmapy (plno/díra) pomocí canvasu a Path2D (jediná část pipeline, která
  // vyžaduje DOM/Canvas — cesty samotné dodává Rust). Vše ostatní — greedy
  // meshing, vytlačení do kvádrů a binární STL encoding — proběhne v Rustu
  // (build_bracket_stl, viz dpi-core/src/bracket.rs).

  // Vykreslí 2D průřez sestavy do canvasu — bíle plné plochy, černě díry.
  // Pořadí vrstev je přesně shodné s SVG náhledem — cesty a obdélníky jsou
  // předpočítané v `g`, žádný další geometrický výpočet zde neprobíhá.
  function drawCrossSectionForSTL(ctx: CanvasRenderingContext2D, g: BracketGeometry) {
    function drawMagnet(cx: number, cy: number) {
      if (g.effective_magnet_size <= 0) return;
      const s = g.effective_magnet_size;
      if (magnetShape === "square") {
        ctx.fillRect(cx - s / 2, cy - s / 2, s, s);
      } else {
        ctx.beginPath();
        ctx.arc(cx, cy, s / 2, 0, Math.PI * 2);
        ctx.fill();
      }
    }

    ctx.fillStyle = "black";
    ctx.fillRect(g.assembly_min_x, g.assembly_min_y, g.assembly_w, g.assembly_h);

    ctx.fillStyle = "white";
    ctx.fillRect(g.left_wall_rect.x,   g.left_wall_rect.y,   g.left_wall_rect.w,   g.left_wall_rect.h);
    ctx.fillRect(g.bottom_wall_rect.x, g.bottom_wall_rect.y, g.bottom_wall_rect.w, g.bottom_wall_rect.h);

    ctx.fillStyle = "black";
    for (const c of g.wall_magnet_centers) drawMagnet(c.x, c.y);

    for (const o of g.copy_offsets) {
      ctx.save();
      ctx.translate(o.tx, o.ty);

      ctx.fillStyle = "white";
      ctx.fill(new Path2D(g.fixed_l_path));

      ctx.fillStyle = "black";
      drawMagnet(g.magnet_center.x, g.magnet_center.y);

      ctx.fillStyle = "white";
      for (const s of g.x_springs) ctx.fillRect(s.x, s.y, s.w, s.h);
      for (const s of g.y_springs) ctx.fillRect(s.x, s.y, s.w, s.h);
      ctx.fillStyle = "black";
      for (const s of g.x_springs) ctx.fill(new Path2D(s.path));
      for (const s of g.y_springs) ctx.fill(new Path2D(s.path));

      ctx.fillStyle = "white";
      ctx.fill(new Path2D(g.flex_l_path));

      ctx.restore();
    }

    ctx.fillStyle = "black";
    for (const c of g.corner_hole_centers) {
      ctx.beginPath();
      ctx.arc(c.x, c.y, cornerR, 0, Math.PI * 2);
      ctx.fill();
    }
  }

  // Vyrastruje průřez do bitmapy plno/díra. Velikost buňky se volí jemně
  // (rozliší i nejtenčí řezy pružin), ale s horní mezí na celkový počet buněk,
  // aby u velkých sestav (více kopií) negenerovala obrovské množství trojúhelníků.
  function rasterizeCrossSection(g: BracketGeometry): { mask: Uint8Array; cols: number; rows: number; cellSize: number } {
    let cellSize = 0.2;
    let cols = Math.max(1, Math.ceil(g.assembly_w / cellSize));
    let rows = Math.max(1, Math.ceil(g.assembly_h / cellSize));
    const MAX_CELLS = 4_000_000;
    if (cols * rows > MAX_CELLS) {
      cellSize *= Math.sqrt((cols * rows) / MAX_CELLS);
      cols = Math.max(1, Math.ceil(g.assembly_w / cellSize));
      rows = Math.max(1, Math.ceil(g.assembly_h / cellSize));
    }

    const canvas = document.createElement("canvas");
    canvas.width  = cols;
    canvas.height = rows;
    const ctx = canvas.getContext("2d", { willReadFrequently: true })!;
    ctx.scale(1 / cellSize, 1 / cellSize);
    ctx.translate(-g.assembly_min_x, -g.assembly_min_y);
    drawCrossSectionForSTL(ctx, g);

    // Čteme jen R kanál (index 0) z RGBA — ostatní tři kanály ignorujeme.
    const { data } = ctx.getImageData(0, 0, cols, rows);
    const mask = new Uint8Array(cols * rows);
    for (let i = 0; i < cols * rows; i++) mask[i] = data[i * 4] > 128 ? 1 : 0;
    return { mask, cols, rows, cellSize };
  }

  function generateExportSTL(): Uint8Array {
    if (!geometry) throw new Error("Geometrie držáku není k dispozici");
    const g = geometry;
    const { mask, cols, rows, cellSize } = rasterizeCrossSection(g);
    return wasm_bracket_stl(
      mask, cols, rows, cellSize, g.assembly_min_x, g.assembly_min_y,
      g.left_wall_rect.x, g.left_wall_rect.y, g.left_wall_rect.w, g.left_wall_rect.h,
      g.bottom_wall_rect.x, g.bottom_wall_rect.y, g.bottom_wall_rect.w, g.bottom_wall_rect.h,
      g.wall_extend, bracketThickness, wallExtraHeight,
    );
  }

  async function handleExportSTL() {
    let content: Uint8Array;
    try {
      content = generateExportSTL();
    } catch (e) { alert(`Chyba při generování STL: ${e}`); return; }
    try {
      const filePath = await save({ filters: [{ name: "STL soubor", extensions: ["stl"] }], defaultPath: "drzak.stl" });
      if (filePath) await writeFile(filePath, content);
    } catch (e) { alert(`Chyba při exportu STL: ${e}`); }
  }

  const PAD = 8;

  // === Debounce živého náhledu ===
  // SVG náhled obsahuje desítky <path> s clip-pathy (pružiny, magnety, kóty)
  // a jeho překreslení je nákladné (~150–300 ms na cyklus layout/paint/composite).
  // Při každém kliknutí na parametr se tak aplikace na chvíli „zasekla". Šablona
  // proto čerpá geometrii ze snímku `rp`, který se aktualizuje až s krátkým
  // zpožděním po poslední změně — vstupní pole i export zůstávají navázané
  // na živé (nedebouncované) hodnoty.
  type PreviewModel = BracketGeometry & { magnetShape: "circle" | "square"; cornerR: number; glassW: number; glassH: number };
  $: previewModel = geometry ? { ...geometry, magnetShape, cornerR, glassW, glassH } as PreviewModel : null;
  let renderedPreview: PreviewModel | undefined;
  let previewDebounce: ReturnType<typeof setTimeout> | undefined;

  // Reset stale preview při zavření/znovuotevření modalu — bez toho by se
  // zobrazila stará geometrie, dokud nepřijde nový model.
  $: if (!isOpen) {
    renderedPreview = undefined;
    clearTimeout(previewDebounce);
  }

  // Pozn.: záměrně obyčejná funkce volaná z reaktivního výrazu (ne `$: { ... }` blok) —
  // ten by četl i zapisoval `renderedPreview`, čímž by se sám stal na sobě závislým
  // a vyvolal nekonečný cyklus přehodnocování (= zamrznutí).
  function schedulePreviewUpdate(model: PreviewModel | null) {
    if (!model) return;
    if (!renderedPreview) {
      renderedPreview = model;
      return;
    }
    clearTimeout(previewDebounce);
    previewDebounce = setTimeout(() => { renderedPreview = model; }, 120);
  }
  $: schedulePreviewUpdate(previewModel);
  $: rp = renderedPreview ?? previewModel;
</script>

{#if isOpen}
<div class="fixed inset-0 bg-black/75 flex items-center justify-center z-50 p-4">
  <div class="glass-panel w-[94vw] h-[90vh] rounded-xl flex flex-col border border-slate-800 shadow-2xl overflow-hidden">

    <div class="flex items-center justify-between px-5 py-3 border-b border-slate-800 shrink-0">
      <div>
        <h3 class="text-sm font-bold text-slate-100 uppercase tracking-wider">Export držáku</h3>
        <p class="text-[10px] text-slate-500 mt-0.5">
          {#if geometry}
            Deska: {geometry.assembly_w.toFixed(1)} × {geometry.assembly_h.toFixed(1)} mm · Sklo: {glassW.toFixed(1)} × {glassH.toFixed(1)} mm
          {/if}
        </p>
      </div>
      <button on:click={close} class="p-1.5 hover:bg-slate-800 rounded-md transition-colors text-slate-400 hover:text-slate-100">
        <X class="w-4 h-4" />
      </button>
    </div>

    <div class="flex-1 flex overflow-hidden min-h-0">

      <!-- SVG náhled -->
      <div class="flex-1 overflow-hidden bg-slate-950 flex items-center justify-center p-8 min-w-0">
        {#if rp}
        <svg
          viewBox="{rp.assembly_min_x - PAD} {rp.assembly_min_y - PAD} {rp.assembly_w + PAD * 2} {rp.assembly_h + PAD * 2}"
          preserveAspectRatio="xMidYMid meet"
          style="max-width: 100%; max-height: 100%; display: block;"
        >
          <defs>
            <pattern id="bgrid" width="5" height="5" patternUnits="userSpaceOnUse">
              <path d="M 5 0 L 0 0 0 5" fill="none" stroke="#1e293b" stroke-width="0.2"/>
            </pattern>
          </defs>
          <!-- Pozadí = vše je díra -->
          <rect x={rp.assembly_min_x - PAD} y={rp.assembly_min_y - PAD} width={rp.assembly_w + PAD * 2} height={rp.assembly_h + PAD * 2} fill="#0b0f19"/>
          <rect x={rp.assembly_min_x - PAD} y={rp.assembly_min_y - PAD} width={rp.assembly_w + PAD * 2} height={rp.assembly_h + PAD * 2} fill="url(#bgrid)"/>

          <!-- Levá stěna sestavy (jen první sloupec) -->
          <rect x={rp.left_wall_rect.x} y={rp.left_wall_rect.y} width={rp.left_wall_rect.w} height={rp.left_wall_rect.h} fill="#3b82f6"/>
          <!-- Spodní stěna sestavy (jen poslední řádek) -->
          <rect x={rp.bottom_wall_rect.x} y={rp.bottom_wall_rect.y} width={rp.bottom_wall_rect.w} height={rp.bottom_wall_rect.h} fill="#3b82f6"/>

          <!-- Díry pro magnet v pevných stěnách sestavy -->
          {#each rp.wall_magnet_centers as c}
            {#if rp.magnetShape === "square"}
              <rect x={c.x - rp.effective_magnet_size / 2} y={c.y - rp.effective_magnet_size / 2}
                width={rp.effective_magnet_size} height={rp.effective_magnet_size} fill="#0b0f19"/>
            {:else}
              <circle cx={c.x} cy={c.y} r={rp.effective_magnet_size / 2} fill="#0b0f19"/>
            {/if}
          {/each}

          {#each rp.copy_offsets as o, idx}
            <g transform="translate({o.tx},{o.ty})">
              <!-- Pevný L roh -->
              <path d={rp.fixed_l_path} fill="#f59e0b"/>

              <!-- Díra pro magnet (uprostřed výztužného čtverce v rohu pevného L) -->
              {#if rp.effective_magnet_size > 0}
                {#if rp.magnetShape === "square"}
                  <rect x={rp.magnet_center.x - rp.effective_magnet_size / 2} y={rp.magnet_center.y - rp.effective_magnet_size / 2}
                    width={rp.effective_magnet_size} height={rp.effective_magnet_size} fill="#0b0f19"/>
                {:else}
                  <circle cx={rp.magnet_center.x} cy={rp.magnet_center.y} r={rp.effective_magnet_size / 2} fill="#0b0f19"/>
                {/if}
              {/if}

              <!-- Pružiny X (horní zóna, jen v mezeře) -->
              {#each rp.x_springs as s, i}
                <clipPath id="prev{idx}-xspring-clip-{i}"><rect x={s.x} y={s.y} width={s.w} height={s.h}/></clipPath>
                <rect x={s.x} y={s.y} width={s.w} height={s.h} fill="#a855f7"/>
                <path d={s.path} fill="#0b0f19" clip-path="url(#prev{idx}-xspring-clip-{i})"/>
              {/each}
              <!-- Pružiny Y (pravá zóna, jen v mezeře) -->
              {#each rp.y_springs as s, i}
                <clipPath id="prev{idx}-yspring-clip-{i}"><rect x={s.x} y={s.y} width={s.w} height={s.h}/></clipPath>
                <rect x={s.x} y={s.y} width={s.w} height={s.h} fill="#a855f7"/>
                <path d={s.path} fill="#0b0f19" clip-path="url(#prev{idx}-yspring-clip-{i})"/>
              {/each}

              <!-- Flex L (zelený, uvnitř díry skla) -->
              <path d={rp.flex_l_path} fill="#22c55e" opacity="0.9"/>

              <!-- Obrys desky -->
              <rect x="0" y="0" width={rp.b_w} height={rp.b_h} fill="none" stroke="#334155" stroke-width="0.3"/>

              <!-- Popisek skla -->
              {#if rp.glassW > 10 && rp.glassH > 8}
                <text x={rp.hole_x + rp.glassW / 2} y={rp.hole_y + rp.glassH / 2}
                  text-anchor="middle" font-size="3" fill="rgba(255,255,255,0.15)"
                >{rp.glassW.toFixed(1)} × {rp.glassH.toFixed(1)}</text>
              {/if}

            </g>
          {/each}
          <!-- Rohové výseče (díry) — poslední vrstva NAD vším, aby výseč udělala
               díru i tam, kudy prochází přes okraj/zónu sousední kopie či stěny -->
          {#each rp.corner_hole_centers as c}
            <circle cx={c.x} cy={c.y} r={rp.cornerR} fill="#0b0f19"/>
          {/each}

          <!-- Kóty celé sestavy -->
          <line x1={rp.assembly_min_x} y1={rp.assembly_min_y - 3} x2={rp.assembly_max_x} y2={rp.assembly_min_y - 3} stroke="#64748b" stroke-width="0.2"/>
          <line x1={rp.assembly_min_x} y1={rp.assembly_min_y - 4} x2={rp.assembly_min_x} y2={rp.assembly_min_y - 2} stroke="#64748b" stroke-width="0.2"/>
          <line x1={rp.assembly_max_x} y1={rp.assembly_min_y - 4} x2={rp.assembly_max_x} y2={rp.assembly_min_y - 2} stroke="#64748b" stroke-width="0.2"/>
          <text x={(rp.assembly_min_x + rp.assembly_max_x) / 2} y={rp.assembly_min_y - 4.5} text-anchor="middle" font-size="2.5" fill="#94a3b8">{rp.assembly_w.toFixed(1)} mm</text>
          <line x1={rp.assembly_max_x + 3} y1={rp.assembly_min_y} x2={rp.assembly_max_x + 3} y2={rp.assembly_max_y} stroke="#64748b" stroke-width="0.2"/>
          <line x1={rp.assembly_max_x + 2} y1={rp.assembly_min_y} x2={rp.assembly_max_x + 4} y2={rp.assembly_min_y} stroke="#64748b" stroke-width="0.2"/>
          <line x1={rp.assembly_max_x + 2} y1={rp.assembly_max_y} x2={rp.assembly_max_x + 4} y2={rp.assembly_max_y} stroke="#64748b" stroke-width="0.2"/>
          <text x={rp.assembly_max_x + 5} y={(rp.assembly_min_y + rp.assembly_max_y) / 2} text-anchor="middle" font-size="2.5" fill="#94a3b8"
            transform="rotate(90,{rp.assembly_max_x + 5},{(rp.assembly_min_y + rp.assembly_max_y) / 2})">{rp.assembly_h.toFixed(1)} mm</text>
        </svg>
        {:else if geometryError}
          <p class="text-[11px] text-red-400/80 px-6 text-center">Chyba výpočtu geometrie držáku: {geometryError}</p>
        {/if}
      </div>

      <!-- Panel -->
      <div class="w-[272px] shrink-0 flex flex-col border-l border-slate-800 overflow-y-auto">
        <div class="flex flex-col gap-4 p-4">

          <section class="rounded-lg p-2.5 border-l-2 border-slate-600 bg-[#0b0f19]/60">
            <div class="text-[9px] font-bold text-slate-400 uppercase tracking-widest border-b border-slate-800 pb-1 mb-2">Substrát</div>
            {#if substrateOptions.length > 0}
              <CustomSelect bind:value={selectedSubstrateId} options={substrateOptions} placeholder="Vyberte substrát..."/>
              {#if selectedSubstrate}
                <p class="text-[10px] text-slate-500 mt-1.5">{glassW.toFixed(1)} × {glassH.toFixed(1)} mm</p>
              {/if}
            {:else}
              <p class="text-[11px] text-yellow-500/80">Žádné substráty — přidejte v Nastavení → Podložky.</p>
            {/if}
          </section>

          <section>
            <div class="text-[9px] font-bold text-slate-400 uppercase tracking-widest border-b border-slate-800 pb-1 mb-2">Multiplikace</div>
            <div class="flex flex-col gap-1.5">
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Počet vzorků</span>
                <div class="w-20"><NumberInput bind:value={multiplyCount} step={1} min={1} max={maxMultiply}/></div>
              </div>
            </div>
          </section>

          <section class="rounded-lg p-2.5 border-l-2 border-[#3b82f6] bg-[#3b82f6]/20">
            <div class="text-[9px] font-bold text-slate-400 uppercase tracking-widest border-b border-slate-800 pb-1 mb-2">Pevné stěny</div>
            <div class="flex flex-col gap-1.5">
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Levá stěna (mm)</span>
                <div class="w-20"><NumberInput bind:value={leftBorderW} step={0.5} min={1} max={50}/></div>
              </div>
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Spodní stěna (mm)</span>
                <div class="w-20"><NumberInput bind:value={bottomBorderH} step={0.5} min={1} max={50}/></div>
              </div>
              <div class="flex items-center justify-between gap-2 pt-1 border-t border-slate-800/60">
                <label class="flex items-center gap-1.5 text-[11px] text-slate-300 flex-1 cursor-pointer">
                  <input type="checkbox" bind:checked={extendWalls}
                    class="w-3.5 h-3.5 rounded accent-[#3b82f6] cursor-pointer"/>
                  Rozšířit doleva a dolů (mm)
                </label>
                <div class="w-20 transition-opacity {extendWalls ? '' : 'opacity-40'}">
                  <NumberInput bind:value={extendAmount} step={0.5} min={0} max={50}/>
                </div>
              </div>
              {#if extendWalls}
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1 pl-5">Zvýšení rozšířené části (mm)</span>
                <div class="w-20"><NumberInput bind:value={wallExtraHeight} step={0.5} min={0} max={50}/></div>
              </div>
              <p class="text-[9px] leading-snug text-slate-500">
                Pouze rozšířená část stěn (přesah {extendAmount.toFixed(1)} mm doleva a dolů) je navíc o {wallExtraHeight.toFixed(1)} mm vyšší — tvoří zarážku, která drží sklo na místě.
              </p>
              {/if}
              <p class="text-[9px] leading-snug text-slate-500 pt-1 border-t border-slate-800/60">
                Díry pro magnet se do pevných stěn doplní automaticky v gridu navazujícím na díry v rohových čtvercích pevného L — přímo pod nimi (spodní stěna) a přímo vedle nich (levá stěna). Sdílejí tvar a velikost s rohovým magnetem ({magnetShape === "square" ? "strana" : "průměr"} {(geometry?.effective_magnet_size ?? 0).toFixed(1)} mm).
              </p>
            </div>
          </section>

          <section class="rounded-lg p-2.5 border-l-2 border-[#f59e0b] bg-[#f59e0b]/25">
            <div class="text-[9px] font-bold text-slate-400 uppercase tracking-widest border-b border-slate-800 pb-1 mb-2">Pevný L roh</div>
            <div class="flex flex-col gap-1.5">
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Tloušťka ramene vpravo (mm)</span>
                <div class="w-20"><NumberInput bind:value={fixedThickX} step={0.5} min={0.5} max={20}/></div>
              </div>
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Tloušťka ramene nahoře (mm)</span>
                <div class="w-20"><NumberInput bind:value={fixedThickY} step={0.5} min={0.5} max={20}/></div>
              </div>
              <div class="pt-1.5 mt-0.5 border-t border-slate-800/60 flex flex-col gap-1.5">
                <div class="flex items-center justify-between gap-2">
                  <span class="text-[11px] text-slate-300 flex-1">Tvar díry pro magnet</span>
                  <div class="flex rounded overflow-hidden border border-slate-700/50 shrink-0">
                    <button type="button" on:click={() => magnetShape = "circle"}
                      class="px-2 py-1 text-[10px] transition-colors {magnetShape === 'circle' ? 'bg-[#f59e0b] text-slate-900 font-medium' : 'bg-slate-800 text-slate-400 hover:bg-slate-700'}">Kruh</button>
                    <button type="button" on:click={() => magnetShape = "square"}
                      class="px-2 py-1 text-[10px] transition-colors border-l border-slate-700/50 {magnetShape === 'square' ? 'bg-[#f59e0b] text-slate-900 font-medium' : 'bg-slate-800 text-slate-400 hover:bg-slate-700'}">Čtverec</button>
                  </div>
                </div>
                <div class="flex items-center justify-between gap-2">
                  <span class="text-[11px] text-slate-300 flex-1">{magnetShape === "square" ? "Strana" : "Průměr"} magnetu (mm)</span>
                  <div class="w-20"><NumberInput bind:value={magnetSize} step={0.5} min={0} max={20}/></div>
                </div>
                {#if geometry}
                <p class="text-[9px] leading-snug {magnetSize > geometry.corner_square_size ? 'text-amber-400/80' : 'text-slate-500'}">
                  Díra se umístí do středu výztužného čtverce v rohu — automaticky omezena na max. {geometry.corner_square_size.toFixed(1)} mm,
                  aby se nikdy nedotkla flexibilního L rohu{magnetSize > geometry.corner_square_size ? ` (aktuálně oříznuto z ${magnetSize.toFixed(1)} mm)` : ''}.
                </p>
                {/if}
              </div>
            </div>
          </section>

          <section class="rounded-lg p-2.5 border-l-2 border-[#22c55e] bg-[#22c55e]/20">
            <div class="text-[9px] font-bold text-slate-400 uppercase tracking-widest border-b border-slate-800 pb-1 mb-2">Flexibilní L roh</div>
            <div class="flex flex-col gap-1.5">
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Tloušťka ramen (mm)</span>
                <div class="w-20"><NumberInput bind:value={flexThick} step={0.5} min={0.5} max={20}/></div>
              </div>
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Mezera od stěn (mm)</span>
                <div class="w-20"><NumberInput bind:value={flexGap} step={0.5} min={0.1} max={10}/></div>
              </div>
            </div>
          </section>

          <section class="rounded-lg p-2.5 border-l-2 border-slate-500 bg-[#0b0f19]/60">
            <div class="text-[9px] font-bold text-slate-400 uppercase tracking-widest border-b border-slate-800 pb-1 mb-2">Rohové výseče</div>
            <div class="flex flex-col gap-1.5">
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Poloměr výseče (mm)</span>
                <div class="w-20"><NumberInput bind:value={cornerR} step={0.25} min={0} max={10}/></div>
              </div>
            </div>
          </section>

          <section class="rounded-lg p-2.5 border-l-2 border-[#a855f7] bg-[#a855f7]/20">
            <div class="text-[9px] font-bold text-slate-400 uppercase tracking-widest border-b border-slate-800 pb-1 mb-2">Pružiny</div>
            <div class="flex flex-col gap-1.5">
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Počet X (horní)</span>
                <div class="w-20"><NumberInput bind:value={springCountX} step={1} min={0} max={10}/></div>
              </div>
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Počet Y (pravý)</span>
                <div class="w-20"><NumberInput bind:value={springCountY} step={1} min={0} max={10}/></div>
              </div>
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Šířka pružiny (mm)</span>
                <div class="w-20"><NumberInput bind:value={springWidth} step={0.5} min={1} max={60}/></div>
              </div>
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Počet ohybů</span>
                <div class="w-20"><NumberInput bind:value={springBends} step={1} min={1} max={20}/></div>
              </div>
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Modifikátor mezery (mm)</span>
                <div class="w-20"><NumberInput bind:value={springGapMod} step={0.05} min={-0.3} max={1.5}/></div>
              </div>
            </div>
          </section>

          <section class="rounded-lg p-2.5 border-l-2 border-slate-400 bg-[#0b0f19]/60">
            <div class="text-[9px] font-bold text-slate-400 uppercase tracking-widest border-b border-slate-800 pb-1 mb-2">3D tisk (STL)</div>
            <div class="flex flex-col gap-1.5">
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] text-slate-300 flex-1">Tloušťka držáku (mm)</span>
                <div class="w-20"><NumberInput bind:value={bracketThickness} step={0.1} min={0.2} max={glassThickness}/></div>
              </div>
              <p class="text-[9px] leading-snug text-slate-500 pt-1 border-t border-slate-800/60">
                Tloušťka držáku nikdy nepřesáhne tloušťku skla ({glassThickness.toFixed(1)} mm), aby deska nevyčnívala nad jeho povrch.
              </p>
            </div>
          </section>

          <section class="mt-auto">
            <div class="text-[9px] font-bold text-slate-400 uppercase tracking-widest border-b border-slate-800 pb-1 mb-2">Legenda</div>
            <div class="flex flex-col gap-1.5">
              <div class="flex items-center gap-2">
                <div class="w-3 h-3 rounded-sm bg-[#3b82f6] shrink-0"></div>
                <span class="text-[10px] text-slate-400">Pevné stěny</span>
              </div>
              <div class="flex items-center gap-2">
                <div class="w-3 h-3 rounded-sm bg-[#f59e0b] shrink-0"></div>
                <span class="text-[10px] text-slate-400">Pevný L roh</span>
              </div>
              <div class="flex items-center gap-2">
                <div class="w-3 h-3 rounded-sm bg-[#a855f7] shrink-0"></div>
                <span class="text-[10px] text-slate-400">Pružiny</span>
              </div>
              <div class="flex items-center gap-2">
                <div class="w-3 h-3 rounded-sm bg-[#22c55e] shrink-0"></div>
                <span class="text-[10px] text-slate-400">Flexibilní L roh</span>
              </div>
              <div class="flex items-center gap-2">
                <div class="w-3 h-3 rounded-sm bg-[#0b0f19] border border-slate-700 shrink-0"></div>
                <span class="text-[10px] text-slate-400">Díra (otevřeno)</span>
              </div>
              <div class="flex items-center gap-2">
                <div class="w-3 h-3 rounded-full bg-[#0b0f19] border border-slate-600 shrink-0"></div>
                <span class="text-[10px] text-slate-400">Rohová výseč (díra)</span>
              </div>
            </div>
          </section>

        </div>
      </div>
    </div>

    <div class="shrink-0 flex items-center justify-between px-5 py-3 border-t border-slate-800 bg-slate-900/30">
      <div class="text-[10px] text-slate-500">
        {#if geometry}
        Deska: <span class="text-slate-300 font-mono">{geometry.b_w.toFixed(1)} × {geometry.b_h.toFixed(1)} mm</span>
        <span class="mx-2 text-slate-700">·</span>
        Sklo: <span class="text-slate-300 font-mono">{glassW.toFixed(1)} × {glassH.toFixed(1)} mm</span>
        {/if}
      </div>
      <div class="flex items-center gap-2">
        <button on:click={handleExportSVG} disabled={!geometry}
          class="flex items-center gap-1.5 bg-labaccent hover:bg-blue-600 text-white text-xs font-medium px-4 py-1.5 rounded-md transition-colors disabled:opacity-40 disabled:cursor-not-allowed">
          <Download class="w-3.5 h-3.5" /> Export SVG
        </button>
        <button on:click={handleExportSTL} disabled={!geometry}
          class="flex items-center gap-1.5 bg-slate-700 hover:bg-slate-600 border border-slate-600 text-white text-xs font-medium px-4 py-1.5 rounded-md transition-colors disabled:opacity-40 disabled:cursor-not-allowed">
          <Download class="w-3.5 h-3.5" /> Export STL
        </button>
        <button on:click={close}
          class="bg-slate-800 hover:bg-slate-700 border border-slate-700 text-slate-200 text-xs px-4 py-1.5 rounded-md transition-colors">
          Zavřít
        </button>
      </div>
    </div>

  </div>
</div>
{/if}
