import { writable, derived } from "svelte/store";
import { settingsStore } from "./settingsStore";

export const selectedLiquidName = writable<string | null>(null);

export interface LiquidLimits {
  color: string;
  z_offset_min: number | null;
  z_offset_max: number | null;
  extrusion_min: number | null;
  extrusion_max: number | null;
  print_speed_min: number | null;
  print_speed_max: number | null;
  bed_temp_min: number | null;
  bed_temp_max: number | null;
  forbidden_nozzles: string[];
}

export const liquidLimits = derived(
  [selectedLiquidName, settingsStore],
  ([$name, $settings]) => {
    if (!$name) return null;
    const def = ($settings.liquid_defs ?? ({} as Record<string, any>))[$name];
    if (!def) return null;
    return {
      color: def.color ?? "#3b82f6",
      z_offset_min: def.z_offset_min ?? null,
      z_offset_max: def.z_offset_max ?? null,
      extrusion_min: def.extrusion_min ?? null,
      extrusion_max: def.extrusion_max ?? null,
      print_speed_min: def.print_speed_min ?? null,
      print_speed_max: def.print_speed_max ?? null,
      bed_temp_min: def.bed_temp_min ?? null,
      bed_temp_max: def.bed_temp_max ?? null,
      forbidden_nozzles: Array.isArray(def.forbidden_nozzles) ? def.forbidden_nozzles : [],
    } as LiquidLimits;
  }
);
