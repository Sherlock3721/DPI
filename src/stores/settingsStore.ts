import { writable } from "svelte/store";
import { get_app_settings, save_app_settings, type AppSettings } from "../lib/tauri";

export type { AppSettings };

const defaultSettings: AppSettings = {
  bed_max_x: 250.0,
  bed_max_y: 210.0,
  bed_min_x: 0.0,
  bed_min_temp: 30,
  start_offset_x: 18.0,
  start_offset_y: 11.0,
  multi_spacing: 5.0,
  block_height: 34.0,
  hidden_nozzle_part: 4.0,
  print_speed: 1500,
  start_gcode: "",
  end_gcode: "",
  loop_start_gcode: "",
  loop_end_gcode: "",
  default_z_offset: 0.2,
  default_z_hop: 2.0,
  default_speed: 1500,
  default_infill: 1.0,
  default_density: 0.05,
  filament_diameter: 9.5,
  flow_multiplier: 1.0,
  calibration_factor: 0.323877,
  sklo_dims: {},
  nozzle_defs: {},
  leveling_circle_diameter: 8.0,
  camera_rotation: 180,
  camera_mirror: false,
  show_slide_grid: true,
  show_bed_axes: true,
  path_fineness: 1.0,
  z_step: 0.0025,
  liquid_density: 1.0,
  liquid_defs: {},
};

function createSettingsStore() {
  const { subscribe, set, update } = writable<AppSettings>(defaultSettings);

  return {
    subscribe,
    set,
    update,
    load: async () => {
      try {
        const setts = await get_app_settings();
        set({ ...defaultSettings, ...setts });
      } catch (err) {
        console.error("Failed to load settings:", err);
      }
    },
    save: async (newSettings: AppSettings) => {
      try {
        await save_app_settings(newSettings);
        set({ ...defaultSettings, ...newSettings });
      } catch (err) {
        console.error("Failed to save settings:", err);
      }
    },
  };
}

export const settingsStore = createSettingsStore();
