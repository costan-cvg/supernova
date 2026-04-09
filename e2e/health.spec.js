import { test, expect } from "@playwright/test";

test.describe("Health Endpoint", () => {
  test("GET /health returns ok with db connected", async ({ request }) => {
    const resp = await request.get("/health");
    expect(resp.ok()).toBeTruthy();

    const body = await resp.json();
    expect(body.status).toBe("ok");
    expect(body.db).toBe("connected");
  });

  test("GET /api/me returns hardcoded admin", async ({ request }) => {
    const resp = await request.get("/api/me");
    expect(resp.ok()).toBeTruthy();

    const body = await resp.json();
    expect(body.display_name).toBe("System Admin");
    expect(body.category).toBe("CentuRiskAdmin");
  });
});
