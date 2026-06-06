<script lang="ts">
  import { onMount, createEventDispatcher } from "svelte";
  import type { LayoutPosition, SubstratePaths, Transform, Point2D, SlideOverride, PreviewSegData } from "../lib/tauri";
  import { compute_preview_segments } from "../lib/tauri";
  import { computeWorldAABB, clampGuidXY, getTransformIdx, type RawBbox } from "../lib/geometry";
  import { projectStore } from "../stores/projectStore";

  const dispatch = createEventDispatcher();

  export let bedMaxX = 250.0;
  export let bedMaxY = 210.0;
  export let positions: LayoutPosition[] = [];
  export let paths: SubstratePaths[] = [];
  export let primePath: SubstratePaths | null = null;
  export let transforms: Transform[] = [];
  export let overrides: Record<string, SlideOverride> = {};
  export let currentNozzle: Point2D | null = null;
  export let nozzleDiam = 0.4;
  export let selectedIndex = -1;
  export let showAxes = true;
  export let isMeasuring = false;
  export let measurePoints: { x: number; y: number }[] = [];
  export let printProgress = 100;

  let canvas: HTMLCanvasElement;
  let ctx: CanvasRenderingContext2D | null = null;

  // Camera
  let panX = 0;
  let panY = 0;
  let zoom = 1.0;

  // Interaction
  type DragOp = "pan" | "move" | "scale" | "rotate" | null;
  let dragOp: DragOp = null;
  /** 'scale' = corner squares, 'rotate' = corner circles */
  let transformMode: "scale" | "rotate" = "scale";
  let activeHandle = -1; // 0=TL 1=TR 2=BL 3=BR
  let hoverHandle = -1;
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

  $: {
    if (selectedIndex < 0) transformMode = "scale";
  }

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
      if (isMeasuring) {
        measurePoints = [];
        dispatch("measurePointsChange", measurePoints);
      }
      transformMode = "scale";
      scheduleDraw();
    }
    if (isMeasuring) scheduleDraw();
  }
  function handleKeyUp(e: KeyboardEvent) {
    ctrlDown = e.ctrlKey;
    altDown = e.altKey;
    shiftDown = e.shiftKey;
    if (isMeasuring || dragOp) scheduleDraw();
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

        // Binárním hledáním najde maximální scale ≤ wanted, při němž cesta (s anchor-based
        // pozicí) stále celá leží uvnitř skla. Zaručuje, že trasa nepřekročí žádnou hranu.
        const maxFitScale = (
          startT: Transform,
          anc: { x: number; y: number },
          wanted: number,
          p: LayoutPosition,
          raw: RawBbox
        ): number => {
          // Scaling down never needs clamping — path only shrinks toward anchor.
          // The clamp logic is unreliable when scaling down because handle positions
          // include ±nozzleDiam/2 padding which places anchors outside the inset,
          // causing fits(startT.scale) to return false and blocking all shrinking.
          if (wanted <= startT.scale) return Math.max(0.05, wanted);

          const nd = nozzleDiam / 2;
          const fits = (s: number) => {
            const ratio = s / startT.scale;
            const dx = anc.x - startT.cx - ratio * (anc.x - startT.gui_dx - startT.cx);
            const dy = anc.y - startT.cy - ratio * (anc.y - startT.gui_dy - startT.cy);
            const a = computeWorldAABB(dx, dy, s, startT.rotation, startT.cx, startT.cy, raw);
            // 1e-4 mm tolerance avoids false negatives from floating-point at the exact boundary.
            const eps = 1e-4;
            return (
              a.minX >= p.x + nd - eps &&
              a.maxX <= p.x + p.width - nd + eps &&
              a.minY >= p.y + nd - eps &&
              a.maxY <= p.y + p.height - nd + eps
            );
          };
          if (fits(wanted)) return wanted;
          let lo = 0.05,
            hi = wanted;
          if (!fits(lo)) {
            lo = startT.scale;
            if (!fits(lo)) return lo;
          }
          for (let i = 0; i < 12; i++) {
            const mid = (lo + hi) / 2;
            if (fits(mid)) lo = mid;
            else hi = mid;
          }
          return lo;
        };

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

      // Returns the largest factor in [0,1] s.t. the path (repositioned by clampGuidXY) still
      // fits entirely inside the slide. factor=1 means the full drag rotation is valid.
      const maxFitFactor = (
        startT: Transform,
        delta: number,
        p: LayoutPosition,
        raw: RawBbox
      ): number => {
        const nd = nozzleDiam / 2;
        const fits = (f: number): boolean => {
          const rot = (((startT.rotation + f * delta) % 360) + 360) % 360;
          const testT = { ...startT, rotation: rot };
          clampGuidXY(testT, p, raw, nozzleDiam);
          const a = computeWorldAABB(
            testT.gui_dx,
            testT.gui_dy,
            testT.scale,
            rot,
            testT.cx,
            testT.cy,
            raw
          );
          return (
            a.minX >= p.x + nd &&
            a.maxX <= p.x + p.width - nd &&
            a.minY >= p.y + nd &&
            a.maxY <= p.y + p.height - nd
          );
        };
        if (fits(1)) return 1;
        if (!fits(0)) return 0;
        let lo = 0,
          hi = 1;
        for (let i = 0; i < 12; i++) {
          const mid = (lo + hi) / 2;
          if (fits(mid)) lo = mid;
          else hi = mid;
        }
        return lo;
      };

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

  // ─── Transformuje bod z path-lokálního prostoru do world space ──────────────
  // Odpovídá transform_pt v Rustu; Y-flip zajišťuje canvas scale(zoom,-zoom).
  function tpt(
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

  // ─── Draw ──────────────────────────────────────────────────────────────────
  export function draw() {
    if (!ctx) return;

    let measTotal: number | null = null;
    let measCount = 0;

    ctx.fillStyle = "#0f172a";
    ctx.fillRect(0, 0, width, height);

    ctx.save();
    ctx.translate(panX, panY);
    ctx.scale(zoom, -zoom);

    // 1. Bed fill
    ctx.fillStyle = "#1e293b";
    ctx.fillRect(0, 0, bedMaxX, bedMaxY);

    // 2. Grid
    if (showAxes) {
      const minX = -panX / zoom,
        maxX = (width - panX) / zoom;
      const minY = (panY - height) / zoom,
        maxY = panY / zoom;

      const { minor: gMinor, major: gMajor } = gridDrawStep();

      ctx.strokeStyle = "#243148";
      ctx.lineWidth = 0.5 / zoom;
      ctx.beginPath();
      for (let x = Math.floor(minX / gMinor) * gMinor; x < maxX + gMinor; x += gMinor) {
        ctx.moveTo(x, minY);
        ctx.lineTo(x, maxY);
      }
      for (let y = Math.floor(minY / gMinor) * gMinor; y < maxY + gMinor; y += gMinor) {
        ctx.moveTo(minX, y);
        ctx.lineTo(maxX, y);
      }
      ctx.stroke();

      ctx.strokeStyle = "#334155";
      ctx.lineWidth = 1 / zoom;
      ctx.beginPath();
      for (let x = Math.floor(minX / gMajor) * gMajor; x < maxX + gMajor; x += gMajor) {
        ctx.moveTo(x, minY);
        ctx.lineTo(x, maxY);
      }
      for (let y = Math.floor(minY / gMajor) * gMajor; y < maxY + gMajor; y += gMajor) {
        ctx.moveTo(minX, y);
        ctx.lineTo(maxX, y);
      }
      ctx.stroke();
    }

    // 3. Bed border
    ctx.strokeStyle = "#64748b";
    ctx.lineWidth = 1.5 / zoom;
    ctx.strokeRect(0, 0, bedMaxX, bedMaxY);

    // 4. Sklíčka + trasy
    // Preview cursor: mapuje printProgress (0-100%) na přesnou vzdálenostní pozici v trase
    let cursor: { slideIdx: number; segIdx: number; ptIdx: number; fracT: number } | null = null;
    if (printProgress < 100 && precomputedSegs.length > 0 && totalPreviewDist > 0) {
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
          cursor = { slideIdx: sd.slide_idx, segIdx: sd.seg_idx, ptIdx, fracT };
          break;
        }
      }
    }
    // Pozice preview trysky ve world space (nastavuje se při kreslení kurzorového segmentu)
    let previewNozzleWX = 0, previewNozzleWY = 0, hasPreviewNozzle = false;
    let previewDone = false;

    for (let i = 0; i < positions.length; i++) {
      const pos = positions[i];
      const tidx = getTransformIdx(i, positions);
      const isSelected = i === selectedIndex;

      const slideColor = pos.is_prime ? "#f97316" : isSelected ? "#3b82f6" : "#94a3b8";

      let glassW = pos.width;
      let glassH = pos.height;
      if (pos.is_prime && overrides["-1"]?.glass_type === "vzorkové") {
        glassW = $projectStore.params?.slide_w ?? glassW;
        glassH = $projectStore.params?.slide_h ?? glassH;
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
          let fracWX: number | null = null, fracWY: number | null = null;

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

    // 6. Handles trasy pro vybrané sklíčko
    if (
      selectedIndex >= 0 &&
      selectedIndex < positions.length &&
      !positions[selectedIndex]?.is_prime
    ) {
      const handles = getPathHandles(selectedIndex);
      if (handles.length === 4) {
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
          const tidx = getTransformIdx(selectedIndex, positions);
          const t = transforms[tidx];
          if (t) {
            const cx = t.gui_dx + t.cx;
            const cy = t.gui_dy + t.cy;
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
    }

    // 7. Měřidlo
    if (isMeasuring && measurePoints.length > 0) {
      const raw = screenToWorld(mouseX, mouseY);
      const snapped = applyMeasureSnap(raw.x, raw.y);
      const allPts = [...measurePoints, snapped];

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

      const isSnapped = !altDown && (ctrlDown || Math.hypot(snapped.x - raw.x, snapped.y - raw.y) > 0.001);
      if (isSnapped) {
        ctx.strokeStyle = ctrlDown ? "#38bdf8" : "#a78bfa";
        ctx.lineWidth = 1 / zoom;
        const r2 = 5 / zoom;
        ctx.beginPath();
        ctx.arc(snapped.x, snapped.y, r2, 0, Math.PI * 2);
        ctx.stroke();
        ctx.beginPath();
        ctx.moveTo(snapped.x - r2 * 1.5, snapped.y);
        ctx.lineTo(snapped.x + r2 * 1.5, snapped.y);
        ctx.moveTo(snapped.x, snapped.y - r2 * 1.5);
        ctx.lineTo(snapped.x, snapped.y + r2 * 1.5);
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
        const lbl2 = `${d.toFixed(1)} mm`;
        const tw2 = ctx.measureText(lbl2).width;
        ctx.fillStyle = "rgba(0,0,0,0.7)";
        ctx.fillRect(mx - tw2 / 2 - 2 / zoom, my - 6 / zoom, tw2 + 4 / zoom, 10 / zoom);
        ctx.fillStyle = "#fde68a";
        ctx.fillText(lbl2, mx, my);
      }
      ctx.restore();
      measTotal = totalDist;
      measCount = allPts.length - 1;
    }

    // 8. Tryska (reálná — červená při tisku)
    if (currentNozzle) {
      ctx.fillStyle = "#ef4444";
      ctx.beginPath();
      ctx.arc(currentNozzle.x, currentNozzle.y, nozzleDiam / 2 + 1 / zoom, 0, Math.PI * 2);
      ctx.fill();
    }

    // 8b. Preview tryska (cyan — zobrazená při náhledu, ne při tisku)
    if (hasPreviewNozzle && !currentNozzle) {
      const r = nozzleDiam / 2 + 1.5 / zoom;
      ctx.beginPath();
      ctx.arc(previewNozzleWX, previewNozzleWY, r, 0, Math.PI * 2);
      ctx.fillStyle = "rgba(34,211,238,0.25)";
      ctx.fill();
      ctx.strokeStyle = "#22d3ee";
      ctx.lineWidth = 1.2 / zoom;
      ctx.stroke();
      // Kříž v centru trysky
      ctx.beginPath();
      ctx.moveTo(previewNozzleWX - r * 0.7, previewNozzleWY);
      ctx.lineTo(previewNozzleWX + r * 0.7, previewNozzleWY);
      ctx.moveTo(previewNozzleWX, previewNozzleWY - r * 0.7);
      ctx.lineTo(previewNozzleWX, previewNozzleWY + r * 0.7);
      ctx.stroke();
    }

    ctx.restore();

    // ── Osy — popisky v obrazovkových souřadnicích (vždy viditelné) ──────────
    if (showAxes) {
      const minX2 = -panX / zoom,
        maxX2 = (width - panX) / zoom;
      const minY2 = (panY - height) / zoom,
        maxY2 = panY / zoom;
      const { major: gMajor2 } = gridDrawStep();
      const fmtVal = (v: number) => Number(v.toFixed(4)).toString();
      const FS = 13;
      ctx.font = `bold ${FS}px sans-serif`;

      // X-osa: pokud je spodní okraj bedu viditelný, ukotvit popisky těsně pod něj
      const bedBottomSY = panY; // world y=0 → screen y=panY
      const xAnchored = bedBottomSY > 20 && bedBottomSY < height - 50;
      const xAxisY = xAnchored ? bedBottomSY + 16 : height - 42;
      ctx.textAlign = "center";
      ctx.textBaseline = "bottom";
      for (let wx = Math.floor(minX2 / gMajor2) * gMajor2; wx < maxX2 + gMajor2; wx += gMajor2) {
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
      for (let wy = Math.floor(minY2 / gMajor2) * gMajor2; wy < maxY2 + gMajor2; wy += gMajor2) {
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

    // ── Obrazovkové overlaye ─────────────────────────────────────────────
    if (measTotal !== null) {
      const count = measCount;
      ctx.font = "bold 11px sans-serif";
      const lbl3 = count > 1 ? `Celkem: ${measTotal.toFixed(1)} mm` : `${measTotal.toFixed(2)} mm`;
      const tw3 = ctx.measureText(lbl3).width;
      ctx.fillStyle = "rgba(0,0,0,0.7)";
      ctx.fillRect(10, 10, tw3 + 16, 22);
      ctx.fillStyle = "#fde68a";
      ctx.fillText(lbl3, 18, 25);
    }

    // ── Pravý horní overlay — info o vybraném prvku ──────────────────────────
    if (
      selectedIndex >= 0 &&
      selectedIndex < positions.length &&
      !positions[selectedIndex]?.is_prime
    ) {
      const _si = getTransformIdx(selectedIndex, positions);
      const _st = transforms[_si];
      const _spos = positions[selectedIndex];
      const _spd = paths[_si];
      if (_st) {
        // Délka trasy — součet vzdáleností po sobě jdoucích bodů × aktuální scale
        let _pathLen = 0;
        if (_spd) {
          for (const seg of _spd.segments)
            for (let k = 1; k < seg.points.length; k++)
              _pathLen += Math.hypot(
                seg.points[k].x - seg.points[k - 1].x,
                seg.points[k].y - seg.points[k - 1].y
              );
          _pathLen *= _st.scale;
        }

        // Pozice středu trasy relativně k levému dolnímu rohu skla
        const _cx = (_st.gui_dx + _st.cx - _spos.x).toFixed(1);
        const _cy = (_st.gui_dy + _st.cy - _spos.y).toFixed(1);

        // Rozměry trasy v lokálním prostoru × scale (nezávislé na rotaci)
        const _raw = getPathBboxRaw(selectedIndex);
        const _pw = _raw ? (_raw.mxX - _raw.mnX) * _st.scale : 0;
        const _ph = _raw ? (_raw.mxY - _raw.mnY) * _st.scale : 0;
        const _area = _pw * _ph;

        const _lines: string[] = [
          `X: ${_cx} mm    Y: ${_cy} mm`,
          `Rotace: ${_st.rotation.toFixed(1)}°`,
          `Rozměr: ${_pw.toFixed(1)} × ${_ph.toFixed(1)} mm  (${_area.toFixed(1)} mm²)`,
          `Trasa: ${_pathLen.toFixed(1)} mm`,
        ];
        if (measTotal !== null) _lines.push(`Pravítko: ${measTotal.toFixed(1)} mm`);

        ctx.font = "bold 11px sans-serif";
        const _lh = 17,
          _pad = 8;
        const _bw = Math.max(..._lines.map((l) => ctx!.measureText(l).width)) + _pad * 2;
        const _bh = _lines.length * _lh + _pad;
        const _bx = width - _bw - 10,
          _by = 54;

        ctx.fillStyle = "rgba(0,0,0,0.65)";
        ctx.fillRect(_bx, _by, _bw, _bh);
        ctx.fillStyle = "#e2e8f0";
        for (let li = 0; li < _lines.length; li++)
          ctx.fillText(_lines[li], _bx + _pad, _by + _pad + li * _lh + 3);
      }
    }

    // Jemná mřížka při CTRL
    if (ctrlDown && (dragOp === "move" || dragOp === null) && selectedIndex >= 0) {
      const g = gridSnapSize();
      ctx.save();
      ctx.translate(panX, panY);
      ctx.scale(zoom, -zoom);
      ctx.strokeStyle = "rgba(56,189,248,0.18)";
      ctx.lineWidth = 0.25 / zoom;
      const minX2 = -panX / zoom,
        maxX2 = (width - panX) / zoom;
      const minY2 = (panY - height) / zoom,
        maxY2 = panY / zoom;
      ctx.beginPath();
      for (let x = Math.floor(minX2 / g) * g; x < maxX2; x += g) {
        ctx.moveTo(x, minY2);
        ctx.lineTo(x, maxY2);
      }
      for (let y = Math.floor(minY2 / g) * g; y < maxY2; y += g) {
        ctx.moveTo(minX2, y);
        ctx.lineTo(maxX2, y);
      }
      ctx.stroke();
      ctx.restore();
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

  $: {
    paths; primePath; positions; transforms;
    if (!dragOp) recomputePreviewDist();
  }

  // Při změně rozměrů podložky resetujeme kameru s odkladem 200 ms —
  // doUpdateLayout má debounce 150 ms, takže reset proběhne až po přepočtu pozic sklíček.
  let _prevBedMaxX = bedMaxX;
  let _prevBedMaxY = bedMaxY;
  let _bedResizeTimer: ReturnType<typeof setTimeout>;
  $: if (bedMaxX !== _prevBedMaxX || bedMaxY !== _prevBedMaxY) {
    _prevBedMaxX = bedMaxX;
    _prevBedMaxY = bedMaxY;
    if (ctx) {
      clearTimeout(_bedResizeTimer);
      _bedResizeTimer = setTimeout(resetCamera, 200);
    }
  }

  $: {
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
  }

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

<svelte:window on:keydown={handleKeyDown} on:keyup={handleKeyUp} />

<canvas
  bind:this={canvas}
  on:mousedown={handleMouseDown}
  on:mousemove={handleMouseMove}
  on:mouseup={handleMouseUp}
  on:mouseleave={handleMouseUp}
  on:wheel={handleWheel}
  on:contextmenu={handleContextMenu}
  class="w-full h-full block"
  style="cursor: {dragOp === 'pan'
    ? 'grabbing'
    : dragOp === 'move'
      ? 'move'
      : hoverHandle >= 0
        ? 'crosshair'
        : selectedIndex >= 0
          ? 'default'
          : 'grab'}"
></canvas>
