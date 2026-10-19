#import "../template/components.typ": note-box, tip-box, important-box, warning-box, badge, callout-table

= 2. Příprava a součásti modifikované tiskárny (Prusa MK3S)

Tato kapitola popisuje fyzické komponenty dávkovací soustavy, se kterými obsluha manipuluje při přípravě a provozu tiskárny.

== 2.1 Komponenty tiskové hlavy
Původní tisková hlava je vybavena stříkačkovým dávkovačem, kde posuv pístu zajišťuje krokový motor osy E:

#align(center)[
  #image("../assets/figures/fig08_syringe_hardware.svg", width: 95%)
  #v(-4pt)
  #text(size: 8.5pt, fill: rgb("#64748b"), [Obrázek 2.1: Schéma tiskové hlavy s popisem klíčových prvků])
]

#callout-table(
  [#badge("1")], [Pohon pístu stříkačky], [Krokový motor s lineárním šroubem, který tlačí přímo na píst upevněné stříkačky.],
  [#badge("2")], [Tělo stříkačky (1 ml / 5 ml)], [Laboratorní stříkačka (skleněná Hamilton nebo polypropylenová) sevřená v úchytu tiskové hlavy.],
  [#badge("3")], [Dávkovací Luer-Lock jehla], [Výměnná ocelová dávkovací jehla s barevnou paticí.],
  [#badge("4")], [Indukční sonda PINDA], [Senzor výšky pro referenční homing – *měří výhradně ocelový plát mimo skleněné substráty*.],
  [#badge("5")], [USB mikroskopická kamera], [Kamera zaměřená na hrot jehly pro optickou kalibraci Z-výšky a vizuální kontrolu kapek.],
)

== 2.2 Volba stříkaček a dávkovacích jehel
V nastavení DPI jsou předdefinovány profily nejčastěji používaných laboratorních jehel:

#table(
  columns: (1.5fr, 1.2fr, 1.2fr, 1fr, 2.5fr),
  table.header([*Označení jehly*], [*Délka (#raw("nozzle_height"))*], [*Vnitřní průměr*], [*Barevný kód*], [*Typické laboratorní použití*]),
  [Modrá jehla], [31.0 mm], [0.41 mm (Gauge 22)], [#text(fill: rgb("#3b82f6"), weight: "bold", [Modrá])], [Viskóznější roztoky, polymery, širší linky],
  [Červená jehla], [31.1 mm], [0.30 mm (Gauge 25)], [#text(fill: rgb("#ef4444"), weight: "bold", [Červená])], [Nízkoviskózní kapaliny, jemné kapky, vysoké rozlišení],
  [Vlastní (Custom)], [Dle měření], [Dle měření], [Volitelná], [Jakákoliv speciální jehla nebo kapilára],
)

#tip-box(title: "Měření délky trysky (nozzle_height)")[
  Délka trysky se měří posuvným měřítkem od dosedacího čela držáku až po hrot jehly. Hodnota `nozzle_hidden` (výchozí 4.0 mm) představuje část patice zapuštěnou uvnitř upínacího bloku.
]

== 2.3 Kalibrační faktor stříkačky (#raw("calibration_factor"))
Aby program DPI správně převedl zadaný objem v mikrolitrech (#raw("µl")) nebo nanolitrech (#raw("nl")) na přesný posuv pístu stříkačky v milimetrech (osa E tiskárny), využívá kalibrační faktor $k$ v jednotkách $[space "mm posuvu pístu" / "µl dávkovaného objemu" space]$.

#table(
  columns: (2fr, 1.5fr, 1.5fr),
  table.header([*Typ stříkačky*], [*Vnitřní průměr válce*], [*Kalibrační faktor (#raw("k"))*]),
  [Hamilton 1 ml (skleněná mikrostříkačka)], [1.98 mm], [0.3239 mm/µl (výchozí hodnota)],
  [Standardní 5 ml (plastová)], [9.50 mm], [0.0141 mm/µl],
)

=== Nastavení faktoru v programu:
1. V horní liště otevřete *Nastavení* -> *Tiskárna*.
2. Do pole *Kalibrační faktor* zadejte příslušnou hodnotu $k$ pro vaši stříkačku.
3. *Alternativně:* Zadejte přímo vnitřní průměr válce stříkačky v milimetrech do pole *Průměr stříkačky* – program faktor $k$ dopočítá automaticky.
4. Pro novou nestandardní stříkačku lze faktor snadno určit: změřte posuvným měřítkem délku stupnice v mm pro známý objem v µl a vydělte je ($k = "posuv v mm" / "objem v µl"$).

== 2.4 Pravidla pro obsluhu indukční sondy PINDA
#warning-box(title: "KRITICKÉ: PINDA nedetekuje sklo a vyžaduje uvolnění před tiskem!")[
  Indukční sonda PINDA reaguje pouze na kovový ocelový plát. *Skleněná podložní sklíčka jsou pro sondu neviditelná!*
  
  Jelikož sklíčko i držák vystupují nad úroveň plátu, nesmí sonda zůstat ve spodní pracovní poloze, jinak by při přejezdu narazila do držáku nebo skla.

  *Postup obsluhy:*
  1. *Homing:* Tiskárna provede referenční sjezd osy Z (`G28 W`) nad obnaženým ocelovým plátem v bezpečném rohu mimo držáky skel.
  2. *Výzva k uvolnění:* Program automaticky pozastaví činnost a zobrazí výzvu na obrazovce:
     _„Odstraňte PINDA sondu z podložky (tiskové hlavy), než bude zahájen tisk.“_
  3. *Vysunutí sondy:* Obsluha imbusovým klíčem povolí aretační šroubek sondy PINDA na tiskové hlavě a vysune ji nahoru (nebo odklopí do servisní polohy), aby byl nejnižším bodem hlavy pouze hrot dávkovací jehly.
  4. *Potvrzení:* Teprve po mechanickém zajištění sondy v horní poloze klikněte na *Pokračovat* v aplikaci.
]

== 2.5 Ovládání USB mikroskopu v aplikaci
Pro sledování dávkování a přesnou optickou kalibraci výšky slouží video stream z USB mikroskopu v pravém panelu aplikace:
- *Volba kamery:* V rozbalovací nabídce vyberte připojený USB mikroskop.
- *Orientace obrazu:* Pomocí tlačítek můžete obraz otočit o 90°, 180° nebo 270° a horizontálně zrcadlit tak, aby směr na monitoru odpovídal skutečnému pohybu tiskárny.
- *Snímek (Snapshot):* Kliknutím na ikonu fotoaparátu uložíte aktuální snímek rozhraní jehla-substrát do souboru `.png`.
- *Záznam videa:* Tlačítko nahrávání umožňuje zaznamenat průběh dávkování kapek do formátu `.webm`.
- *Plovoucí okno (Pop-out):* Tlačítkem zvětšení otevřete video stream v samostatném okně, které lze umístit na druhý monitor pro nepřetržitý dohled nad tiskem.
