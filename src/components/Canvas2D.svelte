<script lang="ts">
  import { run } from 'svelte/legacy';

  import { onMount, createEventDispatcher } from "svelte";
  import type { LayoutPosition, SubstratePaths, Transform, Point2D, SlideOverride, PreviewSegData, PrintCursor, ObjectTransform, PathObject } from "../lib/tauri";
  import { compute_preview_segments, motionFromSettings } from "../lib/tauri";
  import { computeWorldAABB, clampGuidXY, clipPathsToSlide, getTransformIdx, processPathsWasm, type RawBbox } from "../lib/geometry";
  import {
    tpt,
    computePreviewCursor,
    drawBedAndGrid,
    drawSlidesAndPaths,
    drawTransformHandles,
    drawMeasure,
    drawNozzleMarkers,
    drawAxisLabels,
    drawOverlayBox,
    drawSnapGrid,
    drawShapePreview,
    drawMergePreview,
  } from "../lib/canvas2dDraw";
  import type { MergePreview } from "../lib/nodeMerge";
  import { projectStore, sliceParamsFor, type DrawnShape, type LineAnchor } from "../stores/projectStore";
  import { settingsStore } from "../stores/settingsStore";
  import {
    IDENTITY_XF,
    baseCorners,
    designToWorld,
    objectPivot,
    worldToDesign,
    xfApply,
    xfInverse,
    type SlideFrame,
  } from "../lib/objectTransform";

  const dispatch = createEventDispatcher();

  interface Props {
    bedMaxX?: number;
    bedMaxY?: number;
    positions?: LayoutPosition[];
    paths?: SubstratePaths[];
    primePath?: SubstratePaths | null;
    transforms?: Transform[];
    overrides?: Record<string, SlideOverride>;
    currentNozzle?: Point2D | null;
    nozzleDiam?: number;
    selectedIndex?: any;
    showAxes?: boolean;
    isMeasuring?: boolean;
    measurePoints?: { x: number; y: number }[];
    printProgress?: number;
    extrusionRateUl?: number; // µl/mm, 0 = neznámý (skryj objem)
    drawTool?: "rect" | "ellipse" | "line" | null;
    /** Odhad skutečně tištěného místa při tisku (z tiskárny) — má přednost před printProgress. */
    printCursor?: PrintCursor | null;
    /** Výstup: délka časové osy náhledu (s) pro přehrávání v reálném čase. */
    previewTotalTime?: number;
    /** Zvýrazněný objekt (rozbalený v lokálním panelu): index sklíčka a id objektu. */
    focusedObject?: { slide: number; id: number } | null;
    /** Magnet: body kreslených tvarů se přichytávají k trasám. */
    snapToPaths?: boolean;
    /** Úprava objektu platí jen pro jeho substrát (živý náhled jen tam). */
    objEditOnlyHere?: boolean;
    /** Živý náhled sloučení uzlů. */
    mergePreview?: MergePreview | null;
  }

  let {
    bedMaxX = 250.0,
    bedMaxY = 210.0,
    positions = [],
    paths = [],
    primePath = null,
    transforms = [],
    overrides = {},
    currentNozzle = null,
    nozzleDiam = 0.4,
    selectedIndex = -1,
    showAxes = true,
    isMeasuring = false,
    measurePoints = $bindable([]),
    printProgress = 100,
    extrusionRateUl = 0,
    drawTool = null,
    printCursor = null,
    previewTotalTime = $bindable(0),
    focusedObject = null,
    snapToPaths = false,
    objEditOnlyHere = false,
    mergePreview = null
  }: Props = $props();

  let canvas: HTMLCanvasElement = $state()!;
  let ctx: CanvasRenderingContext2D | null = $state(null);

  // Camera
  let panX = 0;
  let panY = 0;
  let zoom = 1.0;

  // Interaction
  type DragOp = "pan" | "move" | "scale" | "rotate" | "objMove" | "objScale" | "objRotate" | null;
  let dragOp: DragOp = $state(null);
  /** 'scale' = corner squares, 'rotate' = corner circles */
  let transformMode: "scale" | "rotate" = $state("scale");
  let activeHandle = -1; // 0=TL 1=TR 2=BL 3=BR
  let hoverHandle = $state(-1);
  let dragSlideIdx = -1; // independent of selectedIndex prop lag
  let dragStartWorld = { x: 0, y: 0 };
  let dragStartTransform: Transform | null = null;
  let dragStartTransformsAll: Map<number, Transform> = new Map();
  // Indexy transformů mutovaných během dragu — store se aktualizuje až na mouseup
  let _dragModifiedIndices: Set<number> = new Set();

  // Pan
  let lastMouseScreenX = 0;
  let lastMouseScreenY = 0;

  // Double-click
  let lastClickTime = 0;
  let lastClickIndex = -1;

  // Úprava vybraného objektu (posun, měřítko, rotace) — během tažení jen náhled,
  // do store se zapíše po puštění. `pendingObj` drží náhled, dokud nedorazí
  // přepočítané dráhy (jinak by objekt na okamžik skočil zpět).
  type ObjEdit = { ti: number; id: number; pivot: Pt; start: ObjectTransform; cur: ObjectTransform };
  let objDrag: (ObjEdit & { startD: Pt; frame: SlideFrame; base: [number, number, number, number] }) | null = null;
  let pendingObj: (ObjEdit & { src: SubstratePaths | undefined }) | null = null;
  let _pendingTimer: ReturnType<typeof setTimeout> | undefined;
  type Pt = { x: number; y: number };

  /** Klik na již vybranou trasu: pokud se myš nepohne, vybere se objekt
   *  pod kurzorem (první klik vybírá trasu, druhý objekt). */
  let clickCandidate: { sx: number; sy: number; posIdx: number } | null = null;
  const CLICK_TOLERANCE_PX = 4;

  let mouseX = 0;
  let mouseY = 0;
  let ctrlDown = false;
  let altDown = false;
  let shiftDown = false;

  let width = 0;
  let height = 0;

  // rAF throttling — draw() se volá max. 1× za snímek
  let rafPending = false;
  function scheduleDraw() {
    if (!rafPending) {
      rafPending = true;
      requestAnimationFrame(() => { rafPending = false; draw(); });
    }
  }

  // Cache bbox pro aktuální drag — path se při dragu nemění
  let _bboxDragCache: Map<number, RawBbox | null> = new Map();

  run(() => {
    if (selectedIndex < 0) transformMode = "scale";
  });

  export function resetCamera() {
    if (width === 0 || height === 0) return;
    const margin = 40;
    zoom = Math.min((width - margin * 2) / bedMaxX, (height - margin * 2) / bedMaxY);
    panX = (width - bedMaxX * zoom) / 2;
    panY = (height + bedMaxY * zoom) / 2;
    draw();
  }

  export function centerOnSlide() {
    if (selectedIndex < 0 || selectedIndex >= positions.length || width === 0 || height === 0) return;
    const pos = positions[selectedIndex];
    const margin = 30;
    zoom = Math.min((width - margin * 2) / pos.width, (height - margin * 2) / pos.height);
    panX = width / 2 - (pos.x + pos.width / 2) * zoom;
    panY = height / 2 + (pos.y + pos.height / 2) * zoom;
    draw();
  }

  function screenToWorld(sx: number, sy: number) {
    return { x: (sx - panX) / zoom, y: (panY - sy) / zoom };
  }
  function worldToScreen(wx: number, wy: number) {
    return { x: wx * zoom + panX, y: panY - wy * zoom };
  }

  // ─── Snap ─────────────────────────────────────────────────────────────────
  function gridSnapSize(): number {
    if (zoom > 60) return 0.1;
    if (zoom > 30) return 0.5;
    if (zoom > 15) return 1;
    if (zoom > 7) return 2;
    if (zoom > 3) return 5;
    return 10;
  }
  function gridDrawStep(): { minor: number; major: number } {
    const worldPerTarget = 55 / zoom;
    const mag = Math.pow(10, Math.floor(Math.log10(Math.max(worldPerTarget, 0.001))));
    const norm = worldPerTarget / mag;
    const minor = norm < 2 ? mag : norm < 5 ? 2 * mag : 5 * mag;
    return { minor, major: minor * 5 };
  }
  function snapToGrid(v: number): number {
    const g = gridSnapSize();
    return Math.round(v / g) * g;
  }

  // ─── Kreslení tvarů ───────────────────────────────────────────────────────
  // Modifikátory během tažení: CTRL = snap po mřížce, SHIFT = 1:1 (čtverec/kruh,
  // u čáry úhel po 45°), ALT = kreslení od středu.
  let shapeStart: { x: number; y: number } | null = null;
  let shapeCurr = { x: 0, y: 0 };
  const MIN_SHAPE_MM = 0.1;

  function drawPointFromMouse(sx: number, sy: number): { x: number; y: number } {
    const w = screenToWorld(sx, sy);
    snapHit = snapToPaths ? snapToTrasa(w) : null;
    if (snapHit) return { x: snapHit.x, y: snapHit.y };
    return ctrlDown ? { x: snapToGrid(w.x), y: snapToGrid(w.y) } : w;
  }

  // ─── Magnet: přichytávání k trase (vrcholy mají přednost před úsečkami) ───
  const SNAP_PX = 8;
  /** Bod, ke kterému se kurzor právě přichytil (zobrazí se značka), a objekt
   *  jeho trasy (konec čáry se ho pak drží). */
  type SnapHit = { x: number; y: number; anchor: LineAnchor | null };
  let snapHit: SnapHit | null = null;
  /** Přichycení začátku a aktuálního konce právě kreslené čáry. */
  let shapeStartSnap: SnapHit | null = null;
  let shapeCurrSnap: SnapHit | null = null;

  function snapToTrasa(w: { x: number; y: number }): SnapHit | null {
    const r = SNAP_PX / zoom;
    type Cand = { x: number; y: number; ti: number; obj: number | null };
    let vertex: Cand | null = null,
      vertexD = r;
    let edge: Cand | null = null,
      edgeD = r;
    for (let i = 0; i < positions.length; i++) {
      if (positions[i].is_prime) continue;
      const ti = getTransformIdx(i, positions);
      const t = transforms[ti];
      const pd = paths[ti];
      const raw = getPathBboxRaw(i);
      if (!t || !pd || !raw) continue;
      const bb = computeWorldAABB(t.gui_dx, t.gui_dy, Math.max(t.scale, t.scale_y ?? t.scale), t.rotation, t.cx, t.cy, raw);
      if (w.x < bb.minX - r || w.x > bb.maxX + r || w.y < bb.minY - r || w.y > bb.maxY + r) continue;
      for (const seg of pd.segments) {
        let prev: [number, number] | null = null;
        for (const pt of seg.points) {
          const cur = tpt(pt.x, pt.y, t.gui_dx, t.gui_dy, t.scale, t.rotation, t.cx, t.cy, t.scale_y);
          const dv = Math.hypot(cur[0] - w.x, cur[1] - w.y);
          if (dv < vertexD) {
            vertexD = dv;
            vertex = { x: cur[0], y: cur[1], ti, obj: seg.object ?? null };
          }
          if (prev) {
            const [ax, ay] = prev;
            const dx = cur[0] - ax,
              dy = cur[1] - ay;
            const len2 = dx * dx + dy * dy;
            const k = len2 > 1e-12 ? Math.max(0, Math.min(1, ((w.x - ax) * dx + (w.y - ay) * dy) / len2)) : 0;
            const q = { x: ax + dx * k, y: ay + dy * k };
            const de = Math.hypot(q.x - w.x, q.y - w.y);
            if (de < edgeD) {
              edgeD = de;
              edge = { ...q, ti, obj: seg.object ?? null };
            }
          }
          prev = cur;
        }
      }
    }
    const hit = vertex ?? edge;
    if (!hit) return null;
    const id = snapObject(hit, hit.ti, hit.obj);
    return { x: hit.x, y: hit.y, anchor: id != null ? { slide: hit.ti, id } : null };
  }

  /** Objekt, kterému patří bod trasy. Sloučené objekty se tisknou dráhou pod id
   *  hlavního objektu — upřesní se podle nejmenší obálky obsahující bod. */
  function snapObject(p: Pt, ti: number, segObj: number | null): number | null {
    const t = transforms[ti];
    if (segObj == null || !t) return segObj;
    const l = worldToLocal(p.x, p.y, t);
    const tol = nozzleDiam;
    let best = segObj,
      bestArea = Infinity;
    for (const o of paths[ti]?.objects ?? []) {
      if (o.id !== segObj && o.merged_into !== segObj) continue;
      const [x0, x1, y0, y1] = o.bounds;
      if (l.x < x0 - tol || l.x > x1 + tol || l.y < y0 - tol || l.y > y1 + tol) continue;
      const area = (x1 - x0 + tol) * (y1 - y0 + tol);
      if (area < bestArea) {
        bestArea = area;
        best = o.id;
      }
    }
    return best;
  }

  /** Dva protilehlé rohy obdélníku/elipsy po aplikaci SHIFT (1:1) a ALT (od středu). */
  function shapeCorners(): { a: { x: number; y: number }; b: { x: number; y: number } } {
    const s = shapeStart!;
    let dx = shapeCurr.x - s.x,
      dy = shapeCurr.y - s.y;
    if (shiftDown) {
      const m = Math.max(Math.abs(dx), Math.abs(dy));
      dx = (dx < 0 ? -1 : 1) * m;
      dy = (dy < 0 ? -1 : 1) * m;
    }
    if (altDown) return { a: { x: s.x - dx, y: s.y - dy }, b: { x: s.x + dx, y: s.y + dy } };
    return { a: { ...s }, b: { x: s.x + dx, y: s.y + dy } };
  }

  /** Koncové body čáry po aplikaci SHIFT (úhel po 45°) a ALT (od středu). */
  function lineEnds(): { a: { x: number; y: number }; b: { x: number; y: number } } {
    const s = shapeStart!;
    let dx = shapeCurr.x - s.x,
      dy = shapeCurr.y - s.y;
    if (shiftDown) {
      const len = Math.hypot(dx, dy);
      const snap = Math.round(Math.atan2(dy, dx) / (Math.PI / 4)) * (Math.PI / 4);
      dx = Math.cos(snap) * len;
      dy = Math.sin(snap) * len;
    }
    if (altDown) return { a: { x: s.x - dx, y: s.y - dy }, b: { x: s.x + dx, y: s.y + dy } };
    return { a: { ...s }, b: { x: s.x + dx, y: s.y + dy } };
  }

  function shapePreview(): { pts: { x: number; y: number }[]; closed: boolean; label: string } | null {
    if (!drawTool || !shapeStart) return null;
    if (drawTool === "line") {
      const { a, b } = lineEnds();
      const len = Math.hypot(b.x - a.x, b.y - a.y);
      return { pts: [a, b], closed: false, label: `${len.toFixed(1)} mm` };
    }
    const { a, b } = shapeCorners();
    const mnX = Math.min(a.x, b.x),
      mxX = Math.max(a.x, b.x);
    const mnY = Math.min(a.y, b.y),
      mxY = Math.max(a.y, b.y);
    const label = `${(mxX - mnX).toFixed(1)} × ${(mxY - mnY).toFixed(1)} mm`;
    if (drawTool === "rect") {
      return {
        pts: [
          { x: mnX, y: mnY },
          { x: mxX, y: mnY },
          { x: mxX, y: mxY },
          { x: mnX, y: mxY },
        ],
        closed: true,
        label,
      };
    }
    // Elipsa — tessellace jen pro náhled, finální geometrii dělá SVG parser
    const cx = (mnX + mxX) / 2,
      cy = (mnY + mxY) / 2;
    const rx = (mxX - mnX) / 2,
      ry = (mxY - mnY) / 2;
    const pts: { x: number; y: number }[] = [];
    for (let i = 0; i < 48; i++) {
      const t = (i / 48) * Math.PI * 2;
      pts.push({ x: cx + rx * Math.cos(t), y: cy + ry * Math.sin(t) });
    }
    return { pts, closed: true, label };
  }

  function finalizeShape(): DrawnShape | null {
    if (!drawTool || !shapeStart) return null;
    if (drawTool === "line") {
      const { a, b } = lineEnds();
      if (Math.hypot(b.x - a.x, b.y - a.y) < MIN_SHAPE_MM) return null;
      // Konec přichycený magnetem (a neposunutý modifikátorem) se drží objektu
      const kept = (snap: SnapHit | null, p: Pt) =>
        snap?.anchor && Math.hypot(snap.x - p.x, snap.y - p.y) < 1e-9 ? snap.anchor : undefined;
      const start = kept(shapeStartSnap, a),
        end = kept(shapeCurrSnap, b);
      return {
        kind: "line",
        x1: a.x,
        y1: a.y,
        x2: b.x,
        y2: b.y,
        ...(start || end ? { anchors: { ...(start ? { start } : {}), ...(end ? { end } : {}) } } : {}),
      };
    }
    const { a, b } = shapeCorners();
    const w = Math.abs(b.x - a.x),
      h = Math.abs(b.y - a.y);
    if (w < MIN_SHAPE_MM || h < MIN_SHAPE_MM) return null;
    if (drawTool === "rect") {
      return { kind: "rect", x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), w, h };
    }
    return {
      kind: "ellipse",
      cx: (a.x + b.x) / 2,
      cy: (a.y + b.y) / 2,
      rx: w / 2,
      ry: h / 2,
    };
  }

  // ─── Sklo hit test: PEVNÁ pozice, bez rotace ─────────────────────────────
  function isInsideSlide(wx: number, wy: number, posIdx: number): boolean {
    const pos = positions[posIdx];
    // Sklo se NEHÝBE — vždy na pos.x, pos.y
    return wx >= pos.x && wx <= pos.x + pos.width && wy >= pos.y && wy <= pos.y + pos.height;
  }

  // ─── Handles pro trasu: rohy bbox trasy v world space (po rotaci) ─────────
  function getPathHandles(posIdx: number, useT?: Transform): { x: number; y: number }[] {
    if (posIdx < 0 || posIdx >= positions.length) return [];
    const pos = positions[posIdx];
    if (pos.is_prime) return [];
    const tidx = getTransformIdx(posIdx, positions);
    const t = useT ?? transforms[tidx];
    if (!t) return [];

    const pathData = paths[tidx];
    let minX: number, maxX: number, minY: number, maxY: number;

    if (pathData && pathData.segments.length > 0) {
      let mn = Infinity,
        mx = -Infinity,
        mny = Infinity,
        mxy = -Infinity;
      for (const seg of pathData.segments) {
        for (const pt of seg.points) {
          if (pt.x < mn) mn = pt.x;
          if (pt.x > mx) mx = pt.x;
          if (pt.y < mny) mny = pt.y;
          if (pt.y > mxy) mxy = pt.y;
        }
      }
      if (!isFinite(mn)) return [];
      const r = nozzleDiam / 2;
      // Aplikovat scale kolem (t.cx, t.cy) — stejná logika jako transform_pt v Rustu
      minX = t.cx + (mn - t.cx) * t.scale - r;
      maxX = t.cx + (mx - t.cx) * t.scale + r;
      const sy = t.scale_y ?? t.scale;
      minY = t.cy + (mny - t.cy) * sy - r;
      maxY = t.cy + (mxy - t.cy) * sy + r;
    } else {
      // Trasa prázdná — žádné handles
      return [];
    }

    // Středová osa rotace trasy v world space
    const pathCX = t.gui_dx + t.cx;
    const pathCY = t.gui_dy + t.cy;
    const rad = (-t.rotation * Math.PI) / 180; // canvas používá -rot
    const cos_r = Math.cos(rad),
      sin_r = Math.sin(rad);

    const localCorners = [
      { x: t.gui_dx + minX, y: t.gui_dy + minY }, // TL
      { x: t.gui_dx + maxX, y: t.gui_dy + minY }, // TR
      { x: t.gui_dx + minX, y: t.gui_dy + maxY }, // BL
      { x: t.gui_dx + maxX, y: t.gui_dy + maxY }, // BR
    ];

    return localCorners.map((c) => {
      const dx = c.x - pathCX,
        dy = c.y - pathCY;
      return {
        x: pathCX + dx * cos_r - dy * sin_r,
        y: pathCY + dx * sin_r + dy * cos_r,
      };
    });
  }

  function isInsidePath(wx: number, wy: number, posIdx: number): boolean {
    if (posIdx < 0 || posIdx >= positions.length) return false;
    const pos = positions[posIdx];
    if (pos.is_prime) return false;
    const tidx = getTransformIdx(posIdx, positions);
    const t = transforms[tidx];
    if (!t) return false;
    const raw = getPathBboxRaw(posIdx);
    if (!raw) return false;

    const { x: px, y: py } = worldToLocal(wx, wy, t);
    const tol = nozzleDiam;
    return px >= raw.mnX - tol && px <= raw.mxX + tol && py >= raw.mnY - tol && py <= raw.mxY + tol;
  }

  /** Inverze transformace tpt: world → lokální souřadnice trasy. */
  function worldToLocal(wx: number, wy: number, t: Transform): { x: number; y: number } {
    const dx = wx - (t.gui_dx + t.cx);
    const dy = wy - (t.gui_dy + t.cy);
    const rot = (t.rotation * Math.PI) / 180;
    const cos_r = Math.cos(rot),
      sin_r = Math.sin(rot);
    return {
      x: t.cx + (dx * cos_r - dy * sin_r) / t.scale,
      y: t.cy + (dx * sin_r + dy * cos_r) / (t.scale_y ?? t.scale),
    };
  }

  /** Objekt trasy pod kurzorem (nejmenší obálka, která bod obsahuje). */
  function hitTestObject(wx: number, wy: number, posIdx: number): number | null {
    const tidx = getTransformIdx(posIdx, positions);
    const t = transforms[tidx];
    const objs = paths[tidx]?.objects ?? [];
    if (!t || objs.length === 0) return null;
    const p = worldToLocal(wx, wy, t);
    const tol = nozzleDiam;
    let best: number | null = null,
      bestArea = Infinity;
    for (const o of objs) {
      const [x0, x1, y0, y1] = o.bounds;
      if (p.x < x0 - tol || p.x > x1 + tol || p.y < y0 - tol || p.y > y1 + tol) continue;
      const area = (x1 - x0 + tol) * (y1 - y0 + tol);
      if (area < bestArea) {
        bestArea = area;
        best = o.id;
      }
    }
    return best;
  }

  // ─── Úprava vybraného objektu ─────────────────────────────────────────────
  function frameOf(ti: number): SlideFrame {
    const ps = $projectStore;
    const us = ps.bakedScales[ti] ?? 1;
    return { t: transforms[ti], us, usY: ps.bakedScalesY?.[ti] ?? us };
  }

  /** Vybraný objekt vybrané trasy, pokud ho lze upravovat v plátně. */
  function editableObject(): { ti: number; obj: PathObject; base: [number, number, number, number]; pivot: Pt } | null {
    if (!focusedObject || selectedIndex < 0 || positions[selectedIndex]?.is_prime) return null;
    const ti = getTransformIdx(selectedIndex, positions);
    if (ti !== focusedObject.slide || !transforms[ti]) return null;
    const obj = paths[ti]?.objects?.find((o) => o.id === focusedObject!.id);
    const pivot = obj ? objectPivot(obj) : null;
    if (!obj || !obj.base_bounds || !pivot) return null;
    return { ti, obj, base: obj.base_bounds, pivot };
  }

  /** Rozpracovaná úprava objektu: tažení, nebo puštěná a čekající na přepočet drah. */
  function activeEdit(): ObjEdit | null {
    if (objDrag) return objDrag;
    return pendingObj && pendingObj.src === paths[pendingObj.ti] ? pendingObj : null;
  }

  /** Úprava objektu, jak ji právě zobrazit (rozpracovaná, jinak uložená). */
  function shownXform(ti: number, obj: PathObject): ObjectTransform {
    const e = activeEdit();
    if (e && e.ti === ti && e.id === obj.id) return e.cur;
    return obj.xform ?? IDENTITY_XF;
  }

  /** Rohy rámu objektu ve world (TL, TR, BL, BR jako úchyty trasy). */
  function objectHandles(e: NonNullable<ReturnType<typeof editableObject>>): Pt[] {
    const f = frameOf(e.ti);
    const xf = shownXform(e.ti, e.obj);
    return baseCorners(e.base, nozzleDiam / 2).map((c) => designToWorld(f, xfApply(xf, c, e.pivot)));
  }

  /** Leží bod (world) uvnitř rámu vybraného objektu? */
  function insideObjectFrame(e: NonNullable<ReturnType<typeof editableObject>>, w: Pt): boolean {
    const q = xfInverse(shownXform(e.ti, e.obj), worldToDesign(frameOf(e.ti), w), e.pivot);
    const [x0, x1, y0, y1] = e.base;
    const tol = nozzleDiam;
    return q.x >= x0 - tol && q.x <= x1 + tol && q.y >= y0 - tol && q.y <= y1 + tol;
  }

  // ─── Živý náhled úpravy objektu ───────────────────────────────────────────
  // Během tažení se dráhy dotčených sklíček přepočítávají ve WASM stejným
  // zpracováním jako layout — výplň, napojené čáry i pořadí tisku odpovídají
  // výsledku, ne jen posunutým starým drahám.
  let livePaths = new Map<number, SubstratePaths>();
  let liveRawJson: string | null = null;
  let liveDirty = false;
  /** Přepočet všech skel trval moc dlouho — do konce tažení jen upravované sklo. */
  let liveOnlyEdited = false;

  function startLivePreview() {
    const raw = $projectStore.rawLoadedPaths;
    liveRawJson = raw ? JSON.stringify(raw) : null;
    livePaths = new Map();
    liveDirty = false;
    liveOnlyEdited = false;
  }

  function updateLivePaths() {
    liveDirty = false;
    const e = objDrag;
    if (!e || !liveRawJson) return;
    const ps = $projectStore;
    const t0 = performance.now();
    const slides = objEditOnlyHere || liveOnlyEdited
      ? [e.ti]
      : positions.filter((p) => !p.is_prime).map((_, k) => k).sort((a, b) => (a === e.ti ? -1 : b === e.ti ? 1 : a - b));
    for (const ti of slides) {
      const t = transforms[ti];
      if (!t) continue;
      const us = ps.bakedScales[ti] ?? 1;
      const sp = sliceParamsFor(ps, ti, t.rotation, us, ps.bakedScalesY?.[ti] ?? us);
      sp.object_transforms = { ...sp.object_transforms, [String(e.id)]: e.cur };
      try {
        livePaths.set(ti, processPathsWasm(liveRawJson, sp));
      } catch (err) {
        console.error("Živý náhled objektu selhal:", err);
        return;
      }
    }
    if (performance.now() - t0 > 40) liveOnlyEdited = true;
  }

  /** Dráhy sklíčka k vykreslení: živý náhled úpravy objektu, jinak ze store. */
  function shownPaths(ti: number): SubstratePaths | undefined {
    const live = activeEdit() ? livePaths.get(ti) : undefined;
    return live ?? paths[ti];
  }

  /** Úchyty právě aktivní úpravy: vybraného objektu, jinak trasy. */
  function activeHandles(): Pt[] {
    const e = editableObject();
    if (e) return objectHandles(e);
    return selectedIndex >= 0 ? getPathHandles(selectedIndex) : [];
  }

  function hitTestHandle(wx: number, wy: number): number {
    if (selectedIndex < 0) return -1;
    const handles = activeHandles();
    const hitR = 7 / zoom;
    for (let i = 0; i < handles.length; i++) {
      if (Math.hypot(handles[i].x - wx, handles[i].y - wy) < hitR) return i;
    }
    return -1;
  }

  // ─── Sdílené geometrické helpery ─────────────────────────────────────────

  function getPathBboxRaw(posIdx: number): RawBbox | null {
    if (_bboxDragCache.has(posIdx)) return _bboxDragCache.get(posIdx)!;
    const tidx = getTransformIdx(posIdx, positions);
    const pathData = paths[tidx];
    if (!pathData || pathData.segments.length === 0) return null;
    let mnX = Infinity,
      mxX = -Infinity,
      mnY = Infinity,
      mxY = -Infinity;
    for (const seg of pathData.segments)
      for (const pt of seg.points) {
        if (pt.x < mnX) mnX = pt.x;
        if (pt.x > mxX) mxX = pt.x;
        if (pt.y < mnY) mnY = pt.y;
        if (pt.y > mxY) mxY = pt.y;
      }
    const result = isFinite(mnX) ? { mnX, mxX, mnY, mxY } : null;
    if (dragOp) _bboxDragCache.set(posIdx, result);
    return result;
  }

  /** Obálka objektů sklíčka (bez děr) v lokálních souřadnicích. */
  function designBbox(pd: SubstratePaths | undefined): RawBbox | null {
    const objs = (pd?.objects ?? []).filter((o) => !o.hole);
    if (objs.length === 0) return null;
    return {
      mnX: Math.min(...objs.map((o) => o.bounds[0])),
      mxX: Math.max(...objs.map((o) => o.bounds[1])),
      mnY: Math.min(...objs.map((o) => o.bounds[2])),
      mxY: Math.max(...objs.map((o) => o.bounds[3])),
    };
  }

  /** Přerušovaný rámeček kolem objektu rozbaleného v lokálním panelu. */
  function drawFocusedObject(c: CanvasRenderingContext2D) {
    if (!focusedObject) return;
    const ti = focusedObject.slide;
    const t = transforms[ti];
    const obj = paths[ti]?.objects?.find((o) => o.id === focusedObject!.id);
    if (!t || !obj) return;
    const pivot = objectPivot(obj);
    let corners: [number, number][];
    if (obj.base_bounds && pivot) {
      // Rám neupraveného objektu po jeho úpravě (otočený s objektem)
      const f = frameOf(ti);
      const xf = shownXform(ti, obj);
      const [a, b, d, e] = baseCorners(obj.base_bounds, nozzleDiam / 2).map((p) =>
        designToWorld(f, xfApply(xf, p, pivot))
      );
      corners = [a, b, e, d].map((p) => [p.x, p.y]);
    } else {
      const [x0, x1, y0, y1] = obj.bounds;
      const pad = nozzleDiam / 2;
      corners = [
        [x0 - pad, y0 - pad],
        [x1 + pad, y0 - pad],
        [x1 + pad, y1 + pad],
        [x0 - pad, y1 + pad],
      ].map(([x, y]) => tpt(x, y, t.gui_dx, t.gui_dy, t.scale, t.rotation, t.cx, t.cy, t.scale_y));
    }
    c.save();
    c.strokeStyle = "#fbbf24";
    c.lineWidth = 1.5 / zoom;
    c.setLineDash([4 / zoom, 3 / zoom]);
    c.beginPath();
    corners.forEach(([x, y], k) => (k === 0 ? c.moveTo(x, y) : c.lineTo(x, y)));
    c.closePath();
    c.stroke();
    c.restore();
  }

  // ─── Measure snap ─────────────────────────────────────────────────────────
  function applyMeasureSnap(wx: number, wy: number): { x: number; y: number } {
    if (ctrlDown) return { x: snapToGrid(wx), y: snapToGrid(wy) };
    if (altDown) return { x: wx, y: wy }; // ALT = volný kurzor bez snapu

    // Výchozí: snap na klíčové body sklíček + body tras
    let best = { x: wx, y: wy }, bestDist = 8 / zoom;

    for (const pos of positions) {
      for (const [cx, cy] of [
        [pos.x, pos.y],
        [pos.x + pos.width, pos.y],
        [pos.x, pos.y + pos.height],
        [pos.x + pos.width, pos.y + pos.height],
        [pos.x + pos.width / 2, pos.y + pos.height / 2],
        [pos.x + pos.width / 2, pos.y],
        [pos.x + pos.width / 2, pos.y + pos.height],
        [pos.x, pos.y + pos.height / 2],
        [pos.x + pos.width, pos.y + pos.height / 2],
      ] as [number, number][]) {
        const d = Math.hypot(cx - wx, cy - wy);
        if (d < bestDist) { bestDist = d; best = { x: cx, y: cy }; }
      }
    }

    for (let i = 0; i < positions.length; i++) {
      if (positions[i].is_prime) continue;
      const tidx = getTransformIdx(i, positions);
      const t = transforms[tidx];
      const pd = paths[tidx];
      if (!t || !pd) continue;
      for (const seg of pd.segments) {
        for (const pt of seg.points) {
          const [ptx, pty] = tpt(pt.x, pt.y, t.gui_dx, t.gui_dy, t.scale, t.rotation, t.cx, t.cy, t.scale_y);
          const d = Math.hypot(ptx - wx, pty - wy);
          if (d < bestDist) { bestDist = d; best = { x: ptx, y: pty }; }
        }
      }
    }

    return best;
  }

  // ─── Key handlers ─────────────────────────────────────────────────────────
  function handleKeyDown(e: KeyboardEvent) {
    ctrlDown = e.ctrlKey;
    altDown = e.altKey;
    shiftDown = e.shiftKey;
    if (e.key === "Escape") {
      if (drawTool) {
        // Rozkreslený tvar zruš; bez rozkresleného tvaru ukonči nástroj
        if (shapeStart) shapeStart = null;
        else dispatch("drawToolExit");
      }
      if (isMeasuring) {
        measurePoints = [];
        dispatch("measurePointsChange", measurePoints);
      }
      transformMode = "scale";
      scheduleDraw();
    }
    if (isMeasuring || (drawTool && shapeStart)) scheduleDraw();
  }
  function handleKeyUp(e: KeyboardEvent) {
    ctrlDown = e.ctrlKey;
    altDown = e.altKey;
    shiftDown = e.shiftKey;
    if (isMeasuring || dragOp || (drawTool && shapeStart)) scheduleDraw();
  }

  // ─── Wheel ────────────────────────────────────────────────────────────────
  function handleWheel(e: WheelEvent) {
    e.preventDefault();
    const rect = canvas.getBoundingClientRect();
    const dir = e.deltaY < 0 ? 1 : -1;
    const margin = 40;
    const minZoom = Math.min((width - margin * 2) / bedMaxX, (height - margin * 2) / bedMaxY) * 0.7;
    const bw = screenToWorld(e.clientX - rect.left, e.clientY - rect.top);
    zoom = Math.max(minZoom, Math.min(zoom * (dir > 0 ? 1.1 : 1 / 1.1), 80));
    const as = worldToScreen(bw.x, bw.y);
    panX += e.clientX - rect.left - as.x;
    panY += e.clientY - rect.top - as.y;
    scheduleDraw();
  }

  // ─── Mouse down ───────────────────────────────────────────────────────────
  function handleMouseDown(e: MouseEvent) {
    const rect = canvas.getBoundingClientRect();
    const sx = e.clientX - rect.left,
      sy = e.clientY - rect.top;
    ctrlDown = e.ctrlKey;
    altDown = e.altKey;
    shiftDown = e.shiftKey;
    mouseX = sx;
    mouseY = sy;
    clickCandidate = null;

    if (e.button === 0) {
      if (drawTool) {
        shapeStart = drawPointFromMouse(sx, sy);
        shapeStartSnap = snapHit;
        shapeCurr = { ...shapeStart };
        shapeCurrSnap = snapHit;
        scheduleDraw();
        return;
      }

      if (isMeasuring) {
        const raw = screenToWorld(sx, sy);
        const snapped = applyMeasureSnap(raw.x, raw.y);
        measurePoints = [...measurePoints, snapped];
        dispatch("measurePointsChange", measurePoints);
        scheduleDraw();
        return;
      }

      const w = screenToWorld(sx, sy);
      const now = Date.now();

      // 0. Vybraný objekt: úchyty = měřítko / rotace objektu, uvnitř rámu = posun
      //    (dvojklik přepne měřítko ↔ rotace jako u trasy)
      const eo = editableObject();
      if (eo) {
        const hi = hitTestHandle(w.x, w.y);
        const inside = hi < 0 && insideObjectFrame(eo, w);
        if (hi >= 0 || inside) {
          const isDoubleClick = inside && now - lastClickTime < 300 && lastClickIndex === selectedIndex;
          lastClickTime = now;
          lastClickIndex = selectedIndex;
          if (isDoubleClick) {
            transformMode = transformMode === "rotate" ? "scale" : "rotate";
            scheduleDraw();
            return;
          }
          dispatch("saveState");
          const frame = frameOf(eo.ti);
          const start: ObjectTransform = { ...(eo.obj.xform ?? IDENTITY_XF) };
          objDrag = { ti: eo.ti, id: eo.obj.id, pivot: eo.pivot, start, cur: start, startD: worldToDesign(frame, w), frame, base: eo.base };
          pendingObj = null;
          dragOp = hi >= 0 ? (transformMode === "rotate" ? "objRotate" : "objScale") : "objMove";
          startLivePreview();
          activeHandle = hi;
          if (inside) clickCandidate = { sx, sy, posIdx: selectedIndex };
          return;
        }
      }

      // 1. Handle na již vybrané trase
      if (selectedIndex >= 0 && !positions[selectedIndex]?.is_prime) {
        const hi = hitTestHandle(w.x, w.y);
        if (hi >= 0) {
          const tidx = getTransformIdx(selectedIndex, positions);
          dispatch("saveState");
          dragOp = transformMode === "rotate" ? "rotate" : "scale";
          activeHandle = hi;
          dragSlideIdx = selectedIndex;
          dragStartWorld = { ...w };
          dragStartTransform = { ...transforms[tidx] };
          if (dragOp === "rotate") {
            // Alt při otáčení = všechna skla (při škálování Alt odemyká poměr stran)
            dragStartTransformsAll = new Map();
            for (let j = 0; j < positions.length; j++) {
              if (positions[j].is_prime) continue;
              const oj = getTransformIdx(j, positions);
              if (transforms[oj]) dragStartTransformsAll.set(oj, { ...transforms[oj] });
            }
          }
          return;
        }
      }

      // 2. Hit test na sklíčko (pevná pozice)
      let hitIdx = -1;
      for (let i = positions.length - 1; i >= 0; i--) {
        if (isInsideSlide(w.x, w.y, i)) {
          hitIdx = i;
          break;
        }
      }

      if (hitIdx >= 0 && !positions[hitIdx].is_prime) {
        const isDoubleClick = now - lastClickTime < 300 && lastClickIndex === hitIdx;
        lastClickTime = now;
        lastClickIndex = hitIdx;

        if (hitIdx === selectedIndex) {
          if (isDoubleClick) {
            // Přepnout rotační / scale mód
            transformMode = transformMode === "rotate" ? "scale" : "rotate";
            scheduleDraw();
            return;
          }
          dispatch("slideSelected", hitIdx);
          // Přesun trasy jen pokud klik míří přímo na bbox trasy
          const tidx = getTransformIdx(hitIdx, positions);
          clickCandidate = { sx, sy, posIdx: hitIdx };
          if (transforms[tidx] && isInsidePath(w.x, w.y, hitIdx)) {
            dispatch("saveState");
            dragOp = "move";
            dragSlideIdx = hitIdx;
            dragStartWorld = { ...w };
            dragStartTransform = { ...transforms[tidx] };
            return;
          }
          // Klik na plochu skla mimo trasu → pan (propad níže)
        } else {
          // Vybrat sklíčko (trasu) — objekt se vybírá až dalším klikem
          dispatch("slideSelected", hitIdx);
          dispatch("objectSelected", null);
          transformMode = "scale";
          return;
        }
      } else {
        // Klik do prázdna — odselektovat
        if (hitIdx !== selectedIndex) {
          dispatch("slideSelected", -1);
          dispatch("objectSelected", null);
          transformMode = "scale";
        }
      }

      // Pan
      dragOp = "pan";
      lastMouseScreenX = sx;
      lastMouseScreenY = sy;
    } else if (e.button === 2) {
      if (drawTool) {
        if (shapeStart) shapeStart = null;
        else dispatch("drawToolExit");
        scheduleDraw();
        return;
      }
      if (isMeasuring) {
        measurePoints = measurePoints.slice(0, -1);
        dispatch("measurePointsChange", measurePoints);
        scheduleDraw();
        return;
      }
      const w = screenToWorld(sx, sy);
      for (let i = positions.length - 1; i >= 0; i--) {
        if (isInsideSlide(w.x, w.y, i)) {
          // Pravý klik na objekt vybrané trasy ho vybere (menu pak nabízí i objekt)
          if (i === selectedIndex) {
            const id = hitTestObject(w.x, w.y, i);
            if (id != null) dispatch("objectSelected", { slide: getTransformIdx(i, positions), id });
          } else {
            dispatch("objectSelected", null);
          }
          dispatch("slideContext", { index: i, x: e.clientX, y: e.clientY });
          return;
        }
      }
    }
  }

  // ─── Mouse move ───────────────────────────────────────────────────────────
  function handleMouseMove(e: MouseEvent) {
    const rect = canvas.getBoundingClientRect();
    mouseX = e.clientX - rect.left;
    mouseY = e.clientY - rect.top;
    ctrlDown = e.ctrlKey;
    altDown = e.altKey;
    shiftDown = e.shiftKey;

    if (drawTool) {
      if (shapeStart) {
        shapeCurr = drawPointFromMouse(mouseX, mouseY);
        shapeCurrSnap = snapHit;
        scheduleDraw();
      } else if (snapToPaths) {
        // Značka magnetu už při najetí, před začátkem kreslení
        drawPointFromMouse(mouseX, mouseY);
        scheduleDraw();
      }
      return;
    }

    if (dragOp === "pan") {
      panX += mouseX - lastMouseScreenX;
      panY += mouseY - lastMouseScreenY;
      lastMouseScreenX = mouseX;
      lastMouseScreenY = mouseY;
      scheduleDraw();
      return;
    }

    // ── Úprava vybraného objektu (v prostoru kresby, viz objectTransform.ts) ──
    if (objDrag && (dragOp === "objMove" || dragOp === "objScale" || dragOp === "objRotate")) {
      const d = objDrag;
      const m = worldToDesign(d.frame, screenToWorld(mouseX, mouseY));
      const st = d.start;
      if (dragOp === "objMove") {
        let ddx = m.x - d.startD.x,
          ddy = m.y - d.startD.y;
        if (ctrlDown) {
          const g = gridSnapSize();
          ddx = Math.round(ddx / g) * g;
          ddy = Math.round(ddy / g) * g;
        }
        d.cur = { ...st, dx: st.dx + ddx, dy: st.dy + ddy };
      } else if (dragOp === "objScale" && activeHandle >= 0) {
        // Úchyt leží na rámu s okrajem, kotva je skutečný roh objektu
        const grip = baseCorners(d.base, nozzleDiam / 2)[activeHandle];
        // SHIFT = škálovat od středu objektu, jinak drží protější roh
        const oppBase = shiftDown ? d.pivot : baseCorners(d.base)[3 - activeHandle];
        const hc = xfApply(st, grip, d.pivot);
        const ac = xfApply(st, oppBase, d.pivot);
        // Osy objektu: inverze rotace z xfApply
        const rad = (st.rotation * Math.PI) / 180;
        const cr = Math.cos(rad),
          sr = Math.sin(rad);
        const toObj = (vx: number, vy: number) => ({ x: vx * cr - vy * sr, y: vx * sr + vy * cr });
        const init = toObj(hc.x - ac.x, hc.y - ac.y);
        const curr = toObj(m.x - ac.x, m.y - ac.y);
        const clampScale = (v: number) => {
          const c = Math.max(0.02, Math.min(50, v));
          return ctrlDown ? Math.max(0.1, Math.round(c * 10) / 10) : c;
        };
        const sy0 = st.scale_y ?? st.scale;
        let sx: number, sy: number;
        if (altDown) {
          // ALT = odemčený poměr stran
          sx = clampScale(st.scale * (Math.abs(init.x) > 1e-6 ? curr.x / init.x : 1));
          sy = clampScale(sy0 * (Math.abs(init.y) > 1e-6 ? curr.y / init.y : 1));
        } else {
          const initDist = Math.hypot(init.x, init.y);
          if (initDist <= 1e-6) return;
          sx = clampScale(st.scale * (Math.hypot(curr.x, curr.y) / initDist));
          sy = sy0 * (sx / st.scale);
        }
        const next: ObjectTransform = { dx: 0, dy: 0, scale: sx, rotation: st.rotation };
        if (altDown || st.scale_y != null) next.scale_y = sy;
        // Kotva zůstává na místě
        const q = xfApply(next, oppBase, d.pivot);
        next.dx = ac.x - q.x;
        next.dy = ac.y - q.y;
        d.cur = next;
      } else if (dragOp === "objRotate") {
        const c = xfApply(st, d.pivot, d.pivot);
        const a0 = Math.atan2(d.startD.y - c.y, d.startD.x - c.x);
        const a1 = Math.atan2(m.y - c.y, m.x - c.x);
        let rot = st.rotation - ((a1 - a0) * 180) / Math.PI;
        rot = ((rot % 360) + 360) % 360;
        if (ctrlDown) rot = Math.round(rot / 15) * 15;
        d.cur = { ...st, rotation: rot };
      }
      liveDirty = true;
      scheduleDraw();
      return;
    }

    // ── Přesun trasy (mění gui_dx / gui_dy; sklo se NEHÝBE) ────────────────
    if (dragOp === "move" && dragStartTransform !== null && dragSlideIdx >= 0) {
      const w = screenToWorld(mouseX, mouseY);
      const pos = positions[dragSlideIdx];
      const tidx = getTransformIdx(dragSlideIdx, positions);
      const t = transforms[tidx];
      if (!t) return;

      let newX = dragStartTransform.gui_dx + (w.x - dragStartWorld.x);
      let newY = dragStartTransform.gui_dy + (w.y - dragStartWorld.y);

      if (ctrlDown) {
        // Snap offsetu trasy vůči levému dolnímu rohu skla
        const g = gridSnapSize();
        newX = pos.x + Math.round((newX - pos.x) / g) * g;
        newY = pos.y + Math.round((newY - pos.y) / g) * g;
      }

      if (altDown) {
        const relDx = newX - pos.x;
        const relDy = newY - pos.y;
        positions.forEach((p, i) => {
          if (p.is_prime) return;
          const oi = getTransformIdx(i, positions);
          const ot = transforms[oi];
          if (ot) {
            ot.gui_dx = p.x + relDx;
            ot.gui_dy = p.y + relDy;
            const raw = getPathBboxRaw(i);
            if (raw) clampGuidXY(ot, p, raw, nozzleDiam);
            _dragModifiedIndices.add(oi);
          }
        });
      } else {
        t.gui_dx = newX;
        t.gui_dy = newY;
        const raw = getPathBboxRaw(dragSlideIdx);
        if (raw) clampGuidXY(t, pos, raw, nozzleDiam);
        _dragModifiedIndices.add(tidx);
      }
      scheduleDraw();
      return;
    }

    // ── Škálování trasy ─────────────────────────────────────────────────────
    if (
      dragOp === "scale" &&
      dragStartTransform !== null &&
      dragSlideIdx >= 0 &&
      activeHandle >= 0
    ) {
      const w = screenToWorld(mouseX, mouseY);
      const pos = positions[dragSlideIdx];
      const tidx = getTransformIdx(dragSlideIdx, positions);
      const t = transforms[tidx];
      if (!t) return;

      const initHandles = getPathHandles(dragSlideIdx, dragStartTransform);
      if (initHandles.length < 4) return;

      const st = dragStartTransform;
      const stSy = st.scale_y ?? st.scale;
      const oppositeIdx = 3 - activeHandle; // TL↔BR, TR↔BL
      // SHIFT = anchor ve středu objektu, jinak = protější roh
      const anchor = shiftDown
        ? { x: st.gui_dx + st.cx, y: st.gui_dy + st.cy }
        : initHandles[oppositeIdx];
      const clampScale = (v: number) => {
        const c = Math.max(0.05, Math.min(10.0, v));
        return ctrlDown ? Math.max(0.1, Math.round(c * 10) / 10) : c;
      };

      // Lokální (neotočené) osy trasy: world = R(−rot)·local, local = R(rot)·world
      const rad = (st.rotation * Math.PI) / 180;
      const cr = Math.cos(rad),
        sr = Math.sin(rad);
      const toLocal = (vx: number, vy: number) => ({ x: vx * cr - vy * sr, y: vx * sr + vy * cr });
      const fromLocal = (vx: number, vy: number) => ({ x: vx * cr + vy * sr, y: -vx * sr + vy * cr });

      let rx: number, ry: number;
      if (altDown) {
        // ALT = odemčený poměr stran: měřítko každé osy zvlášť
        const init = toLocal(initHandles[activeHandle].x - anchor.x, initHandles[activeHandle].y - anchor.y);
        const curr = toLocal(w.x - anchor.x, w.y - anchor.y);
        rx = clampScale(st.scale * (Math.abs(init.x) > 0.01 ? curr.x / init.x : 1)) / st.scale;
        ry = clampScale(stSy * (Math.abs(init.y) > 0.01 ? curr.y / init.y : 1)) / stSy;
      } else {
        const initDist = Math.hypot(initHandles[activeHandle].x - anchor.x, initHandles[activeHandle].y - anchor.y);
        if (initDist <= 0.01) return;
        const currDist = Math.hypot(w.x - anchor.x, w.y - anchor.y);
        rx = ry = clampScale(st.scale * (currDist / initDist)) / st.scale;
      }

      // Anchor zůstává na místě: jeho odsazení od středu trasy se v lokálních
      // osách škáluje (rx, ry) → nový střed = anchor − odsazení
      const c0x = st.gui_dx + st.cx,
        c0y = st.gui_dy + st.cy;
      const d0 = toLocal(anchor.x - c0x, anchor.y - c0y);
      const d1 = fromLocal(d0.x * rx, d0.y * ry);
      t.gui_dx = anchor.x - d1.x - st.cx;
      t.gui_dy = anchor.y - d1.y - st.cy;
      t.scale = st.scale * rx;
      t.scale_y = altDown || st.scale_y != null ? stSy * ry : undefined;
      // Trasa smí přesahovat mimo sklo (ořízne se), nesmí ale odjet celá
      const raw = getPathBboxRaw(dragSlideIdx);
      if (raw) clampGuidXY(t, pos, raw, nozzleDiam);
      _dragModifiedIndices.add(tidx);
      scheduleDraw();
      return;
    }

    // ── Rotace trasy ────────────────────────────────────────────────────────
    if (dragOp === "rotate" && dragStartTransform !== null && dragSlideIdx >= 0) {
      const w = screenToWorld(mouseX, mouseY);
      const pos = positions[dragSlideIdx];
      const tidx = getTransformIdx(dragSlideIdx, positions);
      const t = transforms[tidx];
      if (!t) return;

      const cx = dragStartTransform.gui_dx + dragStartTransform.cx;
      const cy = dragStartTransform.gui_dy + dragStartTransform.cy;
      const initAngle = Math.atan2(dragStartWorld.y - cy, dragStartWorld.x - cx);
      const currAngle = Math.atan2(w.y - cy, w.x - cx);
      // Raw (un-normalized) delta in degrees — kept separate for binary search interpolation.
      const rawDeltaDeg = (-(currAngle - initAngle) * 180) / Math.PI;

      // Trasa smí při otáčení přesahovat mimo sklo — přesah se při tisku ořízne
      const rotFromStart = (startRot: number): number => (((startRot + rawDeltaDeg) % 360) + 360) % 360;

      if (altDown) {
        positions.forEach((p, i) => {
          if (p.is_prime) return;
          const oi = getTransformIdx(i, positions);
          const ot = transforms[oi];
          const startTi = dragStartTransformsAll.get(oi);
          if (!ot || !startTi) return;
          let rot = rotFromStart(startTi.rotation);
          if (ctrlDown) rot = Math.round(rot / 15) * 15;
          ot.rotation = rot;
          const rawI = getPathBboxRaw(i);
          if (rawI) clampGuidXY(ot, p, rawI, nozzleDiam);
          _dragModifiedIndices.add(oi);
        });
      } else {
        const raw = getPathBboxRaw(dragSlideIdx);
        let newRot = rotFromStart(dragStartTransform.rotation);
        if (ctrlDown) newRot = Math.round(newRot / 15) * 15;
        t.rotation = newRot;
        if (raw) clampGuidXY(t, pos, raw, nozzleDiam);
        _dragModifiedIndices.add(tidx);
      }
      scheduleDraw();
      return;
    }

    // Hover feedback na handles
    if (selectedIndex >= 0 && !positions[selectedIndex]?.is_prime && dragOp === null) {
      const w = screenToWorld(mouseX, mouseY);
      const nh = hitTestHandle(w.x, w.y);
      if (nh !== hoverHandle) {
        hoverHandle = nh;
        scheduleDraw();
      }
    }

    if (isMeasuring) scheduleDraw();
  }

  function handleMouseUp() {
    if (drawTool) {
      if (shapeStart) {
        const shape = finalizeShape();
        shapeStart = null;
        if (shape) dispatch("shapeDrawn", shape);
        scheduleDraw();
      }
      return;
    }

    // Klik bez pohybu na již vybranou trasu → výběr objektu pod kurzorem
    const click = clickCandidate;
    clickCandidate = null;
    if (click && Math.hypot(mouseX - click.sx, mouseY - click.sy) < CLICK_TOLERANCE_PX) {
      const tidx = getTransformIdx(click.posIdx, positions);
      const t = transforms[tidx];
      const onlySelf = [..._dragModifiedIndices].every((i) => i === tidx);
      if (dragOp === "objMove" && objDrag) {
        // Klik bez pohybu na vybraný objekt: vybrat objekt pod kurzorem (vnořený)
        objDrag.cur = objDrag.start;
        livePaths = new Map();
        const w = screenToWorld(click.sx, click.sy);
        const id = hitTestObject(w.x, w.y, click.posIdx);
        if (id != null && id !== objDrag.id) dispatch("objectSelected", { slide: tidx, id });
      } else if (dragOp === "pan" || (dragOp === "move" && onlySelf)) {
        // Případný mikroposun během kliku se zahodí
        if (dragOp === "move" && t && dragStartTransform) {
          t.gui_dx = dragStartTransform.gui_dx;
          t.gui_dy = dragStartTransform.gui_dy;
        }
        _dragModifiedIndices.clear();
        const w = screenToWorld(click.sx, click.sy);
        const id = dragOp === "move" ? hitTestObject(w.x, w.y, click.posIdx) : null;
        dispatch("objectSelected", id != null ? { slide: tidx, id } : null);
      }
    }

    if (
      (dragOp === "scale" || dragOp === "rotate") &&
      dragSlideIdx >= 0 &&
      dragStartTransform !== null
    ) {
      const tidx = getTransformIdx(dragSlideIdx, positions);
      if (tidx >= 0) {
        const t = transforms[tidx];
        if (t) {
          const startSy = dragStartTransform.scale_y ?? dragStartTransform.scale;
          const scaleChanged =
            Math.abs(t.scale - dragStartTransform.scale) > 0.001 ||
            Math.abs((t.scale_y ?? t.scale) - startSy) > 0.001;
          const rotChanged = Math.abs(t.rotation - dragStartTransform.rotation) > 0.01;
          if (scaleChanged || rotChanged) {
            dispatch("pathRebuildNeeded", {
              slideIdx: tidx,
              scale: t.scale,
              scaleY: t.scale_y,
              rotation: t.rotation,
            });
          }
        }
      }
    }
    // Úprava objektu se zapíše najednou po puštění; náhled drží do přepočtu drah
    if (objDrag) {
      const e = objDrag;
      objDrag = null;
      if (liveDirty) {
        objDrag = e;
        updateLivePaths();
        objDrag = null;
      }
      if (JSON.stringify(e.cur) !== JSON.stringify(e.start)) {
        pendingObj = { ti: e.ti, id: e.id, pivot: e.pivot, start: e.start, cur: e.cur, src: paths[e.ti] };
        clearTimeout(_pendingTimer);
        _pendingTimer = setTimeout(() => {
          pendingObj = null;
          livePaths = new Map();
          scheduleDraw();
        }, 3000);
        dispatch("objectTransformChanged", { slide: e.ti, id: e.id, xform: { ...e.cur } });
      }
    }

    // Odešli všechny změny transformů najednou (store update 1× místo n× za drag)
    for (const idx of _dragModifiedIndices) {
      if (transforms[idx]) dispatch("transformChanged", { index: idx, transform: { ...transforms[idx] } });
    }
    _dragModifiedIndices.clear();

    dragOp = null;
    activeHandle = -1;
    dragSlideIdx = -1;
    dragStartTransform = null;
    dragStartTransformsAll = new Map();
    _bboxDragCache.clear();
    recomputePreviewDist();
  }
  function handleContextMenu(e: MouseEvent) {
    e.preventDefault();
  }

  // ─── Draw ──────────────────────────────────────────────────────────────────
  // Samotné kreslení je v src/lib/canvas2dDraw.ts — tady jen kompozice
  // s aktuálním stavem kamery, výběru a interakce.
  export function draw() {
    if (!ctx) return;
    if (liveDirty) updateLivePaths();

    const view = { width, height, panX, panY, zoom };
    const { minor: gMinor, major: gMajor } = gridDrawStep();

    let measTotal: number | null = null;
    let measCount = 0;

    ctx.fillStyle = "#0f172a";
    ctx.fillRect(0, 0, width, height);

    ctx.save();
    ctx.translate(panX, panY);
    ctx.scale(zoom, -zoom);

    // 1.–3. Bed, mřížka, okraj
    drawBedAndGrid(ctx, view, bedMaxX, bedMaxY, showAxes, gMinor, gMajor);

    // 4. Sklíčka + trasy (s preview kurzorem dle printProgress)
    const cursor = printCursor
      ? {
          slideIdx: printCursor.pos_idx,
          segIdx: printCursor.seg_idx,
          ptIdx: printCursor.pt_idx,
          fracT: printCursor.frac,
          startIdx: printCursor.start_idx,
        }
      : computePreviewCursor(printProgress, precomputedSegs, previewTotalTime);
    const { printPaths, ghostPaths } = pathsForDrawing();
    const previewNozzle = drawSlidesAndPaths(ctx, {
      positions,
      paths: printPaths,
      ghostPaths,
      primePath,
      transforms,
      overrides,
      selectedIndex,
      nozzleDiam,
      zoom,
      cursor,
      primeGlassSize: $projectStore.params
        ? { w: $projectStore.params.slide_w, h: $projectStore.params.slide_h }
        : null,
      showDirection: !!$settingsStore.show_print_direction,
      // Během úpravy objektu se dráhy liší od předpočítané časové osy
      startIdxOf: activeEdit() ? undefined : (slide, seg) => seqStartIdx.get(`${slide}:${seg}`),
    });

    // 6. Handles trasy pro vybrané sklíčko
    if (
      selectedIndex >= 0 &&
      selectedIndex < positions.length &&
      !positions[selectedIndex]?.is_prime
    ) {
      const eo = editableObject();
      if (eo) {
        // Vybraný objekt: úchyty objektu místo úchytů trasy
        const xf = shownXform(eo.ti, eo.obj);
        const center = designToWorld(frameOf(eo.ti), xfApply(xf, eo.pivot, eo.pivot));
        drawTransformHandles(ctx, objectHandles(eo), transformMode, hoverHandle, zoom, center);
      } else {
        const handles = getPathHandles(selectedIndex);
        const tidx = getTransformIdx(selectedIndex, positions);
        const t = transforms[tidx];
        drawTransformHandles(
          ctx,
          handles,
          transformMode,
          hoverHandle,
          zoom,
          t ? { x: t.gui_dx + t.cx, y: t.gui_dy + t.cy } : null
        );
      }
    }

    // 7. Měřidlo
    if (isMeasuring && measurePoints.length > 0) {
      const raw = screenToWorld(mouseX, mouseY);
      const snapped = applyMeasureSnap(raw.x, raw.y);
      const isSnapped =
        !altDown && (ctrlDown || Math.hypot(snapped.x - raw.x, snapped.y - raw.y) > 0.001);
      const res = drawMeasure(
        ctx,
        measurePoints,
        snapped,
        isSnapped,
        ctrlDown ? "#38bdf8" : "#a78bfa",
        zoom
      );
      measTotal = res.total;
      measCount = res.count;
    }

    drawFocusedObject(ctx);

    // 8. Náhled právě kresleného tvaru
    const sp = shapePreview();
    if (sp) drawShapePreview(ctx, sp.pts, sp.closed, sp.label, zoom);

    // 8c. Živý náhled sloučení uzlů
    if (mergePreview && mergePreview.groups.length > 0) drawMergePreview(ctx, mergePreview, zoom);

    // 8b. Magnet: bod, ke kterému se kurzor přichytil
    if (drawTool && snapToPaths && snapHit) {
      ctx.save();
      ctx.strokeStyle = "#e879f9";
      ctx.lineWidth = 1.5 / zoom;
      ctx.beginPath();
      ctx.arc(snapHit.x, snapHit.y, 5 / zoom, 0, Math.PI * 2);
      ctx.stroke();
      ctx.restore();
    }

    // 9. Trysky (reálná červená při tisku, preview cyan při náhledu)
    drawNozzleMarkers(ctx, currentNozzle, previewNozzle, nozzleDiam, zoom);

    ctx.restore();

    // ── Osy — popisky v obrazovkových souřadnicích (vždy viditelné) ──────────
    if (showAxes) drawAxisLabels(ctx, view, bedMaxX, bedMaxY, gMajor);

    // ── Obrazovkové overlaye ─────────────────────────────────────────────
    if (measTotal !== null) {
      ctx.font = "bold 11px sans-serif";
      const lbl = measCount > 1 ? `Celkem: ${measTotal.toFixed(1)} mm` : `${measTotal.toFixed(2)} mm`;
      const tw = ctx.measureText(lbl).width;
      ctx.fillStyle = "rgba(0,0,0,0.7)";
      ctx.fillRect(10, 10, tw + 16, 22);
      ctx.fillStyle = "#fde68a";
      ctx.fillText(lbl, 18, 25);
    }

    // ── Pravý horní overlay — celkové statistiky G-kódu ─────────────────
    let statsBoxBottom = 46;
    if ($projectStore.totalDist > 0 && ($settingsStore.show_gcode_stats ?? true)) {
      const td = $projectStore.totalDist;
      const tt = $projectStore.totalTime;
      const statLines: string[] = [
        "Statistiky G-kódu",
        `Celková dráha: ${td.toFixed(1)} mm`,
        `Čas tisku: ${Math.floor(tt / 60)} min ${Math.round(tt % 60)} s`,
      ];
      if (extrusionRateUl > 0) {
        const vol = td * extrusionRateUl;
        statLines.push(
          `Celkový objem: ${vol >= 1000 ? (vol / 1000).toFixed(3) + " ml" : vol.toFixed(2) + " µl"}`
        );
      }
      // Páry start–konec = souvislé tiskové tahy (dráha navazující přesně na
      // předchozí se tiskne bez přerušení, např. objekty spojené čarou)
      const runs = positions
        .map((p, i) => ({ p, ti: getTransformIdx(i, positions) }))
        .filter(({ p, ti }) => !p.is_prime && ti >= 0)
        .map(({ ti }) => ({
          name: overrides[String(ti)]?.name || `Substrát ${ti + 1}`,
          n: countPrintRuns(printPaths[ti]),
        }));
      if (runs.length > 0 && runs.every((r) => r.n === runs[0].n)) {
        statLines.push(
          runs.length > 1
            ? `Páry start–konec: ${runs[0].n} na substrát`
            : `Páry start–konec: ${runs[0].n}`
        );
      } else {
        for (const r of runs) statLines.push(`Páry start–konec (${r.name}): ${r.n}`);
      }
      statsBoxBottom = drawOverlayBox(ctx, statLines, width, 54, "#7dd3fc");
    }

    // ── Pravý horní overlay — info o vybraném prvku ──────────────────────────
    if (
      ($settingsStore.show_object_info ?? true) &&
      selectedIndex >= 0 &&
      selectedIndex < positions.length &&
      !positions[selectedIndex]?.is_prime
    ) {
      const si = getTransformIdx(selectedIndex, positions);
      const st = transforms[si];
      const spos = positions[selectedIndex];
      const spd = paths[si];
      if (st) {
        // Délka trasy — součet vzdáleností po sobě jdoucích bodů × aktuální scale
        let pathLen = 0;
        if (spd) {
          for (const seg of spd.segments)
            for (let k = 1; k < seg.points.length; k++)
              pathLen += Math.hypot(
                seg.points[k].x - seg.points[k - 1].x,
                seg.points[k].y - seg.points[k - 1].y
              );
          pathLen *= st.scale;
        }

        // Pozice středu trasy relativně k levému dolnímu rohu skla
        const cx = (st.gui_dx + st.cx - spos.x).toFixed(1);
        const cy = (st.gui_dy + st.cy - spos.y).toFixed(1);

        // Rozměry navrženého tvaru (objekty bez děr) × scale (nezávislé na rotaci) —
        // ne obálka drah, ta je o polovinu rozestupu výplně menší než nakreslený tvar
        const rawBb = designBbox(spd) ?? getPathBboxRaw(selectedIndex);
        const pw = rawBb ? (rawBb.mxX - rawBb.mnX) * st.scale : 0;
        const ph = rawBb ? (rawBb.mxY - rawBb.mnY) * (st.scale_y ?? st.scale) : 0;

        const lines: string[] = [
          `X: ${cx} mm    Y: ${cy} mm`,
          `Rotace: ${st.rotation.toFixed(1)}°`,
          `Rozměr: ${pw.toFixed(1)} × ${ph.toFixed(1)} mm  (${(pw * ph).toFixed(1)} mm²)`,
          `Trasa: ${pathLen.toFixed(1)} mm`,
        ];
        if (extrusionRateUl > 0) {
          const vol = pathLen * extrusionRateUl;
          lines.push(`Objem: ${vol >= 1000 ? (vol / 1000).toFixed(3) + " ml" : vol.toFixed(2) + " µl"}`);
        }
        const eo = editableObject();
        if (eo) {
          const xf = shownXform(eo.ti, eo.obj);
          const sy = xf.scale_y ?? xf.scale;
          const sc = Math.abs(sy - xf.scale) > 1e-6 ? `${xf.scale.toFixed(2)}×${sy.toFixed(2)}` : xf.scale.toFixed(2);
          lines.push(`Objekt: posun ${xf.dx.toFixed(1)}, ${xf.dy.toFixed(1)} mm · ${sc}× · ${xf.rotation.toFixed(1)}°`);
        }
        if (measTotal !== null) lines.push(`Pravítko: ${measTotal.toFixed(1)} mm`);

        drawOverlayBox(ctx, lines, width, statsBoxBottom + 8, null);
      }
    }

    // Jemná snap mřížka při CTRL
    if (
      ctrlDown &&
      (drawTool !== null || ((dragOp === "move" || dragOp === null) && selectedIndex >= 0))
    ) {
      drawSnapGrid(ctx, view, gridSnapSize());
    }
  }

  /** Počet souvislých tiskových tahů (pár start–konec) trasy sklíčka: každá
   *  dráha je tah, pokud přesně nenavazuje na konec předchozí. */
  function countPrintRuns(pd: SubstratePaths | undefined): number {
    let n = 0;
    let last: Point2D | null = null;
    for (const seg of pd?.segments ?? []) {
      if (seg.points.length === 0) continue;
      const pts = seg.points;
      const at = (p: Point2D) => !!last && Math.hypot(p.x - last.x, p.y - last.y) <= 1e-6;
      // Uzavřenou dráhu generátor začne u vrcholu nejbližšímu trysce
      const closed = pts.length >= 3 && Math.hypot(pts[0].x - pts[pts.length - 1].x, pts[0].y - pts[pts.length - 1].y) < 0.1;
      const joined = closed && !seg.fixed_start ? pts.some(at) : at(pts[0]);
      if (!joined) n++;
      last = closed && !seg.fixed_start && joined ? last : pts[pts.length - 1];
    }
    return n;
  }

  // ─── Ořez tras na sklo pro vykreslení ─────────────────────────────────────
  // Tiskne se jen část trasy na skle (Rust clip_paths_to_slide) — plátno kreslí
  // tytéž oříznuté dráhy, aby indexy živého náhledu tisku seděly s G-kódem.
  // Přesahující trasa se navíc kreslí celá slabě (co se netiskne).
  const _clipCache = new Map<number, { src: SubstratePaths; key: string; out: SubstratePaths }>();

  function pathsForDrawing(): { printPaths: SubstratePaths[]; ghostPaths: (SubstratePaths | null)[] } {
    const printPaths: SubstratePaths[] = [...paths];
    const ghostPaths: (SubstratePaths | null)[] = [];
    positions.forEach((pos, i) => {
      if (pos.is_prime) return;
      const ti = getTransformIdx(i, positions);
      const t = transforms[ti];
      const pd = shownPaths(ti);
      const raw = getPathBboxRaw(i);
      if (!t || !pd || !raw) return;
      printPaths[ti] = pd;
      const sy = t.scale_y ?? t.scale;
      const bb = computeWorldAABB(t.gui_dx, t.gui_dy, Math.max(t.scale, sy), t.rotation, t.cx, t.cy, raw);
      const inside =
        bb.minX >= pos.x && bb.maxX <= pos.x + pos.width && bb.minY >= pos.y && bb.maxY <= pos.y + pos.height;
      if (inside) return;
      ghostPaths[ti] = pd;
      // Během tažení (a při nerovnoměrném měřítku) stačí ořez kreslením na hranu skla
      if (dragOp || t.scale_y != null) return;
      const key = `${t.gui_dx},${t.gui_dy},${t.scale},${t.rotation},${pos.x},${pos.y},${pos.width},${pos.height}`;
      const hit = _clipCache.get(ti);
      if (hit && hit.src === pd && hit.key === key) {
        printPaths[ti] = hit.out;
      } else {
        const out = clipPathsToSlide(pd, t, pos);
        _clipCache.set(ti, { src: pd, key, out });
        printPaths[ti] = out;
      }
    });
    return { printPaths, ghostPaths };
  }

  // ─── Preview: předpočítané vzdálenosti segmentů pro přesný náhled tisku ───────
  let precomputedSegs: PreviewSegData[] = [];
  /** Počáteční bod tisku drah („pozice:dráha“) — pro šipky směru tisku. */
  let seqStartIdx = new Map<string, number>();

  async function recomputePreviewDist() {
    if (dragOp) return;
    const params = $projectStore.params;
    if (!params) return;
    try {
      const result = await compute_preview_segments(
        positions,
        paths,
        transforms,
        primePath,
        params,
        overrides,
        motionFromSettings($settingsStore)
      );
      precomputedSegs = result.segs;
      seqStartIdx = new Map(result.segs.map((sd) => [`${sd.slide_idx}:${sd.seg_idx}`, sd.start_idx]));
      previewTotalTime = result.total_time;
      scheduleDraw();
    } catch (e) {
      console.error("Výpočet časové osy náhledu selhal:", e);
    }
  }

  run(() => {
    paths; primePath; positions; transforms; overrides; $projectStore.params;
    $settingsStore.default_z_hop; $settingsStore.retract_nl; $settingsStore.retract_speed;
    $settingsStore.dot_dwell_ms; $settingsStore.calibration_factor;
    if (!dragOp) recomputePreviewDist();
  });

  // Při změně rozměrů podložky resetujeme kameru s odkladem 200 ms —
  // doUpdateLayout má debounce 150 ms, takže reset proběhne až po přepočtu pozic sklíček.
  // svelte-ignore state_referenced_locally -- vzor prev-value: záměrně počáteční hodnota
  let _prevBedMaxX = $state(bedMaxX);
  // svelte-ignore state_referenced_locally -- vzor prev-value: záměrně počáteční hodnota
  let _prevBedMaxY = $state(bedMaxY);
  let _bedResizeTimer: ReturnType<typeof setTimeout> = $state()!;
  run(() => {
    if (bedMaxX !== _prevBedMaxX || bedMaxY !== _prevBedMaxY) {
      _prevBedMaxX = bedMaxX;
      _prevBedMaxY = bedMaxY;
      if (ctx) {
        clearTimeout(_bedResizeTimer);
        _bedResizeTimer = setTimeout(resetCamera, 200);
      }
    }
  });

  run(() => {
    if (
      ctx &&
      positions &&
      transforms &&
      paths &&
      overrides &&
      showAxes !== undefined &&
      isMeasuring !== undefined &&
      selectedIndex !== undefined &&
      currentNozzle !== undefined &&
      printProgress !== undefined &&
      printCursor !== undefined &&
      focusedObject !== undefined &&
      $settingsStore
    ) {
      if (pendingObj && pendingObj.src !== paths[pendingObj.ti]) {
        pendingObj = null;
        livePaths = new Map();
      }
      scheduleDraw();
    }
  });

  onMount(() => {
    ctx = canvas.getContext("2d");

    const applySize = (w: number, h: number) => {
      if (w <= 0 || h <= 0) return;
      width = w;
      height = h;
      canvas.width = w;
      canvas.height = h;
    };

    const ro = new ResizeObserver((entries) => {
      const { width: w, height: h } = entries[0].contentRect;
      applySize(w, h);
      if (zoom === 1.0 && panX === 0) resetCamera();
      else draw();
    });
    ro.observe(canvas.parentElement!);

    // Po prvním framu zajistíme rozměry z BoundingClientRect — obchází případné
    // timing problémy kdy ResizeObserver vystřelí před finálním CSS grid layoutem.
    requestAnimationFrame(() => {
      const rect = canvas.parentElement!.getBoundingClientRect();
      applySize(rect.width, rect.height);
      if (zoom === 1.0 && panX === 0) resetCamera();
      else draw();
    });

    return () => ro.disconnect();
  });
</script>

<svelte:window onkeydown={handleKeyDown} onkeyup={handleKeyUp} />

<canvas
  bind:this={canvas}
  onmousedown={handleMouseDown}
  onmousemove={handleMouseMove}
  onmouseup={handleMouseUp}
  onmouseleave={handleMouseUp}
  onwheel={handleWheel}
  oncontextmenu={handleContextMenu}
  class="w-full h-full block"
  style="cursor: {drawTool
    ? 'crosshair'
    : dragOp === 'pan'
    ? 'grabbing'
    : dragOp === 'move' || dragOp === 'objMove'
      ? 'move'
      : hoverHandle >= 0
        ? 'crosshair'
        : selectedIndex >= 0
          ? 'default'
          : 'grab'}"
></canvas>
