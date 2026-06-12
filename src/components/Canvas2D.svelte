<script lang="ts">
  import { run } from 'svelte/legacy';

  import { onMount, createEventDispatcher } from "svelte";
  import type { LayoutPosition, SubstratePaths, Transform, Point2D, SlideOverride, PreviewSegData } from "../lib/tauri";
  import { compute_preview_segments } from "../lib/tauri";
  import { computeWorldAABB, clampGuidXY, getTransformIdx, type RawBbox } from "../lib/geometry";
  import { wasm_max_fit_rotation_factor, wasm_max_fit_scale } from "../lib/dpiWasm";
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
  } from "../lib/canvas2dDraw";
  import { projectStore, type DrawnShape } from "../stores/projectStore";

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
    drawTool = null
  }: Props = $props();

  let canvas: HTMLCanvasElement = $state()!;
  let ctx: CanvasRenderingContext2D | null = $state(null);

  // Camera
  let panX = 0;
  let panY = 0;
  let zoom = 1.0;

  // Interaction
  type DragOp = "pan" | "move" | "scale" | "rotate" | null;
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
    return ctrlDown ? { x: snapToGrid(w.x), y: snapToGrid(w.y) } : w;
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
      return { kind: "line", x1: a.x, y1: a.y, x2: b.x, y2: b.y };
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
      minY = t.cy + (mny - t.cy) * t.scale - r;
      maxY = t.cy + (mxy - t.cy) * t.scale + r;
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

    // Inverze transformace tpt: world → path lokální souřadnice
    const cx_w = t.gui_dx + t.cx;
    const cy_w = t.gui_dy + t.cy;
    const dx = wx - cx_w;
    const dy = wy - cy_w;
    const rot = (t.rotation * Math.PI) / 180;
    const cos_r = Math.cos(rot), sin_r = Math.sin(rot);
    const px = t.cx + (dx * cos_r - dy * sin_r) / t.scale;
    const py = t.cy + (dx * sin_r + dy * cos_r) / t.scale;

    const tol = nozzleDiam;
    return px >= raw.mnX - tol && px <= raw.mxX + tol && py >= raw.mnY - tol && py <= raw.mxY + tol;
  }

  function hitTestHandle(wx: number, wy: number): number {
    if (selectedIndex < 0) return -1;
    const handles = getPathHandles(selectedIndex);
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
          const [ptx, pty] = tpt(pt.x, pt.y, t.gui_dx, t.gui_dy, t.scale, t.rotation, t.cx, t.cy);
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

    if (e.button === 0) {
      if (drawTool) {
        shapeStart = drawPointFromMouse(sx, sy);
        shapeCurr = { ...shapeStart };
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
          if (dragOp === "scale") {
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
          // Vybrat sklíčko — drag se spustí až při kliknutí na již vybrané
          dispatch("slideSelected", hitIdx);
          transformMode = "scale";
          return;
        }
      } else {
        // Klik do prázdna — odselektovat
        if (hitIdx !== selectedIndex) {
          dispatch("slideSelected", -1);
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
      const oppositeIdx = 3 - activeHandle; // TL↔BR, TR↔BL
      // SHIFT = anchor ve středu objektu, jinak = protější roh
      const anchor = shiftDown
        ? { x: st.gui_dx + st.cx, y: st.gui_dy + st.cy }
        : initHandles[oppositeIdx];

      const initDist = Math.hypot(
        initHandles[activeHandle].x - anchor.x,
        initHandles[activeHandle].y - anchor.y
      );
      const currDist = Math.hypot(w.x - anchor.x, w.y - anchor.y);

      if (initDist > 0.01) {
        let newScale = st.scale * (currDist / initDist);
        if (ctrlDown) newScale = Math.round(newScale * 10) / 10;
        newScale = Math.max(0.05, Math.min(10.0, newScale));
        const scaleRatio = newScale / st.scale;

        const maxFitScale = (
          startT: Transform,
          anc: { x: number; y: number },
          wanted: number,
          p: LayoutPosition,
          raw: RawBbox
        ): number => wasm_max_fit_scale(
          startT.gui_dx, startT.gui_dy, startT.scale, startT.rotation, startT.cx, startT.cy,
          anc.x, anc.y, wanted,
          p.x, p.y, p.width, p.height,
          raw.mnX, raw.mxX, raw.mnY, raw.mxY,
          nozzleDiam
        );

        // Přepočítá gui_dx/gui_dy tak, aby anchor zůstal na místě:
        // anchor_world = (dx+cx) + ratio*(anchor-dx0-cx)  →  dx = anchor-cx - ratio*(anchor-dx0-cx)
        const applyAnchored = (
          ot: Transform,
          startT: Transform,
          anc: { x: number; y: number },
          targetScale: number,
          p: LayoutPosition,
          idx: number
        ) => {
          const raw = getPathBboxRaw(idx);
          const clampedScale = raw ? maxFitScale(startT, anc, targetScale, p, raw) : targetScale;
          const r = clampedScale / startT.scale;
          ot.gui_dx = anc.x - startT.cx - r * (anc.x - startT.gui_dx - startT.cx);
          ot.gui_dy = anc.y - startT.cy - r * (anc.y - startT.gui_dy - startT.cy);
          ot.scale = clampedScale;
        };

        if (altDown) {
          positions.forEach((p, i) => {
            if (p.is_prime) return;
            const oi = getTransformIdx(i, positions);
            const ot = transforms[oi];
            const startT = dragStartTransformsAll.get(oi);
            if (!ot || !startT) return;
            const targetScale = Math.max(0.05, Math.min(10.0, startT.scale * scaleRatio));
            const ih = getPathHandles(i, startT);
            const anc =
              ih.length >= 4 && !shiftDown
                ? ih[oppositeIdx]
                : { x: startT.gui_dx + startT.cx, y: startT.gui_dy + startT.cy };
            applyAnchored(ot, startT, anc, targetScale, p, i);
            _dragModifiedIndices.add(oi);
          });
        } else {
          applyAnchored(t, st, anchor, newScale, pos, dragSlideIdx);
          _dragModifiedIndices.add(tidx);
        }
        scheduleDraw();
      }
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

      const maxFitFactor = (
        startT: Transform,
        delta: number,
        p: LayoutPosition,
        raw: RawBbox
      ): number => wasm_max_fit_rotation_factor(
        startT.gui_dx, startT.gui_dy, startT.scale, startT.cx, startT.cy,
        startT.rotation, delta,
        p.x, p.y, p.width, p.height,
        raw.mnX, raw.mxX, raw.mnY, raw.mxY,
        nozzleDiam
      );

      const rotFromFactor = (startRot: number, factor: number): number =>
        (((startRot + factor * rawDeltaDeg) % 360) + 360) % 360;

      if (altDown) {
        // Most-restrictive factor across all slides so they all rotate by the same amount.
        let factor = 1.0;
        positions.forEach((p, i) => {
          if (p.is_prime) return;
          const oi = getTransformIdx(i, positions);
          const startTi = dragStartTransformsAll.get(oi);
          if (!startTi) return;
          const rawI = getPathBboxRaw(i);
          if (rawI) factor = Math.min(factor, maxFitFactor(startTi, rawDeltaDeg, p, rawI));
        });
        positions.forEach((p, i) => {
          if (p.is_prime) return;
          const oi = getTransformIdx(i, positions);
          const ot = transforms[oi];
          const startTi = dragStartTransformsAll.get(oi);
          if (!ot || !startTi) return;
          let rot = rotFromFactor(startTi.rotation, factor);
          if (ctrlDown) rot = Math.round(rot / 15) * 15;
          ot.rotation = rot;
          const rawI = getPathBboxRaw(i);
          if (rawI) clampGuidXY(ot, p, rawI, nozzleDiam);
          _dragModifiedIndices.add(oi);
        });
      } else {
        const raw = getPathBboxRaw(dragSlideIdx);
        const factor = raw ? maxFitFactor(dragStartTransform, rawDeltaDeg, pos, raw) : 1;
        let newRot = rotFromFactor(dragStartTransform.rotation, factor);
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

    if (
      (dragOp === "scale" || dragOp === "rotate") &&
      dragSlideIdx >= 0 &&
      dragStartTransform !== null
    ) {
      const tidx = getTransformIdx(dragSlideIdx, positions);
      if (tidx >= 0) {
        const t = transforms[tidx];
        if (t) {
          const scaleChanged = Math.abs(t.scale - dragStartTransform.scale) > 0.001;
          const rotChanged = Math.abs(t.rotation - dragStartTransform.rotation) > 0.01;
          if (scaleChanged || rotChanged) {
            dispatch("pathRebuildNeeded", { slideIdx: tidx, scale: t.scale, rotation: t.rotation });
          }
        }
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
    const cursor = computePreviewCursor(printProgress, precomputedSegs, totalPreviewDist);
    const previewNozzle = drawSlidesAndPaths(ctx, {
      positions,
      paths,
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
    });

    // 6. Handles trasy pro vybrané sklíčko
    if (
      selectedIndex >= 0 &&
      selectedIndex < positions.length &&
      !positions[selectedIndex]?.is_prime
    ) {
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

    // 8. Náhled právě kresleného tvaru
    const sp = shapePreview();
    if (sp) drawShapePreview(ctx, sp.pts, sp.closed, sp.label, zoom);

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
    if ($projectStore.totalDist > 0) {
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
      statsBoxBottom = drawOverlayBox(ctx, statLines, width, 54, "#7dd3fc");
    }

    // ── Pravý horní overlay — info o vybraném prvku ──────────────────────────
    if (
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

        // Rozměry trasy v lokálním prostoru × scale (nezávislé na rotaci)
        const rawBb = getPathBboxRaw(selectedIndex);
        const pw = rawBb ? (rawBb.mxX - rawBb.mnX) * st.scale : 0;
        const ph = rawBb ? (rawBb.mxY - rawBb.mnY) * st.scale : 0;

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

  // ─── Preview: předpočítané vzdálenosti segmentů pro přesný náhled tisku ───────
  let precomputedSegs: PreviewSegData[] = [];
  let totalPreviewDist = 0;

  async function recomputePreviewDist() {
    if (dragOp) return;
    const result = await compute_preview_segments(positions, paths, transforms, primePath);
    precomputedSegs = result.segs;
    totalPreviewDist = result.total_dist;
  }

  run(() => {
    paths; primePath; positions; transforms;
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
      printProgress !== undefined
    ) {
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
    : dragOp === 'move'
      ? 'move'
      : hoverHandle >= 0
        ? 'crosshair'
        : selectedIndex >= 0
          ? 'default'
          : 'grab'}"
></canvas>
