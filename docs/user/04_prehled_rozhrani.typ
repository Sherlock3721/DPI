#import "../template/components.typ": note-box, tip-box, important-box, warning-box, badge, callout-table, kbd

= 4. Podrobný průvodce uživatelským rozhraním

Uživatelské rozhraní DPI je navrženo s důrazem na přehlednost v laboratorním prostředí. Využívá třísloupcový layout s tmavou laboratorní estetikou šetřící zrak obsluhy.

== 4.1 Hlavní okno aplikace (Přehled)

#align(center)[
  #image("../assets/figures/fig01_ui_main_overview.svg", width: 98%)
  #v(-4pt)
  #text(size: 8.5pt, fill: rgb("#64748b"), [Obrázek 4.1: Hlavní okno DPI s vyznačenými funkčními zónami])
]

#callout-table(
  [#badge("1")], [Hlavička aplikace (Header)], [Hlavní menu *Soubor* (projekty, import, export CSV), výběr *Kapaliny*, menu *Nástroje* (Bracket Export, Diagnostika), *Nastavení* a nápověda.],
  [#badge("2")], [Správa připojení tiskárny], [Výběr sériového portu, baudrate, stav spojení a živá telemetrie tiskárny (aktuální souřadnice X/Y/Z a teplota podložky).],
  [#badge("3")], [Procesní parametry a krájení (Slicing)], [Volba typu podložního skla, konfigurace jehly/trysky, výška Z-offsetu, dávkovací rychlost (nl/mm), styl výplně a tlačítka spuštění tisku.],
  [#badge("4")], [Centrální grafické plátno (Canvas2D)], [Interaktivní vizualizace tiskové podložky Prusa MK3S (250 × 210 mm), rozložení sklíček, odplivové linky a aktuální polohy trysky.],
  [#badge("5")], [Lokální přepisy sklíček (Slide Overrides)], [Individuální nastavení Z-offsetu, dávkování, rychlosti nebo stylu výplně pro každé jednotlivé sklíčko zvlášť.],
  [#badge("6")], [Ovládací panely (Kamera, Jog, Terminál)], [Živý obraz z USB mikroskopu s možností rotace o 180°, manuální krokovací klávesnice (Jog) a vestavěný terminál.],
)

== 4.2 Levý panel: Procesní a tiskové parametry
Levý panel je řídícím centrem pro přípravu tiskové úlohy a její odeslání do tiskárny.

#align(center)[
  #image("../assets/figures/fig02_left_panel_controls.svg", width: 65%)
  #v(-4pt)
  #text(size: 8.5pt, fill: rgb("#64748b"), [Obrázek 4.2: Detail ovládacích prvků levého panelu])
]

#callout-table(
  [#badge("1")], [Sériový port a připojení], [Výběr portu tiskárny a tlačítko Připojit/Odpojit. Po připojení indikuje stav zelenou barvou.],
  [#badge("2")], [Telemetrie pozice a teplot], [Živý odečet souřadnic X, Y, Z a teploty vyhřívané podložky Prusa v reálném čase.],
  [#badge("3")], [Volba podložního skla], [Výběr geometrie substrátu: *Laboratorní sklo (76 × 26 × 1 mm)*, *FTO (76 × 50 × 1 mm)* nebo *Vlastní* rozměr.],
  [#badge("4")], [Konfigurace dávkovací trysky], [Presety jehel: *Modrá (0.41 mm)*, *Červená (0.30 mm)* nebo *Vlastní* s automatickým načtením výšky a zapuštění.],
  [#badge("5")], [Z-Offset a volba jednotek], [Klíčový parametr výšky dávkování nad sklem. Možnost přepínání mezi mikrometry (#raw("µm")) a milimetry (#raw("mm")).],
  [#badge("6")], [Rychlost tisku a extruze], [Rychlost pojezdu osy X/Y v mm/min a dávkovací rychlost s volbou jednotek (#raw("nl/mm"), #raw("µl/mm") nebo kroky/mm).],
  [#badge("7")], [Styl výplně a šrafování], [Možnost volby: *Pouze obrysy*, *Pouze výplň*, nebo *Okraje + Výplň*. Nastavení hustoty rozestupů a úhlu natočení šrafů.],
  [#badge("8")], [Tlačítka řízení tisku], [Zelené tlačítko *Spustit tisk*, žluté *Pozastavit* a červené *Nouzové zastavení (STOP)* s automatickým odjezdem hlavy.],
)

== 4.3 Centrální plátno (Canvas2D)
Plátno věrně simuluje pracovní prostor tiskárny Prusa MK3S:

#align(center)[
  #image("../assets/figures/fig03_canvas_multislide.svg", width: 75%)
  #v(-4pt)
  #text(size: 8.5pt, fill: rgb("#64748b"), [Obrázek 4.3: Vizualizace tiskové podložky a rozložení substrátů na plátně])
]

#callout-table(
  [#badge("1")], [Ohraničení podložky (Bed Limits)], [Modrý obdélník reprezentující fyzické rozměry ocelového plátu Prusa MK3S (výchozí 250 × 210 mm).],
  [#badge("2")], [Odplivová dráha (Prime Line)], [Svislá linie na levém okraji podložky. Zde tiskárna před tiskem vytlačí definovaný objem kapaliny pro ustavení menisku.],
  [#badge("3")], [Matrice podložních sklíček], [Automaticky vypočtená mřížka skel uspořádaná do sloupců a řad s respektováním nastavených rozestupů (#raw("multi_spacing")).],
  [#badge("4")], [Vektorový vzor kapek], [Vizuální náhled drah kapek. Čárkované čáry znázorňují rychlopřejezdy bez dávkování, plné čáry aktivní extruzi.],
  [#badge("5")], [Počátek souřadnic [0, 0]], [Levý přední roh podložky (Home pozice os X a Y).],
)

=== Horní nástrojová lišta plátna (Canvas Toolbar)
V záhlaví plátna se nachází rychlá nástrojová lišta s interaktivními funkcemi:
- *Zobrazení mřížky (Grid):* Zapne nebo vypne souřadnou mřížku tiskové podložky.
- *Pravítko (Měření vzdáleností):* Aktivuje interaktivní měřicí režim. Kliknutím na dva libovolné body na plátně se zobrazí přesná vzdálenost v milimetrech včetně složek $Delta X$ a $Delta Y$.
- *Vycentrovat (Fit View):* Vycentruje celou tiskovou desku do okna plátna.
- *Na sklíčko (Center on Slide):* Přiblíží a vycentruje pohled na konkrétní vybrané podložní sklíčko.
- *Sloučit uzly (Merge Nodes):* Otevře vyskakovací okno pro vyhlazení a svaření rozpojených vektorových drah.
- *Kreslicí nástroje (Obdélník, Elipsa, Úsečka):* Umožňují nakreslit geometrické tvary přímo na sklíčko bez nutnosti otevírat externí grafický editor.

=== Kontextová nabídka a ovládání objektů na plátně
Kliknutím pravým tlačítkem myši na libovolný objekt se zobrazí kontextové menu:
- *Vycentrovat:* Posune objekt přesně na geometrický střed daného sklíčka.
- *Resetovat:* Vrátí objekt do výchozího měřítka, natočení a pozice.
- *Otočit o 90°:* Rychlá rotace po směru hodinových ručiček.
- *Zrcadlit (Vodorovně / Svisle):* Horizontální a vertikální překlopení trajektorie.
- *Aplikovat na všechna skla:* Replikuje aktuální polohu, měřítko a rotaci vybraného objektu na všechny ostatní vzorky v tiskové dávce.
- *Skrýt z tohoto skla (#kbd("Delete") / #kbd("Backspace")):* Vratně deaktivuje objekt pouze na konkrétním sklíčku.

=== Mikropozicování z klávesnice a režimy transformace
Pro přesné laboratorní usazení nabízí plátno pokročilé interakce:
- *Dvojklik na vybraný objekt:* Přepíná interaktivní transformační režim mezi *Scale* (úchopové čtverečky v rozích pro změnu velikosti) a *Rotate* (kruhové úchopy pro plynulé otáčení).
- *Modifikátory myši při transformaci:*
  - Přidržením klávesy #kbd("Alt"): *Synchronizovaný pohyb* – tažení vybraného objektu na jednom sklíčku současně posune tentýž objekt na všech ostatních substrátech o stejný relativní vektor.
  - Přidržením klávesy #kbd("Ctrl"): Kvantování transformace – rotace skáče přesně po *15°*, měřítko po krocích *0.1* a poloha se přichytává k mřížce.
  - Přidržením klávesy #kbd("Shift"): Kotva ve středu geometrie (škálování symetricky vůči těžišti namísto protilehlého rohu).
- *Posun z klávesnice:*
  - Samotné šipky #kbd("←") #kbd("→") #kbd("↑") #kbd("↓"): Krok *1.0 mm*.
  - Se stisknutou klávesou #kbd("Shift"): Jemný laboratorní posuv *0.1 mm* (100 µm).

== 4.4 Pravý panel: Slide Overrides a přímé ovládání
Pravý panel nabízí pokročilé laboratorní funkce a přímou správu jednotlivých vzorků:

#align(center)[
  #image("../assets/figures/fig04_right_panel_overrides.svg", width: 65%)
  #v(-4pt)
  #text(size: 8.5pt, fill: rgb("#64748b"), [Obrázek 4.4: Pravý panel s lokálními přepisy sklíček, kamerou a Jog ovládáním])
]

#callout-table(
  [#badge("1")], [Accordion sklíček (Vzorky)], [Rozbalovací karty pro jednotlivé vzorky (Sample 1, Sample 2...). Umožňuje nezávislou konfiguraci každého skla v jedné tiskové dávce.],
  [#badge("2")], [Lokální přepis parametrů (Overrides)], [Aktivace lokálního Z-offsetu, jiné dávkovací rychlosti (#raw("nl/mm")) nebo hustoty výplně pro daný vzorek (např. gradientní experimenty).],
  [#badge("3")], [Správa objektů a viditelnosti], [Seznam vektorových tvarů na substrátu s možností dočasného skrytí oka nebo přiřazení jiné trysce.],
  [#badge("4")], [Živý obraz z mikroskopu], [Video stream z USB mikroskopu s tlačítky pro otočení o 180° a zrcadlení horizontální osy.],
  [#badge("5")], [Krokovací klávesnice (Jog)], [Manuální pojezd v osách X, Y a Z s volbou kroku (0.1 mm, 1 mm, 10 mm, 50 mm) a tlačítkem Home (G28).],
)

=== Detailní parametry lokálního přepisu (Slide Overrides)
V rozbalovací kartě každého vzorku lze přepsat globální procesní hodnoty:
- *Uživatelský název a poznámka:* Vložení popisku vzorku (např. „Vzorek B2 – 20 mM koncentrace“), který se promítne do exportovaného laboratorního protokolu (CSV).
- *Lokální Z-Offset:* Umožňuje nastavit odlišnou tiskovou výšku (v µm nebo mm) pro každé sklo zvlášť, což je ideální pro testování smáčivosti a stability menisku v gradientních řadách.
- *Dávkování a rychlost:* Přepis extruze (#raw("nl/mm"), #raw("µl/mm") nebo kroky/mm) a pojezdové rychlosti (mm/min).
- *Přepis výplně:* Volba vlastního rozestupu šrafování (#raw("mm"), #raw("%") nebo celkový počet drah) a stylu výplně (Pouze obrysy, Výplň, Obrysy + Výplň).
- *Správa objektů na substrátu:* Zobrazení fyzických rozměrů objektu v milimetrech, přepínání viditelnosti a přiřazení specifické jehly s okamžitým barevným odlišením dráhy.

=== Manuální ovládání tiskárny (Jog Widget)
Spodní část pravého panelu obsahuje přímé řízení pohonů tiskárny, které je aktivní pouze tehdy, když je stroj připojen a právě netiskne:
- *Krokový přírůstek:* Volba vzdálenosti jednoho pojezdu: `0.1 mm` (jemné dostavení), `1.0 mm`, `10.0 mm` a `50.0 mm` (rychlé přejíždění).
- *Nezávislé parkování os:*
  - *Home XY (`G28 X Y`):* Zaparkuje podložku a hlavu bez pohybu svislé osy Z.
  - *Home Z (`G28 Z`):* Samostatný referenční sjezd osy Z na ocelovém plátu.
  - *Kalibrace sítě podložky (`G80`):* Spustí automatické měření nerovností podložky firmwarem Marlin.
- *Příkazová řádka G-kódu:* Vstupní pole pro okamžité odeslání jakéhokoliv manuálního Marlin příkazu přímo do základní desky Einsy.

=== Bezpečnostní systém tiskových výzev a dialogů
Během přípravy a tisku mohou nastat situace vyžadující zásah obsluhy (pauza tisku, výzva k uvolnění sondy PINDA, výměna jehly či potvrzení operace).
Aplikace obsahuje bezpečnostní pojistky:
- *Fronta výzev:* Pokud nastane více událostí za sebou, dialogy se zobrazují postupně. Žádná výzva se nepřeskočí.
- *Ochrana proti nechtěnému kliknutí:* Po otevření dialogového okna je potvrzovací tlačítko na zlomek sekundy (400 ms) neaktivní. Tím je zabráněno nechtěnému potvrzení kroku překliknutím z předchozí práce.

== 4.5 Konfigurace aplikace a expertní režim (Settings Modal)
Do hlavního okna konfigurace vstoupíte kliknutím na *Nastavení* v horní liště. Konfigurační okno je rozděleno do tematických záložek:
- *Tryska:* Správa profilů dávkovacích jehel (délka, vnitřní průměr, zapuštěná část, barva).
- *Skla:* Knihovna rozměrů substrátů (šířka, výška, tloušťka v ose Z).
- *Kapaliny:* Chemické profily roztoků (procesní Z-offsety, rychlosti extruze, povolené trysky a teploty).
- *Tiskárna:* Fyzické limity podložky, kalibrační faktor stříkačky, výška bloku hlavy a přechod do konfigurace Nivelace podložky.
- *Tisk:* Globální strategie pojezdu (`route_mode`: nejrychlejší vs. `no_cross`) a priorita tisku (`substrate_major` vs. `nozzle_major`).
- *Program:* Přepínání tmavého a světlého motivu, sezónní animace a aktivace *Expertního režimu*.

=== Expertní režim a editace G-kódů
Expertní režim je zabezpečený mód určený pro servisní techniky a vývojáře procesů:
1. *Aktivace:* V záložce *Program* přepněte přepínač *Aktivovat expertní režim* a potvrďte bezpečnostní varování.
2. *Záložka G-kódy:* Po aktivaci se v levém menu nastavení zpřístupní nová záložka *G-kódy*, kde lze přímo editovat šablony `start_gcode`, `end_gcode`, `loop_start_gcode`, `loop_end_gcode` a makro výměny trysky `nozzle_change_gcode`.
3. *Trvalé ukládání (Persistence):* Veškeré úpravy G-kódů se po stisku *Uložit* zapisují *trvale* do konfiguračního souboru `settings.json`. Změny zůstávají v platnosti napořád, i po vypnutí expertního režimu či restartu aplikace.
4. *Bezpečnostní zámek:* Samotný přepínač expertního režimu se po ukončení aplikace z bezpečnostních důvodů automaticky vrací do vypnutého stavu, aby běžný uživatel nemohl omylem poškodit strojní makra.

=== Chování funkce „Obnovit výchozí“
V levém dolním rohu okna nastavení se nachází tlačítko *Obnovit výchozí*:
- *Je-li expertní režim ZAPNUTÝ:* Obnoví se kompletní tovární nastavení tiskárny *včetně* výchozích Marlin šablon G-kódů.
- *Je-li expertní režim VYPNUTÝ:* Obnoví se pouze běžné fyzické limity tiskárny, rychlosti a body nivelace. Uživatelsky přizpůsobené G-kódy v expertní sekci zůstanou *zcela nedotčeny*!
