import { test, expect, type Page } from "@playwright/test";
import { TAURI_MOCK_SCRIPT } from "./tauri-mock";

/**
 * Doplněk mocku: parse_svg_file rozumí obdélníkům, které do SVG zapisuje
 * kreslicí nástroj, a update_layout vrátí geometrii objektů jako trasu.
 * Kreslení je tak plnohodnotnou cestou ke vzniku objektů i bez souborového
 * dialogu, který v holém prohlížeči k dispozici není.
 */
const OBJECTS_PATCH = `
(function () {
  const inner = window.__TAURI_INTERNALS__;
  const origInvoke = inner.invoke;
  inner.invoke = async (cmd, args) => {
    if (cmd === "parse_svg_file") {
      const svg = (args && args.svgText) || "";
      const segments = [];
      const re = /<rect x="([-\\d.]+)" y="([-\\d.]+)" width="([-\\d.]+)" height="([-\\d.]+)"/g;
      let m;
      while ((m = re.exec(svg)) !== null) {
        const x = +m[1], y = +m[2], w = +m[3], h = +m[4];
        segments.push({ points: [
          { x, y }, { x: x + w, y }, { x: x + w, y: y + h }, { x, y: y + h }, { x, y },
        ] });
      }
      return segments.length ? { segments } : { segments: [] };
    }
    const res = await origInvoke(cmd, args);
    if (cmd === "update_layout" && res) {
      // Trasa = surová geometrie objektu; skrytý objekt trasu nemá
      const objects = (args && args.objects) || [];
      res.slides = res.slides.map((row) =>
        row.map((p, k) => ({
          ...p,
          path: p.enabled ? JSON.parse(JSON.stringify(objects[k].raw)) : { segments: [] },
        }))
      );
    }
    return res;
  };
})();
`;

async function boot(page: Page) {
  await page.addInitScript({ content: TAURI_MOCK_SCRIPT });
  await page.addInitScript({ content: OBJECTS_PATCH });
  await page.goto("/");
  await page.waitForSelector("canvas", { timeout: 8000 });
  const closeBtn = page.locator('button[title="Přeskočit uvítací obrazovku"]');
  if (await closeBtn.isVisible({ timeout: 2000 }).catch(() => false)) await closeBtn.click();
  await page.waitForTimeout(500);
}

function toolButton(page: Page, tooltip: string) {
  return page
    .locator("div.group", { has: page.locator("span", { hasText: tooltip }) })
    .locator("button");
}

/** Nakreslí obdélník mezi dvěma body na plátně (v pixelech).
 *  Escape na konci nástroj vypne, aby ho další kreslení zase zapnulo. */
async function drawRect(page: Page, from: { x: number; y: number }, to: { x: number; y: number }) {
  await toolButton(page, "Kreslit obdélník").click();
  const canvas = page.locator("canvas");
  const box = (await canvas.boundingBox())!;
  await page.mouse.move(box.x + from.x, box.y + from.y);
  await page.mouse.down();
  await page.mouse.move(box.x + to.x, box.y + to.y, { steps: 8 });
  await page.mouse.up();
  await page.waitForTimeout(600);
  await page.keyboard.press("Escape");
}

const objectRows = (page: Page) => page.getByLabel(/^(Skrýt|Zobrazit) objekt$/);
/** Položka rozbaleného CustomSelectu (portál do body). */
const dropdownItem = (page: Page, label: string) =>
  page.locator("ul.fixed li button", { hasText: label });

async function openFirstSubstrate(page: Page) {
  await page.getByText("Substrát 1", { exact: true }).click();
  await page.waitForTimeout(300);
}

test("nakreslené objekty se objeví ve výčtu substrátu", async ({ page }) => {
  await boot(page);

  await drawRect(page, { x: 200, y: 200 }, { x: 260, y: 240 });
  await openFirstSubstrate(page);
  await expect(page.getByText(/Objekty na substrátu/)).toBeVisible();
  await expect(objectRows(page)).toHaveCount(1);
  await expect(page.getByText("Obdélník", { exact: true })).toBeVisible();

  // Druhý objekt přibude vedle prvního — jeden import = jeden objekt
  await drawRect(page, { x: 300, y: 200 }, { x: 350, y: 240 });
  await expect(objectRows(page)).toHaveCount(2);
  await expect(page.getByText("Obdélník 2", { exact: true })).toBeVisible();
  await expect(page.getByText(/^2 z 2$/)).toBeVisible();
});

test("oko skryje objekt jen na svém substrátu a jde ho vrátit", async ({ page }) => {
  await boot(page);
  await drawRect(page, { x: 200, y: 200 }, { x: 260, y: 240 });
  await drawRect(page, { x: 300, y: 200 }, { x: 350, y: 240 });
  await openFirstSubstrate(page);
  await expect(page.getByText(/^2 z 2$/)).toBeVisible();

  // Skrytí je vratné — stav nese přepínač, ne smazání
  await objectRows(page).first().click();
  await page.waitForTimeout(400);
  await expect(page.getByText(/^1 z 2$/)).toBeVisible();
  await expect(objectRows(page).first()).toHaveAttribute("aria-pressed", "false");

  await objectRows(page).first().click();
  await page.waitForTimeout(400);
  await expect(page.getByText(/^2 z 2$/)).toBeVisible();
  await expect(objectRows(page).first()).toHaveAttribute("aria-pressed", "true");
});

test("objekt s vlastní tryskou dostane ve výčtu barvu", async ({ page }) => {
  await boot(page);
  await drawRect(page, { x: 200, y: 200 }, { x: 260, y: 240 });
  await openFirstSubstrate(page);

  const name = page.getByText("Obdélník", { exact: true });
  const warning = page.getByText(/Výměna trysky přeruší tisk/);
  await expect(name).not.toHaveAttribute("style", /color/);
  await expect(warning).toBeHidden();

  // Výběr trysky ve výčtu objektů (CustomSelect otevře seznam kliknutím)
  await page.getByText("Globální tryska").click();
  await dropdownItem(page, "Červená").click();
  await page.waitForTimeout(400);

  // Barva trysky (#ef4444) se zesvětlí na čitelnou hodnotu proti tmavé podložce
  await expect(name).toHaveAttribute("style", /color: rgb\(\d+, \d+, \d+\)/);
  const luma = await name.evaluate((el) => {
    const [r, g, b] = getComputedStyle(el).color.match(/\d+/g)!.map(Number);
    const f = (c: number) => {
      const s = c / 255;
      return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
    };
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
  });
  expect(luma).toBeGreaterThanOrEqual(0.3);
  // Jiná tryska znamená přerušení tisku — výčet na to upozorní
  await expect(warning).toBeVisible();
});
