use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Point2D {
    pub x: f64,
    pub y: f64,
}

impl Point2D {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance(&self, other: &Self) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PathSegment {
    pub points: Vec<Point2D>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_filled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_stroke: Option<bool>,
}

impl PathSegment {
    pub fn new(points: Vec<Point2D>) -> Self {
        Self {
            points,
            is_filled: None,
            has_stroke: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubstratePaths {
    pub segments: Vec<PathSegment>,
}

impl SubstratePaths {
    pub fn new(segments: Vec<PathSegment>) -> Self {
        Self { segments }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SlideDimensions {
    pub width: f64,
    pub height: f64,
    pub thickness: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LayoutPosition {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub is_prime: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Transform {
    pub scale: f64,
    pub rotation: f64, // ve stupních
    pub gui_dx: f64,
    pub gui_dy: f64,
    pub cx: f64,
    pub cy: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessParams {
    pub sample_count: usize,
    pub prime_active: bool,
    pub slide_w: f64,
    pub slide_h: f64,
    pub slide_z: f64,
    pub z_offset: f64,
    #[serde(default)]
    pub z_unit: String,
    pub nozzle_height: f64,
    pub nozzle_hidden: f64,
    pub filament_diameter: f64,
    pub flow_multiplier: f64,
    pub bed_temp: f64,
    pub extrusion_rate: f64,
    pub extrusion_unit: String,
    pub nozzle_diam: f64,
    pub infill_style: String,
    #[serde(default)]
    pub infill_val: f64,
    #[serde(default)]
    pub infill_type: String,
    #[serde(default)]
    pub infill_angle: f64,
    pub print_speed: f64,
    #[serde(default)]
    pub nozzle_type: String,
}

/// Konfigurace tiskárny — fyzické rozměry podložky a polohovací offsety.
/// Předávána do `get_layout_positions` a `generate_gcode`.
#[derive(Debug, Clone)]
pub struct BedConfig {
    pub max_x: f64,
    pub max_y: f64,
    pub min_x: f64,
    pub offset_x: f64,
    pub offset_y: f64,
}

/// Nastavení stroje pro generování G-kódu — vše mimo procesní parametry vzorku.
#[derive(Debug, Clone)]
pub struct MachineConfig {
    pub bed: BedConfig,
    pub start_gcode: String,
    pub end_gcode: String,
    pub loop_start_gcode: String,
    pub loop_end_gcode: String,
    pub multi_spacing: f64,
    pub block_height: f64,
    pub calibration_factor: f64,
    /// Výška Z-hopu nad povrchem sklíčka při přejezdech (mm).
    pub z_hop: f64,
    /// Bezpečná Z-výška pro virtuální posun nuly (mm).
    pub safe_z: f64,
}

/// Parametry pro slicování a zpracování vektorových drah do tiskového formátu.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliceParams {
    pub slide_w: f64,
    pub slide_h: f64,
    pub margin: f64,
    pub auto_scale: bool,
    pub infill_style: String,
    pub infill_val: f64,
    pub infill_type: String,
    pub infill_angle: f64,
    pub nozzle_diam: f64,
    pub user_scale: f64,
}

/// Výsledek `recalculate_layout` — nové pozice sklíček a přizpůsobené transformace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutWithTransforms {
    pub positions: Vec<LayoutPosition>,
    pub transforms: Vec<Transform>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SlideOverride {
    pub name: Option<String>,
    pub note: Option<String>,
    pub z_offset: Option<f64>,
    pub extrusion_rate: Option<f64>,
    pub extrusion_unit: Option<String>,
    pub print_speed: Option<f64>,
    pub infill_val: Option<f64>,
    pub infill_type: Option<String>,
    pub nozzle_height: Option<f64>,
    pub infill_style: Option<String>,
    pub slide_w: Option<f64>,
    pub slide_h: Option<f64>,
    pub glass_type: Option<String>,
}
