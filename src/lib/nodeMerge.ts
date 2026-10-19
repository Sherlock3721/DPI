/**
 * Sloučení uzlů trasy podle vzdálenosti („merge by distance“).
 *
 * Uzel = jedinečná souřadnice v trase; bod sdílený více segmenty i uzávěr
 * uzavřeného obrysu tvoří jediný uzel s více referencemi. Skupina = shluk uzlů
 * spojených tranzitivně vzdáleností ≤ maxDist. Skupiny jsou navzájem nezávislé —
 * volba kotvy ani vyjmutí uzlu v jedné skupině neovlivní ostatní.
 *
 * Modul je čistý (bez vazby na Svelte): UI drží MergeNodesPopup, vykreslení
 * náhledu Canvas2D přes drawMergePreview.
 */
import type { Point2D, SubstratePaths, Transform } from "./tauri";
import { tpt } from "./canvas2dDraw";

/** Kvantizace souřadnice pro klíč uzlu (1 nm) — totožný bod má totožný klíč. */
const KEY_QUANT = 1e-6;
/** Body bližší než tato mez jsou po sloučení považovány za jeden bod. */
const WELD_EPS = 1e-7;
/** Strop počtu skupin poslaných do náhledu — ochrana proti patologickým trasám. */
const PREVIEW_LIMIT = 3000;
/** Nad tolik nesloučených uzlů se kontrastní tečky nekreslí — byla by z nich kaše. */
const OTHERS_LIMIT = 6000;
/** Kolik sousedních bodů kolem změny vzít do náhledu, aby úsek navazoval na trasu. */
const RUN_CONTEXT = 1;

export interface NodeRef {
  seg: number;
  pt: number;
}

export interface MergeNode {
  /** Stabilní klíč odvozený ze souřadnice — přežije přepočet skupin. */
  key: string;
  /** Pořadí prvního výskytu v trase — určuje číslování uzlů v popupu. */
  order: number;
  /** World souřadnice (mm) — clustering i náhled na plátně. */
  x: number;
  y: number;
  /** Lokální souřadnice trasy — zápis zpět do path dat. */
  lx: number;
  ly: number;
  refs: NodeRef[];
}

export interface MergeGroup {
  nodes: MergeNode[];
  /** Úhlopříčka obalového obdélníku skupiny (mm) — orientační rozptyl. */
  spread: number;
}

export interface NodeFlags {
  /** Uzel je kotvou cílové pozice (jen režim „manuálně“). */
  anchor?: boolean;
  /** Uzel je z merge vyjmut — zůstane na místě. */
  excluded?: boolean;
}
export type NodeFlagMap = Record<string, NodeFlags>;

/** Kde vznikne nový uzel: v těžišti skupiny, nebo v ručně zvolených kotvách. */
export type MergeOrigin = "centroid" | "manual";

export interface MergePreviewNode {
  x: number;
  y: number;
  /** Číslo uzlu ve skupině (1-based) — shodné s popiskem v popupu. */
  label: number;
  excluded: boolean;
  anchor: boolean;
}

export interface MergePreviewGroup {
  /** Index skupiny (1-based) — shodný s popiskem v popupu. */
  index: number;
  nodes: MergePreviewNode[];
  /** Pozice nově vzniklého uzlu; null = skupina nemá co slučovat. */
  target: Point2D | null;
}

/** Úsek výsledné trasy kolem jedné změny — world souřadnice. */
export interface MergeAffectedRun {
  points: Point2D[];
}

export interface MergePreview {
  groups: MergePreviewGroup[];
  /** Uzly trasy mimo merge — kreslí se potlačeně pro kontrast se slučovanými. */
  others: Point2D[];
  /** Úseky trasy tak, jak budou vypadat po sloučení (jen okolí změn). */
  runs: MergeAffectedRun[];
  /** Aktuální mez vzdálenosti (mm) — kreslí se u zvýrazněné skupiny. */
  radius: number;
  /** Index skupiny pod kurzorem v popupu (1-based), 0 = žádná. */
  hovered: number;
}

export interface MergeResult {
  paths: SubstratePaths;
  /** Počet skupin, které se skutečně sloučily. */
  groups: number;
  /** O kolik uzlů trasa ubyla. */
  removedNodes: number;
  /** Počet segmentů zrušených jako degenerované. */
  droppedSegments: number;
}

function bboxDiagonal(nodes: MergeNode[]): number {
  let mnX = Infinity,
    mxX = -Infinity,
    mnY = Infinity,
    mxY = -Infinity;
  for (const n of nodes) {
    if (n.x < mnX) mnX = n.x;
    if (n.x > mxX) mxX = n.x;
    if (n.y < mnY) mnY = n.y;
    if (n.y > mxY) mxY = n.y;
  }
  return Math.hypot(mxX - mnX, mxY - mnY);
}

/** Shodná geometrie dvou tras (na 1 nm) — kritérium pro promítnutí merge dál. */
export function samePathGeometry(a: SubstratePaths | null | undefined, b: SubstratePaths | null | undefined): boolean {
  if (!a || !b || a.segments.length !== b.segments.length) return false;
  for (let si = 0; si < a.segments.length; si++) {
    const pa = a.segments[si].points,
      pb = b.segments[si].points;
    if (pa.length !== pb.length) return false;
    for (let pi = 0; pi < pa.length; pi++) {
      if (Math.abs(pa[pi].x - pb[pi].x) > KEY_QUANT || Math.abs(pa[pi].y - pb[pi].y) > KEY_QUANT)
        return false;
    }
  }
  return true;
}

/** Bod trasy z lokálního prostoru do world space (bez transformace beze změny). */
function toWorldPt(p: Point2D, t: Transform | null): Point2D {
  if (!t) return { x: p.x, y: p.y };
  const [x, y] = tpt(p.x, p.y, t.gui_dx, t.gui_dy, t.scale, t.rotation, t.cx, t.cy);
  return { x, y };
}

/** Uzly trasy ve world space; duplicitní souřadnice se slévají do jednoho uzlu. */
export function collectNodes(paths: SubstratePaths | null, t: Transform | null): MergeNode[] {
  if (!paths) return [];
  const byKey = new Map<string, MergeNode>();
  const nodes: MergeNode[] = [];

  for (let si = 0; si < paths.segments.length; si++) {
    const seg = paths.segments[si];
    for (let pi = 0; pi < seg.points.length; pi++) {
      const p = seg.points[pi];
      const key = `${Math.round(p.x / KEY_QUANT)}:${Math.round(p.y / KEY_QUANT)}`;
      const existing = byKey.get(key);
      if (existing) {
        existing.refs.push({ seg: si, pt: pi });
        continue;
      }
      const w = toWorldPt(p, t);
      const node: MergeNode = {
        key,
        order: nodes.length,
        x: w.x,
        y: w.y,
        lx: p.x,
        ly: p.y,
        refs: [{ seg: si, pt: pi }],
      };
      byKey.set(key, node);
      nodes.push(node);
    }
  }
  return nodes;
}

/**
 * Shluky uzlů do vzdálenosti maxDist. Union-find nad mřížkou o velikosti buňky
 * maxDist — stačí projít 3×3 okolí, takže složitost je lineární v počtu uzlů.
 * Spojení je tranzitivní: řetěz uzlů po maxDist tvoří jednu skupinu.
 */
export function buildMergeGroups(nodes: MergeNode[], maxDist: number): MergeGroup[] {
  if (nodes.length < 2 || !(maxDist > 0)) return [];

  const cell = maxDist;
  const grid = new Map<string, number[]>();
  for (let i = 0; i < nodes.length; i++) {
    const k = `${Math.floor(nodes[i].x / cell)}:${Math.floor(nodes[i].y / cell)}`;
    const bucket = grid.get(k);
    if (bucket) bucket.push(i);
    else grid.set(k, [i]);
  }

  const parent = new Int32Array(nodes.length);
  for (let i = 0; i < nodes.length; i++) parent[i] = i;
  const find = (a: number): number => {
    while (parent[a] !== a) {
      parent[a] = parent[parent[a]];
      a = parent[a];
    }
    return a;
  };

  const maxSq = maxDist * maxDist;
  for (let i = 0; i < nodes.length; i++) {
    const n = nodes[i];
    const gx = Math.floor(n.x / cell),
      gy = Math.floor(n.y / cell);
    for (let dx = -1; dx <= 1; dx++) {
      for (let dy = -1; dy <= 1; dy++) {
        const bucket = grid.get(`${gx + dx}:${gy + dy}`);
        if (!bucket) continue;
        for (const j of bucket) {
          if (j <= i) continue; // dvojici řeší vždy jen její nižší index
          const ddx = nodes[j].x - n.x,
            ddy = nodes[j].y - n.y;
          if (ddx * ddx + ddy * ddy > maxSq) continue;
          const ra = find(i),
            rb = find(j);
          if (ra !== rb) parent[rb] = ra;
        }
      }
    }
  }

  const buckets = new Map<number, MergeNode[]>();
  for (let i = 0; i < nodes.length; i++) {
    const r = find(i);
    const b = buckets.get(r);
    if (b) b.push(nodes[i]);
    else buckets.set(r, [nodes[i]]);
  }

  const groups: MergeGroup[] = [];
  for (const members of buckets.values()) {
    if (members.length < 2) continue;
    members.sort((a, b) => a.order - b.order);
    groups.push({ nodes: members, spread: bboxDiagonal(members) });
  }
  groups.sort((a, b) => a.nodes[0].order - b.nodes[0].order);
  return groups;
}

/** Uzly skupiny, které se budou slučovat (bez ručně vyjmutých). */
export function includedNodes(group: MergeGroup, flags: NodeFlagMap): MergeNode[] {
  return group.nodes.filter((n) => !flags[n.key]?.excluded);
}

/**
 * Pozice nově vzniklého uzlu — world i lokální souřadnice zároveň
 * (transformace trasy je afinní, takže těžiště obou prostorů si odpovídají).
 *
 * „centroid“ = těžiště všech slučovaných uzlů. „manual“ = těžiště aktivních
 * kotev: jedna kotva → přesně na ní, více kotev → uprostřed mezi nimi;
 * bez kotvy padá zpět na těžiště skupiny. null = skupina nemá co slučovat.
 */
export function groupTarget(
  group: MergeGroup,
  flags: NodeFlagMap,
  origin: MergeOrigin
): { world: Point2D; local: Point2D } | null {
  const included = includedNodes(group, flags);
  if (included.length < 2) return null;
  const anchors = origin === "manual" ? included.filter((n) => flags[n.key]?.anchor) : [];
  const base = anchors.length > 0 ? anchors : included;

  let sx = 0,
    sy = 0,
    slx = 0,
    sly = 0;
  for (const n of base) {
    sx += n.x;
    sy += n.y;
    slx += n.lx;
    sly += n.ly;
  }
  const k = base.length;
  return { world: { x: sx / k, y: sy / k }, local: { x: slx / k, y: sly / k } };
}

function samePt(a: Point2D, b: Point2D): boolean {
  return Math.abs(a.x - b.x) <= WELD_EPS && Math.abs(a.y - b.y) <= WELD_EPS;
}

/**
 * Jediný průchod sloučením — sdílí ho zápis geometrie (applyMerge) i náhled.
 * Vedle výsledných bodů vrací příznak `touched`, tedy které body merge posunul
 * nebo vytvořil; z něj náhled odvozuje úseky trasy, které se budou měnit.
 */
interface MergedSegment {
  points: Point2D[];
  touched: boolean[];
  is_filled?: boolean;
}

function mergeSegments(
  src: SubstratePaths,
  groups: MergeGroup[],
  flags: NodeFlagMap,
  origin: MergeOrigin
): {
  segments: MergedSegment[];
  mergedGroups: number;
  removedNodes: number;
  droppedSegments: number;
} {
  // 1) Cílová pozice pro každý slučovaný bod trasy
  const moved = new Map<string, Point2D>();
  let mergedGroups = 0;
  let removedNodes = 0;
  for (const group of groups) {
    const target = groupTarget(group, flags, origin);
    if (!target) continue;
    const included = includedNodes(group, flags);
    mergedGroups++;
    removedNodes += included.length - 1;
    for (const node of included)
      for (const ref of node.refs) moved.set(`${ref.seg}:${ref.pt}`, target.local);
  }

  // 2) Přepis bodů a odstranění po sobě jdoucích duplicit
  const segments: MergedSegment[] = [];
  let droppedSegments = 0;
  for (let si = 0; si < src.segments.length; si++) {
    const seg = src.segments[si];
    const closed =
      seg.points.length > 2 && samePt(seg.points[0], seg.points[seg.points.length - 1]);

    const points: Point2D[] = [];
    const touched: boolean[] = [];
    for (let pi = 0; pi < seg.points.length; pi++) {
      const tgt = moved.get(`${si}:${pi}`);
      const p = tgt ? { x: tgt.x, y: tgt.y } : { x: seg.points[pi].x, y: seg.points[pi].y };
      const last = points.length - 1;
      if (last >= 0 && samePt(points[last], p)) {
        // Bod splynul s předchozím — změna se propíše do toho, co po něm zbylo
        touched[last] = touched[last] || tgt !== undefined;
        continue;
      }
      points.push(p);
      touched.push(tgt !== undefined);
    }

    if (closed) {
      while (points.length > 1 && samePt(points[0], points[points.length - 1])) {
        touched[0] = touched[0] || touched[points.length - 1];
        points.pop();
        touched.pop();
      }
      // Obrys se smrsknutím pod tři body přestává být plochou — zůstane úsečkou.
      if (points.length >= 3) {
        points.push({ ...points[0] });
        touched.push(touched[0]);
      }
    }

    if (points.length < 2) {
      droppedSegments++;
      continue;
    }
    segments.push(
      seg.is_filled === undefined
        ? { points, touched }
        : { points, touched, is_filled: seg.is_filled }
    );
  }

  return { segments, mergedGroups, removedNodes, droppedSegments };
}

/** Provede sloučení a vrátí novou trasu; vstup zůstává nedotčen. */
export function applyMerge(
  src: SubstratePaths,
  groups: MergeGroup[],
  flags: NodeFlagMap,
  origin: MergeOrigin
): MergeResult {
  const r = mergeSegments(src, groups, flags, origin);
  return {
    paths: {
      segments: r.segments.map((seg) =>
        seg.is_filled === undefined
          ? { points: seg.points }
          : { points: seg.points, is_filled: seg.is_filled }
      ),
    },
    groups: r.mergedGroups,
    removedNodes: r.removedNodes,
    droppedSegments: r.droppedSegments,
  };
}

/**
 * Úseky výsledné trasy kolem změn. Každý dotčený bod se rozšíří o RUN_CONTEXT
 * sousedů na obě strany, překrývající se rozsahy splynou — náhled tak ukáže
 * novou podobu trasy včetně napojení, ale nepřekreslí celou trasu.
 */
function buildRuns(segments: MergedSegment[], t: Transform | null): MergeAffectedRun[] {
  const runs: MergeAffectedRun[] = [];
  for (const seg of segments) {
    const ranges: [number, number][] = [];
    for (let i = 0; i < seg.touched.length; i++) {
      if (!seg.touched[i]) continue;
      const a = Math.max(0, i - RUN_CONTEXT);
      const b = Math.min(seg.points.length - 1, i + RUN_CONTEXT);
      const last = ranges[ranges.length - 1];
      if (last && a <= last[1]) last[1] = Math.max(last[1], b);
      else ranges.push([a, b]);
    }
    for (const [a, b] of ranges) {
      if (b <= a) continue; // samotný bod není co kreslit
      const points: Point2D[] = [];
      for (let i = a; i <= b; i++) points.push(toWorldPt(seg.points[i], t));
      runs.push({ points });
      if (runs.length >= PREVIEW_LIMIT) return runs;
    }
  }
  return runs;
}

export interface PreviewInput {
  paths: SubstratePaths | null;
  transform: Transform | null;
  nodes: MergeNode[];
  groups: MergeGroup[];
  flags: NodeFlagMap;
  origin: MergeOrigin;
  maxDist: number;
}

/** Data pro vykreslení živého náhledu na plátně (bez `hovered` — ten se dosazuje zvlášť). */
export function buildPreview(input: PreviewInput): MergePreview {
  const { paths, transform, nodes, groups, flags, origin, maxDist } = input;

  const previewGroups: MergePreviewGroup[] = [];
  const grouped = new Set<string>();
  for (const group of groups) for (const n of group.nodes) grouped.add(n.key);

  // V automatickém režimu se jednotlivé uzly nekreslí ani nevypisují — vybírat
  // z nich není co, stačí výsledná trasa a nově vznikající uzel.
  const showNodes = origin === "manual";

  const count = Math.min(groups.length, PREVIEW_LIMIT);
  for (let gi = 0; gi < count; gi++) {
    const group = groups[gi];
    const target = groupTarget(group, flags, origin);
    previewGroups.push({
      index: gi + 1,
      target: target ? target.world : null,
      nodes: showNodes
        ? group.nodes.map((n, ni) => ({
            x: n.x,
            y: n.y,
            label: ni + 1,
            excluded: !!flags[n.key]?.excluded,
            anchor: !!flags[n.key]?.anchor && !flags[n.key]?.excluded,
          }))
        : [],
    });
  }

  const rest = nodes.length - grouped.size;
  const others: Point2D[] =
    showNodes && rest > 0 && rest <= OTHERS_LIMIT
      ? nodes.filter((n) => !grouped.has(n.key)).map((n) => ({ x: n.x, y: n.y }))
      : [];

  const runs = paths ? buildRuns(mergeSegments(paths, groups, flags, origin).segments, transform) : [];

  return { groups: previewGroups, others, runs, radius: maxDist, hovered: 0 };
}
