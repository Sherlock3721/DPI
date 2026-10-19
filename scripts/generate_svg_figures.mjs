import fs from "fs";
import path from "path";

// Helper: base64 encode an image
function getBase64Png(relPath) {
  const abs = path.resolve(relPath);
  if (!fs.existsSync(abs)) {
    console.warn(`File ${abs} not found!`);
    return "";
  }
  const data = fs.readFileSync(abs);
  return `data:image/png;base64,${data.toString("base64")}`;
}

// Helper: generate circular callout badge
function makeBadge(num, cx, cy, radius = 16, color = "#3b82f6") {
  return `
  <g class="callout-badge" transform="translate(${cx}, ${cy})">
    <circle r="${radius + 2}" fill="rgba(0,0,0,0.4)" cx="1" cy="2" />
    <circle r="${radius}" fill="${color}" stroke="#ffffff" stroke-width="2.5" />
    <text x="0" y="${radius * 0.38}" text-anchor="middle" fill="#ffffff" font-family="'DejaVu Sans', 'Arial', sans-serif" font-weight="bold" font-size="${radius * 1.15}px">${num}</text>
  </g>`;
}

// Helper: generate highlight box
function makeBox(x, y, w, h, stroke = "#3b82f6", fill = "rgba(59, 130, 246, 0.12)", strokeWidth = 2.5, radius = 6) {
  return `
  <rect x="${x}" y="${y}" width="${w}" height="${h}" rx="${radius}" fill="${fill}" stroke="${stroke}" stroke-width="${strokeWidth}" stroke-dasharray="6 3" />`;
}

// Helper: generate pointer arrow
function makeArrow(x1, y1, x2, y2, color = "#3b82f6", strokeWidth = 2.5) {
  return `
  <g stroke="${color}" stroke-width="${strokeWidth}" fill="${color}">
    <line x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" stroke-linecap="round" />
    <circle cx="${x2}" cy="${y2}" r="3.5" />
  </g>`;
}

console.log("Generating SVG figures...");

// ----------------------------------------------------------------------------
// 1. FIG 01: UI Main Overview
// ----------------------------------------------------------------------------
{
  const b64 = getBase64Png("docs/assets/raw/01_main_ui.png");
  const svg = `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 1500 950" width="1500" height="950">
  <defs>
    <filter id="shadow" x="-5%" y="-5%" width="110%" height="110%">
      <feDropShadow dx="0" dy="4" stdDeviation="6" flood-color="#000" flood-opacity="0.5"/>
    </filter>
  </defs>

  <!-- Background Image -->
  <image href="${b64}" x="0" y="0" width="1500" height="950" />

  <!-- Overlays -->
  <!-- 1. Header -->
  ${makeBox(10, 8, 1480, 52, "#3b82f6", "rgba(59, 130, 246, 0.15)")}
  ${makeBadge("1", 750, 34, 18, "#3b82f6")}

  <!-- 2. Left Panel: Connection -->
  ${makeBox(12, 68, 360, 160, "#3b82f6", "rgba(59, 130, 246, 0.12)")}
  ${makeBadge("2", 340, 100, 18, "#3b82f6")}

  <!-- 3. Left Panel: Parameters & Slicing -->
  ${makeBox(12, 235, 360, 695, "#f97316", "rgba(249, 115, 22, 0.12)")}
  ${makeBadge("3", 340, 300, 18, "#f97316")}

  <!-- 4. Center Canvas2D -->
  ${makeBox(380, 68, 740, 865, "#10b981", "rgba(16, 185, 129, 0.08)")}
  ${makeBadge("4", 750, 120, 20, "#10b981")}

  <!-- 5. Right Panel: Slide Overrides -->
  ${makeBox(1128, 68, 360, 420, "#8b5cf6", "rgba(139, 92, 246, 0.12)")}
  ${makeBadge("5", 1450, 100, 18, "#8b5cf6")}

  <!-- 6. Right Panel: Camera & Jog & Terminal -->
  ${makeBox(1128, 495, 360, 440, "#06b6d4", "rgba(6, 182, 212, 0.12)")}
  ${makeBadge("6", 1450, 530, 18, "#06b6d4")}
</svg>`;
  fs.writeFileSync("docs/assets/figures/fig01_ui_main_overview.svg", svg, "utf-8");
  console.log("Created fig01_ui_main_overview.svg");
}

// ----------------------------------------------------------------------------
// 2. FIG 02: Left Panel Controls Detail
// ----------------------------------------------------------------------------
{
  const b64 = getBase64Png("docs/assets/raw/02_left_panel.png");
  const svg = `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 365 865" width="365" height="865">
  <image href="${b64}" x="0" y="0" width="365" height="865" />

  <!-- 1. Serial Port & Connect -->
  ${makeBox(8, 8, 348, 85, "#3b82f6", "rgba(59, 130, 246, 0.12)")}
  ${makeBadge("1", 330, 30, 14, "#3b82f6")}

  <!-- 2. Telemetry -->
  ${makeBox(8, 98, 348, 60, "#10b981", "rgba(16, 185, 129, 0.12)")}
  ${makeBadge("2", 330, 120, 14, "#10b981")}

  <!-- 3. Glass Preset -->
  ${makeBox(8, 164, 348, 72, "#f97316", "rgba(249, 115, 22, 0.12)")}
  ${makeBadge("3", 330, 190, 14, "#f97316")}

  <!-- 4. Nozzle Preset -->
  ${makeBox(8, 242, 348, 72, "#8b5cf6", "rgba(139, 92, 246, 0.12)")}
  ${makeBadge("4", 330, 268, 14, "#8b5cf6")}

  <!-- 5. Z-Offset -->
  ${makeBox(8, 320, 348, 72, "#ec4899", "rgba(236, 72, 153, 0.12)")}
  ${makeBadge("5", 330, 345, 14, "#ec4899")}

  <!-- 6. Print & Extrusion Speed -->
  ${makeBox(8, 398, 348, 120, "#eab308", "rgba(234, 179, 8, 0.12)")}
  ${makeBadge("6", 330, 425, 14, "#eab308")}

  <!-- 7. Infill & Pattern -->
  ${makeBox(8, 524, 348, 130, "#06b6d4", "rgba(6, 182, 212, 0.12)")}
  ${makeBadge("7", 330, 550, 14, "#06b6d4")}

  <!-- 8. Print action buttons -->
  ${makeBox(8, 660, 348, 180, "#ef4444", "rgba(239, 68, 68, 0.12)")}
  ${makeBadge("8", 330, 685, 14, "#ef4444")}
</svg>`;
  fs.writeFileSync("docs/assets/figures/fig02_left_panel_controls.svg", svg, "utf-8");
  console.log("Created fig02_left_panel_controls.svg");
}

// ----------------------------------------------------------------------------
// 3. FIG 03: Canvas2D Multi-Slide View
// ----------------------------------------------------------------------------
{
  const b64 = getBase64Png("docs/assets/raw/03_canvas.png");
  const svg = `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 740 865" width="740" height="865">
  <image href="${b64}" x="0" y="0" width="740" height="865" />

  <!-- 1. Bed boundaries -->
  ${makeBox(140, 120, 460, 620, "#3b82f6", "none", 2, 8)}
  ${makeBadge("1", 170, 150, 16, "#3b82f6")}

  <!-- 2. Prime Line area -->
  ${makeBox(155, 135, 30, 590, "#f97316", "rgba(249, 115, 22, 0.15)")}
  ${makeBadge("2", 170, 220, 16, "#f97316")}

  <!-- 3. Slide 1 -->
  ${makeBox(230, 180, 130, 480, "#10b981", "rgba(16, 185, 129, 0.1)")}
  ${makeBadge("3", 260, 210, 16, "#10b981")}

  <!-- 4. Dispensing Droplet Path inside slide -->
  ${makeBox(250, 260, 90, 300, "#8b5cf6", "rgba(139, 92, 246, 0.15)")}
  ${makeBadge("4", 295, 290, 16, "#8b5cf6")}

  <!-- 5. Coordinate Origin [0, 0] -->
  ${makeBadge("5", 150, 720, 16, "#ef4444")}
</svg>`;
  fs.writeFileSync("docs/assets/figures/fig03_canvas_multislide.svg", svg, "utf-8");
  console.log("Created fig03_canvas_multislide.svg");
}

// ----------------------------------------------------------------------------
// 4. FIG 04: Right Panel Overrides & Controls
// ----------------------------------------------------------------------------
{
  const b64 = getBase64Png("docs/assets/raw/04_right_panel.png");
  const svg = `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 365 865" width="365" height="865">
  <image href="${b64}" x="0" y="0" width="365" height="865" />

  <!-- 1. Slide Accordions -->
  ${makeBox(8, 8, 348, 120, "#3b82f6", "rgba(59, 130, 246, 0.12)")}
  ${makeBadge("1", 330, 30, 14, "#3b82f6")}

  <!-- 2. Local Z & Extrusion Overrides -->
  ${makeBox(8, 135, 348, 160, "#f97316", "rgba(249, 115, 22, 0.12)")}
  ${makeBadge("2", 330, 160, 14, "#f97316")}

  <!-- 3. Object list & visibility -->
  ${makeBox(8, 302, 348, 140, "#10b981", "rgba(16, 185, 129, 0.12)")}
  ${makeBadge("3", 330, 325, 14, "#10b981")}

  <!-- 4. Camera preview -->
  ${makeBox(8, 450, 348, 200, "#8b5cf6", "rgba(139, 92, 246, 0.12)")}
  ${makeBadge("4", 330, 475, 14, "#8b5cf6")}

  <!-- 5. Manual Jog & Homing -->
  ${makeBox(8, 658, 348, 195, "#06b6d4", "rgba(6, 182, 212, 0.12)")}
  ${makeBadge("5", 330, 680, 14, "#06b6d4")}
</svg>`;
  fs.writeFileSync("docs/assets/figures/fig04_right_panel_overrides.svg", svg, "utf-8");
  console.log("Created fig04_right_panel_overrides.svg");
}

// ----------------------------------------------------------------------------
// 5. FIG 05: Bracket Export Modal
// ----------------------------------------------------------------------------
{
  const b64 = getBase64Png("docs/assets/raw/05_bracket_export.png");
  const svg = `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 1500 950" width="1500" height="950">
  <image href="${b64}" x="0" y="0" width="1500" height="950" />

  <!-- 1. Substrate Selection -->
  ${makeBox(280, 220, 440, 100, "#3b82f6", "rgba(59, 130, 246, 0.15)")}
  ${makeBadge("1", 700, 245, 18, "#3b82f6")}

  <!-- 2. Wall & Border parameters -->
  ${makeBox(280, 330, 440, 140, "#10b981", "rgba(16, 185, 129, 0.15)")}
  ${makeBadge("2", 700, 355, 18, "#10b981")}

  <!-- 3. Flexible spring meanders -->
  ${makeBox(280, 480, 440, 140, "#f97316", "rgba(249, 115, 22, 0.15)")}
  ${makeBadge("3", 700, 505, 18, "#f97316")}

  <!-- 4. Neodymium magnet holes -->
  ${makeBox(280, 630, 440, 90, "#8b5cf6", "rgba(139, 92, 246, 0.15)")}
  ${makeBadge("4", 700, 655, 18, "#8b5cf6")}

  <!-- 5. 2D Interactive Preview -->
  ${makeBox(735, 220, 485, 490, "#06b6d4", "rgba(6, 182, 212, 0.1)")}
  ${makeBadge("5", 760, 250, 18, "#06b6d4")}

  <!-- 6. STL & SVG Export buttons -->
  ${makeBox(280, 730, 940, 70, "#ef4444", "rgba(239, 68, 68, 0.15)")}
  ${makeBadge("6", 1200, 755, 18, "#ef4444")}
</svg>`;
  fs.writeFileSync("docs/assets/figures/fig05_bracket_export.svg", svg, "utf-8");
  console.log("Created fig05_bracket_export.svg");
}

// ----------------------------------------------------------------------------
// 6. FIG 06: Diagnostics Modal
// ----------------------------------------------------------------------------
{
  const b64 = getBase64Png("docs/assets/raw/10_diagnostics_modal.png");
  const svg = `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 1500 950" width="1500" height="950">
  <image href="${b64}" x="0" y="0" width="1500" height="950" />

  <!-- 1. Info & status commands (M115, M119, M114) -->
  ${makeBox(280, 220, 460, 240, "#3b82f6", "rgba(59, 130, 246, 0.15)")}
  ${makeBadge("1", 720, 250, 18, "#3b82f6")}

  <!-- 2. Motion & Dispensing reset (G28, G92 E0, M84) -->
  ${makeBox(280, 470, 460, 250, "#10b981", "rgba(16, 185, 129, 0.15)")}
  ${makeBadge("2", 720, 500, 18, "#10b981")}

  <!-- 3. Temperature & Cooling (M140, M106) -->
  ${makeBox(750, 220, 470, 180, "#f97316", "rgba(249, 115, 22, 0.15)")}
  ${makeBadge("3", 1200, 250, 18, "#f97316")}

  <!-- 4. Interactive G-code Terminal -->
  ${makeBox(750, 410, 470, 310, "#8b5cf6", "rgba(139, 92, 246, 0.15)")}
  ${makeBadge("4", 1200, 440, 18, "#8b5cf6")}
</svg>`;
  fs.writeFileSync("docs/assets/figures/fig06_diagnostics_modal.svg", svg, "utf-8");
  console.log("Created fig06_diagnostics_modal.svg");
}

// ----------------------------------------------------------------------------
// 7. FIG 07: C4 Architecture Diagram (LikeC4 Style)
// ----------------------------------------------------------------------------
{
  const svg = `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 820" width="1200" height="820">
  <defs>
    <filter id="c4-shadow" x="-5%" y="-5%" width="110%" height="110%">
      <feDropShadow dx="0" dy="4" stdDeviation="6" flood-color="#0f172a" flood-opacity="0.15"/>
    </filter>
    <linearGradient id="tauri-grad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#2563eb" />
      <stop offset="100%" stop-color="#1d4ed8" />
    </linearGradient>
    <linearGradient id="svelte-grad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#ea580c" />
      <stop offset="100%" stop-color="#c2410c" />
    </linearGradient>
    <linearGradient id="rust-grad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#b45309" />
      <stop offset="100%" stop-color="#78350f" />
    </linearGradient>
    <linearGradient id="hw-grad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#475569" />
      <stop offset="100%" stop-color="#334155" />
    </linearGradient>
  </defs>

  <!-- Background Canvas -->
  <rect width="1200" height="820" fill="#f8fafc" rx="12" />
  <rect x="15" y="15" width="1170" height="790" fill="none" stroke="#e2e8f0" stroke-width="1.5" rx="8" />

  <!-- Title Header -->
  <g transform="translate(40, 50)">
    <text font-family="'DejaVu Sans', sans-serif" font-size="22px" font-weight="bold" fill="#0f172a">[Diagram C4] Architektura systému Droplet Printing Interface (DPI)</text>
    <text font-family="'DejaVu Sans', sans-serif" font-size="13px" fill="#64748b" y="24">Zobrazení kontejnerů, komponent jádra a fyzického hardwaru 3D tiskárny</text>
  </g>

  <!-- PERSON / USER -->
  <g transform="translate(60, 130)" filter="url(#c4-shadow)">
    <rect width="220" height="110" rx="8" fill="#0284c7" />
    <circle cx="110" cy="35" r="16" fill="#ffffff" opacity="0.9" />
    <path d="M 90 70 Q 110 50 130 70" fill="none" stroke="#ffffff" stroke-width="5" stroke-linecap="round" />
    <text x="110" y="92" text-anchor="middle" font-family="'DejaVu Sans', sans-serif" font-size="14px" font-weight="bold" fill="#ffffff">Laborant / Výzkumník</text>
    <text x="110" y="104" text-anchor="middle" font-family="'DejaVu Sans', sans-serif" font-size="10px" fill="#e0f2fe">[Uživatel]</text>
  </g>

  <!-- TAURI APPLICATION BOUNDARY -->
  <g transform="translate(340, 110)">
    <rect width="810" height="490" rx="10" fill="#ffffff" stroke="#3b82f6" stroke-width="2" stroke-dasharray="6 4" />
    <text x="20" y="28" font-family="'DejaVu Sans', sans-serif" font-size="14px" font-weight="bold" fill="#2563eb">Desktopová aplikace DPI [Tauri v2 Application Boundary]</text>

    <!-- CONTAINER 1: SVELTE 5 FRONTEND -->
    <g transform="translate(25, 50)" filter="url(#c4-shadow)">
      <rect width="360" height="390" rx="8" fill="url(#svelte-grad)" />
      <text x="20" y="32" font-family="'DejaVu Sans', sans-serif" font-size="16px" font-weight="bold" fill="#ffffff">Svelte 5 Frontend</text>
      <text x="20" y="50" font-family="'DejaVu Sans', sans-serif" font-size="11px" fill="#fed7aa">[Kontejner: HTML5 Canvas, Runes, Tailwind]</text>

      <!-- Component A: Canvas2D -->
      <rect x="20" y="70" width="320" height="65" rx="6" fill="rgba(255,255,255,0.18)" stroke="#ffffff" stroke-width="1" />
      <text x="35" y="94" font-family="'DejaVu Sans', sans-serif" font-size="13px" font-weight="bold" fill="#ffffff">Canvas2D Renderer</text>
      <text x="35" y="114" font-family="'DejaVu Sans', sans-serif" font-size="10.5px" fill="#ffedd5">Vykreslování podložky, tras a interpolace trysky</text>

      <!-- Component B: State Stores -->
      <rect x="20" y="145" width="320" height="65" rx="6" fill="rgba(255,255,255,0.18)" stroke="#ffffff" stroke-width="1" />
      <text x="35" y="169" font-family="'DejaVu Sans', sans-serif" font-size="13px" font-weight="bold" fill="#ffffff">Reaktivní Stores (Runes)</text>
      <text x="35" y="189" font-family="'DejaVu Sans', sans-serif" font-size="10.5px" fill="#ffedd5">projectStore, settingsStore, liquidStore</text>

      <!-- Component C: Modals and Panels -->
      <rect x="20" y="220" width="320" height="65" rx="6" fill="rgba(255,255,255,0.18)" stroke="#ffffff" stroke-width="1" />
      <text x="35" y="244" font-family="'DejaVu Sans', sans-serif" font-size="13px" font-weight="bold" fill="#ffffff">UI Komponenty &amp; Modály</text>
      <text x="35" y="264" font-family="'DejaVu Sans', sans-serif" font-size="10.5px" fill="#ffedd5">LeftPanel, RightPanel, BracketExport, ZCalib</text>

      <!-- Component D: WASM Bridge -->
      <rect x="20" y="295" width="320" height="70" rx="6" fill="rgba(255,255,255,0.25)" stroke="#ffffff" stroke-width="1.5" />
      <text x="35" y="322" font-family="'DejaVu Sans', sans-serif" font-size="13px" font-weight="bold" fill="#ffffff">WASM Bridge (dpi-core)</text>
      <text x="35" y="342" font-family="'DejaVu Sans', sans-serif" font-size="10.5px" fill="#ffedd5">Okamžitý přepočet náhledu držáků &amp; drah</text>
    </g>

    <!-- CONTAINER 2: RUST TAURI BACKEND -->
    <g transform="translate(420, 50)" filter="url(#c4-shadow)">
      <rect width="360" height="390" rx="8" fill="url(#rust-grad)" />
      <text x="20" y="32" font-family="'DejaVu Sans', sans-serif" font-size="16px" font-weight="bold" fill="#ffffff">Rust Core Backend</text>
      <text x="20" y="50" font-family="'DejaVu Sans', sans-serif" font-size="11px" fill="#fef3c7">[Kontejner: Tauri v2, Tokio, Serialport]</text>

      <!-- Component E: Serial Comms Loop -->
      <rect x="20" y="70" width="320" height="65" rx="6" fill="rgba(255,255,255,0.18)" stroke="#ffffff" stroke-width="1" />
      <text x="35" y="94" font-family="'DejaVu Sans', sans-serif" font-size="13px" font-weight="bold" fill="#ffffff">comms.rs (Serial Thread)</text>
      <text x="35" y="114" font-family="'DejaVu Sans', sans-serif" font-size="10.5px" fill="#fef3c7">Dedikované vlákno, mpsc fronta, handshake</text>

      <!-- Component F: GCode Engine & Slicer -->
      <rect x="20" y="145" width="320" height="65" rx="6" fill="rgba(255,255,255,0.18)" stroke="#ffffff" stroke-width="1" />
      <text x="35" y="169" font-family="'DejaVu Sans', sans-serif" font-size="13px" font-weight="bold" fill="#ffffff">dpi-core (Nativní jádro)</text>
      <text x="35" y="189" font-family="'DejaVu Sans', sans-serif" font-size="10.5px" fill="#fef3c7">TSP optimalizace, Bézier křivky, extruze</text>

      <!-- Component G: Settings & Config -->
      <rect x="20" y="220" width="320" height="65" rx="6" fill="rgba(255,255,255,0.18)" stroke="#ffffff" stroke-width="1" />
      <text x="35" y="244" font-family="'DejaVu Sans', sans-serif" font-size="13px" font-weight="bold" fill="#ffffff">settings.rs &amp; Storage</text>
      <text x="35" y="264" font-family="'DejaVu Sans', sans-serif" font-size="10.5px" fill="#fef3c7">Atomický zápis, validace typů, defaulty</text>

      <!-- Component H: Sync WS Server -->
      <rect x="20" y="295" width="320" height="70" rx="6" fill="rgba(255,255,255,0.18)" stroke="#ffffff" stroke-width="1" />
      <text x="35" y="322" font-family="'DejaVu Sans', sans-serif" font-size="13px" font-weight="bold" fill="#ffffff">sync.rs (WebSocket Server)</text>
      <text x="35" y="342" font-family="'DejaVu Sans', sans-serif" font-size="10.5px" fill="#fef3c7">Sdílení telemetrie do laboratorní sítě</text>
    </g>
  </g>

  <!-- EXTERNAL HARDWARE -->
  <!-- 1. 3D Printer -->
  <g transform="translate(680, 640)" filter="url(#c4-shadow)">
    <rect width="360" height="130" rx="8" fill="url(#hw-grad)" />
    <text x="20" y="32" font-family="'DejaVu Sans', sans-serif" font-size="16px" font-weight="bold" fill="#ffffff">Prusa i3 MK3S (3D Tiskárna)</text>
    <text x="20" y="52" font-family="'DejaVu Sans', sans-serif" font-size="11px" fill="#cbd5e1">[Hardware &amp; Marlin Firmware]</text>
    <text x="20" y="80" font-family="'DejaVu Sans', sans-serif" font-size="11px" fill="#ffffff">• Stříkačkový dávkovač na pístu (NEMA 17)</text>
    <text x="20" y="100" font-family="'DejaVu Sans', sans-serif" font-size="11px" fill="#ffffff">• PINDA indukční sonda (nivelace bodů)</text>
    <text x="20" y="120" font-family="'DejaVu Sans', sans-serif" font-size="11px" fill="#ffffff">• Vyhřívaná podložka s magnetickou matricí</text>
  </g>

  <!-- 2. USB Camera -->
  <g transform="translate(360, 640)" filter="url(#c4-shadow)">
    <rect width="280" height="130" rx="8" fill="url(#hw-grad)" />
    <text x="20" y="32" font-family="'DejaVu Sans', sans-serif" font-size="15px" font-weight="bold" fill="#ffffff">USB Mikroskop / Kamera</text>
    <text x="20" y="52" font-family="'DejaVu Sans', sans-serif" font-size="11px" fill="#cbd5e1">[Optická kontrola]</text>
    <text x="20" y="80" font-family="'DejaVu Sans', sans-serif" font-size="11px" fill="#ffffff">• Sledování tvorby kapky na jehle</text>
    <text x="20" y="100" font-family="'DejaVu Sans', sans-serif" font-size="11px" fill="#ffffff">• Vizuální asistence Z-Shiftu</text>
    <text x="20" y="120" font-family="'DejaVu Sans', sans-serif" font-size="11px" fill="#ffffff">• WebKit2GTK / WebView2 Stream</text>
  </g>

  <!-- CONNECTIONS / ARROWS -->
  <!-- User -> Frontend -->
  <line x1="280" y1="185" x2="365" y2="185" stroke="#0284c7" stroke-width="3" marker-end="url(#arrow)" />
  <text x="322" y="175" text-anchor="middle" font-family="'DejaVu Sans', sans-serif" font-size="10px" font-weight="bold" fill="#0369a1">Ovládá UI</text>

  <!-- Frontend <-> Backend IPC -->
  <line x1="725" y1="280" x2="760" y2="280" stroke="#3b82f6" stroke-width="3" />
  <text x="742" y="270" text-anchor="middle" font-family="'DejaVu Sans', sans-serif" font-size="10px" font-weight="bold" fill="#1d4ed8">IPC Invoke</text>

  <!-- Backend -> 3D Printer (Serial) -->
  <line x1="940" y1="550" x2="940" y2="640" stroke="#f59e0b" stroke-width="3" stroke-dasharray="4 2" />
  <text x="945" y="600" font-family="'DejaVu Sans', sans-serif" font-size="10px" font-weight="bold" fill="#b45309">USB CDC Serial (115200)</text>

  <!-- Camera -> Frontend -->
  <line x1="500" y1="640" x2="500" y2="550" stroke="#10b981" stroke-width="3" stroke-dasharray="4 2" />
  <text x="505" y="600" font-family="'DejaVu Sans', sans-serif" font-size="10px" font-weight="bold" fill="#047857">WebRTC / MediaDevices</text>
</svg>`;
  fs.writeFileSync("docs/assets/figures/fig07_architecture_c4.svg", svg, "utf-8");
  console.log("Created fig07_architecture_c4.svg");
}

// ----------------------------------------------------------------------------
// 8. FIG 08: Syringe Hardware Assembly Diagram
// ----------------------------------------------------------------------------
{
  const svg = `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1000 650" width="1000" height="650">
  <defs>
    <filter id="hw-shadow" x="-5%" y="-5%" width="110%" height="110%">
      <feDropShadow dx="0" dy="3" stdDeviation="4" flood-color="#000" flood-opacity="0.1"/>
    </filter>
  </defs>

  <rect width="1000" height="650" fill="#f8fafc" rx="10" />
  <rect x="15" y="15" width="970" height="620" fill="none" stroke="#cbd5e1" stroke-width="1.5" rx="8" />

  <text x="40" y="50" font-family="'DejaVu Sans', sans-serif" font-size="20px" font-weight="bold" fill="#0f172a">Schéma sestavy stříkačkového dávkovače na tiskárně Prusa MK3S</text>
  <text x="40" y="72" font-family="'DejaVu Sans', sans-serif" font-size="12px" fill="#64748b">Přestavba osy X z tavení filamentu na laboratorní dávkování kapalin</text>

  <!-- Base Plate / Bed -->
  <g transform="translate(60, 480)">
    <!-- Heated bed -->
    <rect x="0" y="50" width="880" height="25" rx="3" fill="#334155" />
    <text x="20" y="67" font-family="'DejaVu Sans', sans-serif" font-size="11px" font-weight="bold" fill="#ffffff">Vyhřívaná podložka Prusa MK3S (Magnetický ocelový plát 250 × 210 mm)</text>

    <!-- Slide Holder Bracket -->
    <rect x="300" y="25" width="400" height="25" rx="4" fill="#f97316" filter="url(#hw-shadow)" />
    <text x="320" y="42" font-family="'DejaVu Sans', sans-serif" font-size="11px" font-weight="bold" fill="#ffffff">3D tištěná matrice (Bracket Export) s magnety</text>

    <!-- Microscope Slides -->
    <rect x="350" y="12" width="140" height="13" rx="2" fill="#93c5fd" stroke="#2563eb" stroke-width="1" />
    <text x="365" y="22" font-family="'DejaVu Sans', sans-serif" font-size="9px" font-weight="bold" fill="#1e3a8a">Sklo 76 × 26 × 1 mm</text>

    <rect x="520" y="12" width="140" height="13" rx="2" fill="#93c5fd" stroke="#2563eb" stroke-width="1" />
    <text x="535" y="22" font-family="'DejaVu Sans', sans-serif" font-size="9px" font-weight="bold" fill="#1e3a8a">Sklo 76 × 26 × 1 mm</text>
  </g>

  <!-- Syringe Dispenser Mount (Axis X Head) -->
  <g transform="translate(360, 110)" filter="url(#hw-shadow)">
    <!-- X Carriage block -->
    <rect x="-60" y="60" width="260" height="260" rx="8" fill="#1e293b" />
    <text x="-40" y="85" font-family="'DejaVu Sans', sans-serif" font-size="12px" font-weight="bold" fill="#94a3b8">X-Carriage Držák</text>

    <!-- NEMA 17 Stepper Motor -->
    <rect x="20" y="-10" width="100" height="80" rx="4" fill="#475569" stroke="#334155" stroke-width="2" />
    <text x="70" y="35" text-anchor="middle" font-family="'DejaVu Sans', sans-serif" font-size="11px" font-weight="bold" fill="#ffffff">NEMA 17</text>
    <text x="70" y="50" text-anchor="middle" font-family="'DejaVu Sans', sans-serif" font-size="9px" fill="#cbd5e1">Krokový motor</text>

    <!-- Lead Screw / Plunger Driver -->
    <rect x="62" y="70" width="16" height="70" fill="#94a3b8" />
    <line x1="62" y1="80" x2="78" y2="80" stroke="#475569" />
    <line x1="62" y1="95" x2="78" y2="95" stroke="#475569" />
    <line x1="62" y1="110" x2="78" y2="110" stroke="#475569" />
    <line x1="62" y1="125" x2="78" y2="125" stroke="#475569" />

    <!-- Syringe Plunger -->
    <rect x="40" y="140" width="60" height="15" rx="2" fill="#0284c7" />
    <rect x="64" y="155" width="12" height="60" fill="#38bdf8" />
    <rect x="48" y="215" width="44" height="12" rx="2" fill="#0284c7" />

    <!-- Syringe Body (Glass/Plastic) -->
    <rect x="45" y="227" width="50" height="120" rx="4" fill="rgba(224, 242, 254, 0.7)" stroke="#0284c7" stroke-width="2" />
    <!-- Graduation marks -->
    <line x1="45" y1="245" x2="58" y2="245" stroke="#0284c7" stroke-width="1.5" />
    <line x1="45" y1="265" x2="65" y2="265" stroke="#0284c7" stroke-width="1.5" />
    <line x1="45" y1="285" x2="58" y2="285" stroke="#0284c7" stroke-width="1.5" />
    <line x1="45" y1="305" x2="65" y2="305" stroke="#0284c7" stroke-width="1.5" />
    <line x1="45" y1="325" x2="58" y2="325" stroke="#0284c7" stroke-width="1.5" />
    <text x="80" y="295" font-family="'DejaVu Sans', sans-serif" font-size="10px" font-weight="bold" fill="#0369a1">1 ml</text>

    <!-- Liquid inside -->
    <rect x="47" y="275" width="46" height="72" fill="#3b82f6" opacity="0.65" />

    <!-- Luer Lock Tip -->
    <path d="M 60 347 L 80 347 L 76 360 L 64 360 Z" fill="#f97316" />

    <!-- Dispensing Needle (Modrá 0.41 mm) -->
    <rect x="68" y="360" width="4" height="24" fill="#2563eb" />
    <!-- Droplet forming -->
    <circle cx="70" cy="386" r="3.5" fill="#3b82f6" />

    <!-- PINDA Sensor mounted next to syringe -->
    <rect x="140" y="190" width="22" height="75" rx="3" fill="#eab308" stroke="#ca8a04" stroke-width="1.5" />
    <rect x="144" y="265" width="14" height="8" fill="#1e293b" />
    <text x="151" y="235" text-anchor="middle" font-family="'DejaVu Sans', sans-serif" font-size="9px" font-weight="bold" fill="#000000" transform="rotate(-90, 151, 235)">PINDA Sonda</text>

    <!-- USB Microscope Camera -->
    <rect x="-40" y="210" width="30" height="90" rx="5" fill="#10b981" stroke="#059669" stroke-width="1.5" />
    <circle cx="-25" cy="305" r="8" fill="#064e3b" />
    <text x="-25" y="255" text-anchor="middle" font-family="'DejaVu Sans', sans-serif" font-size="9px" font-weight="bold" fill="#ffffff" transform="rotate(-90, -25, 255)">Kamera</text>
  </g>

  <!-- Labels & Callouts -->
  <g font-family="'DejaVu Sans', sans-serif" font-size="11px" fill="#1e293b">
    <!-- Callout 1 -->
    <path d="M 330 140 L 410 140" stroke="#0284c7" stroke-width="1.5" stroke-dasharray="3 3" />
    <text x="170" y="145" font-weight="bold" fill="#0284c7">① Krokový pohon pístu</text>

    <!-- Callout 2 -->
    <path d="M 330 360 L 410 360" stroke="#0284c7" stroke-width="1.5" stroke-dasharray="3 3" />
    <text x="170" y="365" font-weight="bold" fill="#0284c7">② Stříkačka s roztokem</text>

    <!-- Callout 3 -->
    <path d="M 350 480 L 430 480" stroke="#2563eb" stroke-width="1.5" stroke-dasharray="3 3" />
    <text x="170" y="485" font-weight="bold" fill="#2563eb">③ Luer-Lock jehla (0.3 / 0.41 mm)</text>

    <!-- Callout 4 -->
    <path d="M 580 340 L 525 340" stroke="#dc2626" stroke-width="1.5" stroke-dasharray="3 3" />
    <text x="590" y="345" font-weight="bold" fill="#dc2626">④ PINDA sonda (Před tiskem odklopit!)</text>

    <!-- Callout 5 -->
    <path d="M 280 270 L 320 270" stroke="#059669" stroke-width="1.5" stroke-dasharray="3 3" />
    <text x="170" y="275" font-weight="bold" fill="#059669">⑤ USB mikroskop</text>
  </g>
</svg>`;
  fs.writeFileSync("docs/assets/figures/fig08_syringe_hardware.svg", svg, "utf-8");
  console.log("Created fig08_syringe_hardware.svg");
}

// ----------------------------------------------------------------------------
// 9. FIG 09: Z-Shift Calculation Theory Diagram
// ----------------------------------------------------------------------------
{
  const svg = `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 950 520" width="950" height="520">
  <defs>
    <marker id="arrow-up" viewBox="0 0 10 10" refX="5" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
      <path d="M 0 5 L 10 0 L 10 10 z" fill="#2563eb"/>
    </marker>
    <marker id="arrow-down" viewBox="0 0 10 10" refX="5" refY="5" markerWidth="6" markerHeight="6" orient="auto">
      <path d="M 0 5 L 10 0 L 10 10 z" fill="#2563eb"/>
    </marker>
  </defs>

  <rect width="950" height="520" fill="#f8fafc" rx="10" />
  <rect x="15" y="15" width="920" height="490" fill="none" stroke="#cbd5e1" stroke-width="1.5" rx="8" />

  <text x="40" y="50" font-family="'DejaVu Sans', sans-serif" font-size="20px" font-weight="bold" fill="#0f172a">Princip kalibrace výšky trysky a korekce Z-Shift</text>
  <text x="40" y="72" font-family="'DejaVu Sans', sans-serif" font-size="12px" fill="#64748b">Matematický model výpočtu bezpečné výšky v dpi-core::gcode</text>

  <!-- Reference Surface: Heatbed Plate -->
  <line x1="80" y1="440" x2="860" y2="440" stroke="#0f172a" stroke-width="3" />
  <text x="90" y="460" font-family="'DejaVu Sans', sans-serif" font-size="11px" font-weight="bold" fill="#475569">Z = 0.0 (Fyzický povrch ocelového plátu Prusa)</text>

  <!-- Glass Substrate -->
  <rect x="320" y="415" width="280" height="25" fill="#93c5fd" stroke="#2563eb" stroke-width="1.5" rx="2" />
  <text x="380" y="432" font-family="'DejaVu Sans', sans-serif" font-size="11px" font-weight="bold" fill="#1e3a8a">Podložní sklo (slide_z = 1.0 mm)</text>

  <!-- Reference Block -->
  <rect x="180" y="240" width="100" height="200" fill="#e2e8f0" stroke="#64748b" stroke-width="2" rx="4" />
  <text x="230" y="340" text-anchor="middle" font-family="'DejaVu Sans', sans-serif" font-size="12px" font-weight="bold" fill="#334155">Hlava tiskárny</text>
  <text x="230" y="360" text-anchor="middle" font-family="'DejaVu Sans', sans-serif" font-size="10px" fill="#64748b">(block_height = 34 mm)</text>

  <!-- Needle mounted into block -->
  <rect x="222" y="215" width="16" height="25" fill="#f97316" />
  <text x="250" y="230" font-family="'DejaVu Sans', sans-serif" font-size="10px" fill="#c2410c">nozzle_hidden (4 mm)</text>

  <rect x="227" y="240" width="6" height="150" fill="#3b82f6" />
  <text x="245" y="300" font-family="'DejaVu Sans', sans-serif" font-size="10px" fill="#1d4ed8">nozzle_height (30 mm)</text>

  <!-- Height Dimension Lines -->
  <!-- Block Height -->
  <line x1="140" y1="440" x2="140" y2="240" stroke="#64748b" stroke-width="1.5" />
  <line x1="130" y1="440" x2="150" y2="440" stroke="#64748b" stroke-width="1.5" />
  <line x1="130" y1="240" x2="150" y2="240" stroke="#64748b" stroke-width="1.5" />
  <text x="125" y="345" text-anchor="end" font-family="'DejaVu Sans', sans-serif" font-size="11px" font-weight="bold" fill="#475569">block_height</text>

  <!-- Target Glass Height Formula Box -->
  <g transform="translate(610, 130)">
    <rect width="310" height="270" rx="8" fill="#eff6ff" stroke="#3b82f6" stroke-width="1.5" />
    <text x="18" y="28" font-family="'DejaVu Sans', sans-serif" font-size="13px" font-weight="bold" fill="#1d4ed8">Vzorec teoretické výšky (gcode.rs):</text>

    <text x="18" y="58" font-family="'JetBrainsMono NF', monospace" font-size="11px" font-weight="bold" fill="#0f172a">glass_z_theor =</text>
    <text x="25" y="78" font-family="'JetBrainsMono NF', monospace" font-size="10.5px" fill="#ef4444">- block_height (34 mm)</text>
    <text x="25" y="98" font-family="'JetBrainsMono NF', monospace" font-size="10.5px" fill="#2563eb">+ nozzle_height (31 mm)</text>
    <text x="25" y="118" font-family="'JetBrainsMono NF', monospace" font-size="10.5px" fill="#ef4444">- nozzle_hidden (4 mm)</text>
    <text x="25" y="138" font-family="'JetBrainsMono NF', monospace" font-size="10.5px" fill="#10b981">+ slide_z (1 mm) = -6.0 mm</text>

    <line x1="18" y1="155" x2="292" y2="155" stroke="#bfdbfe" stroke-width="1" />

    <text x="18" y="178" font-family="'DejaVu Sans', sans-serif" font-size="11px" font-weight="bold" fill="#0f172a">Korekce Z-Shift (kladný offset):</text>
    <text x="18" y="198" font-family="'JetBrainsMono NF', monospace" font-size="10px" fill="#15803d">z_shift = |Z_min| + 1.0 mm (7.0 mm)</text>
    <text x="18" y="218" font-family="'DejaVu Sans', sans-serif" font-size="10px" fill="#475569">M211 S0 (vypnutí soft endstopů)</text>
    <text x="18" y="238" font-family="'JetBrainsMono NF', monospace" font-size="10px" font-weight="bold" fill="#15803d">G92 Z... (přenastavení nuly na sklo)</text>
    <text x="18" y="258" font-family="'JetBrainsMono NF', monospace" font-size="10.5px" font-weight="bold" fill="#1e3a8a">Z_tisk = Z_theor + z_shift + z_offset</text>
  </g>
</svg>`;
  fs.writeFileSync("docs/assets/figures/fig09_z_shift_principle.svg", svg, "utf-8");
  console.log("Created fig09_z_shift_principle.svg");
}

console.log("All SVG figures successfully created!");
