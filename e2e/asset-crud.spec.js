import { test, expect } from "@playwright/test";

/** Helper: login as a pool admin (has actual pool data). */
async function loginViaStorage(page, request) {
    const users = await (await request.get("/api/users")).json();
    const poolAdmin = users.find(u => u.category === "PoolAdministrator" && u.display_name.includes("Demo"));
    const { token, user } = await (await request.post("/api/login", {
        data: { user_id: poolAdmin.user_id },
    })).json();

    await page.goto("/");
    await page.evaluate(({ token, user }) => {
        localStorage.setItem("centurisk_token", token);
        localStorage.setItem("centurisk_user", JSON.stringify(user));
        document.cookie = "centurisk_session=" + token + ";path=/;SameSite=Strict";
    }, { token, user });
    await page.reload();
}

test.describe("Asset CRUD", () => {
    test("shows imported assets from sample CSVs", async ({ page, request }) => {
        await loginViaStorage(page, request);

        const app = page.locator("centurisk-app");
        await app.locator("centurisk-nav").locator("button", { hasText: "Assets" }).click();

        const table = app.locator("centurisk-asset-list table");
        await expect(table).toBeAttached({ timeout: 5000 });

        // CentuRisk admin with default pool should see assets
        const rows = table.locator("tbody tr");
        const count = await rows.count();
        expect(count).toBeGreaterThan(0);
    });

    test("can create an asset via the form", async ({ page, request }) => {
        await loginViaStorage(page, request);

        const app = page.locator("centurisk-app");
        await app.locator("centurisk-nav").locator("button", { hasText: "Assets" }).click();

        const addBtn = app.locator("button", { hasText: /Add/ }).first();
        await addBtn.click();

        const form = app.locator("centurisk-asset-form");
        await expect(form).toBeAttached();

        await form.locator("select#asset-type").selectOption("Building");
        await form.locator('input[name="building_name"]').fill("New Test Building");
        await form.locator('input[name="address"]').fill("999 Test Ave");
        await form.locator('input[name="replacement_cost"]').fill("2000000");

        await form.locator('button[type="submit"]').click();

        await expect(app.locator(".page-title")).toHaveText("Assets");
        const table = app.locator("centurisk-asset-list table");
        await expect(table).toContainText("New Test Building");
    });

    test("POST /api/onboard creates a new pool with assets from CSV", async ({ request }) => {
        const resp = await request.post("/api/onboard", {
            data: {
                pool_name: "E2E Test Pool",
                members: [{
                    member_name: "Test Town",
                    sov_csv: "asset_type,building_name,address,replacement_cost\nBuilding,Test Hall,1 Test St,500000\nVehicle,Test Truck,,75000"
                }]
            }
        });

        expect(resp.status()).toBe(201);
        const body = await resp.json();
        expect(body.pool_name).toBe("E2E Test Pool");
        expect(body.total_assets).toBe(2);
        expect(body.members[0].assets_imported).toBe(2);
    });
});
