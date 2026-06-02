use crate::types::{BedConfig, LayoutPosition};

/// Vypočítá absolutní pozice pro multiplexní uspořádání sklíček na podložce.
/// Algoritmus začíná vpravo vpředu (bed_max_x - start_offset_x - šířka sloupce)
/// a skládá sklíčka směrem doleva (X-) a dozadu (Y+).
pub fn get_layout_positions(
    count: usize,
    slide_w: f64,
    slide_h: f64,
    spacing: f64,
    prime_active: bool,
    bed: &BedConfig,
) -> Vec<LayoutPosition> {
    let mut positions = Vec::new();

    let mut curr_y = bed.offset_y;
    let mut base_right = bed.max_x - bed.offset_x;

    // Šířka aktuálního sloupce (první sloupec může být širší kvůli odplivovému sklu)
    let mut current_col_w = if prime_active {
        slide_w.max(76.0)
    } else {
        slide_w
    };
    let mut curr_col_left = base_right - current_col_w;

    if prime_active {
        let p_w = 76.0;
        let p_h = 26.0;
        let p_x = base_right - p_w;
        positions.push(LayoutPosition {
            x: p_x,
            y: curr_y,
            width: p_w,
            height: p_h,
            is_prime: true,
        });
        curr_y += p_h + spacing;
    }

    for _ in 0..count {
        // Pokud sklíčko přeteče maximální výšku podložky, posuneme se doleva na nový sloupec
        if curr_y + slide_h > bed.max_y && !positions.is_empty() {
            base_right = curr_col_left - spacing;
            current_col_w = slide_w; // Další sloupce již nemají prime sklíčko
            curr_col_left = base_right - current_col_w;
            curr_y = bed.offset_y;
        }

        // Sklíčko se nevejde ani po přesunu sloupce — netiskneme nic dalšího
        if curr_y + slide_h > bed.max_y {
            break;
        }

        let sample_x = base_right - slide_w;

        // Pokud bychom přetekli přes levou hranici, zastavíme rozložení
        if sample_x < bed.min_x {
            break;
        }

        // Sklíčko přetéká vpravo (mimo podložku) — přeskočíme
        if sample_x + slide_w > bed.max_x {
            break;
        }

        positions.push(LayoutPosition {
            x: sample_x,
            y: curr_y,
            width: slide_w,
            height: slide_h,
            is_prime: false,
        });
        curr_y += slide_h + spacing;
    }

    positions
}
