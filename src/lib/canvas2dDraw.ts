/**
 * Čisté kreslicí funkce pro Canvas2D.svelte — bez vazby na stav komponenty.
 * Komponenta drží kameru (pan/zoom), interakce a hit-testing; sem patří vše,
 * co jen kreslí do CanvasRenderingContext2D na základě předaných hodnot.
 */
import type { LayoutPosition, SubstratePaths, Transform, Point2D, SlideOverride, PreviewSegData } from "./tauri";
import { getTransformIdx } from "./geometry";

/** Transformuje bod z path-lokálního prostoru do world space.
 *  Odpovídá transform_pt v Rustu; Y-flip zajišťuje canvas scale(zoom,-zoom). */
export function tpt(
  px: number,
  py: number,
  pX: number,
  pY: number,
  pScale: number,
  pRot: number,
  pCX: number,
  pCY: number
): [number, number] {
  const dx = (px - pCX) * pScale;
  const dy = (py - pCY) * pScale;
  const rad = (-pRot * Math.PI) / 180;
  const cr = Math.cos(rad),
    sr = Math.sin(rad);
  return [pX + pCX + dx * cr - dy * sr, pY + pCY + dx * sr + dy * cr];
}

export interface PreviewCursor {
  slideIdx: number;
  segIdx: number;
  ptIdx: number;
  fracT: number;
}

/** Mapuje printProgress (0–100 %) na přesnou vzdálenostní pozici v trase. */
export function computePreviewCursor(
  printProgress: number,
  precomputedSegs: PreviewSegData[],
  totalPreviewDist: number
): PreviewCursor | null {
  if (printProgress >= 100 || precomputedSegs.length === 0 || totalPreviewDist <= 0) return null;
  const targetDist = totalPreviewDist * (printProgress / 100);
  for (let ci = 0; ci < precomputedSegs.length; ci++) {
    const sd = precomputedSegs[ci];
    if (targetDist <= sd.path_start_dist + sd.seg_dist || ci === precomputedSegs.length - 1) {
      const distIntoSeg = Math.max(0, targetDist - sd.path_start_dist);
      let ptIdx = sd.point_dists.length - 1;
      for (let j = 1; j < sd.point_dists.length; j++) {
        if (sd.point_dists[j] >= distIntoSeg) { ptIdx = j - 1; break; }
      }
      const d0 = sd.point_dists[ptIdx];
      const d1 = ptIdx + 1 < sd.point_dists.length ? sd.point_dists[ptIdx + 1] : d0;
      const interval = d1 - d0;
      const fracT = interval > 1e-9 ? Math.min(1, (distIntoSeg - d0) / interval) : 0;
      return { slideIdx: sd.slide_idx, segIdx: sd.seg_idx, ptIdx, fracT };
    }
  }
  return null;
}

export interface CameraView {
  width: number;
  height: number;
  panX: number;
  panY: number;
  zoom: number;
}

/** Pozadí, výplň bedu, mřížka (minor/major) a okraj bedu. Volat uvnitř world transformu. */
export function drawBedAndGrid(
  ctx: CanvasRenderingContext2D,
  view: CameraView,
  bedMaxX: number,
  bedMaxY: number,
  showAxes: boolean,
  gridMinor: number,
  gridMajor: number
) {
  const { width, height, panX, panY, zoom } = view;

  // Bed fill
  ctx.fillStyle = "#1e293b";
  ctx.fillRect(0, 0, bedMaxX, bedMaxY);

  // Grid
  if (showAxes) {
    const minX = -panX / zoom,
      maxX = (width - panX) / zoom;
    const minY = (panY - height) / zoom,
      maxY = panY / zoom;

    ctx.strokeStyle = "#243148";
    ctx.lineWidth = 0.5 / zoom;
    ctx.beginPath();
    for (let x = Math.floor(minX / gridMinor) * gridMinor; x < maxX + gridMinor; x += gridMinor) {
      ctx.moveTo(x, minY);
      ctx.lineTo(x, maxY);
    }
    for (let y = Math.floor(minY / gridMinor) * gridMinor; y < maxY + gridMinor; y += gridMinor) {
      ctx.moveTo(minX, y);
      ctx.lineTo(maxX, y);
    }
    ctx.stroke();

    ctx.strokeStyle = "#334155";
    ctx.lineWidth = 1 / zoom;
    ctx.beginPath();
    for (let x = Math.floor(minX / gridMajor) * gridMajor; x < maxX + gridMajor; x += gridMajor) {
      ctx.moveTo(x, minY);
      ctx.lineTo(x, maxY);
    }
    for (let y = Math.floor(minY / gridMajor) * gridMajor; y < maxY + gridMajor; y += gridMajor) {
      ctx.moveTo(minX, y);
      ctx.lineTo(maxX, y);
    }
    ctx.stroke();
  }

  // Bed border
  ctx.strokeStyle = "#64748b";
  ctx.lineWidth = 1.5 / zoom;
  ctx.strokeRect(0, 0, bedMaxX, bedMaxY);
}

export interface SlidesDrawOptions {
  positions: LayoutPosition[];
  paths: SubstratePaths[];
  primePath: SubstratePaths | null;
  transforms: Transform[];
  overrides: Record<string, SlideOverride>;
  selectedIndex: number;
  nozzleDiam: number;
  zoom: number;
  cursor: PreviewCursor | null;
  /** Rozměry skla pro odplivové sklíčko typu „vzorkové" (slide_w/slide_h z parametrů). */
  primeGlassSize: { w: number; h: number } | null;
}

/** Sklíčka, trasy (s ořezem dle preview kurzoru) a popisky.
 *  Vrací world pozici preview trysky (konec nakreslené trasy), pokud existuje. */
export function drawSlidesAndPaths(
  ctx: CanvasRenderingContext2D,
  o: SlidesDrawOptions
): { x: number; y: number } | null {
  const { positions, paths, primePath, transforms, overrides, selectedIndex, nozzleDiam, zoom, cursor } = o;

  let previewNozzleWX = 0,
    previewNozzleWY = 0,
    hasPreviewNozzle = false;
  let previewDone = false;

  for (let i = 0; i < positions.length; i++) {
    const pos = positions[i];
    const tidx = getTransformIdx(i, positions);
    const isSelected = i === selectedIndex;

    const slideColor = pos.is_prime ? "#f97316" : isSelected ? "#3b82f6" : "#94a3b8";

    let glassW = pos.width;
    let glassH = pos.height;
    if (pos.is_prime && overrides["-1"]?.glass_type === "vzorkové" && o.primeGlassSize) {
      glassW = o.primeGlassSize.w;
      glassH = o.primeGlassSize.h;
    }

    // ── SKLO (pevná pozice, bez transformace) ───────────────────────────
    ctx.fillStyle = slideColor;
    ctx.globalAlpha = isSelected ? 0.15 : 0.1;
    ctx.fillRect(pos.x, pos.y, glassW, glassH);
    ctx.globalAlpha = 1.0;

    ctx.strokeStyle = slideColor;
    ctx.lineWidth = (isSelected ? 2.0 : 1.0) / zoom;
    ctx.strokeRect(pos.x, pos.y, glassW, glassH);

    // ── TRASA ────────────────────────────────────────────────────────────
    // Parametry transformace (výchozí = identity pro primepath)
    let pathData: SubstratePaths | null = null;
    let pX = pos.x,
      pY = pos.y,
      pRot = 0.0,
      pScale = 1.0;
    let pCX = pos.width / 2,
      pCY = pos.height / 2;

    if (pos.is_prime) {
      pathData = primePath;
    } else if (transforms[tidx]) {
      const tr = transforms[tidx];
      pathData = paths[tidx] || null;
      pX = tr.gui_dx;
      pY = tr.gui_dy;
      pRot = tr.rotation;
      pScale = tr.scale;
      pCX = tr.cx;
      pCY = tr.cy;
    }

    if (pathData && pathData.segments.length > 0) {
      ctx.save();
      // Klip na hranici skla (world souřadnice — bariéra bez paddingu)
      ctx.beginPath();
      ctx.rect(pos.x, pos.y, glassW, glassH);
      ctx.clip();

      ctx.strokeStyle = slideColor;
      ctx.fillStyle = slideColor;
      ctx.globalAlpha = isSelected ? 1.0 : 0.7;
      ctx.lineWidth = nozzleDiam;
      ctx.lineJoin = "round";
      ctx.lineCap = "round";

      for (let si = 0; si < pathData.segments.length; si++) {
        const seg = pathData.segments[si];
        if (seg.points.length === 0) continue;

        // Určení rozsahu kreslení dle cursor pozice
        let toRender = seg.points.length;
        let fracWX: number | null = null,
          fracWY: number | null = null;

        if (cursor !== null) {
          if (i > cursor.slideIdx || (i === cursor.slideIdx && si > cursor.segIdx)) {
            // Po kurzoru — přeskočit vše
            break;
          } else if (i === cursor.slideIdx && si === cursor.segIdx) {
            // Kurzorovací segment — kreslit jen do ptIdx, pak interpolovat
            toRender = cursor.ptIdx + 1;
            if (cursor.fracT > 0 && cursor.ptIdx + 1 < seg.points.length) {
              const p0 = seg.points[cursor.ptIdx];
              const p1 = seg.points[cursor.ptIdx + 1];
              const lx = p0.x + (p1.x - p0.x) * cursor.fracT;
              const ly = p0.y + (p1.y - p0.y) * cursor.fracT;
              [fracWX, fracWY] = tpt(lx, ly, pX, pY, pScale, pRot, pCX, pCY);
              previewNozzleWX = fracWX;
              previewNozzleWY = fracWY;
              hasPreviewNozzle = true;
            } else if (toRender > 0) {
              const lp = seg.points[cursor.ptIdx];
              [previewNozzleWX, previewNozzleWY] = tpt(lp.x, lp.y, pX, pY, pScale, pRot, pCX, pCY);
              hasPreviewNozzle = true;
            }
            previewDone = true;
          }
          // else: before cursor — draw all (toRender stays seg.points.length)
        }

        const p0 = seg.points[0];
        const isDot =
          fracWX === null &&
          toRender <= 2 &&
          (toRender === 1 ||
            (Math.abs(p0.x - seg.points[Math.min(1, toRender - 1)].x) < 0.01 &&
             Math.abs(p0.y - seg.points[Math.min(1, toRender - 1)].y) < 0.01));

        if (isDot) {
          const [wx, wy] = tpt(p0.x, p0.y, pX, pY, pScale, pRot, pCX, pCY);
          ctx.beginPath();
          ctx.arc(wx, wy, nozzleDiam / 2, 0, Math.PI * 2);
          ctx.fill();
        } else if (toRender > 0) {
          ctx.beginPath();
          for (let j = 0; j < toRender; j++) {
            const [wx, wy] = tpt(seg.points[j].x, seg.points[j].y, pX, pY, pScale, pRot, pCX, pCY);
            if (j === 0) ctx.moveTo(wx, wy);
            else ctx.lineTo(wx, wy);
          }
          // Dokreslení interpolované části do kurzorové pozice
          if (fracWX !== null && fracWY !== null) {
            ctx.lineTo(fracWX, fracWY);
          }
          ctx.stroke();

          // Označení začátků segmentů pro vybranou trasu (orientace tisku)
          if (isSelected && !pos.is_prime) {
            const [sx, sy] = tpt(seg.points[0].x, seg.points[0].y, pX, pY, pScale, pRot, pCX, pCY);
            ctx.fillStyle = "#22d3ee";
            ctx.beginPath();
            ctx.arc(sx, sy, 1.5 / zoom, 0, Math.PI * 2);
            ctx.fill();
            ctx.fillStyle = slideColor;
          }
        }

        if (previewDone) break;
      }
      ctx.globalAlpha = 1.0;
      ctx.restore();
    }

    // ── Popisek ──────────────────────────────────────────────────────────
    ctx.save();
    ctx.scale(1, -1);
    ctx.font = `bold ${10 / zoom}px sans-serif`;
    ctx.textAlign = "left";
    ctx.textBaseline = "top";
    const lbl = pos.is_prime ? "Odliv" : overrides[tidx.toString()]?.name || `Sklo ${tidx + 1}`;
    const lx = pos.x + 4 / zoom,
      ly = -(pos.y + pos.height) + 5 / zoom;
    const tw = ctx.measureText(lbl).width;
    ctx.fillStyle = "rgba(0,0,0,0.55)";
    ctx.fillRect(lx - 2 / zoom, ly - 1 / zoom, tw + 4 / zoom, 12 / zoom);
    ctx.fillStyle = isSelected ? "#93c5fd" : "#e2e8f0";
    ctx.fillText(lbl, lx, ly);
    ctx.restore();
  }

  return hasPreviewNozzle ? { x: previewNozzleWX, y: previewNozzleWY } : null;
}

/** Transformační úchopy vybrané trasy (čtverce = scale, kolečka = rotace)
 *  + přerušovaný obrys bboxu, resp. osa rotace. */
export function drawTransformHandles(
  ctx: CanvasRenderingContext2D,
  handles: { x: number; y: number }[],
  transformMode: "scale" | "rotate",
  hoverHandle: number,
  zoom: number,
  rotationCenter: { x: number; y: number } | null
) {
  if (handles.length !== 4) return;
  const r = 5 / zoom;

  if (transformMode === "rotate") {
    // Kolečka — zlatá barva
    for (let i = 0; i < 4; i++) {
      const h = handles[i],
        hov = i === hoverHandle;
      ctx.beginPath();
      ctx.arc(h.x, h.y, hov ? r * 1.4 : r, 0, Math.PI * 2);
      ctx.fillStyle = hov ? "#fde68a" : "#f59e0b";
      ctx.fill();
      ctx.strokeStyle = "#78350f";
      ctx.lineWidth = 1 / zoom;
      ctx.stroke();
    }
    // Osa rotace — tečkovaný kříž
    if (rotationCenter) {
      const { x: cx, y: cy } = rotationCenter;
      ctx.strokeStyle = "#f59e0b";
      ctx.lineWidth = 0.5 / zoom;
      ctx.setLineDash([2 / zoom, 2 / zoom]);
      ctx.beginPath();
      ctx.moveTo(cx - 4 / zoom, cy);
      ctx.lineTo(cx + 4 / zoom, cy);
      ctx.moveTo(cx, cy - 4 / zoom);
      ctx.lineTo(cx, cy + 4 / zoom);
      ctx.stroke();
      ctx.setLineDash([]);
    }
  } else {
    // Čtverce — modrá barva
    for (let i = 0; i < 4; i++) {
      const h = handles[i],
        hov = i === hoverHandle;
      const hr = hov ? r * 1.3 : r;
      ctx.fillStyle = hov ? "#93c5fd" : "#3b82f6";
      ctx.strokeStyle = "#1d4ed8";
      ctx.lineWidth = 1 / zoom;
      ctx.fillRect(h.x - hr, h.y - hr, hr * 2, hr * 2);
      ctx.strokeRect(h.x - hr, h.y - hr, hr * 2, hr * 2);
    }
    // Obrysy bbox trasy (přerušovaně)
    ctx.strokeStyle = "rgba(59,130,246,0.4)";
    ctx.lineWidth = 0.8 / zoom;
    ctx.setLineDash([3 / zoom, 2 / zoom]);
    ctx.beginPath();
    ctx.moveTo(handles[0].x, handles[0].y);
    ctx.lineTo(handles[1].x, handles[1].y);
    ctx.lineTo(handles[3].x, handles[3].y);
    ctx.lineTo(handles[2].x, handles[2].y);
    ctx.closePath();
    ctx.stroke();
    ctx.setLineDash([]);
  }
}

/** Měřidlo: čára mezi body, kotvy, snap indikátor a délkové popisky.
 *  Vrací celkovou délku a počet úseků pro obrazovkový overlay. */
export function drawMeasure(
  ctx: CanvasRenderingContext2D,
  measurePoints: Point2D[],
  cursorPoint: Point2D,
  isSnapped: boolean,
  snapColor: string,
  zoom: number
): { total: number; count: number } {
  const allPts = [...measurePoints, cursorPoint];

  ctx.strokeStyle = "#eab308";
  ctx.lineWidth = 1.5 / zoom;
  ctx.setLineDash([4 / zoom, 4 / zoom]);
  ctx.beginPath();
  for (let k = 0; k < allPts.length; k++) {
    if (k === 0) ctx.moveTo(allPts[k].x, allPts[k].y);
    else ctx.lineTo(allPts[k].x, allPts[k].y);
  }
  ctx.stroke();
  ctx.setLineDash([]);

  ctx.fillStyle = "#eab308";
  for (const p of measurePoints) {
    ctx.beginPath();
    ctx.arc(p.x, p.y, 3 / zoom, 0, Math.PI * 2);
    ctx.fill();
  }

  if (isSnapped) {
    ctx.strokeStyle = snapColor;
    ctx.lineWidth = 1 / zoom;
    const r2 = 5 / zoom;
    ctx.beginPath();
    ctx.arc(cursorPoint.x, cursorPoint.y, r2, 0, Math.PI * 2);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(cursorPoint.x - r2 * 1.5, cursorPoint.y);
    ctx.lineTo(cursorPoint.x + r2 * 1.5, cursorPoint.y);
    ctx.moveTo(cursorPoint.x, cursorPoint.y - r2 * 1.5);
    ctx.lineTo(cursorPoint.x, cursorPoint.y + r2 * 1.5);
    ctx.stroke();
  }

  ctx.save();
  ctx.scale(1, -1);
  ctx.font = `bold ${9 / zoom}px sans-serif`;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  let totalDist = 0;
  for (let k = 0; k < allPts.length - 1; k++) {
    const a = allPts[k],
      b = allPts[k + 1];
    const d = Math.hypot(b.x - a.x, b.y - a.y);
    totalDist += d;
    const mx = (a.x + b.x) / 2,
      my = -((a.y + b.y) / 2);
    const lbl = `${d.toFixed(1)} mm`;
    const tw = ctx.measureText(lbl).width;
    ctx.fillStyle = "rgba(0,0,0,0.7)";
    ctx.fillRect(mx - tw / 2 - 2 / zoom, my - 6 / zoom, tw + 4 / zoom, 10 / zoom);
    ctx.fillStyle = "#fde68a";
    ctx.fillText(lbl, mx, my);
  }
  ctx.restore();

  return { total: totalDist, count: allPts.length - 1 };
}

/** Reálná tryska (červená při tisku) a preview tryska (cyan kruh s křížem). */
export function drawNozzleMarkers(
  ctx: CanvasRenderingContext2D,
  currentNozzle: Point2D | null,
  previewNozzle: { x: number; y: number } | null,
  nozzleDiam: number,
  zoom: number
) {
  if (currentNozzle) {
    ctx.fillStyle = "#ef4444";
    ctx.beginPath();
    ctx.arc(currentNozzle.x, currentNozzle.y, nozzleDiam / 2 + 1 / zoom, 0, Math.PI * 2);
    ctx.fill();
  }

  if (previewNozzle && !currentNozzle) {
    const r = nozzleDiam / 2 + 1.5 / zoom;
    ctx.beginPath();
    ctx.arc(previewNozzle.x, previewNozzle.y, r, 0, Math.PI * 2);
    ctx.fillStyle = "rgba(34,211,238,0.25)";
    ctx.fill();
    ctx.strokeStyle = "#22d3ee";
    ctx.lineWidth = 1.2 / zoom;
    ctx.stroke();
    // Kříž v centru trysky
    ctx.beginPath();
    ctx.moveTo(previewNozzle.x - r * 0.7, previewNozzle.y);
    ctx.lineTo(previewNozzle.x + r * 0.7, previewNozzle.y);
    ctx.moveTo(previewNozzle.x, previewNozzle.y - r * 0.7);
    ctx.lineTo(previewNozzle.x, previewNozzle.y + r * 0.7);
    ctx.stroke();
  }
}

/** Popisky os v obrazovkových souřadnicích (vždy viditelné). Volat MIMO world transform. */
export function drawAxisLabels(
  ctx: CanvasRenderingContext2D,
  view: CameraView,
  bedMaxX: number,
  bedMaxY: number,
  gridMajor: number
) {
  const { width, height, panX, panY, zoom } = view;
  const minX2 = -panX / zoom,
    maxX2 = (width - panX) / zoom;
  const minY2 = (panY - height) / zoom,
    maxY2 = panY / zoom;
  const fmtVal = (v: number) => Number(v.toFixed(4)).toString();
  const FS = 13;
  ctx.font = `bold ${FS}px sans-serif`;

  // X-osa: pokud je spodní okraj bedu viditelný, ukotvit popisky těsně pod něj
  const bedBottomSY = panY; // world y=0 → screen y=panY
  const xAnchored = bedBottomSY > 20 && bedBottomSY < height - 50;
  const xAxisY = xAnchored ? bedBottomSY + 16 : height - 42;
  ctx.textAlign = "center";
  ctx.textBaseline = "bottom";
  for (let wx = Math.floor(minX2 / gridMajor) * gridMajor; wx < maxX2 + gridMajor; wx += gridMajor) {
    if (wx <= 0 || wx > bedMaxX) continue; // 0 se kreslí společně s Y osou
    const sx = wx * zoom + panX;
    if (sx < 30 || sx > width - 10) continue;
    ctx.fillStyle = "#94a3b8";
    ctx.fillText(fmtVal(wx), sx, xAxisY);
  }

  // Y-osa: pokud je levý okraj bedu viditelný, ukotvit popisky těsně vlevo od něj
  const bedLeftSX = panX; // world x=0 → screen x=panX
  const yAnchored = bedLeftSX > 30 && bedLeftSX < width - 10;
  ctx.textBaseline = "middle";
  ctx.textAlign = yAnchored ? "right" : "left";
  for (let wy = Math.floor(minY2 / gridMajor) * gridMajor; wy < maxY2 + gridMajor; wy += gridMajor) {
    if (wy <= 0 || wy > bedMaxY) continue; // 0 se kreslí jako společný roh
    const sy = panY - wy * zoom;
    if (sy < 10 || sy > height - 20) continue;
    ctx.fillStyle = "#94a3b8";
    ctx.fillText(fmtVal(wy), yAnchored ? bedLeftSX - 5 : 7, sy);
  }

  // Společná nula v rohu (0,0)
  const sx0 = panX;
  if (sx0 >= 30 && sx0 <= width - 10) {
    ctx.textAlign = yAnchored ? "right" : "left";
    ctx.textBaseline = "bottom";
    ctx.fillStyle = "#94a3b8";
    ctx.fillText("0", yAnchored ? bedLeftSX - 5 : 7, xAxisY);
  }

  // Reset kontextu textu — důležité, overlaye níže ho neresetují
  ctx.textAlign = "left";
  ctx.textBaseline = "alphabetic";
}

/** Obrazovkový info box zarovnaný k pravému okraji. První řádek lze obarvit
 *  jako nadpis. Vrací spodní hranu boxu (pro skládání boxů pod sebe). */
export function drawOverlayBox(
  ctx: CanvasRenderingContext2D,
  lines: string[],
  screenWidth: number,
  top: number,
  headerColor: string | null
): number {
  ctx.font = "bold 11px sans-serif";
  const lh = 17,
    pad = 8;
  const bw = Math.max(...lines.map((l) => ctx.measureText(l).width)) + pad * 2;
  const bh = lines.length * lh + pad;
  const bx = screenWidth - bw - 10;

  ctx.fillStyle = "rgba(0,0,0,0.65)";
  ctx.fillRect(bx, top, bw, bh);
  let start = 0;
  if (headerColor) {
    ctx.fillStyle = headerColor;
    ctx.fillText(lines[0], bx + pad, top + pad + 3);
    start = 1;
  }
  ctx.fillStyle = "#e2e8f0";
  for (let li = start; li < lines.length; li++)
    ctx.fillText(lines[li], bx + pad, top + pad + li * lh + 3);

  return top + bh;
}

/** Jemná snap mřížka zobrazená při držení CTRL. Volat MIMO world transform
 *  (funkce si transform nastaví sama). */
export function drawSnapGrid(ctx: CanvasRenderingContext2D, view: CameraView, gridStep: number) {
  const { width, height, panX, panY, zoom } = view;
  ctx.save();
  ctx.translate(panX, panY);
  ctx.scale(zoom, -zoom);
  ctx.strokeStyle = "rgba(56,189,248,0.18)";
  ctx.lineWidth = 0.25 / zoom;
  const minX = -panX / zoom,
    maxX = (width - panX) / zoom;
  const minY = (panY - height) / zoom,
    maxY = panY / zoom;
  ctx.beginPath();
  for (let x = Math.floor(minX / gridStep) * gridStep; x < maxX; x += gridStep) {
    ctx.moveTo(x, minY);
    ctx.lineTo(x, maxY);
  }
  for (let y = Math.floor(minY / gridStep) * gridStep; y < maxY; y += gridStep) {
    ctx.moveTo(minX, y);
    ctx.lineTo(maxX, y);
  }
  ctx.stroke();
  ctx.restore();
}

/** Náhled právě kresleného tvaru (obdélník/elipsa/čára) — čárkovaný cyan obrys
 *  s popiskem rozměru. Volat UVNITŘ world transformu. */
export function drawShapePreview(
  ctx: CanvasRenderingContext2D,
  pts: Point2D[],
  closed: boolean,
  label: string,
  zoom: number
) {
  if (pts.length < 2) return;

  ctx.beginPath();
  ctx.moveTo(pts[0].x, pts[0].y);
  for (let i = 1; i < pts.length; i++) ctx.lineTo(pts[i].x, pts[i].y);
  if (closed) {
    ctx.closePath();
    ctx.fillStyle = "rgba(34,211,238,0.10)";
    ctx.fill();
  }
  ctx.strokeStyle = "#22d3ee";
  ctx.lineWidth = 1.5 / zoom;
  ctx.setLineDash([4 / zoom, 4 / zoom]);
  ctx.stroke();
  ctx.setLineDash([]);

  // Popisek rozměru nad horním okrajem tvaru
  let mnX = Infinity,
    mxX = -Infinity,
    mxY = -Infinity;
  for (const p of pts) {
    if (p.x < mnX) mnX = p.x;
    if (p.x > mxX) mxX = p.x;
    if (p.y > mxY) mxY = p.y;
  }
  ctx.save();
  ctx.scale(1, -1);
  ctx.font = `bold ${9 / zoom}px sans-serif`;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  const lx = (mnX + mxX) / 2;
  const ly = -(mxY + 8 / zoom);
  const tw = ctx.measureText(label).width;
  ctx.fillStyle = "rgba(0,0,0,0.7)";
  ctx.fillRect(lx - tw / 2 - 2 / zoom, ly - 6 / zoom, tw + 4 / zoom, 10 / zoom);
  ctx.fillStyle = "#a5f3fc";
  ctx.fillText(label, lx, ly);
  ctx.restore();
}
