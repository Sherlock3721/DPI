/**
 * Barvy tras podle trysky. Objekt s vlastní tryskou se v plátně i ve výčtu
 * kreslí barvou té trysky — `nozzle_defs[jméno] = [výška, průměr, skrytá část,
 * barva]`, takže barva je součástí definice trysky a je pro ni stabilní.
 */

/** Minimální relativní jas barvy trasy, aby byla na tmavém pozadí (#1e293b)
 *  čitelná — jeho vlastní jas je ~0.03, takže tohle drží kontrast nad 5:1. */
const MIN_LUMA = 0.3;

/** Záloha pro trysky bez definované barvy — vzájemně odlišné a dost světlé. */
const FALLBACK_PALETTE = [
  "#f472b6",
  "#facc15",
  "#4ade80",
  "#38bdf8",
  "#fb923c",
  "#c084fc",
  "#2dd4bf",
];

function parseHex(hex: string): [number, number, number] | null {
  const m = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return null;
  let h = m[1];
  if (h.length === 3) h = h[0] + h[0] + h[1] + h[1] + h[2] + h[2];
  return [
    parseInt(h.slice(0, 2), 16),
    parseInt(h.slice(2, 4), 16),
    parseInt(h.slice(4, 6), 16),
  ];
}

function toHex(rgb: [number, number, number]): string {
  return (
    "#" +
    rgb
      .map((v) => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, "0"))
      .join("")
  );
}

/** Relativní jas (WCAG) — 0 = černá, 1 = bílá. */
function luminance([r, g, b]: [number, number, number]): number {
  const f = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  };
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
}

/**
 * Zesvětlí barvu, dokud nemá dost kontrastu proti pozadí plátna. Tmavé barvy
 * trysek (např. tmavě modrá) by na podložce splývaly s mřížkou.
 */
export function ensureContrast(hex: string): string {
  const rgb = parseHex(hex);
  if (!rgb) return hex;
  let [r, g, b] = rgb;
  let guard = 0;
  while (luminance([r, g, b]) < MIN_LUMA && guard < 24) {
    r += (255 - r) * 0.15;
    g += (255 - g) * 0.15;
    b += (255 - b) * 0.15;
    guard += 1;
  }
  return toHex([r, g, b]);
}

/** Stabilní index do záložní palety podle jména trysky. */
function fallbackFor(name: string): string {
  let h = 0;
  for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0;
  return FALLBACK_PALETTE[h % FALLBACK_PALETTE.length];
}

/**
 * Barva trasy pro objekt s přenastavenou tryskou. `null` = objekt používá
 * globální trysku a kreslí se standardní barvou sklíčka.
 */
export function nozzleTraceColor(
  nozzleType: string | null | undefined,
  nozzleDefs: Record<string, [number, number, number, string]> | undefined
): string | null {
  if (!nozzleType) return null;
  const def = nozzleDefs?.[nozzleType];
  const raw = def && typeof def[3] === "string" && def[3] ? def[3] : fallbackFor(nozzleType);
  return ensureContrast(raw);
}
