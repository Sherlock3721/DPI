//! Kurzor náhledu tisku — co z drah skla je „už vytištěné“.
//!
//! Tohle je referenční (testovaná) verze logiky; shader `slide.wgsl`
//! (`piece_vis`, `dot_visible`) ji zrcadlí řádek po řádku. Změna tady =
//! stejná změna tam.
//!
//! `SlideFrame::cursor` = [režim, aktuální dráha, start_idx, tištěný úsek k],
//! `cursor_frac[0]` = rozpracovaná část úseku k.

/// Režim 0: vše viditelné.
pub const MODE_ALL: i32 = 0;
/// Režim 1: nic viditelné (tisk skla ještě nezačal).
pub const MODE_NONE: i32 = 1;
/// Režim 2: dráhy před kurzorem celé, aktuální po kurzor, další skryté.
pub const MODE_PARTIAL: i32 = 2;

/// Stav jednoho úseku (instance `Piece`) vůči kurzoru.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PieceVis {
    pub visible: bool,
    /// Konec úseku jako podíl a→b (1 = celý úsek, rozpracovaný úsek < 1).
    pub end_frac: f32,
    /// Index úseku v pořadí tisku (u aktuální dráhy posunutý o start_idx), jinak `j`.
    pub k: u32,
    /// První tištěný úsek dráhy → kreslí se i počáteční čepička v `a`.
    pub first: bool,
}

/// Viditelnost úseku `j` dráhy `seg` (dráha má `n_pieces` úseků).
///
/// Uzavřená dráha se tiskne od bodu start_idx: tištěný index
/// k = (j − start) mod n_pieces (start má smysl jen u aktuální dráhy).
pub fn piece_vis(cursor: [i32; 4], frac: f32, seg: u32, j: u32, n_pieces: u32) -> PieceVis {
    let mut r = PieceVis {
        visible: true,
        end_frac: 1.0,
        k: j,
        first: j == 0,
    };
    match cursor[0] {
        MODE_NONE => {
            r.visible = false;
            return r;
        }
        MODE_PARTIAL => {}
        _ => return r,
    }
    let s = seg as i32;
    let cur = cursor[1];
    if s < cur {
        return r;
    }
    if s > cur {
        r.visible = false;
        return r;
    }
    let cp = cursor[3];
    if cp < 0 {
        // Tryska teprve přejíždí na začátek dráhy
        r.visible = false;
        return r;
    }
    let n = n_pieces.max(1);
    let start = (cursor[2].max(0) as u32) % n;
    let k = (j % n + n - start) % n;
    r.k = k;
    r.first = r.first || k == 0;
    let ki = k as i32;
    if ki < cp {
        return r;
    }
    if ki == cp {
        // NaN → 0 (stejně jako clamp v shaderu na běžných GPU)
        r.end_frac = if frac.is_nan() {
            0.0
        } else {
            frac.clamp(0.0, 1.0)
        };
        return r;
    }
    r.visible = false;
    r
}

/// Viditelnost tečky dráhy `seg`: tečka je hotová, jakmile tisk její dráhy začal.
pub fn dot_visible(cursor: [i32; 4], seg: u32) -> bool {
    match cursor[0] {
        MODE_NONE => false,
        MODE_PARTIAL => {
            let s = seg as i32;
            s < cursor[1] || (s == cursor[1] && cursor[3] >= 0)
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_all_and_none() {
        let all = piece_vis([MODE_ALL, 5, 3, 1], 0.5, 9, 4, 10);
        assert_eq!(
            all,
            PieceVis {
                visible: true,
                end_frac: 1.0,
                k: 4,
                first: false
            }
        );
        assert!(!piece_vis([MODE_NONE, 0, 0, 0], 0.5, 0, 0, 10).visible);
        // Neznámý režim = vše
        assert!(piece_vis([7, 0, 0, 0], 0.5, 3, 0, 1).visible);
        assert!(dot_visible([MODE_ALL, 0, 0, -1], 4));
        assert!(!dot_visible([MODE_NONE, 9, 0, 3], 0));
    }

    #[test]
    fn partial_segments_before_and_after() {
        let c = [MODE_PARTIAL, 2, 0, 1];
        let before = piece_vis(c, 0.3, 1, 5, 6);
        assert!(before.visible && before.end_frac == 1.0 && before.k == 5);
        assert!(!piece_vis(c, 0.3, 3, 0, 6).visible);
    }

    #[test]
    fn partial_current_segment_open_path() {
        let c = [MODE_PARTIAL, 2, 0, 2];
        let v: Vec<_> = (0..5).map(|j| piece_vis(c, 0.25, 2, j, 5)).collect();
        assert!(v[0].visible && v[0].first && v[0].end_frac == 1.0);
        assert!(v[1].visible && !v[1].first && v[1].end_frac == 1.0);
        assert!(v[2].visible && v[2].end_frac == 0.25);
        assert!(!v[3].visible && !v[4].visible);
    }

    #[test]
    fn partial_current_segment_closed_loop_with_start() {
        // Uzavřená dráha 4 úseků tištěná od bodu 2: pořadí j = 2, 3, 0, 1
        let c = [MODE_PARTIAL, 0, 2, 2];
        let v: Vec<_> = (0..4).map(|j| piece_vis(c, 0.5, 0, j, 4)).collect();
        assert_eq!(v.iter().map(|p| p.k).collect::<Vec<_>>(), vec![2, 3, 0, 1]);
        assert!(v[2].visible && v[2].first && v[2].end_frac == 1.0); // k = 0
        assert!(v[3].visible && v[3].end_frac == 1.0); // k = 1
        assert!(v[0].visible && v[0].end_frac == 0.5); // k = 2 rozpracovaný
        assert!(v[0].first); // j == 0 má počáteční čepičku vždy
        assert!(!v[1].visible); // k = 3
                                // start_idx == n (bod n = bod 0 u uzavřené dráhy) i záporný = od začátku
        assert_eq!(piece_vis([MODE_PARTIAL, 0, 4, 9], 0.0, 0, 1, 4).k, 1);
        assert_eq!(piece_vis([MODE_PARTIAL, 0, -3, 9], 0.0, 0, 1, 4).k, 1);
    }

    #[test]
    fn partial_travelling_and_frac_clamp() {
        // cur_piece == −1: tryska přejíždí, aktuální dráha celá skrytá
        assert!(!piece_vis([MODE_PARTIAL, 1, 0, -1], 0.0, 1, 0, 3).visible);
        assert!(piece_vis([MODE_PARTIAL, 1, 0, -1], 0.0, 0, 0, 3).visible);
        assert_eq!(
            piece_vis([MODE_PARTIAL, 0, 0, 0], 7.0, 0, 0, 3).end_frac,
            1.0
        );
        assert_eq!(
            piece_vis([MODE_PARTIAL, 0, 0, 0], -1.0, 0, 0, 3).end_frac,
            0.0
        );
        assert_eq!(
            piece_vis([MODE_PARTIAL, 0, 0, 0], f32::NAN, 0, 0, 3).end_frac,
            0.0
        );
        // n_pieces == 0 nesmí dělit nulou
        assert!(piece_vis([MODE_PARTIAL, 0, 5, 0], 0.5, 0, 0, 0).visible);
    }

    #[test]
    fn dots_follow_segment_start() {
        assert!(dot_visible([MODE_PARTIAL, 3, 0, -1], 2));
        assert!(!dot_visible([MODE_PARTIAL, 3, 0, -1], 3));
        assert!(dot_visible([MODE_PARTIAL, 3, 0, 0], 3));
        assert!(!dot_visible([MODE_PARTIAL, 3, 0, 5], 4));
    }
}
