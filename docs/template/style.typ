// ==============================================================================
// Šablona stylu pro dokumentaci Droplet Printing Interface (DPI)
// ==============================================================================

#let primary-color = rgb("#1d4ed8")      // Laboratorní tmavě modrá
#let accent-color = rgb("#3b82f6")       // DPI akcentní modrá
#let secondary-accent = rgb("#f97316")   // Oranžová pro akcenty a callouty
#let success-color = rgb("#10b981")     // Zelená pro úspěch / potvrzení
#let warning-color = rgb("#f59e0b")     // Jantarová pro varování
#let danger-color = rgb("#ef4444")      // Červená pro nebezpečí / STOP
#let bg-dark = rgb("#0f172a")           // Tmavý podklad
#let bg-card = rgb("#1e293b")           // Karta v tmavém režimu
#let text-primary = rgb("#1e293b")      // Hlavní text pro tisk
#let text-muted = rgb("#64748b")        // Tlumený text
#let border-color = rgb("#e2e8f0")      // Jemný rámeček

// Načtení externího CSS stylopisu pro HTML výstup
#let html-css = read("manual.css")

// Pomocná detekce HTML cíle
#let is-html() = ("target" in dictionary(std) and std.target() == "html")

// Pomocná funkce pro extrakci čistého textu z Typst prvků
#let get-text(c) = {
  if type(c) == str {
    c
  } else if type(c) == content {
    if c.has("text") {
      c.text
    } else if c.has("body") {
      get-text(c.body)
    } else if c.has("children") {
      c.children.map(get-text).join("")
    } else {
      ""
    }
  } else {
    str(c)
  }
}

// Generátor URL kotev z textu nadpisů pro HTML prokliky
#let make-slug(c) = {
  let txt = get-text(c)
  txt.trim()
    .replace(" ", "-")
    .replace(".", "-")
    .replace(":", "")
    .replace("?", "")
    .replace("(", "")
    .replace(")", "")
    .replace("/", "-")
}

// Převodník matematických prvků do standardního W3C HTML5 MathML
#let to-mathml(c) = {
  if type(c) == str {
    html.elem("mtext")[#c]
  } else if type(c) == content {
    let f = repr(c.func())
    if c.has("text") {
      let t = c.text
      if t.match(regex("^[0-9]+(\.[0-9]+)?$")) != none {
        html.elem("mn")[#t]
      } else if t in ("=", "+", "-", "−", "×", "·", "≈", "→", "±", "≤", "≥", "/", ",", ";", "|", "(", ")", "[", "]", "approx", "times", "degree", "plus.minus", "diameter", "arrow.r") {
        let sym = if t == "approx" { "≈" }
          else if t == "times" { "×" }
          else if t == "plus.minus" { "±" }
          else if t == "degree" { "°" }
          else if t == "diameter" { "⌀" }
          else if t == "arrow.r" { "→" }
          else { t }
        html.elem("mo")[#sym]
      } else if t.match(regex("^[a-zA-Zα-ωΑ-Ω]$")) != none or t in ("π", "Δ", "pi") {
        let sym = if t == "pi" { "π" } else { t }
        html.elem("mi")[#sym]
      } else {
        html.elem("mtext")[#t]
      }
    } else if f == "space" or f == "h" or c.has("amount") {
      html.elem("mspace", attrs: (width: "0.22em"))
    } else if f == "frac" {
      html.elem("mfrac")[
        #html.elem("mrow")[#to-mathml(c.num)]
        #html.elem("mrow")[#to-mathml(c.denom)]
      ]
    } else if f == "attach" {
      let base = if c.has("base") { to-mathml(c.base) } else { html.elem("mrow") }
      let has-sub = c.has("b") or c.has("bottom")
      let has-sup = c.has("t") or c.has("top")
      let sub-node = if c.has("b") { to-mathml(c.b) } else if c.has("bottom") { to-mathml(c.bottom) } else { none }
      let sup-node = if c.has("t") { to-mathml(c.t) } else if c.has("top") { to-mathml(c.top) } else { none }
      
      if has-sub and has-sup {
        html.elem("msubsup")[#base #sub-node #sup-node]
      } else if has-sub {
        html.elem("msub")[#base #sub-node]
      } else if has-sup {
        html.elem("msup")[#base #sup-node]
      } else {
        base
      }
    } else if f == "lr" or f == "delimited" {
      html.elem("mrow")[
        #to-mathml(c.body)
      ]
    } else if c.has("body") {
      to-mathml(c.body)
    } else if c.has("children") {
      html.elem("mrow")[
        #for ch in c.children {
          to-mathml(ch)
        }
      ]
    } else {
      html.elem("mtext")[#repr(c)]
    }
  } else {
    html.elem("mn")[#str(c)]
  }
}

#let dpi-doc(
  title: "Droplet Printing Interface (DPI)",
  subtitle: "Komplexní uživatelská a technická příručka pro laboratorní mikrodávkování",
  version: "2.2.4",
  date: "Září 2026",
  author: "Cyril Veverka",
  email: "cyrilveverka@email.cz",
  body
) = {
  // Nastavení dokumentu a stránky
  let doc-author = if email != "" { author + " <" + email + ">" } else { author }
  set document(title: title, author: doc-author)
  set text(
    font: ("Adwaita Sans", "FreeSans", "Nimbus Sans", "Cantarell"),
    size: 10pt,
    lang: "cs",
    fill: text-primary,
  )

  // Ošetření exportu do HTML (potlačení pagebreaku a v-spacing v tokovém HTML)
  show pagebreak: it => context {
    if is-html() {
      none
    } else {
      it
    }
  }

  show v: it => context {
    if is-html() {
      none
    } else {
      it
    }
  }

  // Zobrazení obrázků v HTML pomocí responzivního <img> se správnou relativní adresou
  show image: it => context {
    if is-html() {
      let src = it.source.replace("../assets/", "assets/")
      html.elem("div", attrs: (class: "dpi-figure-img-wrap"))[
        #html.elem("img", attrs: (
          src: src,
          alt: if it.alt != none { it.alt } else { "Schéma DPI" },
          class: "dpi-figure-img"
        ))
      ]
    } else {
      it
    }
  }

  // Převod matematických rovnic do nativního HTML5 MathML zobrazení
  show math.equation: it => context {
    if is-html() {
      let disp = if it.block { "block" } else { "inline" }
      let cls = if it.block { "dpi-math-block" } else { "dpi-math-inline" }
      html.elem("span", attrs: (class: cls))[
        #html.elem("math", attrs: (display: disp))[
          #to-mathml(it.body)
        ]
      ]
    } else {
      it
    }
  }

  // Injekce webového CSS stylopisu v HTML režimu
  context {
    if is-html() {
      html.elem("style", html-css)
    }
  }

  // Tiskový layout stránky pro PDF
  show: it => context {
    if is-html() {
      it
    } else {
      set page(
        paper: "a4",
        margin: (top: 2.5cm, bottom: 2.5cm, left: 2.5cm, right: 2.5cm),
        header: context {
          let page-num = here().page()
          // Nezobrazovat záhlaví na titulní straně
          if page-num > 1 {
            grid(
              columns: (1fr, auto),
              align: (left + bottom, right + bottom),
              text(size: 8.5pt, fill: text-muted, weight: "bold", [💧 DPI — Droplet Printing Interface]),
              text(size: 8.5pt, fill: text-muted, [v#version | Manuál pro Prusa MK3S])
            )
            line(length: 100%, stroke: 0.5pt + border-color)
          }
        },
        footer: context {
          let page-num = here().page()
          if page-num > 1 {
            line(length: 100%, stroke: 0.5pt + border-color)
            v(2mm)
            grid(
              columns: (1fr, auto),
              align: (left, right),
              text(size: 8pt, fill: text-muted, [Droplet Printing Interface — Uživatelský manuál a příručka laboranta]),
              text(size: 8.5pt, fill: text-primary, weight: "bold", counter(page).display("1 / 1", both: true))
            )
          }
        }
      )
      it
    }
  }

  // Formátování odstavců
  set par(
    justify: true,
    leading: 0.75em,
    spacing: 1.2em,
    first-line-indent: 0pt,
  )

  // Formátování nadpisů
  show heading.where(level: 1): it => context {
    if is-html() {
      let s = make-slug(it.body)
      html.elem("h2", attrs: (id: s, class: "dpi-h1"))[#it.body]
    } else {
      block(
        width: 100%,
        stroke: (bottom: 2pt + accent-color),
        inset: (bottom: 0.4em),
        above: 2.2em,
        below: 1.2em,
        text(fill: primary-color, weight: "bold", size: 18pt, it.body)
      )
    }
  }

  show heading.where(level: 2): it => context {
    if is-html() {
      let s = make-slug(it.body)
      html.elem("h3", attrs: (id: s, class: "dpi-h2"))[#it.body]
    } else {
      block(
        above: 1.8em,
        below: 0.9em,
        text(fill: accent-color, weight: "bold", size: 13pt, it.body)
      )
    }
  }

  show heading.where(level: 3): it => context {
    if is-html() {
      let s = make-slug(it.body)
      html.elem("h4", attrs: (id: s, class: "dpi-h3"))[#it.body]
    } else {
      block(
        above: 1.4em,
        below: 0.6em,
        text(fill: rgb("#334155"), weight: "bold", size: 11pt, it.body)
      )
    }
  }

  // Přizpůsobení položek obsahu pro HTML režim (klikatelné kotvy navigace)
  show outline.entry: it => context {
    if is-html() {
      let lvl = it.level
      let elem = it.element
      let s = make-slug(elem.body)
      html.elem("div", attrs: (class: "dpi-toc-item lvl-" + str(lvl)))[
        #html.elem("a", attrs: (href: "#" + s, class: "dpi-toc-link"))[#elem.body]
      ]
    } else {
      it
    }
  }

  // Formátování bloků kódu
  show raw.where(block: true): it => block(
    fill: rgb("#f8fafc"),
    inset: 10pt,
    radius: 5pt,
    stroke: 1pt + rgb("#e2e8f0"),
    width: 100%,
    text(font: ("JetBrainsMono NF", "FreeMono", "DejaVu Sans Mono"), size: 8.5pt, it)
  )

  show raw.where(block: false): it => box(
    fill: rgb("#f1f5f9"),
    inset: (x: 4pt, y: 1.5pt),
    radius: 3pt,
    baseline: 0pt,
    text(font: ("JetBrainsMono NF", "FreeMono", "DejaVu Sans Mono"), size: 8.5pt, fill: rgb("#0f172a"), it)
  )

  // Formátování odkazů
  show link: it => underline(text(fill: primary-color, it))

  // Formátování tabulek
  set table(
    stroke: (x, y) => if y == 0 { (bottom: 1.5pt + primary-color) } else { 0.5pt + rgb("#e2e8f0") },
    fill: (col, row) => if row == 0 { rgb("#eff6ff") } else if calc.even(row) { rgb("#f8fafc") } else { none },
    inset: (x: 8pt, y: 6pt),
  )

  // ============================================================================
  // Titulní část, obsah a tělo (HTML web vs. PDF tisk)
  // ============================================================================
  context {
    if is-html() {
      html.elem("main", attrs: (class: "dpi-container"))[
        #html.elem("header", attrs: (class: "dpi-hero"))[
          #html.elem("div", attrs: (class: "dpi-hero-pill"))[💧 OPEN-SOURCE MIKRODÁVKOVACÍ ROZHRANÍ PRO 3D TISKÁRNY]
          #html.elem("h1", attrs: (class: "dpi-hero-title"))[#title]
          #html.elem("p", attrs: (class: "dpi-hero-subtitle"))[#subtitle]
          #html.elem("div", attrs: (class: "dpi-author-card"))[
            #html.elem("div", attrs: (class: "dpi-author-name"))[Autor: #author]
            #if email != "" [
              #html.elem("a", attrs: (href: "mailto:" + email, class: "dpi-author-email"))[#email]
            ]
          ]
          #html.elem("div", attrs: (class: "dpi-hero-cards"))[
            #html.elem("div", attrs: (class: "dpi-hero-card"))[
              #html.elem("div", attrs: (class: "dpi-card-label"))[Referenční hardware]
              #html.elem("div", attrs: (class: "dpi-card-val"))[Prusa i3 MK3S / Marlin]
              #html.elem("div", attrs: (class: "dpi-card-sub"))[Stříkačkový dávkovač & mikroskop]
            ]
            #html.elem("div", attrs: (class: "dpi-hero-card"))[
              #html.elem("div", attrs: (class: "dpi-card-label"))[Řídicí aplikace]
              #html.elem("div", attrs: (class: "dpi-card-val"))[DPI Desktop (Linux & Windows)]
              #html.elem("div", attrs: (class: "dpi-card-sub"))[Tauri v2 + Svelte 5 rozhraní]
            ]
          ]
          #html.elem("div", attrs: (class: "dpi-hero-meta"))[
            #html.elem("span", attrs: (class: "dpi-pill"))[Verze #version]
            #html.elem("span", attrs: (class: "dpi-pill"))[#date]
            #html.elem("span", attrs: (class: "dpi-pill"))[Licence GPLv2 / Komerční duální licence]
          ]
        ]
        #html.elem("nav", attrs: (class: "dpi-toc"))[
          #html.elem("h2", attrs: (class: "dpi-toc-title"))[Obsah manuálu]
          #outline(
            title: none,
            indent: auto,
            depth: 3,
          )
        ]
        #body
      ]
    } else {
      align(center + top)[
        #v(2.5cm)
        #block(
          width: 100%,
          fill: rgb("#f0fdf4"),
          stroke: 1.5pt + rgb("#bbf7d0"),
          radius: 8pt,
          inset: 12pt,
          [
            #text(size: 11pt, weight: "bold", fill: rgb("#166534"), [💧 OPEN-SOURCE MIKRODÁVKOVACÍ ROZHRANÍ PRO 3D TISKÁRNY])
          ]
        )
        #v(1.2cm)
        #text(size: 26pt, weight: "bold", fill: primary-color, title)
        #v(0.5cm)
        #text(size: 13pt, fill: rgb("#475569"), subtitle)
        #v(0.8cm)
        #block(
          fill: rgb("#f8fafc"),
          inset: (x: 16pt, y: 10pt),
          radius: 6pt,
          stroke: 1pt + rgb("#e2e8f0"),
          [
            #text(size: 11pt, weight: "bold", fill: text-primary, [Autor: #author]) \
            #if email != "" [
              #v(2pt)
              #text(size: 10pt, fill: primary-color, link("mailto:" + email)[#email])
            ]
          ]
        )
        #v(0.8cm)
        #line(length: 40%, stroke: 2pt + accent-color)
        #v(0.8cm)

        #grid(
          columns: (1fr, 1fr),
          align: (center, center),
          [
            #text(weight: "bold", size: 10pt, [Referenční hardware:]) \
            #text(fill: text-primary, [Prusa i3 MK3S / Marlin]) \
            #text(size: 8.5pt, fill: text-muted, [Stříkačkový dávkovač & mikroskop])
          ],
          [
            #text(weight: "bold", size: 10pt, [Řídicí aplikace:]) \
            #text(fill: text-primary, [DPI Desktop (Linux & Windows)]) \
            #text(size: 8.5pt, fill: text-muted, [Tauri v2 + Svelte 5 rozhraní])
          ]
        )

        #v(3.0cm)
        #align(center + bottom)[
          #text(size: 9.5pt, fill: text-muted, [Verze dokumentu #version | #date]) \
          #v(2mm)
          #text(size: 9pt, fill: rgb("#94a3b8"), [Licence GPLv2 / Komerční duální licence])
        ]
      ]
      pagebreak()

      outline(
        title: [Obsah manuálu],
        indent: auto,
        depth: 3,
      )
      pagebreak()

      body
    }
  }
}
