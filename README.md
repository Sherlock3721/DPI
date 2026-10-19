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
| **Frontend**       | Svelte 5                 | Reaktivní uživatelské rozhraní, rychlé překreslování stavu tiskárny a nulový overhead v produkci. |
| **Styling**        | TailwindCSS              | Responzivní komponenty, laboratorní "Dark Mode" estetika. |
| **Build System**   | Vite                     | Rychlý Hot-Module-Replacement (HMR) pro vývoj frontendu. |
| **Komunikace**     | Sériová linka (Serial)   | Plně asynchronní obousměrná komunikace s firmwarem tiskárny (Marlin, apod.) v dedikovaném vlákně. |

### Instalace a Sestavení ze zdrojových kódů

Pro lokální kompilaci budete potřebovat následující nástroje:
1. **[Node.js](https://nodejs.org/en/)** (verze 20 a vyšší) a **npm**
2. **[Rust Toolchain](https://rustup.rs/)** (`rustc`, `cargo`)
3. **[wasm-pack](https://rustwasm.github.io/wasm-pack/)** (pro kompilaci výpočetního jádra `dpi-core` do WebAssembly)

---

#### 📦 Systémové prerekvizity pro Linux

Před prvním spuštěním je nutné nainstalovat systémové závislosti (WebKit2GTK a nástroje pro kompilaci):

##### Arch Linux / Manjaro
```bash
sudo pacman -S --needed base-devel webkit2gtk-4.1 libappindicator-gtk3 librsvg wasm-pack nodejs npm rust
```

##### Ubuntu / Debian
```bash
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libudev-dev build-essential
# Instalace wasm-pack:
cargo install wasm-pack
```

##### Fedora
```bash
sudo dnf install webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel
cargo install wasm-pack
```

---

#### 🚀 Lokální spuštění (Vývojový mód)

Projekt využívá **Tauri v2**, který spojuje Rust backend s frontendem ve Svelte.

> [!IMPORTANT]
> **Pro spuštění desktopové aplikace vždy použijte `npm run tauri dev`.**
> Samotný příkaz `cargo run` nespustí Vite dev server a okno aplikace by zůstalo skryté.

```bash
# 1. Klonování repozitáře a přechod do složky
git clone https://github.com/Sherlock3721/DPI.git
cd DPI

# 2. Instalace frontendových závislostí
npm install

# 3. Spuštění aplikace v dev módu (automaticky zkompiluje WASM a spustí frontend i backend)
npm run tauri dev
```

> [!TIP]
> **Samostatné spuštění pouze webového rozhraní:**
> Pokud chcete ladit čistě UI v prohlížeči (bez napojení na sériové porty a 3D tiskárnu), stačí spustit:
> ```bash
> npm run dev
> ```
> a otevřít `http://localhost:5173`.

---

#### 🔨 Sestavení produkční verze (Build)

Pro sestavení desktopové aplikace pro váš aktuální systém (výsledné instalační balíčky a binárka vzniknou v `src-tauri/target/release/bundle/`):
```bash
npm run tauri build
```

Pro sestavení Windows binárky (Cross-compile z Linuxu):
```bash
npm run tauri build -- --target x86_64-pc-windows-gnu --no-bundle
```

---

#### 💡 Řešení častých problémů na Linuxu

- **Chyba protokolu Wayland (`Error 71 dispatching to Wayland display`):**
  Aplikace automaticky ošetřuje renderer nastavením `WEBKIT_DISABLE_DMABUF_RENDERER=1`. Pokud by na některých kompozitorech (KDE Wayland / Hyprland s NVIDIA kartami) okno stále spadlo, spusťte aplikaci s:
  ```bash
  WEBKIT_DISABLE_DMABUF_RENDERER=1 npm run tauri dev
  # nebo při potížích s hardwarovou akcelerací:
  WEBKIT_DISABLE_COMPOSITING_MODE=1 npm run tauri dev
  ```
- **Chyba `wasm-pack nenalezen`:**
  Ujistěte se, že máte nainstalovaný `wasm-pack` (`sudo pacman -S wasm-pack` na Archu nebo `cargo install wasm-pack`) a že máte složku `~/.cargo/bin` nebo `~/.local/bin` přidanou v proměnné `$PATH`.

---

## 📜 Licence

DPI je nabízeno pod **dvojí licencí** — zvolte si tu, která vyhovuje vašemu použití:

| Licence | Pro koho | Podmínky |
|---------|----------|----------|
| **GPLv2** (zdarma) | Open-source projekty, interní laboratorní použití, výzkum | Software smíte volně používat, upravovat i šířit, ovšem veškerá odvozená díla musí zůstat open-source pod toutéž licencí. Plné znění: [`LICENSE`](LICENSE). |
| **Komerční licence** (placená) | Uzavřené (proprietární) produkty, distribuce bez zveřejnění zdrojového kódu, nárok na podporu/SLA | Zbavuje vás copyleftových povinností GPLv2. Podmínky: [`LICENSE-COMMERCIAL.md`](LICENSE-COMMERCIAL.md). |

Pokud vám podmínky GPLv2 vyhovují (typicky provoz Softwaru ve vlastní laboratoři bez další distribuce), **žádnou komerční licenci nepotřebujete**. Komerční licence je určena subjektům, které chtějí Software začlenit do proprietárního produktu nebo potřebují záruky a podporu nad rámec open-source licence.

Zájem o komerční licenci? Kontaktujte **cyrilveverka@email.cz**.
