#import "../template/components.typ": note-box, tip-box, important-box, warning-box, kbd

= 5. Příprava tiskové úlohy a procesní parametry

Příprava tiskového experimentu v DPI probíhá zcela digitálně prostřednictvím importu standardní vektorové grafiky a konfigurace chemicko-fyzikálních vlastností použitého roztoku.

== 5.1 Import vektorové předlohy (SVG a DXF)
DPI podporuje dva průmyslové a vědecké formáty vektorové grafiky:
- *SVG (Scalable Vector Graphics):* Standardní otevřený formát exportovatelný z grafických editorů jako je Inkscape, Adobe Illustrator, CorelDRAW či Python knihoven (např. `matplotlib`). DPI podporuje úsečky, lomené čáry, obdélníky, kružnice, elipsy a kubické i kvadratické Bézierovy křivky.
- *DXF (Drawing Exchange Format):* Standardní CAD formát používaný v AutoCADu, FreeCADu či SolidWorks pro přesné mikrofluidní kanálky a čipy.

=== Postup importu
1. V horní nabídce klikněte na *Soubor* -> *Načíst vzorek* nebo stiskněte #kbd("Ctrl") + #kbd("O").
2. Vyberte soubor `.svg` nebo `.dxf`. Program automaticky načte a zpracuje křivky do tiskových trajektorií.
3. *Přidání dalšího objektu:* Pokud chcete na stejné substráty přidat další nezávislý vzor (např. druhou vrstvu pro tisk s jinou jehlou nebo roztokem), zvolte *Soubor* -> *Přidat objekt* (#kbd("Ctrl") + #kbd("Shift") + #kbd("O")).

#tip-box(title: "Doporučení pro kreslení v Inkscape")[
  Při přípravě předlohy v Inkscape nastavte jednotky dokumentu na milimetry (`mm`) v měřítku 1:1 k reálnému podložnímu sklíčku. Všechny texty a speciální objekty před exportem převeďte na křivky (*Křivka* -> *Objekt na křivku*).
]

== 5.2 Automatické škálování a bezpečnostní okraje
Při načtení grafiky DPI automaticky provádí analýzu rozměrů:
- Pokud vzor přesahuje fyzické rozměry zvoleného sklíčka (např. 76 × 26 mm), aplikace automaticky nabídne proporční zmenšení.
- *Hlídání okrajů skla:* Tryska má určitý vnější průměr. DPI automaticky hlídá, aby žádná část kapky ani kovové patice jehly nezasáhla okraj sklíčka. Při změně průměru jehly na větší velikost aplikace upozorní na nutnost přizpůsobení geometrie.
- *Kompenzace šířky stopy:* Tryska u vyplněných tvarů jede o polovinu svého průměru uvnitř obrysu, takže vytištěný tvar má rozměr z výkresu (čtverec 24 mm se vytiskne jako 24 mm, ne 24 mm + šířka housenky). Díry se naopak o polovinu průměru zvětší. Ostré špičky se tím mírně zkrátí; tvary užší než tryska a čáry se tisknou beze změny. Rozlití kapaliny nad šířku trysky kompenzace nepokrývá.

== 5.3 Multiplikace vzorků a mřížka podložky
V laboratorní praxi je nutné zpracovávat desítky identických nebo mírně modifikovaných vzorků najednou.
V poli *Počet vzorků (Sample count)* v levém panelu můžete zvolit libovolný počet sklíček:
- DPI automaticky spočítá maximální kapacitu podložky Prusa MK3S na základě:
  - Fyzických rozměrů podložky (250 × 210 mm).
  - Počátečních odsazení od okrajů (`start_offset_x = 18 mm`, `start_offset_y = 11 mm`).
  - Bezpečnostních mezer mezi sousedními sklíčky (`multi_spacing = 5 mm`).
- Sklíčka jsou řazena do sloupců od levého okraje směrem doprava. Každé sklíčko v plátně obdrží své identifikační číslo vzorku.

== 5.4 Význam a konfigurace úvodního odplivu (Prime Feature)
Před zahájením samotného tisku mikrokapek na čistá sklíčka je kriticky nutné provést tzv. *Odpliv (Prime)*:

#important-box(title: "Proč je odpliv pro kapaliny zásadní?")[
  1. *Zasychání menisku:* Na hrotu jehly dochází během přípravy k částečnému odpaření rozpouštědla, což vede ke zahuštění kapaliny a vytvoření tuhé kapilární zátky.
  2. *Vzduchová kapsa:* Po nasazení nové jehly je vnitřní kanálek vyplněn vzduchem. Bez odplivu by prvních několik milimetrů dráhy netisklo vůbec nic.
  3. *Ustavení hydrodynamického tlaku:* Pružnost pístu a stlačitelnost mikrobublin vyžaduje natlakování soustavy.
]

V levém panelu zaškrtněte volbu *Aktivovat odpliv (Prime)*:
- Tiskárna před najetím nad první sklíčko sjede na bezpečnou zónu při levém okraji podložky (mimo skla) a vytlačí úzkou souvislou čistící linku o délce cca 30 mm.
- Teprve po ustavení stabilního kapkového menisku provede tisková hlava přejezd (Z-hop) na první sklíčko.

== 5.5 Výběr a správa kapalin (Liquid Profiles)
Různá rozpouštědla a suspenze mají odlišnou viskozitu, povrchové napětí a smáčivost. V horní liště aplikace naleznete menu *Kapaliny*:

#table(
  columns: (1.5fr, 1.2fr, 1.2fr, 2.5fr),
  table.header([*Kapalina*], [*Doporučený Z-offset*], [*Max. rychlost*], [*Specifika chování při dávkování*]),
  [Voda / Pufry], [0.15–0.25 mm], [1200 mm/min], [Vysoké povrchové napětí, tvoří vyšší zaoblené kapky, nízká smáčivost],
  [Ethanol / IPA], [0.08–0.15 mm], [800 mm/min], [Extrémní smáčivost, rychlý samovolný rozliv po skle, rychlý odpar],
  [DMSO / DMF], [0.20–0.30 mm], [1500 mm/min], [Vysoká viskozita, pomalý odpar, nutný pomalejší náběh tlaku],
  [Polymerní roztok (PEDOT:PSS)], [0.10–0.20 mm], [600 mm/min], [Náchylnost k tvorbě vláken (stringing), nutná delší prodleva],
)

Profil kapaliny umožňuje v konfiguračním okně (*Nastavení* -> *Kapaliny*) nastavit přísné laboratorní mantinely (*Guard Limits*), které brání chybě lidského faktoru při přípravě tisku:
- *Kategorie kapalin:* Chemické roztoky lze organizovat do skupin (např. _Prekurzory solárních článků_, _Biologické pufry_, _Vodivé polymery_). V rozbalovacích nabídkách se profily přehledně seskupují.
- *Bezpečnostní rozsah Z-offsetu (`z_offset_min` / `z_offset_max`):* Pokud laborant v panelu zadá hodnotu mimo tento interval, aplikace vstup odmítne. Tím je fyzicky vyloučeno poškrábání skla jehlou při nechtěném překlepu.
- *Limity dávkovací rychlosti a pojezdu:* Nastavení povoleného intervalu extruze (`extrusion_min/max`) a pojezdové rychlosti (`print_speed_min/max`).
- *Teplotní okno podložky (`bed_temp_min/max`):* Stanovuje povolený rozsah temperování substrátu pro dané rozpouštědlo.
- *Zakázané trysky (`forbidden_nozzles`):* Přetažením myší lze pro konkrétní kapalinu zakázat nevhodné jehly (např. pro suspenzi s nanočásticemi zakázat jehlu s malým průměrem 0.30 mm kvůli vysokému riziku zanesení). Pokud je zakázaná tryska vybrána, aplikace obsluhu ihned vizuálně varuje.
- *Barevné kódování:* Každému roztoku je přiřazen specifický barevný odstín usnadňující orientaci v multi-materiálových projektech.

== 5.6 Styly výplně (Infill Styles)
Pro uzavřené tvary (polygony, kružnice, obdélníky) nabízí DPI tři tiskové režimy:
1. *Pouze obrysy (Contour only):* Jehla vykreslí pouze vnější obvodové křivky. Ideální pro mikrofluidní stěny a oddělovací hráze.
2. *Pouze výplň (Infill only):* Plocha je vyplněna paralelními tahy s nastaveným rozestupem (např. 1.0 mm) pod zadaným úhlem (výchozí 0°, volitelně 45° či 90°).
3. *Okraje + Výplň (Contour + Infill):* Nejprve je vykreslen přesný obrys, který zabrání nežádoucímu přetékání kapalin, a následně je vnitřek vyplněn rastrem.

== 5.7 Svařování uzlů a čištění drah (Merge Nodes)
Při importu z CAD programů nebo vektorových editorů se stává, že křivky nejsou exportovány jako souvislé lomené čáry (polylines), nýbrž jako stovky drobných rozpojených úseček. Bez optimalizace by tiskárna na každém uzlu zvedala jehlu (Z-hop), což způsobuje nežádoucí kapání a trhání tekuté stopy.

DPI obsahuje integrovaný nástroj *Sloučit uzly (Merge Nodes)*:
- V nástrojové liště plátna klikněte na ikonu spojení uzlů.
- Zadejte toleranci vzdálenosti v milimetrech (např. `0.05 mm` nebo `0.10 mm`).
- Aplikace vyhledá sousedící koncové body v rámci zadaného poloměru a spojí je do jediné plynulé křivky. Tím se dramaticky sníží počet nutných přejezdů a zrychlí tisk.

== 5.8 Víceobjektový tisk a přiřazení trysek k objektům
Pokud tiskový experiment vyžaduje nanesení více různých činidel na jedno sklíčko (např. nanesení elektrodového materiálu a následně biologického senzoru):
1. Přidejte do projektu další vektorový objekt přes *Soubor* -> *Přidat objekt*.
2. V pravém panelu v sekci *Objekty* rozklikněte požadovaný objekt a v rozbalovací nabídce *Tryska* mu přiřaďte konkrétní jehlu (např. *Objekt 1* = Červená 0.3 mm, *Objekt 2* = Modrá 0.41 mm).
3. Tiskárna vytiskne všechny geometrie první trysky, poté vyvolá pauzu pro výměnu jehly a stříkačky, obsluha nasadí novou jehlu se stříkačkou a po potvrzení tiskárna provede homing a pokračuje druhým objektem.

== 5.9 Řízení teploty podložky a prahová hodnota (#raw("bed_min_temp"))
Některé laboratorní procesy (např. krystalizace solí či rychlé odpaření rozpouštědla) vyžadují temperování podložních skel:
- V profilu kapaliny nebo v nastavení tisku lze zadat cílovou teplotu vyhřívané podložky (`bed_temp`).
- *Prahová hodnota (šedá zóna 30 °C):* DPI implementuje bezpečnostní prahové pravidlo `bed_min_temp` (výchozí 30.0 °C). Pokud je požadovaná teplota nastavena na hodnotu menší než 30 °C (např. 0 °C nebo 25 °C), vyhřívání se považuje za neaktivní a do G-kódu se negenerují žádné příkazy pro čekání na teplotu (`M190`/`M140`).
- Je-li teplota nastavena na $>= 30 space degree "C"$, tiskárna před zahájením tisku zapne vyhřívání a počká na dosažení stabilní teploty sklíček.

== 5.10 Přímé kreslení geometrických tvarů na plátně
Kromě importu externích SVG a DXF souborů umožňuje DPI navrhovat základní tvary přímo v aplikaci bez nutnosti otevírat CAD software:
1. *Výběr nástroje:* V pravé části horní lišty plátna klikněte na ikonu:
   - *Obdélník (Square):* Pro ohraničené obdélníkové zóny či plošná pole.
   - *Elipsa (Circle):* Pro kruhová terčíková pole a kapilární rezervoáry.
   - *Čára (Line):* Pro rovné dělící linie či odplivové stopy.
2. *Kreslení tažením myši:* Klikněte na plochu vybraného sklíčka a tažením myši definujte rozměry.
3. *Klávesové modifikátory během kreslení:*
   - #kbd("Ctrl"): Aktivuje magnetické přichytávání k souřadné mřížce (Grid Snap).
   - #kbd("Alt"): Kreslí tvar symetricky od středu (namísto od rohu).
   - #kbd("Shift"): Zamkne poměr stran na 1:1 (vytvoří dokonalý čtverec či kružnici) nebo u úsečky zaokrouhlí úhel po 45°.
   - #kbd("Escape"): Okamžitě zruší rozkreslený tvar nebo opustí kreslicí režim.
4. *Automatická integrace:* Nakreslený tvar je okamžitě přidán do seznamu objektů jako samostatná plnohodnotná vektorová stopa, pro kterou lze volit styl výplně, rychlost extruze i přiřazenou jehlu.
