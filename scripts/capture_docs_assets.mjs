import { chromium } from "playwright";

async function main() {
  console.log("Starting screenshot capture...");
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({
    viewport: { width: 1500, height: 950 },
    deviceScaleFactor: 2,
  });
  const page = await context.newPage();

  async function freshLoad() {
    await page.goto("http://127.0.0.1:5173", { waitUntil: "networkidle" });
    await page.waitForTimeout(600);
  }

  // 1. Hlavní okno a jednotlivé panely
  await freshLoad();
  console.log("Capturing 01_main_ui.png...");
  await page.screenshot({ path: "docs/assets/raw/01_main_ui.png" });

  const cols = page.locator(".grid-cols-12 > div");
  if (await cols.count() >= 3) {
    console.log("Capturing panels...");
    await cols.first().screenshot({ path: "docs/assets/raw/02_left_panel.png" });
    await cols.nth(1).screenshot({ path: "docs/assets/raw/03_canvas.png" });
    await cols.nth(2).screenshot({ path: "docs/assets/raw/04_right_panel.png" });
  }

  // 2. Bracket Export Modal
  await freshLoad();
  console.log("Capturing 05_bracket_export.png...");
  try {
    const toolsBtn = page.locator("header button:has-text('Nástroje')");
    await toolsBtn.hover();
    await page.waitForTimeout(300);
    await page.locator("button:has-text('Export držáku')").click();
    await page.waitForTimeout(600);
    await page.screenshot({ path: "docs/assets/raw/05_bracket_export.png" });
  } catch (e) {
    console.warn("Bracket modal warning:", e.message);
  }

  // 3. Settings - Limity
  await freshLoad();
  console.log("Capturing 06_settings_limits.png...");
  try {
    await page.locator("header button:has-text('Nastavení')").click();
    await page.waitForTimeout(400);
    const limitsTab = page.locator("button:has-text('Limity a tiskárna')");
    if (await limitsTab.count() > 0) {
      await limitsTab.click();
      await page.waitForTimeout(300);
    }
    await page.screenshot({ path: "docs/assets/raw/06_settings_limits.png" });
  } catch (e) {
    console.warn("Settings limits warning:", e.message);
  }

  // 4. Settings - Kapaliny
  await freshLoad();
  console.log("Capturing 07_settings_liquids.png...");
  try {
    await page.locator("header button:has-text('Nastavení')").click();
    await page.waitForTimeout(400);
    const liqTab = page.locator("button:has-text('Kapaliny')");
    if (await liqTab.count() > 0) {
      await liqTab.click();
      await page.waitForTimeout(300);
    }
    await page.screenshot({ path: "docs/assets/raw/07_settings_liquids.png" });
  } catch (e) {
    console.warn("Settings liquids warning:", e.message);
  }

  // 5. Settings - Nivelace
  await freshLoad();
  console.log("Capturing 08_settings_leveling.png...");
  try {
    await page.locator("header button:has-text('Nastavení')").click();
    await page.waitForTimeout(400);
    const levTab = page.locator("button:has-text('Nivelace')");
    if (await levTab.count() > 0) {
      await levTab.click();
      await page.waitForTimeout(300);
    }
    await page.screenshot({ path: "docs/assets/raw/08_settings_leveling.png" });
  } catch (e) {
    console.warn("Settings leveling warning:", e.message);
  }

  // 6. Settings - G-kód
  await freshLoad();
  console.log("Capturing 09_settings_gcode.png...");
  try {
    await page.locator("header button:has-text('Nastavení')").click();
    await page.waitForTimeout(400);
    const gcodeTab = page.locator("button:has-text('G-kód')");
    if (await gcodeTab.count() > 0) {
      await gcodeTab.click();
      await page.waitForTimeout(300);
    }
    await page.screenshot({ path: "docs/assets/raw/09_settings_gcode.png" });
  } catch (e) {
    console.warn("Settings gcode warning:", e.message);
  }

  // 7. Diagnostika modal
  await freshLoad();
  console.log("Capturing 10_diagnostics_modal.png...");
  try {
    const toolsBtn = page.locator("header button:has-text('Nástroje')");
    await toolsBtn.hover();
    await page.waitForTimeout(300);
    await page.locator("button:has-text('Diagnostika tiskárny')").click();
    await page.waitForTimeout(600);
    await page.screenshot({ path: "docs/assets/raw/10_diagnostics_modal.png" });
  } catch (e) {
    console.warn("Diagnostics modal warning:", e.message);
  }

  // 8. Shortcuts modal
  await freshLoad();
  console.log("Capturing 11_shortcuts_modal.png...");
  try {
    const helpBtn = page.locator("header button:has-text('Nápověda')");
    await helpBtn.hover();
    await page.waitForTimeout(300);
    await page.locator("button:has-text('Klávesové zkratky')").click();
    await page.waitForTimeout(600);
    await page.screenshot({ path: "docs/assets/raw/11_shortcuts_modal.png" });
  } catch (e) {
    console.warn("Shortcuts modal warning:", e.message);
  }

  await browser.close();
  console.log("All screenshots successfully captured!");
}

main().catch(console.error);
