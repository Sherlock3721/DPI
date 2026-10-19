//! Odhad skutečné polohy trysky během tisku.
//!
//! Marlin potvrzuje příkaz („ok“), jakmile ho zařadí do plánovače — tryska je
//! v tu chvíli až o celý buffer pohybů pozadu. Tracker proto modeluje provádění
//! pohybů v čase (délka / rychlost s limity os z M203) a koriguje ho podle
//! zpoždění potvrzení: když je plánovač plný, potvrzení dalšího pohybu přijde
//! přesně ve chvíli, kdy se uvolní místo — tedy když se dokončí nejstarší
//! pohyb ve frontě.

use serde::Serialize;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Potvrzení pohybu, které čekalo déle, znamená plný plánovač.
const SATURATION_THRESHOLD: Duration = Duration::from_millis(30);

/// Poloha tisku v náhledu: sklo (index pozice), dráha, počáteční bod dráhy
/// a úsek v pořadí tisku (od bodu `pt_idx` k `pt_idx + 1`, podíl `frac`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PrintCursor {
    pub pos_idx: usize,
    pub seg_idx: usize,
    pub start_idx: usize,
    pub pt_idx: usize,
    pub frac: f64,
}

/// Značka dráhy z G-kódu (`; DPI_SEG sklo dráha start`).
pub type SegMarker = (usize, usize, usize);

/// Rozpozná značku dráhy vkládanou generátorem G-kódu.
pub fn parse_seg_marker(raw_line: &str) -> Option<SegMarker> {
    let rest = raw_line.trim_start().strip_prefix("; DPI_SEG")?;
    let mut it = rest.split_whitespace().map(|v| v.parse::<usize>().ok());
    Some((it.next()??, it.next()??, it.next()??))
}

#[derive(Debug, Clone)]
struct LineInfo {
    /// Délka extruze v XY (mm) — pro procenta průběhu.
    dist: f64,
    /// Odhad doby provedení (s).
    dur: f64,
    /// Příkaz zabírá místo v plánovači (nenulový pohyb).
    motion: bool,
    /// Příkaz čeká na dokončení všech pohybů (G28, G29, M400, G4).
    sync: bool,
    from: [f64; 3],
    to: [f64; 3],
    /// Extruzní úsek dráhy náhledu: (značka, pořadí úseku v dráze od 1).
    seg: Option<(SegMarker, usize)>,
}

/// Výsledek odhadu v daném okamžiku.
pub struct Estimate {
    pub cursor: Option<PrintCursor>,
    pub position: Option<[f64; 3]>,
    /// 0–100 % podle vytištěné délky.
    pub progress: f64,
    pub remaining_s: f64,
}

pub struct PrintTracker {
    lines: Vec<LineInfo>,
    /// Index posledního extruzního řádku dráhy ≤ i.
    last_ext: Vec<Option<usize>>,
    /// Součet odhadovaných dob od řádku i do konce.
    suffix_dur: Vec<f64>,
    total_dist: f64,
    /// Potvrzené, dosud neprovedené řádky (index, čas potvrzení).
    in_flight: VecDeque<(usize, Instant)>,
    front_start: Option<Instant>,
    done_dist: f64,
    last_done: Option<usize>,
    last_acked: Option<usize>,
    saturated: bool,
}

fn axis(line: &str, c: char) -> Option<f64> {
    dpi_core::parse_axis_value(line, c)
}

impl PrintTracker {
    /// `queue` = řádky k odeslání (bez komentářů), `markers` = značka dráhy
    /// platná pro daný řádek.
    pub fn new(queue: &[String], markers: &[Option<SegMarker>]) -> PrintTracker {
        let mut pos = [0.0_f64; 3];
        let mut feed = 1500.0_f64; // mm/min
        let mut relative = false;
        // Limity rychlosti os z M203 (mm/s)
        let mut max_feed = [f64::INFINITY; 4];
        let mut ext_count = 0usize;
        let mut prev_marker: Option<SegMarker> = None;

        let mut lines = Vec::with_capacity(queue.len());
        for (i, line) in queue.iter().enumerate() {
            let upper = line.trim().to_ascii_uppercase();
            let marker = markers.get(i).copied().flatten();
            if marker != prev_marker {
                ext_count = 0;
                prev_marker = marker;
            }
            let mut info = LineInfo {
                dist: 0.0,
                dur: 0.0,
                motion: false,
                sync: false,
                from: pos,
                to: pos,
                seg: None,
            };
            if dpi_core::is_linear_move(&upper) {
                if let Some(f) = axis(&upper, 'F') {
                    if f > 0.0 {
                        feed = f;
                    }
                }
                let mut to = pos;
                for (k, c) in ['X', 'Y', 'Z'].into_iter().enumerate() {
                    if let Some(v) = axis(&upper, c) {
                        to[k] = if relative { pos[k] + v } else { v };
                    }
                }
                let d = [to[0] - pos[0], to[1] - pos[1], to[2] - pos[2]];
                let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                let e = axis(&upper, 'E').unwrap_or(0.0).abs();
                let extrusion = dpi_core::is_extrusion_move(&upper);
                // Rychlost omezená limity jednotlivých os (mm/s)
                let mut speed = feed / 60.0;
                let motion_len = if len > 1e-9 {
                    for k in 0..3 {
                        if d[k].abs() > 1e-9 {
                            speed = speed.min(max_feed[k] * len / d[k].abs());
                        }
                    }
                    len
                } else {
                    speed = speed.min(max_feed[3]);
                    e
                };
                info.motion = motion_len > 1e-9;
                info.dur = if info.motion && speed > 0.0 { motion_len / speed } else { 0.0 };
                info.to = to;
                if extrusion {
                    info.dist = (d[0] * d[0] + d[1] * d[1]).sqrt();
                    if let Some(m) = marker {
                        ext_count += 1;
                        info.seg = Some((m, ext_count));
                    }
                }
                pos = to;
            } else if upper.starts_with("G90") {
                relative = false;
            } else if upper.starts_with("G91") {
                relative = true;
            } else if upper.starts_with("G92") {
                for (k, c) in ['X', 'Y', 'Z'].into_iter().enumerate() {
                    if let Some(v) = axis(&upper, c) {
                        pos[k] = v;
                    }
                }
                info.to = pos;
            } else if upper.starts_with("G28") {
                pos = [0.0; 3];
                info.to = pos;
                info.sync = true;
            } else if upper.starts_with("G29") || upper.starts_with("M400") {
                info.sync = true;
            } else if upper.starts_with("G4") {
                info.sync = true;
                info.dur = axis(&upper, 'S')
                    .or_else(|| axis(&upper, 'P').map(|ms| ms / 1000.0))
                    .unwrap_or(0.0);
            } else if upper.starts_with("M203") {
                for (k, c) in ['X', 'Y', 'Z', 'E'].into_iter().enumerate() {
                    if let Some(v) = axis(&upper, c) {
                        if v > 0.0 {
                            max_feed[k] = v;
                        }
                    }
                }
            }
            lines.push(info);
        }

        let mut last_ext = Vec::with_capacity(lines.len());
        let mut last = None;
        for (i, l) in lines.iter().enumerate() {
            if l.seg.is_some() {
                last = Some(i);
            }
            last_ext.push(last);
        }
        let mut suffix_dur = vec![0.0; lines.len() + 1];
        for i in (0..lines.len()).rev() {
            suffix_dur[i] = suffix_dur[i + 1] + lines[i].dur;
        }
        let total_dist = lines.iter().map(|l| l.dist).sum();

        PrintTracker {
            lines,
            last_ext,
            suffix_dur,
            total_dist,
            in_flight: VecDeque::new(),
            front_start: None,
            done_dist: 0.0,
            last_done: None,
            last_acked: None,
            saturated: false,
        }
    }

    fn complete_front(&mut self, end: Instant) {
        let Some((i, _)) = self.in_flight.pop_front() else { return };
        self.done_dist += self.lines[i].dist;
        self.last_done = Some(i);
        // Další pohyb začíná po dokončení předchozího, nejdřív však po potvrzení
        self.front_start = self.in_flight.front().map(|&(_, ack)| end.max(ack));
    }

    /// Dokončí pohyby, jejichž odhadovaná doba uplynula. Při plném plánovači
    /// rozhodují potvrzení, model jen pojistkou proti zaseknutí.
    fn advance(&mut self, now: Instant) {
        while let Some(&(i, _)) = self.in_flight.front() {
            let start = self.front_start.unwrap_or(now);
            let dur = self.lines[i].dur;
            let limit = if self.saturated && self.lines[i].motion {
                dur * 1.5 + 0.5
            } else {
                dur
            };
            if now.saturating_duration_since(start).as_secs_f64() < limit {
                break;
            }
            self.complete_front(start + Duration::from_secs_f64(dur));
        }
    }

    /// Řádek `i` byl potvrzen tiskárnou; `waited` = doba od odeslání.
    pub fn on_ack(&mut self, i: usize, waited: Duration, now: Instant) {
        if i >= self.lines.len() {
            return;
        }
        self.advance(now);
        if self.lines[i].sync {
            // Příkaz počkal na dokončení všech předchozích pohybů
            while !self.in_flight.is_empty() {
                self.complete_front(now);
            }
            self.saturated = false;
        }
        self.in_flight.push_back((i, now));
        if self.in_flight.len() == 1 {
            self.front_start = Some(now);
        }
        self.last_acked = Some(i);
        if self.lines[i].motion {
            let saturated = waited >= SATURATION_THRESHOLD;
            if saturated {
                // Uvolnilo se místo v plánovači = dokončil se nejstarší pohyb
                while let Some(&(j, _)) = self.in_flight.front() {
                    if j == i {
                        break;
                    }
                    let motion = self.lines[j].motion;
                    self.complete_front(now);
                    if motion {
                        break;
                    }
                }
            }
            self.saturated = saturated;
        }
    }

    /// Tisk se zastavil (pauza, konec fronty) — zbylé pohyby doběhnou podle modelu.
    pub fn drain(&mut self) {
        self.saturated = false;
    }

    /// Všechny potvrzené pohyby jsou podle modelu provedené.
    pub fn idle(&mut self, now: Instant) -> bool {
        self.advance(now);
        self.in_flight.is_empty()
    }

    fn cursor_at(&self, i: usize, frac: f64) -> Option<PrintCursor> {
        let (j, frac) = match self.lines[i].seg {
            Some(_) => (i, frac),
            None => (self.last_ext[i]?, 1.0),
        };
        let ((pos_idx, seg_idx, start_idx), k) = self.lines[j].seg?;
        Some(PrintCursor { pos_idx, seg_idx, start_idx, pt_idx: k - 1, frac })
    }

    pub fn estimate(&mut self, now: Instant) -> Estimate {
        self.advance(now);
        let mut front_dist = 0.0;
        let mut front_left = 0.0;
        let (cursor, position) = match (self.in_flight.front(), self.front_start) {
            (Some(&(i, _)), Some(start)) if now >= start => {
                let l = &self.lines[i];
                let el = now.saturating_duration_since(start).as_secs_f64();
                let frac = if l.dur > 1e-9 { (el / l.dur).min(1.0) } else { 1.0 };
                front_dist = frac * l.dist;
                front_left = (l.dur - el).max(0.0);
                let p = [0, 1, 2].map(|k| l.from[k] + (l.to[k] - l.from[k]) * frac);
                (self.cursor_at(i, frac), Some(p))
            }
            _ => match self.last_done {
                Some(i) => (self.cursor_at(i, 1.0), Some(self.lines[i].to)),
                None => (None, None),
            },
        };
        let queued: f64 = self.in_flight.iter().skip(1).map(|&(i, _)| self.lines[i].dur).sum();
        let unsent = self.last_acked.map_or(0, |i| i + 1);
        let progress = if self.total_dist > 0.0 {
            ((self.done_dist + front_dist) / self.total_dist * 100.0).min(100.0)
        } else {
            self.last_done.map_or(0.0, |i| (i + 1) as f64 / self.lines.len().max(1) as f64 * 100.0)
        };
        Estimate {
            cursor,
            position,
            progress,
            remaining_s: front_left + queued + self.suffix_dur[unsent.min(self.lines.len())],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker(gcode: &str) -> PrintTracker {
        let mut queue = Vec::new();
        let mut markers = Vec::new();
        let mut marker = None;
        for raw in gcode.lines() {
            if let Some(m) = parse_seg_marker(raw) {
                marker = Some(m);
                continue;
            }
            let clean = raw.split(';').next().unwrap().trim();
            if !clean.is_empty() {
                queue.push(clean.to_string());
                markers.push(marker);
            }
        }
        PrintTracker::new(&queue, &markers)
    }

    const GCODE: &str = "G90\nM83\n; DPI_SEG 1 0 0\nG0 X0 Y0 F3000\n\
        G1 X10 Y0 E1 F600\nG1 X10 Y10 E1 F600\n; DPI_SEG 1 1 2\nG0 X20 Y10 F3000\nG1 X30 Y10 E1 F600\n";

    #[test]
    fn test_parse_seg_marker() {
        assert_eq!(parse_seg_marker("; DPI_SEG 3 12 4"), Some((3, 12, 4)));
        assert_eq!(parse_seg_marker("; jiný komentář"), None);
        assert_eq!(parse_seg_marker("; DPI_SEG x"), None);
    }

    #[test]
    fn test_durations_and_cursor() {
        let mut t = tracker(GCODE);
        // G1 X10 F600 = 10 mm při 10 mm/s → 1 s
        assert!((t.lines[3].dur - 1.0).abs() < 1e-9);
        let t0 = Instant::now();
        for i in 0..t.lines.len() {
            t.on_ack(i, Duration::from_millis(2), t0);
        }
        // 1,5 s po startu: přejezd (0 s), první úsek hotový, druhý v polovině
        let e = t.estimate(t0 + Duration::from_millis(1500));
        let c = e.cursor.expect("kurzor");
        assert_eq!((c.pos_idx, c.seg_idx, c.pt_idx), (1, 0, 1));
        assert!((c.frac - 0.5).abs() < 1e-6);
        let p = e.position.unwrap();
        assert!((p[0] - 10.0).abs() < 1e-6 && (p[1] - 5.0).abs() < 1e-6);
        assert!((e.progress - 50.0).abs() < 1e-6, "{}", e.progress);
    }

    #[test]
    fn test_saturated_ack_completes_oldest_move() {
        let mut t = tracker(GCODE);
        let t0 = Instant::now();
        // Rychlá potvrzení naplní frontu, pak zpožděné potvrzení = plný plánovač
        for i in 0..=4 {
            t.on_ack(i, Duration::from_millis(2), t0);
        }
        let late = t0 + Duration::from_millis(100);
        t.on_ack(5, Duration::from_millis(90), late);
        // Plánovač byl plný → dokončil se nejstarší pohyb (první úsek dráhy 0),
        // i když podle modelu měl trvat ještě 0,9 s; teď se tiskne druhý úsek
        let c = t.estimate(late).cursor.expect("kurzor");
        assert_eq!((c.seg_idx, c.pt_idx), (0, 1));
        assert!(c.frac < 1e-9);
        // Bez dalšího potvrzení kurzor nepředběhne tiskárnu dál než na konec úseku
        let c = t.estimate(late + Duration::from_millis(1400)).cursor.unwrap();
        assert_eq!((c.seg_idx, c.pt_idx), (0, 1));
        assert!((c.frac - 1.0).abs() < 1e-9);
    }
}
