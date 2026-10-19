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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BedConfig {
    pub max_x: f64,
    pub max_y: f64,
    pub min_x: f64,
    pub offset_x: f64,
    pub offset_y: f64,
}

/// Nastavení stroje pro generování G-kódu — vše mimo procesní parametry vzorku.
/// Serializovatelná, aby šla předat z frontendu jako jediný objekt
/// (místo dlouhého seznamu pozičních parametrů Tauri commandu).
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Maximální povolená teplota podložky (°C) — bezpečnostní strop pro M140/M190.
    #[serde(default)]
    pub bed_max_temp: Option<f64>,
    /// Pokud true, negeneruje se blok virtuálního posunu Z (výjezd na safe_z + G92) —
    /// volající už virtuální souřadný systém nastavil sám (kalibrační tok ve frontendu).
    /// Samotný z_shift se na tiskové výšky aplikuje vždy.
    #[serde(default)]
    pub skip_z_shift_setup: bool,
    /// G-kód vkládaný po výměně trysky, než se pokračuje v tisku. Výchozí
    /// (prázdná hodnota) = `G28` — homing je po výměně povinný, protože nová
    /// jehla má jinou délku i vyosení. Za tento blok generátor vloží značku
    /// `NOZZLE_ALIGN_MARKER` pro kontrolu vyosení.
    #[serde(default)]
    pub nozzle_change_gcode: String,
    /// Pořadí tisku napříč substráty a tryskami — "substrate_major" | "nozzle_major".
    /// Prázdné/neznámé se vyhodnotí jako "nozzle_major" (viz `schedule::PrintOrder::from_setting`).
    #[serde(default)]
    pub print_order: String,
    /// Strategie přejezdů — "fastest" | "cross_allowed" (alias, stejný algoritmus)
    /// nebo "no_cross". Prázdné/neznámé se vyhodnotí jako "no_cross"
    /// (viz `schedule::RouteMode::from_setting`).
    #[serde(default)]
    pub route_mode: String,
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
    /// Rám pro normalizaci a centrování: `[min_x, max_x, min_y, max_y]` bboxu
    /// celého importu. Díky němu drží objekty z jednoho souboru vzájemné
    /// rozložení — každý se normalizuje vůči společnému rámu, ne vůči sobě.
    /// `None` = rám se odvodí z vlastní geometrie (chování s jediným objektem).
    #[serde(default)]
    pub frame: Option<[f64; 4]>,
}

/// Výsledek `recalculate_layout` — nové pozice sklíček a přizpůsobené transformace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutWithTransforms {
    pub positions: Vec<LayoutPosition>,
    pub transforms: Vec<Transform>,
}

/// Předpočítaná data jednoho segmentu pro náhled průběhu tisku.
/// Výstup `compute_preview_segments` — odpovídá `PreviewSegData` v Canvas2D.svelte.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewSegData {
    pub slide_idx: usize,
    /// Index objektu (placementu) na sklíčku — u odplivu vždy 0.
    pub obj_idx: usize,
    pub seg_idx: usize,
    pub path_start_dist: f64,
    pub point_dists: Vec<f64>,
    pub seg_dist: f64,
}

/// Výsledek `compute_preview_segments`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewDistResult {
    pub segs: Vec<PreviewSegData>,
    pub total_dist: f64,
}

/// Jeden objekt scény — samostatně umístitelný, skrývatelný a mazatelný tvar.
/// Jeden import se na objekty rozpadá podle oddělených tvarů
/// (`path_processing::split_into_objects`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneObject {
    pub id: String,
    pub name: String,
    /// Surová geometrie JEN tohoto objektu (souřadnice zdrojového importu).
    pub raw: SubstratePaths,
    /// Bbox celého importu, ze kterého objekt pochází — viz `SliceParams::frame`.
    pub frame: [f64; 4],
    #[serde(default)]
    pub auto_scale: bool,
}

/// Stav umístění objektu na jednom skle — vstup do `update_layout`.
/// Neobsahuje geometrii; tu z něj layout teprve spočítá.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementState {
    pub object_id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub transform: Transform,
    #[serde(default = "default_one")]
    pub baked_scale: f64,
    /// Název trysky přenastavené jen pro tento objekt (klíč do `nozzle_defs`).
    /// `None` = tryska z globálních parametrů.
    #[serde(default)]
    pub nozzle_type: Option<String>,
    /// Průměr přenastavené trysky v mm — řídí rozestup výplně i inset okrajů.
    #[serde(default)]
    pub nozzle_diam: Option<f64>,
}

fn default_true() -> bool {
    true
}

fn default_one() -> f64 {
    1.0
}

/// Umístění objektu na jednom skle včetně spočtené trasy — výstup `update_layout`
/// a vstup do generátoru G-kódu.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Placement {
    pub object_id: String,
    pub enabled: bool,
    pub transform: Transform,
    pub baked_scale: f64,
    #[serde(default)]
    pub nozzle_type: Option<String>,
    #[serde(default)]
    pub nozzle_diam: Option<f64>,
    /// Zpracovaná trasa v lokálních souřadnicích sklíčka (po výplni a měřítku).
    pub path: SubstratePaths,
}

impl Placement {
    /// Stav bez geometrie — pro zpětné předání do `update_layout`.
    pub fn state(&self) -> PlacementState {
        PlacementState {
            object_id: self.object_id.clone(),
            enabled: self.enabled,
            transform: self.transform,
            baked_scale: self.baked_scale,
            nozzle_type: self.nozzle_type.clone(),
            nozzle_diam: self.nozzle_diam,
        }
    }
}

impl PlacementState {
    /// Efektivní průměr trysky: override objektu, jinak globální hodnota.
    pub fn effective_nozzle_diam(&self, global: f64) -> f64 {
        self.nozzle_diam.filter(|d| *d > 0.0).unwrap_or(global)
    }
}

impl Placement {
    /// Efektivní průměr trysky: override objektu, jinak globální hodnota.
    pub fn effective_nozzle_diam(&self, global: f64) -> f64 {
        self.nozzle_diam.filter(|d| *d > 0.0).unwrap_or(global)
    }
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
