#import "../template/components.typ": note-box, tip-box, important-box, warning-box, badge, callout-table, kbd

= 6. Kalibrace, Z-Shift a tiskový proces

Přesnost v ose Z je u dávkování kapalin naprosto kritická. Odchylka pouhých 50 mikrometrů může způsobit buď rozstříknutí kapky a nechtěné rozlití do stran, nebo naopak to, že kapka neukápne z jehly a zůstane viset na hrotu vlivem povrchového napětí.

== 6.1 Princip výškového nastavení (Z-Shift)
Dávkovací jehla upevněná ve stříkačkovém držáku sahá níže než původní tryska tiskárny a skleněný substrát má určitou tloušťku.

#align(center)[
  #image("../assets/figures/fig09_z_shift_principle.svg", width: 95%)
  #v(-4pt)
  #text(size: 8.5pt, fill: rgb("#64748b"), [Obrázek 6.1: Vzájemná geometrie tiskové hlavy, jehly a povrchu skla])
]

Aplikace DPI automaticky provádí přepočet souřadnic (tzv. *Z-Shift*), aby se tiskárna mohla bezpečně pohybovat nad sklem bez kolizí a bez blokování koncovými spínači. Skutečná tisková výška je pak dána součtem zkalibrované polohy povrchu skla a procesního Z-offsetu (např. 180 µm nad sklem pro stabilní meniskus kapky).

Obsluha nemusí provádět žádné ruční výpočty – veškeré přesné doladění probíhá interaktivně v kalibračním okně pomocí kamery.

== 6.2 Interaktivní optická kalibrace (Z-Calibration Modal)
Před spuštěním první tiskové úlohy (nebo po každé fyzické výměně jehly/stříkačky) se automaticky otevírá okno *Přesná kalibrace skla (Z-Shift)*:

1. Tiskárna automaticky provede homing (`G28 W`) a najede hrotem jehly nad bezpečný referenční roh prvního podložního sklíčka do bezpečné výšky.
2. V okně kalibrace se zobrazí živý video stream z USB mikroskopu namířeného na rozhraní jehla-sklo.
3. *Jemný posuv:*
   - Zvolte krok posunu: `0.5 mm`, `0.1 mm`, `0.05 mm` nebo ultra-jemný `0.01 mm` (10 µm).
   - Pomocí tlačítek na obrazovce nebo kláves #kbd("↑") / #kbd("↓") sjíždějte jehlou dolů.
   - Na obrazovce mikroskopu sledujte dotyk hrotu jehly (nebo jeho odrazu na lesklém povrchu skla). V momentě, kdy se hrot jehly právě dotkne povrchu skla, je dosaženo nulové relativní výšky.
4. Klikněte na zelené tlačítko *Potvrdit a spustit tisk*. Vypočtená hodnota korekce `z_shift` se uloží do paměti a aplikuje na celou tiskovou úlohu.

#tip-box(title: "Z-Hop pro bezpečné přejezdy")[
  Během tisku tiskárna při každém přejezdu mezi jednotlivými kapkami nebo sklíčky automaticky nadzvedne trysku o hodnotu `default_z_hop` (výchozí 2.0 mm) a při přejezdu celé podložky na bezpečnou výšku `safe_z` (20.0 mm), aby nedošlo k poškrábání vzorků.
]

== 6.3 Nivelace podložky (Bed Leveling) a testovací jízda bodů
Pokud má vyhřívaná podložka tiskárny drobné mechanické nerovnosti, lze v konfiguraci aktivovat automatickou nivelaci sítě:
- V záložce *Nastavení* -> *Nivelace* je definována síť bodů (výchozí matice 3 × 3 = 9 bodů).
- Všechny tyto body musí ležet na obnaženém ocelovém plátu mimo prostor držáků sklíček. V náhledu je každý bod znázorněn kruhovým terčíkem o nastavitelném průměru `leveling_circle_diameter` (výchozí 8.0 mm).
- *Optimalizace přejezdů:* Body jsou automaticky seřazeny meandrovitě (střídavě zleva doprava a zprava doleva), aby byl přejezd hlavy co nejrychlejší a nejplynulejší.

=== Interaktivní ověření bodů na tiskárně (Test bodů)
Pro naprostou laboratorní jistotu, že sonda PINDA při nivelaci nezasáhne sklo, obsahuje DPI interaktivní testovací režim:
1. V záložce *Nivelace* klikněte na tlačítko *Otestovat body na tiskárně*.
2. Aplikace zobrazí bezpečnostní upozornění a po potvrzení se automaticky připojí k tiskárně.
3. Tiskárna provede rychlý homing (`G28 W`) a následně postupně najíždí sondou nad jednotlivé definované body (`G0 X... Y... F3000`).
4. Laborant vizuálně zkontroluje, že se hrot sondy nachází bezpečně nad ocelovým plechem s dostatečnou vzdáleností od hrany plastového držáku skla.
5. Pokud je bod příliš blízko držáku, lze jeho souřadnice přímo v okně testu okamžitě jemně posunout a přeměřit.

== 6.4 Průběh tisku a správa plánovaných pauz (Print Pause)
Po spuštění tisku DPI zobrazuje:
- Průběžný ukazatel v procentech a zbývající čas v minutách a sekundách.
- Animovanou aktuální polohu trysky přímo na plátně.

=== Obsluha plánovaných pauz (M0 / M1 / M601)
Pokud tisková úloha obsahuje více vrstev, vyžaduje výměnu činidla nebo jehly:
1. Tiskárna dokončí stávající segment, zvedne trysku na bezpečnou výšku a zaparkuje na kraji podložky.
2. Na obrazovce se otevře modální okno *Pauza tisku (Požadavek obsluhy)* s popisem kroku.
3. Obsluha provede potřebný laboratorní úkon (např. nasazení nové stříkačky s dalším činidlem).
4. Kliknutím na *Pokračovat v tisku* tiskárna automaticky provede příkaz definovaný v `nozzle_change_gcode` (výchozí bezpečný homing `G28`) a plynule naváže přesně v místě přerušení.

== 6.5 Export laboratorního protokolu (CSV)
Pro potřeby vědecké archivace a laboratorních deníků umožňuje DPI vygenerovat detailní strukturovaný CSV protokol:
- V horním menu zvolte *Soubor* -> *Uložit protokol (CSV)*.
- Protokol obsahuje: datum a čas experimentu, verzi DPI, typ použitého skla a trysek, celkovou ujetou vzdálenost, tiskový čas a přehlednou tabulku všech vzorků včetně jejich individuálních Z-offsetů a dávkovacích objemů.
