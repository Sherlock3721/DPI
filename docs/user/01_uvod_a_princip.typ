#import "../template/components.typ": note-box, tip-box, important-box, warning-box

= 1. Úvod a bezpečnostní zásady obsluhy

== 1.1 Účel programu Droplet Printing Interface (DPI)
*Droplet Printing Interface (DPI)* je řídicí desktopová aplikace pro mikrodávkování roztoků a suspenzí na skleněné substráty pomocí modifikované 3D tiskárny (referenční model *Prusa i3 MK3S*). 

Aplikace umožňuje obsluze:
- Připojit se k tiskárně přes USB a řídit její pohyby i dávkování.
- Načítat vektorové předlohy kapek a drah (SVG, DXF) nebo je přímo kreslit na plátně.
- Konfigurovat dávkovací parametry (Z-offset, rychlost pojezdu, dávkovaný objem v nl/mm či µl/mm).
- Provádět přesnou optickou kalibraci výšky jehly nad sklem pomocí USB mikroskopu.
- Řídit tisk na jedno nebo více podložních sklíček současně s možností individuálního nastavení pro každý vzorek.

#note-box(title: "Lokální provoz aplikace")[
  DPI funguje jako plně lokální aplikace bez nutnosti připojení k internetu. Veškeré tiskové úlohy a nastavení zůstávají uloženy výhradně v počítači.
]

== 1.2 Bezpečnostní zásady při obsluze tiskárny
Při práci s modifikovanou tiskárnou je nutné dbát zvýšené opatrnosti kvůli přítomnosti křehkých skleněných substrátů a ostrých dávkovacích jehel:

#important-box(title: "Prevence mechanických kolizí křehkého skla")[
  Skleněná podložní sklíčka a FTO substráty jsou křehké. Náraz kovové jehly nebo tiskové hlavy může způsobit roztříštění skla a poškození mechaniky tiskárny.
  - Před každým spuštěním tisku vždy proveďte optickou kontrolu Z-kalibrace pomocí kamery.
  - Ujistěte se, že sklíčka jsou správně a pevně usazena v držácích.
  - V případě hrozící kolize okamžitě stiskněte červené tlačítko *Nouzové zastavení (STOP)* v levém panelu aplikace.
]

#warning-box(title: "Manipulace s dávkovací soustavou")[
  - *Ostré jehly:* Dávkovací jehly mají ostrý nebo zúžený kovový hrot. Při nasazování a manipulaci v pracovním prostoru dbejte opatrnosti, aby nedošlo k poranění.
  - *Pohybující se osy:* Během tisku nezasahujte rukama do pracovního prostoru tiskárny. Pro manuální manipulaci s tiskovou hlavou vždy nejve uvolněte motory příkazem *Uvolnit motory (M84)* v diagnostickém okně.
]
