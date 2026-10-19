#import "../template/components.typ": note-box, tip-box, important-box, warning-box, badge, kbd

= 8. Praktický průvodce experimentem krok za krokem

Tato kapitola slouží jako rychlý standardní operační postup (SOP) pro laboranta, který provádí tiskovou dávku od prázdné tiskárny až po hotový vzorek a vygenerovaný protokol.

== Krok 1: Příprava roztoku a naplnění stříkačky
1. Připravte požadovaný roztok (např. 1 ml suspenze perovskitového prekurzoru).
2. Natáhněte roztok do stříkačky (ideálně přes tenkou kanylu nebo filtr).
3. *Kritický krok – Odstranění mikrobublin:*
   - Otočte stříkačku hrotem vzhůru a lehkým poklepáváním vytlačte veškerý vzduch.
   - Jakákoliv vzduchová bublina uvnitř válce stříkačky působí jako pružina a způsobí nekontrolované odkapávání kapaliny i po zastavení motoru!
4. Našroubujte zvolenou Luer-Lock dávkovací jehlu (např. modrou 0.41 mm).
5. Vložte stříkačku do držáku na ose X tiskárny a dotáhněte aretační šrouby. Přitlačte píst k jezdci lineárního šroubu.

== Krok 2: Příprava tiskové podložky a substrátů
1. Očistěte pružný ocelový plát tiskárny Prusa MK3S isopropanolem (IPA).
2. Položte na ocelový plát 3D tištěný držák (matrici). Neodymové magnety držák okamžitě pevně zafixují.
3. Vložte čistá laboratorní podložní sklíčka (76 × 26 mm) do matrice. Ujistěte se, že meandrové pružiny skla pevně domáčkly do referenčního rohu.

== Krok 3: Spuštění DPI a připojení tiskárny
1. Zapněte tiskárnu hlavním vypínačem a propojte ji USB kabelem s počítačem.
2. Spusťte aplikaci DPI.
3. V levém panelu zkontrolujte nastavení portu (*"Automaticky"* nebo `/dev/ttyACM0` / `COM3`) a klikněte na tlačítko *Připojit*.
4. Jakmile se rozsvítí zelený indikátor a načtou souřadnice, otevřete horní menu *Nástroje* -> *Diagnostika tiskárny* a klikněte na *Najetí domů (Bez kalibrace podložky) [G28 W]*. Tiskárna bezpečně zkalibruje koncové spínače.

== Krok 4: Načtení vzoru a konfigurace parametrů
1. Klikněte na *Soubor* -> *Načíst vzorek* (#kbd("Ctrl") + #kbd("O")) a vyberte připravený soubor SVG.
2. Zkontrolujte na plátně rozložení sklíček. Nastavte *Počet vzorků* (např. 4 sklíčka).
3. V levém panelu zkontrolujte:
   - *Sklo:* Laboratorní sklo (76 × 26 × 1 mm).
   - *Tryska:* Modrá (0.41 mm).
   - *Z-Offset:* Nastavte např. `180 µm` (výchozí laboratorní hodnota).
   - *Extruze:* Zadejte dávkovací poměr, např. `200 nl/mm`.
   - *Rychlost:* 600 mm/min.
   - *Odpliv:* Ujistěte se, že volba *Aktivovat odpliv* je zaškrtnuta!

== Krok 5: Demontáž sondy PINDA a optická Z-kalibrace
1. V levém panelu klikněte na zelené tlačítko *Spustit tisk*.
2. Tiskárna provede referenční homing všech os (`G28 W`) nad obnaženým ocelovým plátem.
3. *Kritický bezpečnostní krok – Demontáž sondy PINDA:*
   - Jakmile se hlava zastaví, na obrazovce se zobrazí modální výzva: _„Odstraňte PINDA sondu z podložky (tiskové hlavy), než bude zahájen tisk.“_
   - Imbusovým klíčem povolte upínací šroubek držáku sondy na tiskové hlavě a vysuňte sondu PINDA nahoru (nebo odklopte do servisní polohy), aby byl nejnižším bodem hlavy výhradně hrot dávkovací jehly.
   - V dialogu klikněte na *Pokračovat*.
4. Tiskárna najede tryskou nad roh prvního sklíčka do bezpečné výšky a otevře se okno *Přesná kalibrace skla (Z-Shift)* se živým obrazem z mikroskopu.
5. Zvolte krok `0.1 mm` a poté jemný `0.01 mm` (10 µm). Pomocí tlačítek na obrazovce nebo klávesy #kbd("↓") sjíždějte jehlou dolů.
6. Sledujte obraz: Jakmile se hrot jehly dotkne skla (hrot se spojí se svým zrcadlovým odrazem na povrchu skla), okamžitě zastavte posuv.
7. Klikněte na zelené tlačítko *Potvrdit a spustit tisk*. Vypočtená hodnota Z-Shift se uloží do tiskové úlohy.

== Krok 6: Dohled nad tiskem
1. Tiskárna sjede na levou stranu a provede odplivovou čistící linku na ocelovém plátu pro ustavení menisku.
2. Následně přejede na první sklíčko a začne nanášet vzor mikrokapek.
3. V průběhu tisku můžete na plátně sledovat zelenou tečku indikující přesnou polohu jehly a v levém panelu odečítat zbývající čas experimentu.
4. *V případě nouze:* Kdykoliv můžete stisknout červené tlačítko *Nouzové zastavení (STOP)* v levém panelu – tiskárna okamžitě zastaví veškerý posuv (#raw("M410")), bezpečně přizvedne trysku o 15 mm (#raw("G0 Z15")) a odjede na parkovací pozici mimo substráty.

== Krok 7: Vyjmutí vzorků a export protokolu
1. Po dokončení tiskárna zaparkuje hlavu vzadu a vysune podložku dopředu.
2. Opatrně pinzetou vyjměte potištěná sklíčka a přeneste je k sušení či analýze.
3. V menu DPI klikněte na *Soubor* -> *Uložit protokol (CSV)* a uložte záznam o šarži do složky projektu.

== Krok 8: Údržba a proplach soustavy po tisku
#warning-box(title: "Neponechávejte roztoky v jehle!")[
  Po skončení práce ihned vyjměte stříkačku a propláchněte dávkovací jehlu 2–3 ml čistého rozpouštědla (např. acetonem či ethanolem) a následně profoukněte vzduchem. Zaschlé činidlo v jehle o průměru 0.3 mm obvykle již nelze rozpustit a jehlu je nutné vyřadit.
]
