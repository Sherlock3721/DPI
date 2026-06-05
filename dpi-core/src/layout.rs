use crate::types::{BedConfig, LayoutPosition};

/// Vypočítá absolutní pozice pro multiplexní uspořádání sklíček na podložce.
/// Algoritmus začíná vlevo vzadu (bed_min_x + start_offset_x)
/// a skládá sklíčka směrem doprava (X+) a dozadu (Y+).
pub fn get_layout_positions(
    count: usize,
    slide_w: f64,
    slide_h: f64,
    spacing: f64,
    prime_active: bool,
    prime_glass_type: Option<&str>,
    bed: &BedConfig,
) -> Vec<LayoutPosition> {
    let mut positions = Vec::new();

    let mut curr_y = bed.offset_y;
    let mut curr_x = bed.min_x + bed.offset_x;

    let p_w = match prime_glass_type {
        Some("vzorkové") => slide_w,
        _ => 76.0,
    };
    let p_h = match prime_glass_type {
        Some("vzorkové") => slide_h,
        _ => 26.0,
    };

    // Šířka aktuálního sloupce (první sloupec může být širší kvůli odplivovému sklu)
    let mut current_col_w = if prime_active {
        slide_w.max(p_w)
    } else {
        slide_w
    };

    if prime_active {
        positions.push(LayoutPosition {
            x: curr_x,
            y: curr_y,
            width: p_w,
            height: p_h,
            is_prime: true,
        });
        curr_y += p_h + spacing;
    }

    for _ in 0..count {
        // Pokud sklíčko přeteče maximální výšku podložky, posuneme se doprava na nový sloupec
        if curr_y + slide_h > bed.max_y && !positions.is_empty() {
            curr_x += current_col_w + spacing;
            current_col_w = slide_w; // Další sloupce již nemají prime sklíčko
            curr_y = bed.offset_y;
        }

        // Sklíčko se nevejde ani po přesunu sloupce — netiskneme nic dalšího
        if curr_y + slide_h > bed.max_y {
            break;
        }

        // Pokud bychom přetekli přes pravou hranici, zastavíme rozložení
        if curr_x + slide_w > bed.max_x {
            break;
        }

        positions.push(LayoutPosition {
            x: curr_x,
            y: curr_y,
            width: slide_w,
            height: slide_h,
            is_prime: false,
        });
        curr_y += slide_h + spacing;
    }

    positions
}
