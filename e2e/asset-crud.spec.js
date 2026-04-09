import { test, expect } from "@playwright/test";

test.describe("Asset CRUD", () => {
  test("shows empty state when no assets exist", async ({ page }) => {
    await page.goto("/");

    const app = page.locator("centurisk-app");
    const nav = app.locator("centurisk-nav");

    // Navigate to Assets
    await nav.locator("button", { hasText: "Assets" }).click();

    // Asset list component renders
    const assetList = app.locator("centurisk-asset-list");
    await expect(assetList).toBeAttached();

    // Shows empty state or table
    const content = assetList.locator("#content");
    await expect(content).toBeAttached();
  });

  test("can create an asset and see it in the list", async ({ page }) => {
    await page.goto("/");

    const app = page.locator("centurisk-app");
    const nav = app.locator("centurisk-nav");

    // Navigate to Assets
    await nav.locator("button", { hasText: "Assets" }).click();

    // Click Add Asset button (could be in toolbar or empty state)
    const addBtn = app.locator("button", { hasText: /Add/ }).first();
    await addBtn.click();

    // Asset form should appear
    const form = app.locator("centurisk-asset-form");
    await expect(form).toBeAttached();

    // Fill in the form
    await form.locator('select#asset-type').selectOption("Building");
    await form.locator('input[name="building_name"]').fill("Fire Station #7");
    await form.locator('input[name="address"]').fill("123 Main St");
    await form.locator('input[name="replacement_cost"]').fill("1500000");

    // Submit
    await form.locator('button[type="submit"]').click();

    // Should navigate back to asset list
    await expect(app.locator(".page-title")).toHaveText("Assets");

    // Asset should appear in the table
    const table = app.locator("centurisk-asset-list table");
    await expect(table).toBeAttached({ timeout: 5000 });

    // Verify the asset data is in the table
    const rows = table.locator("tbody tr");
    await expect(rows).toHaveCount(1);

    // Check the row contains our asset data
    const firstRow = rows.first();
    await expect(firstRow).toContainText("Fire Station #7");
    await expect(firstRow).toContainText("Building");
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
    // Create an asset first
    await request.post("/api/assets", {
      data: {
        asset_type: "Vehicle",
        fields: {
          building_name: "Engine 42",
        },
      },
    });

    const resp = await request.get("/api/assets");
    expect(resp.ok()).toBeTruthy();

    const assets = await resp.json();
    expect(Array.isArray(assets)).toBeTruthy();
    // Should have at least 1 asset (could have more from other tests sharing the DB)
    expect(assets.length).toBeGreaterThanOrEqual(1);
  });
});
