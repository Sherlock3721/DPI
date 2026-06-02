/**
 * Zpracování vektorových drah — nyní deleguje na Rust backend přes Tauri.
 * TypeScript implementace je odstraněna, logika žije v dpi-core/src/path_processing.rs.
 *
 * Pro bounding box výpočet (čistá TS funkce bez Tauri závislosti):
 */
import type { PathSegment } from "./tauri";

export function getBoundingBoxOfPaths(segments: PathSegment[]) {
  let minX = Infinity,
    maxX = -Infinity;
  let minY = Infinity,
    maxY = -Infinity;
  let hasPoints = false;
  for (const seg of segments) {
    for (const pt of seg.points) {
      if (pt.x < minX) minX = pt.x;
      if (pt.x > maxX) maxX = pt.x;
      if (pt.y < minY) minY = pt.y;
      if (pt.y > maxY) maxY = pt.y;
      hasPoints = true;
    }
  }
  return { minX, maxX, minY, maxY, hasPoints };
}
