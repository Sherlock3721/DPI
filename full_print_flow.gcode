; ╔══════════════════════════════════════════════════════════════════╗
; ║  DPI — KOMPLETNÍ TOK G-KÓDU (1 vzorek, prime aktivní)          ║
; ║  Podložka : 250 × 210 mm       Sklíčko : 76 × 26 × 1 mm       ║
; ║  Tryska   : Modrá Ø0.41 mm, h=31 mm    z_offset = 0.200 mm     ║
; ║  Bed Leveling : AKTIVNÍ (první tisk) → G28 (plný probe)        ║
; ║  z_shift  : 6.800 mm  →  virtual print_z = 1.000 mm           ║
; ╚══════════════════════════════════════════════════════════════════╝


; ┌──────────────────────────────────────────────────────────────────┐
; │  FÁZE 1 — INICIALIZACE + POHYB NA KALIBRACI                     │
; │  send_manual_blocking() — probíhá PŘED spuštěním tiskové fronty │
; └──────────────────────────────────────────────────────────────────┘

; ════════════════════════════════════════════════
; BLOK 1a: M410 + start_gcode (před prvním M1)
; ════════════════════════════════════════════════
M410                              ; Nouzový stop všech pohybů
;FLAVOR:Marlin
; --- INICIALIZACE TISKÁRNY PRO KAPALINY ---
M201 X1000 Y1000 Z200 E5000       ; Maximální zrychlení [mm/s²]
M203 X200 Y200 Z12 E120           ; Maximální rychlost [mm/s]
M204 S1250 T1250                  ; Zrychlení tisku / přejezdu
M205 X8.00 Y8.00 Z0.40 E4.50     ; Jerk limity
M205 S0 T0
G90                               ; Absolutní souřadnice pohybu
M83                               ; Relativní souřadnice extruze
M302 P1                           ; Bez kontroly studené extruze
M302 S0                           ; Vždy povol extruzi
M900 K0                           ; Disable Linear Advance (kapaliny)
G4 S1                             ; Prodleva 1 s
M300 S900 P150                    ; Pípnutí

; ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄
; ⚠  APP MODAL — "Push down the PINDA"
;    (čeká na libovolnou klávesu / klik Pokračovat)
;    M1 příkaz NENÍ odeslán tiskárně
; ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄

; ════════════════════════════════════════════════
; BLOK 1b: po potvrzení Fáze 1 modal #1
; ════════════════════════════════════════════════
G28                               ; Homování + mesh probe (PINDA dolů)
G92 E0.0                          ; Reset extrudéru
G0 Z20                            ; Bezpečná výška po homing
G0 Y200 F5000                     ; Vysunutí podložky pro přístup k PINDĚ
G4 S1
M300 S900 P150                    ; Pípnutí

; ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄
; ⚠  APP MODAL — "Retract the PINDA"
;    (čeká na libovolnou klávesu / klik Pokračovat)
;    M1 příkaz NENÍ odeslán tiskárně
; ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄

; ════════════════════════════════════════════════
; BLOK 1c: po potvrzení Fáze 1 modal #2
; ════════════════════════════════════════════════
G0 Y10 F5000                      ; Zpět na tiskovou pozici (PINDA retrahována)

; ════════════════════════════════════════════════
; BLOK 2: Pohyb na kalibrační pozici
;   positions[0] = prime střed: X194.000 Y24.000
;   calibVirtualZ = 1.000 (= print_z -5.8 + z_shift 6.8)
;   approachVirtualZ = 3.000 (= calibVirtualZ + 2.0)
; ════════════════════════════════════════════════
G0 Z20.000 F1000                  ; Výjezd do bezpečné výšky
G92 Z26.800                       ; Virtuální posun Z (z_shift = 6.800 mm)
G0 X194.000 Y24.000 F3000         ; Přejezd nad střed první pozice (prime)
M400                              ; Počkat na dokončení pohybu
G0 Z3.000 F1000                   ; Sjezd na přiblížení (approachVirtualZ)
G1 Z1.000 F300                    ; Pomalé spuštění na kalibrační výšku
M400                              ; Počkat

; ┌──────────────────────────────────────────────┐
; │  ⚙  Z-KALIBRACE (ZCalibrationModal)         │
; │     Uživatel nastaví Z-offset mikroposunem  │
; │     Potvrzení → spustí startPrintAfterCalib │
; └──────────────────────────────────────────────┘

; ════════════════════════════════════════════════
; Oddálení od skla po kalibraci (send_manual_blocking)
; ════════════════════════════════════════════════
G91                               ; Relativní mode
G0 Z5 F1000                       ; Oddálit trysku od skla (+5 mm)
G90                               ; Zpět na absolutní


; ╔══════════════════════════════════════════════════════════════════╗
; ║  FÁZE 2 — TISKOVÁ FRONTA  (start_print → Rust gcode queue)     ║
; ╚══════════════════════════════════════════════════════════════════╝

G21                               ; Nastaveni jednotek na milimetry
;FLAVOR:Marlin
; --- INICIALIZACE TISKÁRNY PRO KAPALINY ---
M201 X1000 Y1000 Z200 E5000
M203 X200 Y200 Z12 E120
M204 S1250 T1250
M205 X8.00 Y8.00 Z0.40 E4.50
M205 S0 T0
G90
M83
M302 P1
M302 S0
M900 K0
G4 S1
M300 S900 P150
; APP_PAUSE:Push down the PINDA    ← Rust zachytí → modal, NENÍ odesláno tiskárně
G28                               ; Homování + mesh probe (PINDA dolů)
G92 E0.0
G0 Z20
G0 Y200 F5000
G4 S1
M300 S900 P150
; APP_PAUSE:Retract the PINDA      ← Rust zachytí → modal, NENÍ odesláno tiskárně
G0 Y10 F5000

; ════════════════════════════════════════════════
; VIRTUÁLNÍ POSUN Z  (z_shift = 6.800 mm)
; ════════════════════════════════════════════════
; --- VIRTUALNI POSUN Z (SHIFT 6.80mm) ---
G1 Z20 F1000                      ; Vyjezd do bezpecne vysky
G92 Z26.800                       ; Nastaveni posunute nuly

; (bed_temp = 0 → M140/M190 přeskočeno)

; ════════════════════════════════════════════════
; PRIME — ODPLIVOVÁ POZICE
;   Pozice: X156.000 Y11.000, 76×26 mm
;   Vzor: 15×15 mm na středu → (186.500, 16.500) – (201.500, 31.500)
;   Rozestup: 0.410 mm (= nozzle_diam)
;   e_per_mm = 32.388  (100 nl/mm × cal.f. 0.323877)
; ════════════════════════════════════════════════
; --- VZOREK (ODPLIV) ---
G90                               ; Absolutni souradnice pohybu
M83                               ; Relativni souradnice extruze
G1 Z3.000 F1000                   ; Z-hop pro odpliv (1.000 + 2.000)
G0 X186.500 Y16.500 F3000         ; Přejezd na start odplivového vzoru
G1 Z1.000 F1000                   ; Sjezd k povrchu
G1 X201.500 Y16.500 E486.820 F1500
G1 X201.500 Y16.910 E13.267 F1500
G1 X186.500 Y16.910 E486.820 F1500
G1 X186.500 Y17.320 E13.267 F1500
G1 X201.500 Y17.320 E486.820 F1500
G1 X201.500 Y17.730 E13.267 F1500
G1 X186.500 Y17.730 E486.820 F1500
G1 X186.500 Y18.140 E13.267 F1500
G1 X201.500 Y18.140 E486.820 F1500
G1 X201.500 Y18.550 E13.267 F1500
G1 X186.500 Y18.550 E486.820 F1500
; ... (řádky odplivu pokračují po Y = 31.500)
G0 Z3.000 F1000                   ; Z-hop po odplivu


; ╔══════════════════════════════════════════════════════════════════╗
; ║  VZOREK 1 — TISK OBJEKTU                                        ║
; ║  Pozice: X156.000 Y42.000, 76×26 mm                             ║
; ║  Cesta: z SVG/DXF souboru, styl Okraje + Výplň, 1.000 mm       ║
; ╚══════════════════════════════════════════════════════════════════╝
; --- VZOREK 1 ---
G90
M83
G92 E0.0                          ; Reset extrudéru

;;; ┌──────────────────────────────────────────────────────────────────┐
;;; │  TISKOVÁ DRÁHA — VZOREK 1                                        │
;;; │  print_z_virtual = 1.000 mm    z_hop_virtual = 3.000 mm         │
;;; │  Příklad: obdélník 50×20 mm se středem na sklíčku (194, 55)    │
;;; │  Okraje  → 4 úsečky obvodu                                       │
;;; │  Výplň   → horizontální čáry, rozestup 1.000 mm                 │
;;; └──────────────────────────────────────────────────────────────────┘
;;;
;;; ─── OKRAJE (obvod) ────────────────────────────────────────────────
;;;
;;; G1 Z3.000 F1000                ; Z-hop → přejezd na start
;;; G0 X169.000 Y45.000 F3000      ; Přejezd na roh [0]
;;; G1 Z1.000 F1000                ; Sjezd na tiskovou výšku
;;; G1 X219.000 Y45.000 E1619.385 F1500  ; Okraj →  (50 mm)
;;; G1 X219.000 Y65.000 E647.754 F1500   ; Okraj ↑  (20 mm)
;;; G1 X169.000 Y65.000 E1619.385 F1500  ; Okraj ←  (50 mm)
;;; G1 X169.000 Y45.000 E647.754 F1500   ; Okraj ↓  (20 mm)
;;; G1 E-32.388 F3000              ; Retrakce (pokud retraction > 0)
;;; G1 Z3.000 F1000                ; Z-hop po segmentu
;;;
;;; ─── VÝPLŇ (horizontální čáry, 1 mm rozestup) ─────────────────────
;;;
;;; G1 Z3.000 F1000
;;; G0 X169.500 Y46.000 F3000      ; Přejezd na start výplně řádek 1
;;; G1 Z1.000 F1000
;;; G1 E32.388 F3000               ; De-retrakce
;;; G1 X218.500 Y46.000 E1603.511 F1500  ; Výplň → řádek 1  (49 mm)
;;; G1 E-32.388 F3000              ; Retrakce
;;; G1 Z3.000 F1000
;;;
;;; G1 Z3.000 F1000
;;; G0 X218.500 Y47.000 F3000      ; Přejezd na start výplně řádek 2
;;; G1 Z1.000 F1000
;;; G1 E32.388 F3000               ; De-retrakce
;;; G1 X169.500 Y47.000 E1603.511 F1500  ; Výplň ← řádek 2  (49 mm)
;;; G1 E-32.388 F3000
;;; G1 Z3.000 F1000
;;;
;;; G1 Z3.000 F1000
;;; G0 X169.500 Y48.000 F3000
;;; G1 Z1.000 F1000
;;; G1 E32.388 F3000
;;; G1 X218.500 Y48.000 E1603.511 F1500  ; Výplň → řádek 3  (49 mm)
;;; G1 E-32.388 F3000
;;; G1 Z3.000 F1000
;;;
;;; ... (řádky výplně pokračují po Y = 64.000, celkem 19 řádků)
;;;
;;; G1 Z3.000 F1000                ; Zvednuti po tisku sklicka
;;; └──────────────────────────────────────────────────────────────────┘


; ════════════════════════════════════════════════
; KONEC TISKU
; ════════════════════════════════════════════════
; (bed_temp = 0 → M140 S0 přeskočeno)
G0 Z30 F1000                      ; Zvednuti tiskove hlavy
G0 X0 Y200 F3000                  ; Vysunuti podlozky vpred
M84                               ; Vypnuti motoru
