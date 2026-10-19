#import "../template/components.typ": note-box, tip-box, important-box, warning-box, badge, callout-table, kbd

= 7. Výroba příslušenství: Modul Bracket Export (Držáky sklíček)

Při rychlých pohybech tiskové podložky v ose Y (až 3000 mm/min) by volně položená sklíčka okamžitě sklouzla. DPI proto obsahuje unikátní vestavěný generátor laboratorních montážních matric – modul *Bracket Export*.

== 7.1 Princip magnetických pružných držáků
Držák generovaný v DPI zajišťuje spolehlivé usazení skla:
1. *Pevné L-rameno:* Tvoří pevnou referenční zarážku pro dva kolmé okraje sklíčka.
2. *Flexibilní meandrové pružiny:* Pružná ramena z tištěného plastu jemně, ale pevně dotlačují sklíčko do referenčního rohu, čímž kompenzují výrobní tolerance řezaných skel (obvykle $plus.minus 0.3 "mm"$).
3. *Neodymové magnety:* Vespod držáku jsou kapsy pro malé neodymové magnety (např. válečky $diameter 3 times 2 "mm"$), které celou matrici pevně přichytí k pružnému ocelovému plátu Prusa MK3S bez nutnosti šroubování.

#align(center)[
  #image("../assets/figures/fig05_bracket_export.svg", width: 95%)
  #v(-4pt)
  #text(size: 8.5pt, fill: rgb("#64748b"), [Obrázek 7.1: Okno modulu Bracket Export pro návrh a generování držáků])
]

#callout-table(
  [#badge("1")], [Volba rozměrů substrátu], [Výběr geometrie skla. Automaticky načte délku, šířku a nominální tloušťku skla (např. 1.0 mm).],
  [#badge("2")], [Parametry pevných stěn], [Nastavení tloušťky základních stěn L-profilu a možnost rozšíření okrajů pro stabilní dosednutí na podložku.],
  [#badge("3")], [Geometrie pružinových meandrů], [Počet ohybů pružiny, šířka prvku a kompenzace mezer pro tiskové tolerance.],
  [#badge("4")], [Otvory pro neodymové magnety], [Volba tvaru (kruhové vrtání vs. čtvercová kapsa) a průměru otvorů pro vlisování magnetů (výchozí 3.0 mm).],
  [#badge("5")], [Interaktivní 2D náhled], [Okamžitá vizualizace generované geometrie držáku, pružin a magnetických kapes v reálném čase.],
  [#badge("6")], [Tlačítka exportu (STL a SVG)], [Přímý export do 3D modelu #raw(".stl") pro tisk na 3D tiskárně nebo do vektorového #raw(".svg") pro laserové vyřezání.],
)

== 7.2 Klíčová konstrukční pravidla pro 3D tisk držáku
Při generování držáku je nutné respektovat pravidla navržená v DPI:

#important-box(title: "Tloušťka držáku nesmí přesáhnout tloušťku skla!")[
  Výška držáku v ose Z je v DPI záměrně automaticky omezena maximálně na tloušťku skla (`glassThickness`, obvykle 1.0 mm). Pokud by plastový rámeček přečníval nad horní rovinu sklíčka, hrozilo by, že dávkovací jehla při nízkém Z-offsetu (např. 150 µm) narazí do plastové hrany držáku.
]

=== Doporučené tiskové parametry pro výrobu držáku na Prusa MK3S:
- *Materiál:* PETG nebo ASA (vynikající pružnost meandrů a vysoká chemická odolnost vůči kapkám rozpouštědel; PLA je příliš křehké).
- *Výška vrstvy:* 0.15 mm nebo 0.20 mm.
- *Výplň:* 100 % (plný perimetr pro maximální tuhost stěn).
- *Magnety:* Neodymové magnety $diameter 3 times 2 "mm"$ vtlačte do otvorů na spodní straně matrice. Pro jistotu je lze zajistit kapkou vteřinového lepidla.
