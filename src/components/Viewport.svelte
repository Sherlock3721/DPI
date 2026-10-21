<script lang="ts">
  import { run } from "svelte/legacy";

  import { onMount, createEventDispatcher } from "svelte";
  import type { LayoutPosition, SubstratePaths, Transform, Point2D, SlideOverride, PreviewSegData, PrintCursor, ObjectTransform, PathObject } from "../lib/tauri";
  import { compute_preview_segments, motionFromSettings } from "../lib/tauri";
  import { bboxOfPaths, computeWorldAABB, clampGuidXY, clipPathsToSlide, getTransformIdx, processPathsWasm, type RawBbox } from "../lib/geometry";
  import { tpt, tptFn, computePreviewCursor, previewNozzleWorld, printedPoint, isDotSeg, pathFrameOf, frameFn, type PreviewCursor } from "../lib/pathMath";
  import type { MergePreview } from "../lib/nodeMerge";
  import { projectStore, sliceParamsFor, type DrawnShape, type LineAnchor } from "../stores/projectStore";
  import { settingsStore } from "../stores/settingsStore";
  import { viewportRenderer } from "../stores/viewportStore";
  import { IDENTITY_XF, baseCorners, designToWorld, objectPivot, worldToDesign, xfApply, xfInverse, type SlideFrame } from "../lib/objectTransform";
  import { OrbitCamera, PITCH_MIN, PITCH_TOP, type Vec3 } from "../lib/viewport/camera";
  import * as hud from "../lib/viewport/hud";
  import { SceneSync, encodeFrame, hexToRgb, type SlideScene, type SlideFrame as SlideFrameData, type RGB } from "../lib/viewport/sceneSync";
  import { createHost, hostOrder, type ViewportHost, type HostKind } from "../lib/viewport/host";
  import { printHeights, heightsKey, glassThickness, beadSize, type HeightsMap } from "../lib/viewport/heights";

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
    mergePreview = null,
  }: Props = $props();

  let container: HTMLDivElement = $state()!;
  let sceneHolder: HTMLDivElement = $state()!;
  let hudCanvas: HTMLCanvasElement = $state()!;
  let hudCtx: CanvasRenderingContext2D | null = null;

  // ─── Kamera a renderer ────────────────────────────────────────────────────
  const cam = new OrbitCamera();
  let width = 0;
  let height = 0;
  let dpr = 1;

  let host: ViewportHost | null = null;
  let hostKind: HostKind | null = $state(null);
  let hostError = $state("");
  let ready = $state(false);
  const sync = new SceneSync();

  // Interaction
  type DragOp = "pan" | "move" | "scale" | "rotate" | "objMove" | "objScale" | "objRotate" | null;
  let dragOp: DragOp = $state(null);
  /** Navigace kamerou (prostřední tlačítko / gizmo). */
  type NavOp = {
    kind: "orbit" | "pan" | "zoom";
    sx: number;
    sy: number;
    start: ReturnType<OrbitCamera["state"]>;
    grab: Pt | null;
    /** Při otáčení mimo osu přepnout do perspektivy. */
    persp: boolean;
  } | null;
  let navOp: NavOp = $state(null);
  /** 'scale' = corner squares, 'rotate' = corner circles */
  let transformMode: "scale" | "rotate" = $state("scale");
  let activeHandle = -1; // 0=TL 1=TR 2=BL 3=BR
  let hoverHandle = $state(-1);
  let hoverGizmo: hud.GizmoAxis | "ring" | null = $state(null);
  let dragSlideIdx = -1; // independent of selectedIndex prop lag
  let dragStartWorld = { x: 0, y: 0 };
  let dragStartTransform: Transform | null = null;
  let dragStartTransformsAll: Map<number, Transform> = new Map();
  // Indexy transformů mutovaných během dragu — store se aktualizuje až na mouseup
  let _dragModifiedIndices: Set<number> = new Set();
  /** Pracovní rovina tažení (vršek taženého skla) a px/mm zmražené na začátku. */
  let dragZ = 0;
  let dragPxPerMm = 1;

  // Pan (levé tlačítko do prázdna)
  let panStart: { sx: number; sy: number; target: Vec3; grab: Pt | null } | null = null;

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
  const DOUBLE_CLICK_MS = 300;

  let mouseX = 0;
  let mouseY = 0;
  let ctrlDown = false;
  let altDown = false;
  let shiftDown = false;
  /** Poslední vstup (interakce) — nativní hostitel kreslí do té doby nižší rozlišení. */
  let lastInteraction = 0;
  let _refineTimer: ReturnType<typeof setTimeout> | undefined;

  /** Hodnota odvozená z drah, spočtená jednou pro každý objekt drah — dráhy
   *  jsou neměnné (změna = nový objekt), takže se nemusí procházet všechny
   *  body při každém snímku. */
  function memoByPaths<T>(fn: (pd: SubstratePaths) => T): (pd: SubstratePaths) => T {
    const cache = new WeakMap<SubstratePaths, T>();
    return (pd) => {
      if (!cache.has(pd)) cache.set(pd, fn(pd));
      return cache.get(pd)!;
    };
  }
  const cachedBbox = memoByPaths(bboxOfPaths);

  run(() => {
    if (selectedIndex < 0) transformMode = "scale";
  });

  // ─── Výšky (tloušťka skel, housenky) ──────────────────────────────────────
  // Přepočet jen po změně store (příznak nastaví efekt níže) a jen při jiném
  // obsahu — předvolba skla mění params.slide_z na místě, identita nestačí.
  let _heights: HeightsMap = {};
  let _heightsKey = "";
  let heightsDirty = true;

  function heights(): HeightsMap {
    if (heightsDirty) {
      heightsDirty = false;
      const ps = $projectStore;
      const cal = $settingsStore.calibration_factor ?? 0.323877;
      const key = JSON.stringify([ps.params, ps.overrides ?? {}, cal]);
      if (key !== _heightsKey) {
        _heightsKey = key;
        _heights = ps.params ? printHeights(ps.params, ps.overrides ?? {}, cal) : {};
      }
    }
    return _heights;
  }

  /** Vršek skla pozice (mm nad deskou). */
  function glassTop(posIdx: number): number {
    const pos = positions[posIdx];
    if (!pos) return 0;
    const fallback = $projectStore.params?.slide_z ?? 1;
    return glassThickness(heights(), heightsKey(pos.is_prime, getTransformIdx(posIdx, positions)), fallback);
  }

  /** Vršek skla pod bodem (world), jinak deska. */
  function surfaceZAt(x: number, y: number): number {
    for (let i = 0; i < positions.length; i++) {
      const p = positions[i];
      if (x >= p.x && x <= p.x + p.width && y >= p.y && y <= p.y + p.height) return glassTop(i);
    }
    return 0;
  }

  /** Rozměr skla k vykreslení (odpliv „vzorkové“ má rozměr vzorkového skla). */
  function glassSize(pos: LayoutPosition): [number, number] {
    const params = $projectStore.params;
    if (pos.is_prime && overrides["-1"]?.glass_type === "vzorkové" && params) return [params.slide_w, params.slide_h];
    return [pos.width, pos.height];
  }

  // ─── Kamera ───────────────────────────────────────────────────────────────
  /** Tloušťka tiskové podložky (mm) — vršek v z = 0, na něm stojí skla. */
  const BED_THICKNESS = 2;

  function updateSceneBounds() {
    const ext = Math.max(bedMaxX, bedMaxY) * 2;
    let top = 0;
    for (let i = 0; i < positions.length; i++) top = Math.max(top, glassTop(i));
    cam.sceneMin = [-ext, -ext, -BED_THICKNESS - 1];
    cam.sceneMax = [bedMaxX + ext, bedMaxY + ext, top + 12];
  }

  /** Pohled shora na celou plochu (dřívější 2D výchozí pohled, okraj 40 px). */
  export function resetCamera(animate = false) {
    if (width === 0 || height === 0) return;
    if (animate && cam.initialized) {
      const tmp = cam.clone();
      tmp.fitTop(0, 0, bedMaxX, bedMaxY, 40);
      cam.animateTo({ ...tmp.state(), projection: "ortho" });
    } else {
      cam.fitTop(0, 0, bedMaxX, bedMaxY, 40);
    }
    scheduleDraw();
  }

  /** Celá plocha se zachovanou orientací (Home); v pohledu shora = resetCamera. */
  export function fitAll() {
    if (!cam.initialized || cam.isTopView()) return resetCamera(cam.initialized);
    cam.animateTo(cam.fitBox([0, 0, 0], [bedMaxX, bedMaxY, maxGlassTop()], 40));
    scheduleDraw();
  }

  function maxGlassTop(): number {
    let z = 0;
    for (let i = 0; i < positions.length; i++) z = Math.max(z, glassTop(i));
    return z;
  }

  /** Přiblížit na výběr (objekt, jinak sklo) se zachovanou orientací. */
  export function frameSelection() {
    if (selectedIndex < 0 || selectedIndex >= positions.length || width === 0 || height === 0) return;
    const pos = positions[selectedIndex];
    const z = glassTop(selectedIndex);
    const eo = editableObject();
    let mn: Vec3 = [pos.x, pos.y, 0],
      mx: Vec3 = [pos.x + pos.width, pos.y + pos.height, z];
    if (eo) {
      const c = objectHandles(eo);
      mn = [Math.min(...c.map((p) => p.x)), Math.min(...c.map((p) => p.y)), z];
      mx = [Math.max(...c.map((p) => p.x)), Math.max(...c.map((p) => p.y)), z];
    }
    cam.animateTo(cam.fitBox(mn, mx, 30));
    scheduleDraw();
  }

  /** Zpětná kompatibilita (tlačítko „Na sklíčko“). */
  export function centerOnSlide() {
    frameSelection();
  }

  /** Směry šipek ve world: osa světa nejbližší směru na obrazovce. */
  export function arrowAxes(): { up: [number, number]; right: [number, number] } {
    const q = Math.round(cam.yaw / (Math.PI / 2));
    const a = q * (Math.PI / 2);
    const r: [number, number] = [Math.round(Math.cos(a)), Math.round(Math.sin(a))];
    return { right: r, up: [-r[1], r[0]] };
  }

  function minScale(): number {
    const margin = 40;
    return Math.min((width - margin * 2) / bedMaxX, (height - margin * 2) / bedMaxY) * 0.7;
  }

  function screenToWorld(sx: number, sy: number, z = dragZ): Pt {
    const p = cam.screenToPlane(sx, sy, z);
    if (p) lastValidWorld = p;
    return p ?? lastValidWorld;
  }
  let lastValidWorld: Pt = { x: 0, y: 0 };

  /** Paprsek × kvádr skla (slab metoda) — vzdálenost zásahu, jinak null. */
  function rayBox(o: Vec3, d: Vec3, mn: Vec3, mx: Vec3): number | null {
    let t0 = -Infinity,
      t1 = Infinity;
    for (let k = 0; k < 3; k++) {
      if (Math.abs(d[k]) < 1e-12) {
        if (o[k] < mn[k] || o[k] > mx[k]) return null;
        continue;
      }
      let a = (mn[k] - o[k]) / d[k],
        b = (mx[k] - o[k]) / d[k];
      if (a > b) [a, b] = [b, a];
      t0 = Math.max(t0, a);
      t1 = Math.min(t1, b);
      if (t0 > t1) return null;
    }
    return t1 < 0 ? null : Math.max(t0, 0);
  }

  /** Sklo pod kurzorem: nejbližší zásah paprsku do kvádru skla. */
  function pickSlide(sx: number, sy: number): number {
    const { o, d } = cam.ray(sx, sy);
    let best = -1,
      bestT = Infinity;
    for (let i = positions.length - 1; i >= 0; i--) {
      const p = positions[i];
      const t = rayBox(o, d, [p.x, p.y, 0], [p.x + p.width, p.y + p.height, glassTop(i)]);
      if (t !== null && t < bestT - 1e-9) {
        bestT = t;
        best = i;
      }
    }
    return best;
  }

  /** Bod pro zoom kolečkem: vršek skla pod kurzorem → deska → rovina targetu. */
  function wheelAnchor(sx: number, sy: number): Vec3 | null {
    const i = pickSlide(sx, sy);
    const z = i >= 0 ? glassTop(i) : 0;
    const p = cam.screenToPlane(sx, sy, z, true) ?? cam.screenToPlane(sx, sy, cam.target[2], true);
    return p ? [p.x, p.y, p ? z : cam.target[2]] : null;
  }

  function markInteraction() {
    lastInteraction = performance.now();
    clearTimeout(_refineTimer);
    // Po klidu se snímek doostří na plné rozlišení
    _refineTimer = setTimeout(scheduleDraw, 160);
  }

  // ─── Vycentrování po kliknutí ─────────────────────────────────────────────
  // Pivot (target) se přesune na vybraný objekt až po okně dvojkliku: druhý
  // klik na stejné místo (výběr objektu, přepnutí rotace) musí trefit totéž.
  let pendingRecenter: Vec3 | null = null;
  let _recenterTimer: ReturnType<typeof setTimeout> | undefined;

  function scheduleRecenter(center: Vec3 | null) {
    clearTimeout(_recenterTimer);
    pendingRecenter = null;
    if (!center || !($settingsStore.center_on_click ?? true)) return;
    pendingRecenter = center;
    _recenterTimer = setTimeout(() => {
      if (!pendingRecenter || dragOp || navOp) return;
      cam.animateTo({ target: pendingRecenter });
      pendingRecenter = null;
      scheduleDraw();
    }, DOUBLE_CLICK_MS);
  }

  /** Další stisk: čekající vycentrování se zahodí, běžící animace dokončí. */
  function settleCamera() {
    clearTimeout(_recenterTimer);
    pendingRecenter = null;
    cam.finishAnimation();
  }

  function slideCenter(posIdx: number): Vec3 | null {
    const p = positions[posIdx];
    if (!p) return null;
    return [p.x + p.width / 2, p.y + p.height / 2, glassTop(posIdx)];
  }

  function objectCenter(ti: number, id: number): Vec3 | null {
    const t = transforms[ti];
    const obj = paths[ti]?.objects?.find((o) => o.id === id);
    const posIdx = positions.findIndex((p, i) => !p.is_prime && getTransformIdx(i, positions) === ti);
    if (!t || !obj || posIdx < 0) return null;
    const pivot = objectPivot(obj);
    let c: Pt;
    if (obj.base_bounds && pivot) c = designToWorld(frameOf(ti), xfApply(shownXform(ti, obj), pivot, pivot));
    else {
      const [x0, x1, y0, y1] = obj.bounds;
      const [x, y] = tpt((x0 + x1) / 2, (y0 + y1) / 2, t.gui_dx, t.gui_dy, t.scale, t.rotation, t.cx, t.cy, t.scale_y);
      c = { x, y };
    }
    return [c.x, c.y, glassTop(posIdx)];
  }

  function selectObject(sel: { slide: number; id: number } | null) {
    dispatch("objectSelected", sel);
    if (sel) scheduleRecenter(objectCenter(sel.slide, sel.id));
  }

  // ─── Snap ─────────────────────────────────────────────────────────────────
  /** px/mm pro krok mřížky: během tažení zmražené, jinak v targetu. */
  function snapPxPerMm(): number {
    return dragOp ? dragPxPerMm : cam.pxPerMm();
  }
  function gridSnapSize(): number {
    const zoom = snapPxPerMm();
    if (zoom > 60) return 0.1;
    if (zoom > 30) return 0.5;
    if (zoom > 15) return 1;
    if (zoom > 7) return 2;
    if (zoom > 3) return 5;
    return 10;
  }
  function gridDrawStep(): { minor: number; major: number } {
    const worldPerTarget = 55 / cam.pxPerMm();
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
  /** Rovina kresleného tvaru — vršek skla pod počátečním bodem, po celý tah. */
  let shapeZ = 0;
  const MIN_SHAPE_MM = 0.1;

  function drawPointFromMouse(sx: number, sy: number): { x: number; y: number } {
    if (!shapeStart) {
      const i = pickSlide(sx, sy);
      shapeZ = i >= 0 ? glassTop(i) : 0;
    }
    const w = screenToWorld(sx, sy, shapeZ);
    snapHit = snapToPaths ? snapToTrasa(sx, sy, shapeZ) : null;
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

  /** Vzdálenost na obrazovce (px) bodu roviny z od kurzoru. */
  function screenDist(x: number, y: number, z: number, sx: number, sy: number): number {
    const s = cam.project(x, y, z);
    return s ? Math.hypot(s.x - sx, s.y - sy) : Infinity;
  }

  function snapToTrasa(sx: number, sy: number, z: number): SnapHit | null {
    const w = cam.screenToPlane(sx, sy, z, true);
    if (!w) return null;
    // Ořez světovým poloměrem (nejmenší px/mm → největší poloměr), přesně pak v px
    const r = SNAP_PX / cam.pxPerMmMin(w.x, w.y, z);
    type Cand = { x: number; y: number; ti: number; obj: number | null };
    let vertex: Cand | null = null,
      vertexD = SNAP_PX;
    let edge: Cand | null = null,
      edgeD = SNAP_PX;
    for (let i = 0; i < positions.length; i++) {
      if (positions[i].is_prime) continue;
      const ti = getTransformIdx(i, positions);
      const t = transforms[ti];
      const pd = paths[ti];
      const raw = getPathBboxRaw(i);
      if (!t || !pd || !raw) continue;
      const bb = computeWorldAABB(t.gui_dx, t.gui_dy, Math.max(t.scale, t.scale_y ?? t.scale), t.rotation, t.cx, t.cy, raw);
      if (w.x < bb.minX - r || w.x > bb.maxX + r || w.y < bb.minY - r || w.y > bb.maxY + r) continue;
      const toWorld = tptFn(t.gui_dx, t.gui_dy, t.scale, t.rotation, t.cx, t.cy, t.scale_y);
      for (const seg of pd.segments) {
        let prev: [number, number] | null = null;
        for (const pt of seg.points) {
          const cur = toWorld(pt.x, pt.y);
          if (Math.abs(cur[0] - w.x) <= r && Math.abs(cur[1] - w.y) <= r) {
            const dv = screenDist(cur[0], cur[1], z, sx, sy);
            if (dv < vertexD) {
              vertexD = dv;
              vertex = { x: cur[0], y: cur[1], ti, obj: seg.object ?? null };
            }
          }
          if (prev) {
            const [ax, ay] = prev;
            const dx = cur[0] - ax,
              dy = cur[1] - ay;
            const len2 = dx * dx + dy * dy;
            const k = len2 > 1e-12 ? Math.max(0, Math.min(1, ((w.x - ax) * dx + (w.y - ay) * dy) / len2)) : 0;
            const q = { x: ax + dx * k, y: ay + dy * k };
            if (Math.hypot(q.x - w.x, q.y - w.y) <= r) {
              const de = screenDist(q.x, q.y, z, sx, sy);
              if (de < edgeD) {
                edgeD = de;
                edge = { ...q, ti, obj: seg.object ?? null };
              }
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

  // ─── Handles pro trasu: rohy bbox trasy v world space (po rotaci) ─────────
  function getPathHandles(posIdx: number, useT?: Transform): { x: number; y: number }[] {
    if (posIdx < 0 || posIdx >= positions.length) return [];
    const pos = positions[posIdx];
    if (pos.is_prime) return [];
    const tidx = getTransformIdx(posIdx, positions);
    const t = useT ?? transforms[tidx];
    if (!t) return [];

    const raw = getPathBboxRaw(posIdx);
    if (!raw) return [];
    const r = nozzleDiam / 2;
    // Aplikovat scale kolem (t.cx, t.cy) — stejná logika jako transform_pt v Rustu
    const minX = t.cx + (raw.mnX - t.cx) * t.scale - r;
    const maxX = t.cx + (raw.mxX - t.cx) * t.scale + r;
    const sy = t.scale_y ?? t.scale;
    const minY = t.cy + (raw.mnY - t.cy) * sy - r;
    const maxY = t.cy + (raw.mxY - t.cy) * sy + r;

    // Středová osa rotace trasy v world space
    const pathCX = t.gui_dx + t.cx;
    const pathCY = t.gui_dy + t.cy;
    const rad = (-t.rotation * Math.PI) / 180;
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

  /** Úchyt pod kurzorem (v obrazovkových px). */
  function hitTestHandle(sx: number, sy: number): number {
    if (selectedIndex < 0) return -1;
    return hud.hitHandle(cam, activeHandles(), glassTop(selectedIndex), sx, sy, 7);
  }

  // ─── Sdílené geometrické helpery ─────────────────────────────────────────

  function getPathBboxRaw(posIdx: number): RawBbox | null {
    const pathData = paths[getTransformIdx(posIdx, positions)];
    if (!pathData || pathData.segments.length === 0) return null;
    return cachedBbox(pathData);
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

  /** Rohy rámu objektu rozbaleného v lokálním panelu (v pořadí obvodu). */
  function focusedObjectCorners(): { corners: Pt[]; z: number } | null {
    if (!focusedObject) return null;
    const ti = focusedObject.slide;
    const t = transforms[ti];
    const obj = paths[ti]?.objects?.find((o) => o.id === focusedObject!.id);
    const posIdx = positions.findIndex((p, i) => !p.is_prime && getTransformIdx(i, positions) === ti);
    if (!t || !obj || posIdx < 0) return null;
    const pivot = objectPivot(obj);
    let corners: Pt[];
    if (obj.base_bounds && pivot) {
      // Rám neupraveného objektu po jeho úpravě (otočený s objektem)
      const f = frameOf(ti);
      const xf = shownXform(ti, obj);
      const [a, b, d, e] = baseCorners(obj.base_bounds, nozzleDiam / 2).map((p) => designToWorld(f, xfApply(xf, p, pivot)));
      corners = [a, b, e, d];
    } else {
      const [x0, x1, y0, y1] = obj.bounds;
      const pad = nozzleDiam / 2;
      corners = [
        [x0 - pad, y0 - pad],
        [x1 + pad, y0 - pad],
        [x1 + pad, y1 + pad],
        [x0 - pad, y1 + pad],
      ].map(([x, y]) => {
        const [wx, wy] = tpt(x, y, t.gui_dx, t.gui_dy, t.scale, t.rotation, t.cx, t.cy, t.scale_y);
        return { x: wx, y: wy };
      });
    }
    return { corners, z: glassTop(posIdx) };
  }

  // ─── Measure snap ─────────────────────────────────────────────────────────
  /** Rovina pravítka — vršek skla pod prvním bodem. */
  let measureZ = 0;

  function applyMeasureSnap(sx: number, sy: number): { x: number; y: number } {
    const raw = cam.screenToPlane(sx, sy, measureZ, true) ?? lastValidWorld;
    if (ctrlDown) return { x: snapToGrid(raw.x), y: snapToGrid(raw.y) };
    if (altDown) return raw; // ALT = volný kurzor bez snapu

    // Výchozí: snap na klíčové body sklíček + body tras (vzdálenost v px)
    let best = raw,
      bestDist = 8;
    const r = 8 / cam.pxPerMmMin(raw.x, raw.y, measureZ);

    for (let i = 0; i < positions.length; i++) {
      const pos = positions[i];
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
        const d = screenDist(cx, cy, measureZ, sx, sy);
        if (d < bestDist) {
          bestDist = d;
          best = { x: cx, y: cy };
        }
      }
    }

    for (let i = 0; i < positions.length; i++) {
      if (positions[i].is_prime) continue;
      const tidx = getTransformIdx(i, positions);
      const t = transforms[tidx];
      const pd = paths[tidx];
      const rawBb = getPathBboxRaw(i);
      if (!t || !pd || !rawBb) continue;
      // Trasy daleko od kurzoru se neprocházejí
      const bb = computeWorldAABB(t.gui_dx, t.gui_dy, Math.max(t.scale, t.scale_y ?? t.scale), t.rotation, t.cx, t.cy, rawBb);
      if (raw.x < bb.minX - r || raw.x > bb.maxX + r || raw.y < bb.minY - r || raw.y > bb.maxY + r) continue;
      const toWorld = tptFn(t.gui_dx, t.gui_dy, t.scale, t.rotation, t.cx, t.cy, t.scale_y);
      for (const seg of pd.segments) {
        for (const pt of seg.points) {
          const [ptx, pty] = toWorld(pt.x, pt.y);
          if (Math.abs(ptx - raw.x) > r || Math.abs(pty - raw.y) > r) continue;
          const d = screenDist(ptx, pty, measureZ, sx, sy);
          if (d < bestDist) {
            bestDist = d;
            best = { x: ptx, y: pty };
          }
        }
      }
    }

    return best;
  }

  // ─── Klávesy ──────────────────────────────────────────────────────────────
  function typingTarget(e: KeyboardEvent): boolean {
    const el = e.target;
    if (el instanceof HTMLTextAreaElement) return true;
    return el instanceof HTMLInputElement && !["checkbox", "radio", "button", "range"].includes(el.type);
  }

  /** Pohled po ose (numpad 1/3/7, gizmo) — vždy ortho. */
  function axisView(yaw: number, pitch: number) {
    cam.animateTo({ yaw, pitch, projection: "ortho" });
    scheduleDraw();
  }

  export function toggleProjection() {
    if (cam.isAxisView()) {
      dispatch("notice", "Pohled po ose X / Y / Z je vždy ortho — pro perspektivu pohled natočte");
      return;
    }
    cam.setProjection(cam.projection === "ortho" ? "persp" : "ortho");
    scheduleDraw();
  }

  /** Šikmý pohled na celou plochu (tlačítko „3D“). */
  export function obliqueView() {
    const tmp = cam.clone();
    tmp.set({ yaw: -Math.PI / 7, pitch: (35 * Math.PI) / 180, projection: "persp" });
    cam.animateTo({ ...tmp.fitBox([0, 0, 0], [bedMaxX, bedMaxY, maxGlassTop()], 40), yaw: tmp.yaw, pitch: tmp.pitch, projection: "persp" });
    scheduleDraw();
  }

  export function topView() {
    axisView(0, PITCH_TOP);
  }

  export function viewInfo(): { projection: "ortho" | "persp"; top: boolean; axis: boolean } {
    return { projection: cam.projection, top: cam.isTopView(), axis: cam.isAxisView() };
  }

  function handleNavKey(e: KeyboardEvent): boolean {
    const step = Math.PI / 12;
    switch (e.code) {
      case "Numpad7":
        axisView(0, PITCH_TOP);
        return true;
      case "Numpad1":
        axisView(e.ctrlKey ? Math.PI : 0, PITCH_MIN);
        return true;
      case "Numpad3":
        axisView(e.ctrlKey ? -Math.PI / 2 : Math.PI / 2, PITCH_MIN);
        return true;
      case "Numpad5":
        toggleProjection();
        return true;
      case "Numpad4":
      case "Numpad6":
      case "Numpad8":
      case "Numpad2": {
        settleCamera();
        const dy = e.code === "Numpad4" ? step : e.code === "Numpad6" ? -step : 0;
        // Numpad 8 = pohled víc shora, 2 = níž (jako Blender)
        const dp = e.code === "Numpad8" ? step : e.code === "Numpad2" ? -step : 0;
        const pitch = Math.max(PITCH_MIN, Math.min(PITCH_TOP, cam.pitch + dp));
        // Mimo osu v auto-perspektivě perspektiva; na ose ji kamera vrátí do ortho
        const autoPersp = $settingsStore.auto_perspective ?? true;
        cam.animateTo({ yaw: cam.yaw + dy, pitch, projection: autoPersp ? "persp" : cam.projection }, 150);
        scheduleDraw();
        return true;
      }
      case "NumpadDecimal":
        frameSelection();
        return true;
      case "Home":
        fitAll();
        return true;
    }
    return false;
  }

  function handleKeyDown(e: KeyboardEvent) {
    ctrlDown = e.ctrlKey;
    altDown = e.altKey;
    shiftDown = e.shiftKey;
    if (typingTarget(e)) return;
    if (!e.altKey && !e.metaKey && handleNavKey(e)) {
      e.preventDefault();
      return;
    }
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

  // ─── Kolečko ──────────────────────────────────────────────────────────────
  // Posluchač je na okně (ne na plátně) a o plátně rozhoduje poloha kurzoru:
  // WebKitGTK po kliknutí/scrollu v jiném panelu doručuje kolečko latchovanému
  // prvku (levý panel), takže by se plátno o události vůbec nedozvědělo.
  function handleWheel(e: WheelEvent) {
    const hit = document.elementFromPoint(e.clientX, e.clientY);
    if (!hit || !container.contains(hit)) return;
    e.preventDefault();
    const rect = hudCanvas.getBoundingClientRect();
    const sx = e.clientX - rect.left,
      sy = e.clientY - rect.top;
    const dir = e.deltaY < 0 ? 1 : -1;
    settleCamera();
    cam.zoomAt(dir > 0 ? 1.1 : 1 / 1.1, wheelAnchor(sx, sy), minScale(), 80);
    markInteraction();
    scheduleDraw();
  }

  // ─── Navigace kamerou (prostřední tlačítko, gizmo) ───────────────────────
  const ORBIT_RAD_PER_PX = (0.3 * Math.PI) / 180;

  function startNav(kind: "orbit" | "pan" | "zoom", sx: number, sy: number) {
    settleCamera();
    const persp = kind === "orbit" && (cam.projection === "persp" || ($settingsStore.auto_perspective ?? true));
    navOp = {
      kind,
      sx,
      sy,
      start: cam.state(),
      grab: kind === "pan" ? cam.screenToPlane(sx, sy, cam.target[2], true) : null,
      persp,
    };
  }

  /** Posun „uchopením“ bodu v rovině targetu — zůstává pod kurzorem. */
  function grabPan(startTarget: Vec3, grab: Pt | null, sx0: number, sy0: number, sx: number, sy: number) {
    cam.target = [...startTarget];
    const now = grab ? cam.screenToPlane(sx, sy, startTarget[2], true) : null;
    if (grab && now) {
      cam.target = [startTarget[0] + grab.x - now.x, startTarget[1] + grab.y - now.y, startTarget[2]];
    } else {
      // Rovina skoro z boku — posun v rovině obrazovky
      const k = 1 / cam.pxPerMm();
      const r = cam.right(),
        u = cam.up();
      const dx = (sx - sx0) * k,
        dy = (sy - sy0) * k;
      cam.target = [
        startTarget[0] - r[0] * dx + u[0] * dy,
        startTarget[1] - r[1] * dx + u[1] * dy,
        startTarget[2] - r[2] * dx + u[2] * dy,
      ];
    }
  }

  function moveNav(sx: number, sy: number) {
    const n = navOp!;
    if (n.kind === "orbit") {
      cam.set({ yaw: n.start.yaw - (sx - n.sx) * ORBIT_RAD_PER_PX, pitch: n.start.pitch + (sy - n.sy) * ORBIT_RAD_PER_PX });
      // Mimo osu zpět do perspektivy (pohled po ose drží kamera v ortho)
      if (!cam.isAxisView() && n.persp) cam.setProjection("persp");
    } else if (n.kind === "pan") {
      grabPan(n.start.target, n.grab, n.sx, n.sy, sx, sy);
    } else {
      const f = Math.exp(-(sy - n.sy) * 0.01);
      cam.scale = Math.max(minScale(), Math.min(n.start.scale * f, 80));
    }
    markInteraction();
    scheduleDraw();
  }

  function gizmoView(axis: hud.GizmoAxis) {
    if (axis === "z") axisView(0, PITCH_TOP);
    else if (axis === "y") axisView(Math.PI, PITCH_MIN);
    else if (axis === "-y") axisView(0, PITCH_MIN);
    else if (axis === "x") axisView(Math.PI / 2, PITCH_MIN);
    else axisView(-Math.PI / 2, PITCH_MIN);
  }

  /** Editace v rovině je v bočním pohledu vypnutá (paprsky téměř rovnoběžné s deskou). */
  function editAllowed(): boolean {
    if (cam.canEditPlane()) return true;
    dispatch("notice", "Úpravy nejsou v bočním pohledu možné — natočte pohled více shora (Numpad 7)");
    return false;
  }

  // ─── Pointer down ─────────────────────────────────────────────────────────
  function localXY(e: MouseEvent): [number, number] {
    const rect = hudCanvas.getBoundingClientRect();
    return [e.clientX - rect.left, e.clientY - rect.top];
  }

  function handlePointerDown(e: PointerEvent) {
    const [sx, sy] = localXY(e);
    ctrlDown = e.ctrlKey;
    altDown = e.altKey;
    shiftDown = e.shiftKey;
    mouseX = sx;
    mouseY = sy;
    clickCandidate = null;
    hudCanvas.setPointerCapture(e.pointerId);

    // Prostřední tlačítko: navigace (bez vložení schránky / autoscrollu)
    if (e.button === 1) {
      e.preventDefault();
      startNav(e.shiftKey ? "pan" : e.ctrlKey ? "zoom" : "orbit", sx, sy);
      return;
    }

    // Navigační gizmo
    if (e.button === 0) {
      const g = hud.hitGizmo(cam, hud.gizmoLayout(width), sx, sy);
      if (g) {
        if (g === "ring") startNav("orbit", sx, sy);
        else gizmoView(g);
        return;
      }
    }

    settleCamera();

    if (e.button === 0) {
      if (drawTool) {
        if (!editAllowed()) return;
        shapeStart = null;
        shapeStart = drawPointFromMouse(sx, sy);
        shapeStartSnap = snapHit;
        shapeCurr = { ...shapeStart };
        shapeCurrSnap = snapHit;
        scheduleDraw();
        return;
      }

      if (isMeasuring) {
        if (!editAllowed()) return;
        if (measurePoints.length === 0) {
          const i = pickSlide(sx, sy);
          measureZ = i >= 0 ? glassTop(i) : 0;
        }
        const snapped = applyMeasureSnap(sx, sy);
        measurePoints = [...measurePoints, snapped];
        dispatch("measurePointsChange", measurePoints);
        scheduleDraw();
        return;
      }

      const now = Date.now();
      const selZ = selectedIndex >= 0 ? glassTop(selectedIndex) : 0;
      const wSel = cam.screenToPlane(sx, sy, selZ);

      // 0. Vybraný objekt: úchyty = měřítko / rotace objektu, uvnitř rámu = posun
      //    (dvojklik přepne měřítko ↔ rotace jako u trasy)
      const eo = editableObject();
      if (eo && wSel) {
        const hi = hitTestHandle(sx, sy);
        const inside = hi < 0 && insideObjectFrame(eo, wSel);
        if (hi >= 0 || inside) {
          const isDoubleClick = inside && now - lastClickTime < DOUBLE_CLICK_MS && lastClickIndex === selectedIndex;
          lastClickTime = now;
          lastClickIndex = selectedIndex;
          if (isDoubleClick) {
            transformMode = transformMode === "rotate" ? "scale" : "rotate";
            scheduleDraw();
            return;
          }
          if (!editAllowed()) return;
          dispatch("saveState");
          dragZ = selZ;
          dragPxPerMm = cam.pxPerMm(wSel.x, wSel.y, selZ);
          const frame = frameOf(eo.ti);
          const start: ObjectTransform = { ...(eo.obj.xform ?? IDENTITY_XF) };
          objDrag = { ti: eo.ti, id: eo.obj.id, pivot: eo.pivot, start, cur: start, startD: worldToDesign(frame, wSel), frame, base: eo.base };
          pendingObj = null;
          dragOp = hi >= 0 ? (transformMode === "rotate" ? "objRotate" : "objScale") : "objMove";
          startLivePreview();
          activeHandle = hi;
          if (inside) clickCandidate = { sx, sy, posIdx: selectedIndex };
          return;
        }
      }

      // 1. Handle na již vybrané trase
      if (selectedIndex >= 0 && !positions[selectedIndex]?.is_prime && wSel) {
        const hi = hitTestHandle(sx, sy);
        if (hi >= 0) {
          if (!editAllowed()) return;
          const tidx = getTransformIdx(selectedIndex, positions);
          dispatch("saveState");
          dragOp = transformMode === "rotate" ? "rotate" : "scale";
          activeHandle = hi;
          dragSlideIdx = selectedIndex;
          dragZ = selZ;
          dragPxPerMm = cam.pxPerMm(wSel.x, wSel.y, selZ);
          dragStartWorld = { ...wSel };
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

      // 2. Sklo pod kurzorem (nejbližší zásah paprsku)
      const hitIdx = pickSlide(sx, sy);
      const hitZ = hitIdx >= 0 ? glassTop(hitIdx) : 0;
      const w = cam.screenToPlane(sx, sy, hitZ, true);

      if (hitIdx >= 0 && !positions[hitIdx].is_prime) {
        const isDoubleClick = now - lastClickTime < DOUBLE_CLICK_MS && lastClickIndex === hitIdx;
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
          if (w && transforms[tidx] && isInsidePath(w.x, w.y, hitIdx) && cam.canEditPlane()) {
            dispatch("saveState");
            dragOp = "move";
            dragSlideIdx = hitIdx;
            dragZ = hitZ;
            dragPxPerMm = cam.pxPerMm(w.x, w.y, hitZ);
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
          scheduleRecenter(slideCenter(hitIdx));
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

      // Pan (uchopení bodu v rovině targetu)
      dragOp = "pan";
      panStart = { sx, sy, target: [...cam.target], grab: cam.screenToPlane(sx, sy, cam.target[2], true) };
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
      const i = pickSlide(sx, sy);
      if (i >= 0) {
        // Pravý klik na objekt vybrané trasy ho vybere (menu pak nabízí i objekt)
        if (i === selectedIndex) {
          const w = cam.screenToPlane(sx, sy, glassTop(i), true);
          const id = w ? hitTestObject(w.x, w.y, i) : null;
          if (id != null) dispatch("objectSelected", { slide: getTransformIdx(i, positions), id });
        } else {
          dispatch("objectSelected", null);
        }
        dispatch("slideContext", { index: i, x: e.clientX, y: e.clientY });
      }
    }
  }

  // ─── Pointer move ─────────────────────────────────────────────────────────
  function handlePointerMove(e: PointerEvent) {
    [mouseX, mouseY] = localXY(e);
    ctrlDown = e.ctrlKey;
    altDown = e.altKey;
    shiftDown = e.shiftKey;

    if (navOp) {
      moveNav(mouseX, mouseY);
      return;
    }

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

    if (dragOp === "pan" && panStart) {
      grabPan(panStart.target, panStart.grab, panStart.sx, panStart.sy, mouseX, mouseY);
      markInteraction();
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
      markInteraction();
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
      markInteraction();
      scheduleDraw();
      return;
    }

    // ── Škálování trasy ─────────────────────────────────────────────────────
    if (dragOp === "scale" && dragStartTransform !== null && dragSlideIdx >= 0 && activeHandle >= 0) {
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
      const anchor = shiftDown ? { x: st.gui_dx + st.cx, y: st.gui_dy + st.cy } : initHandles[oppositeIdx];
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
      markInteraction();
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
      markInteraction();
      scheduleDraw();
      return;
    }

    // Hover: gizmo a úchyty
    const g = hud.hitGizmo(cam, hud.gizmoLayout(width), mouseX, mouseY);
    if (g !== hoverGizmo) {
      hoverGizmo = g;
      scheduleDraw();
    }
    if (selectedIndex >= 0 && !positions[selectedIndex]?.is_prime && dragOp === null) {
      const nh = g ? -1 : hitTestHandle(mouseX, mouseY);
      if (nh !== hoverHandle) {
        hoverHandle = nh;
        scheduleDraw();
      }
    }

    if (isMeasuring) scheduleDraw();
  }

  // ─── Pointer up ───────────────────────────────────────────────────────────
  function handlePointerUp(e: PointerEvent) {
    if (hudCanvas.hasPointerCapture(e.pointerId)) hudCanvas.releasePointerCapture(e.pointerId);
    if (navOp) {
      navOp = null;
      markInteraction();
      scheduleDraw();
      return;
    }

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
      const w = cam.screenToPlane(click.sx, click.sy, glassTop(click.posIdx), true);
      if (dragOp === "objMove" && objDrag) {
        // Klik bez pohybu na vybraný objekt: vybrat objekt pod kurzorem (vnořený)
        objDrag.cur = objDrag.start;
        livePaths = new Map();
        const id = w ? hitTestObject(w.x, w.y, click.posIdx) : null;
        if (id != null && id !== objDrag.id) selectObject({ slide: tidx, id });
      } else if (dragOp === "pan" || (dragOp === "move" && onlySelf)) {
        // Případný mikroposun během kliku se zahodí
        if (dragOp === "move" && t && dragStartTransform) {
          t.gui_dx = dragStartTransform.gui_dx;
          t.gui_dy = dragStartTransform.gui_dy;
        }
        if (dragOp === "pan" && panStart) cam.target = [...panStart.target];
        _dragModifiedIndices.clear();
        const id = dragOp === "move" && w ? hitTestObject(w.x, w.y, click.posIdx) : null;
        if (id != null) selectObject({ slide: tidx, id });
        else {
          dispatch("objectSelected", null);
          scheduleRecenter(slideCenter(click.posIdx));
        }
      }
    }

    if ((dragOp === "scale" || dragOp === "rotate") && dragSlideIdx >= 0 && dragStartTransform !== null) {
      const tidx = getTransformIdx(dragSlideIdx, positions);
      if (tidx >= 0) {
        const t = transforms[tidx];
        if (t) {
          const startSy = dragStartTransform.scale_y ?? dragStartTransform.scale;
          const scaleChanged =
            Math.abs(t.scale - dragStartTransform.scale) > 0.001 || Math.abs((t.scale_y ?? t.scale) - startSy) > 0.001;
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
      const ed = objDrag;
      objDrag = null;
      if (liveDirty) {
        objDrag = ed;
        updateLivePaths();
        objDrag = null;
      }
      if (JSON.stringify(ed.cur) !== JSON.stringify(ed.start)) {
        pendingObj = { ti: ed.ti, id: ed.id, pivot: ed.pivot, start: ed.start, cur: ed.cur, src: paths[ed.ti] };
        clearTimeout(_pendingTimer);
        _pendingTimer = setTimeout(() => {
          pendingObj = null;
          livePaths = new Map();
          scheduleDraw();
        }, 3000);
        dispatch("objectTransformChanged", { slide: ed.ti, id: ed.id, xform: { ...ed.cur } });
      }
    }

    // Odešli všechny změny transformů najednou (store update 1× místo n× za drag)
    for (const idx of _dragModifiedIndices) {
      if (transforms[idx]) dispatch("transformChanged", { index: idx, transform: { ...transforms[idx] } });
    }
    _dragModifiedIndices.clear();

    dragOp = null;
    panStart = null;
    activeHandle = -1;
    dragSlideIdx = -1;
    dragStartTransform = null;
    dragStartTransformsAll = new Map();
    recomputePreviewDist();
    scheduleDraw();
  }
  function handleContextMenu(e: MouseEvent) {
    e.preventDefault();
  }

  // ─── Kreslení ──────────────────────────────────────────────────────────────
  // Snímek: scéna v rendereru (dpi-render) + HUD ve 2D canvasu nad ní.
  // Nativní hostitel vrací snímek asynchronně — vždy nejvýš jeden v letu,
  // novější stav se vykreslí hned po něm.
  let rafPending = false;
  let inFlight = false;
  let dirtyAgain = false;

  function scheduleDraw() {
    if (!rafPending) {
      rafPending = true;
      requestAnimationFrame(frameTick);
    }
  }

  /** Překreslení (API pro rodiče). */
  export function draw() {
    scheduleDraw();
  }

  async function frameTick() {
    rafPending = false;
    if (cam.tick()) {
      markInteraction();
      scheduleDraw();
    }
    if (!hudCtx || width === 0 || height === 0) return;
    if (inFlight) {
      dirtyAgain = true;
      return;
    }
    inFlight = true;
    try {
      await renderFrame();
    } catch (err) {
      console.error("Vykreslení náhledu selhalo:", err);
    } finally {
      inFlight = false;
      if (dirtyAgain) {
        dirtyAgain = false;
        scheduleDraw();
      }
    }
  }

  const BG = "#0f172a";
  const COLOR_PRIME = "#f97316";
  const COLOR_SELECTED = "#3b82f6";
  const COLOR_SLIDE = "#94a3b8";
  const COLOR_FOCUS = "#fbbf24";
  const OBJECT_PALETTE = ["#f472b6", "#facc15", "#4ade80", "#38bdf8", "#fb923c", "#c084fc", "#2dd4bf", "#f87171"].map(hexToRgb);

  interface DrawState {
    cursor: PreviewCursor | null;
    printPaths: SubstratePaths[];
    ghostPaths: (SubstratePaths | null)[];
    previewNozzle: Point2D | null;
  }

  function drawState(): DrawState {
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
    const previewNozzle = currentNozzle ? null : previewNozzleWorld(cursor, positions, printPaths, primePath, transforms);
    return { cursor, printPaths, ghostPaths, previewNozzle };
  }

  /** Vrchol trysky (z) nad bodem: vršek skla + mezera. */
  function nozzleTip(p: Point2D | null): [number, number, number] | null {
    if (!p) return null;
    const z = surfaceZAt(p.x, p.y);
    const gap = heights()["0"]?.gap ?? 0.05;
    return [p.x, p.y, z + gap];
  }

  function sceneSlides(st: DrawState): SlideScene[] {
    const h = heights();
    const mode = $settingsStore.view_color_mode ?? "substrate";
    return positions.map((pos, i) => {
      const ti = getTransformIdx(i, positions);
      const isSel = i === selectedIndex;
      const hex = pos.is_prime ? COLOR_PRIME : isSel ? COLOR_SELECTED : COLOR_SLIDE;
      const rgb = hexToRgb(hex);
      const [gw, gh] = glassSize(pos);
      const key = heightsKey(pos.is_prime, ti);
      const thick = glassThickness(h, key, $projectStore.params?.slide_z ?? 1);
      const hasT = pos.is_prime || !!transforms[ti];
      const printed = pos.is_prime ? primePath : hasT ? (st.printPaths[ti] ?? null) : null;
      const ghost = pos.is_prime ? null : (st.ghostPaths[ti] ?? null);
      const focusId = focusedObject && focusedObject.slide === ti && !pos.is_prime ? focusedObject.id : null;
      return {
        glass: [pos.x, pos.y, gw, gh, thick],
        fill: [...rgb, isSel ? 0.2 : 0.12] as [number, number, number, number],
        edge: [...rgb, isSel ? 1 : 0.7] as [number, number, number, number],
        paths: printed,
        ghost,
        clip: [pos.x, pos.y, pos.x + gw, pos.y + gh],
        style: (id: number | null) => {
          const b = beadSize(h, key, id, nozzleDiam);
          let color: RGB = rgb;
          if (mode === "object" && id != null) color = OBJECT_PALETTE[Math.abs(id) % OBJECT_PALETTE.length];
          if (focusId != null && id === focusId) color = hexToRgb(COLOR_FOCUS);
          return { w: b.w, h: b.h, color };
        },
      };
    });
  }

  function slideFrames(st: DrawState): SlideFrameData[] {
    return positions.map((pos, i) => {
      const f = pathFrameOf(pos, transforms[getTransformIdx(i, positions)]);
      const th = (-f.rot * Math.PI) / 180;
      let cursor: [number, number, number, number] = [0, 0, 0, 0];
      let frac = 0;
      const c = st.cursor;
      if (c) {
        if (i < c.slideIdx) cursor = [0, 0, 0, 0];
        else if (i > c.slideIdx) cursor = [1, 0, 0, 0];
        else {
          const pd = pos.is_prime ? primePath : st.printPaths[getTransformIdx(i, positions)];
          const pts = pd?.segments[c.segIdx]?.points;
          const start = pts && c.startIdx < pts.length ? c.startIdx : 0;
          cursor = [2, c.segIdx, start, c.travel ? -1 : c.ptIdx];
          frac = c.travel ? 0 : c.fracT;
        }
      }
      return { xf: [f.x, f.y, f.cx, f.cy, f.scale, f.scaleY, Math.cos(th), Math.sin(th)], cursor, frac };
    });
  }

  // Přejezdy (z časové osy náhledu): zdvih, přejezd, sjezd
  let _travels: { src: PreviewSegData[]; out: Float32Array } | null = null;
  function travels(): Float32Array | null {
    if (!($settingsStore.show_travels ?? false)) return null;
    if (_travels && _travels.src === precomputedSegs) return _travels.out;
    const hop = Math.max(0, $settingsStore.default_z_hop ?? 2);
    const h = heights();
    const out: number[] = [];
    let prevZ: number | null = null;
    for (const sd of precomputedSegs) {
      const pos = positions[sd.slide_idx];
      if (!pos) continue;
      const ti = getTransformIdx(sd.slide_idx, positions);
      const pd = pos.is_prime ? primePath : paths[ti];
      const seg = pd?.segments[sd.seg_idx];
      if (!seg || seg.points.length === 0) continue;
      const toWorld = frameFn(pathFrameOf(pos, transforms[ti]));
      const p0 = printedPoint(seg.points, sd.start_idx < seg.points.length ? sd.start_idx : 0, 0);
      const [x1, y1] = toWorld(p0.x, p0.y);
      const z = glassThickness(h, heightsKey(pos.is_prime, ti), 1) + beadSize(h, heightsKey(pos.is_prime, ti), seg.object ?? null, nozzleDiam).gap;
      const za = prevZ ?? z;
      const { x: x0, y: y0 } = sd.travel_from;
      if (Math.hypot(x1 - x0, y1 - y0) > 1e-6) {
        if (seg.wet_travel) out.push(x0, y0, za, x1, y1, z);
        else {
          const lift = Math.max(za, z) + hop;
          out.push(x0, y0, za, x0, y0, lift, x0, y0, lift, x1, y1, lift, x1, y1, lift, x1, y1, z);
        }
      }
      prevZ = z;
    }
    _travels = { src: precomputedSegs, out: new Float32Array(out) };
    return _travels.out;
  }

  /** Rozlišení cíle: CSS × DPR; nativně během interakce nižší (přenos snímku). */
  function renderSize(): { w: number; h: number } {
    const d = Math.min(dpr, 2);
    let k = d;
    if (host?.kind === "native") {
      const interacting = dragOp !== null || navOp !== null || cam.animating || performance.now() - lastInteraction < 150;
      if (interacting) k = Math.min(d, Math.sqrt(1_000_000 / (width * height)));
    }
    return { w: Math.max(1, Math.round(width * k)), h: Math.max(1, Math.round(height * k)) };
  }

  async function renderFrame() {
    if (liveDirty) updateLivePaths();
    updateSceneBounds();
    const st = drawState();
    const view = cam.clone();
    if (host) {
      const pkt = sync.packet({ bed: { w: bedMaxX, h: bedMaxY, thickness: BED_THICKNESS }, slides: sceneSlides(st), travels: travels() });
      if (pkt) await host.applyScene(pkt);
      const { w, h } = renderSize();
      const { minor, major } = gridDrawStep();
      const eye = view.eye();
      const frame = encodeFrame({
        viewProj: view.viewProj(),
        eye,
        ortho: view.projection === "ortho",
        viewDir: view.forward(),
        width: w,
        height: h,
        pxPerMm: view.pxPerMm() * (w / width),
        dpr: w / width,
        grid: [minor, major, showAxes],
        zExaggeration: $settingsStore.z_exaggeration ?? 1,
        orderColors: ($settingsStore.view_color_mode ?? "substrate") === "order",
        lodPx: 2,
        dashMm: 6 / view.pxPerMm(),
        nozzleReal: nozzleTip(currentNozzle),
        nozzlePreview: nozzleTip(st.previewNozzle),
        clear: [...hexToRgb(BG), 1],
        showGhosts: true,
        showTravels: $settingsStore.show_travels ?? false,
        slides: slideFrames(st),
      });
      await host.render(frame, w, h);
    }
    drawHud(st, view);
    if (!ready) ready = true;
    const vi = viewInfo();
    const key = `${vi.projection}|${vi.top}|${vi.axis}`;
    if (key !== _lastViewKey) {
      _lastViewKey = key;
      dispatch("viewChanged", vi);
    }
  }
  let _lastViewKey = "";

  function drawHud(st: DrawState, view: OrbitCamera) {
    const ctx = hudCtx!;
    const W = Math.round(width * dpr),
      H = Math.round(height * dpr);
    if (hudCanvas.width !== W || hudCanvas.height !== H) {
      hudCanvas.width = W;
      hudCanvas.height = H;
    }
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, W, H);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    let measTotal: number | null = null;
    const { major: gMajor } = gridDrawStep();

    // Šipky směru tisku (všechna skla) a značky začátků drah (vybrané sklo)
    const showDirection = !!$settingsStore.show_print_direction;
    positions.forEach((pos, i) => {
      if (pos.is_prime) return;
      const markers = i === selectedIndex;
      if (!showDirection && !markers) return;
      const ti = getTransformIdx(i, positions);
      const pd = st.printPaths[ti];
      if (!pd || !transforms[ti]) return;
      const toWorld = frameFn(pathFrameOf(pos, transforms[ti]));
      const runs = markerRuns(pd, i, st.cursor, toWorld);
      hud.drawPathMarkers(ctx, view, runs, glassTop(i), showDirection, markers);
    });

    // Popisky skel — názvy shodné s lokálním panelem: „Substrát N“ / vlastní název, „Odpliv“
    hud.drawSlideLabels(
      ctx,
      view,
      positions.map((pos, i) => {
        const ti = getTransformIdx(i, positions);
        const [, gh] = glassSize(pos);
        return {
          text: pos.is_prime ? "Odpliv" : overrides[ti.toString()]?.name || `Substrát ${ti + 1}`,
          x: pos.x,
          y: pos.y + gh,
          z: glassTop(i),
          selected: i === selectedIndex,
        };
      })
    );

    // Úchyty vybrané trasy / objektu
    if (selectedIndex >= 0 && selectedIndex < positions.length && !positions[selectedIndex]?.is_prime) {
      const z = glassTop(selectedIndex);
      const eo = editableObject();
      if (eo) {
        const xf = shownXform(eo.ti, eo.obj);
        const center = designToWorld(frameOf(eo.ti), xfApply(xf, eo.pivot, eo.pivot));
        hud.drawTransformHandles(ctx, view, objectHandles(eo), z, transformMode, hoverHandle, center);
      } else {
        const t = transforms[getTransformIdx(selectedIndex, positions)];
        hud.drawTransformHandles(ctx, view, getPathHandles(selectedIndex), z, transformMode, hoverHandle, t ? { x: t.gui_dx + t.cx, y: t.gui_dy + t.cy } : null);
      }
    }

    // Měřidlo
    if (isMeasuring && measurePoints.length > 0) {
      const snapped = applyMeasureSnap(mouseX, mouseY);
      const raw = cam.screenToPlane(mouseX, mouseY, measureZ, true);
      const isSnapped = !altDown && (ctrlDown || (!!raw && Math.hypot(snapped.x - raw.x, snapped.y - raw.y) > 0.001));
      measTotal = hud.drawMeasure(ctx, view, measurePoints, snapped, measureZ, isSnapped, ctrlDown ? "#38bdf8" : "#a78bfa").total;
    }

    const fo = focusedObjectCorners();
    if (fo) hud.drawDashedFrame(ctx, view, fo.corners, fo.z, COLOR_FOCUS);

    // Náhled právě kresleného tvaru
    const sp = shapePreview();
    if (sp) hud.drawShapePreview(ctx, view, sp.pts, shapeZ, sp.closed, sp.label);

    // Živý náhled sloučení uzlů
    if (mergePreview && mergePreview.groups.length > 0) {
      hud.drawMergePreview(ctx, view, mergePreview, selectedIndex >= 0 ? glassTop(selectedIndex) : 0);
    }

    // Magnet: bod, ke kterému se kurzor přichytil
    if (drawTool && snapToPaths && snapHit) hud.drawRing(ctx, view, snapHit, shapeZ, "#e879f9");

    if (navOp?.kind === "orbit") hud.drawPivot(ctx, view);

    // Osy — shora popisky podél hran desky; z boku (ortho po ose X / Y) navíc
    // svislá osa Z, aby šla odečíst výška skel a drah
    if (showAxes) {
      if (view.isAxisView() && !view.isTopDown()) hud.drawSideAxes(ctx, view, bedMaxX, bedMaxY, BED_THICKNESS);
      else hud.drawAxisLabels(ctx, view, bedMaxX, bedMaxY, gMajor);
    }

    hud.drawGizmo(ctx, view, hud.gizmoLayout(width), hoverGizmo);

    // ── Obrazovkové overlaye ─────────────────────────────────────────────
    if (measTotal !== null) {
      const count = measurePoints.length;
      hud.drawCornerText(ctx, count > 1 ? `Celkem: ${measTotal.toFixed(1)} mm` : `${measTotal.toFixed(2)} mm`);
    }

    // ── Pravý horní overlay — celkové statistiky G-kódu ─────────────────
    // Info boxy vpravo nahoře začínají pod gizmem
    let statsBoxBottom = hud.overlayTop(width) - 8;
    if ($projectStore.totalDist > 0 && ($settingsStore.show_gcode_stats ?? true)) {
      const td = $projectStore.totalDist;
      const tt = $projectStore.totalTime;
      const statLines: string[] = ["Statistiky G-kódu", `Celková dráha: ${td.toFixed(1)} mm`, `Čas tisku: ${Math.floor(tt / 60)} min ${Math.round(tt % 60)} s`];
      if (extrusionRateUl > 0) {
        const vol = td * extrusionRateUl;
        statLines.push(`Celkový objem: ${vol >= 1000 ? (vol / 1000).toFixed(3) + " ml" : vol.toFixed(2) + " µl"}`);
      }
      // Páry start–konec = souvislé tiskové tahy (dráha navazující přesně na
      // předchozí se tiskne bez přerušení, např. objekty spojené čarou)
      const runs = positions
        .map((p, i) => ({ p, ti: getTransformIdx(i, positions) }))
        .filter(({ p, ti }) => !p.is_prime && ti >= 0)
        .map(({ ti }) => ({
          name: overrides[String(ti)]?.name || `Substrát ${ti + 1}`,
          n: st.printPaths[ti] ? cachedPrintRuns(st.printPaths[ti]) : 0,
        }));
      if (runs.length > 0 && runs.every((r) => r.n === runs[0].n)) {
        statLines.push(runs.length > 1 ? `Páry start–konec: ${runs[0].n} na substrát` : `Páry start–konec: ${runs[0].n}`);
      } else {
        for (const r of runs) statLines.push(`Páry start–konec (${r.name}): ${r.n}`);
      }
      statsBoxBottom = hud.drawOverlayBox(ctx, statLines, width, hud.overlayTop(width), "#7dd3fc");
    }

    // ── Pravý horní overlay — info o vybraném prvku ──────────────────────────
    if (($settingsStore.show_object_info ?? true) && selectedIndex >= 0 && selectedIndex < positions.length && !positions[selectedIndex]?.is_prime) {
      const si = getTransformIdx(selectedIndex, positions);
      const t = transforms[si];
      const spos = positions[selectedIndex];
      const spd = paths[si];
      if (t) {
        // Délka trasy — součet vzdáleností po sobě jdoucích bodů × aktuální scale
        const pathLen = spd ? cachedPathLength(spd) * t.scale : 0;

        // Pozice středu trasy relativně k levému dolnímu rohu skla
        const cx = (t.gui_dx + t.cx - spos.x).toFixed(1);
        const cy = (t.gui_dy + t.cy - spos.y).toFixed(1);

        // Rozměry navrženého tvaru (objekty bez děr) × scale (nezávislé na rotaci) —
        // ne obálka drah, ta je o polovinu rozestupu výplně menší než nakreslený tvar
        const rawBb = designBbox(spd) ?? getPathBboxRaw(selectedIndex);
        const pw = rawBb ? (rawBb.mxX - rawBb.mnX) * t.scale : 0;
        const ph = rawBb ? (rawBb.mxY - rawBb.mnY) * (t.scale_y ?? t.scale) : 0;

        const lines: string[] = [
          `X: ${cx} mm    Y: ${cy} mm`,
          `Rotace: ${t.rotation.toFixed(1)}°`,
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

        hud.drawOverlayBox(ctx, lines, width, statsBoxBottom + 8, null);
      }
    }

    // Jemná snap mřížka při CTRL
    if (ctrlDown && (drawTool !== null || ((dragOp === "move" || dragOp === null) && selectedIndex >= 0))) {
      const z = drawTool ? shapeZ : selectedIndex >= 0 ? glassTop(selectedIndex) : 0;
      hud.drawSnapGrid(ctx, view, gridSnapSize(), z, [0, 0, bedMaxX, bedMaxY]);
    }
  }

  /** Dráhy pro šipky a značky začátků (world, v pořadí tisku) — jen vytištěná
   *  část podle kurzoru náhledu, bez teček. */
  function markerRuns(pd: SubstratePaths, posIdx: number, cursor: PreviewCursor | null, toWorld: (x: number, y: number) => [number, number]) {
    const segs = pd.segments;
    let full = segs.length,
      curSeg = -1;
    if (cursor) {
      if (posIdx > cursor.slideIdx) full = 0;
      else if (posIdx === cursor.slideIdx) {
        full = Math.min(cursor.segIdx, segs.length);
        if (cursor.segIdx < segs.length && !cursor.travel) curSeg = cursor.segIdx;
      }
    }
    const startOf = (si: number) => (activeEdit() ? 0 : (seqStartIdx.get(`${posIdx}:${si}`) ?? 0));
    const runs: { pts: Pt[]; whole: boolean }[] = [];
    const add = (si: number, upTo: number, whole: boolean, start: number) => {
      const pts = segs[si].points;
      if (pts.length === 0 || isDotSeg(pts)) return;
      const s = start >= 0 && start < pts.length ? start : 0;
      const n = Math.min(upTo, pts.length);
      const out: Pt[] = [];
      for (let k = 0; k < n; k++) {
        const p = printedPoint(pts, s, k);
        const [x, y] = toWorld(p.x, p.y);
        out.push({ x, y });
      }
      runs.push({ pts: out, whole });
    };
    for (let si = 0; si < full; si++) add(si, segs[si].points.length, true, startOf(si));
    if (curSeg >= 0 && cursor) add(curSeg, cursor.ptIdx + 1 + (cursor.fracT > 0 ? 1 : 0), false, cursor.startIdx);
    return runs;
  }

  /** Součet délek drah (bez měřítka). */
  const cachedPathLength = memoByPaths((pd) => {
    let len = 0;
    for (const seg of pd.segments)
      for (let k = 1; k < seg.points.length; k++)
        len += Math.hypot(seg.points[k].x - seg.points[k - 1].x, seg.points[k].y - seg.points[k - 1].y);
    return len;
  });

  /** Počet souvislých tiskových tahů (pár start–konec) trasy sklíčka: každá
   *  dráha je tah, pokud přesně nenavazuje na konec předchozí. */
  const cachedPrintRuns = memoByPaths(countPrintRuns);
  function countPrintRuns(pd: SubstratePaths): number {
    let n = 0;
    let last: Point2D | null = null;
    for (const seg of pd.segments) {
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
      const inside = bb.minX >= pos.x && bb.maxX <= pos.x + pos.width && bb.minY >= pos.y && bb.maxY <= pos.y + pos.height;
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
      const result = await compute_preview_segments(positions, paths, transforms, primePath, params, overrides, motionFromSettings($settingsStore));
      precomputedSegs = result.segs;
      seqStartIdx = new Map(result.segs.map((sd) => [`${sd.slide_idx}:${sd.seg_idx}`, sd.start_idx]));
      previewTotalTime = result.total_time;
      scheduleDraw();
    } catch (e) {
      console.error("Výpočet časové osy náhledu selhal:", e);
    }
  }

  run(() => {
    paths;
    primePath;
    positions;
    transforms;
    overrides;
    $projectStore.params;
    $settingsStore.default_z_hop;
    $settingsStore.retract_nl;
    $settingsStore.retract_speed;
    $settingsStore.dot_dwell_ms;
    $settingsStore.calibration_factor;
    if (!dragOp) recomputePreviewDist();
  });

  // Při změně rozměrů podložky ukážeme celou plochu s odkladem 200 ms —
  // doUpdateLayout má debounce 150 ms, takže proběhne až po přepočtu pozic sklíček.
  // svelte-ignore state_referenced_locally -- vzor prev-value: záměrně počáteční hodnota
  let _prevBedMaxX = $state(bedMaxX);
  // svelte-ignore state_referenced_locally -- vzor prev-value: záměrně počáteční hodnota
  let _prevBedMaxY = $state(bedMaxY);
  let _bedResizeTimer: ReturnType<typeof setTimeout> = $state()!;
  run(() => {
    if (bedMaxX !== _prevBedMaxX || bedMaxY !== _prevBedMaxY) {
      _prevBedMaxX = bedMaxX;
      _prevBedMaxY = bedMaxY;
      if (hudCtx) {
        clearTimeout(_bedResizeTimer);
        _bedResizeTimer = setTimeout(fitAll, 200);
      }
    }
  });

  run(() => {
    if (
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
      mergePreview !== undefined &&
      $settingsStore &&
      $projectStore
    ) {
      if (pendingObj && pendingObj.src !== paths[pendingObj.ti]) {
        pendingObj = null;
        livePaths = new Map();
      }
      heightsDirty = true;
      scheduleDraw();
    }
  });

  // ─── Hostitel rendereru ───────────────────────────────────────────────────
  let _hostSetting: string | undefined = undefined;
  let _hostGen = 0;

  async function initHost() {
    const gen = ++_hostGen;
    const old = host;
    host = null;
    await old?.dispose();
    hostKind = null;
    hostError = "";
    sync.reset();
    const errors: string[] = [];
    for (const kind of hostOrder($settingsStore.render_host)) {
      const canvas = document.createElement("canvas");
      canvas.className = "absolute inset-0 w-full h-full block";
      canvas.dataset.testid = "viewport-scene";
      sceneHolder.replaceChildren(canvas);
      try {
        const h = await createHost(kind, canvas);
        if (gen !== _hostGen) {
          // Nativní renderer je jeden sdílený — novější inicializace ho používá
          if (h.kind === "wasm") h.dispose();
          return;
        }
        host = h;
        hostKind = kind;
        viewportRenderer.set({ kind, ...h.info });
        scheduleDraw();
        return;
      } catch (err) {
        console.error(`Renderer (${kind}) nelze spustit:`, err);
        errors.push(`${kind}: ${err instanceof Error ? err.message : String(err)}`);
      }
    }
    sceneHolder.replaceChildren();
    hostError = errors.join(" · ");
    viewportRenderer.set(null);
    scheduleDraw();
  }

  run(() => {
    const s = $settingsStore.render_host ?? "auto";
    if (sceneHolder && s !== _hostSetting) {
      _hostSetting = s;
      initHost();
    }
  });

  onMount(() => {
    hudCtx = hudCanvas.getContext("2d");

    const applySize = (w: number, h: number) => {
      if (w <= 0 || h <= 0) return;
      width = w;
      height = h;
      dpr = window.devicePixelRatio || 1;
      cam.width = w;
      cam.height = h;
    };

    const ro = new ResizeObserver((entries) => {
      const { width: w, height: h } = entries[0].contentRect;
      applySize(w, h);
      if (!cam.initialized) resetCamera();
      else scheduleDraw();
    });
    ro.observe(container);

    window.addEventListener("wheel", handleWheel, { passive: false, capture: true });

    // Po prvním framu zajistíme rozměry z BoundingClientRect — obchází případné
    // timing problémy kdy ResizeObserver vystřelí před finálním CSS grid layoutem.
    requestAnimationFrame(() => {
      const rect = container.getBoundingClientRect();
      applySize(rect.width, rect.height);
      if (!cam.initialized) resetCamera();
      else scheduleDraw();
    });

    // Vývojový háček pro e2e testy (stav kamery a projekce bodů)
    if (import.meta.env.DEV) {
      (window as unknown as { __dpiViewport: unknown }).__dpiViewport = {
        camera: () => ({ ...cam.state(), width: cam.width, height: cam.height }),
        project: (x: number, y: number, z = 0) => cam.project(x, y, z),
        glassTop,
        transforms: () => transforms.map((t) => ({ ...t })),
      };
    }

    return () => {
      ro.disconnect();
      window.removeEventListener("wheel", handleWheel, { capture: true });
      _hostGen++;
      host?.dispose();
      host = null;
      clearTimeout(_recenterTimer);
      clearTimeout(_refineTimer);
    };
  });
</script>

<svelte:window onkeydown={handleKeyDown} onkeyup={handleKeyUp} />

<!-- isolate: z-index HUD platí jen uvnitř plátna, lišta a panely GraphicsView zůstávají nad ním -->
<div bind:this={container} class="relative isolate w-full h-full overflow-hidden" data-testid="viewport" data-ready={ready ? "1" : "0"} data-host={hostKind ?? "none"}>
  <canvas
    bind:this={hudCanvas}
    data-testid="viewport-hud"
    onpointerdown={handlePointerDown}
    onpointermove={handlePointerMove}
    onpointerup={handlePointerUp}
    onpointercancel={handlePointerUp}
    onauxclick={(e) => e.preventDefault()}
    oncontextmenu={handleContextMenu}
    class="absolute inset-0 w-full h-full block z-10"
    style="cursor: {drawTool
      ? 'crosshair'
      : navOp
        ? 'grabbing'
        : hoverGizmo
          ? 'pointer'
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
  <div bind:this={sceneHolder} class="absolute inset-0" style="background: #0f172a"></div>
  {#if hostError}
    <div class="absolute inset-x-6 top-20 z-20 rounded-lg border border-rose-500/40 bg-slate-950/90 p-3 text-xs text-rose-200 pointer-events-none">
      3D náhled nelze vykreslit: {hostError}
    </div>
  {/if}
</div>
