use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Výchozí nastavení vestavěné do binárky — viz src-tauri/default_settings.json.
/// include_str! zaručí, že se soubor zvaliduje při kompilaci (existence)
/// a nemíchá se JSON s Rust kódem.
pub const DEFAULT_SETTINGS_JSON: &str = include_str!("../default_settings.json");

/// Bod nivelace podložky.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelingPoint {
    pub name: String,
    pub x: f64,
    pub y: f64,
}

/// Položka seznamu nedávno otevřených souborů (dřív v localStorage).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentFileEntry {
    pub path: String,
    pub name: String,
    pub timestamp: f64,
}

/// Typovaná podoba settings.json — zrcadlí AppSettings v src/lib/tauri.ts.
///
/// Chybějící klíče doplní [`merge_with_defaults`] z DEFAULT_SETTINGS_JSON
/// (jediný zdroj výchozích hodnot — žádné serde defaulty zde, aby hodnoty
/// nebyly na dvou místech). Neznámé klíče (budoucí rozšíření) se zachovávají
/// v `extra` a beze změny roundtripují zpět na disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    // Tisková plocha
    pub bed_max_x: f64,
    pub bed_max_y: f64,
    pub bed_min_x: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bed_max_temp: Option<f64>,
    pub bed_min_temp: f64,
    // Polohovací offsety a rozestupy
    pub start_offset_x: f64,
    pub start_offset_y: f64,
    pub multi_spacing: f64,
    // Mechanika tiskárny
    pub block_height: f64,
    pub hidden_nozzle_part: f64,
    pub print_speed: f64,
    // G-kód makra
    pub start_gcode: String,
    pub end_gcode: String,
    pub loop_start_gcode: String,
    pub loop_end_gcode: String,
    #[serde(default)]
    pub nozzle_change_gcode: String,
    #[serde(default)]
    pub print_order: String,
    #[serde(default)]
    pub route_mode: String,
    // Výchozí procesní hodnoty
    pub default_z_offset: f64,
    pub default_z_hop: f64,
    pub safe_z: f64,
    // Dávkování kapalin
    /// Zpětné nasátí kapaliny před přejezdem se zdvihem (nl); 0 = vypnuto.
    pub retract_nl: f64,
    /// Rychlost retrakce a návratu kapaliny (posuv E, mm/min).
    pub retract_speed: f64,
    /// Čekání po dávce kapky, než se tryska zvedne (ms).
    pub dot_dwell_ms: f64,
    pub default_speed: f64,
    pub default_infill: f64,
    pub default_density: f64,
    // Extruze a kalibrace
    pub filament_diameter: f64,
    pub flow_multiplier: f64,
    pub calibration_factor: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calibration_object_height: Option<f64>,
    // Definice sklíček a trysek; délky polí historicky kolísají → Vec, ne [_; N]
    pub sklo_dims: serde_json::Map<String, Value>,
    pub nozzle_defs: serde_json::Map<String, Value>,
    // Nivelace
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leveling_points: Option<Vec<LevelingPoint>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leveling_circle_diameter: Option<f64>,
    // Kamera
    pub camera_rotation: f64,
    pub camera_mirror: bool,
    pub camera_device_id: String,
    // UI
    pub show_slide_grid: bool,
    pub show_bed_axes: bool,
    /// Šipky směru tisku u drah v plátně.
    #[serde(default)]
    pub show_print_direction: bool,
    /// Přehled statistik G-kódu v plátně.
    #[serde(default = "default_true")]
    pub show_gcode_stats: bool,
    /// Přehled pozice a rozměrů vybrané trasy / objektu v plátně.
    #[serde(default = "default_true")]
    pub show_object_info: bool,
    /// 3D náhled: barvení drah — "substrate" | "object" | "order" (pořadí tisku).
    #[serde(default = "default_view_color_mode")]
    pub view_color_mode: String,
    /// 3D náhled: přejezdy trysky včetně Z-hopu.
    #[serde(default)]
    pub show_travels: bool,
    /// 3D náhled: převýšení výšky housenek (×).
    #[serde(default = "default_z_exaggeration")]
    pub z_exaggeration: f64,
    /// Otočení z ortho pohledu přepne do perspektivy, pohled po ose zpět do ortho.
    #[serde(default = "default_true")]
    pub auto_perspective: bool,
    /// Klik na substrát / objekt plynule vycentruje pohled.
    #[serde(default = "default_true")]
    pub center_on_click: bool,
    /// Vykreslování 3D náhledu — "auto" | "native" (wgpu v aplikaci) | "wasm" (WebGL2 ve WebView).
    #[serde(default = "default_render_host")]
    pub render_host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_fineness: Option<f64>,
    pub z_step: f64,
    pub liquid_density: f64,
    /// Definice kapalin — hluboce vnořená volná struktura, validuje frontend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liquid_defs: Option<Value>,
    // UI stav (dřív roztroušený v localStorage)
    pub theme: String,
    pub disable_snow: bool,
    pub recent_files: Vec<RecentFileEntry>,
    /// Neznámé klíče z JSON souboru — zachovávají se beze změny.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

/// Položí uložené hodnoty přes vestavěné defaulty: klíče chybějící ve
/// `stored` (např. settings.json ze starší verze aplikace) dostanou
/// výchozí hodnotu, vše ostatní zůstává z disku.
fn merge_with_defaults(stored: Value) -> Value {
    let mut base: Value =
        serde_json::from_str(DEFAULT_SETTINGS_JSON).expect("default_settings.json je validní");
    if let (Value::Object(base_map), Value::Object(stored_map)) = (&mut base, stored) {
        for (k, v) in stored_map {
            base_map.insert(k, v);
        }
    }
    base
}

/// Načte settings z JSON textu: merge s defaulty + typová validace.
/// Při nevalidních typech (ručně rozbitý soubor) vrací chybu s popisem.
pub fn parse_settings(stored_json: &str) -> Result<AppSettings, String> {
    let stored: Value = serde_json::from_str(stored_json)
        .map_err(|e| format!("settings.json není validní JSON: {e}"))?;
    if !stored.is_object() {
        return Err("settings.json musí být JSON objekt".to_string());
    }
    serde_json::from_value(merge_with_defaults(stored))
        .map_err(|e| format!("settings.json má neplatnou strukturu: {e}"))
}

/// Vestavěné výchozí nastavení (validované při testech).
fn default_view_color_mode() -> String {
    "substrate".to_string()
}

fn default_z_exaggeration() -> f64 {
    1.0
}

fn default_render_host() -> String {
    "auto".to_string()
}

fn default_true() -> bool {
    true
}

pub fn default_settings() -> AppSettings {
    serde_json::from_value(merge_with_defaults(Value::Object(Default::default())))
        .expect("default_settings.json odpovídá struktuře AppSettings")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings_parse() {
        let s = default_settings();
        assert!(s.bed_max_x > 0.0);
        assert!(s.safe_z > 0.0);
        assert!(!s.start_gcode.is_empty());
        assert!(s.extra.is_empty(), "defaulty nesmí mít netypované klíče");
    }

    #[test]
    fn test_merge_fills_missing_keys() {
        // Starý settings.json jen s jedním klíčem — zbytek doplní defaulty
        let s = parse_settings(r#"{ "bed_max_x": 300.0 }"#).unwrap();
        assert_eq!(s.bed_max_x, 300.0);
        assert_eq!(s.bed_max_y, 210.0);
        assert_eq!(s.safe_z, 20.0);
    }

    #[test]
    fn test_unknown_keys_roundtrip() {
        let s = parse_settings(r#"{ "muj_experiment": [1, 2, 3] }"#).unwrap();
        assert_eq!(s.extra.get("muj_experiment").unwrap()[0], 1);
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("muj_experiment"));
    }

    #[test]
    fn test_invalid_type_rejected() {
        assert!(parse_settings(r#"{ "bed_max_x": "haf" }"#).is_err());
        assert!(parse_settings("[1,2]").is_err());
    }
}
