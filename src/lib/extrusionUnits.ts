export type ExtUnit = "µl/mm" | "nl/mm" | "kroky/mm";

const TO_UL: Record<string, number> = {
  "µl/mm": 1.0,
  "nl/mm": 0.001,
};

/**
 * Převede hodnotu extruze z jedné jednotky do druhé.
 * calFactor je kalibrační faktor stroje [kroky/µl].
 */
export function convertExtrusionRate(
  value: number,
  fromUnit: ExtUnit,
  toUnit: ExtUnit,
  calFactor: number
): number {
  if (fromUnit === toUnit) return value;

  const baseUl =
    fromUnit === "kroky/mm" ? value / calFactor : value * TO_UL[fromUnit];

  return toUnit === "kroky/mm" ? baseUl * calFactor : baseUl / TO_UL[toUnit];
}

/**
 * Normalizuje hodnotu extruze na µl/mm — kanonická jednotka pro Rust backend.
 */
export function toCanonicalExtrusionRate(
  value: number,
  unit: ExtUnit,
  calFactor: number
): number {
  return convertExtrusionRate(value, unit, "µl/mm", calFactor);
}
