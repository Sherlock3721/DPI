/**
 * HUD 3D náhledu — 2D canvas nad obrazem scény. Vše se kreslí v obrazovkových
 * px (CSS), body ve world se promítají kamerou, takže úchyty, pravítko, popisky
 * a náhledy mají stálou velikost v jakémkoli natočení i projekci.
 *
 * Úsečky v rovině se promítají po koncových bodech (projekce zachovává přímky);
 * body za kamerou polyline přeruší.
 */
import type { Point2D } from "../tauri";
import type { MergePreview } from "../nodeMerge";
import type { OrbitCamera } from "./camera";

type Pt = { x: number; y: number };

/** Polyline v rovině z do aktuální cesty kontextu. */
export function pathWorld(ctx: CanvasRenderingContext2D, cam: OrbitCamera, pts: Pt[], z: number, closed = false) {
  let pen = false;
  let all = true;
  for (const p of pts) {
    const s = cam.project(p.x, p.y, z);
    if (!s) {
      pen = false;
      all = false;
      continue;
    }
    if (pen) ctx.lineTo(s.x, s.y);
    else ctx.moveTo(s.x, s.y);
    pen = true;
  }
  if (closed && all && pts.length > 2) ctx.closePath();
}

/** Kružnice v rovině z (rozteč 48 bodů). */
function circleWorld(ctx: CanvasRenderingContext2D, cam: OrbitCamera, c: Pt, r: number, z: number) {
  const pts: Pt[] = [];
  for (let i = 0; i < 48; i++) {
    const t = (i / 48) * Math.PI * 2;
    pts.push({ x: c.x + r * Math.cos(t), y: c.y + r * Math.sin(t) });
  }
  pathWorld(ctx, cam, pts, z, true);
}

function label(ctx: CanvasRenderingContext2D, text: string, x: number, y: number, fg: string, font = "bold 11px sans-serif") {
  ctx.font = font;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  const tw = ctx.measureText(text).width;
  ctx.fillStyle = "rgba(0,0,0,0.7)";
  ctx.fillRect(x - tw / 2 - 3, y - 7, tw + 6, 14);
  ctx.fillStyle = fg;
  ctx.fillText(text, x, y);
}

/** Transformační úchopy (čtverce = měřítko, kolečka = rotace) + přerušovaný
 *  obrys rámu, resp. osa rotace. Úchyty: TL, TR, BL, BR. */
export function drawTransformHandles(
  ctx: CanvasRenderingContext2D,
  cam: OrbitCamera,
  handles: Pt[],
  z: number,
  transformMode: "scale" | "rotate",
  hoverHandle: number,
  rotationCenter: Pt | null
) {
  if (handles.length !== 4) return;
  const s = handles.map((h) => cam.project(h.x, h.y, z));
  const r = 5;
  ctx.save();
  if (transformMode === "rotate") {
    s.forEach((p, i) => {
      if (!p) return;
      const hov = i === hoverHandle;
      ctx.beginPath();
      ctx.arc(p.x, p.y, hov ? r * 1.4 : r, 0, Math.PI * 2);
      ctx.fillStyle = hov ? "#fde68a" : "#f59e0b";
      ctx.fill();
      ctx.strokeStyle = "#78350f";
      ctx.lineWidth = 1;
      ctx.stroke();
    });
    const c = rotationCenter ? cam.project(rotationCenter.x, rotationCenter.y, z) : null;
    if (c) {
      ctx.strokeStyle = "#f59e0b";
      ctx.lineWidth = 0.75;
      ctx.setLineDash([2, 2]);
      ctx.beginPath();
      ctx.moveTo(c.x - 4, c.y);
      ctx.lineTo(c.x + 4, c.y);
      ctx.moveTo(c.x, c.y - 4);
      ctx.lineTo(c.x, c.y + 4);
      ctx.stroke();
    }
  } else {
    ctx.strokeStyle = "rgba(59,130,246,0.4)";
    ctx.lineWidth = 0.8;
    ctx.setLineDash([3, 2]);
    ctx.beginPath();
    pathWorld(ctx, cam, [handles[0], handles[1], handles[3], handles[2]], z, true);
    ctx.stroke();
    ctx.setLineDash([]);
    s.forEach((p, i) => {
      if (!p) return;
      const hr = i === hoverHandle ? r * 1.3 : r;
      ctx.fillStyle = i === hoverHandle ? "#93c5fd" : "#3b82f6";
      ctx.strokeStyle = "#1d4ed8";
      ctx.lineWidth = 1;
      ctx.fillRect(p.x - hr, p.y - hr, hr * 2, hr * 2);
      ctx.strokeRect(p.x - hr, p.y - hr, hr * 2, hr * 2);
    });
  }
  ctx.restore();
}

/** Index úchytu do `tolPx` od bodu obrazovky, jinak −1. */
export function hitHandle(cam: OrbitCamera, handles: Pt[], z: number, sx: number, sy: number, tolPx = 7): number {
  for (let i = 0; i < handles.length; i++) {
    const p = cam.project(handles[i].x, handles[i].y, z);
    if (p && Math.hypot(p.x - sx, p.y - sy) < tolPx) return i;
  }
  return -1;
}

/** Měřidlo: čára mezi body, kotvy, snap indikátor a délkové popisky.
 *  Vrací celkovou délku a počet úseků pro obrazovkový overlay. */
export function drawMeasure(
  ctx: CanvasRenderingContext2D,
  cam: OrbitCamera,
  measurePoints: Point2D[],
  cursorPoint: Point2D,
  z: number,
  isSnapped: boolean,
  snapColor: string
): { total: number; count: number } {
  const allPts = [...measurePoints, cursorPoint];
  ctx.save();
  ctx.strokeStyle = "#eab308";
  ctx.lineWidth = 1.5;
  ctx.setLineDash([4, 4]);
  ctx.beginPath();
  pathWorld(ctx, cam, allPts, z);
  ctx.stroke();
  ctx.setLineDash([]);

  ctx.fillStyle = "#eab308";
  for (const p of measurePoints) {
    const s = cam.project(p.x, p.y, z);
    if (!s) continue;
    ctx.beginPath();
    ctx.arc(s.x, s.y, 3, 0, Math.PI * 2);
    ctx.fill();
  }

  const c = cam.project(cursorPoint.x, cursorPoint.y, z);
  if (isSnapped && c) {
    ctx.strokeStyle = snapColor;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.arc(c.x, c.y, 5, 0, Math.PI * 2);
    ctx.moveTo(c.x - 7.5, c.y);
    ctx.lineTo(c.x + 7.5, c.y);
    ctx.moveTo(c.x, c.y - 7.5);
    ctx.lineTo(c.x, c.y + 7.5);
    ctx.stroke();
  }

  let total = 0;
  for (let k = 0; k < allPts.length - 1; k++) {
    const a = allPts[k],
      b = allPts[k + 1];
    const d = Math.hypot(b.x - a.x, b.y - a.y);
    total += d;
    const m = cam.project((a.x + b.x) / 2, (a.y + b.y) / 2, z);
    if (m) label(ctx, `${d.toFixed(1)} mm`, m.x, m.y, "#fde68a", "bold 10px sans-serif");
  }
  ctx.restore();
  return { total, count: allPts.length - 1 };
}

/** Popisky os podél hran desky (X u hrany y = 0, Y u hrany x = 0). Popisek se
 *  odsune od hrany ven a drží se uvnitř výřezu (jako dřív u okraje obrazovky). */
export function drawAxisLabels(ctx: CanvasRenderingContext2D, cam: OrbitCamera, bedMaxX: number, bedMaxY: number, gridMajor: number) {
  const W = cam.width,
    H = cam.height;
  const fmt = (v: number) => Number(v.toFixed(4)).toString();
  ctx.save();
  ctx.font = "bold 13px sans-serif";
  ctx.fillStyle = "#94a3b8";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  const c0 = cam.project(bedMaxX / 2, bedMaxY / 2, 0);

  // Odsazení od hrany ven z desky (ve směru od středu desky) o `off` px
  const place = (wx: number, wy: number, off: number): Pt | null => {
    const p = cam.project(wx, wy, 0);
    if (!p || !c0) return null;
    // Normála hrany: X popisky odsunout ve směru −Y světa, Y popisky ve směru −X
    const q = wy === 0 ? cam.project(wx, wy - 1, 0) : cam.project(wx - 1, wy, 0);
    if (!q) return null;
    const dx = q.x - p.x,
      dy = q.y - p.y;
    const len = Math.hypot(dx, dy) || 1;
    return { x: p.x + (dx / len) * off, y: p.y + (dy / len) * off };
  };
  const clampIn = (p: Pt, mx: number, my: number): Pt => ({
    x: Math.max(mx, Math.min(W - mx, p.x)),
    y: Math.max(my, Math.min(H - 42, p.y)),
  });

  const steps = (max: number) => {
    const out: number[] = [];
    for (let v = gridMajor; v <= max + 1e-9; v += gridMajor) out.push(v);
    return out;
  };
  for (const wx of steps(bedMaxX)) {
    const p = place(wx, 0, 16);
    if (!p || p.x < 30 || p.x > W - 10) continue;
    const q = clampIn(p, 30, 12);
    ctx.fillText(fmt(wx), q.x, q.y);
  }
  for (const wy of steps(bedMaxY)) {
    const p = place(0, wy, 20);
    if (!p || p.y < 10 || p.y > H - 20) continue;
    const q = clampIn(p, 18, 10);
    ctx.fillText(fmt(wy), q.x, q.y);
  }
  const o = place(0, 0, 18);
  if (o) {
    const q = clampIn(o, 18, 12);
    ctx.fillText("0", q.x, q.y);
  }
  ctx.restore();
}

/** Pěkný krok os (1–2–5 × 10^n) pro cílovou rozteč v px. */
function niceStep(pxPerMm: number, targetPx: number): number {
  const raw = targetPx / pxPerMm;
  const mag = Math.pow(10, Math.floor(Math.log10(Math.max(raw, 1e-6))));
  const n = raw / mag;
  return (n < 2 ? 1 : n < 5 ? 2 : 5) * mag;
}

/** Osy bočního ortho pohledu (po ose X nebo Y): svislá osa Z u kraje desky
 *  s jemnými vodorovnými čarami (odečet výšky skel a drah) a vodorovná osa
 *  X / Y pod deskou. Z = 0 je vršek desky, na kterém stojí skla. */
export function drawSideAxes(ctx: CanvasRenderingContext2D, cam: OrbitCamera, bedMaxX: number, bedMaxY: number, bedThickness: number) {
  const W = cam.width,
    H = cam.height;
  const r = cam.right();
  // Vodorovná osa obrazovky = osa X (pohled zepředu/zezadu), nebo Y (z boku)
  const alongX = Math.abs(r[0]) > Math.abs(r[1]);
  const extent = alongX ? bedMaxX : bedMaxY;
  const at = (v: number, z: number) => (alongX ? cam.project(v, 0, z) : cam.project(0, v, z));
  const fmt = (v: number) => Number(v.toFixed(4)).toString();
  const px = cam.pxPerMm();

  const a = at(0, 0),
    b = at(extent, 0);
  if (!a || !b) return;
  // Osa Z u levého kraje desky na obrazovce (mimo výřez přichycená k okraji)
  const leftX = Math.min(a.x, b.x);
  const axisX = Math.max(44, Math.min(W - 12, leftX));
  const zBottom = at(0, -bedThickness)!.y;

  // Viditelný rozsah Z
  const zAt = (sy: number) => cam.target[2] + (H / 2 - sy) / px;
  const zLo = zAt(H),
    zHi = zAt(0);
  const zStep = niceStep(px, 40);

  ctx.save();
  ctx.font = "bold 12px sans-serif";
  ctx.textBaseline = "middle";
  ctx.textAlign = "right";
  // Vodorovné čáry výšky přes celý výřez + rysky a popisky
  for (let z = Math.ceil(zLo / zStep) * zStep; z <= zHi; z += zStep) {
    const y = at(0, z)!.y;
    if (y < 8 || y > H - 8) continue;
    const zero = Math.abs(z) < zStep * 1e-6;
    ctx.strokeStyle = zero ? "rgba(148,163,184,0.45)" : "rgba(148,163,184,0.14)";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(0, y);
    ctx.lineTo(W, y);
    ctx.stroke();
    ctx.strokeStyle = "#94a3b8";
    ctx.beginPath();
    ctx.moveTo(axisX - 6, y);
    ctx.lineTo(axisX, y);
    ctx.stroke();
    ctx.fillStyle = "#94a3b8";
    ctx.fillText(fmt(zero ? 0 : z), axisX - 9, y);
  }
  // Svislá osa a její název
  ctx.strokeStyle = "#94a3b8";
  ctx.beginPath();
  ctx.moveTo(axisX, 0);
  ctx.lineTo(axisX, H);
  ctx.stroke();
  ctx.textAlign = "left";
  ctx.fillStyle = "#60a5fa";
  ctx.fillText("Z [mm]", axisX + 6, 16);

  // Vodorovná osa X / Y pod deskou
  const hStep = niceStep(px, 70);
  const labelY = Math.min(H - 42, zBottom + 16);
  ctx.textAlign = "center";
  ctx.fillStyle = "#94a3b8";
  for (let v = 0; v <= extent + 1e-9; v += hStep) {
    const p = at(v, -bedThickness);
    if (!p || p.x < axisX + 14 || p.x > W - 10) continue;
    ctx.fillText(fmt(v), p.x, labelY);
  }
  ctx.fillStyle = alongX ? "#f87171" : "#4ade80";
  ctx.textAlign = "right";
  ctx.fillText(alongX ? "X [mm]" : "Y [mm]", W - 10, labelY + 16 > H - 30 ? labelY - 16 : labelY + 16);
  ctx.restore();
}

/** Obrazovkový info box zarovnaný k pravému okraji. První řádek lze obarvit
 *  jako nadpis. Vrací spodní hranu boxu (pro skládání boxů pod sebe). */
export function drawOverlayBox(ctx: CanvasRenderingContext2D, lines: string[], screenWidth: number, top: number, headerColor: string | null): number {
  ctx.save();
  ctx.font = "bold 11px sans-serif";
  ctx.textAlign = "left";
  ctx.textBaseline = "alphabetic";
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
  for (let li = start; li < lines.length; li++) ctx.fillText(lines[li], bx + pad, top + pad + li * lh + 3);
  ctx.restore();
  return top + bh;
}

/** Text v levém horním rohu (pravítko). */
export function drawCornerText(ctx: CanvasRenderingContext2D, text: string) {
  ctx.save();
  ctx.font = "bold 11px sans-serif";
  const tw = ctx.measureText(text).width;
  ctx.fillStyle = "rgba(0,0,0,0.7)";
  ctx.fillRect(10, 10, tw + 16, 22);
  ctx.fillStyle = "#fde68a";
  ctx.fillText(text, 18, 25);
  ctx.restore();
}

/** Jemná snap mřížka (Ctrl) v rovině z — jen viditelná část, rozumný počet čar. */
export function drawSnapGrid(ctx: CanvasRenderingContext2D, cam: OrbitCamera, gridStep: number, z: number, fallback: [number, number, number, number]) {
  let [x0, y0, x1, y1] = fallback;
  const corners = [
    [0, 0],
    [cam.width, 0],
    [0, cam.height],
    [cam.width, cam.height],
  ].map(([sx, sy]) => cam.screenToPlane(sx, sy, z, true));
  if (corners.every((c) => c)) {
    x0 = Math.min(...corners.map((c) => c!.x));
    x1 = Math.max(...corners.map((c) => c!.x));
    y0 = Math.min(...corners.map((c) => c!.y));
    y1 = Math.max(...corners.map((c) => c!.y));
  }
  if ((x1 - x0) / gridStep + (y1 - y0) / gridStep > 600) return;
  ctx.save();
  ctx.strokeStyle = "rgba(56,189,248,0.18)";
  ctx.lineWidth = 0.5;
  ctx.beginPath();
  for (let x = Math.floor(x0 / gridStep) * gridStep; x <= x1; x += gridStep)
    pathWorld(ctx, cam, [{ x, y: y0 }, { x, y: y1 }], z);
  for (let y = Math.floor(y0 / gridStep) * gridStep; y <= y1; y += gridStep)
    pathWorld(ctx, cam, [{ x: x0, y }, { x: x1, y }], z);
  ctx.stroke();
  ctx.restore();
}

/** Náhled právě kresleného tvaru — čárkovaný cyan obrys s popiskem rozměru. */
export function drawShapePreview(ctx: CanvasRenderingContext2D, cam: OrbitCamera, pts: Point2D[], z: number, closed: boolean, text: string) {
  if (pts.length < 2) return;
  ctx.save();
  ctx.beginPath();
  pathWorld(ctx, cam, pts, z, closed);
  if (closed) {
    ctx.fillStyle = "rgba(34,211,238,0.10)";
    ctx.fill();
  }
  ctx.strokeStyle = "#22d3ee";
  ctx.lineWidth = 1.5;
  ctx.setLineDash([4, 4]);
  ctx.stroke();
  ctx.setLineDash([]);
  // Popisek nad nejvýše položeným bodem na obrazovce
  let top: Pt | null = null;
  let mx = 0,
    n = 0;
  for (const p of pts) {
    const s = cam.project(p.x, p.y, z);
    if (!s) continue;
    if (!top || s.y < top.y) top = s;
    mx += s.x;
    n++;
  }
  if (top && n > 0) label(ctx, text, mx / n, top.y - 12, "#a5f3fc", "bold 10px sans-serif");
  ctx.restore();
}

/** Živý náhled sloučení uzlů (souřadnice ve world). */
export function drawMergePreview(ctx: CanvasRenderingContext2D, cam: OrbitCamera, preview: MergePreview, z: number) {
  ctx.save();
  if (preview.others.length > 0) {
    for (const [half, color] of [
      [1.9, "rgba(15,23,42,0.85)"],
      [1.0, "rgba(203,213,225,0.9)"],
    ] as [number, string][]) {
      ctx.fillStyle = color;
      ctx.beginPath();
      for (const p of preview.others) {
        const s = cam.project(p.x, p.y, z);
        if (s) ctx.rect(s.x - half, s.y - half, half * 2, half * 2);
      }
      ctx.fill();
    }
  }

  if (preview.runs.length > 0) {
    ctx.lineJoin = "round";
    ctx.lineCap = "round";
    ctx.beginPath();
    for (const run of preview.runs) pathWorld(ctx, cam, run.points, z);
    ctx.strokeStyle = "rgba(34,211,238,0.28)";
    ctx.lineWidth = 5;
    ctx.stroke();
    ctx.strokeStyle = "#22d3ee";
    ctx.lineWidth = 1.6;
    ctx.stroke();
  }

  for (const g of preview.groups) {
    const hot = g.index === preview.hovered;
    const t = g.target ? cam.project(g.target.x, g.target.y, z) : null;

    if (g.target && t) {
      ctx.strokeStyle = hot ? "#f5d0fe" : "rgba(217,70,239,0.5)";
      ctx.lineWidth = hot ? 1.4 : 0.9;
      ctx.setLineDash([3, 3]);
      ctx.beginPath();
      for (const n of g.nodes) {
        if (n.excluded) continue;
        const s = cam.project(n.x, n.y, z);
        if (!s) continue;
        ctx.moveTo(s.x, s.y);
        ctx.lineTo(t.x, t.y);
      }
      ctx.stroke();
      ctx.setLineDash([]);
    }

    if (hot && g.target && preview.radius > 0) {
      ctx.strokeStyle = "rgba(34,211,238,0.35)";
      ctx.lineWidth = 1;
      ctx.setLineDash([2, 3]);
      ctx.beginPath();
      circleWorld(ctx, cam, g.target, preview.radius, z);
      ctx.stroke();
      ctx.setLineDash([]);
    }

    for (const n of g.nodes) {
      const s = cam.project(n.x, n.y, z);
      if (!s) continue;
      const r = (n.anchor ? 3.6 : 2.4) * (hot ? 1.25 : 1);
      ctx.beginPath();
      ctx.arc(s.x, s.y, r, 0, Math.PI * 2);
      ctx.fillStyle = n.excluded ? "rgba(100,116,139,0.6)" : n.anchor ? "#fbbf24" : "#e879f9";
      ctx.fill();
      if (n.anchor || n.excluded) {
        ctx.strokeStyle = n.excluded ? "rgba(148,163,184,0.7)" : "#fef3c7";
        ctx.lineWidth = 1;
        ctx.stroke();
      }
    }

    if (t) {
      ctx.beginPath();
      ctx.arc(t.x, t.y, hot ? 5 : 3.4, 0, Math.PI * 2);
      ctx.fillStyle = "#22d3ee";
      ctx.fill();
      ctx.strokeStyle = "#0f172a";
      ctx.lineWidth = 1;
      ctx.stroke();
    }

    if (hot) {
      for (const n of g.nodes) {
        const s = cam.project(n.x, n.y, z);
        if (s) label(ctx, `${n.label}`, s.x, s.y - 9, n.excluded ? "#94a3b8" : n.anchor ? "#fde68a" : "#f5d0fe", "bold 9px sans-serif");
      }
    }
  }
  ctx.restore();
}

/** Šipky směru tisku a značky začátků drah. `runs` = dráhy ve world (v pořadí
 *  tisku): `whole` = vytištěná celá (šipky), `start` = první tištěný bod. */
export function drawPathMarkers(
  ctx: CanvasRenderingContext2D,
  cam: OrbitCamera,
  runs: { pts: Pt[]; whole: boolean }[],
  z: number,
  arrows: boolean,
  markers: boolean
) {
  const startArrows = new Path2D(),
    endArrows = new Path2D(),
    starts = new Path2D();
  const arrow = (path: Path2D, a: Pt, b: Pt, atEnd: boolean) => {
    const pa = cam.project(a.x, a.y, z),
      pb = cam.project(b.x, b.y, z);
    if (!pa || !pb) return;
    const len = Math.hypot(pb.x - pa.x, pb.y - pa.y);
    if (len < 1e-9) return;
    const ux = (pb.x - pa.x) / len,
      uy = (pb.y - pa.y) / len;
    const size = 7;
    const tip = atEnd ? pb : { x: pa.x + ux * size * 1.2, y: pa.y + uy * size * 1.2 };
    const bx = tip.x - ux * size,
      by = tip.y - uy * size;
    path.moveTo(tip.x, tip.y);
    path.lineTo(bx - uy * size * 0.55, by + ux * size * 0.55);
    path.lineTo(bx + uy * size * 0.55, by - ux * size * 0.55);
    path.closePath();
  };
  for (const r of runs) {
    const n = r.pts.length;
    if (n < 2) continue;
    if (arrows && r.whole) {
      arrow(startArrows, r.pts[0], r.pts[1], false);
      arrow(endArrows, r.pts[n - 2], r.pts[n - 1], true);
    }
    if (markers) {
      const s = cam.project(r.pts[0].x, r.pts[0].y, z);
      if (s) {
        starts.moveTo(s.x + 1.5, s.y);
        starts.arc(s.x, s.y, 1.5, 0, Math.PI * 2);
      }
    }
  }
  ctx.save();
  if (arrows) {
    ctx.strokeStyle = "#0f172a";
    ctx.lineWidth = 0.8;
    ctx.lineJoin = "miter";
    for (const [path, color] of [
      [startArrows, "#4ade80"],
      [endArrows, "#f87171"],
    ] as [Path2D, string][]) {
      ctx.fillStyle = color;
      ctx.fill(path);
      ctx.stroke(path);
    }
  }
  if (markers) {
    ctx.fillStyle = "#22d3ee";
    ctx.fill(starts);
  }
  ctx.restore();
}

/** Popisky skel u levého horního rohu skla. */
export function drawSlideLabels(ctx: CanvasRenderingContext2D, cam: OrbitCamera, labels: { text: string; x: number; y: number; z: number; selected: boolean }[]) {
  ctx.save();
  ctx.font = "bold 10px sans-serif";
  ctx.textAlign = "left";
  ctx.textBaseline = "top";
  for (const l of labels) {
    const p = cam.project(l.x, l.y, l.z);
    if (!p) continue;
    const x = p.x + 4,
      y = p.y + 5;
    const tw = ctx.measureText(l.text).width;
    ctx.fillStyle = "rgba(0,0,0,0.55)";
    ctx.fillRect(x - 2, y - 1, tw + 4, 12);
    ctx.fillStyle = l.selected ? "#93c5fd" : "#e2e8f0";
    ctx.fillText(l.text, x, y);
  }
  ctx.restore();
}

/** Přerušovaný rám (rozbalený objekt) — rohy v pořadí obvodu. */
export function drawDashedFrame(ctx: CanvasRenderingContext2D, cam: OrbitCamera, corners: Pt[], z: number, color: string) {
  ctx.save();
  ctx.strokeStyle = color;
  ctx.lineWidth = 1.5;
  ctx.setLineDash([4, 3]);
  ctx.beginPath();
  pathWorld(ctx, cam, corners, z, true);
  ctx.stroke();
  ctx.restore();
}

/** Značka bodu (magnet): kroužek o poloměru 5 px. */
export function drawRing(ctx: CanvasRenderingContext2D, cam: OrbitCamera, p: Pt, z: number, color: string) {
  const s = cam.project(p.x, p.y, z);
  if (!s) return;
  ctx.save();
  ctx.strokeStyle = color;
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  ctx.arc(s.x, s.y, 5, 0, Math.PI * 2);
  ctx.stroke();
  ctx.restore();
}

/** Křížek pivotu během otáčení. */
export function drawPivot(ctx: CanvasRenderingContext2D, cam: OrbitCamera) {
  const p = cam.project(cam.target[0], cam.target[1], cam.target[2]);
  if (!p) return;
  ctx.save();
  ctx.strokeStyle = "rgba(251,191,36,0.9)";
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  ctx.moveTo(p.x - 7, p.y);
  ctx.lineTo(p.x + 7, p.y);
  ctx.moveTo(p.x, p.y - 7);
  ctx.lineTo(p.x, p.y + 7);
  ctx.stroke();
  ctx.beginPath();
  ctx.arc(p.x, p.y, 3, 0, Math.PI * 2);
  ctx.stroke();
  ctx.restore();
}

// ─── Navigační gizmo ─────────────────────────────────────────────────────────
// Vpravo nahoře: osy X/Y/Z jako v Blenderu. Tažení po
// gizmu = orbit, klik na koncový bod osy = pohled ve směru osy (numpad 3/1/7).

export type GizmoAxis = "x" | "-x" | "y" | "-y" | "z";
export interface GizmoLayout {
  cx: number;
  cy: number;
  r: number;
}

/** Horní okraj prvků HUD vpravo nahoře (px). */
export const HUD_TOP = 12;

export function gizmoLayout(width: number): GizmoLayout {
  const r = 30;
  return { cx: width - 52, cy: HUD_TOP + r + 12, r };
}

/** Kde začínají info boxy vpravo nahoře — pod gizmem. */
export function overlayTop(width: number): number {
  const g = gizmoLayout(width);
  return g.cy + g.r + 12 + 8;
}

const GIZMO_AXES: { axis: GizmoAxis; dir: [number, number, number]; color: string; text: string }[] = [
  { axis: "x", dir: [1, 0, 0], color: "#f87171", text: "X" },
  { axis: "-x", dir: [-1, 0, 0], color: "#7f1d1d", text: "" },
  { axis: "y", dir: [0, 1, 0], color: "#4ade80", text: "Y" },
  { axis: "-y", dir: [0, -1, 0], color: "#14532d", text: "" },
  { axis: "z", dir: [0, 0, 1], color: "#60a5fa", text: "Z" },
];

function gizmoEnds(cam: OrbitCamera, g: GizmoLayout) {
  const r = cam.right(),
    u = cam.up(),
    f = cam.forward();
  return GIZMO_AXES
    .map((a) => {
      const d = a.dir;
      const sx = d[0] * r[0] + d[1] * r[1] + d[2] * r[2];
      const sy = d[0] * u[0] + d[1] * u[1] + d[2] * u[2];
      const depth = d[0] * f[0] + d[1] * f[1] + d[2] * f[2];
      return { ...a, x: g.cx + sx * g.r, y: g.cy - sy * g.r, depth };
    })
    .sort((a, b) => b.depth - a.depth); // vzdálenější první
}

export function drawGizmo(ctx: CanvasRenderingContext2D, cam: OrbitCamera, g: GizmoLayout, hover: GizmoAxis | "ring" | null) {
  ctx.save();
  ctx.beginPath();
  ctx.arc(g.cx, g.cy, g.r + 12, 0, Math.PI * 2);
  ctx.fillStyle = hover === "ring" ? "rgba(148,163,184,0.22)" : "rgba(15,23,42,0.45)";
  ctx.fill();
  for (const e of gizmoEnds(cam, g)) {
    ctx.strokeStyle = e.color;
    ctx.lineWidth = 2;
    if (e.text) {
      ctx.beginPath();
      ctx.moveTo(g.cx, g.cy);
      ctx.lineTo(e.x, e.y);
      ctx.stroke();
    }
    ctx.beginPath();
    ctx.arc(e.x, e.y, e.text ? 8 : 5, 0, Math.PI * 2);
    ctx.fillStyle = e.color;
    ctx.globalAlpha = hover === e.axis ? 1 : 0.85;
    ctx.fill();
    ctx.globalAlpha = 1;
    if (hover === e.axis) {
      ctx.strokeStyle = "#f8fafc";
      ctx.lineWidth = 1.5;
      ctx.stroke();
    }
    if (e.text) {
      ctx.fillStyle = "#0f172a";
      ctx.font = "bold 10px sans-serif";
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(e.text, e.x, e.y + 0.5);
    }
  }
  ctx.restore();
}

/** Co je pod kurzorem v gizmu: konec osy, plocha gizma (orbit), nebo nic. */
export function hitGizmo(cam: OrbitCamera, g: GizmoLayout, sx: number, sy: number): GizmoAxis | "ring" | null {
  if (Math.hypot(sx - g.cx, sy - g.cy) > g.r + 12) return null;
  const ends = gizmoEnds(cam, g).reverse(); // bližší mají přednost
  for (const e of ends) if (Math.hypot(sx - e.x, sy - e.y) <= (e.text ? 9 : 6)) return e.axis;
  return "ring";
}
