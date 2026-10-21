/**
 * Výšky pro 3D náhled — tloušťka skla, mezera trysky a průřez housenky.
 * Počítá je Rust (`dpi-core::print_heights`) ze stejných efektivních nastavení
 * (objekt → sklo → globál) jako G-kód; tady jen převod na rozměry housenky.
 */
import { wasm_print_heights } from "../dpiWasm";
import { paramsForRust, type ProcessParams, type SlideOverride } from "../tauri";

export interface DepositHeights {
  /** Mezera špičky trysky nad sklem (mm). */
  gap: number;
  /** Průřez housenky (mm² = µl/mm); 0 = neznámý. */
  area: number;
}

export interface SlideHeights extends DepositHeights {
  glass_z: number;
  objects: Record<string, DepositHeights>;
}

export type HeightsMap = Record<string, SlideHeights>;

/** Výšky všech pozic (klíč "-1" = odpliv, "0".. = vzorková skla). Volající
 *  si výsledek drží — parametry se mění i na místě (předvolba skla), takže
 *  změnu pozná jen podle obsahu, ne podle identity objektu. */
export function printHeights(params: ProcessParams, overrides: Record<string, SlideOverride>, calibrationFactor: number): HeightsMap {
  try {
    return JSON.parse(wasm_print_heights(JSON.stringify(paramsForRust(params)), JSON.stringify(overrides ?? {}), calibrationFactor));
  } catch (e) {
    console.error("Výpočet výšek pro 3D náhled selhal:", e);
    return {};
  }
}

/** Klíč výšek pozice: odpliv "-1", jinak index skla. */
export function heightsKey(isPrime: boolean, transformIdx: number): string {
  return isPrime ? "-1" : String(transformIdx);
}

/** Tloušťka skla pozice (mm); bez výpočtu výchozí tloušťka vzorkového skla. */
export function glassThickness(h: HeightsMap, key: string, fallback: number): number {
  const z = h[key]?.glass_z;
  return Number.isFinite(z) && z! > 0 ? z! : fallback;
}

/** Rozměry housenky objektu: šířka = šířka čáry, výška z objemu (půlelipsa:
 *  A = π·w·h/4), bez známého objemu z mezery trysky. */
export function beadSize(h: HeightsMap, key: string, objectId: number | null, lineWidth: number): { w: number; h: number; gap: number } {
  const sl = h[key];
  const d = (objectId != null ? sl?.objects?.[String(objectId)] : undefined) ?? sl;
  const w = Math.max(lineWidth, 0.01);
  const gap = d?.gap ?? 0.05;
  const area = d?.area ?? 0;
  const hh = area > 0 ? Math.min((4 * area) / (Math.PI * w), 3 * w) : Math.max(gap, 0.25 * w);
  return { w, h: Math.max(hh, 0.005), gap };
}
