# G-kód generátor — vývojový diagram

```mermaid
flowchart TD
    START([🚀 generate_gcode]) --> INIT

    subgraph INIT["⚙️ Inicializace"]
        direction TB
        I1["ExtrusionCalculator\n(filament_diam, flow_mult, calib_factor)"]
        I1 --> I2["Výpočet min_needed_z\npřes všechna sklíčka + overrides\n─────────────────────────\nget_abs_z = −block_height + nozzle_height\n        − nozzle_hidden + slide_z + z_offset"]
        I2 --> I3{min_needed_z < 0?}
        I3 -- Ano --> I4["z_shift = |min_needed_z| + 1\n(virtuální posunutí nuly Z)"]
        I3 -- Ne  --> I5[z_shift = 0]
    end

    INIT --> HEADER

    subgraph HEADER["📄 Hlavička G-kódu"]
        direction TB
        H1["G21  — jednotky mm"]
        H1 --> H2["machine.start_gcode"]
        H2 --> H3{z_shift > 0?}
        H3 -- Ano --> H4["G1 Z{safe_z} F1000\nG92 Z{safe_z + z_shift}\n(přenastavení nuly)"]
        H3 -- Ne  --> H5
        H4 --> H5{bed_temp > 0?}
        H5 -- Ano --> H6["M140 S{temp}  — start ohřevu\nM190 S{temp}  — čekání na teplotu"]
        H5 -- Ne  --> H7[" "]
        H6 --> H7
    end

    HEADER --> LAYOUT["📐 get_layout_positions\n(sample_count, slide_w/h,\n spacing, prime_active, bed)"]
    LAYOUT --> LOOP_START

    subgraph LOOP["🔁 Smyčka přes pozice  (for pos in positions)"]
        direction TB
        LOOP_START["machine.loop_start_gcode\nG90 — absolutní pohyb\nM83 — relativní extruze"]
        LOOP_START --> OVR["Načti override pro pozici\n─────────────────────\nis_prime → key = &quot;-1&quot;\nvzorek N → key = &quot;N&quot;"]
        OVR --> PZ["Výpočet print_z\n─────────────────────\n= −block_height + loc_nozzle_h\n  − nozzle_hidden + slide_z\n  + loc_z + z_shift"]
        PZ --> BRANCH{pos.is_prime?}

        BRANCH -- Ano --> PRIME
        BRANCH -- Ne  --> SAMPLE

        subgraph PRIME["💧 Odplivová pozice (Prime)"]
            direction TB
            PR1["G92 E0  — reset extruderu"]
            PR1 --> PR2["Výpočet spacing (rozestup linií)"]
            PR2 --> PR3{infill_type?}
            PR3 -- &quot;%&quot;    --> PR4["spacing = nozzle_diam / (val/100)"]
            PR3 -- &quot;počet&quot; --> PR5["spacing = prime_h / val"]
            PR3 -- jinak  --> PR6["spacing = val  (nebo nozzle_diam)"]
            PR4 & PR5 & PR6 --> PR7["Z-hop → G0 na (x1, y1) → sjezd na print_z"]
            PR7 --> PR8{curr_y ≤ y2?}
            PR8 -- Ano --> PR9["G1 X{target_x} Y{curr_y} E{...} F{loc_spd}\n← tisková linie (boustrophedon)"]
            PR9 --> PR10{curr_y + spacing ≤ y2?}
            PR10 -- Ano --> PR11["G1 Y{curr_y+spacing}\n← přechod na další řádek"]
            PR10 -- Ne  --> PR12[" "]
            PR11 & PR12 --> PR13["curr_y += spacing\ndirection *= −1"]
            PR13 --> PR8
            PR8 -- Ne --> PR14["G0 Z{print_z + z_hop}"]
        end

        subgraph SAMPLE["🧪 Vzorek N"]
            direction TB
            S1["G92 E0  — reset extruderu"]
            S1 --> S2["Načti slide_paths[N] + transforms[N]\n(fallback: scale=1, rot=0, offset=pos.x/y)"]
            S2 --> SEGLOOP{Další segment?}
            SEGLOOP -- Ne --> S_END["G1 Z{print_z + z_hop}\n— zdvihnutí po tisku"]
            SEGLOOP -- Ano --> S3{segment prázdný?}
            S3 -- Ano --> SEGLOOP
            S3 -- Ne  --> S4["transform_pt(p0) → abs_p0\n(scale → rotace → posun gui_dx/dy)"]
            S4 --> S5{infill_style == &quot;Tečky&quot;\nAND p[0] == p[1]?}

            S5 -- Ano --> DOT
            S5 -- Ne  --> LINE

            subgraph DOT["⚫ Dot Dispensing"]
                direction TB
                D1["G1 Z{print_z + z_hop}"]
                D1 --> D2["G0 X{abs_p0.x} Y{abs_p0.y}"]
                D2 --> D3["G1 Z{print_z}"]
                D3 --> D4["G1 E{dot_extrusion} F300\n— dávkování kapky"]
                D4 --> D5["G1 Z{print_z + z_hop}"]
            end

            subgraph LINE["➡️ Normální čára"]
                direction TB
                L1["G1 Z{print_z + z_hop}\nG0 X{abs_p0.x} Y{abs_p0.y}\nG1 Z{print_z}"]
                L1 --> L2{is_retracted\nAND retraction > 0?}
                L2 -- Ano --> L3["G1 E{+retraction} F{retract_speed}\n— de-retrakce"]
                L2 -- Ne  --> L4
                L3 --> L4["Smyčka windows(2) přes body:\n transform_pt(pa), transform_pt(pb)\n dist = √((pb−pa)²)\n G1 X{pb.x} Y{pb.y} E{dist×e_per_mm} F{loc_spd}"]
                L4 --> L5{retraction > 0?}
                L5 -- Ano --> L6["G1 E{−retraction} F{retract_speed}\n— retrakce po segmentu"]
                L5 -- Ne  --> L7[" "]
                L6 --> L7
            end

            DOT & LINE --> SEGLOOP
        end

        PRIME & SAMPLE --> LEND["machine.loop_end_gcode"]
        LEND --> NEXT{Další pozice?}
        NEXT -- Ano --> LOOP_START
        NEXT -- Ne  --> FOOTER
    end

    subgraph FOOTER["🏁 Závěr"]
        direction TB
        F1{bed_temp > 0?}
        F1 -- Ano --> F2["M140 S0  — vypnout ohřev podložky"]
        F1 -- Ne  --> F3
        F2 --> F3["machine.end_gcode"]
    end

    FOOTER --> RESULT(["✅ Výsledek\n(G-kód string, total_dist mm, total_time sec)"])
```

---

## Klíčové datové toky

| Parametr        | Výpočet / Zdroj                                                            |
| --------------- | -------------------------------------------------------------------------- |
| `print_z`       | `−block_height + loc_nozzle_h − nozzle_hidden + slide_z + loc_z + z_shift` |
| `e_per_mm`      | `ExtrusionCalculator::calculate_e_per_mm(loc_ext, loc_ext_unit)`           |
| `dot_extrusion` | `ExtrusionCalculator::calculate_dot_extrusion(loc_ext, loc_ext_unit)`      |
| Absolutní X/Y   | `slide_paths` → `transform_pt()` (scale → rotace → posun)                  |
| Override        | `slide_overrides["-1"]` pro prime, `slide_overrides["N"]` pro vzorek N     |
| `z_shift`       | Automatická korekce pokud `min_needed_z < 0`                               |

## Override hierarchie (pro každý parametr)

```
slide_overrides[key].param  →  params.param  (globální fallback)
```
