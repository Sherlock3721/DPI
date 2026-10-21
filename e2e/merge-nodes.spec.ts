import { test, expect, type Page } from "@playwright/test";
import { TAURI_MOCK_SCRIPT } from "./tauri-mock";

/** Doplněk mocku: update_layout vrátí reálnou trasu, kde dvě dvojice/trojice
 *  bodů leží těsně u sebe — právě na nich se testuje merge by distance.
 *  Scéna má jeden objekt, takže každé sklíčko dostane jedno umístění. */
const PATHS_PATCH = `
(function () {
  const inner = window.__TAURI_INTERNALS__;
  const origInvoke = inner.invoke;
  inner.invoke = async (cmd, args) => {
    const res = await origInvoke(cmd, args);
    if (cmd === "update_layout" && res) {
      const seg = { points: [
        { x: 5, y: 5 }, { x: 35, y: 5 }, { x: 35.08, y: 5.04 }, { x: 35.05, y: 5.12 },
        { x: 35, y: 20 }, { x: 20, y: 20.05 }, { x: 20.06, y: 20 }, { x: 5, y: 20 },
        { x: 5, y: 5 },
      ] };
      res.slides = res.positions.map((pos) => [{
        object_id: "obj-test",
        enabled: true,
        transform: { scale: 1, rotation: 0, gui_dx: pos.x, gui_dy: pos.y,
                     cx: pos.width / 2, cy: pos.height / 2 },
        baked_scale: 1,
        nozzle_type: null,
        nozzle_diam: null,
        path: { segments: [JSON.parse(JSON.stringify(seg))] },
      }]);
    }
    return res;
  };
})();
`;

async function boot(page: Page) {
  await page.addInitScript({ content: TAURI_MOCK_SCRIPT });
  await page.addInitScript({ content: PATHS_PATCH });
  await page.goto("/");
  await page.waitForSelector("[data-testid=viewport-hud]", { timeout: 8000 });
  const closeBtn = page.locator('button[title="Přeskočit uvítací obrazovku"]');
  if (await closeBtn.isVisible({ timeout: 2000 }).catch(() => false)) await closeBtn.click();
  await page.waitForTimeout(500);
}

function mergeButton(page: Page) {
  return page
    .locator("div.group", {
      has: page.locator("span", { hasText: "Sloučit uzly (merge by distance)" }),
    })
    .locator("button");
}

const popupOf = (page: Page) => page.locator('[aria-label="Sloučení uzlů podle vzdálenosti"]');
const summaryOf = (page: Page) => popupOf(page).getByText(/skupin · ubyde/);
const groupsOf = (page: Page) => popupOf(page).locator('[aria-label^="Skupina "]');
/** Výčet uzlů existuje jen v manuálním režimu — automatický ho nepotřebuje. */
const manualMode = (page: Page) => popupOf(page).getByRole("button", { name: "Manuálně" }).click();
/** Počet uzlů trasy z hlavičky popupu — měřítko toho, zda merge proběhl. */
async function nodeCount(page: Page): Promise<number> {
  const txt = await popupOf(page).getByText(/· \d+ uzlů/).innerText();
  return Number(txt.match(/(\d+) uzlů/)![1]);
}

test("merge popup najde skupiny blízkých uzlů", async ({ page }) => {
  await boot(page);
  await expect(mergeButton(page)).toBeEnabled({ timeout: 5000 });
  await mergeButton(page).click();

  const popup = popupOf(page);
  await expect(popup).toBeVisible();
  await manualMode(page);
  // Trojice u pravého dolního rohu + dvojice uprostřed horní hrany
  await expect(groupsOf(page)).toHaveCount(2);
  await expect(summaryOf(page)).toHaveText("2 skupin · ubyde 3 uzlů");
});

test("mez vzdálenosti mění počet skupin", async ({ page }) => {
  await boot(page);
  await mergeButton(page).click();
  await manualMode(page);
  const popup = popupOf(page);
  const dist = popup.locator('input[type="number"]');

  await dist.fill("0.01");
  await expect(groupsOf(page)).toHaveCount(0);
  await expect(popup.getByText(/není žádná dvojice uzlů/)).toBeVisible();

  await dist.fill("0.1");
  await expect(groupsOf(page)).toHaveCount(2);
});

test("kotva a vyjmutí uzlu ovlivní jen svou skupinu", async ({ page }) => {
  await boot(page);
  await mergeButton(page).click();
  await manualMode(page);
  const popup = popupOf(page);
  const rows = groupsOf(page);

  // Klik na číslo přepne do manuálního režimu a určí cílovou pozici
  await rows.first().locator("button", { hasText: "1." }).click();
  await expect(rows.first().getByText("na uzlu 1")).toBeVisible();
  // Druhá skupina zůstává nedotčená
  await expect(rows.nth(1).getByText("těžiště (bez kotvy)")).toBeVisible();

  // Druhá kotva ve stejné skupině → střed mezi nimi
  await rows.first().locator("button", { hasText: "3." }).click();
  await expect(rows.first().getByText("střed 2 kotev")).toBeVisible();

  // Opětovný klik kotvu ruší
  await rows.first().locator("button", { hasText: "3." }).click();
  await expect(rows.first().getByText("na uzlu 1")).toBeVisible();

  // ✕ vyjme uzel z merge — o jeden sloučený uzel méně
  await rows.first().locator("button").nth(3).click();
  await expect(summaryOf(page)).toHaveText("2 skupin · ubyde 2 uzlů");
  // …a druhý klik ho vrátí zpět
  await rows.first().locator("button").nth(3).click();
  await expect(summaryOf(page)).toHaveText("2 skupin · ubyde 3 uzlů");
});

test("sloučení zkrátí trasu a zavře popup", async ({ page }) => {
  await boot(page);
  await mergeButton(page).click();
  const popup = popupOf(page);

  await popup.getByRole("button", { name: "Sloučit" }).click();
  await expect(popup).toBeHidden();

  // Po merge už není co slučovat
  await mergeButton(page).click();
  await manualMode(page);
  await expect(groupsOf(page)).toHaveCount(0);
  await expect(popupOf(page).getByRole("button", { name: "Sloučit" })).toBeDisabled();
});

test("Escape zavře merge popup", async ({ page }) => {
  await boot(page);
  await mergeButton(page).click();
  await expect(popupOf(page)).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(popupOf(page)).toBeHidden();
});

test("merge se promítne i na ostatní sklíčka se shodnou trasou", async ({ page }) => {
  await boot(page);

  // Tři vzorky → trasy jsou shodné, popup nabídne hromadné použití
  const countInput = page.locator("input[type='number']").first();
  await countInput.fill("3");
  await countInput.press("Tab");
  await page.waitForTimeout(700);

  await mergeButton(page).click();
  const popup = popupOf(page);
  await expect(popup.getByText("Použít i na ostatní sklíčka se shodnou trasou (2)")).toBeVisible();
  await expect(popup.locator("input[type='checkbox']")).toBeChecked();
  await popup.getByRole("button", { name: "Sloučit" }).click();
  await expect(popup).toBeHidden();

  // Přepnutí na druhý substrát — jeho trasa je sloučená také
  const canvas = page.getByTestId("viewport-hud");
  const box = (await canvas.boundingBox())!;
  const zoom = Math.min((box.width - 80) / 250, (box.height - 80) / 210);
  const panX = (box.width - 250 * zoom) / 2;
  const panY = (box.height + 210 * zoom) / 2;
  const slide2 = { x: 18 + 81 + 38, y: 11 + 13 }; // střed druhého sklíčka (76×26 mm, rozestup 5 mm)
  await canvas.click({
    position: { x: slide2.x * zoom + panX, y: panY - slide2.y * zoom },
  });
  await page.waitForTimeout(300);

  await mergeButton(page).click();
  await expect(popupOf(page).getByText("Substrát 2")).toBeVisible();
  await manualMode(page);
  await expect(groupsOf(page)).toHaveCount(0);
});

/** Azurové pixely na plátně — #22d3ee kreslí jen náhled výsledné trasy
 *  a nově vznikající uzly, takže jejich počet měří, zda se náhled vykresluje. */
async function cyanPixels(page: Page) {
  return await page.evaluate(() => {
    // Náhled sloučení kreslí HUD (2D canvas nad scénou)
    const c = document.querySelector("[data-testid=viewport-hud]") as HTMLCanvasElement;
    const d = c.getContext("2d")!.getImageData(0, 0, c.width, c.height).data;
    let cyan = 0;
    for (let i = 0; i < d.length; i += 4) {
      if (d[i] < 80 && d[i + 1] > 180 && d[i + 2] > 190) cyan++;
    }
    return cyan;
  });
}

test("v merge režimu se na plátno kreslí výsledná trasa", async ({ page }) => {
  await boot(page);
  expect(await cyanPixels(page)).toBe(0);

  // Automatický režim kreslí výslednou trasu i bez výčtu uzlů
  await mergeButton(page).click();
  const popup = popupOf(page);
  await expect(summaryOf(page)).toHaveText("2 skupin · ubyde 3 uzlů");
  await page.waitForTimeout(250);
  expect(await cyanPixels(page)).toBeGreaterThan(0);

  // Náhled se překresluje průběžně: bez slučovaných uzlů zmizí…
  await manualMode(page);
  await expect(groupsOf(page)).toHaveCount(2);
  const excludes = popup.locator('button[title="Vyjmout uzel z merge"]');
  for (let i = (await excludes.count()) - 1; i >= 0; i--) await excludes.nth(i).click();
  await expect(popup.getByRole("button", { name: "Sloučit" })).toBeDisabled();
  await page.waitForTimeout(250);
  expect(await cyanPixels(page)).toBe(0);

  // …a po obnovení výběru se vrátí
  await popup.getByRole("button", { name: "Obnovit" }).click();
  await page.waitForTimeout(250);
  expect(await cyanPixels(page)).toBeGreaterThan(0);

  // Po zavření je plátno zpět bez náhledu
  await page.keyboard.press("Escape");
  await page.waitForTimeout(300);
  expect(await cyanPixels(page)).toBe(0);
});

test("automatický režim výčet uzlů neukazuje", async ({ page }) => {
  await boot(page);
  await mergeButton(page).click();
  const popup = popupOf(page);

  // Výchozí režim je „Uprostřed“ — vybírat z uzlů není co
  await expect(groupsOf(page)).toHaveCount(0);
  await expect(popup.getByText("2 skupin ke sloučení", { exact: false })).toBeVisible();
  await expect(summaryOf(page)).toHaveText("2 skupin · ubyde 3 uzlů");

  // Manuální režim výčet odkryje a vyjmutí uzlu se projeví v souhrnu
  await manualMode(page);
  await expect(groupsOf(page)).toHaveCount(2);
  await popup.locator('button[title="Vyjmout uzel z merge"]').first().click();
  await expect(summaryOf(page)).toHaveText("2 skupin · ubyde 2 uzlů");

  // Návrat do automatického režimu výčet skryje a skryté vyjmutí neuplatní
  await popup.getByRole("button", { name: "Uprostřed" }).click();
  await expect(groupsOf(page)).toHaveCount(0);
  await expect(summaryOf(page)).toHaveText("2 skupin · ubyde 3 uzlů");
});

test("Ctrl+Z vrátí sloučení uzlů a Ctrl+Shift+Z ho zopakuje", async ({ page }) => {
  await boot(page);
  await mergeButton(page).click();
  const before = await nodeCount(page);
  await popupOf(page).getByRole("button", { name: "Sloučit" }).click();
  await expect(popupOf(page)).toBeHidden();

  await mergeButton(page).click();
  const merged = await nodeCount(page);
  expect(merged).toBe(before - 3);
  await page.keyboard.press("Escape");

  await page.keyboard.press("Control+z");
  await page.waitForTimeout(300);
  await mergeButton(page).click();
  expect(await nodeCount(page)).toBe(before);
  await page.keyboard.press("Escape");

  await page.keyboard.press("Control+Shift+z");
  await page.waitForTimeout(300);
  await mergeButton(page).click();
  expect(await nodeCount(page)).toBe(merged);
});
