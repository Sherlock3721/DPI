use crate::types::{ProcessParams, SlideOverride, Transform};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GCodeMetadata {
    pub params: ProcessParams,
    #[serde(default)]
    pub overrides: HashMap<String, SlideOverride>,
    #[serde(default)]
    pub transforms: Vec<Transform>,
    #[serde(default)]
    pub baked_scales: Vec<f64>,
    #[serde(default)]
    pub source_file_name: String,
    #[serde(default)]
    pub source_file_ext: String,
    #[serde(default)]
    pub source_file_content: String,
    #[serde(default)]
    pub auto_scale: bool,
    #[serde(default = "default_fineness")]
    pub fineness: f64,
}

fn default_fineness() -> f64 {
    1.0
}

/// Serializuje metadata do G-kód hlavičky vkládané na začátek souboru.
pub fn serialize_metadata(meta: &GCodeMetadata) -> String {
    let wrapped = serde_json::json!({
        "dpi_version": 2,
        "params": meta.params,
        "overrides": meta.overrides,
        "transforms": meta.transforms,
        "baked_scales": meta.baked_scales,
        "source_file_name": meta.source_file_name,
        "source_file_ext": meta.source_file_ext,
        "source_file_content": meta.source_file_content,
        "auto_scale": meta.auto_scale,
        "fineness": meta.fineness,
    });
    let json = serde_json::to_string(&wrapped).unwrap_or_default();
    format!("; --- EDITOR METADATA ---\n; {json}\n; --- END METADATA ---\n\n")
}

/// Extrahuje a deserializuje metadata ze záhlaví G-kód souboru.
/// Vrátí None pokud soubor neobsahuje DPI metadata nebo jsou poškozená.
pub fn deserialize_metadata(gcode: &str) -> Option<GCodeMetadata> {
    let meta_start = gcode.find("; --- EDITOR METADATA ---")?;
    let meta_end = gcode.find("; --- END METADATA ---")?;

    // get() místo přímého slice — chrání proti panicu, pokud poškozený
    // soubor obsahuje END marker před START markerem.
    let block = gcode.get(meta_start..meta_end)?;
    let json_text: String = block
        .lines()
        .filter(|l| l.starts_with(';'))
        .map(|l| {
            let s = l.trim_start_matches(';');
            s.strip_prefix(' ').unwrap_or(s)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let js = json_text.find('{')?;
    let je = json_text.rfind('}')?;
    let json_str = &json_text[js..=je];

    let raw: serde_json::Value = serde_json::from_str(json_str).ok()?;

    let params: ProcessParams = serde_json::from_value(raw.get("params")?.clone()).ok()?;

    let overrides: HashMap<String, SlideOverride> = raw
        .get("overrides")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let transforms: Vec<Transform> = raw
        .get("transforms")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let baked_scales: Vec<f64> = raw
        .get("baked_scales")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let source_file_name = raw
        .get("source_file_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let source_file_ext = raw
        .get("source_file_ext")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let source_file_content = raw
        .get("source_file_content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let auto_scale = raw
        .get("auto_scale")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let fineness = raw.get("fineness").and_then(|v| v.as_f64()).unwrap_or(1.0);

    Some(GCodeMetadata {
        params,
        overrides,
        transforms,
        baked_scales,
        source_file_name,
        source_file_ext,
        source_file_content,
        auto_scale,
        fineness,
    })
}

/// Escapuje hodnotu pro CSV pole v uvozovkách: zdvojí vnitřní uvozovky
/// a neutralizuje úvodní znaky, které by tabulkový procesor interpretoval
/// jako vzorec (=, +, -, @) — ochrana proti CSV/formula injection.
fn csv_quote(value: &str) -> String {
    let escaped = value.replace('"', "\"\"");
    if escaped.starts_with(['=', '+', '-', '@']) {
        format!("\"'{escaped}\"")
    } else {
        format!("\"{escaped}\"")
    }
}

/// Sestaví CSV protokol tisku z parametrů projektu.
/// `date_str` se předává z frontendu aby respektoval lokalizaci.
pub fn build_csv_protocol(
    params: &ProcessParams,
    overrides: &HashMap<String, SlideOverride>,
    total_dist: f64,
    total_time: f64,
    selected_glass: &str,
    app_version: &str,
    date_str: &str,
) -> String {
    let glass = if selected_glass.is_empty() {
        "Vlastní"
    } else {
        selected_glass
    };
    let total_min = (total_time / 60.0).ceil() as i64;

    let mut rows: Vec<String> = vec![
        format!("\"Název projektu\";\"Experimentální Tisk\""),
        format!("\"Datum generování\";\"{date_str}\""),
        format!("\"Verze aplikace\";\"{app_version}\""),
        format!(
            "\"Celkový počet vzorků\";\"{count}\"",
            count = params.sample_count
        ),
        format!("\"Základní typ sklíčka\";{}", csv_quote(glass)),
        format!(
            "\"Rozměry sklíčka (X x Y x Z)\";\"{w} x {h} x {z} mm\"",
            w = params.slide_w,
            h = params.slide_h,
            z = params.slide_z
        ),
        format!("\"Teplota podložky\";\"{temp} °C\"", temp = params.bed_temp),
        format!(
            "\"Typ injekční stříkačky/jehly\";\"{t}\"",
            t = params.nozzle_type
        ),
        format!(
            "\"Vnitřní průměr jehly\";\"{d} mm\"",
            d = params.nozzle_diam
        ),
        format!(
            "\"Výška jehly k podložce\";\"{h} mm\"",
            h = params.nozzle_height
        ),
        format!(
            "\"Režim tisku (Z-Offset)\";\"{off} {unit}\"",
            off = params.z_offset,
            unit = params.z_unit
        ),
        format!(
            "\"Teoretická rychlost tisku\";\"{spd} mm/min\"",
            spd = params.print_speed
        ),
        format!("\"Styl nanášení výplně\";\"{s}\"", s = params.infill_style),
        format!(
            "\"Základní dávkování materiálu\";\"{r} {u}\"",
            r = params.extrusion_rate,
            u = params.extrusion_unit
        ),
        format!(
            "\"Teoreticky ujetá vzdálenost\";\"{dist:.2} mm\"",
            dist = total_dist
        ),
        format!("\"Odhadovaný čas procesu\";\"{total_min} min\""),
        String::new(),
    ];

    rows.push(
        [
            "Index vzorku",
            "Název/Šarže",
            "Poznámka k chemikálii/vzorku",
            "Korekce výšky Z-offset",
            "Dávkování (Extruze)",
            "Teoretický objem látky na čáru",
            "Modifikovaná rychlost tisku [mm/min]",
            "Hustota nanášení",
        ]
        .join(";"),
    );

    for i in 0..params.sample_count {
        let o = overrides.get(&i.to_string());
        let name_raw = o.and_then(|x| x.name.as_deref()).unwrap_or("");
        let name = if name_raw.is_empty() {
            format!("Sklíčko {}", i + 1)
        } else {
            name_raw.to_string()
        };
        let note = o.and_then(|x| x.note.as_deref()).unwrap_or("").to_string();
        let z_off = o.and_then(|x| x.z_offset).unwrap_or(params.z_offset);
        let ext = o
            .and_then(|x| x.extrusion_rate)
            .unwrap_or(params.extrusion_rate);
        let spd = o.and_then(|x| x.print_speed).unwrap_or(params.print_speed);
        let inf = o.and_then(|x| x.infill_val).unwrap_or(params.infill_val);
        let inf_t = o
            .and_then(|x| x.infill_type.as_deref())
            .unwrap_or(&params.infill_type);

        rows.push(
            [
                (i + 1).to_string(),
                csv_quote(&name),
                csv_quote(&note),
                format!("{} {}", z_off, params.z_unit),
                format!("{} {}", ext, params.extrusion_unit),
                format!("\"{} {}\"", ext, params.extrusion_unit),
                format!("{}", spd),
                format!("{} {}", inf, inf_t),
            ]
            .join(";"),
        );
    }

    rows.join("\n")
}
