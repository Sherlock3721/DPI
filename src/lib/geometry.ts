import type { LayoutPosition, Transform } from "./tauri";

export interface RawBbox {
  mnX: number;
  mxX: number;
  mnY: number;
  mxY: number;
}

/**
 * Vypočítá world-space AABB transformované tiskové trasy.
 * Odpovídá transform_pt v Rustu (Canvas2D používá scale(zoom, -zoom)).
 */
export function computeWorldAABB(
  gui_dx: number,
  gui_dy: number,
  scale: number,
  rotation: number,
  cx: number,
  cy: number,
  raw: RawBbox
): { minX: number; maxX: number; minY: number; maxY: number } {
  const sMinX = cx + (raw.mnX - cx) * scale;
  const sMaxX = cx + (raw.mxX - cx) * scale;
  const sMinY = cy + (raw.mnY - cy) * scale;
  const sMaxY = cy + (raw.mxY - cy) * scale;
  const wcx = gui_dx + cx,
    wcy = gui_dy + cy;
  const rad = (-rotation * Math.PI) / 180;
  const cr = Math.cos(rad),
    sr = Math.sin(rad);
  let minX = Infinity,
    maxX = -Infinity,
    minY = Infinity,
    maxY = -Infinity;
  for (const [lx, ly] of [
    [gui_dx + sMinX, gui_dy + sMinY],
    [gui_dx + sMaxX, gui_dy + sMinY],
    [gui_dx + sMinX, gui_dy + sMaxY],
    [gui_dx + sMaxX, gui_dy + sMaxY],
  ] as [number, number][]) {
    const dx = lx - wcx,
      dy = ly - wcy;
    const wx = wcx + dx * cr - dy * sr;
    const wy = wcy + dx * sr + dy * cr;
    if (wx < minX) minX = wx;
    if (wx > maxX) maxX = wx;
    if (wy < minY) minY = wy;
    if (wy > maxY) maxY = wy;
  }
  return { minX, maxX, minY, maxY };
}

/** Posune gui_dx/gui_dy tak, aby trasa nepřesahovala okraj skla (s insetem o nozzleDiam/2). */
export function clampGuidXY(
  t: Transform,
  pos: LayoutPosition,
  raw: RawBbox,
  nozzleDiam: number = 0
): void {
  const r = nozzleDiam / 2;
  const a = computeWorldAABB(t.gui_dx, t.gui_dy, t.scale, t.rotation, t.cx, t.cy, raw);
  if (a.minX < pos.x + r) t.gui_dx += pos.x + r - a.minX;
  if (a.maxX > pos.x + pos.width - r) t.gui_dx -= a.maxX - (pos.x + pos.width - r);
  if (a.minY < pos.y + r) t.gui_dy += pos.y + r - a.minY;
  if (a.maxY > pos.y + pos.height - r) t.gui_dy -= a.maxY - (pos.y + pos.height - r);
}

/** Mapuje index pozice na index transformace (přeskakuje is_prime pozice). */
export function getTransformIdx(posIdx: number, positions: LayoutPosition[]): number {
  if (posIdx < 0 || posIdx >= positions.length || positions[posIdx].is_prime) return -1;
  let cnt = 0;
  for (let i = 0; i < posIdx; i++) if (!positions[i].is_prime) cnt++;
  return cnt;
}
