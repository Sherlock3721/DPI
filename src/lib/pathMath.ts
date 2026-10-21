/**
 * Čistá geometrie drah bez kreslení: transformace trasy do world, pořadí tisku
 * bodů a kurzor náhledu tisku. Používá ji plátno (picking, HUD), kódování
 * scény pro renderer i náhled slučování uzlů.
 */
import type { LayoutPosition, Point2D, PreviewSegData, SubstratePaths, Transform } from "./tauri";
import { getTransformIdx } from "./geometry";

/** Transformuje bod z path-lokálního prostoru do world space.
 *  Odpovídá transform_pt v Rustu. */
export function tpt(
  px: number,
  py: number,
  pX: number,
  pY: number,
  pScale: number,
  pRot: number,
  pCX: number,
  pCY: number,
  /** Měřítko osy Y (odemčený poměr stran při tažení); chybí = pScale. */
  pScaleY: number = pScale
): [number, number] {
  const dx = (px - pCX) * pScale;
  const dy = (py - pCY) * pScaleY;
  const rad = (-pRot * Math.PI) / 180;
  const cr = Math.cos(rad),
    sr = Math.sin(rad);
  return [pX + pCX + dx * cr - dy * sr, pY + pCY + dx * sr + dy * cr];
}

/** tpt pro jednu trasu s předpočítanou rotací — pro průchod mnoha bodů. */
export function tptFn(
  pX: number,
  pY: number,
  pScale: number,
  pRot: number,
  pCX: number,
  pCY: number,
  pScaleY: number = pScale
): (px: number, py: number) => [number, number] {
  const rad = (-pRot * Math.PI) / 180;
  const cr = Math.cos(rad),
    sr = Math.sin(rad);
  const ox = pX + pCX,
    oy = pY + pCY;
  return (px, py) => {
    const dx = (px - pCX) * pScale;
    const dy = (py - pCY) * pScaleY;
    return [ox + dx * cr - dy * sr, oy + dx * sr + dy * cr];
  };
}

/** Parametry transformace trasy pozice — u odplivu identita kolem středu skla. */
export interface PathFrame {
  x: number;
  y: number;
  scale: number;
  scaleY: number;
  rot: number;
  cx: number;
  cy: number;
}

export function pathFrameOf(pos: LayoutPosition, t: Transform | undefined): PathFrame {
  if (pos.is_prime || !t) {
    return { x: pos.x, y: pos.y, scale: 1, scaleY: 1, rot: 0, cx: pos.width / 2, cy: pos.height / 2 };
  }
  return { x: t.gui_dx, y: t.gui_dy, scale: t.scale, scaleY: t.scale_y ?? t.scale, rot: t.rotation, cx: t.cx, cy: t.cy };
}

export function frameFn(f: PathFrame): (px: number, py: number) => [number, number] {
  return tptFn(f.x, f.y, f.scale, f.rot, f.cx, f.cy, f.scaleY);
}

export interface PreviewCursor {
  slideIdx: number;
  segIdx: number;
  /** Úsek v pořadí tisku: tryska jede od bodu ptIdx k ptIdx + 1. */
  ptIdx: number;
  fracT: number;
  /** Původní bod, kterým tisk dráhy začal (uzavřené dráhy začínají u trysky). */
  startIdx: number;
  /** Tryska teprve přejíždí na začátek dráhy: odkud (world) a jak daleko (0–1). */
  travel?: { from: Point2D; frac: number };
}

/** Bod dráhy v pořadí tisku — uzavřená dráha se tiskne od bodu startIdx. */
export function printedPoint(points: Point2D[], startIdx: number, k: number): Point2D {
  if (startIdx === 0) return points[k];
  const n = points.length - 1; // poslední bod uzavřené dráhy = první
  return points[(startIdx + k) % n];
}

/** Mapuje printProgress (0–100 % času tisku) na místo v trase podle časové osy
 *  (rychlosti objektů, přejezdy a tečky jako v G-kódu). */
export function computePreviewCursor(
  printProgress: number,
  precomputedSegs: PreviewSegData[],
  totalPreviewTime: number
): PreviewCursor | null {
  if (printProgress >= 100 || precomputedSegs.length === 0 || totalPreviewTime <= 0) return null;
  const t = totalPreviewTime * (printProgress / 100);
  for (let ci = 0; ci < precomputedSegs.length; ci++) {
    const sd = precomputedSegs[ci];
    const base = { slideIdx: sd.slide_idx, segIdx: sd.seg_idx, startIdx: sd.start_idx };
    const travelStart = ci > 0 ? endTime(precomputedSegs[ci - 1]) : 0;
    if (t < sd.start_time) {
      // Přejezd na začátek dráhy — nic z ní ještě není vytištěno
      const span = sd.start_time - travelStart;
      const frac = span > 1e-9 ? Math.min(1, Math.max(0, (t - travelStart) / span)) : 1;
      return { ...base, ptIdx: 0, fracT: 0, travel: { from: sd.travel_from, frac } };
    }
    if (t <= endTime(sd) || ci === precomputedSegs.length - 1) {
      const tt = t - sd.start_time;
      const times = sd.point_times;
      let ptIdx = Math.max(0, times.length - 2);
      for (let j = 1; j < times.length; j++) {
        if (times[j] >= tt) {
          ptIdx = j - 1;
          break;
        }
      }
      const interval = (times[ptIdx + 1] ?? times[ptIdx]) - times[ptIdx];
      const fracT = interval > 1e-9 ? Math.min(1, Math.max(0, (tt - times[ptIdx]) / interval)) : 1;
      return { ...base, ptIdx, fracT };
    }
  }
  return null;
}

function endTime(sd: PreviewSegData): number {
  return sd.start_time + (sd.point_times[sd.point_times.length - 1] ?? 0);
}

/** World pozice náhledové trysky pro kurzor (konec vytištěné části), jinak null. */
export function previewNozzleWorld(
  cursor: PreviewCursor | null,
  positions: LayoutPosition[],
  paths: SubstratePaths[],
  primePath: SubstratePaths | null,
  transforms: Transform[]
): Point2D | null {
  if (!cursor) return null;
  const pos = positions[cursor.slideIdx];
  if (!pos) return null;
  const ti = getTransformIdx(cursor.slideIdx, positions);
  const pd = pos.is_prime ? primePath : paths[ti];
  const pts = pd?.segments[cursor.segIdx]?.points;
  if (!pts || pts.length === 0) return null;
  const toWorld = frameFn(pathFrameOf(pos, transforms[ti]));
  const startIdx = cursor.startIdx < pts.length ? cursor.startIdx : 0;
  const pt = (k: number) => printedPoint(pts, startIdx, k);
  if (cursor.travel) {
    const [sx, sy] = toWorld(pt(0).x, pt(0).y);
    const f = cursor.travel.frac;
    return { x: cursor.travel.from.x + (sx - cursor.travel.from.x) * f, y: cursor.travel.from.y + (sy - cursor.travel.from.y) * f };
  }
  const toRender = Math.min(cursor.ptIdx + 1, pts.length);
  if (cursor.fracT > 0 && cursor.ptIdx + 1 < pts.length) {
    const p0 = pt(cursor.ptIdx);
    const p1 = pt(cursor.ptIdx + 1);
    const [x, y] = toWorld(p0.x + (p1.x - p0.x) * cursor.fracT, p0.y + (p1.y - p0.y) * cursor.fracT);
    return { x, y };
  }
  if (toRender > 0) {
    const lp = pt(toRender - 1);
    const [x, y] = toWorld(lp.x, lp.y);
    return { x, y };
  }
  return null;
}

export function isDotPair(a: Point2D, b: Point2D): boolean {
  return Math.abs(a.x - b.x) < 0.01 && Math.abs(a.y - b.y) < 0.01;
}

/** Tečka = dráha z jediného bodu, nebo ze dvou (téměř) shodných. */
export function isDotSeg(pts: Point2D[]): boolean {
  return pts.length === 1 || (pts.length === 2 && isDotPair(pts[0], pts[1]));
}
