#import "../template/components.typ": note-box, tip-box, important-box, warning-box, kbd

= 9. Laboratorní Troubleshooting (Řešení potíží)

V této kapitole naleznete postupy pro rychlé řešení nejčastějších laboratorních a technických komplikací, se kterými se můžete při provozu DPI setkat.

== 9.1 Přehledná diagnostická tabulka problémů

#table(
  columns: (1.5fr, 1.8fr, 2.7fr),
  table.header([*Symptom / Problém*], [*Pravděpodobná příčina*], [*Doporučené řešení*]),
  [Kapalina samovolně odkapává při přejezdech], [Stlačený vzduch/bublina ve stříkačce nebo hydrostatický tlak sloupce], [1. Důkladně odvzdušněte stříkačku.\ 2. Zvyšte hodnotu Z-Hopu.\ 3. Zkontrolujte těsnost pístu.],
  [Jehla na začátku skla netiskne nic (vynechává)], [Zaschnutý meniskus v ústí trysky nebo prázdný objem jehly], [1. Ujistěte se, že je zapnutá funkce *Odpliv (Prime)*.\ 2. Před tiskem jehlu otřete netkanou textilií navlhčenou v rozpouštědle.],
  [Kapalina se na skle slévá do jedné velké louže], [1. Z-offset je příliš malý (jehla drtí kapku).\ 2. Mastné či hydrofilní sklo], [1. Zvyšte Z-offset o 50–100 µm v nastavení nebo Overrides.\ 2. Vyčistěte skla v lázni IPA / piranha / UV-ozon.],
  [Kapky neulpívají a táhnou se za jehlou], [Z-offset je příliš velký, kapka nedosáhne na povrch skla], [1. Proveďte znovu Z-kalibraci pomocí mikroskopu.\ 2. Snižte Z-offset o 30–50 µm.\ 3. Mírně zvyšte dávkovací rychlost (nl/mm).],
  [Ztráta spojení / Zamrznutí komunikace], [Uvolněný USB kabel, rušení od spínaného zdroje nebo sleep režim OS], [1. Zkontrolujte kvalitní stíněný USB kabel.\ 2. Vypněte v systému úsporný režim USB řadiče.\ 3. Znovu klikněte na *Připojit*.],
  [Tiskárna naráží do držáku sklíček], [Nesprávně zadané rozměry podložky nebo posunutý držák], [1. Ověřte v Nastavení -> Tiskárna (Limity) hodnoty `start_offset_x/y`.\ 2. Zkontrolujte, zda je držák plně dosednutý na dorazy ocelového plátu Prusa.],
  [Sonda PINDA zachytila o sklo nebo držák], [Sonda nebyla po Z-homingu vysunuta/odklopena], [Vždy po provedení Z-homingu na ocelovém plátu povolte aretační šroubek a vysuňte sondu PINDA do horní servisní polohy podle výzvy na obrazovce.],
  [Chyba pohybu pod Z=0 / Omezení endstopu], [Firmware odmítá záporné souřadnice bez příkazu M211 S0], [Ujistěte se, že v G-kódu je aktivní vypnutí softwarových koncových spínačů `M211 S0` a posuv `z_shift`. V expertním režimu případně obnovte výchozí G-kódy.],
)

== 9.2 Co dělat při ucpání dávkovací jehly
Ucpání tenké jehly (0.30 mm) je v laboratorní praxi běžným jevem, zejména u koloidních suspenzí:
1. *Okamžité zastavení:* Stiskněte tlačítko *Pozastavit* v levém panelu.
2. *Demontáž jehly:* Povolte bajonet Luer-Lock a sejměte ucpanou jehlu.
3. *Vyčištění:*
   - Ponořte hrot jehly do ultrazvukové lázně s vhodným rozpouštědlem na 2–3 minuty.
   - Pomocí čisté stříkačky protlačte rozpouštědlo v protisměru (ze strany hrotu směrem do patice).
   - Nikdy nepoužívejte tvrdý ocelový drát, který by deformoval přesný vnitřní průměr trysky!
4. *Nasazení a obnovení:* Po nasazení jehly stiskněte *Pokračovat v tisku*. Tiskárna provede homing a bude pokračovat v experimentu.

== 9.3 Přehled klávesových zkratek aplikace DPI

#table(
  columns: (1.6fr, 1.4fr, 3fr),
  table.header([*Klávesová zkratka*], [*Kontext / Oblast*], [*Vyvolaná akce*]),
  [#kbd("Ctrl") + #kbd("O")], [Aplikace], [Otevřít dialog pro načtení nového vzorku (SVG/DXF)],
  [#kbd("Ctrl") + #kbd("Shift") + #kbd("O")], [Aplikace], [Přidat další objekt/vrstvu ke stávajícím substrátům],
  [#kbd("Ctrl") + #kbd("S")], [Aplikace], [Uložit projekt / Vygenerovat G-kód],
  [#kbd("Ctrl") + #kbd("Shift") + #kbd("S")], [Aplikace], [Uložit projekt jako nový soubor (#raw(".gcode"))],
  [#kbd("Ctrl") + #kbd("Z")], [Aplikace / Plátno], [Krok zpět v historii změn projektu (Undo)],
  [#kbd("Ctrl") + #kbd("Shift") + #kbd("Z")], [Aplikace / Plátno], [Krok vpřed v historii změn projektu (Redo)],
  [#kbd("Ctrl") + #kbd("Q")], [Aplikace], [Bezpečně ukončit aplikaci],
  [#kbd("←") #kbd("→") #kbd("↑") #kbd("↓")], [Plátno (Objekt)], [Posun vybraného objektu po ploše sklíčka o 1.0 mm],
  [#kbd("Shift") + #kbd("Šipky")], [Plátno (Objekt)], [Jemný laboratorní posuv objektu s krokem 0.1 mm (100 µm)],
  [#kbd("Delete") / #kbd("Backspace")], [Plátno (Objekt)], [Skrýt / odebrat vybraný objekt z aktuálního sklíčka],
  [#kbd("↑") / #kbd("↓")], [Z-Kalibrace modal], [Krokový vertikální posuv trysky vůči sklu],
  [#kbd("Escape")], [Aplikace / Plátno], [Zavřít otevřené modální okno nebo zrušit kreslení tvaru],
)
