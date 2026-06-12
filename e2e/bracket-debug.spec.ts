import { test, expect, type Page } from "@playwright/test";
import { TAURI_MOCK_SCRIPT } from "./tauri-mock";

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

test("bracket export preview renders in Chromium", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (err) => errors.push(err.message));
  page.on("console", (msg) => {
    if (msg.type() === "error") errors.push(msg.text());
  });

  await page.goto("/");
  await page.waitForLoadState("networkidle");
  await dismissWelcomeModal(page);

  // Otevřít Nástroje → Export držáku (menu se otevírá přes group-hover)
  await page.getByRole("button", { name: "Nástroje" }).hover();
  await page.getByRole("button", { name: "Export držáku" }).click();

  // Modal s náhledem
  const svg = page.locator(".bg-slate-950 svg[viewBox]").first();
  await expect(svg).toBeVisible({ timeout: 5000 });
  await page.waitForTimeout(500);

  console.log("JS errors:", JSON.stringify(errors, null, 2));

  // Screenshot náhledu i celé stránky
  await svg.screenshot({ path: "/tmp/bracket-preview-chromium.png" });
  await page.screenshot({ path: "/tmp/bracket-page-chromium.png" });

  const diag = await svg.evaluate((el) => ({
    viewBox: el.getAttribute("viewBox"),
    childTags: Array.from(el.children).map((c) => c.tagName),
    html: el.outerHTML,
  }));
  console.log("viewBox:", diag.viewBox);
  console.log("childTags:", JSON.stringify(diag.childTags));
  console.log("SVG_HTML_START");
  console.log(diag.html);
  console.log("SVG_HTML_END");
});
