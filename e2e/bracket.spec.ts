import { test, expect, type Page } from "@playwright/test";
import { TAURI_MOCK_SCRIPT } from "./tauri-mock";

// Regrese: s výchozím multi_spacing 5.0 a tloušťkou ramene 5.0 zanikla zóna
// pružin (spacing − tloušťka = 0) a Rust pružiny tiše vypustil — náhled i export
// pak vypadaly "rozbité" (chybějící pružiny, zmenšené díry pro magnet).
// Modal nyní tloušťku ramen clampuje, takže pružiny musí existovat i s defaulty.

test.beforeEach(async ({ page }) => {
  await page.addInitScript({ content: TAURI_MOCK_SCRIPT });
});

async function dismissWelcomeModal(page: Page) {
  const closeBtn = page.locator('button[title="Přeskočit uvítací obrazovku"]');
  if (await closeBtn.isVisible({ timeout: 2000 }).catch(() => false)) {
    await closeBtn.click();
    await closeBtn.waitFor({ state: "hidden", timeout: 2000 }).catch(() => {});
  }
}

test("náhled držáku obsahuje pružiny i s výchozím rozestupem vzorků", async ({ page }) => {
  await page.goto("/");
  await page.waitForLoadState("networkidle");
  await dismissWelcomeModal(page);

  await page.getByRole("button", { name: "Nástroje" }).hover();
  await page.getByRole("button", { name: "Export držáku" }).click();

  const svg = page.locator("svg:has(pattern)").first();
  await expect(svg).toBeVisible({ timeout: 5000 });
  // Debounce náhledu (120 ms) — počkat na finální geometrii
  await page.waitForTimeout(400);

  // Výchozí parametry: 2× X pružina + 1× Y pružina → 3 clipPathy řezů
  const clipPaths = svg.locator("clipPath");
  await expect(clipPaths).toHaveCount(3);

  // Tloušťka ramen byla clampnuta pod rozestup (5.0 − 1.5 = 3.5)
  const thickInputs = page.locator('section:has-text("Pevný L roh") input');
  await expect(thickInputs.first()).toHaveValue("3.5");
});
