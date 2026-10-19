use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PathSegment {
    pub points: Vec<Point2D>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_filled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_stroke: Option<bool>,
    /// Zdrojový prvek (SVG element). Výplně různých prvků se sjednocují, uvnitř
    /// prvku platí even-odd. None = všechny takové segmenty tvoří jednu
    /// even-odd skupinu (DXF, starší data).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u32>,
    /// Objekt, ke kterému zpracovaná dráha patří (klíč per-objektového nastavení).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<u32>,
    /// Tvar je díra — odečítá se z výplně ostatních prvků (SVG `data-dpi-hole`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hole: Option<bool>,
    /// Konce čáry přichycené magnetem k trase jiného objektu (jen zdrojové dráhy).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<Anchor>,
    /// Dráha výplně typu had — v plátně se ukazuje směr tisku.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub snake: bool,
    /// Uzavřená dráha už začíná ve zvoleném bodě (napojení na spojovací čáru) —
    /// generátor ji nesmí otáčet k trysce.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fixed_start: bool,
    /// Přejezd na začátek dráhy vede jen přes plochu, kterou výplň objektu
    /// pokryje — kapalina se po ní smí táhnout, generátor trysku nezvedá.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub wet_travel: bool,
}

/// Konec čáry přichycený k trase objektu: čára se ho drží i po přesunu,
/// otočení či změně měřítka objektu.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Anchor {
    /// 0 = první bod čáry, 1 = poslední bod.
    pub end: u8,
    /// Id objektu, ke kterému je konec přichycený.
    pub object: u32,
    /// Bod přichycení ve zdrojových souřadnicích neupraveného objektu.
    pub x: f64,
    pub y: f64,
}

/// Úprava jednoho objektu (posun, měřítko, rotace) v souřadnicích kresby
/// na sklíčku (před zapečeným měřítkem trasy), kolem středu jeho obálky.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ObjectTransform {
    #[serde(default)]
    pub dx: f64,
    #[serde(default)]
    pub dy: f64,
    #[serde(default = "one")]
    pub scale: f64,
    /// Měřítko v ose Y (odemčený poměr stran); None = `scale`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale_y: Option<f64>,
    /// Ve stupních, stejná orientace jako rotace trasy (`Transform::rotation`).
    #[serde(default)]
    pub rotation: f64,
}

fn one() -> f64 {
    1.0
}

impl ObjectTransform {
    /// Bod `p` po úpravě objektu se středem obálky `pivot`.
    pub fn apply(&self, p: Point2D, pivot: Point2D) -> Point2D {
        let (vx, vy) = ((p.x - pivot.x) * self.scale, (p.y - pivot.y) * self.scale_y.unwrap_or(self.scale));
        let rad = (-self.rotation).to_radians();
        let (s, c) = rad.sin_cos();
        Point2D::new(pivot.x + self.dx + vx * c - vy * s, pivot.y + self.dy + vx * s + vy * c)
    }

    pub fn is_identity(&self) -> bool {
        self.dx.abs() < 1e-9
            && self.dy.abs() < 1e-9
            && (self.scale - 1.0).abs() < 1e-9
            && self.scale_y.is_none_or(|v| (v - 1.0).abs() < 1e-9)
            && self.rotation.abs() < 1e-9
    }
}

impl PathSegment {
    pub fn new(points: Vec<Point2D>) -> Self {
        Self {
            points,
            ..Default::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

/// Objekt na sklíčku — souvislý tvar (ostrůvek výplně) nebo samostatná čára.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PathObject {
    /// Stabilní id: index zdrojového segmentu, ze kterého objekt vznikl.
    pub id: u32,
    /// Ohraničení v souřadnicích sklíčka: [min_x, max_x, min_y, max_y].
    pub bounds: [f64; 4],
    /// true = vyplněný tvar, false = čára / obrys bez výplně.
    pub filled: bool,
    /// Objekt se překrývá s jiným a tiskne se s ním jako jedna dráha s nastavením
    /// objektu s tímto id (v seznamu objektů ale zůstává samostatný).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_into: Option<u32>,
    /// Objekt je díra (odečítá se z ostatních, sám se netiskne).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hole: bool,
    /// Index zdrojového SVG prvku (pro úpravy prvku z UI).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element: Option<u32>,
    /// Obálka neupraveného objektu v souřadnicích kresby (před zapečeným
    /// měřítkem trasy) — rám úchytů pro úpravu objektu v plátně.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_bounds: Option<[f64; 4]>,
    /// Platná úprava objektu (vlastní sklíčka, jinak sdílená z kresby).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xform: Option<ObjectTransform>,
    /// Indexy zdrojových segmentů objektu (obrys, díry) — pro kopírování objektu.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubstratePaths {
    pub segments: Vec<PathSegment>,
    /// Objekty sklíčka (jen u zpracovaných drah).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<PathObject>,
    /// Objekty smazané ze sdílené kresby — ze všech skel (jen u zdrojových drah;
    /// SVG kořenový atribut `data-dpi-deleted-objects`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deleted_objects: Vec<u32>,
    /// Úpravy objektů sdílené všemi skly (jen u zdrojových drah; SVG kořenový
    /// atribut `data-dpi-object-transforms`). Klíč = id objektu.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub object_transforms: HashMap<String, ObjectTransform>,
}

impl SubstratePaths {
    pub fn new(segments: Vec<PathSegment>) -> Self {
        Self {
            segments,
            objects: Vec::new(),
            deleted_objects: Vec::new(),
            object_transforms: HashMap::new(),
        }
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
    /// Korekce nuly Z z kalibrace (mm) — přičítá se ke všem sklům, i k těm
    /// s vlastním z_offsetem (na rozdíl od z_offsetu ji lokální přepis nenahradí).
    #[serde(default)]
    pub z_calibration: f64,
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
    #[serde(default)]
    pub nozzle_change_gcode: String,
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
    /// Objem zpětného nasátí kapaliny před přejezdem se zdvihem (nl); 0 = bez retrakce.
    #[serde(default)]
    pub retract_nl: f64,
    /// Rychlost retrakce a návratu kapaliny (posuv E, mm/min).
    #[serde(default = "default_retract_speed")]
    pub retract_speed: f64,
    /// Čekání po dávce kapky, než se tryska zvedne (ms).
    #[serde(default)]
    pub dot_dwell_ms: f64,
}

pub(crate) fn default_retract_speed() -> f64 {
    300.0
}

impl MachineConfig {
    pub fn motion(&self) -> MotionConfig {
        MotionConfig {
            z_hop: self.z_hop,
            retract_nl: self.retract_nl,
            retract_speed: self.retract_speed,
            dot_dwell_ms: self.dot_dwell_ms,
            calibration_factor: self.calibration_factor,
        }
    }
}

/// Pohyby mimo tiskové dráhy (zdvih, retrakce, čekání po kapce) — sdílí
/// generátor G-kódu a časová osa náhledu.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MotionConfig {
    pub z_hop: f64,
    #[serde(default)]
    pub retract_nl: f64,
    #[serde(default = "default_retract_speed")]
    pub retract_speed: f64,
    #[serde(default)]
    pub dot_dwell_ms: f64,
    pub calibration_factor: f64,
}

impl Default for MotionConfig {
    fn default() -> Self {
        MotionConfig {
            z_hop: 2.0,
            retract_nl: 0.0,
            retract_speed: default_retract_speed(),
            dot_dwell_ms: 0.0,
            calibration_factor: 0.323877,
        }
    }
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
    /// Zapečené měřítko v ose Y (nerovnoměrné škálování); None = `user_scale`.
    #[serde(default)]
    pub user_scale_y: Option<f64>,
    /// Přepisy jednotlivých objektů sklíčka (klíč = id objektu).
    #[serde(default)]
    pub object_overrides: HashMap<String, ObjectOverride>,
    /// Objekty smazané z tohoto sklíčka (id objektů) — netisknou se a v seznamu nejsou.
    #[serde(default)]
    pub deleted_objects: Vec<u32>,
    /// Úpravy objektů jen pro toto sklíčko (mají přednost před sdílenými z kresby).
    #[serde(default)]
    pub object_transforms: HashMap<String, ObjectTransform>,
    /// Pořadí tisku objektů (id); neuvedené objekty následují podle id.
    #[serde(default)]
    pub object_order: Vec<u32>,
}

/// Výsledek `recalculate_layout` — nové pozice sklíček a přizpůsobené transformace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutWithTransforms {
    pub positions: Vec<LayoutPosition>,
    pub transforms: Vec<Transform>,
}

/// Předpočítaná data jednoho segmentu pro náhled průběhu tisku.
/// Body jsou v pořadí tisku: bod k = původní bod (start_idx + k) mod n
/// (uzavřené dráhy začínají u vrcholu nejbližšího trysce, stejně jako G-kód).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewSegData {
    pub slide_idx: usize,
    pub seg_idx: usize,
    pub path_start_dist: f64,
    pub point_dists: Vec<f64>,
    pub seg_dist: f64,
    /// Index původního bodu, kterým tisk dráhy začíná.
    pub start_idx: usize,
    /// Čas začátku tisku dráhy od začátku tisku (s), po přejezdu.
    pub start_time: f64,
    /// Kumulativní čas tisku k jednotlivým bodům od `start_time` (s).
    pub point_times: Vec<f64>,
    /// Bod, odkud se na dráhu přejíždí (world), pro náhled trysky při přejezdu.
    pub travel_from: Point2D,
}

/// Výsledek `compute_preview_segments`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewDistResult {
    pub segs: Vec<PreviewSegData>,
    pub total_dist: f64,
    /// Odhadovaný čas tisku drah bez nahřívání (s) — časová osa náhledu.
    pub total_time: f64,
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
    /// Tloušťka skla pod touto pozicí (mm); None = tloušťka vzorkového skla
    /// (`ProcessParams::slide_z`). Odplivové sklo může být jinak tlusté.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glass_z: Option<f64>,
    /// Objekty smazané z tohoto sklíčka (id objektů). Sdílená kresba je nemění,
    /// ostatní skla je tisknou dál.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deleted_objects: Vec<u32>,
    /// Úpravy objektů jen na tomto sklíčku (klíč = id objektu).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub object_transforms: HashMap<String, ObjectTransform>,
    /// Pořadí tisku objektů (id); neuvedené objekty následují podle id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub object_order: Vec<u32>,
    /// Skupiny propojených klonů (id objektů): změna měřítka, rotace a nastavení
    /// jednoho se přenáší na ostatní (spravuje frontend, generátor je nepoužívá).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clones: Vec<Vec<u32>>,
    /// Třetí, nejvyšší úroveň nastavení: přepisy jednotlivých objektů sklíčka
    /// (klíč = id objektu). Co objekt nepřepíše, dědí od sklíčka.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub objects: HashMap<String, ObjectOverride>,
}

/// Přepis nastavení jednoho objektu — stejné parametry jako lokální nastavení sklíčka.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ObjectOverride {
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
    /// Začátek hada: "bl" / "br" / "tl" / "tr" = roh obálky objektu
    /// (levý dolní …), None = automaticky podle nejkratší trasy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snake_start: Option<String>,
}

fn default_one() -> f64 {
    1.0
}

/// Stav umístění objektu na jednom sklíčku (bez geometrie tras).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementState {
    pub object_id: String,
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

impl PlacementState {
    /// Efektivní průměr trysky: override objektu, jinak globální hodnota.
    pub fn effective_nozzle_diam(&self, global: f64) -> f64 {
        self.nozzle_diam.filter(|d| *d > 0.0).unwrap_or(global)
    }
}

/// Umístění objektu na jednom skle včetně spočtené trasy — výstup `update_layout`
/// a vstup do generátoru G-kódu / plánovače.
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

    /// Efektivní průměr trysky: override objektu, jinak globální hodnota.
    pub fn effective_nozzle_diam(&self, global: f64) -> f64 {
        self.nozzle_diam.filter(|d| *d > 0.0).unwrap_or(global)
    }
}

/// Objekt scény — odkaz do zdroje plus pořadí v něm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneObject {
    pub id: String,
    pub source_id: String,
    pub index: usize,
    pub name: String,
    pub raw: SubstratePaths,
}
