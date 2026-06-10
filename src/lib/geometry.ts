import type { LayoutPosition, Transform } from "./tauri";
import { wasm_world_aabb, wasm_clamp_gui_xy } from "./dpiWasm";

export interface RawBbox {
  mnX: number;
  mxX: number;
  mnY: number;
  mxY: number;
}

export function computeWorldAABB(
  gui_dx: number,
  gui_dy: number,
  scale: number,
  rotation: number,
  cx: number,
  cy: number,
  raw: RawBbox
): { minX: number; maxX: number; minY: number; maxY: number } {
  const r = wasm_world_aabb(gui_dx, gui_dy, scale, rotation, cx, cy, raw.mnX, raw.mxX, raw.mnY, raw.mxY);
  return { minX: r[0], maxX: r[1], minY: r[2], maxY: r[3] };
}

export function clampGuidXY(
  t: Transform,
  pos: LayoutPosition,
  raw: RawBbox,
  nozzleDiam: number = 0
): void {
  const r = wasm_clamp_gui_xy(
    t.gui_dx, t.gui_dy, t.scale, t.rotation, t.cx, t.cy,
    pos.x, pos.y, pos.width, pos.height,
    raw.mnX, raw.mxX, raw.mnY, raw.mxY,
    nozzleDiam
  );
  t.gui_dx = r[0];
  t.gui_dy = r[1];
}

/** Mapuje index pozice na index transformace (přeskakuje is_prime pozice). */
export function getTransformIdx(posIdx: number, positions: LayoutPosition[]): number {
  if (posIdx < 0 || posIdx >= positions.length || positions[posIdx].is_prime) return -1;
  let cnt = 0;
  for (let i = 0; i < posIdx; i++) if (!positions[i].is_prime) cnt++;
  return cnt;
}
