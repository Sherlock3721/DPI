/**
 * Úprava jednotlivých objektů (posun, měřítko, rotace) — zrcadlí Rust
 * `ObjectTransform::apply` a mapování souřadnic sklíčka:
 *
 *   kresba (D) ──zapečené měřítko U kolem středu sklíčka──▶ lokální (L) ──Transform──▶ world (W)
 *
 * Úprava objektu žije v prostoru kresby (D), takže ji zapečené měřítko trasy
 * škáluje stejně jako zbytek kresby.
 */
import type { ObjectTransform, PathObject, Transform } from "./tauri";

type Pt = { x: number; y: number };

export const IDENTITY_XF: ObjectTransform = { dx: 0, dy: 0, scale: 1, rotation: 0 };

export function isIdentityXf(xf: ObjectTransform | null | undefined): boolean {
  if (!xf) return true;
  return (
    Math.abs(xf.dx) < 1e-9 &&
    Math.abs(xf.dy) < 1e-9 &&
    Math.abs(xf.scale - 1) < 1e-9 &&
    Math.abs((xf.scale_y ?? xf.scale) - 1) < 1e-9 &&
    Math.abs(xf.rotation) < 1e-9
  );
}

/** Bod `p` po úpravě objektu se středem obálky `pivot` (Rust `ObjectTransform::apply`). */
export function xfApply(xf: ObjectTransform, p: Pt, pivot: Pt): Pt {
  const vx = (p.x - pivot.x) * xf.scale;
  const vy = (p.y - pivot.y) * (xf.scale_y ?? xf.scale);
  const rad = (-xf.rotation * Math.PI) / 180;
  const c = Math.cos(rad),
    s = Math.sin(rad);
  return { x: pivot.x + xf.dx + vx * c - vy * s, y: pivot.y + xf.dy + vx * s + vy * c };
}

/** Inverze `xfApply`: bod upraveného objektu → bod neupraveného objektu. */
export function xfInverse(xf: ObjectTransform, p: Pt, pivot: Pt): Pt {
  const qx = p.x - pivot.x - xf.dx,
    qy = p.y - pivot.y - xf.dy;
  const rad = (xf.rotation * Math.PI) / 180;
  const c = Math.cos(rad),
    s = Math.sin(rad);
  return {
    x: pivot.x + (qx * c - qy * s) / xf.scale,
    y: pivot.y + (qx * s + qy * c) / (xf.scale_y ?? xf.scale),
  };
}

/** Střed obálky neupraveného objektu (střed úpravy) v prostoru kresby. */
export function objectPivot(o: PathObject): Pt | null {
  const b = o.base_bounds;
  return b ? { x: (b[0] + b[1]) / 2, y: (b[2] + b[3]) / 2 } : null;
}

/** Mapování souřadnic jednoho sklíčka. `us`/`usY` = zapečené měřítko trasy. */
export interface SlideFrame {
  t: Transform;
  us: number;
  usY: number;
}

/** Kresba (D) → lokální souřadnice trasy (L). */
export function designToLocal(f: SlideFrame, p: Pt): Pt {
  return { x: f.t.cx + (p.x - f.t.cx) * f.us, y: f.t.cy + (p.y - f.t.cy) * f.usY };
}

/** Lokální souřadnice trasy (L) → kresba (D). */
export function localToDesign(f: SlideFrame, p: Pt): Pt {
  return { x: f.t.cx + (p.x - f.t.cx) / f.us, y: f.t.cy + (p.y - f.t.cy) / f.usY };
}

/** Lokální (L) → world, shodné s `tpt` / Rust `transform_pt`. */
export function localToWorld(t: Transform, p: Pt): Pt {
  const dx = (p.x - t.cx) * t.scale;
  const dy = (p.y - t.cy) * (t.scale_y ?? t.scale);
  const rad = (-t.rotation * Math.PI) / 180;
  const c = Math.cos(rad),
    s = Math.sin(rad);
  return { x: t.gui_dx + t.cx + dx * c - dy * s, y: t.gui_dy + t.cy + dx * s + dy * c };
}

/** World → lokální (L), inverze `localToWorld`. */
export function worldToLocal(t: Transform, w: Pt): Pt {
  const dx = w.x - (t.gui_dx + t.cx);
  const dy = w.y - (t.gui_dy + t.cy);
  const rad = (t.rotation * Math.PI) / 180;
  const c = Math.cos(rad),
    s = Math.sin(rad);
  return {
    x: t.cx + (dx * c - dy * s) / t.scale,
    y: t.cy + (dx * s + dy * c) / (t.scale_y ?? t.scale),
  };
}

export const designToWorld = (f: SlideFrame, p: Pt): Pt => localToWorld(f.t, designToLocal(f, p));
export const worldToDesign = (f: SlideFrame, w: Pt): Pt => localToDesign(f, worldToLocal(f.t, w));

/** Rohy obálky neupraveného objektu (pořadí jako úchyty trasy: TL, TR, BL, BR)
 *  — v prostoru kresby, s okrajem `pad`. */
export function baseCorners(b: [number, number, number, number], pad = 0): Pt[] {
  return [
    { x: b[0] - pad, y: b[2] - pad },
    { x: b[1] + pad, y: b[2] - pad },
    { x: b[0] - pad, y: b[3] + pad },
    { x: b[1] + pad, y: b[3] + pad },
  ];
}

/** Zápis sdílených úprav do kořene SVG: "id:dx,dy,měřítko,měřítkoY,rotace;…". */
export function serializeObjectTransforms(map: Record<string, ObjectTransform>): string {
  const f = (v: number) => String(+v.toFixed(6));
  return Object.entries(map)
    .filter(([, xf]) => !isIdentityXf(xf))
    .sort(([a], [b]) => Number(a) - Number(b))
    .map(([id, xf]) => `${id}:${f(xf.dx)},${f(xf.dy)},${f(xf.scale)},${xf.scale_y != null ? f(xf.scale_y) : ""},${f(xf.rotation)}`)
    .join(";");
}

export function parseObjectTransforms(v: string | null): Record<string, ObjectTransform> {
  const out: Record<string, ObjectTransform> = {};
  for (const item of (v ?? "").split(";")) {
    const [id, vals] = item.split(":");
    if (!vals || !/^\d+$/.test(id.trim())) continue;
    const n = vals.split(",").map((x) => x.trim());
    if (n.length !== 5) continue;
    const num = (s: string) => (s === "" ? NaN : Number(s));
    const [dx, dy, scale, sy, rotation] = n.map(num);
    if (![dx, dy, scale, rotation].every(Number.isFinite)) continue;
    out[id.trim()] = { dx, dy, scale, rotation, ...(Number.isFinite(sy) ? { scale_y: sy } : {}) };
  }
  return out;
}
