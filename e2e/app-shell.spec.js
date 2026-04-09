import { test, expect } from "@playwright/test";

test.describe("App Shell", () => {
  test("loads and shows CentuRisk nav sidebar", async ({ page }) => {
    await page.goto("/");

    // App shell renders
    const app = page.locator("centurisk-app");
    await expect(app).toBeAttached();

    // Nav sidebar has CentuRisk branding (inside shadow DOM)
    const logoText = app.locator("centurisk-nav").locator(".logo-text");
    await expect(logoText).toHaveText("CentuRisk");
  });

  test("shows logged-in user in header", async ({ page }) => {
    await page.goto("/");

    const app = page.locator("centurisk-app");
    const userInfo = app.locator(".user-info");
    await expect(userInfo).toContainText("System Admin");
  });

  test("navigates between sections via sidebar", async ({ page }) => {
    await page.goto("/");

    const app = page.locator("centurisk-app");
    const nav = app.locator("centurisk-nav");

    // Click Assets nav item
    await nav.locator("button", { hasText: "Assets" }).click();

    // Page title updates
    const title = app.locator(".page-title");
    await expect(title).toHaveText("Assets");

    // Click Dashboard nav item
    await nav.locator("button", { hasText: "Dashboard" }).click();
    await expect(title).toHaveText("Dashboard");
  });
});
