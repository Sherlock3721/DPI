# 💧 Droplet Printing Interface (DPI)

**Droplet Printing Interface (DPI)** je specializovaná, lokálně běžící open-source aplikace navržená speciálně pro **chemiky, výzkumníky a laboratorní techniky**. Umožňuje extrémně přesné dávkování kapalin a mikrotisk na podložní sklíčka pomocí modifikovaných 3D tiskáren a stříkačkových dávkovačů.

---

## 🧪 Pro Chemiky a Výzkumníky

Vaše laboratoř vyžaduje preciznost a spolehlivost. Na rozdíl od běžných 3D tiskových programů (které jsou dimenzované pro tavení plastů), se DPI soustředí výhradně na plynulou manipulaci s kapalinami bez nežádoucího zahřívání trysek.

### Hlavní funkce pro laboratoř:
- 🔬 **Vizuální kontrola v reálném čase:** Integrovaná podpora pro USB a vestavěné mikroskopické kamery. Skrze uživatelské rozhraní můžete sledovat proces dávkování, rotovat nebo zrcadlit obraz.
- 📐 **Přesné dávkování (G-kód & Vektory):** Nahrajte váš vlastní G-kód, nebo naimportujte standardní vektorovou grafiku (SVG/DXF) pro tisk složitých vzorů a plošných polí mikrokapek.
- 🛠 **Diagnostika a čištění:** Dedikované nástroje pro údržbu vašich pump a trysek (např. rychlé pročištění stříkačky, testovací extruze).
- 🔒 **Bezpečné a lokální:** Aplikace nevyžaduje připojení k internetu a veškerá vaše experimentální data zůstávají pouze na vašem zařízení.
- 💬 **Přímá zpětná vazba:** Integrovaný systém pro hlášení chyb a nápadů přímo vývojářům.

---

## 💻 Pro IT a Vývojáře (Technická specifikace)

DPI je postaveno na moderním technologickém stacku spojujícím rychlý a bezpečný backend v jazyce **Rust** s plynulým, dynamickým frontendem vytvořeným pomocí **Svelte** a **TailwindCSS**, vše zabalené pomocí **Tauri v2**.

### Architektura a Technologie

| Komponenta         | Technologie              | Popis a využití |
|--------------------|--------------------------|-----------------|
| **Framework**      | Tauri v2                 | Zajišťuje bezpečné spuštění aplikace s minimální zátěží na operační systém, menší velikost binárek a integraci systémových API. |
| **Backend (Core)** | Rust                     | Zajišťuje nízkoúrovňovou komunikaci se sériovými porty (`std::sync::mpsc`), zpracování a parsing G-kódu bez záseků. |
| **Frontend**       | Svelte 4                 | Reaktivní uživatelské rozhraní, rychlé překreslování stavu tiskárny a nulový overhead v produkci. |
| **Styling**        | TailwindCSS              | Responzivní komponenty, laboratorní "Dark Mode" estetika. |
| **Build System**   | Vite                     | Rychlý Hot-Module-Replacement (HMR) pro vývoj frontendu. |
| **Komunikace**     | Sériová linka (Serial)   | Plně asynchronní obousměrná komunikace s firmwarem tiskárny (Marlin, apod.) v dedikovaném vlákně. |

### Instalace a Sestavení ze zdrojových kódů

Pro lokální kompilaci budete potřebovat následující nástroje:
1. **[Node.js](https://nodejs.org/en/)** (verze 20 a vyšší)
2. **[Rust Toolchain](https://rustup.rs/)**

#### Prerekvizity pro Linux (Ubuntu/Debian)
Před prvním spuštěním je nutné nainstalovat systémové závislosti (WebKit2GTK a nástroje pro kompilaci):
```bash
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libudev-dev
```

#### Lokální spuštění (Vývojový mód)
```bash
# 1. Instalace frontendových závislostí
npm install

# 2. Spuštění v dev módu (s Hot-Reloadingem frontendu)
npm run dev
```

#### Sestavení produkční verze (Build)
Pro sestavení aplikace pro váš aktuální systém:
```bash
npm run tauri build
```
Pro sestavení Windows binárky (Cross-compile z Linuxu):
```bash
npm run tauri build -- --target x86_64-pc-windows-gnu --no-bundle
```

---

## 📜 Licence
Tento projekt je uvolněn pod svobodnou licencí **GPLv2** (GNU General Public License v2.0). Můžete jej volně používat, upravovat i šířit, ovšem veškeré odvozené verze musí zůstat open-source pod stejnou licencí. Plné znění naleznete v souboru `LICENSE`.
