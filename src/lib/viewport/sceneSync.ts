/**
 * Kódování scény a snímků pro renderer (dpi-render) — zrcadlo
 * `dpi-render/src/protocol.rs`. Oba hostitelé (nativní i wasm) dostávají
 * stejné bajty.
 *
 * Geometrie drah se posílá jen jednou na objekt `SubstratePaths` (dráhy jsou
 * neměnné — změna = nový objekt); tažení trasy mění jen proměnné snímku.
 */
import type { PathSegment, SubstratePaths } from "../tauri";
import { isDotSeg } from "../pathMath";

const MAGIC = 0x52495044; // "DPIR"
const VERSION = 1;
export const PIECE_BYTES = 36;
export const DOT_BYTES = 16;
export const HEADER_BYTES = 208;
export const SLIDE_FRAME_BYTES = 64;
/** Nejvíc stylů objektů na sklo (dpi-render MAX_OBJECTS). */
const MAX_STYLES = 128;

export type RGB = [number, number, number];
export type RGBA = [number, number, number, number];

export interface ObjStyle {
  w: number;
  h: number;
  color: RGB;
}

export interface SlideScene {
  /** x, y, šířka, výška, tloušťka skla. */
  glass: [number, number, number, number, number];
  fill: RGBA;
  edge: RGBA;
  /** Tištěné dráhy (oříznuté na sklo) — kurzor náhledu indexuje tyto. */
  paths: SubstratePaths | null;
  /** Neoříznuté dráhy přesahující sklo (kreslí se slabě mimo sklo). */
  ghost: SubstratePaths | null;
  /** x0, y0, x1, y1 (svět). */
  clip: [number, number, number, number];
  /** Styl objektu podle id (null = dráha bez objektu). */
  style: (objectId: number | null) => ObjStyle;
}

export interface SceneInput {
  /** Deska: šířka, hloubka a tloušťka (vršek v z = 0, spodek v z = −thickness). */
  bed: { w: number; h: number; thickness: number };
  slides: SlideScene[];
  /** LineList [x, y, z] dvojic; stejný objekt = beze změny. */
  travels: Float32Array | null;
}

/** "#rrggbb" → RGB 0–1. */
export function hexToRgb(hex: string): RGB {
  const h = hex.replace("#", "");
  return [parseInt(h.slice(0, 2), 16) / 255, parseInt(h.slice(2, 4), 16) / 255, parseInt(h.slice(4, 6), 16) / 255];
}

/** Seřazená id objektů drah (dráhy bez objektu mají styl 0). */
function objectIdsOf(...pds: (SubstratePaths | null)[]): number[] {
  const ids = new Set<number>();
  for (const pd of pds) for (const s of pd?.segments ?? []) if (s.object != null) ids.add(s.object);
  return [...ids].sort((a, b) => a - b);
}

interface Built {
  pieces: Uint8Array;
  dots: Uint8Array;
  nSegs: number;
}

function buildBuffers(segments: PathSegment[], objIndex: Map<number, number>): Built {
  let nPieces = 0,
    nDots = 0;
  for (const s of segments) {
    const n = s.points.length;
    if (n === 0) continue;
    if (isDotSeg(s.points)) nDots++;
    else nPieces += n - 1;
  }
  const pieces = new ArrayBuffer(nPieces * PIECE_BYTES);
  const dots = new ArrayBuffer(nDots * DOT_BYTES);
  const pv = new DataView(pieces),
    dv = new DataView(dots);
  let po = 0,
    doff = 0;
  segments.forEach((s, si) => {
    const pts = s.points;
    const n = pts.length;
    if (n === 0) return;
    const obj = s.object != null ? (objIndex.get(s.object) ?? 0) : 0;
    if (isDotSeg(pts)) {
      dv.setFloat32(doff, pts[0].x, true);
      dv.setFloat32(doff + 4, pts[0].y, true);
      dv.setUint32(doff + 8, si, true);
      dv.setUint32(doff + 12, obj, true);
      doff += DOT_BYTES;
      return;
    }
    let cum = 0;
    for (let j = 0; j < n - 1; j++) {
      const a = pts[j],
        b = pts[j + 1];
      pv.setFloat32(po, a.x, true);
      pv.setFloat32(po + 4, a.y, true);
      pv.setFloat32(po + 8, b.x, true);
      pv.setFloat32(po + 12, b.y, true);
      pv.setUint32(po + 16, si, true);
      pv.setUint32(po + 20, j, true);
      pv.setUint32(po + 24, n - 1, true);
      pv.setUint32(po + 28, obj, true);
      pv.setFloat32(po + 32, cum, true);
      cum += Math.hypot(b.x - a.x, b.y - a.y);
      po += PIECE_BYTES;
    }
  });
  return { pieces: new Uint8Array(pieces), dots: new Uint8Array(dots), nSegs: segments.length };
}

export class SceneSync {
  /** Buffer drah podle objektu drah a mapování id objektů (klíč). */
  private cache = new WeakMap<SubstratePaths, { key: string; id: number }>();
  /** Buffery, které renderer drží. */
  private live = new Set<number>();
  private nextId = 1;
  private lastStructure = "";
  private lastTravels: Float32Array | null | undefined = undefined;
  private lastBed = "";

  /** Renderer začíná znovu (nový hostitel) — vše se pošle znovu. */
  reset() {
    this.cache = new WeakMap();
    this.live.clear();
    this.lastStructure = "";
    this.lastTravels = undefined;
    this.lastBed = "";
  }

  /** Paket se změnami scény, nebo null, když se nic nezměnilo. */
  packet(input: SceneInput): Uint8Array | null {
    const blobs: Uint8Array[] = [];
    let blobLen = 0;
    const addBlob = (b: Uint8Array) => {
      const offset = blobLen;
      blobs.push(b);
      blobLen += (b.byteLength + 3) & ~3;
      return { offset, len: b.byteLength };
    };

    const buffersAdd: unknown[] = [];
    const used = new Set<number>();
    const bufferFor = (pd: SubstratePaths | null, objIndex: Map<number, number>, key: string): number | null => {
      if (!pd || pd.segments.length === 0) return null;
      const hit = this.cache.get(pd);
      if (hit && hit.key === key && this.live.has(hit.id)) {
        used.add(hit.id);
        return hit.id;
      }
      const built = buildBuffers(pd.segments, objIndex);
      const id = this.nextId++;
      buffersAdd.push({ id, pieces: addBlob(built.pieces), dots: addBlob(built.dots), n_segs: built.nSegs });
      this.cache.set(pd, { key, id });
      this.live.add(id);
      used.add(id);
      return id;
    };

    const slides = input.slides.map((s) => {
      // Shodné styly objektů sdílí jeden záznam — renderer drží nejvýš
      // MAX_STYLES stylů na sklo, objektů může být mnohem víc (pole teček)
      const ids = objectIdsOf(s.paths, s.ghost);
      const objects: ObjStyle[] = [];
      const byKey = new Map<string, number>();
      const styleOf = (id: number | null) => {
        const st = s.style(id);
        const k = `${st.w},${st.h},${st.color.join(",")}`;
        let i = byKey.get(k);
        if (i === undefined) {
          i = Math.min(objects.length, MAX_STYLES - 1);
          if (objects.length < MAX_STYLES) objects.push(st);
          byKey.set(k, i);
        }
        return i;
      };
      styleOf(null);
      const styleIdx = new Map(ids.map((id) => [id, styleOf(id)]));
      const key = ids.map((id) => `${id}:${styleIdx.get(id)}`).join(",");
      return {
        glass: s.glass,
        glass_fill: s.fill,
        glass_edge: s.edge,
        paths: bufferFor(s.paths, styleIdx, key),
        ghost: bufferFor(s.ghost, styleIdx, key),
        clip: s.clip,
        objects,
      };
    });

    const freed = [...this.live].filter((id) => !used.has(id));
    for (const id of freed) this.live.delete(id);

    const structure = JSON.stringify(slides);
    const bed = `${input.bed.w},${input.bed.h},${input.bed.thickness}`;
    const travelsChanged = input.travels !== this.lastTravels;
    if (buffersAdd.length === 0 && freed.length === 0 && structure === this.lastStructure && bed === this.lastBed && !travelsChanged)
      return null;

    const packet: Record<string, unknown> = { buffers_add: buffersAdd, buffers_free: freed };
    if (bed !== this.lastBed) packet.bed = input.bed;
    if (structure !== this.lastStructure || buffersAdd.length > 0 || freed.length > 0) packet.slides = slides;
    if (travelsChanged) {
      const t = input.travels;
      packet.travels = t && t.length > 0 ? addBlob(new Uint8Array(t.buffer, t.byteOffset, t.byteLength)) : { offset: 0, len: 0 };
    }
    this.lastStructure = structure;
    this.lastBed = bed;
    this.lastTravels = input.travels;

    const json = new TextEncoder().encode(JSON.stringify(packet));
    const jsonPadded = (json.byteLength + 3) & ~3;
    const out = new Uint8Array(12 + jsonPadded + blobLen);
    const dv = new DataView(out.buffer);
    dv.setUint32(0, MAGIC, true);
    dv.setUint32(4, VERSION, true);
    dv.setUint32(8, json.byteLength, true);
    out.set(json, 12);
    let off = 12 + jsonPadded;
    for (const b of blobs) {
      out.set(b, off);
      off += (b.byteLength + 3) & ~3;
    }
    return out;
  }
}

export interface SlideFrame {
  /** tx, ty, cx, cy, sx, sy, cos θ, sin θ. */
  xf: [number, number, number, number, number, number, number, number];
  /** Režim (0 vše, 1 nic, 2 částečně), dráha, start_idx, tištěný úsek (−1 = přejezd). */
  cursor: [number, number, number, number];
  frac: number;
}

export interface FrameInput {
  viewProj: Float32Array;
  eye: [number, number, number];
  ortho: boolean;
  viewDir: [number, number, number];
  /** Velikost cíle v px. */
  width: number;
  height: number;
  /** px cíle na mm v targetu kamery. */
  pxPerMm: number;
  dpr: number;
  grid: [number, number, boolean];
  zExaggeration: number;
  orderColors: boolean;
  lodPx: number;
  dashMm: number;
  nozzleReal: [number, number, number] | null;
  nozzlePreview: [number, number, number] | null;
  clear: RGBA;
  showGhosts: boolean;
  showTravels: boolean;
  slides: SlideFrame[];
}

export function encodeFrame(f: FrameInput): Uint8Array {
  const out = new ArrayBuffer(HEADER_BYTES + f.slides.length * SLIDE_FRAME_BYTES);
  const fl = new Float32Array(out, 0, HEADER_BYTES / 4);
  const u = new Uint32Array(out, 0, HEADER_BYTES / 4);
  fl.set(f.viewProj, 0);
  fl.set([...f.eye, f.ortho ? 1 : 0], 16);
  fl.set([...f.viewDir, 0], 20);
  fl.set([f.width, f.height, f.pxPerMm, f.dpr], 24);
  fl.set([f.grid[0], f.grid[1], f.grid[2] ? 1 : 0, 0], 28);
  fl.set([f.zExaggeration, f.orderColors ? 1 : 0, f.lodPx, f.dashMm], 32);
  fl.set(f.nozzleReal ? [...f.nozzleReal, 1] : [0, 0, 0, 0], 36);
  fl.set(f.nozzlePreview ? [...f.nozzlePreview, 1] : [0, 0, 0, 0], 40);
  fl.set(f.clear, 44);
  u.set([f.showGhosts ? 1 : 0, f.showTravels ? 1 : 0, f.slides.length, 0], 48);
  f.slides.forEach((s, i) => {
    const base = HEADER_BYTES + i * SLIDE_FRAME_BYTES;
    new Float32Array(out, base, 8).set(s.xf);
    new Int32Array(out, base + 32, 4).set(s.cursor);
    new Float32Array(out, base + 48, 4).set([s.frac, 0, 0, 0]);
  });
  return new Uint8Array(out);
}
