// ==============================================================================
// Znovupoužitelné komponenty pro DPI dokumentaci (PDF i Web HTML)
// ==============================================================================

#let accent-blue = rgb("#3b82f6")
#let accent-orange = rgb("#f97316")
#let alert-green = rgb("#10b981")
#let alert-yellow = rgb("#f59e0b")
#let alert-red = rgb("#ef4444")

#let is-html() = ("target" in dictionary(std) and std.target() == "html")

// Kruhová číslovaná visačka (odpovídající SVG screenshotům)
#let badge(num, color: accent-blue) = context {
  if is-html() {
    let col-hex = if type(color) == color { color.to-hex() } else { "#3b82f6" }
    html.elem("span", attrs: (class: "dpi-badge", style: "background-color: " + col-hex + ";"))[#str(num)]
  } else {
    box(
      baseline: 15%,
      circle(
        radius: 7pt,
        fill: color,
        inset: 0pt,
        align(center + horizon, text(fill: white, weight: "bold", size: 7.5pt, str(num)))
      )
    )
  }
}

// Alternativní oranžová visačka
#let badge-alt(num) = badge(num, color: accent-orange)

// Klávesová zkratka
#let kbd(key) = context {
  if is-html() {
    html.elem("kbd", attrs: (class: "dpi-kbd"))[#key]
  } else {
    box(
      fill: rgb("#f1f5f9"),
      stroke: 0.5pt + rgb("#cbd5e1"),
      radius: 3pt,
      inset: (x: 4pt, y: 2pt),
      baseline: 10%,
      text(size: 8pt, font: ("JetBrainsMono NF", "DejaVu Sans Mono"), weight: "bold", fill: rgb("#334155"), key)
    )
  }
}

// Základní alert box
#let custom-box(title: none, icon: none, stroke-col: gray, fill-col: luma(250), text-col: black, alert-class: "alert-note", body) = context {
  if is-html() {
    html.elem("div", attrs: (class: "dpi-alert " + alert-class))[
      #if title != none [
        #html.elem("div", attrs: (class: "dpi-alert-header"))[
          #if icon != none [ #html.elem("span", attrs: (class: "dpi-alert-icon"))[#icon] ]
          #html.elem("span")[#title]
        ]
      ]
      #html.elem("div", attrs: (class: "dpi-alert-body"))[#body]
    ]
  } else {
    block(
      width: 100%,
      stroke: (left: 3.5pt + stroke-col),
      fill: fill-col,
      radius: (right: 4pt),
      inset: (x: 12pt, y: 10pt),
      spacing: 1.2em,
      [
        #if title != none [
          #grid(
            columns: (auto, 1fr),
            gutter: 6pt,
            align: horizon,
            text(size: 11pt, icon),
            text(weight: "bold", size: 9.5pt, fill: text-col, title)
          )
          #v(2pt)
        ]
        #text(size: 9pt, fill: rgb("#1e293b"), body)
      ]
    )
  }
}

// Poznámka / Info
#let note-box(title: "Poznámka", body) = custom-box(
  title: title,
  icon: "ℹ️",
  stroke-col: rgb("#3b82f6"),
  fill-col: rgb("#eff6ff"),
  text-col: rgb("#1e40af"),
  alert-class: "alert-note",
  body
)

// Tip z praxe
#let tip-box(title: "Tip z laboratorní praxe", body) = custom-box(
  title: title,
  icon: "💡",
  stroke-col: rgb("#10b981"),
  fill-col: rgb("#f0fdf4"),
  text-col: rgb("#065f46"),
  alert-class: "alert-tip",
  body
)

// Důležité upozornění
#let important-box(title: "Důležité upozornění", body) = custom-box(
  title: title,
  icon: "⭐",
  stroke-col: rgb("#f59e0b"),
  fill-col: rgb("#fffbeb"),
  text-col: rgb("#92400e"),
  alert-class: "alert-important",
  body
)

// Varování / Bezpečnost
#let warning-box(title: "Bezpečnostní varování", body) = custom-box(
  title: title,
  icon: "⚠️",
  stroke-col: rgb("#ef4444"),
  fill-col: rgb("#fef2f2"),
  text-col: rgb("#991b1b"),
  alert-class: "alert-warning",
  body
)

// Tabulka pro vysvětlení callout visaček ze screenshotů
#let callout-table(..rows) = {
  table(
    columns: (auto, 1.8fr, 3.2fr),
    align: (center + horizon, left + horizon, left + horizon),
    table.header(
      [*Bod*], [*Ovládací prvek / Oblast*], [*Funkce a laboratorní význam*]
    ),
    ..rows
  )
}
