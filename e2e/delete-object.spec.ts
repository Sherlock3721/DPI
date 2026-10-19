import { test, expect, type Page } from "@playwright/test";
import { TAURI_MOCK_SCRIPT } from "./tauri-mock";

// Trasa s objektem (id 0) a dírou (id 1) — mock update_layout ji vrací pro každé sklo
const PATH_SCRIPT = `
(function () {
  const internals = window.__TAURI_INTERNALS__;
  const orig = internals.invoke;
  window.__layoutCalls = [];
  internals.invoke = async (cmd, args) => {
    if (cmd !== "update_layout") return orig(cmd, args);
    window.__layoutCalls.push(JSON.parse(JSON.stringify(args)));
    const res = await orig(cmd, args);
    const del = (args.overrides && args.overrides["0"] && args.overrides["0"].deleted_objects) || [];
    const sq = (x0, y0, x1, y1) => ({ points: [{x:x0,y:y0},{x:x1,y:y0},{x:x1,y:y1},{x:x0,y:y1},{x:x0,y:y0}] });
    const objects = [
      { id: 0, bounds: [5, 70, 3, 23], filled: true, base_bounds: [5, 70, 3, 23] },
      { id: 1, bounds: [30, 45, 8, 18], filled: true, hole: true, base_bounds: [30, 45, 8, 18] },
    ].filter((o) => !del.includes(o.id));
    res.paths = res.positions.map(() => ({
      segments: objects.map((o) => sq(o.bounds[0], o.bounds[2], o.bounds[1], o.bounds[3])),
      objects,
    }));
    return res;
  };
})();
`;

test.beforeEach(async ({ page }) => {
  await page.addInitScript({ content: TAURI_MOCK_SCRIPT });
  await page.addInitScript({ content: PATH_SCRIPT });
});

async function dismissWelcomeModal(page: Page) {
  const closeBtn = page.locator('button[title="Přeskočit uvítací obrazovku"]');
  if (await closeBtn.isVisible({ timeout: 2000 }).catch(() => false)) {
    await closeBtn.click();
    await closeBtn.waitFor({ state: "hidden", timeout: 2000 }).catch(() => {});
  }
}

/** Bod sklíčka (mm, souřadnice sklíčka) → bod na stránce podle resetCamera. */
async function slidePoint(page: Page, x: number, y: number) {
  const box = (await page.locator("canvas").first().boundingBox())!;
  const margin = 40, bedX = 250, bedY = 210;
  const zoom = Math.min((box.width - margin * 2) / bedX, (box.height - margin * 2) / bedY);
  const panX = (box.width - bedX * zoom) / 2;
  const panY = (box.height + bedY * zoom) / 2;
  const wx = 16 + x, wy = 12 + y;
  return { x: box.x + wx * zoom + panX, y: box.y + panY - wy * zoom };
}

for (const [label, pt, id] of [
  ["objekt", [10, 20], 0],
  ["díra", [37, 13], 1],
] as const) {
  test(`Delete smaže vybraný ${label}`, async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await dismissWelcomeModal(page);
    await page.waitForTimeout(800);

    const p = await slidePoint(page, pt[0], pt[1]);
    await page.mouse.click(p.x, p.y); // výběr skla
    await page.waitForTimeout(200);
    await page.mouse.click(p.x, p.y); // výběr objektu
    await page.waitForTimeout(200);
    await page.keyboard.press("Delete");
    await page.waitForTimeout(800);

    const calls = await page.evaluate(() => (window as any).__layoutCalls);
    const last = calls[calls.length - 1];
    expect(last.overrides?.["0"]?.deleted_objects ?? []).toContain(id);
  });
}

test("kontextové menu odstraní vybranou díru", async ({ page }) => {
  await page.goto("/");
  await page.waitForLoadState("networkidle");
  await dismissWelcomeModal(page);
  await page.waitForTimeout(800);
  const p = await slidePoint(page, 37, 13);
  await page.mouse.click(p.x, p.y);
  await page.waitForTimeout(200);
  await page.mouse.click(p.x, p.y, { button: "right" });
  await page.waitForTimeout(300);
  await page.getByRole("button", { name: /^Odstranit objekt Del/ }).click();
  await page.waitForTimeout(800);
  const calls = await page.evaluate(() => (window as any).__layoutCalls);
  expect(calls[calls.length - 1].overrides?.["0"]?.deleted_objects ?? []).toContain(1);
});
