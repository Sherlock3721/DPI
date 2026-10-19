import type { LayoutPosition, SliceParams, SubstratePaths, Transform } from "./tauri";
import { wasm_world_aabb, wasm_clamp_gui_xy, wasm_clip_paths, wasm_process_paths } from "./dpiWasm";

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

/** Udrží trasu aspoň zčásti na skle (přesah mimo sklo se při tisku ořízne). */
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

/** Ořízne dráhy na sklo (Rust `clip_paths_to_slide`) — stejné dráhy jako G-kód,
 *  takže sedí indexy živého náhledu tisku. Dráha celá na skle se vrací beze změny. */
export function clipPathsToSlide(paths: SubstratePaths, t: Transform, pos: LayoutPosition): SubstratePaths {
  const raw = bboxOfPaths(paths);
  if (!raw) return paths;
  const w = computeWorldAABB(t.gui_dx, t.gui_dy, t.scale, t.rotation, t.cx, t.cy, raw);
  if (w.minX >= pos.x && w.maxX <= pos.x + pos.width && w.minY >= pos.y && w.maxY <= pos.y + pos.height) {
    return paths;
  }
  const json = wasm_clip_paths(
    JSON.stringify(paths),
    t.gui_dx, t.gui_dy, t.scale, t.rotation, t.cx, t.cy,
    pos.x, pos.y, pos.width, pos.height
  );
  return JSON.parse(json) as SubstratePaths;
}

/** Zpracuje zdrojové dráhy pro jedno sklíčko synchronně ve WASM (stejný Rust
 *  `process_substrate_paths` jako layout) — živý náhled během tažení v plátně.
 *  `rawJson` = JSON zdrojových drah (serializuje se jednou na začátku tažení). */
export function processPathsWasm(rawJson: string, sp: SliceParams): SubstratePaths {
  return JSON.parse(wasm_process_paths(rawJson, JSON.stringify(sp))) as SubstratePaths;
}

/** Obálka bodů drah v lokálních souřadnicích, null = žádné body. */
export function bboxOfPaths(paths: SubstratePaths): RawBbox | null {
  let mnX = Infinity,
    mxX = -Infinity,
    mnY = Infinity,
    mxY = -Infinity;
  for (const seg of paths.segments)
    for (const pt of seg.points) {
      if (pt.x < mnX) mnX = pt.x;
      if (pt.x > mxX) mxX = pt.x;
      if (pt.y < mnY) mnY = pt.y;
      if (pt.y > mxY) mxY = pt.y;
    }
  return isFinite(mnX) ? { mnX, mxX, mnY, mxY } : null;
}

/** Mapuje index pozice na index transformace (přeskakuje is_prime pozice). */
export function getTransformIdx(posIdx: number, positions: LayoutPosition[]): number {
  if (posIdx < 0 || posIdx >= positions.length || positions[posIdx].is_prime) return -1;
  let cnt = 0;
  for (let i = 0; i < posIdx; i++) if (!positions[i].is_prime) cnt++;
  return cnt;
}
