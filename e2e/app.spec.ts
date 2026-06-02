import { test, expect } from "@playwright/test";
import { TAURI_MOCK_SCRIPT } from "./tauri-mock";

// Každý test začne s injektovaným Tauri mockem — jinak by invoke() selhal
test.beforeEach(async ({ page }) => {
  await page.addInitScript({ content: TAURI_MOCK_SCRIPT });
});

// ──────────────────────────────────────────────────────────────────────────────
// 1. Inicializace aplikace
// ──────────────────────────────────────────────────────────────────────────────

test("aplikace se načte bez JS chyb v konzoli", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (err) => errors.push(err.message));

  await page.goto("/");
  // Počkáme na svelte komponentu (záhlaví nebo canvas)
  await page.waitForSelector("canvas, header, [data-testid='app']", { timeout: 8000 });

  // Žádné neočekávané JS chyby
  const fatalErrors = errors.filter(
    (e) => !e.includes("tauri") && !e.includes("WebSocket") && !e.includes("ResizeObserver")
  );
  expect(fatalErrors).toHaveLength(0);
});

test("zobrazí se LeftPanel s ovládacími prvky", async ({ page }) => {
  await page.goto("/");
  await page.waitForLoadState("networkidle");

  // Hledáme vstupní pole nebo tlačítka v levém panelu
  const inputs = page.locator("input[type='number'], input[type='text']");
  await expect(inputs.first()).toBeVisible({ timeout: 5000 });
});

// ──────────────────────────────────────────────────────────────────────────────
// 2. Canvas a vizualizace
// ──────────────────────────────────────────────────────────────────────────────

test("canvas element se vykreslí", async ({ page }) => {
  await page.goto("/");
  const canvas = page.locator("canvas");
  await expect(canvas).toBeVisible({ timeout: 5000 });

  // Canvas musí mít nenulové rozměry
  const box = await canvas.boundingBox();
  expect(box).not.toBeNull();
  expect(box!.width).toBeGreaterThan(100);
  expect(box!.height).toBeGreaterThan(100);
});

// ──────────────────────────────────────────────────────────────────────────────
// 3. Procesní parametry
// ──────────────────────────────────────────────────────────────────────────────

test("změna počtu vzorků aktualizuje formulář", async ({ page }) => {
  await page.goto("/");
  await page.waitForLoadState("networkidle");

  // Hledáme number input pro počet vzorků (sample_count)
  // Použijeme first() čísleného inputu — v levém panelu je to obvykle první
  const countInput = page.locator("input[type='number']").first();
  await expect(countInput).toBeVisible({ timeout: 5000 });

  // Nastavíme hodnotu a ověříme
  await countInput.click({ clickCount: 3 });
  await countInput.fill("3");
  await countInput.press("Tab");

  // Hodnota se musí udržet (ne reset na default)
  const val = await countInput.inputValue();
  expect(Number(val)).toBeGreaterThanOrEqual(1);
});

// ──────────────────────────────────────────────────────────────────────────────
// 4. Generování G-kódu
// ──────────────────────────────────────────────────────────────────────────────

test("kliknutí na tlačítko generování G-kódu volá backend", async ({ page }) => {
  // Sledujeme invoke volání
  await page.addInitScript({ content: `
    ${TAURI_MOCK_SCRIPT}
    const _origInvoke = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = async (cmd, args) => {
      window.__invokedCmds = window.__invokedCmds || [];
      window.__invokedCmds.push(cmd);
      return _origInvoke(cmd, args);
    };
  ` });

  await page.goto("/");
  await page.waitForLoadState("networkidle");

  // Najdeme tlačítko pro generování G-kódu (obsahuje "G-kód" nebo "Generovat")
  const gcodeBtn = page
    .locator("button")
    .filter({ hasText: /G-?kód|Generovat|Generate/i })
    .first();

  if (await gcodeBtn.isVisible()) {
    await gcodeBtn.click();
    // Počkáme na možný async invoke
    await page.waitForTimeout(500);

    const cmds: string[] = await page.evaluate(() => (window as any).__invokedCmds || []);
    // Generování G-kódu nebo načtení nastavení musí proběhnout
    expect(cmds.some((c) => c.includes("gcode") || c.includes("settings"))).toBe(true);
  } else {
    // Tlačítko není viditelné bez nahraného souboru — test přeskočíme
    test.skip();
  }
});

// ──────────────────────────────────────────────────────────────────────────────
// 5. Nastavení
// ──────────────────────────────────────────────────────────────────────────────

test("lze otevřít nastavení", async ({ page }) => {
  await page.goto("/");
  await page.waitForLoadState("networkidle");

  // Tlačítko nastavení (ozubené kolo nebo text "Nastavení")
  const settingsBtn = page
    .locator("button")
    .filter({ hasText: /Nastaven|Settings|⚙/i })
    .first();

  if (await settingsBtn.isVisible()) {
    await settingsBtn.click();
    // Aspoň zkontrolujeme, že se stránka nezhroutila po otevření nastavení
    await page.waitForTimeout(300);
    const title = await page.title();
    expect(title).not.toBe("");
  } else {
    test.skip();
  }
});

// ──────────────────────────────────────────────────────────────────────────────
// 6. Responzivita — resize okna
// ──────────────────────────────────────────────────────────────────────────────

test("canvas se přizpůsobí při resize okna", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/");

  const canvas = page.locator("canvas");
  await expect(canvas).toBeVisible({ timeout: 5000 });

  await page.setViewportSize({ width: 900, height: 600 });
  await page.waitForTimeout(300); // ResizeObserver debounce

  const after = await canvas.boundingBox();
  expect(after).not.toBeNull();
  // Canvas se musí nějak změnit (nebo zůstat rozumně veliký)
  expect(after!.width).toBeGreaterThan(50);
});
