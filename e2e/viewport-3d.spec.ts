import { test, expect, type Page } from "@playwright/test";
import { TAURI_MOCK_SCRIPT } from "./tauri-mock";

// Trasa s jedním objektem — mock update_layout ji vrací pro každé sklo
const PATH_SCRIPT = `
(function () {
  const internals = window.__TAURI_INTERNALS__;
  const orig = internals.invoke;
  internals.invoke = async (cmd, args) => {
    const res = await orig(cmd, args);
    if (cmd !== "update_layout") return res;
    const sq = { points: [{x:5,y:3},{x:70,y:3},{x:70,y:23},{x:5,y:23},{x:5,y:3}] };
    const objects = [{ id: 0, bounds: [5, 70, 3, 23], filled: true, base_bounds: [5, 70, 3, 23] }];
    res.paths = res.positions.map(() => ({ segments: [sq], objects }));
    return res;
  };
})();
`;

type Cam = { target: [number, number, number]; yaw: number; pitch: number; scale: number; projection: string; width: number; height: number };

async function boot(page: Page) {
  await page.addInitScript({ content: TAURI_MOCK_SCRIPT });
  await page.addInitScript({ content: PATH_SCRIPT });
  await page.goto("/");
  await page.waitForLoadState("networkidle");
  const closeBtn = page.locator('button[title="Přeskočit uvítací obrazovku"]');
  if (await closeBtn.isVisible({ timeout: 2000 }).catch(() => false)) await closeBtn.click();
  await page.waitForTimeout(800);
}

const camera = (page: Page) => page.evaluate(() => (window as any).__dpiViewport.camera() as Cam);
const hudBox = async (page: Page) => (await page.getByTestId("viewport-hud").boundingBox())!;

test("výchozí pohled shora mapuje jako dřívější 2D plátno", async ({ page }) => {
  await boot(page);
  const box = await hudBox(page);
  const margin = 40,
    bedX = 250,
    bedY = 210;
  const zoom = Math.min((box.width - margin * 2) / bedX, (box.height - margin * 2) / bedY);
  const panX = (box.width - bedX * zoom) / 2;
  const panY = (box.height + bedY * zoom) / 2;
  for (const [x, y] of [
    [0, 0],
    [26, 32],
    [250, 210],
  ]) {
    const p = await page.evaluate(([x, y]) => (window as any).__dpiViewport.project(x, y, 0), [x, y]);
    expect(Math.abs(p.x - (x * zoom + panX))).toBeLessThan(0.5);
    expect(Math.abs(p.y - (panY - y * zoom))).toBeLessThan(0.5);
  }
  const cam = await camera(page);
  expect(cam.projection).toBe("ortho");
  expect(cam.pitch).toBeCloseTo(Math.PI / 2, 5);
});

test("prostřední tlačítko otáčí plochou a Numpad 7 vrátí pohled shora", async ({ page }) => {
  await boot(page);
  const box = await hudBox(page);
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down({ button: "middle" });
  await page.mouse.move(box.x + box.width / 2 + 120, box.y + box.height / 2 - 80, { steps: 6 });
  await page.mouse.up({ button: "middle" });
  let cam = await camera(page);
  expect(cam.pitch).toBeLessThan(Math.PI / 2 - 0.2);
  expect(Math.abs(cam.yaw)).toBeGreaterThan(0.2);
  expect(cam.projection).toBe("persp"); // automatická perspektiva

  await page.keyboard.press("Numpad7");
  await page.waitForTimeout(450);
  cam = await camera(page);
  expect(cam.pitch).toBeCloseTo(Math.PI / 2, 4);
  expect(cam.yaw).toBeCloseTo(0, 4);
  expect(cam.projection).toBe("ortho");
});

test("klik na substrát nastaví na něj střed otáčení", async ({ page }) => {
  await boot(page);
  // Střed prvního skla (pozice 16, 12; sklo 76 × 26 mm) v pohledu shora
  const p = await page.evaluate(() => (window as any).__dpiViewport.project(16 + 38, 12 + 13, 0));
  const box = await hudBox(page);
  await page.mouse.click(box.x + p.x, box.y + p.y);
  await page.waitForTimeout(800);
  const cam = await camera(page);
  const top = await page.evaluate(() => (window as any).__dpiViewport.glassTop(0));
  expect(cam.target[0]).toBeCloseTo(16 + 38, 0);
  expect(cam.target[1]).toBeCloseTo(12 + 13, 0);
  expect(cam.target[2]).toBeCloseTo(top, 3);
  expect(top).toBeGreaterThan(0); // sklo má tloušťku
});

test("renderer vykreslí desku", async ({ page }) => {
  await boot(page);
  const vp = page.getByTestId("viewport");
  await expect(vp).toHaveAttribute("data-host", "wasm", { timeout: 10_000 });
  await expect(vp).toHaveAttribute("data-ready", "1");
  // Deska (#1e293b) se liší od pozadí (#0f172a) — v obrázku jí musí být hodně
  const png = await page.getByTestId("viewport").screenshot();
  const count = await page.evaluate(async (b64) => {
    const img = new Image();
    img.src = "data:image/png;base64," + b64;
    await img.decode();
    const c = document.createElement("canvas");
    c.width = img.width;
    c.height = img.height;
    const ctx = c.getContext("2d")!;
    ctx.drawImage(img, 0, 0);
    const d = ctx.getImageData(0, 0, c.width, c.height).data;
    let bed = 0;
    for (let i = 0; i < d.length; i += 4) if (Math.abs(d[i] - 0x1e) < 6 && Math.abs(d[i + 1] - 0x29) < 6 && Math.abs(d[i + 2] - 0x3b) < 6) bed++;
    return bed / (c.width * c.height);
  }, png.toString("base64"));
  expect(count).toBeGreaterThan(0.2);
});

test("trasu lze posunout i v natočeném pohledu", async ({ page }) => {
  await boot(page);
  const box = await hudBox(page);
  // Výběr skla 1 a vyčkat na vycentrování
  const c0 = await page.evaluate(() => (window as any).__dpiViewport.project(16 + 38, 12 + 13, 0));
  await page.mouse.click(box.x + c0.x, box.y + c0.y);
  await page.waitForTimeout(700);
  // Natočit (prostřední tlačítko nahoru a doprava)
  await page.mouse.move(box.x + 100, box.y + box.height / 2);
  await page.mouse.down({ button: "middle" });
  await page.mouse.move(box.x + 180, box.y + box.height / 2 - 110, { steps: 6 });
  await page.mouse.up({ button: "middle" });
  const cam = await camera(page);
  expect(cam.pitch).toBeLessThan(1.2);

  const before = (await page.evaluate(() => (window as any).__dpiViewport.transforms()))[0];
  const top = await page.evaluate(() => (window as any).__dpiViewport.glassTop(0));
  const cx = before.gui_dx + before.cx,
    cy = before.gui_dy + before.cy;
  const a = await page.evaluate(([x, y, z]) => (window as any).__dpiViewport.project(x, y, z), [cx, cy, top]);
  const b = await page.evaluate(([x, y, z]) => (window as any).__dpiViewport.project(x, y, z), [cx + 4, cy, top]);
  await page.mouse.move(box.x + a.x, box.y + a.y);
  await page.mouse.down();
  await page.mouse.move(box.x + b.x, box.y + b.y, { steps: 5 });
  await page.mouse.up();
  await page.waitForTimeout(300);
  const after = (await page.evaluate(() => (window as any).__dpiViewport.transforms()))[0];
  // Posun o 4 mm v ose X světa (na rovině vršku skla), Y beze změny
  expect(after.gui_dx - before.gui_dx).toBeCloseTo(4, 0);
  expect(Math.abs(after.gui_dy - before.gui_dy)).toBeLessThan(0.3);
});

test("ikony lišty nad plátnem jsou klikatelné", async ({ page }) => {
  await boot(page);
  const button = (tooltip: string) =>
    page.locator("div.group", { has: page.locator("span", { hasText: tooltip }) }).locator("button");
  await button("3D pohled").click({ timeout: 3000 });
  await page.waitForTimeout(450);
  let cam = await camera(page);
  expect(cam.pitch).toBeLessThan(1.2);
  expect(cam.projection).toBe("persp");
  await button("Pohled shora").click({ timeout: 3000 });
  await page.waitForTimeout(450);
  cam = await camera(page);
  expect(cam.pitch).toBeCloseTo(Math.PI / 2, 4);
  // Mřížka (přepínač) — klik projde na tlačítko, ne do plátna
  await button("Mřížka").click({ timeout: 3000 });
});

test("tloušťka skla odpovídá předvolbě skla, ne výchozím 2 mm", async ({ page }) => {
  await boot(page);
  // Mock: „Laboratorní Sklo (76 x 26 x 1 mm)“ → 1 mm
  const top = await page.evaluate(() => (window as any).__dpiViewport.glassTop(0));
  expect(top).toBeCloseTo(1, 5);
});

test("pohledy po ose X / Y / Z jsou přesné a vždy ortho", async ({ page }) => {
  await boot(page);
  await page.keyboard.press("Numpad2"); // natočit mimo osu → perspektiva
  await page.waitForTimeout(300);
  expect((await camera(page)).projection).toBe("persp");

  await page.keyboard.press("Numpad1"); // zepředu (po ose Y)
  await page.waitForTimeout(450);
  let cam = await camera(page);
  expect(cam.pitch).toBeCloseTo(0, 6);
  expect(cam.yaw).toBeCloseTo(0, 6);
  expect(cam.projection).toBe("ortho");

  await page.keyboard.press("Numpad5"); // na ose perspektivu nepřepne
  await page.waitForTimeout(100);
  expect((await camera(page)).projection).toBe("ortho");

  await page.keyboard.press("Numpad3"); // zprava (po ose X)
  await page.waitForTimeout(450);
  cam = await camera(page);
  expect(cam.yaw).toBeCloseTo(Math.PI / 2, 6);
  expect(cam.projection).toBe("ortho");
});

test("navigační gizmo je vpravo nahoře a klik na osu Z dá pohled shora", async ({ page }) => {
  await boot(page);
  await page.keyboard.press("Numpad1");
  await page.waitForTimeout(450);
  const box = await hudBox(page);
  // Gizmo: střed 52 px od pravého okraje, 54 px od horního; z pohledu zepředu míří osa Z nahoru
  await page.mouse.click(box.x + box.width - 52, box.y + 54 - 30);
  await page.waitForTimeout(450);
  const cam = await camera(page);
  expect(cam.pitch).toBeCloseTo(Math.PI / 2, 4);
  expect(cam.projection).toBe("ortho");
});

test("kolečko nad plátnem zoomuje i s fokusem v levém panelu, nad panelem ne", async ({ page }) => {
  await boot(page);
  const box = await hudBox(page);
  await page.locator("input[type=number]").first().click();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  const s0 = (await camera(page)).scale;
  await page.mouse.wheel(0, -100);
  await page.waitForTimeout(300);
  const s1 = (await camera(page)).scale;
  expect(s1).toBeGreaterThan(s0);
  // Kolečko mimo plátno (nad levým panelem) kamerou nehýbe
  await page.mouse.move(box.x - 100, box.y + box.height / 2);
  await page.mouse.wheel(0, -100);
  await page.waitForTimeout(300);
  expect((await camera(page)).scale).toBeCloseTo(s1, 6);
});

test("panel časové osy se vysune u spodního okraje a při přehrávání zůstane", async ({ page }) => {
  await boot(page);
  const box = await hudBox(page);
  const panel = page.locator('input[type="range"][max="100"]');
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.waitForTimeout(300);
  await expect(panel).not.toBeInViewport();
  // I pod plátnem (mimo něj) se panel vysune
  await page.mouse.move(box.x + box.width / 2, box.y + box.height + 4);
  await page.waitForTimeout(300);
  await expect(panel).toBeInViewport();
  // Krok zpět → časová osa < 100 % → panel zůstane i po odjetí myši
  await page.locator('button[title="Krok zpět (←)"]').click();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.waitForTimeout(300);
  await expect(panel).toBeInViewport();
  await page.locator('button[title="Zobrazit celý tisk"]').click();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.waitForTimeout(300);
  await expect(panel).not.toBeInViewport();
});
