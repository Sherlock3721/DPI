//! Binární protokol mezi frontendem (TS, `src/lib/viewport/sceneSync.ts`) a rendererem.
//!
//! Oba hostitelé (nativní offscreen v src-tauri i wasm nad WebGL2) dostávají
//! stejné bajty, takže frontend kóduje jen jednou.
//!
//! Souřadnice: svět v mm, X doprava, Y nahoru (deska), Z nahoru. Deska leží
//! v z = 0 na [0, bed.w] × [0, bed.h]; sklo stojí na desce, vršek v z = `thickness`.
//!
//! ## Scéna (`apply_scene`) — mění se zřídka
//! ```text
//! 0   u32  MAGIC ("DPIR")
//! 4   u32  VERSION
//! 8   u32  délka JSON (bajty)
//! 12  JSON `ScenePacket` (UTF-8), doplněný nulami na násobek 4
//! ..  sekce blobů — `BlobRef.offset` se počítá od jejího začátku, bloby
//!     jsou zarovnané na 4 bajty, little-endian
//! ```
//!
//! ## Snímek (`render`) — každý snímek
//! `FrameHeader` (pevná velikost) a za ním `n_slides` × `SlideFrame`,
//! ve stejném pořadí jako `ScenePacket::slides`.

use bytemuck::{Pod, Zeroable};
use serde::Deserialize;

pub const MAGIC: u32 = 0x5249_5044; // "DPIR" little-endian
pub const VERSION: u32 = 1;

/// Odkaz do sekce blobů (bajty).
#[derive(Debug, Clone, Copy, Deserialize, Default)]
pub struct BlobRef {
    pub offset: u32,
    pub len: u32,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ScenePacket {
    /// Rozměr desky; `None` = beze změny.
    pub bed: Option<Bed>,
    /// Nové buffery geometrie drah (id přiděluje frontend, jedno id = jeden objekt drah).
    #[serde(default)]
    pub buffers_add: Vec<PathBufferDesc>,
    /// Buffery k uvolnění (GPU paměť).
    #[serde(default)]
    pub buffers_free: Vec<u32>,
    /// Kompletní seznam skel; `None` = beze změny. Pořadí = pořadí `SlideFrame` ve snímku.
    pub slides: Option<Vec<Slide>>,
    /// Přejezdy trysky jako LineList `[f32; 3]` (dvojice bodů, svět, z už se Z-hopem);
    /// `None` = beze změny, `len == 0` = žádné.
    pub travels: Option<BlobRef>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Bed {
    pub w: f32,
    pub h: f32,
    /// Tloušťka podložky (mm): deska je kvádr [0, w] × [0, h] × [−thickness, 0],
    /// vršek zůstává v z = 0. Chybí = 0 (plochá deska).
    #[serde(default)]
    pub thickness: f32,
}

/// Geometrie drah jednoho objektu `SubstratePaths` v jeho lokálních souřadnicích `p`.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PathBufferDesc {
    pub id: u32,
    /// Pole `Piece` (úseky drah, v pořadí drah a bodů).
    pub pieces: BlobRef,
    /// Pole `Dot` (tečky = dráha z 1 bodu nebo ze 2 téměř shodných).
    pub dots: BlobRef,
    /// Počet drah (segmentů) — normalizace barvy „pořadí tisku“.
    pub n_segs: u32,
}

/// Úsek dráhy od bodu `j` k bodu `j + 1` (uložené pořadí bodů).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Piece {
    pub a: [f32; 2],
    pub b: [f32; 2],
    /// Index dráhy v `SubstratePaths::segments`.
    pub seg: u32,
    /// Index úseku v dráze (0..n_pieces).
    pub j: u32,
    /// Počet úseků dráhy (= počet bodů − 1).
    pub n_pieces: u32,
    /// Index do `Slide::objects` (tabulka stylu objektů skla).
    pub obj: u32,
    /// Délka dráhy od jejího prvního bodu k `a` (lokální jednotky) — čárkování duchů.
    pub cum_len: f32,
}

/// Tečka (kapka) v lokálních souřadnicích.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Dot {
    pub p: [f32; 2],
    pub seg: u32,
    pub obj: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Slide {
    /// Sklo: x, y, šířka, výška (svět), tloušťka (vršek v z = thickness).
    pub glass: [f32; 5],
    /// Výplň skla RGBA (lineární 0–1, alfa = průhlednost).
    pub glass_fill: [f32; 4],
    /// Hrany skla RGBA.
    pub glass_edge: [f32; 4],
    /// Buffer tištěných drah (oříznuté na sklo) — kurzor náhledu indexuje tyto dráhy.
    pub paths: Option<u32>,
    /// Buffer neoříznutých drah přesahujících sklo — kreslí se slabě mimo sklo.
    pub ghost: Option<u32>,
    /// Obdélník ořezu (svět) x0, y0, x1, y1: housenky se kreslí jen uvnitř, duchové jen vně.
    pub clip: [f32; 4],
    /// Styl objektů; index = `Piece::obj` / `Dot::obj` obou bufferů skla.
    pub objects: Vec<ObjStyle>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ObjStyle {
    /// Šířka housenky (mm).
    pub w: f32,
    /// Výška housenky nad sklem (mm, bez převýšení).
    pub h: f32,
    /// Barva RGB 0–1 (sRGB hodnoty, renderer je nepřevádí).
    pub color: [f32; 3],
}

/// Hlavička snímku. Všechny barvy jsou sRGB hodnoty 0–1 a renderer je zapisuje
/// beze změny (cíl je `Rgba8Unorm`), aby nativní i wasm výstup vypadal stejně.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct FrameHeader {
    /// Svět → clip prostor (sloupcově, WebGPU konvence: z ∈ [0, 1], y nahoru).
    pub view_proj: [f32; 16],
    /// xyz = poloha oka (perspektiva), u ortho libovolný bod na ose pohledu; w = 1 pro ortho.
    pub eye: [f32; 4],
    /// xyz = směr pohledu (normalizovaný, od oka do scény).
    pub view_dir: [f32; 4],
    /// Šířka a výška cíle (px), px na mm v targetu kamery, DPR.
    pub viewport: [f32; 4],
    /// Mřížka: minor krok (mm), major krok (mm), zobrazit (0/1), rezerva.
    pub grid: [f32; 4],
    /// Převýšení Z housenek, režim barvy (0 = barvy objektů, 1 = pořadí tisku),
    /// práh LOD v px (pod ním plochý pásek), perioda čárkování duchů (mm).
    pub params: [f32; 4],
    /// Reálná tryska: x, y, z špičky, viditelnost (0/1).
    pub nozzle_real: [f32; 4],
    /// Náhledová tryska: x, y, z špičky, viditelnost (0/1).
    pub nozzle_preview: [f32; 4],
    /// Barva pozadí RGBA.
    pub clear: [f32; 4],
    /// Zobrazit duchy (0/1), zobrazit přejezdy (0/1), počet skel ve snímku, rezerva.
    pub flags: [u32; 4],
}

/// Proměnné skla pro snímek (tažení trasy mění jen tohle, ne buffery).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct SlideFrame {
    /// Transformace drah: svět.xy = (tx + cx, ty + cy) + R(θ)·(S·(p − c)).
    /// tx, ty, cx, cy, sx, sy, cos θ, sin θ. (θ = −rotation° v radiánech, jako `tpt`.)
    pub xf: [f32; 8],
    /// Kurzor náhledu: režim (0 = vše, 1 = nic, 2 = částečně), aktuální dráha,
    /// start_idx aktuální dráhy, tištěný úsek k (−1 = tryska teprve přejíždí).
    pub cursor: [i32; 4],
    /// frac rozpracovaného úseku (0–1), rezerva ×3.
    pub cursor_frac: [f32; 4],
}

/// Výsledek čtení scénového paketu.
pub struct ParsedScene<'a> {
    pub packet: ScenePacket,
    pub blobs: &'a [u8],
}

pub fn parse_scene(bytes: &[u8]) -> Result<ParsedScene<'_>, String> {
    if bytes.len() < 12 {
        return Err("scéna: paket je kratší než hlavička".into());
    }
    let word = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
    if word(0) != MAGIC {
        return Err("scéna: neplatný magic".into());
    }
    if word(4) != VERSION {
        return Err(format!("scéna: nepodporovaná verze {}", word(4)));
    }
    let json_len = word(8) as usize;
    let json_end = 12 + json_len;
    if json_end > bytes.len() {
        return Err("scéna: JSON přesahuje paket".into());
    }
    let packet: ScenePacket =
        serde_json::from_slice(&bytes[12..json_end]).map_err(|e| format!("scéna: {e}"))?;
    let blob_start = (json_end + 3) & !3;
    let blobs = bytes.get(blob_start..).unwrap_or(&[]);
    Ok(ParsedScene { packet, blobs })
}

impl ParsedScene<'_> {
    /// Typovaný pohled na blob (kopie — bloby nemusí být zarovnané v paměti JS).
    pub fn blob<T: Pod>(&self, r: BlobRef) -> Result<Vec<T>, String> {
        let start = r.offset as usize;
        let end = start + r.len as usize;
        let raw = self.blobs.get(start..end).ok_or("scéna: blob mimo paket")?;
        let size = std::mem::size_of::<T>();
        if raw.len() % size != 0 {
            return Err("scéna: délka blobu neodpovídá typu".into());
        }
        Ok(raw
            .chunks_exact(size)
            .map(bytemuck::pod_read_unaligned)
            .collect())
    }
}

pub struct ParsedFrame {
    pub header: FrameHeader,
    pub slides: Vec<SlideFrame>,
}

pub fn parse_frame(bytes: &[u8]) -> Result<ParsedFrame, String> {
    let hs = std::mem::size_of::<FrameHeader>();
    let ss = std::mem::size_of::<SlideFrame>();
    let raw = bytes.get(..hs).ok_or("snímek: kratší než hlavička")?;
    let header: FrameHeader = bytemuck::pod_read_unaligned(raw);
    let n = header.flags[2] as usize;
    let body = bytes
        .get(hs..hs + n * ss)
        .ok_or("snímek: chybí data skel")?;
    let slides = body
        .chunks_exact(ss)
        .map(bytemuck::pod_read_unaligned)
        .collect();
    Ok(ParsedFrame { header, slides })
}

impl FrameHeader {
    /// Velikost cíle v px z `viewport` (zaokrouhleno); `None` pro nulovou/neplatnou velikost.
    pub fn target_size(&self) -> Option<(u32, u32)> {
        let w = self.viewport[0];
        let h = self.viewport[1];
        if !(w.is_finite() && h.is_finite()) || w < 0.5 || h < 0.5 {
            return None;
        }
        Some((w.round() as u32, h.round() as u32))
    }
}

/// Sestaví scénový paket (testy a nativní nástroje; frontend má vlastní kodér
/// se stejným formátem). `blobs` už musí mít bloby zarovnané na 4 bajty.
pub fn encode_scene(json: &str, blobs: &[u8]) -> Vec<u8> {
    let json = json.as_bytes();
    let padded = (json.len() + 3) & !3;
    let mut out = Vec::with_capacity(12 + padded + blobs.len());
    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(json);
    out.resize(12 + padded, 0);
    out.extend_from_slice(blobs);
    out
}

/// Sestaví paket snímku: hlavička + skla (`flags[2]` se přepíše počtem skel).
pub fn encode_frame(header: &FrameHeader, slides: &[SlideFrame]) -> Vec<u8> {
    let mut h = *header;
    h.flags[2] = slides.len() as u32;
    let mut out = bytemuck::bytes_of(&h).to_vec();
    out.extend_from_slice(bytemuck::cast_slice(slides));
    out
}

/// Skládání sekce blobů po 4bajtově zarovnaných kusech.
#[derive(Debug, Default)]
pub struct BlobWriter {
    pub data: Vec<u8>,
}

impl BlobWriter {
    pub fn push<T: Pod>(&mut self, items: &[T]) -> BlobRef {
        let offset = self.data.len() as u32;
        self.data.extend_from_slice(bytemuck::cast_slice(items));
        let len = self.data.len() as u32 - offset;
        self.data.resize((self.data.len() + 3) & !3, 0);
        BlobRef { offset, len }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene_err(bytes: &[u8]) -> String {
        parse_scene(bytes).err().expect("očekávaná chyba scény")
    }

    fn frame_err(bytes: &[u8]) -> String {
        parse_frame(bytes).err().expect("očekávaná chyba snímku")
    }

    fn header() -> FrameHeader {
        let mut h = FrameHeader::zeroed();
        h.viewport = [320.0, 200.0, 2.0, 1.0];
        h
    }

    #[test]
    fn scene_roundtrip_with_blobs() {
        let mut blobs = BlobWriter::default();
        let pieces = [Piece {
            a: [0.0, 0.0],
            b: [1.0, 0.0],
            seg: 0,
            j: 0,
            n_pieces: 1,
            obj: 0,
            cum_len: 0.0,
        }];
        let r = blobs.push(&pieces);
        let json = format!(
            r#"{{"bed":{{"w":120,"h":80}},"buffers_add":[{{"id":7,"pieces":{{"offset":{},"len":{}}},"dots":{{"offset":0,"len":0}},"n_segs":1}}]}}"#,
            r.offset, r.len
        );
        // JSON délky, která není násobkem 4 → test zarovnání sekce blobů
        let bytes = encode_scene(&format!("{json} "), &blobs.data);
        let parsed = parse_scene(&bytes).unwrap();
        assert_eq!(parsed.packet.buffers_add.len(), 1);
        let back: Vec<Piece> = parsed.blob(parsed.packet.buffers_add[0].pieces).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].b, [1.0, 0.0]);
        assert!(parsed.packet.slides.is_none());
        assert!(parsed.packet.travels.is_none());
    }

    #[test]
    fn bed_thickness_defaults_to_zero() {
        let thickness = |json: &str| {
            let bytes = encode_scene(json, &[]);
            parse_scene(&bytes).unwrap().packet.bed.unwrap().thickness
        };
        assert_eq!(thickness(r#"{"bed":{"w":120,"h":80}}"#), 0.0);
        assert_eq!(thickness(r#"{"bed":{"w":120,"h":80,"thickness":2}}"#), 2.0);
    }

    #[test]
    fn scene_errors() {
        assert!(scene_err(&[0u8; 8]).contains("kratší"));
        let mut bad = encode_scene("{}", &[]);
        bad[0] = 0;
        assert!(scene_err(&bad).contains("magic"));
        let mut ver = encode_scene("{}", &[]);
        ver[4] = 99;
        assert!(scene_err(&ver).contains("verze"));
        let mut long = encode_scene("{}", &[]);
        long[8] = 200;
        assert!(scene_err(&long).contains("přesahuje"));
        assert!(scene_err(&encode_scene("{nope", &[])).starts_with("scéna:"));
        // blob mimo paket a blob s délkou nedělitelnou typem
        let parsed_bytes = encode_scene("{}", &[0u8; 8]);
        let parsed = parse_scene(&parsed_bytes).unwrap();
        assert!(parsed
            .blob::<Piece>(BlobRef { offset: 4, len: 36 })
            .is_err());
        assert!(parsed.blob::<Dot>(BlobRef { offset: 0, len: 8 }).is_err());
        assert_eq!(
            parsed
                .blob::<u32>(BlobRef { offset: 0, len: 8 })
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn frame_roundtrip_and_errors() {
        let sf = SlideFrame {
            xf: [1.0; 8],
            cursor: [2, 3, 0, -1],
            cursor_frac: [0.5, 0.0, 0.0, 0.0],
        };
        let bytes = encode_frame(&header(), &[sf, sf]);
        let f = parse_frame(&bytes).unwrap();
        assert_eq!(f.slides.len(), 2);
        assert_eq!(f.slides[1].cursor, [2, 3, 0, -1]);
        assert_eq!(f.header.target_size(), Some((320, 200)));
        assert!(frame_err(&bytes[..10]).contains("hlavička"));
        assert!(frame_err(&bytes[..bytes.len() - 4]).contains("skel"));
        let mut z = header();
        z.viewport[0] = 0.0;
        assert_eq!(z.target_size(), None);
        z.viewport[0] = f32::NAN;
        assert_eq!(z.target_size(), None);
    }

    #[test]
    fn layout_sizes_match_frontend() {
        // Frontend (sceneSync.ts) zapisuje stejné velikosti — změna = změna protokolu.
        assert_eq!(std::mem::size_of::<Piece>(), 36);
        assert_eq!(std::mem::size_of::<Dot>(), 16);
        assert_eq!(std::mem::size_of::<FrameHeader>(), 64 + 9 * 16);
        assert_eq!(std::mem::size_of::<SlideFrame>(), 64);
    }
}
