/**
 * Hostitelé rendereru dpi-render. Stejný renderer (Rust/wgpu) běží:
 *  - nativně v aplikaci (src-tauri/src/viewport.rs): kreslí mimo WebView
 *    a hotový snímek (RGBA) posílá raw IPC do 2D canvasu. Nezávisí na WebGL
 *    ve WebKitGTK, které na Linuxu umí tiše běžet softwarově.
 *  - jako wasm nad WebGL2 přímo v canvasu (Windows, prohlížeč, e2e).
 */
import { invoke } from "@tauri-apps/api/core";
import type { RenderHostSetting } from "../tauri";

export type HostKind = "native" | "wasm";

export interface AdapterInfo {
  name: string;
  backend: string;
  device_type: string;
  driver?: string;
}

export interface ViewportHost {
  readonly kind: HostKind;
  readonly info: AdapterInfo;
  applyScene(bytes: Uint8Array): Promise<void>;
  /** Vykreslí snímek do canvasu hostitele (w × h px cíle). */
  render(frame: Uint8Array, w: number, h: number): Promise<void>;
  /** Uvolní renderer; u nativního hostitele až po doběhnutí (jinak by se
   *  ukončení vlákna mohlo předběhnout s inicializací nového). */
  dispose(): Promise<void>;
}

export const isTauri = typeof window !== "undefined" && (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !== undefined;
const isLinux = typeof navigator !== "undefined" && /Linux/.test(navigator.userAgent) && !/Android/.test(navigator.userAgent);

/** Pořadí hostitelů k vyzkoušení podle nastavení a systému. */
export function hostOrder(setting: RenderHostSetting | undefined): HostKind[] {
  if (!isTauri) return ["wasm"];
  const preferred: HostKind = setting === "native" || setting === "wasm" ? setting : isLinux ? "native" : "wasm";
  return preferred === "native" ? ["native", "wasm"] : ["wasm", "native"];
}

export async function createHost(kind: HostKind, canvas: HTMLCanvasElement): Promise<ViewportHost> {
  return kind === "native" ? createNativeHost(canvas) : createWasmHost(canvas);
}

async function createNativeHost(canvas: HTMLCanvasElement): Promise<ViewportHost> {
  const info = await invoke<AdapterInfo>("viewport_init", { forceFallback: false });
  const ctx = canvas.getContext("2d", { alpha: false });
  if (!ctx) throw new Error("2D kontext pro nativní snímky není k dispozici");
  let disposed = false;
  return {
    kind: "native",
    info,
    async applyScene(bytes) {
      await invoke("viewport_scene", bytes);
    },
    async render(frame, w, h) {
      const buf = await invoke<ArrayBuffer>("viewport_frame", frame);
      if (disposed) return;
      if (buf.byteLength !== w * h * 4) throw new Error(`Snímek má ${buf.byteLength} B, čekáno ${w * h * 4}`);
      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w;
        canvas.height = h;
      }
      ctx.putImageData(new ImageData(new Uint8ClampedArray(buf), w, h), 0, 0);
    },
    async dispose() {
      disposed = true;
      await invoke("viewport_dispose").catch(() => {});
    },
  };
}

/** Rozhraní wasm buildu dpi-render (src/wasm-render, generuje `npm run wasm:build`). */
interface WasmViewportApi {
  adapter_info(): string;
  apply_scene(bytes: Uint8Array): void;
  render(frame: Uint8Array): void;
  free(): void;
}
interface WasmRenderModule {
  WasmViewport: { create(canvas: HTMLCanvasElement): Promise<WasmViewportApi> };
}

// Glob místo přímého importu: bez sestaveného modulu dostane uživatel hlášku
// v plátně (a nativní hostitel přednost), ne pád celé aplikace při načtení.
const wasmModules = import.meta.glob<WasmRenderModule>("../../wasm-render/dpi_render.js");

async function createWasmHost(canvas: HTMLCanvasElement): Promise<ViewportHost> {
  const load = wasmModules["../../wasm-render/dpi_render.js"];
  if (!load) throw new Error("wasm renderer není sestavený (npm run wasm:build)");
  const mod = await load();
  const vp = await mod.WasmViewport.create(canvas);
  const info = JSON.parse(vp.adapter_info()) as AdapterInfo;
  let disposed = false;
  return {
    kind: "wasm",
    info,
    async applyScene(bytes) {
      vp.apply_scene(bytes);
    },
    async render(frame) {
      // Velikost canvasu (w × h z hlavičky snímku) nastavuje renderer sám
      if (!disposed) vp.render(frame);
    },
    async dispose() {
      if (disposed) return;
      disposed = true;
      vp.free();
    },
  };
}
