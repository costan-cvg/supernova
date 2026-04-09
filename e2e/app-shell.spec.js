import { test, expect } from "@playwright/test";

/** Helper: set up a logged-in session via localStorage. */
async function loginViaStorage(page, userId) {
    // Get a token from the API
    const resp = await page.request.post("/api/login", {
        data: { user_id: userId },
    });
    const { token, user } = await resp.json();

    await page.goto("/");
    await page.evaluate(({ token, user }) => {
        localStorage.setItem("centurisk_token", token);
        localStorage.setItem("centurisk_user", JSON.stringify(user));
        document.cookie = "centurisk_session=" + token + ";path=/;SameSite=Strict";
    }, { token, user });
    await page.reload();
}

const ADMIN_ID = "00000000-0000-0000-0000-000000000001";

test.describe("App Shell", () => {
    test("loads and shows CentuRisk nav sidebar", async ({ page }) => {
        await loginViaStorage(page, ADMIN_ID);

        const app = page.locator("centurisk-app");
        const logoText = app.locator("centurisk-nav").locator(".logo-text");
        await expect(logoText).toHaveText("CentuRisk");
    });

    test("shows logged-in user in header", async ({ page }) => {
        await loginViaStorage(page, ADMIN_ID);

        const app = page.locator("centurisk-app");
        const userInfo = app.locator(".user-info");
        await expect(userInfo).toContainText("Alice Admin");
    });

    test("navigates between sections via sidebar", async ({ page }) => {
        await loginViaStorage(page, ADMIN_ID);

        const app = page.locator("centurisk-app");
        const nav = app.locator("centurisk-nav");

        await nav.locator("button", { hasText: "Assets" }).click();
        const title = app.locator(".page-title");
        await expect(title).toHaveText("Assets");

        await nav.locator("button", { hasText: "Dashboard" }).click();
        await expect(title).toHaveText("Dashboard");
    });

    test("logout returns to login page", async ({ page }) => {
        await loginViaStorage(page, ADMIN_ID);

        const app = page.locator("centurisk-app");
        await app.locator("#logout-btn").click();

        // Should show login page
        const login = app.locator("centurisk-login");
        await expect(login).toBeAttached({ timeout: 5000 });
    });
});
