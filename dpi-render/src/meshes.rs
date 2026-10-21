//! Šablonové sítě instancí (housenka, plochý pásek, kapka, disk).
//!
//! Vrchol šablony je `[f32; 4]`:
//! - housenka/pásek: (ldir, lperp, lup, druh) — lokální souřadnice na jednotkové
//!   sféře v rámu úseku (směr a→b, kolmice doleva, Z). Shader z nich skládá
//!   polohu `základ + dir·ldir·r + perp·lperp·r + Z·lup·h` a normálu elipsoidu.
//!   Druh: 0 = tělo v `a`, 1 = tělo v `b`, 2 = koncová čepička v `b`,
//!   3 = počáteční čepička v `a`; +4 = plochý pásek (LOD).
//! - kapka/disk: (x, y, z, plochý) — offset na jednotkové polokouli.
//!
//! Indexy jsou absolutní (bez base_vertex — WebGL2 ho neumí). Trojúhelníky
//! jsou CCW při pohledu zvenku (orientace se kontroluje a opravuje při stavbě).

use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Dělení půlelipsy průřezu housenky (musí být sudé: čepička má N_PHI/2 prstenců).
const N_PHI: u32 = 8;
/// Dělení půlkruhu čepičky (azimut −90°…+90°).
const N_AZ: u32 = 8;
/// Dělení plného kruhu kapky.
const N_DOT: u32 = 12;
/// Prstence kapky (elevace).
const N_DOT_EL: u32 = 3;
/// Dělení plochého půlkruhu pásku.
const N_FAN: u32 = 6;

/// Rozsah indexů jedné šablony.
#[derive(Debug, Clone, Copy, Default)]
pub struct Template {
    pub first_index: u32,
    pub index_count: u32,
}

impl Template {
    pub fn range(&self) -> std::ops::Range<u32> {
        self.first_index..self.first_index + self.index_count
    }
}

pub struct Meshes {
    pub vertices: Vec<[f32; 4]>,
    pub indices: Vec<u16>,
    /// 3D housenka: tělo + koncová čepička (kreslí se pro všechny úseky).
    pub bead: Template,
    /// 3D počáteční čepička (jen začátky drah — zvláštní instance).
    pub bead_start: Template,
    /// Plochý pásek: tělo + koncový půlkruh (LOD).
    pub ribbon: Template,
    /// Plochý počáteční půlkruh.
    pub ribbon_start: Template,
    /// Pásek s oběma půlkruhy (duchové; `ribbon` a `ribbon_start` jdou za sebou).
    pub ribbon_full: Template,
    /// 3D kapka.
    pub dome: Template,
    /// Plochý disk (LOD kapky).
    pub disc: Template,
}

struct Builder {
    vertices: Vec<[f32; 4]>,
    indices: Vec<u16>,
}

impl Builder {
    fn vertex(&mut self, v: [f32; 4]) -> u16 {
        self.vertices.push(v);
        (self.vertices.len() - 1) as u16
    }

    fn begin(&self) -> u32 {
        self.indices.len() as u32
    }

    fn end(&self, first: u32) -> Template {
        Template {
            first_index: first,
            index_count: self.indices.len() as u32 - first,
        }
    }

    /// Trojúhelník; orientace se srovná tak, aby normála mířila ven.
    fn tri(&mut self, a: u16, b: u16, c: u16, check: CheckFn) {
        let (pa, oa) = check(self.vertices[a as usize]);
        let (pb, ob) = check(self.vertices[b as usize]);
        let (pc, oc) = check(self.vertices[c as usize]);
        let n = cross(sub(pb, pa), sub(pc, pa));
        let out = [
            oa[0] + ob[0] + oc[0],
            oa[1] + ob[1] + oc[1],
            oa[2] + ob[2] + oc[2],
        ];
        if dot(n, out) < 0.0 {
            self.indices.extend_from_slice(&[a, c, b]);
        } else {
            self.indices.extend_from_slice(&[a, b, c]);
        }
    }
}

/// Kontrolní poloha a směr „ven“ vrcholu šablony (pro srovnání orientace trojúhelníků).
type CheckFn = fn([f32; 4]) -> ([f32; 3], [f32; 3]);

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Kontrolní poloha (úsek délky 3, r = h = 1) a směr „ven“ vrcholu housenky.
fn bead_check(v: [f32; 4]) -> ([f32; 3], [f32; 3]) {
    let kind = v[3].round() as u32;
    let along = if matches!(kind & 3, 1 | 2) { 3.0 } else { 0.0 };
    if kind & 4 != 0 {
        ([along + v[0], v[1], 0.0], [0.0, 0.0, 1.0])
    } else {
        ([along + v[0], v[1], v[2]], [v[0], v[1], v[2]])
    }
}

fn dot_check(v: [f32; 4]) -> ([f32; 3], [f32; 3]) {
    if v[3] > 0.5 {
        ([v[0], v[1], 0.0], [0.0, 0.0, 1.0])
    } else {
        ([v[0], v[1], v[2]], [v[0], v[1], v[2]])
    }
}

/// Polokoule/čtvrtelipsoid: poledníky po azimutech `az0 + i·az_step`,
/// `rings` prstenců elevace + pól.
struct Cap {
    kind: f32,
    /// −1 = zrcadlit ldir (počáteční čepička míří dozadu).
    sign: f32,
    az0: f32,
    az_step: f32,
    meridians: u32,
    rings: u32,
    /// Poslední poledník navazuje na první (plný kruh).
    closed: bool,
    check: CheckFn,
}

fn dome_cap(b: &mut Builder, c: &Cap) {
    let cols = if c.closed {
        c.meridians
    } else {
        c.meridians + 1
    };
    let mut grid = Vec::with_capacity((cols * c.rings) as usize);
    for r in 0..c.rings {
        let e = FRAC_PI_2 * r as f32 / c.rings as f32;
        for i in 0..cols {
            let a = c.az0 + c.az_step * i as f32;
            grid.push(b.vertex([
                c.sign * e.cos() * a.cos(),
                e.cos() * a.sin(),
                e.sin(),
                c.kind,
            ]));
        }
    }
    let pole = b.vertex([0.0, 0.0, 1.0, c.kind]);
    let at = |r: u32, i: u32| grid[(r * cols + i % cols) as usize];
    for r in 0..c.rings {
        for i in 0..c.meridians {
            let (p00, p01) = (at(r, i), at(r, i + 1));
            if r + 1 < c.rings {
                let (p10, p11) = (at(r + 1, i), at(r + 1, i + 1));
                b.tri(p00, p01, p11, c.check);
                b.tri(p00, p11, p10, c.check);
            } else {
                b.tri(p00, p01, pole, c.check);
            }
        }
    }
}

pub fn build() -> Meshes {
    let mut b = Builder {
        vertices: Vec::new(),
        indices: Vec::new(),
    };

    // ── 3D housenka ──
    let first = b.begin();
    {
        // Tělo: půlelipsa φ ∈ [0, π] — lperp = cos φ, lup = sin φ, v a i v b
        let ring = |b: &mut Builder, kind: f32| -> Vec<u16> {
            (0..=N_PHI)
                .map(|i| {
                    let phi = PI * i as f32 / N_PHI as f32;
                    b.vertex([0.0, phi.cos(), phi.sin(), kind])
                })
                .collect()
        };
        let ra = ring(&mut b, 0.0);
        let rb = ring(&mut b, 1.0);
        for i in 0..N_PHI as usize {
            b.tri(ra[i], rb[i], rb[i + 1], bead_check);
            b.tri(ra[i], rb[i + 1], ra[i + 1], bead_check);
        }
    }
    // Čepičky: azimut −90°…+90° (ldir ≥ 0 vpřed); krajní poledníky = průřez těla
    let bead_cap = |kind: f32, sign: f32| Cap {
        kind,
        sign,
        az0: -FRAC_PI_2,
        az_step: PI / N_AZ as f32,
        meridians: N_AZ,
        rings: N_PHI / 2,
        closed: false,
        check: bead_check,
    };
    dome_cap(&mut b, &bead_cap(2.0, 1.0));
    let bead = b.end(first);
    let first = b.begin();
    dome_cap(&mut b, &bead_cap(3.0, -1.0));
    let bead_start = b.end(first);

    // ── Plochý pásek ──
    let first = b.begin();
    {
        let a0 = b.vertex([0.0, 1.0, 0.0, 4.0]);
        let a1 = b.vertex([0.0, -1.0, 0.0, 4.0]);
        let b0 = b.vertex([0.0, 1.0, 0.0, 5.0]);
        let b1 = b.vertex([0.0, -1.0, 0.0, 5.0]);
        b.tri(a0, a1, b1, bead_check);
        b.tri(a0, b1, b0, bead_check);
    }
    let fan = |b: &mut Builder, kind: f32, sign: f32| {
        let c = b.vertex([0.0, 0.0, 0.0, kind]);
        let rim: Vec<u16> = (0..=N_FAN)
            .map(|i| {
                let a = -FRAC_PI_2 + PI * i as f32 / N_FAN as f32;
                b.vertex([sign * a.cos(), a.sin(), 0.0, kind])
            })
            .collect();
        for i in 0..N_FAN as usize {
            b.tri(c, rim[i], rim[i + 1], bead_check);
        }
    };
    fan(&mut b, 6.0, 1.0);
    let ribbon = b.end(first);
    let first_start = b.begin();
    fan(&mut b, 7.0, -1.0);
    let ribbon_start = b.end(first_start);
    let ribbon_full = b.end(first);

    // ── 3D kapka ──
    let first = b.begin();
    let cap = Cap {
        kind: 0.0,
        sign: 1.0,
        az0: 0.0,
        az_step: TAU / N_DOT as f32,
        meridians: N_DOT,
        rings: N_DOT_EL,
        closed: true,
        check: dot_check,
    };
    dome_cap(&mut b, &cap);
    let dome = b.end(first);

    // ── Plochý disk ──
    let first = b.begin();
    {
        let c = b.vertex([0.0, 0.0, 0.0, 1.0]);
        let rim: Vec<u16> = (0..N_DOT)
            .map(|i| {
                let a = TAU * i as f32 / N_DOT as f32;
                b.vertex([a.cos(), a.sin(), 0.0, 1.0])
            })
            .collect();
        for i in 0..N_DOT as usize {
            b.tri(c, rim[i], rim[(i + 1) % N_DOT as usize], dot_check);
        }
    }
    let disc = b.end(first);

    Meshes {
        vertices: b.vertices,
        indices: b.indices,
        bead,
        bead_start,
        ribbon,
        ribbon_start,
        ribbon_full,
        dome,
        disc,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_are_consistent_and_outward() {
        let m = build();
        assert!(m.vertices.len() < u16::MAX as usize);
        for t in [
            m.bead,
            m.bead_start,
            m.ribbon,
            m.ribbon_start,
            m.ribbon_full,
            m.dome,
            m.disc,
        ] {
            assert!(t.index_count > 0 && t.index_count % 3 == 0);
            assert!((t.first_index + t.index_count) as usize <= m.indices.len());
        }
        assert_eq!(
            m.ribbon_full.index_count,
            m.ribbon.index_count + m.ribbon_start.index_count
        );
        // Všechny trojúhelníky housenky mají nedegenerovanou plochu a normálu ven
        let r = m.bead.first_index as usize..m.bead_start.range().end as usize;
        assert_eq!(m.bead.range().end, m.bead_start.first_index);
        for tri in m.indices[r].chunks(3) {
            let p: Vec<_> = tri
                .iter()
                .map(|&i| bead_check(m.vertices[i as usize]))
                .collect();
            let n = cross(sub(p[1].0, p[0].0), sub(p[2].0, p[0].0));
            let out = [
                p[0].1[0] + p[1].1[0] + p[2].1[0],
                p[0].1[1] + p[1].1[1] + p[2].1[1],
                p[0].1[2] + p[1].1[2] + p[2].1[2],
            ];
            assert!(dot(n, n) > 1e-8, "degenerovaný trojúhelník");
            assert!(dot(n, out) > 0.0, "trojúhelník míří dovnitř");
        }
    }
}
