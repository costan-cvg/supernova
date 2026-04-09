import { test, expect } from "@playwright/test";

/** Helper: find a user by category (and optionally display_name substring). */
async function findUser(request, category, nameContains) {
    const resp = await request.get("/api/users");
    const users = await resp.json();
    return users.find(u =>
        u.category === category &&
        (!nameContains || u.display_name.includes(nameContains))
    );
}

/** Helper: login and return { token, user }. */
async function loginAs(request, userId) {
    const resp = await request.post("/api/login", {
        data: { user_id: userId },
    });
    expect(resp.ok()).toBeTruthy();
    return await resp.json();
}

test.describe("Authentication", () => {
    test("GET /api/users lists seeded users across roles", async ({ request }) => {
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
        const admin = await findUser(request, "CentuRiskAdmin");
        const { token, user } = await loginAs(request, admin.user_id);
        expect(token.split(".").length).toBe(3);
        expect(user.category).toBe("CentuRiskAdmin");
    });

    test("POST /api/login returns 404 for unknown user", async ({ request }) => {
        const resp = await request.post("/api/login", {
            data: { user_id: "nonexistent" },
        });
        expect(resp.status()).toBe(404);
    });

    test("GET /api/me with JWT returns authenticated user", async ({ request }) => {
        const member = await findUser(request, "MemberUser", "Springfield");
        const { token } = await loginAs(request, member.user_id);

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
        // Find two pool admins from different pools
        const resp = await request.get("/api/users");
        const users = await resp.json();
        const poolAdmins = users.filter(u => u.category === "PoolAdministrator");
        expect(poolAdmins.length).toBeGreaterThanOrEqual(2);

        const adminA = poolAdmins[0];
        const adminB = poolAdmins[1];
        expect(adminA.pool_id).not.toBe(adminB.pool_id);

        const { token: tokenA } = await loginAs(request, adminA.user_id);
        const { token: tokenB } = await loginAs(request, adminB.user_id);

        // Get Pool B's assets
        const assetsB = await (await request.get("/api/assets", {
            headers: { Authorization: "Bearer " + tokenB },
        })).json();
        expect(assetsB.length).toBeGreaterThan(0);

        // Pool A should NOT see any of Pool B's assets
        const assetsA = await (await request.get("/api/assets", {
            headers: { Authorization: "Bearer " + tokenA },
        })).json();

        const assetIdsB = new Set(assetsB.map(a => a.asset_id));
        const leaked = assetsA.filter(a => assetIdsB.has(a.asset_id));
        expect(leaked).toHaveLength(0);
    });

    test("Member in Pool A cannot see Pool B member assets", async ({ request }) => {
        const resp = await request.get("/api/users");
        const users = await resp.json();
        const members = users.filter(u => u.category === "MemberUser");

        // Find two members from different pools
        const memberA = members[0];
        const memberB = members.find(u => u.pool_id !== memberA.pool_id);
        expect(memberB).toBeTruthy();

        const { token: tokenA } = await loginAs(request, memberA.user_id);
        const { token: tokenB } = await loginAs(request, memberB.user_id);

        const assetsA = await (await request.get("/api/assets", {
            headers: { Authorization: "Bearer " + tokenA },
        })).json();

        const assetsB = await (await request.get("/api/assets", {
            headers: { Authorization: "Bearer " + tokenB },
        })).json();

        // No overlap
        const idsA = new Set(assetsA.map(a => a.asset_id));
        const leaked = assetsB.filter(a => idsA.has(a.asset_id));
        expect(leaked).toHaveLength(0);
    });
});

test.describe("Login UI Flow", () => {
    test("shows login page when no session", async ({ page }) => {
        await page.goto("/");
        await page.evaluate(() => {
            localStorage.removeItem("centurisk_token");
            localStorage.removeItem("centurisk_user");
            document.cookie = "centurisk_session=;path=/;expires=Thu, 01 Jan 1970 00:00:00 GMT";
        });
        await page.reload();

        const login = page.locator("centurisk-app").locator("centurisk-login");
        await expect(login).toBeAttached({ timeout: 5000 });
    });

    test("can log in as pool admin and see assets", async ({ page, request }) => {
        // Find a pool admin
        const poolAdmin = await findUser(request, "PoolAdministrator", "Demo");

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

        // Wait for user list to load then select
        const select = login.locator("select#user-select");
        await page.waitForTimeout(500);
        await select.selectOption(poolAdmin.user_id);
        await login.locator("#login-btn").click();

        // Should see the app with nav
        await expect(app.locator("centurisk-nav")).toBeAttached({ timeout: 5000 });

        // Navigate to assets and see imported data
        await app.locator("centurisk-nav").locator("button", { hasText: "Assets" }).click();
        const table = app.locator("centurisk-asset-list table");
        await expect(table).toBeAttached({ timeout: 5000 });

        // Should see Springfield + Shelbyville assets (Demo Risk Pool)
        const rows = table.locator("tbody tr");
        const count = await rows.count();
        expect(count).toBeGreaterThanOrEqual(10); // 10 Springfield + 4 Shelbyville
    });
});
