import { test, expect } from "@playwright/test";

const ADMIN_ID = "00000000-0000-0000-0000-000000000001";

/** Helper: set up a logged-in session via localStorage. */
async function loginViaStorage(page, userId) {
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

test.describe("Asset CRUD", () => {
    test("shows empty state when no assets exist", async ({ page }) => {
        await loginViaStorage(page, ADMIN_ID);

        const app = page.locator("centurisk-app");
        const nav = app.locator("centurisk-nav");

        await nav.locator("button", { hasText: "Assets" }).click();

        const assetList = app.locator("centurisk-asset-list");
        await expect(assetList).toBeAttached();

        const content = assetList.locator("#content");
        await expect(content).toBeAttached();
    });

    test("can create an asset and see it in the list", async ({ page }) => {
        await loginViaStorage(page, ADMIN_ID);

        const app = page.locator("centurisk-app");
        const nav = app.locator("centurisk-nav");

        await nav.locator("button", { hasText: "Assets" }).click();

        const addBtn = app.locator("button", { hasText: /Add/ }).first();
        await addBtn.click();

        const form = app.locator("centurisk-asset-form");
        await expect(form).toBeAttached();

        await form.locator("select#asset-type").selectOption("Building");
        await form.locator('input[name="building_name"]').fill("Fire Station #7");
        await form.locator('input[name="address"]').fill("123 Main St");
        await form.locator('input[name="replacement_cost"]').fill("1500000");

        await form.locator('button[type="submit"]').click();

        await expect(app.locator(".page-title")).toHaveText("Assets");

        const table = app.locator("centurisk-asset-list table");
        await expect(table).toBeAttached({ timeout: 5000 });

        const rows = table.locator("tbody tr");
        const count = await rows.count();
        expect(count).toBeGreaterThanOrEqual(1);

        // At least one row should contain our asset
        await expect(table).toContainText("Fire Station #7");
    });

    test("POST /api/assets creates an asset via API", async ({ request }) => {
        const resp = await request.post("/api/assets", {
            data: {
                asset_type: "Building",
                fields: {
                    building_name: "City Hall",
                    address: "456 Oak Ave",
                    replacement_cost: "5000000",
                },
            },
        });

        expect(resp.status()).toBe(201);

        const body = await resp.json();
        expect(body.asset_type).toBe("Building");
        expect(body.asset_id).toBeTruthy();
        expect(body.fields.building_name).toBe("City Hall");
    });

    test("GET /api/assets returns created assets", async ({ request }) => {
        await request.post("/api/assets", {
            data: {
                asset_type: "Vehicle",
                fields: { building_name: "Engine 42" },
            },
        });

        const resp = await request.get("/api/assets");
        expect(resp.ok()).toBeTruthy();

        const assets = await resp.json();
        expect(Array.isArray(assets)).toBeTruthy();
        expect(assets.length).toBeGreaterThanOrEqual(1);
    });
});
