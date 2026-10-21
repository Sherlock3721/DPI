/**
 * Orbitální kamera 3D náhledu tiskové plochy.
 *
 * Svět v mm: X doprava, Y nahoru (deska), Z nahoru. Kamera obíhá kolem `target`
 * (pivot); `yaw` otáčí kolem osy Z, `pitch` je výška nad rovinou desky
 * (90° = pohled shora). `scale` = px na mm v rovině targetu — u ortho všude,
 * u perspektivy v hloubce targetu — takže přepnutí projekce měřítko nemění.
 *
 * Pohled shora (yaw 0, pitch 90°, ortho) mapuje přesně jako dřívější 2D plátno:
 * sx = (wx − tx)·scale + W/2, sy = H/2 − (wy − ty)·scale.
 *
 * Matice jsou sloupcové v konvenci WebGPU (clip z ∈ [0, 1]) a posílají se
 * rendereru beze změny — HUD i picking tak sedí s obrazem scény na pixel.
 */

export type Vec3 = [number, number, number];
export type Projection = "ortho" | "persp";

export interface CameraState {
  target: Vec3;
  yaw: number;
  pitch: number;
  scale: number;
  projection: Projection;
}

/** Bod na obrazovce (CSS px) a jeho hloubka (w) — null, pokud je za kamerou. */
export interface ScreenPoint {
  x: number;
  y: number;
  w: number;
}

/** Sklon 0 = pohled přesně z boku (po ose X / Y), 90° = shora (po ose Z). */
export const PITCH_MIN = 0;
export const PITCH_TOP = Math.PI / 2;
const AXIS_EPS = 1e-4;
/** Zorný úhel perspektivy (svisle). */
export const FOV = (35 * Math.PI) / 180;
/** Paprsky skoro rovnoběžné s rovinou (|dir.z| pod touto mezí) se pro editaci nepoužijí. */
const GRAZING = Math.sin((5 * Math.PI) / 180);

const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const add = (a: Vec3, b: Vec3): Vec3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const mul = (a: Vec3, k: number): Vec3 => [a[0] * k, a[1] * k, a[2] * k];
const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const lerp = (a: number, b: number, t: number) => a + (b - a) * t;

/** Úhel do intervalu (−π, π]. */
function wrapAngle(a: number): number {
  a = (a + Math.PI) % (2 * Math.PI);
  if (a < 0) a += 2 * Math.PI;
  return a - Math.PI;
}

export class OrbitCamera {
  target: Vec3 = [0, 0, 0];
  yaw = 0;
  pitch = PITCH_TOP;
  scale = 1;
  projection: Projection = "ortho";
  /** Rozměr výřezu v CSS px. */
  width = 1;
  height = 1;
  /** Rozsah scény pro near/far (min a max roh). */
  sceneMin: Vec3 = [0, 0, 0];
  sceneMax: Vec3 = [1, 1, 1];
  /** Kamera už byla nastavená (náhrada dřívější podmínky zoom === 1 && panX === 0). */
  initialized = false;

  private anim: { from: CameraState; to: CameraState; start: number; ms: number } | null = null;

  /** Kopie pro HUD ke snímku, který dorazí později (nativní hostitel). */
  clone(): OrbitCamera {
    const c = new OrbitCamera();
    c.target = [...this.target];
    c.yaw = this.yaw;
    c.pitch = this.pitch;
    c.scale = this.scale;
    c.projection = this.projection;
    c.width = this.width;
    c.height = this.height;
    c.sceneMin = [...this.sceneMin];
    c.sceneMax = [...this.sceneMax];
    c.initialized = this.initialized;
    return c;
  }

  /** Pohled shora bez natočení (dřívější 2D)? */
  isTopView(): boolean {
    return this.pitch > PITCH_TOP - AXIS_EPS && Math.abs(this.yaw) < AXIS_EPS;
  }

  /** Pohled čistě po ose: shora (po Z, natočení kolem Z nevadí), nebo přesně
   *  z boku po ose X / Y. Takový pohled je vždy ortho — rozměry (i tloušťka
   *  skla) se v něm dají odečíst ve skutečném poměru. */
  isAxisView(): boolean {
    if (this.pitch > PITCH_TOP - AXIS_EPS) return true;
    if (this.pitch > AXIS_EPS) return false;
    const q = this.yaw / (Math.PI / 2);
    return Math.abs(q - Math.round(q)) < AXIS_EPS;
  }

  /** Pohled shora po ose Z (natočení kolem Z nevadí)? */
  isTopDown(): boolean {
    return this.pitch > PITCH_TOP - AXIS_EPS;
  }

  /** Pohled po ose → ortho. */
  private enforceAxisOrtho() {
    if (this.isAxisView()) this.projection = "ortho";
  }

  // ─── Báze ────────────────────────────────────────────────────────────────
  // Z yaw/pitch přímo (lookAt má v pohledu shora singularitu).

  /** Vpravo na obrazovce (world). */
  right(): Vec3 {
    return [Math.cos(this.yaw), Math.sin(this.yaw), 0];
  }
  /** Nahoru na obrazovce (world). */
  up(): Vec3 {
    const s = Math.sin(this.pitch),
      c = Math.cos(this.pitch);
    return [-Math.sin(this.yaw) * s, Math.cos(this.yaw) * s, c];
  }
  /** Směr pohledu (od oka do scény). */
  forward(): Vec3 {
    const s = Math.sin(this.pitch),
      c = Math.cos(this.pitch);
    return [-Math.sin(this.yaw) * c, Math.cos(this.yaw) * c, -s];
  }

  /** Vzdálenost oka od targetu v perspektivě (z měřítka a výšky výřezu). */
  distance(): number {
    return this.height / 2 / (this.scale * Math.tan(FOV / 2));
  }

  /** Poloha oka (u ortho bod na ose pohledu za scénou). */
  eye(): Vec3 {
    const d = this.projection === "persp" ? this.distance() : this.orthoBack();
    return sub(this.target, mul(this.forward(), d));
  }

  /** Odstup oka u ortho — za celou scénou, aby nic neleželo za near. */
  private orthoBack(): number {
    const ext = sub(this.sceneMax, this.sceneMin);
    return Math.hypot(ext[0], ext[1], ext[2]) + Math.hypot(...sub(this.target, this.sceneMin)) + 10;
  }

  /** Near/far z obálky scény (dynamicky kvůli přesnosti hloubky). */
  private depthRange(): [number, number] {
    const eye = this.eye();
    const f = this.forward();
    let mn = Infinity,
      mx = -Infinity;
    for (let i = 0; i < 8; i++) {
      const p: Vec3 = [
        i & 1 ? this.sceneMax[0] : this.sceneMin[0],
        i & 2 ? this.sceneMax[1] : this.sceneMin[1],
        i & 4 ? this.sceneMax[2] : this.sceneMin[2],
      ];
      const d = dot(sub(p, eye), f);
      mn = Math.min(mn, d);
      mx = Math.max(mx, d);
    }
    if (this.projection === "ortho") return [Math.max(0.01, mn - 1), mx + 1];
    const dist = this.distance();
    const near = Math.max(dist * 0.01, mn * 0.9, 0.01);
    return [near, Math.max(mx * 1.1, near * 2)];
  }

  // ─── Matice ──────────────────────────────────────────────────────────────

  /** Svět → clip (sloupcově, WebGPU: z ∈ [0, 1]). */
  viewProj(): Float32Array {
    const r = this.right(),
      u = this.up(),
      f = this.forward();
    const b: Vec3 = [-f[0], -f[1], -f[2]];
    const e = this.eye();
    // View: řádky r, u, b; posun −dot(·, eye)
    const v = [
      [r[0], r[1], r[2], -dot(r, e)],
      [u[0], u[1], u[2], -dot(u, e)],
      [b[0], b[1], b[2], -dot(b, e)],
      [0, 0, 0, 1],
    ];
    const [near, far] = this.depthRange();
    const aspect = this.width / this.height;
    let p: number[][];
    if (this.projection === "persp") {
      const fy = 1 / Math.tan(FOV / 2);
      p = [
        [fy / aspect, 0, 0, 0],
        [0, fy, 0, 0],
        [0, 0, far / (near - far), (near * far) / (near - far)],
        [0, 0, -1, 0],
      ];
    } else {
      const hw = this.width / (2 * this.scale),
        hh = this.height / (2 * this.scale);
      // Ortho kolem targetu: posun targetu do středu je už ve view (r, u míří z oka na osu)
      p = [
        [1 / hw, 0, 0, 0],
        [0, 1 / hh, 0, 0],
        [0, 0, 1 / (near - far), near / (near - far)],
        [0, 0, 0, 1],
      ];
    }
    const out = new Float32Array(16);
    for (let col = 0; col < 4; col++)
      for (let row = 0; row < 4; row++) {
        let acc = 0;
        for (let k = 0; k < 4; k++) acc += p[row][k] * v[k][col];
        out[col * 4 + row] = acc;
      }
    return out;
  }

  // ─── Projekce a paprsky ──────────────────────────────────────────────────

  /** Svět → obrazovka (CSS px). Null, pokud bod leží za kamerou. */
  project(x: number, y: number, z = 0): ScreenPoint | null {
    const p = sub([x, y, z], this.target);
    const rx = dot(p, this.right()),
      uy = dot(p, this.up());
    if (this.projection === "ortho") {
      return { x: this.width / 2 + rx * this.scale, y: this.height / 2 - uy * this.scale, w: 1 };
    }
    const dist = this.distance();
    const depth = dist + dot(p, this.forward());
    if (depth <= dist * 1e-3) return null;
    const k = (this.scale * dist) / depth;
    return { x: this.width / 2 + rx * k, y: this.height / 2 - uy * k, w: depth / dist };
  }

  /** Paprsek z bodu obrazovky: počátek a normalizovaný směr. */
  ray(sx: number, sy: number): { o: Vec3; d: Vec3 } {
    const r = this.right(),
      u = this.up(),
      f = this.forward();
    const ox = (sx - this.width / 2) / this.scale;
    const oy = (this.height / 2 - sy) / this.scale;
    if (this.projection === "ortho") {
      const o = add(add(this.target, mul(r, ox)), mul(u, oy));
      return { o: sub(o, mul(f, this.orthoBack())), d: f };
    }
    const dist = this.distance();
    const d = add(f, add(mul(r, ox / dist), mul(u, oy / dist)));
    const len = Math.hypot(d[0], d[1], d[2]);
    return { o: this.eye(), d: mul(d, 1 / len) };
  }

  /** Průsečík paprsku s vodorovnou rovinou z. Null pro paprsek skoro
   *  rovnoběžný s rovinou (editace by v bočním pohledu „ujížděla“) nebo za kamerou. */
  screenToPlane(sx: number, sy: number, z = 0, allowGrazing = false): { x: number; y: number } | null {
    const { o, d } = this.ray(sx, sy);
    if (Math.abs(d[2]) < (allowGrazing ? 1e-6 : GRAZING)) return null;
    const t = (z - o[2]) / d[2];
    if (t < 0) return null;
    return { x: o[0] + d[0] * t, y: o[1] + d[1] * t };
  }

  /** Lze v aktuálním pohledu editovat v rovině desky? (Ne v bočním pohledu.) */
  canEditPlane(): boolean {
    return Math.sin(this.pitch) >= GRAZING * 1.5;
  }

  /** px na mm v hloubce bodu (u ortho všude stejně). */
  pxPerMm(x = this.target[0], y = this.target[1], z = this.target[2]): number {
    if (this.projection === "ortho") return this.scale;
    const dist = this.distance();
    const depth = dist + dot(sub([x, y, z], this.target), this.forward());
    return depth > 1e-6 ? (this.scale * dist) / depth : this.scale;
  }

  /** Nejmenší px na mm v rovině desky — ve šikmém pohledu je směr od kamery
   *  zkrácený, tolerance v mm z něj jsou konzervativní. */
  pxPerMmMin(x?: number, y?: number, z?: number): number {
    return this.pxPerMm(x, y, z) * Math.max(Math.sin(this.pitch), 0.05);
  }

  // ─── Úpravy pohledu ──────────────────────────────────────────────────────

  state(): CameraState {
    return { target: [...this.target], yaw: this.yaw, pitch: this.pitch, scale: this.scale, projection: this.projection };
  }

  set(s: Partial<CameraState>) {
    if (s.target) this.target = [...s.target];
    if (s.yaw !== undefined) this.yaw = wrapAngle(s.yaw);
    if (s.pitch !== undefined) this.pitch = Math.max(PITCH_MIN, Math.min(PITCH_TOP, s.pitch));
    if (s.scale !== undefined) this.scale = s.scale;
    if (s.projection) this.projection = s.projection;
    this.enforceAxisOrtho();
  }

  /** Pohled shora na obdélník [x0,x1]×[y0,y1] s okrajem v px (dřívější resetCamera). */
  fitTop(x0: number, y0: number, x1: number, y1: number, marginPx: number) {
    this.anim = null;
    this.yaw = 0;
    this.pitch = PITCH_TOP;
    this.projection = "ortho";
    this.scale = Math.min((this.width - marginPx * 2) / (x1 - x0), (this.height - marginPx * 2) / (y1 - y0));
    this.target = [(x0 + x1) / 2, (y0 + y1) / 2, 0];
    this.initialized = true;
  }

  /** Měřítko a target tak, aby se obálka vešla do výřezu při zachované orientaci. */
  fitBox(min: Vec3, max: Vec3, marginPx: number): Partial<CameraState> {
    const r = this.right(),
      u = this.up();
    let rx0 = Infinity,
      rx1 = -Infinity,
      uy0 = Infinity,
      uy1 = -Infinity;
    for (let i = 0; i < 8; i++) {
      const p: Vec3 = [i & 1 ? max[0] : min[0], i & 2 ? max[1] : min[1], i & 4 ? max[2] : min[2]];
      const a = dot(p, r),
        b = dot(p, u);
      rx0 = Math.min(rx0, a);
      rx1 = Math.max(rx1, a);
      uy0 = Math.min(uy0, b);
      uy1 = Math.max(uy1, b);
    }
    const w = Math.max(rx1 - rx0, 1e-3),
      h = Math.max(uy1 - uy0, 1e-3);
    const scale = Math.min((this.width - marginPx * 2) / w, (this.height - marginPx * 2) / h);
    const target: Vec3 = [(min[0] + max[0]) / 2, (min[1] + max[1]) / 2, (min[2] + max[2]) / 2];
    return { target, scale };
  }

  /** Orbit o úhly (rad). */
  orbit(dYaw: number, dPitch: number) {
    this.set({ yaw: this.yaw + dYaw, pitch: this.pitch + dPitch });
  }

  /** Zoom faktorem k bodu P (zůstane na obrazovce na místě). */
  zoomAt(factor: number, p: Vec3 | null, minScale: number, maxScale: number) {
    const s1 = Math.max(minScale, Math.min(this.scale * factor, maxScale));
    const k = this.scale / s1;
    if (p) {
      let off = mul(sub(this.target, p), k);
      if (this.projection === "ortho") {
        // Ortho: posun jen v rovině obrazovky, hloubka pivotu zůstává
        const f = this.forward();
        const along = dot(sub(this.target, p), f);
        off = add(off, mul(f, along * (1 - k)));
      }
      this.target = add(p, off);
    }
    this.scale = s1;
  }

  /** Přepnutí projekce — měřítko v rovině targetu zůstává, obraz neskočí.
   *  Pohled po ose zůstává ortho. */
  setProjection(p: Projection) {
    this.projection = p;
    this.enforceAxisOrtho();
  }

  // ─── Animace ─────────────────────────────────────────────────────────────

  animateTo(to: Partial<CameraState>, ms = 250) {
    const from = this.state();
    const target = { ...from, ...to, target: to.target ? ([...to.target] as Vec3) : from.target };
    // Nejkratší cesta kolem osy
    target.yaw = from.yaw + wrapAngle((to.yaw ?? from.yaw) - from.yaw);
    this.anim = { from, to: target, start: performance.now(), ms };
  }

  get animating(): boolean {
    return this.anim !== null;
  }

  /** Posune animaci; vrací true, pokud ještě běží. */
  tick(now = performance.now()): boolean {
    const a = this.anim;
    if (!a) return false;
    const t = Math.min(1, (now - a.start) / a.ms);
    const e = 1 - Math.pow(1 - t, 3);
    this.target = [
      lerp(a.from.target[0], a.to.target[0], e),
      lerp(a.from.target[1], a.to.target[1], e),
      lerp(a.from.target[2], a.to.target[2], e),
    ];
    this.yaw = wrapAngle(lerp(a.from.yaw, a.to.yaw, e));
    this.pitch = lerp(a.from.pitch, a.to.pitch, e);
    // Měřítko geometricky (zoom působí rovnoměrně)
    this.scale = a.from.scale * Math.pow(a.to.scale / a.from.scale, e);
    this.projection = t < 1 ? a.from.projection : a.to.projection;
    if (t >= 1) this.anim = null;
    this.enforceAxisOrtho();
    return this.anim !== null;
  }

  /** Dokončí běžící animaci okamžitě. */
  finishAnimation() {
    if (this.anim) this.tick(this.anim.start + this.anim.ms);
  }

  cancelAnimation() {
    this.anim = null;
  }
}
