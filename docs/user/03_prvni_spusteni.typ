#import "../template/components.typ": note-box, tip-box, important-box, warning-box, badge, callout-table, kbd

= 3. Připojení a první spuštění

Tato kapitola vás provede spuštěním aplikace, navázáním obousměrné komunikace s tiskárnou Prusa MK3S a ověřením základních diagnostických funkcí.

== 3.1 Spuštění aplikace
DPI je dodáváno jako nativní binární balíček:
- *Linux:* Spusťte spustitelný soubor `dpi` nebo nainstalujte balíček `.deb` / `.rpm` / AppImage.
- *Windows:* Spusťte instalátor `DPI_Setup.exe`, který vytvoří zástupce na ploše a v nabídce Start.

#note-box(title: "Oprávnění k sériovému portu na Linuxu")[
  Pokud se po spuštění na Linuxu nezobrazují žádné sériové porty, ujistěte se, že váš uživatelský účet je členem skupiny `dialout` (Ubuntu/Debian) nebo `uucp` (Arch/Manjaro):
  ```bash
  sudo usermod -a -G dialout $USER
  ```
  Po přidání do skupiny je nutné se odhlásit a znovu přihlásit do systému.
]

== 3.2 Připojení k tiskárně (Sériová linka USB)
Tiskárna Prusa MK3S se k řídícímu počítači připojuje standardním kabelem USB typu B.

V levém horním rohu aplikace (Levý panel) se nachází sekce *Připojení k tiskárně*:
1. *Výběr portu:*
   - Možnost *"Automaticky"* (doporučeno): Aplikace postupně projde všechny dostupné porty a odešle příkaz `M115`. První port, který odpoví validním identifikačním řetězcem firmwaru Marlin, je automaticky připojen.
   - *Manuální výběr:* Vyberte konkrétní port z rozbalovacího seznamu (např. `/dev/ttyACM0` na Linuxu nebo `COM3` na Windows).
2. *Baudrate:* Výchozí přenosová rychlost pro firmware Marlin na Prusa tiskárnách je *115200 Bd*.
3. *Tlačítko Připojit:* Klikněte na tlačítko se symbolem řetězu. Po úspěšném navázání spojení se ikona zbarví do zelena, tlačítko zobrazí *"Odpojit"* a v telemetrickém bloku pod ním se začnou živě aktualizovat souřadnice X, Y, Z a teploty.

== 3.3 Diagnostika tiskárny a ověření spojení
Před zahájením práce se doporučuje ověřit základní funkce tiskárny prostřednictvím okna *Diagnostika tiskárny* (dostupné z horního menu: *Nástroje* -> *Diagnostika tiskárny* nebo kliknutím na stavový řádek):

#align(center)[
  #image("../assets/figures/fig06_diagnostics_modal.svg", width: 95%)
  #v(-4pt)
  #text(size: 8.5pt, fill: rgb("#64748b"), [Obrázek 3.1: Diagnostické okno s rychlými příkazy a terminálem])
]

#callout-table(
  [#badge("1")], [Informace a stav (M115, M119, M114)], [Ověření verze firmware (#raw("M115")), kontrola stavu koncových spínačů (#raw("M119")) a zjištění aktuální fyzické polohy (#raw("M114")).],
  [#badge("2")], [Pohyb a dávkování (G28, G92, M84)], [Základní homing všech os (#raw("G28")), rychlý homing bez měření sítě (#raw("G28 W")), vynulování extruderu po výměně stříkačky (#raw("G92 E0")) a uvolnění krokových motorů (#raw("M84")).],
  [#badge("3")], [Teplota a chlazení (M140, M106)], [Okamžité a bezpečné vypnutí výhřevu podložky (#raw("M140 S0")) a spuštění ofukového větráčku pro urychlení odparu rozpouštědla (#raw("M106 S255")).],
  [#badge("4")], [Interaktivní terminál G-kódu], [Přímé odesílání vlastních G-kódových příkazů do tiskárny s okamžitým výpisem odpovědí firmware a historií příkazů.],
)

#tip-box(title: "Uvolnění motorů při čištění jehly")[
  Při manuální výměně stříkačky nebo čištění dávkovací jehly klikněte na tlačítko *Uvolnit motory (M84)*. Tím se odpojí napájení krokových motorů a tiskovou hlavou lze rukou lehce pohybovat bez namáhání mechaniky.
]

== 3.4 Uvítací obrazovka a vzdálený mobilní náhled (QR Kód)
Při prvním spuštění (nebo z horního menu *Nápověda* -> *Vítejte v DPI*) se otevírá uvítací centrum aplikace:
- *Rychlý start a nedávné projekty:* Umožňuje jedním kliknutím otevřít naposledy zpracovávané experimenty s přehledem data a času jejich uložení.
- *Mobilní laboratorní náhled (QR kód):*
  - DPI automaticky zjistí lokální IP adresu počítače v síti a vygeneruje unikátní QR kód.
  - Naskenováním tohoto kódu fotoaparátem mobilního telefonu nebo tabletu připojeného na stejnou laboratorní Wi-Fi síť se otevře webové rozhraní aplikace (`http://<ip>:5173`).
  - Webové rozhraní v reálném čase přebírá telemetrii z tiskárny (aktuální souřadnice, teploty podložky, stav dávkování), takže laborant může dohlížet na průběh experimentu z kteréhokoliv koutu laboratoře.

== 3.5 Kontrola a instalace aktualizací aplikace
DPI obsahuje integrovanou kontrolu aktualizací:
1. *Automatická kontrola:* Krátce po spuštění aplikace automaticky ověří dostupnost novější verze. Pokud je nalezena nová verze, zobrazí se dialog s přehledem změn a možností instalace.
2. *Ruční kontrola:* Aktualizaci můžete kdykoliv zkontrolovat v horním menu *Nápověda* -> *Zkontrolovat aktualizace*.

== 3.6 Odesílání laboratorní zpětné vazby (Feedback)
Přímo z aplikace lze komunikovat s vývojovým týmem:
- V horním menu zvolte *Nápověda* -> *Odeslat zpětnou vazbu*.
- Otevře se formulář, kam můžete popsat své postřehy, navrhnout novou funkci nebo nahlásit nestandardní chování. Zpráva je bezpečně uložena a přeposlána přes šifrovaný protokol vývojářům.
