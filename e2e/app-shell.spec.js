import { test, expect } from "@playwright/test";

/** Helper: login via API and set localStorage/cookie. */
async function loginViaStorage(page, request) {
    // Find the CentuRisk admin
    const users = await (await request.get("/api/users")).json();
    const admin = users.find(u => u.category === "CentuRiskAdmin");

    const { token, user } = await (await request.post("/api/login", {
        data: { user_id: admin.user_id },
    })).json();

    await page.goto("/");
    await page.evaluate(({ token, user }) => {
        localStorage.setItem("centurisk_token", token);
        localStorage.setItem("centurisk_user", JSON.stringify(user));
        document.cookie = "centurisk_session=" + token + ";path=/;SameSite=Strict";
    }, { token, user });
    await page.reload();
}

test.describe("App Shell", () => {
    test("loads and shows CentuRisk nav sidebar", async ({ page, request }) => {
        await loginViaStorage(page, request);

        const app = page.locator("centurisk-app");
        const logoText = app.locator("centurisk-nav").locator(".logo-text");
        await expect(logoText).toHaveText("CentuRisk");
    });

    test("shows logged-in user in header", async ({ page, request }) => {
        await loginViaStorage(page, request);

        const app = page.locator("centurisk-app");
        const userInfo = app.locator(".user-info");
        await expect(userInfo).toContainText("CentuRisk Admin");
    });

    test("navigates between sections via sidebar", async ({ page, request }) => {
        await loginViaStorage(page, request);

        const app = page.locator("centurisk-app");
        const nav = app.locator("centurisk-nav");

        await nav.locator("button", { hasText: "Assets" }).click();
        await expect(app.locator(".page-title")).toHaveText("Assets");

        await nav.locator("button", { hasText: "Dashboard" }).click();
        await expect(app.locator(".page-title")).toHaveText("Dashboard");
    });

    test("logout returns to login page", async ({ page, request }) => {
        await loginViaStorage(page, request);

        const app = page.locator("centurisk-app");
        await app.locator("#logout-btn").click();

        const login = app.locator("centurisk-login");
        await expect(login).toBeAttached({ timeout: 5000 });
    });
});
