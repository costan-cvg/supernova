import { test, expect } from "@playwright/test";

// Deterministic user IDs from seed data
const USERS = {
    centuriskAdmin: "00000000-0000-0000-0000-000000000001",
    poolAdminA:     "00000000-0000-0000-0000-000000000002",
    memberA:        "00000000-0000-0000-0000-000000000003",
    poolAdminB:     "00000000-0000-0000-0000-000000000004",
    memberB:        "00000000-0000-0000-0000-000000000005",
};

/** Helper: login as a specific user and return the JWT token. */
async function loginAs(request, userId) {
    const resp = await request.post("/api/login", {
        data: { user_id: userId },
    });
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.token).toBeTruthy();
    return body;
}

test.describe("Authentication", () => {
    test("GET /api/users lists all seeded users", async ({ request }) => {
        const resp = await request.get("/api/users");
        expect(resp.ok()).toBeTruthy();
        const users = await resp.json();
        expect(users.length).toBeGreaterThanOrEqual(5);

        const categories = users.map(u => u.category);
        expect(categories).toContain("CentuRiskAdmin");
        expect(categories).toContain("PoolAdministrator");
        expect(categories).toContain("MemberUser");
    });

    test("POST /api/login returns JWT for valid user", async ({ request }) => {
        const { token, user } = await loginAs(request, USERS.centuriskAdmin);
        expect(token.split(".").length).toBe(3); // JWT has 3 parts
        expect(user.display_name).toBe("Alice Admin");
        expect(user.category).toBe("CentuRiskAdmin");
    });

    test("POST /api/login returns 404 for unknown user", async ({ request }) => {
        const resp = await request.post("/api/login", {
            data: { user_id: "nonexistent" },
        });
        expect(resp.status()).toBe(404);
    });

    test("GET /api/me with JWT returns authenticated user", async ({ request }) => {
        const { token } = await loginAs(request, USERS.memberA);

        const resp = await request.get("/api/me", {
            headers: { Authorization: "Bearer " + token },
        });
        expect(resp.ok()).toBeTruthy();

        const me = await resp.json();
        expect(me.category).toBe("MemberUser");
        expect(me.pool_id).toBeTruthy();
        expect(me.member_id).toBeTruthy();
    });
});

test.describe("Cross-Tenant Isolation (PERMANENT CI FIXTURE)", () => {
    test("Pool A admin cannot see Pool B assets via API", async ({ request }) => {
        const { token: tokenA } = await loginAs(request, USERS.poolAdminA);
        const { token: tokenB } = await loginAs(request, USERS.poolAdminB);

        // Create an asset as Pool B admin
        const createResp = await request.post("/api/assets", {
            headers: { Authorization: "Bearer " + tokenB },
            data: {
                asset_type: "Building",
                fields: { building_name: "Pool B Secret Building" },
            },
        });
        expect(createResp.status()).toBe(201);

        // Pool A admin should NOT see Pool B's asset
        const listResp = await request.get("/api/assets", {
            headers: { Authorization: "Bearer " + tokenA },
        });
        expect(listResp.ok()).toBeTruthy();
        const assetsA = await listResp.json();

        const leaked = assetsA.find(a => a.fields?.building_name === "Pool B Secret Building");
        expect(leaked).toBeUndefined();
    });

    test("Member A cannot see Member B assets (different pool)", async ({ request }) => {
        const { token: tokenA } = await loginAs(request, USERS.memberA);
        const { token: tokenB } = await loginAs(request, USERS.memberB);

        // Create an asset as Member B
        await request.post("/api/assets", {
            headers: { Authorization: "Bearer " + tokenB },
            data: {
                asset_type: "Vehicle",
                fields: { building_name: "Member B Fire Truck" },
            },
        });

        // Member A should NOT see it
        const listResp = await request.get("/api/assets", {
            headers: { Authorization: "Bearer " + tokenA },
        });
        const assetsA = await listResp.json();

        const leaked = assetsA.find(a => a.fields?.building_name === "Member B Fire Truck");
        expect(leaked).toBeUndefined();
    });

    test("CentuRisk admin sees assets across the default pool", async ({ request }) => {
        const { token } = await loginAs(request, USERS.centuriskAdmin);

        const resp = await request.get("/api/assets", {
            headers: { Authorization: "Bearer " + token },
        });
        expect(resp.ok()).toBeTruthy();
        // Admin sees pool A assets (their default pool)
    });
});

test.describe("Login UI Flow", () => {
    test("shows login page when no session", async ({ page }) => {
        // Clear any existing session
        await page.goto("/");
        await page.evaluate(() => {
            localStorage.removeItem("centurisk_token");
            localStorage.removeItem("centurisk_user");
            document.cookie = "centurisk_session=;path=/;expires=Thu, 01 Jan 1970 00:00:00 GMT";
        });
        await page.reload();

        // Login component should be visible
        const login = page.locator("centurisk-app").locator("centurisk-login");
        await expect(login).toBeAttached({ timeout: 5000 });

        // User selector should have options
        const select = login.locator("select#user-select");
        await expect(select).toBeAttached();
    });

    test("can log in and see the app with user info", async ({ page }) => {
        await page.goto("/");
        await page.evaluate(() => {
            localStorage.removeItem("centurisk_token");
            localStorage.removeItem("centurisk_user");
            document.cookie = "centurisk_session=;path=/;expires=Thu, 01 Jan 1970 00:00:00 GMT";
        });
        await page.reload();

        const app = page.locator("centurisk-app");
        const login = app.locator("centurisk-login");
        await expect(login).toBeAttached({ timeout: 5000 });

        // Wait for users to load, then select one
        const select = login.locator("select#user-select");
        await page.waitForTimeout(500); // Wait for fetch to complete

        await select.selectOption(USERS.poolAdminA);

        // Click login
        await login.locator("#login-btn").click();

        // After login, should see the app with sidebar
        await expect(app.locator("centurisk-nav")).toBeAttached({ timeout: 5000 });

        // Should show user info with role badge
        const userInfo = app.locator(".user-info");
        await expect(userInfo).toContainText("Bob Pool-Admin");
    });
});
