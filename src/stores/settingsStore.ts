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
  safe_z: 20.0,
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
  camera_device_id: "",
  show_slide_grid: true,
  show_bed_axes: true,
  path_fineness: 1.0,
  z_step: 0.0025,
  liquid_density: 1.0,
  liquid_defs: {},
  theme: "dark",
  disable_snow: false,
  recent_files: [],
};

/**
 * Jednorázová migrace UI stavu z localStorage do settings.json.
 * Vrací patch (jen klíče, které v localStorage skutečně byly) — po aplikaci
 * se staré klíče smažou, takže migrace proběhne jen jednou.
 */
function migrateFromLocalStorage(): Partial<AppSettings> | null {
  const patch: Partial<AppSettings> = {};
  const theme = localStorage.getItem("app-theme");
  if (theme === "dark" || theme === "light") patch.theme = theme;

  const snow = localStorage.getItem("disable-snow");
  if (snow !== null) patch.disable_snow = snow === "1";

  const camId = localStorage.getItem("preferredCameraId");
  if (camId) patch.camera_device_id = camId;
  const camRot = localStorage.getItem("preferredCameraRotation");
  if (camRot !== null && !isNaN(parseInt(camRot))) patch.camera_rotation = parseInt(camRot);
  const camMirror = localStorage.getItem("preferredCameraMirror");
  if (camMirror !== null) patch.camera_mirror = camMirror === "true";

  const recent = localStorage.getItem("dpi_recent_files");
  if (recent) {
    try {
      const parsed = JSON.parse(recent);
      if (Array.isArray(parsed)) patch.recent_files = parsed;
    } catch {
      // poškozený záznam — ignorovat
    }
  }

  if (Object.keys(patch).length === 0) return null;
  for (const key of [
    "app-theme",
    "disable-snow",
    "preferredCameraId",
    "preferredCameraRotation",
    "preferredCameraMirror",
    "dpi_recent_files",
  ]) {
    localStorage.removeItem(key);
  }
  return patch;
}

function createSettingsStore() {
  const { subscribe, set, update } = writable<AppSettings>(defaultSettings);

  const store = {
    subscribe,
    set,
    update,
    load: async () => {
      try {
        const setts = await get_app_settings();
        const migrated = migrateFromLocalStorage();
        const merged = { ...defaultSettings, ...setts, ...(migrated ?? {}) };
        set(merged);
        if (migrated) await save_app_settings(merged);
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
    /** Aplikuje částečnou změnu na aktuální stav a hned ji uloží na disk.
     *  Pro průběžné UI preference (theme, kamera, recent files). */
    persistPatch: async (patch: Partial<AppSettings>) => {
      let next: AppSettings = defaultSettings;
      update((s) => {
        next = { ...s, ...patch };
        return next;
      });
      try {
        await save_app_settings(next);
      } catch (err) {
        console.error("Failed to persist settings patch:", err);
      }
    },
  };
  return store;
}

export const settingsStore = createSettingsStore();
